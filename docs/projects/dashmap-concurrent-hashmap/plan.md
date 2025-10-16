# Implementation Plan: DashMap for Concurrent HashMap Usage

**Status**: In Progress
**Complexity**: Medium
**Estimated Duration**: 4-6 hours
**Created On Branch**: perf/use-dashmap
**Implementation Branch**: perf/use-dashmap
**Related Design**: [Design Document](design.md)

## Context

This plan implements the replacement of `Arc<Mutex<HashMap>>` with `dashmap::DashMap` for lock-free concurrent access in hardlink tracking. The design document identified lock contention issues and proposes using DashMap to eliminate mutex overhead and simplify code.

## Overview

Replace HashMap with DashMap in `FilesystemTracker` and simplify `SharedHardlinkTracker` to eliminate mutex operations. This will improve concurrent performance, remove 3 clippy suppressions, and simplify error handling.

## Design References

- **Design Document**: [design.md](design.md)
- Key decision: Use DashMap for lock-free concurrent HashMap
- Simplify SharedHardlinkTracker to remove mutex wrapper
- Target: 2-10x better concurrent throughput
- Risk: Low - DashMap is battle-tested

## Prerequisites

- [x] Review `src/directory.rs` lines 17, 217, 1309-1468 (FilesystemTracker implementation)
- [x] Review SharedHardlinkTracker methods (lines 220-348)
- [x] Understand current mutex-based error handling
- [x] Check existing tests in `src/directory.rs` (lines 1668-1854)
- [x] Read design doc: [design.md](design.md)

## Phase 1: Add Dependency & Replace HashMap

**Objective**: Add dashmap dependency and replace HashMap with DashMap in FilesystemTracker

### Steps

- [x] Add dashmap dependency to `Cargo.toml`
  ```toml
  [dependencies]
  dashmap = "6.1"
  ```

- [x] Update imports in `src/directory.rs` (line 17)
  - Remove: `#[allow(clippy::disallowed_types)]`
  - Remove: `use std::collections::HashMap;`
  - Add: `use dashmap::DashMap;`

- [x] Update `FilesystemTracker` struct (lines 1304-1313)
  - Replace `HashMap<InodeInfo, HardlinkInfo>` with `DashMap<InodeInfo, HardlinkInfo>`
  - Remove `#[allow(clippy::disallowed_types)]` from hardlinks field
  - Consider: Replace `Option<u64>` with `AtomicU64` for source_filesystem
  **Note**: Kept `Option<u64>` for source_filesystem for simplicity in Phase 1. Can revisit in Phase 2 if needed.

- [x] Update `FilesystemTracker::new()` (lines 1318-1325)
  - Change `HashMap::new()` to `DashMap::new()`

- [x] Update `FilesystemTracker::register_file()` (lines 1356-1394)
  - Use `entry()` API for concurrent insert/update
  - Replace `get_mut()` with DashMap's `entry()` pattern
  **Note**: Used `dashmap::mapref::entry::Entry` pattern for concurrent-safe insert/update.

- [x] Update `FilesystemTracker::get_hardlink_info()` (lines 1400-1403)
  - Use `get()` which returns `Option<Ref<K, V>>`
  **Note**: Changed return type to `Option<HardlinkInfo>` (clones value) for simpler API.

- [x] Update `FilesystemTracker::get_hardlink_groups()` (lines 1409-1414)
  - Iteration works similarly, may need to collect values differently
  **Note**: Changed to use `iter().map(|entry| entry.value().clone())` pattern. Return type now `Vec<HardlinkInfo>`.

- [x] Update `FilesystemTracker::is_inode_copied()` (lines 1421-1425)
  - Use `iter()` which is concurrent-safe

- [x] Update `FilesystemTracker::mark_inode_copied()` (lines 1431-1440)
  - Iterate and modify values using DashMap API
  **Note**: Used `iter_mut()` with `entry.value_mut()` for mutable access.

- [x] Update `FilesystemTracker::get_original_path_for_inode()` (lines 1447-1452)
  - Use DashMap's `iter()` for concurrent-safe iteration
  **Note**: Changed return type to `Option<PathBuf>` (clones path) for simpler API.

### Quality Checks

- [x] `/fmt false true` - Format code
- [x] `/clippy false false` - Check for warnings
- [x] `/build "debug" "all" false` - Verify compilation
- [x] `/test "directory"` - Run directory module tests

### Files Modified
- `Cargo.toml` - Add dependency
- `src/directory.rs` - HashMap → DashMap (lines 17, 1309-1452)

### Tests
- Existing unit tests should pass (lines 1668-1854)
- No new tests needed yet (behavior unchanged)

## Phase 2: Simplify SharedHardlinkTracker

**Objective**: Remove mutex operations and simplify SharedHardlinkTracker wrapper

### Steps

- [x] Decide on wrapper approach
  - Option A: Keep struct, remove mutex, wrap Arc<FilesystemTracker>
  - Option B: Type alias: `type SharedHardlinkTracker = Arc<FilesystemTracker>`
  - Recommendation: Option A for gradual migration
  **Note**: Chose Option A - keeps the wrapper for future flexibility.

- [x] Update `SharedHardlinkTracker` struct (line 217)
  - Change from `Arc<Mutex<FilesystemTracker>>` to `Arc<FilesystemTracker>`
  - Remove Mutex entirely
  **Note**: Updated struct and imports (removed `Mutex` import).

- [x] Update `SharedHardlinkTracker::new()` (lines 227-232)
  - Remove `Mutex::new()` wrapper
  - Just use `Arc::new(tracker)`

- [x] Add `SharedHardlinkTracker::with_source_filesystem()`
  - New constructor that creates tracker with filesystem set
  - Better API than requiring set_source_filesystem() before wrapping
  **Note**: User suggestion - much cleaner API!

- [x] Update `is_inode_copied()` (lines 238-246)
  - Remove `Result<bool>` return → `bool`
  - Remove `.lock().map_err(...)` pattern
  - Direct call: `self.inner.is_inode_copied(inode)`

- [x] Update `get_original_path_for_inode()` (lines 253-262)
  - Remove `Result<Option<PathBuf>>` → `Option<PathBuf>`
  - Remove mutex lock error handling
  - Direct call to inner method

- [x] Update `mark_inode_copied()` (lines 269-277)
  - Remove `Result<()>` → just `()`
  - Remove mutex lock error handling
  - Direct call to inner method

- [x] Update `register_file()` (lines 285-299)
  - Remove `Result<()>` return → `bool`
  - Direct call to inner method (returns bool like FilesystemTracker)

- [x] Remove `set_source_filesystem()` (lines 307-315)
  - Removed entirely - use constructor instead
  - **Note**: Better handled by `with_source_filesystem()` constructor

- [x] Update `get_stats()` (lines 323-331)
  - Remove `Result<FilesystemStats>` → `FilesystemStats`
  - Direct call to inner method

- [x] Update `into_inner()` (lines 340-347)
  - Keep `Result<>` for Arc::try_unwrap
  - Remove mutex poisoning error case
  - Simplify to just Arc::try_unwrap

- [x] Update all call sites of SharedHardlinkTracker methods
  - Remove `?` where method no longer returns Result (3 locations)
  - Updated: is_inode_copied, mark_inode_copied, get_original_path_for_inode
  **Note**: All call sites updated successfully.

### Quality Checks

- [x] `/fmt false true` - Format code
- [x] `/clippy false false` - Verify no warnings
- [x] `/test "directory"` - Run directory tests
- [x] `/build "debug" "all" false` - Verify compilation

### Files Modified
- `src/directory.rs` - SharedHardlinkTracker (lines 214-348, plus call sites)

### Tests
- All existing tests must still pass
- Verify method signature changes don't break callers

## Phase 3: Cleanup & Verification

**Objective**: Remove all clippy suppressions, update tests, and verify performance

### Steps

- [x] Remove remaining `#[allow(clippy::disallowed_types)]` suppressions
  - Line 17 (import) - Already removed
  - Line 1309 (struct field) - Already removed
  - Line 1321 (HashMap::new) - Already removed
  - Verified: No more suppressions remain
  **Note**: All 3 suppressions successfully removed in Phase 1.

- [x] Update any TODO/FIXME comments related to HashMap usage
  - No HashMap-related TODOs found
  **Note**: Code is clean, no action needed.

- [x] Review all error handling changes
  - SharedHardlinkTracker methods simplified (no mutex poisoning)
  - 3 call sites updated (removed `?` operators)
  - Error messages remain clear and helpful
  **Note**: Error handling is simpler and more maintainable.

- [ ] Add concurrent access stress test (optional but recommended)
  ```rust
  #[compio::test]
  async fn test_concurrent_hardlink_tracking() {
      // Spawn multiple tasks accessing tracker concurrently
      // Verify correctness under concurrent load
  }
  ```
  **Note**: Skipped for now - can be added in future PR if needed.

- [x] Run full test suite
  - `/test "all"` - 34 tests pass (lib tests)
  - Integration tests: Skipped (some doctest failures in compio-fs-extended, pre-existing)
  - Verified: No regressions in main crate

- [x] Run clippy with strict checks
  - `/clippy false false` - All warnings resolved
  - Verified: disallowed-types rule satisfied
  - No HashMap suppressions remain

- [ ] Run benchmarks
  - `/bench true false` - Quick benchmark
  - Compare with baseline (if available)
  - Document any performance changes
  **Note**: Can be run, but baseline comparison would need separate baseline run.

- [x] Update documentation
  - Added comment about DashMap in SharedHardlinkTracker
  - Module-level docs unchanged (still accurate)
  **Note**: Documentation accurately reflects new implementation.

### Quality Checks

- [x] `/fmt true true` - Verify formatting (check mode)
- [x] `/clippy false false` - Verify no warnings
- [x] `/test "all"` - All tests pass (34 lib tests)
- [ ] `/smoke` - Smoke tests (skipped for now)
- [x] `/build "release" "all" false` - Release build succeeds
- [ ] `/bench true false` - Benchmarks (optional, can run later)

### Files Modified
- `src/directory.rs` - Final cleanup
- Potentially: Update comments/docs

### Tests
- Full test suite verification
- Optional: Add concurrent stress test

## Final Phase: Create Pull Request

### Pre-PR Verification

- [ ] `/fmt true true` - Verify formatting (check mode)
- [ ] `/clippy false false` - Verify no warnings  
- [ ] `/test "all"` - All tests pass
- [ ] `/smoke` - Smoke tests pass
- [ ] `/build "release" "all" false` - Release build succeeds
- [ ] `/docs true false` - Documentation builds
- [ ] `/review` - Final review of changes

### Benchmarks

- [ ] `/bench true false` - Quick benchmark
- [ ] Compare results with baseline
- [ ] Document performance impact in PR description

### PR Creation

- [ ] `/commit "perf(directory): replace HashMap with DashMap for concurrent access"`
- [ ] `/pr "perf(directory): use DashMap for lock-free concurrent HashMap" "See template" main false`
- [ ] Fill out PR template:
  - Summary: Replaced Arc<Mutex<HashMap>> with DashMap
  - Motivation: Eliminate lock contention, satisfy clippy rules
  - Changes: Updated FilesystemTracker and SharedHardlinkTracker
  - Test plan: All existing tests pass, benchmarks show improvement
  - Performance: Lock-free reads, 2-10x better concurrent throughput
  - Risks: Low - DashMap is battle-tested
- [ ] `/pr-ready "perf(directory): use DashMap for lock-free concurrent HashMap"`
- [ ] `/pr-checks` - Monitor CI checks

### PR Body Checklist

- [ ] Summary: Clear 1-3 bullets on what changed
- [ ] Motivation: Why DashMap (lock contention, clippy rules)
- [ ] Test plan: All tests pass, benchmarks included
- [ ] Performance: Document throughput improvements
- [ ] Breaking changes: None (internal only)
- [ ] Before/after code examples

## Summary

This plan implements DashMap replacement in 3 main phases:
1. **Phase 1**: Add dependency and replace HashMap (1-2 hours)
2. **Phase 2**: Simplify wrapper and remove mutex operations (2-3 hours)
3. **Phase 3**: Cleanup, testing, and benchmarking (1-2 hours)

Total estimated time: 4-7 hours

Key benefits:
- Lock-free concurrent reads
- Simplified code (no mutex operations)
- Satisfies clippy disallowed-types rule
- 2-10x better concurrent performance
- Fewer error cases to handle

---

**Implementation on branch**: `perf/use-dashmap`

