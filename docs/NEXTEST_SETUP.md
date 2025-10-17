# cargo-nextest Setup Complete! 🎉

This document summarizes the test categorization system that's been set up using `cargo-nextest`.

## What's Been Done

✅ Created `.config/nextest.toml` - nextest configuration with:
- Per-category timeout settings
- Thread allocation rules
- CI profile with retries and JUnit output
- Automatic timeout adjustments for slow tests

✅ Added cargo-make tasks to `Makefile.toml`:
- `cargo make test-fast` - Quick tests only
- `cargo make test-int` - Integration tests
- `cargo make test-perf` - Performance tests
- `cargo make test-meta` - Metadata tests
- `cargo make test-xattr` - XAttr tests
- `cargo make test-rsync` - Rsync tests

✅ Created comprehensive documentation:
- `tests/TEST_TAGS.md` - Complete guide to test categories
- `tests/QUICK_REFERENCE.md` - Command cheat sheet
- Updated `tests/README.md` with quick start

## Installation

To use this system, install cargo-nextest (one-time):

```bash
cargo install cargo-nextest --locked
```

Or via your package manager:
```bash
# macOS
brew install cargo-nextest

# Arch Linux
sudo pacman -S cargo-nextest
```

## Basic Usage

### Daily Development Workflow

```bash
# Run fast tests only (recommended during development)
cargo make test-fast

# Or directly with nextest:
cargo nextest run -E 'not(test(/integration|performance|rsync/))'
```

This skips the slow integration, performance, and rsync tests, giving you quick feedback.

### Run Specific Categories

```bash
cargo make test-int      # Integration tests
cargo make test-perf     # Performance tests (slow!)
cargo make test-meta     # Metadata tests
cargo make test-xattr    # XAttr tests
cargo make test-rsync    # Rsync protocol tests
```

### Run Everything

```bash
cargo make nextest
# or: cargo nextest run
```

## How It Works

Tests are filtered by their **file names** and **function names**:

- Files named `*_integration_*.rs` → Integration tests
- Files named `*_performance_*.rs` → Performance tests
- Files matching `*metadata*.rs` → Metadata tests
- Files matching `*xattr*.rs` → XAttr tests
- Files matching `rsync*.rs` → Rsync tests

Functions can also include these keywords in their names:
- `test_integration_*`
- `test_performance_*`
- etc.

### Current Test Files

Your test files automatically categorize as:
```
tests/
├── integration_tests.rs              → integration
├── copy_integration_tests.rs         → integration
├── performance_metadata_tests.rs     → performance
├── comprehensive_metadata_tests.rs   → metadata
├── directory_metadata_tests.rs       → metadata
├── edge_case_metadata_tests.rs       → metadata
├── metadata_flag_tests.rs            → metadata
├── file_xattr_tests.rs              → xattr
├── directory_xattr_tests.rs         → xattr
├── rsync_*.rs                       → rsync (multiple files)
└── ... other tests run by default
```

## Benefits

1. **Fast Iteration**: `cargo make test-fast` runs in seconds instead of minutes
2. **Targeted Testing**: Only run the tests relevant to your changes
3. **Better CI**: Parallel execution, automatic retries, JUnit output
4. **No Code Changes**: Works with all your existing tests
5. **Professional Tool**: Used by Tokio, Clap, and other major Rust projects

## Optional: Shell Aliases

Add these to your `~/.bashrc` or `~/.zshrc`:

```bash
# Test aliases
alias atf='cargo nextest run -E "not(test(/integration|performance|rsync/))"'
alias ati='cargo nextest run -E "test(/integration/)"'
alias atp='cargo nextest run -E "test(/performance/)"'
alias ata='cargo nextest run'
alias atl='cargo nextest list'
```

Then you can just type:
```bash
atf   # Run fast tests
ati   # Run integration tests
ata   # Run all tests
```

## Next Steps

1. **Install nextest**: `cargo install cargo-nextest --locked`
2. **Try it out**: `cargo make test-fast`
3. **Verify categories**: `cargo nextest list -E 'test(/integration/)'`
4. **Update your workflow**: Use `test-fast` during development

## Learn More

- Full documentation: [../tests/TEST_TAGS.md](../tests/TEST_TAGS.md)
- Quick reference: [../tests/QUICK_REFERENCE.md](../tests/QUICK_REFERENCE.md)
- cargo-nextest book: https://nexte.st/

## Comparison: Before vs After

### Before
```bash
cargo test  # Runs ALL tests, takes several minutes
```

### After
```bash
# Quick feedback during development
cargo make test-fast  # Takes seconds

# Run specific categories as needed
cargo make test-int   # Only integration tests
cargo make test-perf  # Only performance tests

# Run everything when ready
cargo make nextest    # Same as before, but faster and better output
```

---

**Questions?** See [../tests/TEST_TAGS.md](../tests/TEST_TAGS.md) for complete documentation!

