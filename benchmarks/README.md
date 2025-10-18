# Syscall Analyzer

End-to-end syscall analysis tool for arsync. Does everything automatically:
- Creates test dataset
- Runs arsync with strace
- Parses syscall output  
- Generates markdown reports

## Building

```bash
cd benchmarks
cargo build --release
```

## Usage

From workspace root:
```bash
./target/release/syscall-analyzer \
  --arsync-bin ./target/release/arsync \
  --num-files 5 \
  --file-size-mb 10 \
  --output /tmp/syscall-analysis-report.md
```

From benchmarks directory:
```bash
../target/release/syscall-analyzer \
  --arsync-bin ../target/release/arsync
```

All options have sensible defaults. Run `--help` for full documentation.

## Features

- **Automated test dataset creation** - no manual setup needed
- **Runs strace automatically** - captures all syscalls
- **Comprehensive analysis:**
  - io_uring usage and batching efficiency
  - Metadata operation patterns (statx, openat, etc.)
  - Security assessment (TOCTOU-safe FD-based operations)
  - Per-file and per-directory syscall breakdowns
- **Markdown reports** - ready for GitHub display
- **Smart exit codes:** 0 (success/warnings), 2 (critical failures)

## CI Integration

Used directly in GitHub Actions - no shell script wrapper needed.

## Note

This is a **development tool** for benchmarking and CI, not part of the shipped arsync binary.
