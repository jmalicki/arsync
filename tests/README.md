# Test Suite Guidelines

## Test Categories and Running Tests

Tests are organized by category for efficient development workflows. See:
- **[QUICK_REFERENCE.md](QUICK_REFERENCE.md)** - Quick commands cheat sheet
- **[TEST_TAGS.md](TEST_TAGS.md)** - Complete guide to test categories and cargo-nextest

### Quick Start

Install nextest (one-time):
```bash
cargo install cargo-nextest --locked
```

Run fast tests (recommended during development):
```bash
cargo make test-fast
# or: cargo nextest run -E 'not(test(integration)) and not(test(performance)) and not(test(rsync))'
```

Run all tests:
```bash
cargo make nextest
# or: cargo nextest run
```

## Timeouts to prevent hangs

compio-based tests can hang if an async task or I/O submission/completion pipeline stalls. To harden the suite:

- A shared timeout guard is available in `tests/common/mod.rs`.
- Every long-running `#[compio::test]` should start with a guard.

Example:

```rust
#[path = "common/mod.rs"]
mod test_utils;
use test_utils::test_timeout_guard;
use std::time::Duration as StdDuration;

#[compio::test]
async fn my_test() {
    // Abort the process if this test exceeds 120s
    let _timeout = test_timeout_guard(StdDuration::from_secs(120));

    // ... test body ...
}
```

Recommended timeouts:
- Standard tests: 120s
- Heavy performance tests: 240s

## CI timeouts

In CI, configure global test timeouts (e.g., via nextest or your runner) to ensure the suite cannot hang indefinitely. Suggested defaults:
- Global test-timeout: 120s
- Slow-timeout (warning/terminate after): 45–60s

If adopting nextest, add a `.cargo/nextest.toml` with profiles that set `test-timeout` and `slow-timeout`.

## When a test times out

The guard prints a message and aborts the process to surface the problem immediately:
- This makes hung tests fail fast
- Prevents CI from idling

## Scope

- Apply guards to all `#[compio::test]` functions
- Prefer tighter test bodies over large end-to-end flows
- For very large scenarios, split into smaller, faster tests and keep a single long-running test with a larger timeout
