# Syscall Analyzer

Standalone Rust tool for analyzing arsync syscall patterns via strace.

## Building

```bash
cd benchmarks
cargo build --release
```

The binary will be at `target/release/syscall-analyzer`.

## Usage

```bash
syscall-analyzer \
  --trace-raw /tmp/syscall-analysis-raw.txt \
  --trace-summary /tmp/syscall-analysis-summary.txt \
  --output /tmp/syscall-analysis-report.md \
  --num-files 5 \
  --file-size-mb 10 \
  --binary ./target/release/arsync \
  --test-dir-src /tmp/test-src \
  --test-dir-dst /tmp/test-dst
```

## Features

- Parses strace output for arsync performance metrics
- Generates markdown reports with:
  - io_uring usage and batching efficiency
  - Metadata operation analysis (statx, openat, etc.)
  - Security assessment (TOCTOU-safe FD-based operations)
  - Per-file and per-directory syscall breakdowns
- Exit codes: 0 (success), 1 (warnings), 2 (critical failures)

## Integration

Used by `syscall_analysis.sh` for automated CI analysis.

## Note

This is a **development tool** for benchmarking and CI, not part of the shipped arsync binary.
