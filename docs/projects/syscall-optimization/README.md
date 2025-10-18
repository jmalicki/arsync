# Syscall Optimization Project

**Goal:** Minimize syscall overhead and eliminate TOCTOU vulnerabilities through DirectoryFd-based operations and io_uring integration.

## Quick Summary

**Achievement:** 80% reduction in statx calls + 100% FD-based metadata operations

| Metric | Before | After | Improvement |
|--------|--------|-------|-------------|
| statx per file | 5.2 | 1.0 | -80% |
| Path-based statx | 13 | 0 | -100% |
| Security score | 80/100 | 95/100 | +19% |

**PR:** [#84](https://github.com/jmalicki/arsync/pull/84)

## Documents in This Project

### Implementation & Design
1. **[DIRFD_IO_URING_ARCHITECTURE.md](DIRFD_IO_URING_ARCHITECTURE.md)** - Architecture plan for Phase 2
2. **[SYSCALL_OPTIMIZATION_PROGRESS.md](SYSCALL_OPTIMIZATION_PROGRESS.md)** - Progress tracking

### Analysis & Results
3. **[SYSCALL_BREAKDOWN_PHASE2.md](SYSCALL_BREAKDOWN_PHASE2.md)** - Detailed syscall breakdown with real data
4. **[LOGICAL_SYSCALL_ANALYSIS.md](LOGICAL_SYSCALL_ANALYSIS.md)** - Including io_uring operations
5. **[SYSCALL_TRACE_ANALYSIS.md](SYSCALL_TRACE_ANALYSIS.md)** - Initial trace analysis
6. **[METADATA_SYSCALL_VERIFICATION.md](METADATA_SYSCALL_VERIFICATION.md)** - Metadata preservation verification

### Tooling & Techniques
7. **[SYSCALL_TRACE_FILTERING.md](SYSCALL_TRACE_FILTERING.md)** - How to filter strace output
8. **[SESSION_2025-10-18_SYSCALL_OPTIMIZATION.md](SESSION_2025-10-18_SYSCALL_OPTIMIZATION.md)** - Session log

### Benchmarking
9. **[LARGE_SCALE_BENCHMARK_PLAN.md](LARGE_SCALE_BENCHMARK_PLAN.md)** - 1TB benchmark plan

## Key Tools

Located in `benchmarks/`:
- **`syscall_analysis.sh`** - Comprehensive syscall analysis (CI-integrated)
- **`trace_from_getdents.sh`** - Filter strace from first directory read
- **`trace_io_uring_ops.bt`** - bpftrace script for io_uring opcodes
- **`count_io_uring_ops_simple.sh`** - Estimate io_uring operations

## Quick Start

### Run Syscall Analysis
```bash
# Automated analysis
cargo make syscall-analysis

# Or manually
./benchmarks/syscall_analysis.sh /tmp/src /tmp/dst 5 10
```

### Trace Syscalls (Filtered)
```bash
# From first directory read (42% noise reduction)
./benchmarks/trace_from_getdents.sh /src /dst -a

# Per-file only
strace -f -P /src/file -P /dst/file arsync /src/file /dst/file -a
```

### Count io_uring Operations
```bash
# Estimate (no sudo)
./benchmarks/count_io_uring_ops_simple.sh /tmp/src 10

# Exact (needs sudo)
sudo bpftrace benchmarks/trace_io_uring_ops.bt -c 'arsync /src /dst -a'
```

## Key Findings

### Phase 1: FileMetadata Unification
- Merged `FileMetadata` with `ExtendedMetadata`
- io_uring statx infrastructure ready
- All tests passing

### Phase 2: DirectoryFd Traversal
- `DirectoryFd::statx_full()` - complete metadata via io_uring
- `DirectoryFd::open_file_at()` - TOCTOU-safe file opens
- `ExtendedMetadata::from_dirfd()` - preferred constructor
- 80% reduction in statx calls achieved

### Security Improvements
- 100% FD-based metadata (fchmod, fchown, futimens)
- TOCTOU-safe directory traversal
- O_NOFOLLOW on file opens
- Pre-epoch timestamp support

### Performance Optimizations
- io_uring statx (async metadata)
- Batching: ~15 ops per io_uring_enter
- Optional --fsync flag (default OFF, matches rsync)
- 99.94% of file I/O is async

## CI Integration

Syscall analysis runs automatically on every PR:
- `.github/workflows/syscall-analysis.yml`
- Reports issues as PR comments
- Uploads detailed traces as artifacts

## References

- Main PR: [#84](https://github.com/jmalicki/arsync/pull/84)
- Parent docs: [docs/README.md](../../README.md)
- Development docs: [docs/development/](../development/)

