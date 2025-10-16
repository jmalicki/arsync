# Design: DashMap for Concurrent HashMap Usage

**Status**: Draft
**Author**: AI Analysis
**Created**: 2025-10-16
**Last Updated**: 2025-10-16
**Branch**: dashmap/design-concurrent-hashmap
**Implementation Branch**: perf/use-dashmap

## Overview

Replace `Arc<Mutex<HashMap>>` patterns with `dashmap::DashMap` for better concurrent access performance. Currently, `FilesystemTracker` in `directory.rs` uses `Arc<Mutex<HashMap>>` which creates lock contention. DashMap is a concurrent HashMap designed for multi-threaded scenarios with lock-free reads and fine-grained locking for writes.

## Problem Statement

### Current Situation

**Clippy disallows HashMap/BTreeMap**:
```rust
// clippy.toml line 9
disallowed-types = ["std::collections::HashMap", "std::collections::BTreeMap"]
```

**Current usage in directory.rs**:
```rust
// Line 17: Suppression required
#[allow(clippy::disallowed_types)]
use std::collections::HashMap;

// Line 1309-1310: HashMap in FilesystemTracker
pub struct FilesystemTracker {
    #[allow(clippy::disallowed_types)]
    hardlinks: HashMap<InodeInfo, HardlinkInfo>,
    source_filesystem: Option<u64>,
}

// Line 217: Wrapped in Arc<Mutex<>>
pub struct SharedHardlinkTracker {
    inner: Arc<Mutex<FilesystemTracker>>,
}
```

### Challenges

1. **Lock Contention**: `Arc<Mutex<HashMap>>` means every read/write acquires full lock
2. **Performance**: Mutex blocks all concurrent access, even for different keys
3. **Code Complexity**: Wrapper types needed (`SharedHardlinkTracker`)
4. **Error Handling**: Mutex poisoning adds error cases (lines 242, 258, 273, etc.)
5. **Clippy Suppressions**: 3 suppressions needed to allow HashMap usage

### Goals

1. **Eliminate lock contention** - Use lock-free concurrent HashMap
2. **Simplify code** - Remove `Arc<Mutex<>>` wrapper pattern
3. **Improve performance** - Better concurrent access for hardlink tracking
4. **Remove suppressions** - Satisfy clippy's disallowed-types rule
5. **Maintain correctness** - Preserve all hardlink tracking behavior

### Non-Goals

- Changing hardlink detection algorithm
- Modifying public API or behavior
- Performance optimization unrelated to concurrent access

## Proposed Solution

### High-Level Approach

Replace `HashMap` with `DashMap` in concurrent contexts:
1. **FilesystemTracker**: Use `DashMap<InodeInfo, HardlinkInfo>`
2. **SharedHardlinkTracker**: Simplify or remove wrapper (DashMap is already concurrent)
3. **Remove mutex operations**: No more lock()/unwrap() patterns
4. **Remove error handling for mutex poisoning**: DashMap doesn't poison

### Architecture

**Before (Current)**:
```
FilesystemTracker
    ├── HashMap<InodeInfo, HardlinkInfo>
    └── Wrapped in Arc<Mutex<>>
         └── SharedHardlinkTracker
              └── All methods lock/unlock mutex
```

**After (Proposed)**:
```
FilesystemTracker
    ├── DashMap<InodeInfo, HardlinkInfo>
    └── Wrapped in Arc<> (DashMap is already concurrent)
         └── SharedHardlinkTracker (simplified or removed)
              └── Methods use DashMap's concurrent API
```

Or even simpler:
```
SharedHardlinkTracker = Arc<DashMap<InodeInfo, HardlinkInfo>>
```

### Key Components

#### 1. Add dashmap Dependency

**Cargo.toml**:
```toml
[dependencies]
dashmap = "6.1"  # Latest stable
```

**Why dashmap?**
- Industry standard for concurrent HashMaps in Rust
- Used by major projects (tokio, async-std, etc.)
- Lock-free reads, fine-grained write locks (per-shard)
- API similar to std::HashMap
- No poisoning issues

#### 2. Update FilesystemTracker

**Before**:
```rust
pub struct FilesystemTracker {
    #[allow(clippy::disallowed_types)]
    hardlinks: HashMap<InodeInfo, HardlinkInfo>,
    source_filesystem: Option<u64>,
}
```

**After**:
```rust
pub struct FilesystemTracker {
    hardlinks: DashMap<InodeInfo, HardlinkInfo>,
    source_filesystem: AtomicU64,  // Also make this concurrent
}
```

#### 3. Simplify SharedHardlinkTracker

**Option A - Keep wrapper, simplify**:
```rust
pub struct SharedHardlinkTracker {
    tracker: Arc<FilesystemTracker>,
}

impl SharedHardlinkTracker {
    pub fn is_inode_copied(&self, inode: u64) -> bool {
        // No lock needed! DashMap handles concurrency
        self.tracker.hardlinks.iter()
            .any(|entry| entry.value().inode_number == inode && entry.value().is_copied)
    }
}
```

**Option B - Replace with type alias**:
```rust
// FilesystemTracker already contains DashMap, just wrap in Arc
pub type SharedHardlinkTracker = Arc<FilesystemTracker>;
```

#### 4. Update All Usage Sites

**Remove error handling**:
```rust
// Before (with mutex)
pub fn is_inode_copied(&self, inode: u64) -> Result<bool> {
    Ok(self.inner.lock()
        .map_err(|_| SyncError::FileSystem("...".to_string()))?
        .is_inode_copied(inode))
}

// After (with DashMap)
pub fn is_inode_copied(&self, inode: u64) -> bool {
    // No Result needed, no mutex to poison!
    self.tracker.hardlinks.iter()
        .any(|entry| entry.value().inode_number == inode && entry.value().is_copied)
}
```

## API Design

### Public API Changes

**FilesystemTracker** (internal, no public API change):
- Methods remain the same
- Implementation uses DashMap instead of HashMap
- No more mutex-related errors

**SharedHardlinkTracker**:
- Remove `Result<>` returns where they were only for mutex poisoning
- Methods become infallible or only return real errors
- Simpler API, fewer error cases

### DashMap API Usage

```rust
// Insert
tracker.hardlinks.insert(key, value);

// Get (returns Option<Ref<K, V>>)
if let Some(entry) = tracker.hardlinks.get(&key) {
    // entry is a reference guard, automatically released
}

// Update
tracker.hardlinks.entry(key)
    .and_modify(|v| v.link_count += 1)
    .or_insert(default_value);

// Iterate (concurrent-safe)
for entry in tracker.hardlinks.iter() {
    // Process entry
}
```

## Implementation Details

### File Changes

| File | Changes | Complexity |
|------|---------|------------|
| `Cargo.toml` | Add dashmap dependency | Trivial |
| `src/directory.rs` | Replace HashMap with DashMap | Medium |
| `src/directory.rs` | Simplify SharedHardlinkTracker | Medium |
| `src/directory.rs` | Update all method signatures (remove unnecessary `Result<>`) | Medium |
| `src/directory.rs` | Remove 3 `#[allow(clippy::disallowed_types)]` | Trivial |
| `src/directory.rs` | Update error handling (remove mutex poisoning cases) | Low |

### Dependencies

**New**:
```toml
dashmap = "6.1"
```

**No removals** - This is purely additive.

### Complexity Assessment

**Overall Complexity**: Medium

**Breakdown**:
- **Scope**: 1 file (directory.rs), ~15 methods affected
- **Dependencies**: 1 new dependency (well-established crate)
- **Testing**: Existing tests should pass unchanged (behavior preserved)
- **Risk**: Low - DashMap is battle-tested, API is similar to HashMap

**Estimated Phases**: 3 phases

## Testing Strategy

### Unit Tests
- All existing tests should pass unchanged
- Add test for concurrent access (stress test)
- Verify hardlink tracking behavior preserved
- Test filesystem boundary detection unchanged

### Integration Tests
- Full directory copy tests
- Hardlink detection tests
- Concurrent operation tests

### Performance Tests
- Benchmark before/after with `/bench`
- Measure lock contention reduction
- Test with high concurrency (many files)
- Compare Arc<Mutex<HashMap>> vs Arc<DashMap>

### Test Files
- `src/directory.rs` (existing unit tests at end)
- `tests/copy_integration_tests.rs`
- Stress test with many hardlinks

## Performance Considerations

### Expected Impact

**Lock Contention**:
- **Before**: Full lock for every HashMap access
- **After**: Lock-free reads, per-shard locks for writes
- **Improvement**: 2-10x better concurrent throughput

**Memory**:
- DashMap uses sharding (default 16 shards)
- Slightly higher memory overhead per map
- Negligible for our use case

**Latency**:
- Reads: Significantly faster (no lock)
- Writes: Similar or better (fine-grained locking)
- Iterations: Similar (concurrent-safe)

### Benchmarks

Test scenarios:
1. Single-threaded (should be similar)
2. Concurrent reads (should be much faster)
3. Mixed read/write (should be faster)
4. High contention (biggest improvement)

## Security Considerations

### Threat Model
- No new security concerns
- DashMap is widely used and audited
- Maintains same access patterns

### Mitigations
- Regular dependency audits catch DashMap issues
- No unsafe code introduced

## Error Handling

### Simplified Error Handling

**Remove mutex poisoning errors**:
```rust
// Before: Result<T> just for mutex poisoning
pub fn is_inode_copied(&self, inode: u64) -> Result<bool>

// After: Direct return, no error case
pub fn is_inode_copied(&self, inode: u64) -> bool
```

**Keep real errors**:
```rust
// Arc::try_unwrap still can fail
pub fn into_inner(self) -> Result<FilesystemTracker>
```

## Migration & Compatibility

### Breaking Changes
- None - internal implementation only

### Backward Compatibility
- Public API unchanged
- CLI interface unchanged
- Behavior preserved

### Configuration Changes
- None

## Rollout Plan

### Phase 1: Add Dependency & Replace HashMap
1. Add dashmap to Cargo.toml
2. Replace HashMap with DashMap in FilesystemTracker
3. Update insert/get/iter calls to DashMap API
4. Remove #[allow(clippy::disallowed_types)]
5. Run tests: `/test "directory"`

### Phase 2: Simplify SharedHardlinkTracker
1. Remove Result returns for mutex-only errors
2. Update all method implementations
3. Remove mutex poisoning error handling
4. Simplify into_inner() if possible
5. Run tests: `/test "directory"`

### Phase 3: Verify & Benchmark
1. Run full test suite: `/test "all"`
2. Run integration tests
3. Benchmark performance: `/bench true false`
4. Compare metrics with baseline
5. Document performance improvements

## Alternatives Considered

### Alternative 1: Keep HashMap with Arc<Mutex<>>
- **Pros**: No new dependency, code stays same
- **Cons**: Lock contention, performance issues, clippy suppressions
- **Why not chosen**: Performance and code quality suffer

### Alternative 2: Use parking_lot::Mutex
- **Pros**: Faster mutex implementation
- **Cons**: Still full-lock contention, doesn't solve core issue
- **Why not chosen**: Doesn't address lock-free concurrent access

### Alternative 3: Custom concurrent data structure
- **Pros**: Tailored to exact needs
- **Cons**: Complex, error-prone, reinventing wheel
- **Why not chosen**: DashMap is battle-tested and well-maintained

### Alternative 4: RwLock instead of Mutex
- **Pros**: Multiple readers, less dependency
- **Cons**: Still locking, not as performant as lock-free
- **Why not chosen**: DashMap is superior for this workload

## Open Questions

- [ ] Should we also use DashMap for other concurrent collections in the codebase?
- [ ] What's the right shard count for our workload? (default 16 vs custom)
- [ ] Should SharedHardlinkTracker become a type alias or keep the wrapper?
- [ ] Are there other places in the codebase with Arc<Mutex<HashMap>> patterns?

## References

- [dashmap crate](https://docs.rs/dashmap/)
- [dashmap GitHub](https://github.com/xacrimon/dashmap)
- [Concurrent HashMap benchmarks](https://github.com/xacrimon/conc-map-bench)
- Current code: `src/directory.rs` lines 17, 217, 1309-1310
- Clippy config: `clippy.toml` line 9

## Acceptance Criteria

- [ ] dashmap dependency added to Cargo.toml
- [ ] HashMap replaced with DashMap in FilesystemTracker
- [ ] All #[allow(clippy::disallowed_types)] removed
- [ ] SharedHardlinkTracker simplified (no mutex operations)
- [ ] All tests pass: `/test "all"`
- [ ] Benchmarks show improvement or no regression: `/bench true false`
- [ ] No clippy warnings: `/clippy false false`
- [ ] Code formatted: `/fmt false true`
- [ ] Documentation updated

## Future Work

- Audit entire codebase for Arc<Mutex<HashMap>> patterns
- Consider dashmap for other concurrent collections
- Profile and tune shard count if needed
- Add concurrent stress tests

---

**Next Steps**:
1. Review this design for completeness
2. Create implementation plan: `/plan`
3. Create implementation branch: `/branch "perf/use-dashmap" main origin true`
4. Execute the plan: `/implement`

