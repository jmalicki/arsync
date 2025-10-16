# Implementation Plan: Race-Free Hardlink Synchronization with CondVar

**Status**: Planning
**Complexity**: High
**Estimated Duration**: 8-12 hours
**Created On Branch**: sync/hardlink-condvar
**Implementation Branch**: sync/hardlink-condvar
**Related Design**: [Design Document](design.md)

## Context

This plan implements condition variable-based synchronization for hardlink tracking to eliminate race conditions in concurrent file copying. The current `AtomicBool is_copied` approach has a TOCTOU race where multiple threads can copy the same file or create hardlinks before the file is fully written.

## Overview

Implement `CondVar` in `compio-sync` and refactor hardlink tracking to use proper wait/signal semantics. This ensures only one thread copies each unique inode, while other threads wait for completion before creating hardlinks.

## Design References

- **Design Document**: [design.md](design.md)
- Key decision: Use condition variables for copy-complete signaling
- Pattern: Atomic insert determines winner, condvar synchronizes completion
- Benefits: Race-free, no double copies, correct hardlink timing
- Risk: Medium - new synchronization primitive

## Prerequisites

- [x] Review design doc: [design.md](design.md)
- [ ] Research compio event/notification primitives
- [ ] Review `tokio::sync::Notify` implementation
- [ ] Review `crates/compio-sync/src/lib.rs` (existing Semaphore pattern)
- [ ] Review `src/directory.rs` lines 1209-1376 (HardlinkInfo and tracking)
- [ ] Understand current race condition in concurrent hardlink handling

## Phase 1: Research & Design CondVar API

**Objective**: Understand compio primitives and design CondVar implementation

### Steps

- [ ] Research compio primitives
  - Check compio documentation for Event or notification types
  - Review compio::runtime for async notification mechanisms
  - Look for existing sync primitives in compio

- [ ] Review reference implementations
  - Study `tokio::sync::Notify` source code
  - Understand one-shot vs multi-use notification
  - Review atomic vs lock-based approaches

- [ ] Review our Semaphore pattern
  - Read `crates/compio-sync/src/lib.rs`
  - Understand Arc wrapping pattern
  - See how acquire/release works with compio

- [ ] Design CondVar API for compio
  - Decide: Semaphore-based vs Event-based (if Event exists)
  - API: `wait()`, `notify_one()`, `notify_all()`
  - Cloning behavior (Arc-wrapped)
  - Error handling (if any)

- [ ] Write API documentation
  - Document usage pattern
  - Add example code
  - Note differences from std::sync::Condvar

### Quality Checks

- [ ] `/review` - Review API design decisions
- [ ] Document rationale in design doc if needed

### Deliverables
- Clear understanding of compio primitives
- CondVar API design
- Implementation approach decided

## Phase 2: Implement CondVar in compio-sync

**Objective**: Create working CondVar implementation with tests

### Steps

- [ ] Create `crates/compio-sync/src/condvar.rs`
  ```rust
  use compio_sync::Semaphore;
  use std::sync::Arc;
  
  pub struct CondVar {
      inner: Arc<CondVarInner>,
  }
  
  struct CondVarInner {
      semaphore: Semaphore,  // Or Event if available
  }
  
  impl CondVar {
      pub fn new() -> Self { ... }
      pub async fn wait(&self) { ... }
      pub fn notify_all(&self) { ... }
  }
  ```

- [ ] Implement `CondVar::new()`
  - Initialize with Semaphore(0) or Event
  - Wrap in Arc for cloning

- [ ] Implement `CondVar::wait()`
  - Async wait for notification
  - Should be cancellation-safe
  - Consider timeout (optional)

- [ ] Implement `CondVar::notify_one()`
  - Wake one waiter
  - Handle case of no waiters

- [ ] Implement `CondVar::notify_all()`
  - Wake all waiters
  - Efficient implementation (large permit count or broadcast)

- [ ] Implement `Clone` for `CondVar`
  - Clone the Arc wrapper
  - Test that clones share state

- [ ] Add comprehensive documentation
  - Module docs
  - Usage examples
  - Differences from std::sync::Condvar

- [ ] Export from `crates/compio-sync/src/lib.rs`
  ```rust
  pub mod condvar;
  pub use condvar::CondVar;
  ```

### Testing

- [ ] Write basic condvar test
  ```rust
  #[compio::test]
  async fn test_condvar_basic() {
      let cv = CondVar::new();
      let cv_clone = cv.clone();
      
      let handle = compio::runtime::spawn(async move {
          cv_clone.wait().await;
          42
      });
      
      cv.notify_all();
      assert_eq!(handle.await.unwrap(), 42);
  }
  ```

- [ ] Write multiple waiters test
  ```rust
  #[compio::test]
  async fn test_condvar_multiple_waiters() {
      // Spawn 10 waiters
      // Notify all
      // Verify all wake up
  }
  ```

- [ ] Write notify_one test
  - Spawn 3 waiters
  - Call notify_one() three times
  - Verify exactly 3 wake up

- [ ] Test with no waiters
  - Call notify_all() with no waiters
  - Should not error or panic

### Quality Checks

- [ ] `/fmt false true` - Format code
- [ ] `/clippy false false` - Check for warnings
- [ ] `/build "debug" "all" false` - Verify compilation
- [ ] `/test "condvar"` - Run condvar tests

### Files Modified
- `crates/compio-sync/src/condvar.rs` - New file
- `crates/compio-sync/src/lib.rs` - Export CondVar

### Tests
- `crates/compio-sync/src/condvar.rs` - Unit tests inline or in tests/ module

## Phase 3: Update HardlinkInfo with Atomics and CondVar

**Objective**: Refactor HardlinkInfo to use interior mutability pattern

### Steps

- [ ] Update imports in `src/directory.rs`
  - Add: `use std::sync::atomic::AtomicBool;`
  - Add: `use compio_sync::CondVar;`
  - Update Mutex import if needed

- [ ] Update `HardlinkInfo` struct (lines ~1210-1223)
  ```rust
  pub struct HardlinkInfo {
      pub original_path: PathBuf,  // Immutable
      pub inode_number: u64,  // Immutable
      pub link_count: AtomicU64,  // Atomic increment
      pub dst_path: PathBuf,  // Immutable! (set at creation)
      pub copy_complete: CondVar,  // Synchronization
  }
  ```
  - Remove `is_copied: AtomicBool`
  - Change `dst_path: Mutex<Option<PathBuf>>` to `dst_path: PathBuf`
  - Add `copy_complete: CondVar`

- [ ] Implement `Clone` for `HardlinkInfo` manually
  ```rust
  impl Clone for HardlinkInfo {
      fn clone(&self) -> Self {
          Self {
              original_path: self.original_path.clone(),
              inode_number: self.inode_number,
              link_count: AtomicU64::new(self.link_count.load(Ordering::Relaxed)),
              dst_path: self.dst_path.clone(),
              copy_complete: self.copy_complete.clone(),
          }
      }
  }
  ```

- [ ] Update `FilesystemTracker::register_file()` signature (line ~1275)
  - Add `dst_path: &Path` parameter
  - Return type: Change from `bool` to enum
  - Remove link_count == 1 skip (handle outside function)

- [ ] Define `HardlinkRegistration` enum
  ```rust
  pub enum HardlinkRegistration {
      FirstOccurrence {
          copy_complete: CondVar,
      },
      Hardlink {
          dst_path: PathBuf,
          copy_complete: CondVar,
      },
  }
  ```

- [ ] Implement new `register_file()` logic
  - Use entry() API for atomic insert
  - Vacant: Create HardlinkInfo with dst_path and condvar
  - Occupied: Increment link_count, return info for waiting

- [ ] Update `get_hardlink_groups()` (line ~1345)
  - Access `link_count.load(Ordering::Relaxed)` instead of direct field

- [ ] Remove obsolete methods
  - Remove `is_inode_copied()` - replaced by registration pattern
  - Remove `mark_inode_copied()` - signaling via condvar
  - Remove `get_original_path_for_inode()` - dst_path in registration result

### Quality Checks

- [ ] `/fmt false true` - Format code
- [ ] `/clippy false false` - Check for warnings
- [ ] `/build "debug" "all" false` - Verify compilation

### Files Modified
- `src/directory.rs` - HardlinkInfo struct and FilesystemTracker methods

### Tests
- Update existing tests to match new API (deferred to Phase 4)

## Phase 4: Update process_file() Hardlink Handling

**Objective**: Implement wait/signal pattern in file processing

### Steps

- [ ] Update `process_file()` function (lines ~853-914)
  - Change from if/else to match on registration result
  - Implement FirstOccurrence path (copy and signal)
  - Implement Hardlink path (wait and link)

- [ ] Implement FirstOccurrence (copier) path
  ```rust
  Some(HardlinkRegistration::FirstOccurrence { copy_complete }) => {
      // We won the race - copy the file
      copy_file(src, dst, metadata_config).await?;
      stats.increment_files_copied();
      stats.increment_bytes_copied(metadata.len());
      
      // Signal all waiting threads
      copy_complete.notify_all();
  }
  ```

- [ ] Implement Hardlink (linker) path
  ```rust
  Some(HardlinkRegistration::Hardlink { dst_path, copy_complete }) => {
      // Another thread is copying - wait
      copy_complete.wait().await;
      
      // Create parent dir if needed
      if let Some(parent) = dst.parent() {
          create_dir_all(parent).await?;
      }
      
      // Create hardlink
      create_hardlink(&dst_path, dst).await?;
      stats.increment_files_copied();
  }
  ```

- [ ] Handle None case (link_count == 1)
  ```rust
  None => {
      // Not a hardlink - just copy normally
      copy_file(src, dst, metadata_config).await?;
      stats.increment_files_copied();
      stats.increment_bytes_copied(metadata.len());
  }
  ```

- [ ] Move link_count == 1 check outside register_file()
  - Check before calling register_file()
  - Simplifies register_file() logic

- [ ] Update `SharedHardlinkTracker` wrapper methods
  - Update `register_file()` to pass dst_path
  - Remove `is_inode_copied()` method
  - Remove `mark_inode_copied()` method
  - Remove `get_original_path_for_inode()` method

- [ ] Update call sites in `process_file()`
  - Remove old if/else hardlink check
  - Replace with new match pattern

- [ ] Remove `handle_existing_hardlink()` helper function
  - Logic integrated into Hardlink match arm
  - Simplifies code flow

### Quality Checks

- [ ] `/fmt false true` - Format code
- [ ] `/clippy false false` - Check for warnings
- [ ] `/build "debug" "all" false` - Verify compilation
- [ ] `/test "directory"` - Run directory tests

### Files Modified
- `src/directory.rs` - process_file(), SharedHardlinkTracker methods

### Tests
- Existing tests may need updates
- Add concurrent hardlink test

## Phase 5: Add Concurrent Hardlink Tests

**Objective**: Verify no race conditions under concurrent access

### Steps

- [ ] Add concurrent hardlink registration test
  ```rust
  #[compio::test]
  async fn test_concurrent_hardlink_registration() {
      let tracker = Arc::new(FilesystemTracker::new());
      
      // Spawn 5 concurrent threads discovering same inode
      let mut handles = vec![];
      for i in 0..5 {
          let tracker = Arc::clone(&tracker);
          let handle = compio::runtime::spawn(async move {
              let result = tracker.register_file(
                  &PathBuf::from("/src/file"),
                  &PathBuf::from(format!("/dst/link{}", i)),
                  1, 100, 5
              );
              result
          });
          handles.push(handle);
      }
      
      // Collect results
      let mut copiers = 0;
      let mut linkers = 0;
      for handle in handles {
          match handle.await.unwrap() {
              Some(HardlinkRegistration::FirstOccurrence { .. }) => copiers += 1,
              Some(HardlinkRegistration::Hardlink { .. }) => linkers += 1,
              None => panic!("Should not happen"),
          }
      }
      
      // Verify exactly 1 copier, 4 linkers
      assert_eq!(copiers, 1, "Should have exactly 1 copier");
      assert_eq!(linkers, 4, "Should have 4 linkers");
  }
  ```

- [ ] Add wait/signal test
  ```rust
  #[compio::test]
  async fn test_hardlink_wait_signal() {
      let cv = CondVar::new();
      let cv_clone = cv.clone();
      
      // Spawn linker that waits
      let linker = compio::runtime::spawn(async move {
          cv_clone.wait().await;
          "linked"
      });
      
      // Simulate copy delay
      compio::time::sleep(Duration::from_millis(10)).await;
      
      // Signal completion
      cv.notify_all();
      
      // Verify linker completes
      assert_eq!(linker.await.unwrap(), "linked");
  }
  ```

- [ ] Add end-to-end concurrent copy test
  - Create directory with hardlinks
  - Copy with multiple concurrent threads
  - Verify no double copies
  - Verify all hardlinks created correctly

- [ ] Stress test with many concurrent hardlinks
  - 100 hardlinks to same inode
  - All discovered concurrently
  - Verify exactly 1 copy
  - Verify 99 hardlinks created

- [ ] Update existing hardlink tests
  - Update `test_filesystem_tracker_hardlinks`
  - Adjust for new API
  - Verify behavior unchanged

### Quality Checks

- [ ] `/test "condvar"` - CondVar tests pass
- [ ] `/test "directory"` - Directory tests pass
- [ ] `/test "all"` - All tests pass
- [ ] `/smoke` - Smoke tests pass

### Files Modified
- `src/directory.rs` - Add concurrent tests
- `crates/compio-sync/src/condvar.rs` - Add tests

### Tests
- New: Concurrent hardlink tests (3-4 tests)
- Updated: Existing hardlink tests

## Phase 6: Documentation & Cleanup

**Objective**: Clean up code, remove obsolete patterns, update documentation

### Steps

- [ ] Remove obsolete code
  - Verify no remaining `is_inode_copied()` calls
  - Verify no remaining `mark_inode_copied()` calls
  - Verify no remaining `get_original_path_for_inode()` calls

- [ ] Update module documentation
  - Document new wait/signal pattern
  - Add example of hardlink handling
  - Note thread-safety improvements

- [ ] Update `HardlinkInfo` documentation
  - Document immutable vs atomic fields
  - Explain CondVar usage
  - Add usage example

- [ ] Update `CondVar` documentation
  - Add module-level examples
  - Document common patterns
  - Note cancellation safety

- [ ] Add inline comments
  - Explain why dst_path is required at creation
  - Document wait/signal pattern in process_file()
  - Note race-free guarantees

- [ ] Update error handling documentation
  - CondVar operations are infallible
  - Simpler error handling overall

### Quality Checks

- [ ] `/fmt true true` - Verify formatting (check mode)
- [ ] `/clippy false false` - Verify no warnings
- [ ] `/docs true false` - Verify docs build
- [ ] `/test "all"` - All tests pass

### Files Modified
- `src/directory.rs` - Documentation updates
- `crates/compio-sync/src/condvar.rs` - Documentation

## Final Phase: Create Pull Request

### Pre-PR Verification

- [ ] `/fmt true true` - Verify formatting (check mode)
- [ ] `/clippy false false` - Verify no warnings
- [ ] `/test "all"` - All tests pass
- [ ] `/smoke` - Smoke tests pass
- [ ] `/build "release" "all" false` - Release build succeeds
- [ ] `/docs true false` - Documentation builds
- [ ] `/review` - Final review of changes

### Integration Tests

- [ ] Test with real directory containing hardlinks
- [ ] Verify no double copies in logs
- [ ] Verify correct hardlink creation
- [ ] Check for race conditions under load

### PR Creation

- [ ] `/commit "feat(sync): add CondVar for race-free hardlink synchronization"`
- [ ] `/pr "feat(sync): add CondVar for race-free hardlink synchronization" "See template" main false`
- [ ] Fill out PR template:
  - Summary: Added CondVar, fixed hardlink race condition
  - Motivation: Eliminate TOCTOU race in concurrent hardlink handling
  - Changes: New CondVar primitive, refactored HardlinkInfo, updated process_file()
  - Test plan: Concurrent hardlink tests, stress tests
  - Risks: New synchronization primitive, but well-tested
  - Benefits: Race-free, correct hardlink timing, simpler code
- [ ] `/pr-ready "feat(sync): add CondVar for race-free hardlink synchronization"`
- [ ] `/pr-checks` - Monitor CI checks

### PR Body Checklist

- [ ] Summary: Clear explanation of race condition fix
- [ ] Motivation: Why CondVar is needed (TOCTOU race)
- [ ] Changes: New CondVar, refactored HardlinkInfo, updated hardlink flow
- [ ] Test plan: Concurrent tests, stress tests, integration tests
- [ ] Before/after code examples showing race elimination
- [ ] Performance: No regression (or improvement)

## Summary

This plan implements race-free hardlink synchronization in 6 phases:

1. **Phase 1**: Research compio primitives (1-2 hours)
2. **Phase 2**: Implement CondVar in compio-sync (2-3 hours)
3. **Phase 3**: Update HardlinkInfo structure (1-2 hours)
4. **Phase 4**: Refactor process_file() logic (2-3 hours)
5. **Phase 5**: Add concurrent tests (1-2 hours)
6. **Phase 6**: Documentation and PR (1-2 hours)

Total estimated time: 8-14 hours

### Key Benefits

1. **Race-free**: Atomic insert ensures only one copier per inode
2. **Correct timing**: Hardlinks created after copy completes
3. **Simpler API**: Fewer methods, clearer semantics
4. **Lock-free**: No mutex on dst_path (immutable after creation)
5. **Reusable**: CondVar useful for other async coordination

### Technical Highlights

- New sync primitive: `CondVar` for async notification
- Atomic insert pattern: DashMap determines winner
- Wait/signal semantics: Proper concurrent coordination
- Immutable dst_path: Set once, never changes
- No boolean flags: Condvar signaling is the state

---

**Implementation on branch**: `sync/hardlink-condvar`

