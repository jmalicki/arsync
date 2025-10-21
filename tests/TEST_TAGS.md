# Test Organization with cargo-nextest

This document explains how to run and organize tests using `cargo-nextest` in the arsync project.

## What is cargo-nextest?

cargo-nextest is a next-generation test runner for Rust that provides:
- **Fast parallel test execution**
- **Powerful test filtering** (our "tags" system)
- **Better test output**
- **Flaky test detection and retries**
- **JUnit XML output for CI**
- **Per-test timeouts**

## Installation

```bash
cargo install cargo-nextest --locked
```

Or via package manager:
```bash
# macOS
brew install cargo-nextest

# Arch Linux
pacman -S cargo-nextest
```

## Test Categories

Tests are organized by naming convention and can be filtered using expressions:

| Category | Filter Expression | Description |
|----------|------------------|-------------|
| **Fast unit tests** | `not(test(/integration\|performance\|rsync\|docker/))` | Quick tests, run by default |
| **Integration tests** | `test(/integration/)` | File operation integration tests |
| **Performance tests** | `test(/performance/)` | Stress and performance tests (slow) |
| **Metadata tests** | `test(/metadata/)` | Metadata preservation tests |
| **XAttr tests** | `test(/xattr/)` | Extended attribute tests |
| **Rsync tests** | `test(/rsync/)` | Rsync protocol compatibility tests |
| **Docker tests** | `test(/docker/)` | Privileged tests requiring Docker (containers) |

## Running Tests

### Quick feedback - run fast tests only
```bash
# Default: skips integration, performance, rsync (the slow tests)
cargo nextest run -E '!test(integration) & !test(performance) & !test(rsync)'
```

### Run specific categories
```bash
# Integration tests only
cargo nextest run -E 'test(/integration/)'

# Performance tests only  
cargo nextest run -E 'test(/performance/)'

# Metadata tests
cargo nextest run -E 'test(/metadata/)'

# XAttr tests
cargo nextest run -E 'test(/xattr/)'

# Rsync tests
cargo nextest run -E 'test(/rsync/)'

# Docker tests (requires Docker daemon)
cargo nextest run -E 'test(/docker/)'
# or: cargo make test-docker
```

### Combine filters
```bash
# Integration AND metadata tests
cargo nextest run -E 'test(/integration/) and test(/metadata/)'

# Metadata OR xattr tests
cargo nextest run -E 'test(/metadata/) or test(/xattr/)'

# All tests EXCEPT performance tests
cargo nextest run -E 'not(test(/performance/))'

# Run everything except slow tests
cargo nextest run -E 'not(test(/performance|rsync/))'
```

### Run ALL tests
```bash
cargo nextest run
# or
cargo nextest run -E 'all()'
```

### Run specific test by name
```bash
# Any test with "copy" in the name
cargo nextest run -E 'test(copy)'

# Exact test name
cargo nextest run -E 'test(=test_copy_file_basic)'
```

## Naming Convention for Tests

To make filtering work, use consistent naming:

```rust
// Integration test - include "integration" in the function name
#[compio::test]
async fn test_integration_copy_file() { }

// Or in the test file name: copy_integration_tests.rs
#[compio::test]  
async fn test_copy_file() { }  // File name provides "integration"

// Performance test
#[compio::test]
async fn test_performance_1000_files() { }

// Metadata test  
#[test]
fn test_metadata_preservation() { }

// XAttr test
#[test]
fn test_xattr_copy() { }

// Rsync test
#[test]
fn test_rsync_protocol_handshake() { }
```

## Configuration

The project includes `.config/nextest.toml` which defines:
- Test timeouts (performance tests get longer timeouts)
- Thread allocation (integration tests run with fewer parallel threads)
- Retry policies for flaky tests
- Output formatting

## Useful Commands

### See what tests would run
```bash
# List integration tests
cargo nextest list -E 'test(/integration/)'

# Count tests by category
cargo nextest list -E 'test(/integration/)' | wc -l
cargo nextest list -E 'test(/performance/)' | wc -l
```

### Run with more verbose output
```bash
cargo nextest run -E 'test(/integration/)' --no-capture
```

### Run tests serially (no parallelism)
```bash
cargo nextest run --test-threads=1
```

### Run with retries for flaky tests
```bash
cargo nextest run --retries 3
```

### Generate JUnit XML for CI
```bash
cargo nextest run --profile ci
# Output: target/nextest/junit.xml
```

## CI Integration

Example GitHub Actions workflow:

```yaml
name: Tests

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      
      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable
      
      - name: Install nextest
        uses: taiki-e/install-action@nextest
      
      - name: Run fast tests
        run: cargo nextest run -E '!test(integration) & !test(performance) & !test(rsync)'
      
      - name: Run integration tests
        run: cargo nextest run -E 'test(/integration/)'
      
      - name: Run all tests (slow)
        run: cargo nextest run --profile ci
        if: github.event_name == 'push' && github.ref == 'refs/heads/main'
      
      - name: Upload test results
        uses: actions/upload-artifact@v4
        if: always()
        with:
          name: test-results
          path: target/nextest/junit.xml
```

## Comparison to cargo test

| Feature | cargo test | cargo-nextest |
|---------|------------|---------------|
| Parallel execution | Per-file | Per-test |
| Test filtering | Basic patterns | Powerful expressions |
| Flaky test retries | No | Yes |
| Per-test timeouts | No | Yes |
| JUnit output | No | Yes |
| Test isolation | Process reuse | Process-per-test |
| Speed | Good | Better |

## Shell Aliases

Add these to your `~/.bashrc` or `~/.zshrc` for convenience:

```bash
# Fast tests (default development workflow)
alias atf='cargo nextest run -E "not(test(/integration|performance|rsync/))"'

# Integration tests
alias ati='cargo nextest run -E "test(/integration/)"'

# Performance tests
alias atp='cargo nextest run -E "test(/performance/)"'

# All tests
alias ata='cargo nextest run'

# List all tests
alias atl='cargo nextest list'
```

Then use:
```bash
atf   # Run fast tests
ati   # Run integration tests  
atp   # Run performance tests
ata   # Run all tests
```

## Filter Expression Cheat Sheet

```bash
# Exact match
-E 'test(=test_copy_file)'

# Contains
-E 'test(copy)'

# Regex
-E 'test(/^test_copy_/)'

# Boolean logic
-E 'test(copy) and not(test(integration))'
-E 'test(metadata) or test(xattr)'

# Package/binary filtering
-E 'package(arsync) and test(/integration/)'

# Kind filtering
-E 'kind(test)'  # Only #[test] functions
-E 'kind(bench)' # Only benchmarks
```

## Test Organization Guidelines

### File naming
- `*_integration_tests.rs` - Integration tests
- `*_performance_tests.rs` - Performance tests  
- `*_metadata_tests.rs` - Metadata tests
- `*_xattr_tests.rs` - XAttr tests
- `rsync_*.rs` - Rsync protocol tests

### Function naming
Use descriptive names that include the category:
- `test_integration_*`
- `test_performance_*`
- `test_metadata_*`
- `test_xattr_*`
- `test_rsync_*`

This makes filtering intuitive and self-documenting.

## Migration from cargo test

cargo-nextest is compatible with `cargo test` - you don't need to change any test code:

```bash
# Before
cargo test

# After  
cargo nextest run
```

All your existing tests work immediately. The filter expressions are the only new feature.

## Troubleshooting

### nextest not finding tests
```bash
# Make sure you've built tests first
cargo build --tests
cargo nextest list
```

### Tests timing out
Check `.config/nextest.toml` and adjust timeout values if needed.

### Flaky tests
Use retries:
```bash
cargo nextest run --retries 3 -E 'test(flaky_test_name)'
```

## Benefits

1. **Fast development workflow**: Run only fast tests with `atf` alias
2. **Powerful filtering**: Target exactly the tests you want
3. **Better CI**: Parallel execution, retries, JUnit output
4. **No test code changes**: Works with existing tests
5. **Professional standard**: Used by Tokio, Clap, and other major projects

## Learn More

- [cargo-nextest book](https://nexte.st/)
- [Filter expressions](https://nexte.st/book/filter-expressions.html)
- [Configuration](https://nexte.st/book/configuration.html)
