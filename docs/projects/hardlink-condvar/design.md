# Design: Condition Variables for Hardlink Synchronization

**Status**: Draft
**Author**: AI Analysis
**Created**: 2025-10-16
**Last Updated**: 2025-10-16
**Branch**: sync/hardlink-condvar
**Implementation Branch**: sync/hardlink-condvar

## Overview

Replace `is_copied: AtomicBool` and mutex-protected `dst_path` in hardlink tracking with proper condition variable synchronization. This fixes a race condition where multiple threads discovering the same inode can all attempt to copy the file, or create hardlinks before the file is fully copied.

## Problem Statement

### Current Race Condition

**Current implementation** in `HardlinkInfo`:
```rust
pub struct HardlinkInfo {
    pub link_count: AtomicU64,
    pub is_copied: AtomicBool,        // ❌ Race condition!
    pub dst_path: Mutex<Option<PathBuf>>,  // ❌ Incorrect synchronization!
}
```

**Race scenario**:
1. Thread A discovers inode 123, checks `is_copied == false`
2. Thread B discovers inode 123, checks `is_copied == false` 
3. Both threads start copying! ❌
4. Or: Thread A is still copying, Thread B creates hardlink too early ❌

**The problem**: Atomic bool doesn't provide synchronization between:
- Check: "Is file copied?"
- Wait: "Wait for copy to complete"
- Signal: "Copy complete, proceed with hardlinks"

### Why Current Approach Fails

```rust
// Thread A (first hardlink)
if !is_copied.load(Ordering::Relaxed) {
    copy_file(src, dst).await;  // ← Taking time...
    is_copied.store(true, Ordering::Relaxed);
    *dst_path.lock() = Some(dst);
}

// Thread B (second hardlink, concurrent)
if is_copied.load(Ordering::Relaxed) {  // ← Might be false while A is copying!
    let path = dst_path.lock().unwrap();  // ← Might be None!
    create_hardlink(path);  // ← Creates hardlink to incomplete file!
}
```

### Goals

1. **Atomic "try register"** - Only one thread copies the file
2. **Wait for completion** - Other threads wait for copy to finish
3. **Signal completion** - Copier signals when ready for hardlinks
4. **Lock-free reads** - Checking if inode is registered doesn't block

### Non-Goals

- Changing hardlink detection algorithm
- Modifying DashMap usage
- Performance optimization unrelated to synchronization

## Proposed Solution

### High-Level Approach

Use condition variables for proper "copy once, hardlink many" synchronization:

1. **Atomic insert** - Use DashMap's `insert` to atomically determine "winner"
2. **Condition variable** - Winner copies, losers wait on condvar
3. **Signal on complete** - Winner signals condvar after copy completes
4. **Create hardlinks** - Losers wake up and create hardlinks

### Architecture

**Before (Current - Has Race)**:
```
Thread A: Check is_copied → false → Copy file → Set is_copied
Thread B: Check is_copied → false → Copy file (RACE!)
```

**After (Proposed - Race-Free)**:
```
Thread A: Try insert → Success → Copy file → Signal condvar
Thread B: Try insert → Fail (exists) → Wait on condvar → Create hardlink
```

### Key Components

#### 1. Add Condition Variable to compio-sync

**Location**: `crates/compio-sync/src/condvar.rs` (new file)

**Inspiration**: 
- `tokio::sync::Notify` - async notification primitive
- Our existing `compio-sync::Semaphore` - pattern to follow

**API**:
```rust
pub struct CondVar {
    inner: Arc<CondVarInner>,
}

impl CondVar {
    pub fn new() -> Self { ... }
    
    /// Wait for notification (async)
    pub async fn wait(&self) { ... }
    
    /// Notify one waiter
    pub fn notify_one(&self) { ... }
    
    /// Notify all waiters
    pub fn notify_all(&self) { ... }
}
```

**Implementation approach**:
- Use `compio::event::Event` or `Semaphore` internally
- Similar pattern to our `Semaphore` wrapper
- Look at tokio's `Notify` for async-first design

#### 2. Update HardlinkInfo Structure

**Before**:
```rust
pub struct HardlinkInfo {
    pub original_path: PathBuf,
    pub inode_number: u64,
    pub link_count: AtomicU64,
    pub is_copied: AtomicBool,  // ❌ Remove
    pub dst_path: Mutex<Option<PathBuf>>,  // ❌ Remove
}
```

**After**:
```rust
pub struct HardlinkInfo {
    pub original_path: PathBuf,  // Source path (immutable)
    pub inode_number: u64,  // Inode number (immutable)
    pub link_count: AtomicU64,  // Concurrent increment
    pub dst_path: PathBuf,  // Destination (set at creation, immutable)
    pub copy_complete: CondVar,  // Wait for copy completion
}
```

**Key changes**:
- `dst_path` is now required at creation (not Option)
- `copy_complete` condition variable signals when safe to hardlink
- No `is_copied` flag - condvar being signaled means it's copied
- Simpler: fewer mutable fields

#### 3. Atomic Registration Pattern

**DashMap try-insert pattern**:
```rust
pub fn register_file(&self, src_path: &Path, dst_path: &Path, dev: u64, ino: u64) -> HardlinkRole {
    let inode_info = InodeInfo { dev, ino };
    
    // Try to atomically insert
    match self.hardlinks.entry(inode_info) {
        Entry::Vacant(entry) => {
            // We won! We're responsible for copying
            entry.insert(HardlinkInfo {
                original_path: src_path.to_path_buf(),
                inode_number: ino,
                link_count: AtomicU64::new(1),
                dst_path: dst_path.to_path_buf(),
                copy_complete: CondVar::new(),
            });
            HardlinkRole::Copier  // Caller should copy file
        }
        Entry::Occupied(entry) => {
            // Already exists - we're a hardlink
            entry.get().link_count.fetch_add(1, Ordering::Relaxed);
            HardlinkRole::Linker(entry.get().clone())  // Caller should wait & link
        }
    }
}

pub enum HardlinkRole {
    Copier,  // This thread should copy the file
    Linker(HardlinkInfo),  // This thread should wait and create hardlink
}
```

#### 4. Updated Hardlink Handling Flow

**In `process_file()`**:
```rust
// Register file and determine role
match tracker.register_file(src, dst, dev, ino, link_count) {
    HardlinkRole::Copier => {
        // We're first - copy the file
        copy_file(src, dst).await?;
        stats.increment_files_copied();
        
        // Signal that copy is complete
        tracker.signal_copy_complete(ino);
    }
    HardlinkRole::Linker(info) => {
        // Another thread is copying - wait for it
        info.copy_complete.wait().await;
        
        // Now safe to create hardlink
        create_hardlink(&info.dst_path, dst).await?;
        stats.increment_files_copied();
    }
}
```

**Key improvement**: No race conditions!
- Only one thread copies
- Other threads wait for completion
- Hardlinks created after file exists

## API Design

### CondVar in compio-sync

**Public API**:
```rust
// crates/compio-sync/src/condvar.rs

use compio::event::Event;
use std::sync::Arc;

/// Async condition variable for notification
///
/// Similar to `tokio::sync::Notify` but using compio primitives.
pub struct CondVar {
    inner: Arc<CondVarInner>,
}

struct CondVarInner {
    // Implementation detail - use compio Event or build on Semaphore
    notified: Semaphore,  // Or Event, depending on compio API
}

impl CondVar {
    /// Create a new condition variable
    pub fn new() -> Self {
        Self {
            inner: Arc::new(CondVarInner {
                notified: Semaphore::new(0),  // Start with 0 permits
            }),
        }
    }
    
    /// Wait for notification
    pub async fn wait(&self) {
        // Wait for a permit (blocks until notify_one/notify_all)
        let _ = self.inner.notified.acquire().await;
    }
    
    /// Notify one waiting task
    pub fn notify_one(&self) {
        self.inner.notified.add_permits(1);
    }
    
    /// Notify all waiting tasks
    pub fn notify_all(&self) {
        // Add "infinite" permits (or track waiter count)
        self.inner.notified.add_permits(usize::MAX);
    }
}

impl Clone for CondVar {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}
```

**Alternative using Event** (if available in compio):
```rust
use compio::event::Event;

struct CondVarInner {
    event: Event,
}

impl CondVar {
    pub async fn wait(&self) {
        self.inner.event.wait().await;
    }
    
    pub fn notify_all(&self) {
        self.inner.event.notify(usize::MAX);
    }
}
```

### FilesystemTracker API Changes

**New enum for registration result**:
```rust
pub enum HardlinkRegistration {
    /// This is the first occurrence - caller should copy the file
    FirstOccurrence,
    
    /// This is a subsequent hardlink - caller should wait and link
    Hardlink {
        /// Destination path to link to
        dst_path: PathBuf,
        /// Condition variable to wait on
        copy_complete: CondVar,
    },
}
```

**Updated `register_file()` signature**:
```rust
// Before: Returns bool (new or duplicate)
pub fn register_file(&self, path: &Path, dev: u64, ino: u64, link_count: u64) -> bool;

// After: Returns role and synchronization info
pub fn register_file(
    &self,
    src_path: &Path,
    dst_path: &Path,
    dev: u64,
    ino: u64,
    link_count: u64,
) -> HardlinkRegistration;
```

**New method to signal completion**:
```rust
pub fn signal_copy_complete(&self, ino: u64) {
    for entry in self.hardlinks.iter() {
        if entry.value().inode_number == ino {
            entry.value().copy_complete.notify_all();
            break;
        }
    }
}
```

## Implementation Details

### File Changes

| File | Changes | Complexity |
|------|---------|------------|
| `crates/compio-sync/src/condvar.rs` | New file: CondVar implementation | Medium |
| `crates/compio-sync/src/lib.rs` | Export CondVar | Trivial |
| `src/directory.rs` | Update HardlinkInfo struct | Medium |
| `src/directory.rs` | Update register_file() API | Medium |
| `src/directory.rs` | Update process_file() to handle roles | High |
| `src/directory.rs` | Remove mark_inode_copied() | Trivial |
| `src/directory.rs` | Remove is_inode_copied() | Trivial |
| `src/directory.rs` | Remove get_original_path_for_inode() | Trivial |

### Dependencies

**No new external dependencies** - Build on existing `compio-sync::Semaphore`.

### Complexity Assessment

**Overall Complexity**: Medium-High

**Breakdown**:
- **New crate module**: CondVar implementation (medium)
- **API changes**: Breaking change to register_file() (medium)
- **Concurrent logic**: Proper wait/signal pattern (high)
- **Testing**: Need concurrent tests for race conditions (high)

**Estimated Phases**: 4 phases

## Detailed Design

### Phase 1: Implement CondVar in compio-sync

**Research needed**:
- Check if compio has `Event` primitive we can use
- Review `tokio::sync::Notify` implementation
- Review our `compio-sync::Semaphore` pattern

**Approach 1 - Using Semaphore** (simpler):
```rust
pub struct CondVar {
    semaphore: Arc<Semaphore>,
}

impl CondVar {
    pub fn new() -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(0)),  // 0 permits initially
        }
    }
    
    pub async fn wait(&self) {
        // Acquire permit - blocks until notify
        let _permit = self.semaphore.acquire().await;
        // Permit dropped immediately
    }
    
    pub fn notify_all(&self) {
        // Add many permits (one per waiter)
        // Semaphore will release all waiting acquire() calls
        self.semaphore.add_permits(1000);  // Or track waiter count
    }
}
```

**Approach 2 - Using Event** (if available):
```rust
pub struct CondVar {
    event: Arc<compio::event::Event>,
}

impl CondVar {
    pub async fn wait(&self) {
        self.event.wait().await;
    }
    
    pub fn notify_all(&self) {
        self.event.notify(usize::MAX);
    }
}
```

### Phase 2: Update HardlinkInfo Structure

**Changes**:
```rust
pub struct HardlinkInfo {
    // Immutable fields
    pub original_path: PathBuf,  // Source path
    pub inode_number: u64,  // Inode
    pub dst_path: PathBuf,  // Destination (set at creation, immutable!)
    
    // Concurrent fields
    pub link_count: AtomicU64,  // Increment atomically
    pub copy_complete: CondVar,  // Wait/signal for copy completion
    
    // Removed:
    // - is_copied: AtomicBool  (replaced by condvar)
    // - dst_path: Mutex<Option<PathBuf>>  (now immutable PathBuf)
}
```

**Benefits**:
- `dst_path` is set once at creation (caller must provide)
- No mutex around `dst_path` (immutable after creation)
- No `is_copied` flag (condvar provides synchronization)
- Simpler Clone impl (no mutex to clone)

### Phase 3: Update register_file() API

**New return type**:
```rust
pub enum HardlinkRegistration {
    /// This thread won the race - copy the file and signal
    FirstOccurrence {
        /// Condition variable to signal after copy
        copy_complete: CondVar,
    },
    
    /// Another thread is copying - wait for it
    Hardlink {
        /// Destination path to link to (after waiting)
        dst_path: PathBuf,
        /// Condition variable to wait on
        copy_complete: CondVar,
    },
}
```

**Implementation**:
```rust
pub fn register_file(
    &self,
    src_path: &Path,
    dst_path: &Path,
    dev: u64,
    ino: u64,
    link_count: u64,
) -> Option<HardlinkRegistration> {
    // Skip files with link count of 1 - they're not hardlinks
    if link_count == 1 {
        return None;  // Not a hardlink, caller should just copy
    }
    
    let inode_info = InodeInfo { dev, ino };
    
    // Atomic try-insert
    match self.hardlinks.entry(inode_info) {
        Entry::Vacant(entry) => {
            // We're first! Create entry with condvar
            let info = HardlinkInfo {
                original_path: src_path.to_path_buf(),
                inode_number: ino,
                link_count: AtomicU64::new(1),
                dst_path: dst_path.to_path_buf(),
                copy_complete: CondVar::new(),
            };
            let condvar = info.copy_complete.clone();
            entry.insert(info);
            
            Some(HardlinkRegistration::FirstOccurrence {
                copy_complete: condvar,
            })
        }
        Entry::Occupied(entry) => {
            // Already exists - we're a subsequent hardlink
            entry.get().link_count.fetch_add(1, Ordering::Relaxed);
            
            Some(HardlinkRegistration::Hardlink {
                dst_path: entry.get().dst_path.clone(),
                copy_complete: entry.get().copy_complete.clone(),
            })
        }
    }
}
```

### Phase 4: Update process_file() Logic

**Before (race-prone)**:
```rust
if link_count > 1 && hardlink_tracker.is_inode_copied(inode) {
    create_hardlink(...).await?;
} else {
    copy_file(...).await?;
    hardlink_tracker.mark_inode_copied(inode, dst);
}
```

**After (race-free)**:
```rust
match hardlink_tracker.register_file(src, dst, dev, ino, link_count) {
    None => {
        // Not a hardlink (link_count == 1) - just copy
        copy_file(src, dst).await?;
        stats.increment_files_copied();
    }
    Some(HardlinkRegistration::FirstOccurrence { copy_complete }) => {
        // We're first - copy the file
        copy_file(src, dst).await?;
        stats.increment_files_copied();
        
        // Signal all waiters that copy is complete
        copy_complete.notify_all();
    }
    Some(HardlinkRegistration::Hardlink { dst_path, copy_complete }) => {
        // Another thread is copying - wait for it
        copy_complete.wait().await;
        
        // Now safe to create hardlink
        create_hardlink(&dst_path, dst).await?;
        stats.increment_files_copied();
    }
}
```

## Testing Strategy

### Unit Tests

**CondVar tests** (`crates/compio-sync/src/condvar.rs`):
```rust
#[compio::test]
async fn test_condvar_basic() {
    let cv = CondVar::new();
    
    // Spawn waiter
    let cv_clone = cv.clone();
    let handle = spawn(async move {
        cv_clone.wait().await;
        42
    });
    
    // Signal
    cv.notify_all();
    
    // Waiter completes
    assert_eq!(handle.await, 42);
}

#[compio::test]
async fn test_condvar_multiple_waiters() {
    let cv = CondVar::new();
    let mut handles = vec![];
    
    // Spawn 10 waiters
    for i in 0..10 {
        let cv_clone = cv.clone();
        handles.push(spawn(async move {
            cv_clone.wait().await;
            i
        }));
    }
    
    // Notify all
    cv.notify_all();
    
    // All complete
    for handle in handles {
        handle.await;
    }
}
```

**Hardlink race tests** (`src/directory.rs`):
```rust
#[compio::test]
async fn test_concurrent_hardlink_no_double_copy() {
    // Create file with 2 hardlinks
    // Spawn 2 concurrent threads to register
    // Verify only 1 copies, the other waits and links
    // Verify no race conditions
}
```

### Integration Tests

- Full directory copy with concurrent hardlink discovery
- Stress test: Many hardlinks discovered simultaneously
- Verify correct file count (no double copies)

## Performance Considerations

### Expected Impact

**Correctness**: 
- **Before**: Race condition - possible double copies or early hardlinks
- **After**: Correct synchronization - exactly one copy per inode

**Performance**:
- **Copy path**: Similar (one extra condvar allocation)
- **Hardlink path**: Adds wait overhead (but prevents race)
- **Memory**: One CondVar per hardlinked inode (negligible)

**Lock contention**:
- **Before**: Mutex on dst_path (though short-lived)
- **After**: No mutex (dst_path is immutable)

### Benchmarks

- Measure directory copy with many hardlinks
- Compare before/after for concurrent access
- Verify no regression in single-threaded case

## Security Considerations

### Threat Model

- No new security concerns
- Fixes potential race that could create incomplete hardlinks
- CondVar is standard synchronization primitive

### Correctness

- Eliminates TOCTOU (time-of-check-time-of-use) race
- Ensures hardlinks only created after file fully written
- Prevents data corruption from incomplete copies

## Error Handling

### Simplified Error Handling

**Removed methods** (no longer needed):
- `is_inode_copied()` - replaced by registration pattern
- `mark_inode_copied()` - signaling done via condvar
- `get_original_path_for_inode()` - dst_path in HardlinkRegistration

**New error cases**:
- CondVar operations are infallible (wait/notify don't error)
- Mutex in Clone only (already handled)

## Migration & Compatibility

### Breaking Changes

**Internal only**:
- `HardlinkInfo` structure changes
- `register_file()` API changes
- Removed methods from `FilesystemTracker`

**No public API changes**:
- `copy_directory()` signature unchanged
- CLI interface unchanged
- Behavior preserved (but race-free!)

### Rollout Plan

#### Phase 1: Implement CondVar in compio-sync
1. Create `crates/compio-sync/src/condvar.rs`
2. Research compio primitives (Event vs Semaphore-based)
3. Implement wait/notify API
4. Add unit tests
5. Export from `compio-sync`

#### Phase 2: Update HardlinkInfo
1. Add atomic fields to HardlinkInfo
2. Replace is_copied with CondVar
3. Make dst_path immutable (set at creation)
4. Implement Clone manually
5. Update tests

#### Phase 3: Update register_file() API
1. Define HardlinkRegistration enum
2. Refactor register_file() to return enum
3. Update all call sites
4. Remove old helper methods

#### Phase 4: Update Hardlink Handling
1. Update process_file() to match on registration result
2. Implement copy-and-signal path
3. Implement wait-and-link path
4. Add concurrent tests
5. Verify no races

## Alternatives Considered

### Alternative 1: Keep AtomicBool, add spin-wait

**Approach**:
```rust
while !is_copied.load(Ordering::Acquire) {
    std::hint::spin_loop();
}
```

**Why not chosen**:
- Busy-waiting wastes CPU
- No proper signaling mechanism
- Still has TOCTOU between check and action

### Alternative 2: Use RwLock on dst_path

**Why not chosen**:
- Still need condvar for "copy complete" signal
- Doesn't solve the race condition
- More complex than needed

### Alternative 3: Single-threaded hardlink registration

**Why not chosen**:
- Defeats purpose of concurrent file processing
- DashMap designed for concurrent access
- Would create bottleneck

### Alternative 4: Optimistic locking (try copy, rollback on conflict)

**Why not chosen**:
- Complex error recovery
- Wasted work on conflicts
- File system side effects hard to rollback

## Open Questions

- [ ] Does compio have an Event primitive we can use?
- [ ] Should CondVar::wait() be cancellable?
- [ ] Should we track waiter count for efficient notify_all()?
- [ ] Should copy_complete be signaled before or after metadata preservation?
- [ ] Do we need timeout on wait() to prevent deadlocks?

## References

- [tokio::sync::Notify](https://docs.rs/tokio/latest/tokio/sync/struct.Notify.html)
- [std::sync::Condvar](https://doc.rust-lang.org/std/sync/struct.Condvar.html)
- [compio event primitives](https://docs.rs/compio/)
- Current code: `src/directory.rs` lines 1209-1376
- Current semaphore: `crates/compio-sync/src/lib.rs`

## Acceptance Criteria

- [ ] CondVar implemented in compio-sync with tests
- [ ] HardlinkInfo uses CondVar for synchronization
- [ ] register_file() returns HardlinkRegistration enum
- [ ] process_file() handles FirstOccurrence vs Hardlink
- [ ] No race conditions in concurrent hardlink tests
- [ ] All existing tests pass
- [ ] Performance not degraded
- [ ] No clippy warnings

## Future Work

- Consider using CondVar elsewhere for async coordination
- Benchmark CondVar overhead vs alternatives
- Add timeout support if deadlocks occur
- Explore using compio's Event if available

---

**Next Steps**:
1. Research compio event/notification primitives
2. Create implementation plan: `/plan`
3. Implement CondVar in compio-sync
4. Update hardlink tracking logic

