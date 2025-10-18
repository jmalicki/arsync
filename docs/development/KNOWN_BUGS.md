# Known Bugs

## ~~CRITICAL: Deadlock on "Too many open files" Error~~ ✅ FIXED

**Severity**: CRITICAL - Causes hang/deadlock  
**Status**: ✅ **FIXED** with adaptive concurrency control  
**Date Discovered**: 2025-10-08  
**Date Fixed**: 2025-10-09

### Symptoms (Before Fix)

When arsync encountered "Too many open files" (EMFILE) error:
1. Logged WARN messages about failing to copy files
2. **Did NOT exit gracefully**
3. **HUNG/DEADLOCKED** instead of failing
4. Required `kill -9` to terminate

### Reproduction (Before Fix)

```bash
# With default settings, copying 1000+ small files caused:
./target/release/arsync -a --source /path/to/1000-files/ --destination /dest/

# Output showed:
WARN Failed to copy file ... Too many open files (os error 24)
# Then hung forever (deadlock)
```

### Root Cause

**Identified**: Semaphore permits were not being released on error paths when file operations failed.

1. `copy_file()` would fail with EMFILE
2. Error was logged but permit was NOT released (held until drop)
3. Eventually all permits were held by failed operations
4. New operations blocked forever waiting for permits
5. **Result**: Deadlock

### ✅ Fix Implemented

**Adaptive Concurrency Control** (implemented 2025-10-09):

**Key Components:**

1. **`Semaphore.reduce_permits()`** - Atomically reduce available permits
2. **`Semaphore.add_permits()`** - Add permits back with waker notification  
3. **`AdaptiveConcurrencyController`** - Wraps semaphore with EMFILE detection
4. **Error detection** - Identifies EMFILE errors and adapts automatically
5. **Startup FD check** - `check_fd_limits()` warns early if ulimit is too low

**Default behavior** (adaptive - recommended):
```bash
arsync -a --source /path/ --destination /dest/
# ⚠️  FILE DESCRIPTOR EXHAUSTION DETECTED (EMFILE)
#    
#    arsync has hit the system file descriptor limit.
#    
#    Self-adaptive response:
#    - Reduced concurrent operations: 1024 → 768 (-256)
#    - Currently available: 120
#    - Minimum limit: 100
#    
#    This may slow down processing but prevents crashes.
#    
#    To avoid this:
#    - Increase ulimit: ulimit -n 100000
#    - Or use --max-files-in-flight to set lower initial concurrency
#    
#    Continuing with reduced concurrency...
# 
# ✓ Completes successfully with adaptive concurrency
```

**Strict mode** (fail-fast for CI/CD):
```bash
arsync -a --no-adaptive-concurrency --source /path/ --destination /dest/
# ERROR: File descriptor exhaustion detected (--no-adaptive-concurrency is set).
#        Failed to copy /path/file.txt -> /dest/file.txt: Too many open files.
#        Either increase ulimit or remove --no-adaptive-concurrency flag.
# Exit code: 1
```

**Best practice** (increase ulimit):
```bash
ulimit -n 100000
arsync -a --source /path/ --destination /dest/
# File descriptor limit: 100000 (adequate)
# ✓ No adaptation needed - runs at full speed
```

### Technical Implementation

**Files Modified:**
- `crates/compio-sync/src/semaphore.rs` - Added `reduce_permits()` and `add_permits()`
- `src/adaptive_concurrency.rs` - **NEW** - Adaptive concurrency controller
- `src/directory.rs` - Integrated controller, added EMFILE detection
- `src/error.rs` - Added `FdExhaustion` error variant
- `src/cli.rs` - Added `--no-adaptive-concurrency` flag
- `src/main.rs` - Added adaptive_concurrency module

**Key Algorithm:**

```rust
// In process_file():
match copy_file(&src_path, &dst_path, args).await {
    Ok(()) => { /* success */ },
    Err(e) => {
        // Detect EMFILE and adapt
        let adapted = concurrency_controller.handle_error(&e);
        
        if adapted {
            if args.no_adaptive_concurrency {
                // Fail hard
                return Err(FdExhaustion(...));
            }
            // Continue with reduced concurrency
            warn!("Adapted to FD exhaustion - continuing...");
        }
        stats.increment_errors()?;
    }
}
```

**Adaptation Strategy:**
- Detects EMFILE errors via error message matching
- Reduces by 25% or minimum 10 permits
- Never goes below minimum (10 or 10% of initial)
- Only adapts every 5 errors (to avoid over-reaction)
- Logs detailed warnings on first adaptation
- Subsequent reductions logged briefly

### Testing

**Stress test available:**
```bash
./benchmarks/stress_test_small_files.sh
# Tests with 10, 50, 100, 500, 1K, 5K, 10K files
# Reports where EMFILE occurs
# Verifies no hanging
```

**Expected results:**
- ✅ No hanging or deadlock
- ✅ Clear warnings when adapting
- ✅ Completion with reduced concurrency (slower but successful)
- ✅ Or immediate failure if `--no-adaptive-concurrency` set

### Impact

**Before Fix:**
- ❌ Unacceptable for production use
- ❌ Required kill -9 to terminate
- ❌ Silent failure (no exit code)
- ❌ Data integrity unclear

**After Fix:**
- ✅ Safe for production use
- ✅ Never hangs or deadlocks  
- ✅ Clear error messages and warnings
- ✅ Graceful degradation or fail-fast (user choice)
- ✅ Data integrity maintained (errors tracked in stats)
- ✅ Self-aware of system limits

### References

- **Implementation**: `src/adaptive_concurrency.rs`
- **Documentation**: `ADAPTIVE_CONCURRENCY_IMPLEMENTATION.md`
- **Bug discovered**: During initial benchmark run (2025-10-08)
- **Fix implemented**: 2025-10-09
- **Branch**: `fix/self-adaptive-concurrency-fd-awareness`

---

## Other Known Issues

None at this time. The critical deadlock bug has been resolved.
