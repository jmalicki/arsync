# Adaptive Concurrency Implementation Plan

**Branch**: `fix/self-adaptive-concurrency-fd-awareness`

## Problem Statement

arsync deadlocks when hitting "Too many open files" (EMFILE):
- Logs warnings about file open failures
- Doesn't exit or adapt
- Hangs forever (requires kill -9)
- CRITICAL bug blocking production use

## Solution: Self-Adaptive Concurrency

Make arsync **resource-aware** and **never hang**.

---

## Implementation Status

### ✅ Completed

1. **Semaphore dynamic permits** (`crates/compio-sync/src/semaphore.rs`)
   - Added `reduce_permits()` - atomically reduce available permits
   - Added `add_permits()` - add permits back and wake waiters
   - Both thread-safe with atomic operations

2. **SharedSemaphore wrapper** (`src/directory.rs`)
   - Exposed `available_permits()`, `max_permits()`
   - Exposed `reduce_permits()`, `add_permits()`

3. **Adaptive controller** (`src/adaptive_concurrency.rs`)
   - `AdaptiveConcurrencyController` - wraps semaphore
   - `handle_error()` - detects EMFILE and adapts
   - `check_fd_limits()` - startup warning if ulimit low
   - Self-reduces by 25% when EMFILE detected
   - Comprehensive warning messages

4. **Bug documentation** (`KNOWN_BUGS.md`)
   - Documented deadlock bug
   - Reproduction steps
   - Root cause hypothesis

5. **Stress test** (`benchmarks/stress_test_small_files.sh`)
   - Tests escalating file counts (10, 50, 100, 500, 1K, 5K, 10K)
   - Finds breaking point
   - Reports where EMFILE occurs

---

## 🚧 TODO: Integration Steps

### Step 1: Replace SharedSemaphore with AdaptiveController

**File**: `src/directory.rs`

```rust
// Change from:
let semaphore = SharedSemaphore::new(max_files_in_flight);

// To:
use crate::adaptive_concurrency::AdaptiveConcurrencyController;
let concurrency_controller = Arc::new(AdaptiveConcurrencyController::new(max_files_in_flight));
```

### Step 2: Add EMFILE Detection to File Operations

**File**: `src/directory.rs` (process_file function, line 970-986)

```rust
// Current code:
match copy_file(&src_path, &dst_path, args).await {
    Ok(()) => {
        stats.increment_files_copied()?;
        // ...
    }
    Err(e) => {
        warn!("Failed to copy file {} -> {}: {}", src_path.display(), dst_path.display(), e);
        stats.increment_errors()?;
    }
}

// Replace with:
match copy_file(&src_path, &dst_path, args).await {
    Ok(()) => {
        stats.increment_files_copied()?;
        // ...
    }
    Err(e) => {
        // Check if this is EMFILE and adapt
        if concurrency_controller.handle_error(&e) {
            warn!("Adapted to FD exhaustion - continuing with reduced concurrency");
        } else {
            warn!("Failed to copy file {} -> {}: {}", src_path.display(), dst_path.display(), e);
        }
        stats.increment_errors()?;
    }
}
```

### Step 3: Add Startup FD Limit Check

**File**: `src/main.rs` or `src/directory.rs` (in traverse_directory function)

```rust
use crate::adaptive_concurrency::check_fd_limits;

// At start of traverse_directory:
if let Ok(fd_limit) = check_fd_limits() {
    if fd_limit < max_files_in_flight as u64 {
        warn!(
            "FD limit ({}) is less than --max-files-in-flight ({}). \
             Consider: ulimit -n {}",
            fd_limit,
            max_files_in_flight,
            max_files_in_flight * 2
        );
    }
}
```

### Step 4: Pass Controller Through Call Chain

Need to thread `AdaptiveConcurrencyController` through:
- `traverse_directory` → `process_directory_entry_with_compio` → `process_file`

**Signature changes**:
```rust
async fn process_file(
    // ... existing params ...
    concurrency_controller: Arc<AdaptiveConcurrencyController>,  // ADD THIS
) -> Result<()> {
    // ... use controller.handle_error() on failures
}
```

### Step 5: Improve Error Detection in copy_file

**File**: `src/copy.rs` (copy_read_write function)

```rust
// When opening files fails:
let src_file = OpenOptions::new().read(true).open(src).await.map_err(|e| {
    if e.kind() == ErrorKind::Other && e.raw_os_error() == Some(libc::EMFILE) {
        SyncError::FdExhaustion(format!(
            "File descriptor exhaustion opening source file {}: {}",
            src.display(),
            e
        ))
    } else {
        SyncError::FileSystem(format!("Failed to open source file {}: {e}", src.display()))
    }
})?;
```

Need to add to `src/error.rs`:
```rust
pub enum SyncError {
    // ... existing variants ...
    /// File descriptor exhaustion (EMFILE)
    FdExhaustion(String),
}
```

---

## Testing Strategy

### 1. Unit Tests

Add to `crates/compio-sync/src/semaphore.rs`:
```rust
#[test]
fn test_reduce_permits() {
    let sem = Semaphore::new(100);
    let reduced = sem.reduce_permits(20);
    assert_eq!(reduced, 20);
    assert_eq!(sem.available_permits(), 80);
}

#[test]
fn test_add_permits() {
    let sem = Semaphore::new(100);
    sem.reduce_permits(30);
    sem.add_permits(15);
    assert_eq!(sem.available_permits(), 85);
}
```

### 2. Integration Test

Run the stress test:
```bash
./benchmarks/stress_test_small_files.sh
```

Should see:
- Warning when hitting EMFILE
- Auto-reduction of concurrency
- Continuation with reduced permits
- NO HANGING

### 3. Benchmark Test

Remove `--max-files-in-flight 100` from benchmarks and let it auto-adapt:
```bash
arsync -a --source /large/dataset --destination /dest
# Should auto-adapt if FD limit hit
```

---

## Expected Behavior After Fix

### Before (Current - BROKEN):
```
WARN Failed to copy: Too many open files
WARN Failed to copy: Too many open files
WARN Failed to copy: Too many open files
[hangs forever - DEADLOCK]
```

### After (Fixed):
```
WARN Failed to copy: Too many open files
WARN ⚠️  FILE DESCRIPTOR EXHAUSTION DETECTED
     Self-adaptive response:
     - Reduced concurrent operations: 1024 → 768 (-256)
     Continuing with reduced concurrency...
✓ Completed successfully with adaptive concurrency
```

---

## Files Modified

- ✅ `crates/compio-sync/src/semaphore.rs` - add/reduce permits
- ✅ `src/directory.rs` - expose semaphore methods
- ✅ `src/adaptive_concurrency.rs` - NEW adaptive controller
- ✅ `src/lib.rs` - export new module
- ⏳ `src/error.rs` - add FdExhaustion variant
- ⏳ `src/directory.rs` - integrate controller
- ⏳ `src/copy.rs` - detect EMFILE
- ⏳ `src/main.rs` - startup FD check

---

## Next Steps

1. Add FdExhaustion error variant
2. Thread AdaptiveController through function calls
3. Add EMFILE detection in file open operations
4. Add startup ulimit check
5. Test with stress_test_small_files.sh
6. Verify NO HANGING occurs
7. Update KNOWN_BUGS.md to mark as FIXED

---

## Success Criteria

✅ **Fix successful if**:
1. No hanging/deadlock when hitting EMFILE
2. Clear warnings when FD exhaustion detected
3. Automatic concurrency reduction
4. Operation completes (possibly slower, but completes)
5. stress_test_small_files.sh passes all tests

❌ **Still broken if**:
1. Still hangs on EMFILE
2. Exits without warning
3. Crashes instead of adapting

---

This is the RIGHT way to handle resource constraints: detect, warn, adapt, continue.

Never hang. Never crash. Always fail gracefully or adapt.

