# Semaphore-Based Concurrency Control Design

## Problem Statement

The original directory traversal implementation used breadth-first search (BFS) with unbounded concurrent operations. This created several issues:

1. **Unbounded Memory Growth**: During BFS, all discovered files and directories were dispatched immediately, creating an unbounded queue of pending operations
2. **Resource Exhaustion**: On directories with thousands of files, this could exhaust file descriptors, memory, and kernel resources
3. **Poor Backpressure**: No mechanism to slow down directory discovery when file processing couldn't keep up

## Solution: Async Semaphore with Bounded Concurrency

### Architecture

We implemented a custom async semaphore compatible with the [compio](https://github.com/compio-rs/compio) runtime to bound the number of concurrent operations:

```
┌─────────────────────────────────────────┐
│  Directory Traversal (BFS)              │
│                                         │
│  ┌───────────────────────────────────┐ │
│  │ Semaphore (max_files_in_flight)   │ │
│  │  - Default: 1024 permits          │ │
│  │  - Configurable via CLI           │ │
│  └───────────────────────────────────┘ │
│                 │                       │
│                 ▼                       │
│  ┌───────────────────────────────────┐ │
│  │ acquire() → SemaphorePermit       │ │
│  │  - Blocks if no permits available │ │
│  │  - RAII: auto-release on drop     │ │
│  └───────────────────────────────────┘ │
│                 │                       │
│                 ▼                       │
│  ┌───────────────────────────────────┐ │
│  │ Process Entry (file/dir/symlink)  │ │
│  │  - Holds permit for entire op     │ │
│  │  - Releases when complete         │ │
│  └───────────────────────────────────┘ │
└─────────────────────────────────────────┘
```

### Key Design Decisions

#### 1. Permit Acquisition Scope

**Decision**: Acquire one permit per directory entry (file, directory, or symlink), held for the entire processing duration.

**Rationale**:
- **Coarse-grained control**: Simpler to reason about and prevents over-subscription
- **Natural backpressure**: When max permits are in use, new directory entries block until existing operations complete
- **Covers all operations**: Includes metadata reads, file copies, directory creation, and symlink handling

**Alternative considered**: Acquire permits only for file operations
- **Rejected**: Would still allow unbounded directory discovery, defeating the purpose

#### 2. Semaphore Implementation

**Decision**: Custom implementation using atomics, mutex, and waker queue

**Key features**:
- **Lock-free fast path**: Uses `AtomicUsize` for acquiring/releasing when permits are available
- **FIFO waker queue**: Ensures fair ordering and prevents starvation
- **RAII permits**: `SemaphorePermit` automatically releases on drop, preventing leaks
- **compio-compatible**: Uses `std::task::Waker` for async notification

**Code location**: `crates/compio-sync/src/semaphore.rs`

#### 3. CLI Integration

**CLI flag**: `--max-files-in-flight <N>`
- **Default**: 1024 (good balance for NVMe SSDs)
- **Recommended for NVMe**: 512-2048
- **Recommended for HDD**: 64-256
- **Type**: `usize` (unsigned integer)

**Design consideration**: Global limit, not per-core
- Each operation counts as one unit regardless of which CPU core processes it
- Simplifies reasoning and prevents core count from affecting memory usage

### Integration Points

#### Directory Traversal

Location: `src/directory.rs::traverse_and_copy_directory_iterative()`

1. **Semaphore creation**:
   ```rust
   let semaphore = SharedSemaphore::new(args.max_files_in_flight);
   ```

2. **Shared across all operations**:
   ```rust
   let semaphore = semaphore.clone(); // Arc clone, not deep copy
   ```

3. **Passed to recursive calls**:
   ```rust
   process_directory_entry_with_compio(
       /* ... */
       semaphore,
       /* ... */
   )
   ```

#### Entry Processing

Location: `src/directory.rs::process_directory_entry_with_compio()`

**Critical section**:
```rust
async fn process_directory_entry_with_compio(/* ... */) -> Result<()> {
    // Acquire permit at the start
    let _permit = semaphore.acquire().await;
    
    // Process entry (file/dir/symlink)
    // ...
    
    // Permit auto-released when _permit drops at function exit
}
```

**Behavior**:
- If permits available: proceeds immediately
- If at limit: blocks until another operation completes and releases its permit
- On error: permit still released (RAII guarantees cleanup)

### Performance Characteristics

#### Time Complexity

- **Acquire (fast path)**: O(1) - atomic decrement
- **Acquire (slow path)**: O(1) - mutex lock + queue push + await
- **Release (no waiters)**: O(1) - atomic increment
- **Release (with waiters)**: O(1) - mutex lock + queue pop + wake

#### Memory Usage

- **Semaphore struct**: ~40 bytes (Arc + atomic + mutex + VecDeque)
- **Per-permit overhead**: 0 bytes (permits are counts, not allocations)
- **Waker queue**: 8 bytes per waiting task (pointer-sized)

#### Throughput Impact

**Before (unbounded)**:
- Queue depth: unlimited
- Memory usage: O(total_files) in worst case
- Risk: OOM, file descriptor exhaustion

**After (bounded)**:
- Queue depth: `max_files_in_flight`
- Memory usage: O(max_files_in_flight)
- Risk: eliminated

**Performance cost**: Minimal
- Fast path (permits available): ~5 CPU cycles (atomic operation)
- Slow path (blocking): Negligible compared to I/O latency (microseconds vs milliseconds)

### Testing Strategy

#### Unit Tests

Location: `crates/compio-sync/src/semaphore.rs`

1. **Basic acquire/release**: Single task acquires and releases
2. **Concurrent access**: Multiple tasks contend for permits
3. **Blocking behavior**: Verify tasks block when permits exhausted
4. **RAII cleanup**: Verify permits released on drop
5. **Fairness**: Verify FIFO ordering of blocked tasks

#### Integration Tests

Location: `tests/integration_tests.rs`, `tests/rsync_compat.rs`

- All existing integration tests pass with bounded concurrency
- Confirms no deadlocks or resource leaks
- Validates that limiting concurrency doesn't break correctness

#### Performance Tests

Location: `tests/performance_metadata_tests.rs`

- `test_metadata_preservation_many_small_files`: 100 files processed correctly
- `test_metadata_preservation_concurrent_operations`: Concurrent processing works
- All performance tests pass with semaphore enabled

### Future Enhancements

1. **Dynamic adjustment**: Adjust `max_files_in_flight` based on system load
2. **Per-operation-type limits**: Different limits for files vs directories
3. **Metrics**: Track permit utilization, wait times, queue depth
4. **Weighted permits**: Large files acquire more permits than small files

### References

- [compio runtime](https://github.com/compio-rs/compio)
- [Tokio semaphore](https://docs.rs/tokio/latest/tokio/sync/struct.Semaphore.html) - Design inspiration
- [io_uring](https://kernel.dk/io_uring.pdf) - Underlying async I/O mechanism
- [Semaphore (programming)](https://en.wikipedia.org/wiki/Semaphore_(programming))

### Related Documents

- [`IMPLEMENTATION_PLAN.md`](../IMPLEMENTATION_PLAN.md) - Overall project plan
- [`crates/compio-sync/README.md`](../crates/compio-sync/README.md) - compio-sync crate documentation
- [`TESTING_STRATEGY.md`](../TESTING_STRATEGY.md) - Comprehensive testing approach
