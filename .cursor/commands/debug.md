# /debug

Run a disciplined debugging loop to diagnose and fix issues systematically.

- files (string, optional): File(s) to focus on, can use @-mentions
- issue (string, optional): Description of the issue if not clear from context

```bash
/debug
/debug @src/copy.rs "buffer overflow on large files"
/debug "test_metadata_preservation fails intermittently"
```

## Context Inference

The command automatically analyzes:

1. **Current conversation** - What problem is being discussed?
2. **Error messages** - Compiler errors, test failures, panics
3. **Open files** - What code is being examined?
4. **Git diff** - Recent changes that might have caused issue
5. **Test output** - Failing test details
6. **Logs** - Runtime errors or debug output

If context is unclear, ask for:
- Specific error message
- Steps to reproduce
- Expected vs actual behavior

## Debugging Process

The command follows a disciplined, iterative approach:

### 1. Identify & Capture
- **Single failing scenario** - Focus on ONE issue at a time
- **Exact error message** - Copy the full error, including backtrace
- **Reproduction steps** - Minimal steps to trigger the issue
- **Current state** - What changed? What works/doesn't work?

### 2. Reproduce Locally
```bash
# Try to reproduce with minimal example
cargo test failing_test_name
cargo run -- [args that trigger issue]
RUST_BACKTRACE=1 cargo test failing_test_name

# For benchmarks
cargo bench failing_benchmark

# For integration issues
./benchmarks/smoke_test.sh
```

### 3. Form Hypothesis
- **What could cause this?** - Based on error and code analysis
- **Where's the bug likely located?** - Specific file/function
- **Why did it start failing?** - Recent change, environment, race condition?

### 4. Improve Observability (If Cause Unclear)

**Before jumping to fixes, make the code more debuggable:**

#### Add Debug Instrumentation (TO BE COMMITTED)
```rust
// Add debug logging behind a feature flag or env var
#[cfg(feature = "debug-io")]
eprintln!("[DEBUG] Buffer state: len={}, pos={}, cap={}", buf.len(), pos, cap);

// Or use conditional compilation
if cfg!(debug_assertions) {
    log::debug!("Processing file: {:?}, size: {}", path, size);
}

// Or use environment variable
if std::env::var("ARSYNC_DEBUG").is_ok() {
    eprintln!("[ARSYNC_DEBUG] Current state: {:?}", state);
}
```

#### Add Fine-Grained Unit Tests
```rust
// Break down large function into testable pieces
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_position_calculation() {
        // Test the specific calculation that might be wrong
        assert_eq!(calculate_position(1024, 512), 512);
    }

    #[test]
    fn test_edge_case_empty_buffer() {
        // Test edge case
        let result = process_buffer(&[]);
        assert!(result.is_ok());
    }
}
```

#### Make Code More Testable
```rust
// BEFORE: Hard to test
async fn copy_file_internal(src: &Path, dst: &Path) -> Result<()> {
    // Direct file I/O, hard to mock
    let data = tokio::fs::read(src).await?;
    tokio::fs::write(dst, data).await?;
    Ok(())
}

// AFTER: Testable with dependency injection
async fn copy_file_internal<R: AsyncRead, W: AsyncWrite>(
    src: &mut R,
    dst: &mut W,
) -> Result<()> {
    // Can now test with in-memory buffers
    tokio::io::copy(src, dst).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn test_copy_with_mock_io() {
        let mut src = &b"test data"[..];
        let mut dst = Vec::new();
        copy_file_internal(&mut src, &mut dst).await.unwrap();
        assert_eq!(dst, b"test data");
    }
}
```

#### Add Assertions (To Be Kept)
```rust
// Add invariant checks that stay in the code
debug_assert!(position <= buffer.len(), 
    "Position {} exceeds buffer length {}", position, buffer.len());

// For critical invariants, use runtime checks
if position > buffer.len() {
    return Err(Error::InvalidState {
        msg: format!("Position {} exceeds buffer length {}", position, buffer.len()),
    });
}
```

#### Add Tracing/Instrumentation
```rust
use tracing::{debug, info, warn, instrument};

#[instrument(skip(buffer), fields(buffer_len = buffer.len()))]
async fn process_buffer(buffer: &[u8]) -> Result<usize> {
    debug!("Processing buffer");
    let result = do_work(buffer).await?;
    debug!(bytes_processed = result, "Buffer processed");
    Ok(result)
}
```

**This step is crucial:**
- Don't guess at fixes without visibility
- Instrumentation helps with current AND future bugs
- Unit tests prevent regression
- Making code testable often reveals the bug

**Commit these improvements:**
```bash
# These changes get committed, they're not temporary
/commit "debug(module): add instrumentation and tests for issue diagnosis"
```

### 5. Make ONE Targeted Change
- **Single, focused change** - Don't change multiple things
- **Test the hypothesis** - Does this fix align with the theory?
- **Keep changes minimal** - Easy to understand and revert
- **Now informed by better visibility** - Use the debug output to guide the fix

### 6. Verify Fix
```bash
# Re-run the failing scenario
cargo test failing_test_name

# Run related tests
cargo test module_name

# Check for regressions
/smoke
/test "all"

# Format and lint
/fmt false true
/clippy false false
```

### 7. Iterate or Complete
- **If fixed**: Write/strengthen tests, commit, document
- **If not fixed**: 
  - Revert fix attempt
  - Keep observability improvements
  - Form new hypothesis based on debug output
  - Try again with better information
- **Never bypass checks** - All fixes must pass tests and lints
- **Never claim success without proof** - Prove it works

## Debugging Guidelines

### DO:
- ✅ Focus on ONE issue at a time
- ✅ Capture exact error messages and backtraces
- ✅ **Add observability BEFORE guessing at fixes**
- ✅ Write fine-grained unit tests for unclear behavior
- ✅ Add debug logging behind feature flags (commit these!)
- ✅ Make code more testable (refactor for dependency injection)
- ✅ Add assertions for invariants (keep these!)
- ✅ Make minimal, targeted changes
- ✅ Test after each change
- ✅ Use `RUST_BACKTRACE=1` for panics
- ✅ Write regression tests after fixing
- ✅ Document root cause in commit message
- ✅ Check for similar issues elsewhere

### DON'T:
- ❌ Try to fix multiple issues simultaneously
- ❌ Make large refactoring changes while debugging
- ❌ Skip reproduction step
- ❌ **Jump to fixes without understanding the problem**
- ❌ **Remove debug instrumentation that helps future debugging**
- ❌ Guess without testing hypothesis
- ❌ Bypass lints or tests
- ❌ Use temporary debug prints instead of proper logging
- ❌ Claim it's fixed without verification

## Common Scenarios

### Compiler Error
```rust
error[E0502]: cannot borrow `data` as mutable because it is also borrowed as immutable
  --> src/copy.rs:145:5
```

**Debug Process:**
1. Understand borrow checker error
2. Identify conflicting borrows
3. Restructure code to satisfy borrow rules
4. Verify with `cargo check`

### Test Failure
```bash
thread 'test_copy_large_file' panicked at 'assertion failed: copied_size == expected_size'
```

**Debug Process:**
1. Run test with `RUST_BACKTRACE=1`
2. Add debug output to see actual values
3. Identify why sizes differ
4. Fix logic, verify test passes
5. Check for edge cases

### Intermittent Failure (Race Condition)
```bash
test test_metadata_preservation ... FAILED (sometimes)
```

**Debug Process:**
1. Run test repeatedly: `cargo test test_metadata_preservation -- --nocapture --test-threads=1`
2. Add synchronization logging
3. Identify race condition
4. Add proper synchronization (mutex, channel, etc.)
5. Verify with stress testing

### Performance Regression
```bash
Benchmark shows 30% slowdown
```

**Debug Process:**
1. Run benchmarks: `/bench true false`
2. Profile with `perf` or `cargo flamegraph`
3. Identify hot path changes
4. Optimize or revert changes
5. Verify performance restored

### Memory Leak
```bash
Memory usage keeps growing
```

**Debug Process:**
1. Use `valgrind` or `heaptrack`
2. Identify allocation without corresponding free
3. Check for reference cycles (Arc<Mutex<...>>)
4. Add proper cleanup
5. Verify with long-running test

### IO Error
```bash
Os { code: 24, kind: Uncategorized, message: "Too many open files" }
```

**Debug Process:**
1. Check file descriptor usage: `lsof -p <pid>`
2. Identify files not being closed
3. Add proper cleanup (Drop impl, defer, etc.)
4. Verify fix with `ulimit -n 256` stress test

## Debugging Tools

### Rust-Specific Tools
```bash
# Backtraces
RUST_BACKTRACE=1 cargo test
RUST_BACKTRACE=full cargo test

# Logging
RUST_LOG=debug cargo run
RUST_LOG=trace,tokio=info cargo run

# Sanitizers
RUSTFLAGS="-Z sanitizer=address" cargo +nightly test
RUSTFLAGS="-Z sanitizer=thread" cargo +nightly test

# Miri (undefined behavior detection)
cargo +nightly miri test

# Profiling
cargo flamegraph --bin arsync
perf record cargo bench
cargo bench --bench benchmark_name
```

### Testing Tools
```bash
# Run specific test
cargo test test_name

# Run with output
cargo test test_name -- --nocapture

# Run single-threaded (for debugging)
cargo test -- --test-threads=1

# Run ignored tests
cargo test -- --ignored

# Stress test
for i in {1..100}; do cargo test test_name || break; done
```

### Debugging Prints
```rust
// Temporary debugging (remove before commit)
dbg!(&variable);
eprintln!("Debug: value = {:?}", value);

// Permanent logging (use log crate)
use log::{debug, info, warn, error};
debug!("Processing file: {:?}", path);
```

## Example Debugging Sessions

### Example 1: Test Failure (with Observability First)
```bash
# User: "test_large_file_copy is failing"
/debug "test_large_file_copy fails with size mismatch"

# Agent process:
1. Run: `cargo test test_large_file_copy -- --nocapture`
2. Error: "assertion failed: `(left == right)` left: `1048576`, right: `1048577`"
3. Hypothesis: Off-by-one error in buffer handling
4. **Add observability (TO COMMIT):**
   ```rust
   #[cfg(feature = "debug-io")]
   eprintln!("[DEBUG] buffer_len={}, bytes_written={}, total={}", 
             buffer.len(), bytes_written, total_written);
   
   // Add unit test for edge case
   #[test]
   fn test_final_buffer_write() {
       let result = write_buffer(&vec![0u8; 1025], 1024);
       assert_eq!(result.unwrap(), 1025);
   }
   ```
5. Commit observability: `git commit -m "debug(copy): add instrumentation for buffer write tracking"`
6. Run with debug: `RUSTFLAGS="--cfg debug-io" cargo test test_large_file_copy`
7. Observe: Last buffer write only writes 1024 of 1025 bytes
8. Identify: Loop terminates early on final partial buffer
9. Fix: Ensure loop processes all bytes in buffer
10. Verify: `cargo test test_large_file_copy` - PASSES
11. Run full tests: `cargo test`
12. Commit fix: "fix(copy): ensure complete buffer writes including partial final buffer"
```

### Example 2: Performance Issue
```bash
# User: "Sync is slow after recent changes"
/debug @src/sync.rs "performance regression"

# Agent process:
1. Run benchmarks: `/bench true false`
2. Compare with baseline: 40% slower
3. Check recent changes: `git diff main...HEAD src/sync.rs`
4. Hypothesis: New validation check in hot path
5. Profile: `cargo flamegraph --bin arsync -- SOURCE DEST`
6. Confirm: 30% time in new validation
7. Optimization: Cache validation result
8. Verify: `/bench true false` - back to baseline
9. Test: `cargo test`
10. Commit: "perf(sync): cache validation to avoid hot path overhead"
```

### Example 3: Race Condition (with Better Instrumentation)
```bash
# User: "Metadata test fails intermittently"
/debug "test_concurrent_metadata flaky"

# Agent process:
1. Stress test: `for i in {1..100}; do cargo test test_concurrent_metadata || break; done`
2. Fails on iteration 23
3. Hypothesis: Race between metadata read and write
4. **Add observability (TO COMMIT):**
   ```rust
   use tracing::{debug, instrument};
   
   #[instrument(skip(metadata), fields(path = ?path))]
   async fn write_metadata(path: &Path, metadata: &Metadata) -> Result<()> {
       debug!("Starting metadata write");
       // ... implementation
       debug!("Completed metadata write");
       Ok(())
   }
   
   // Add test with better isolation
   #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
   async fn test_concurrent_metadata_isolated() {
       // More controlled test
   }
   ```
5. Commit: `git commit -m "debug(metadata): add tracing for concurrent operations"`
6. Run with tracing: `RUST_LOG=debug cargo test test_concurrent_metadata`
7. Observe: Write completion log appears AFTER read start log
8. Identify: Missing await on metadata write future
9. Fix: Add proper async synchronization
10. Stress test with tracing: 1000 iterations, all pass, logs show correct ordering
11. Test: `cargo test`
12. Commit: "fix(metadata): await metadata write to prevent race condition"
```

## Progress Reporting

During debugging, report progress:

```markdown
## Debugging: [Issue Description]

**Status**: Investigating | Hypothesis Formed | Testing Fix | Resolved

### Problem
- Error: [exact error message]
- Reproduction: [steps]
- Expected: [expected behavior]
- Actual: [actual behavior]

### Investigation
- [x] Reproduced locally
- [x] Analyzed error message
- [x] Identified potential cause
- [ ] Formed hypothesis
- [ ] Tested fix

### Hypothesis
[Current theory about the root cause]

### Changes Attempted
1. Attempt 1: [description] - Result: [failed/worked]
2. Attempt 2: [description] - Result: [failed/worked]

### Solution
[What fixed it and why]

### Verification
- [x] Original test passes
- [x] Related tests pass
- [x] Added regression test
- [x] All quality checks pass
```

## Integration with Other Commands

Use debugging alongside:
- `/test "test_name"` - Run specific tests
- `/fmt false true` - Format after changes
- `/clippy false false` - Check for issues
- `/bench true false` - Performance verification
- `/smoke` - Quick sanity check
- `/review` - Review all debugging changes

## Completion

When issue is resolved:

1. **Verify fix** - All tests pass, issue doesn't recur
2. **Add regression test** - Prevent issue from returning
3. **Clean up** - Remove debug prints, temporary code
4. **Document** - Commit message explains root cause and fix
5. **Check related code** - Look for similar issues

```bash
# After successful debug
/test "all"              # All tests pass
/smoke                   # Smoke tests pass
/fmt false true          # Format code
/clippy false false      # No warnings
/commit "fix(module): resolve issue with detailed explanation"
```

## Notes

- Debugging is iterative - don't expect first attempt to work
- Systematic approach beats random changes
- Always verify fixes don't introduce regressions
- Document lessons learned
- Race conditions may need many iterations to catch
- Performance issues need profiling, not guessing

