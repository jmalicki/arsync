# Parallel Copy Implementation Summary

**Date:** October 18, 2025  
**Status:** ✅ IMPLEMENTED AND TESTED

## Overview

Implemented recursive binary split parallel file copying for large files to maximize NVMe bandwidth utilization.

## Implementation Details

### Architecture: Recursive Binary Split

Instead of pre-calculating worker regions, we use a **trivially simple recursive approach**:

```rust
fn copy_region_recursive(src, dst, start, end, depth, max_depth) {
    if depth >= max_depth || (end - start) < MIN_SPLIT_SIZE {
        return copy_sequential(src, dst, start, end);  // Base case
    }
    
    let mid = align_to_page((start + end) / 2, 2MB);
    
    // Clone file handles for concurrent access
    let left = copy_region_recursive(src.clone(), dst.clone(), start, mid, depth+1, max_depth);
    let right = copy_region_recursive(src.clone(), dst.clone(), mid, end, depth+1, max_depth);
    
    futures::try_join!(left, right)?;  // Run in parallel
}
```

### Key Features

1. **Off by default** - Requires `--enabled` flag
2. **Configurable threshold** - `--min-file-size-mb` (default: 128MB)
3. **Depth-based** - `--max-depth N` creates up to 2^N parallel tasks
   - depth 2 = 4 tasks
   - depth 3 = 8 tasks
   - depth 4 = 16 tasks
4. **Page-aligned splits** - Aligns to 2MB huge page boundaries
5. **Adaptive** - Stops splitting when regions < 8MB

### File Handle Strategy

- **compio::fs::File** implements `Clone` (cheap Rust-level clone, not `dup()`)
- Each recursive branch gets its own mutable clone
- No shared mutable state - each task owns its file handles
- `read_at`/`write_at` are position-independent, so concurrent access is safe

### Critical Implementation Details

1. **fallocate first** - Pre-allocates entire destination file before parallel writes
   - Prevents fragmentation
   - Allows concurrent writes without conflicts
   
2. **Box::pin for recursion** - Async recursion requires boxing to avoid infinite size
   
3. **Fail-fast error handling** - `try_join!` aborts all branches on first error

## CLI Usage

```bash
# Enable with defaults (128MB threshold, depth 2 = 4 tasks)
arsync --enabled /source /dest

# Customize parameters  
arsync --enabled \
       --min-file-size-mb 256 \
       --max-depth 3 \              # 8 tasks
       --chunk-size-mb 4 \
       /source /dest
```

## Test Coverage

Comprehensive data integrity tests in `tests/parallel_copy_tests.rs`:

- ✅ **test_parallel_copy_data_integrity_large_file** - 200MB file, byte-perfect verification
- ✅ **test_parallel_copy_various_depths** - Tests depths 1-4 (2-16 tasks)
- ✅ **test_below_threshold_uses_sequential** - Verifies threshold behavior
- ✅ **test_uneven_file_split** - 17MB+777 bytes (uneven division)
- ✅ **test_no_region_overlap** - Verifies no byte shuffling/corruption

All tests pass ✅

## Performance Expectations

Based on design analysis (needs real-world benchmarking):

- **Sequential copy**: ~2-3 GB/s on NVMe
- **Parallel depth 2** (4 tasks): ~5-7 GB/s
- **Parallel depth 3** (8 tasks): ~7-9 GB/s
- **Parallel depth 4** (16 tasks): ~8-10 GB/s (diminishing returns)

## Memory Usage

Predictable and bounded:
- **Peak:** 2^depth × chunk_size_mb
- depth 2 (4 tasks): 4 × 2MB = 8MB
- depth 3 (8 tasks): 8 × 2MB = 16MB
- depth 4 (16 tasks): 16 × 2MB = 32MB

## Files Changed

### New Files
- `docs/PARALLEL_COPY_DESIGN.md` - Complete design document
- `tests/parallel_copy_tests.rs` - Data integrity test suite

### Modified Files
- `src/cli.rs` - Added `ParallelCopyConfig` with CLI parameters
- `src/copy.rs` - Added `copy_read_write_parallel()`, `copy_region_recursive()`, `copy_region_sequential()`
- `src/directory.rs` - Updated `copy_file()` call sites (currently with disabled config)

## Known Limitations / TODOs

1. **Directory traversal integration** - Currently parallel copy is disabled in directory traversal
   - Need to thread `ParallelCopyConfig` through the call chain
   - Needs consideration of memory usage: `max_files_in_flight × 2^depth × chunk_size`

2. **Benchmarking needed** - Real-world performance testing on NVMe
   - Validate expected throughput improvements
   - Tune default parameters based on results

3. **Progress tracking** - No per-region progress yet (not critical for MVP)

4. **Cross-filesystem support** - Needs testing on different filesystems

## Safety Guarantees

1. ✅ **No overlapping writes** - Each task writes to exclusive regions
2. ✅ **Atomic file creation** - fallocate reserves space upfront
3. ✅ **Data integrity verified** - Tests confirm byte-perfect copies
4. ✅ **Error propagation** - Any task error cancels entire operation
5. ✅ **Metadata preservation** - Same as sequential copy

## Next Steps

1. **Enable in directory traversal** - Thread config through call chain
2. **Run real benchmarks** - Measure actual throughput on NVMe
3. **Tune defaults** - Adjust based on benchmark results
4. **Add to README** - Document the feature for users
5. **Consider auto-tuning** - Detect storage type and adjust depth automatically

## References

- Design doc: `docs/PARALLEL_COPY_DESIGN.md`
- Tests: `tests/parallel_copy_tests.rs`
- Implementation: `src/copy.rs` lines 296-560

