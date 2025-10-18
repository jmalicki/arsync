# Parallel Large File Copy Design

## Overview

This document describes the design for parallel copying of large files to maximize NVMe bandwidth utilization. The feature splits large files into regions and uses multiple async tasks to read and write different regions concurrently.

**Implementation Location:** All code lives in `src/copy.rs` alongside existing sequential copy logic.

## Motivation

Modern NVMe drives can sustain multiple GB/s of throughput, but sequential single-threaded copying often cannot saturate this bandwidth. By partitioning large files and using parallel I/O operations, we can:

1. **Maximize NVMe queue depth utilization** - Submit multiple I/O operations simultaneously
2. **Reduce latency impact** - While one operation waits, others can proceed
3. **Improve throughput** - Achieve closer to theoretical maximum bandwidth
4. **Leverage io_uring** - Take full advantage of io_uring's async capabilities

## Design Principles

1. **Safe by default** - Feature is disabled by default to avoid surprising users
2. **Correctness first** - Never sacrifice data integrity for performance
3. **Minimal changes** - Add one function to existing code, reuse existing stats/logging
4. **Memory efficient** - Limit buffer usage to avoid excessive memory consumption
5. **Adaptive** - Only activate for files that benefit from parallelization

## Architecture

### Iterative Spawn Strategy

Calculate all regions upfront and spawn parallel tasks:

```
File: [==============================================] (1 GB)
max_depth = 2 → 2^2 = 4 tasks

Task 0: [0 to 256MB]
Task 1: [256MB to 512MB]      ← Page-aligned split
Task 2: [512MB to 768MB]      ← Page-aligned split  
Task 3: [768MB to 1GB]
```

**Key insights:**
- `read_at` and `write_at` are position-independent, so we can share file handles
- Calculate regions iteratively, spawn all tasks at once
- Use `compio::runtime::spawn` (matches existing pattern in directory.rs)
- No recursion → No `Box::pin` needed
- Number of parallel tasks = 2^max_depth

### Implementation Structure

All code lives in `src/copy.rs` - just three simple functions:

```rust
// Entry point (modified)
pub async fn copy_file(
    src: &Path, 
    dst: &Path, 
    metadata_config: &MetadataConfig,
    parallel_config: &ParallelCopyConfig,
) -> Result<()>

// NEW: Parallel coordinator
async fn copy_read_write_parallel(
    src: &Path,
    dst: &Path,
    metadata_config: &MetadataConfig,
    parallel_config: &ParallelCopyConfig,
    file_size: u64,
) -> Result<()>

// NEW: Recursive worker (does the actual splitting)
async fn copy_region_recursive(
    src: &File,
    dst: &File,
    start: u64,
    end: u64,
    depth: usize,
    max_depth: usize,
    chunk_size: usize,
) -> Result<()>
```

### Core Implementation

```rust
/// 2MB huge page size for alignment
const HUGE_PAGE_SIZE: u64 = 2 * 1024 * 1024;

/// Minimum region size to consider splitting (8MB)
const MIN_SPLIT_SIZE: u64 = 8 * 1024 * 1024;

async fn copy_read_write_parallel(
    src: &Path,
    dst: &Path,
    metadata_config: &MetadataConfig,
    parallel_config: &ParallelCopyConfig,
    file_size: u64,
) -> Result<()> {
    let max_depth = parallel_config.max_depth;
    let max_tasks = 1 << max_depth;  // 2^max_depth
    
    info!(
        "Using parallel copy: depth {} (up to {} tasks) for {} MB",
        max_depth,
        max_tasks,
        file_size / 1024 / 1024
    );

    // 1. Capture timestamps (same as sequential)
    let (src_accessed, src_modified) = get_precise_timestamps(src).await?;

    // 2. Open files (same as sequential)
    let src_file = OpenOptions::new().read(true).open(src).await?;
    let dst_file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(dst)
        .await?;

    // 3. CRITICAL: fallocate the entire file first
    if file_size > 0 {
        use compio_fs_extended::{ExtendedFile, Fallocate};
        let extended_dst = ExtendedFile::from_ref(&dst_file);
        extended_dst.fallocate(0, file_size, 0).await?;
        
        // Apply fadvise hints (Linux only)
        #[cfg(target_os = "linux")]
        {
            use compio_fs_extended::{fadvise::FadviseAdvice, Fadvise};
            ExtendedFile::from_ref(&src_file)
                .fadvise(FadviseAdvice::NoReuse, 0, file_size as i64)
                .await?;
            extended_dst
                .fadvise(FadviseAdvice::NoReuse, 0, file_size as i64)
                .await?;
        }
    }

    // 4. Start recursive binary split
    let chunk_size = parallel_config.chunk_size_bytes();
    copy_region_recursive(&src_file, &dst_file, 0, file_size, 0, max_depth, chunk_size).await?;

    // 5. Sync all data to disk (same as sequential)
    dst_file.sync_all().await?;

    // 6. Preserve metadata (same as sequential)
    preserve_file_metadata(
        &src_file,
        &dst_file,
        dst,
        src_accessed,
        src_modified,
        metadata_config,
    )
    .await?;

    info!("Parallel copy completed: {} bytes", file_size);
    Ok(())
}

/// Recursively split and copy a region of a file
async fn copy_region_recursive(
    src: &File,
    dst: &File,
    start: u64,
    end: u64,
    depth: usize,
    max_depth: usize,
    chunk_size: usize,
) -> Result<()> {
    let size = end - start;
    
    // Base case: region too small, max depth reached, or not worth splitting
    if depth >= max_depth || size < MIN_SPLIT_SIZE {
        return copy_region_sequential(src, dst, start, end, chunk_size).await;
    }
    
    // Recursive case: split in half at page-aligned boundary
    let mid = (start + end) / 2;
    let mid_aligned = align_to_page(mid, HUGE_PAGE_SIZE);
    
    // Make sure alignment didn't push us to the boundaries
    let mid_aligned = mid_aligned.max(start + MIN_SPLIT_SIZE).min(end - MIN_SPLIT_SIZE);
    
    // Spawn two tasks: left and right halves
    let left = copy_region_recursive(src, dst, start, mid_aligned, depth + 1, max_depth, chunk_size);
    let right = copy_region_recursive(src, dst, mid_aligned, end, depth + 1, max_depth, chunk_size);
    
    futures_util::try_join!(left, right)?;
    Ok(())
}

/// Copy a region sequentially (no more splitting)
async fn copy_region_sequential(
    src: &File,
    dst: &File,
    start: u64,
    end: u64,
    chunk_size: usize,
) -> Result<()> {
    let mut buffer = vec![0u8; chunk_size];
    let mut offset = start;
    
    while offset < end {
        let remaining = end - offset;
        let to_read = remaining.min(chunk_size as u64);
        
        // Read from source at this offset
        let (bytes_read, buf) = src.read_at(buffer, offset).await
            .map_err(|e| SyncError::IoUring(format!("read_at failed: {e}")))?;
        buffer = buf;
        
        if bytes_read == 0 {
            break;
        }
        
        // Write to destination at same offset
        buffer.truncate(bytes_read);
        let (bytes_written, buf) = dst.write_at(buffer, offset).await
            .map_err(|e| SyncError::IoUring(format!("write_at failed: {e}")))?;
        buffer = buf;
        buffer.resize(chunk_size, 0);
        
        if bytes_written != bytes_read {
            return Err(SyncError::CopyFailed(format!(
                "Write size mismatch at offset {}: read {}, wrote {}",
                offset, bytes_read, bytes_written
            )));
        }
        
        offset += bytes_written as u64;
    }
    
    Ok(())
}

/// Align offset to page boundary (round down)
fn align_to_page(offset: u64, page_size: u64) -> u64 {
    (offset / page_size) * page_size
}
```

### Key Design Decisions

1. **Iterative spawning** - Calculate all regions upfront, spawn all tasks at once
2. **No recursion** - No `Box::pin` needed, simpler code
3. **compio::runtime::spawn** - Matches existing pattern in directory.rs
4. **Page-aligned splits** - Align to 2MB huge page boundaries
5. **File.clone()** - Each task gets its own mutable file handles
6. **Buffer per task** - Each task owns its buffer (no sharing)

## CLI Parameters

Add new parameters to `IoConfig` in `src/cli.rs`:

```rust
#[derive(clap::Args, Debug, Clone)]
#[command(next_help_heading = "Parallel Copy Options")]
pub struct ParallelCopyConfig {
    /// Enable parallel copying for large files
    /// 
    /// When enabled, files larger than --parallel-min-size will be split
    /// recursively and copied by multiple tasks concurrently.
    #[arg(long)]
    pub enabled: bool,

    /// Minimum file size (in MB) to trigger parallel copying
    /// 
    /// Files smaller than this threshold will be copied sequentially.
    /// Default: 128 MB
    #[arg(long, default_value = "128", requires = "enabled")]
    pub min_file_size_mb: u64,

    /// Maximum recursion depth for parallel splits
    /// 
    /// Creates up to 2^depth parallel tasks.
    /// Depth 2 = 4 tasks, Depth 3 = 8 tasks, Depth 4 = 16 tasks
    /// Recommended: 2-3 for NVMe, 1-2 for SSD
    /// Default: 2 (4 tasks)
    #[arg(long, default_value = "2", requires = "enabled")]
    pub max_depth: usize,

    /// Chunk size (in MB) for each read/write operation
    /// 
    /// Larger chunks reduce syscall overhead but increase memory usage.
    /// Default: 2 MB
    #[arg(long, default_value = "2", requires = "enabled")]
    pub chunk_size_mb: usize,
}
```

## Integration with Existing Code

### Modify `copy_file()` in `src/copy.rs`

```rust
pub async fn copy_file(
    src: &Path,
    dst: &Path,
    metadata_config: &MetadataConfig,
    parallel_config: &ParallelCopyConfig,
) -> Result<()> {
    // Get file size
    let metadata = compio::fs::metadata(src).await?;
    let file_size = metadata.len();
    
    // Decide whether to use parallel copy
    if parallel_config.should_use_parallel(file_size) {
        copy_read_write_parallel(src, dst, metadata_config, parallel_config, file_size).await
    } else {
        copy_read_write(src, dst, metadata_config).await
    }
}
```

## Performance Considerations

### Benefits

1. **Higher throughput** - Can achieve 2-4x speedup for large files on NVMe
2. **Better queue depth** - Keeps io_uring queues saturated
3. **Lower latency sensitivity** - One slow operation doesn't block others
4. **Simple implementation** - Recursive split is naturally parallel

### Trade-offs

1. **Memory usage** - 2^depth × chunk_size (peak)
   - Depth 2 (4 tasks): 4 × 2MB = 8MB
   - Depth 3 (8 tasks): 8 × 2MB = 16MB
   - Depth 4 (16 tasks): 16 × 2MB = 32MB
   - Much more predictable than fixed worker pool

2. **Task overhead** - Spawning many tasks has some cost
   - Mitigated by MIN_SPLIT_SIZE (only split if region is large enough)

3. **Not always faster** - Benefits depend on:
   - Storage type (NVMe >> SSD > HDD)
   - File size (larger files benefit more)
   - System load (may compete with other I/O)

### Benchmarking

Expected performance on modern NVMe:
- **Sequential copy**: ~2-3 GB/s
- **Parallel depth 2 (4 tasks)**: ~5-7 GB/s
- **Parallel depth 3 (8 tasks)**: ~7-9 GB/s
- **Parallel depth 4 (16 tasks)**: ~8-10 GB/s (diminishing returns)

## Safety and Correctness

### Guarantees

1. **No overlapping writes** - Each worker writes to a distinct region
2. **Atomic file creation** - fallocate ensures space is reserved upfront
3. **Verification** - Total bytes copied is verified against file size
4. **Error propagation** - Any worker error cancels entire operation

### Edge Cases

1. **File size not evenly divisible by num_workers**
   - Last worker handles remainder

2. **Very small files**
   - Disabled by min_file_size threshold

3. **Sparse files**
   - fallocate handles properly, parallel copy works

4. **Write failures**
   - All workers abort on first error via try_join_all

## Testing Strategy

### Unit Tests

1. **Page alignment**
   - Test align_to_page() function
   - Verify splits are page-aligned

2. **Region copying**
   - Test copy_region_sequential() with various sizes
   - Test edge cases (empty region, single byte, etc.)

3. **Recursive splitting**
   - Test depth limits are respected
   - Test MIN_SPLIT_SIZE prevents over-splitting
   - Test alignment edge cases

4. **Configuration validation**
   - Invalid max_depth (>10 would create too many tasks)
   - Invalid chunk_size (0, too large)

### Integration Tests

1. **End-to-end parallel copy**
   - Copy various file sizes
   - Verify content integrity (checksums)
   - Verify metadata preservation

2. **Performance tests**
   - Ensure parallel copy is faster for large files
   - Ensure sequential copy not affected
   - Test various depths (1, 2, 3, 4)

3. **Error handling**
   - Disk full during parallel copy
   - Permission errors
   - I/O errors on source/destination
   - Error propagation from recursive tasks

### Benchmark Suite

1. **Throughput comparison**
   - Sequential vs parallel for various sizes
   - Different depths (2, 3, 4)

2. **Memory usage**
   - Monitor RSS during parallel copies
   - Ensure bounded: should be ~2^depth × chunk_size

## Implementation Plan

### Phase 1: CLI Configuration ✅
1. Add `ParallelCopyConfig` to CLI (DONE)
2. Add validation logic (DONE)

### Phase 2: Core Implementation
3. Add `copy_read_write_parallel()` to `src/copy.rs`
4. Modify `copy_file()` to route to parallel or sequential
5. Wire up configuration throughout call chain

### Phase 3: Testing
6. Write unit tests for partitioning logic
7. Write integration tests for parallel copy correctness
8. Test with various file sizes and worker counts

### Phase 4: Documentation & Tuning
9. Add usage examples to README
10. Run benchmarks and validate performance
11. Tune default parameters based on results

## Design Decisions

1. **Why iterative spawn instead of recursion?**
   - Simpler - just calculate regions and spawn
   - No `Box::pin` overhead
   - Matches existing codebase pattern (directory.rs)
   - Easier to understand and debug
   - 2^depth tasks calculated upfront

2. **Interaction with directory-level concurrency**
   - Directory traversal already copies multiple files concurrently
   - Parallel copy adds per-file parallelism
   - Memory usage = `max_files_in_flight × 2^depth × chunk_size`
   - Peak case: 100 files × 4 tasks × 2MB = 800MB
   - Should monitor, but probably fine for NVMe use cases

3. **Error handling**
   - Use `try_join!`: First error cancels both branches
   - Propagates up the recursion tree
   - All tasks abort on first error
   - Decision: Fail fast for correctness and simplicity

4. **File handle strategy**
   - Share `&src_file` and `&dst_file` across tasks
   - Works because `read_at`/`write_at` are position-independent
   - No cloning, no re-opening, minimal overhead

5. **Buffer management**
   - Each leaf task (sequential copy) owns its buffer
   - No sharing, no locking
   - Predictable peak memory: `2^depth × chunk_size`

6. **Page alignment**
   - Split at 2MB boundaries (huge pages)
   - Improves TLB performance
   - Reduces filesystem fragmentation
   - Falls back gracefully if alignment isn't possible

## Future Enhancements

1. **Auto-tuning** - Detect storage type and adjust workers automatically
2. **Progress tracking** - Per-worker progress for large files
3. **Adaptive partitioning** - Adjust regions based on I/O latency
4. **Direct I/O** - Use O_DIRECT for even better performance
5. **Zero-copy** - Investigate io_uring's IORING_OP_SPLICE for parallel operations

## References

- Linux io_uring documentation: https://kernel.dk/io_uring.pdf
- NVMe queue depth optimization: https://www.kernel.org/doc/html/latest/block/blk-mq.html
- Parallel I/O best practices: https://docs.kernel.org/filesystems/io_uring.html

## Example Usage

```bash
# Enable with 4 parallel tasks (depth 2)
arsync --parallel-max-depth 2 /source /dest

# Customize all parameters
arsync --parallel-max-depth 3 \              # 2^3 = 8 parallel tasks
       --parallel-min-size-mb 256 \          # 256 MB minimum file size
       --parallel-chunk-size-mb 4 \          # 4 MB chunks
       /source /dest

# More aggressive (16 tasks for very large files)
arsync --parallel-max-depth 4 /source /dest

# View which files were copied in parallel (with verbose logging)
arsync --parallel-max-depth 2 -vv /source /dest

# Disabled by default (max-depth defaults to 0)
arsync /source /dest  # Sequential copy only
```

## Metrics and Observability

**Metrics:** Use existing `SyncStats` structure. No changes needed - bytes copied are bytes copied regardless of method.

**Logging:**
- `info!` when parallel copy is used (which files, max depth, max tasks)
- `info!` when parallel copy completes

Example log output:
```
INFO  Using parallel copy: depth 2 (up to 4 tasks) for 512 MB
INFO  Parallel copy completed: 536870912 bytes
```

**Debug logging** (optional, if needed):
- Could add per-region completion messages
- Could add split point logging
- Start simple: just entry/exit logging

**Future enhancements:**
- Could add a summary at end showing how many files used parallel copy
- Could add timing breakdown (fallocate time, copy time, sync time)
- But start simple: just basic info/debug logging

