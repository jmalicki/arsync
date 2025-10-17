# Test Running Quick Reference

## Setup (one-time)

```bash
cargo install cargo-nextest --locked
```

## Most Common Commands

```bash
# Fast tests (your daily workflow) - skips slow tests
cargo nextest run -E 'not(test(integration) | test(performance) | test(rsync))'
cargo make test-fast

# Integration tests
cargo nextest run -E 'test(/integration/)'
cargo make test-int

# Performance tests (slow!)
cargo nextest run -E 'test(/performance/)'
cargo make test-perf

# Metadata tests
cargo nextest run -E 'test(/metadata/)'
cargo make test-meta

# XAttr tests
cargo nextest run -E 'test(/xattr/)'
cargo make test-xattr

# Rsync protocol tests
cargo nextest run -E 'test(/rsync/)'
cargo make test-rsync

# Run EVERYTHING
cargo nextest run
cargo make nextest

# List what tests exist
cargo nextest list
cargo make test-list
```

## cargo-make shortcuts

Even simpler - use cargo-make tasks:

```bash
cargo make test-fast    # Fast tests only
cargo make test-int     # Integration tests
cargo make test-perf    # Performance tests
cargo make test-meta    # Metadata tests
cargo make test-xattr   # XAttr tests
cargo make test-rsync   # Rsync tests
cargo make nextest      # All tests
cargo make test-list    # List all tests
cargo make test-ci      # CI mode (with retries)
```

## Shell Aliases (Optional)

Add to `~/.bashrc` or `~/.zshrc`:

```bash
alias atf='cargo nextest run -E "not(test(/integration|performance|rsync/))"'
alias ati='cargo nextest run -E "test(/integration/)"'
alias atp='cargo nextest run -E "test(/performance/)"'
alias ata='cargo nextest run'
alias atl='cargo nextest list'
```

Then just:
```bash
atf   # fast tests
ati   # integration
atp   # performance
ata   # all tests
```

## During Development

**Recommended workflow:**

1. Make code changes
2. Run `cargo make test-fast` or `atf` to get quick feedback
3. Once that passes, run relevant category (e.g., `cargo make test-int`)
4. Before pushing, optionally run `cargo make nextest` for everything

This gives you fast iteration while still ensuring quality.

