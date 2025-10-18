# Syscall Optimization Progress

**Date:** 2025-10-18  
**Branch:** `benchmark/parallel-copy-perf-analysis`

## Summary

This document tracks our progress toward 100% FD-based, io_uring-accelerated operations.

## Current Status (Phase 1 Complete)

### ✅ Completed Optimizations

1. **FD-based Timestamp Preservation**
   - **Before:** `utimensat(AT_FDCWD, "/path/to/file", times)` ❌ PATH
   - **After:** `utimensat(fd, NULL, times)` ✅ FD (futimens equivalent)
   - **Impact:** TOCTOU-safe, eliminates symlink attacks

2. **io_uring Fallocate**
   - **Status:** Already using io_uring FALLOCATE opcode ✅
   - **Evidence:** Zero direct `fallocate()` syscalls in traces
   - **Verified:** All preallocation via `io_uring_enter()`

3. **Unified Metadata Structure**
   - **Merged:** `FileMetadata` with `ExtendedMetadata`
   - **Ready for:** io_uring statx in Phase 2
   - **Fields:** size, mode, uid, gid, nlink, ino, dev, timestamps

4. **Syscall Analysis CI**
   - **Script:** `benchmarks/syscall_analysis.sh`
   - **CI:** `.github/workflows/syscall-analysis.yml`
   - **Local:** `cargo make syscall-analysis`
   - **Tracking:** io_uring usage, TOCTOU safety, redundancy

### ⚠️  Current Issues (Phase 2 Targets)

1. **statx: Direct Syscalls (NOT io_uring)**
   ```bash
   # Current:
   statx(AT_FDCWD, "/path/to/file", STATX_ALL, ...) = 0  ❌ Blocking syscall
   statx(AT_FDCWD, "/path/to/file", STATX_ALL, ...) = 0  ❌ Called ~2-3x per file
   
   # Target:
   io_uring_enter([STATX(dirfd, "file", ...)])  ✅ Async via io_uring
   ```
   **Impact:** Blocking syscalls, TOCTOU-vulnerable

2. **statx: Redundant Calls**
   - **Current:** ~2.6 statx calls per file
   - **Target:** 1 statx call per file
   - **Cause:** Multiple code paths fetching same metadata
   - **Fix:** Pass metadata through call chain

3. **openat: Not Using dirfd**
   ```bash
   # Current:
   openat(AT_FDCWD, "/absolute/path/to/file", O_RDONLY) = 7  ❌
   
   # Target:
   openat(parent_dirfd, "file", O_RDONLY) = 7  ✅
   ```
   **Impact:** TOCTOU-vulnerable, can't use io_uring OPENAT2

## Syscall Analysis Results

### Test Configuration
- Files: 5 × 10MB
- Mode: Archive (-a, full metadata preservation)
- Total syscalls: ~3500

### Breakdown
| Operation | Count | Per File | Status |
|-----------|-------|----------|--------|
| io_uring_enter | 1398 | 280 | ✅ Primary I/O path |
| statx | 26 | 5.2 | ⚠️ Redundant |
| statx (path-based) | 13 | 2.6 | ⚠️ TOCTOU risk |
| statx (FD-based) | 13 | 2.6 | ✅ Safe |
| openat | 23 | 4.6 | ⚠️ Not dirfd |
| openat (user files) | 1 | 0.2 | ✅ Minimal |
| fallocate (direct) | 0 | 0 | ✅ Via io_uring |
| fchmod | 5 | 1.0 | ✅ FD-based |
| fchown | 5 | 1.0 | ✅ FD-based |
| utimensat (FD-based) | 5 | 1.0 | ✅ FD-based |
| utimensat (path-based) | 0 | 0 | ✅ None |

### Security Score: 80/100

**PASS:**
- ✅ 100% FD-based metadata preservation (fchmod, fchown, futimens)
- ✅ 0 path-based utimensat
- ✅ Heavy io_uring usage (40% of all syscalls)
- ✅ fallocate via io_uring

**WARNINGS:**
- ⚠️ Path-based statx calls (TOCTOU risk)
- ⚠️ Redundant statx calls (~2.6 per file, target: 1)
- ⚠️ openat not using dirfd

## Phase 2: DirectoryFd + io_uring Architecture

### Planned Changes

1. **Extend compio-fs-extended** ✅ DONE
   - Added `FileMetadata` struct with full statx fields
   - `DirectoryFd::statx_full()` returns complete metadata
   - io_uring STATX infrastructure ready

2. **Add DirectoryFd::open_file_at()** 🚧 IN PROGRESS
   - Open files relative to directory FD
   - Enable dirfd-based file operations

3. **Refactor Directory Traversal** 📋 PLANNED
   - Pass `DirectoryFd` through call chain
   - Use `DirectoryFd::statx_full()` for metadata
   - Eliminate redundant statx calls

4. **Update copy_file() Signature** 📋 PLANNED
   - Accept dirfd + filename instead of full path
   - Use pre-fetched metadata (no redundant stats)
   - Open files via dirfd

### Expected Improvements

| Metric | Before | After | Improvement |
|--------|--------|-------|-------------|
| statx calls | 2.6/file | 1.0/file | -62% |
| statx via io_uring | 0% | 100% | ∞ |
| Path-based statx | 2.6/file | 0/file | -100% |
| TOCTOU-safe opens | No | Yes | Security ↑ |
| Security score | 80/100 | 100/100 | +25% |

## Verification

### How to Run Locally

```bash
# Quick check
cargo make syscall-analysis

# Or manually:
./benchmarks/syscall_analysis.sh /tmp/src /tmp/dst 5 10

# View detailed traces:
cat /tmp/syscall-analysis-report.txt
cat /tmp/syscall-analysis-summary.txt  # strace -c output
cat /tmp/syscall-analysis-raw.txt      # Full trace
```

### CI Integration

- Runs automatically on every PR
- Comments on PR with results
- Uploads traces as artifacts (30-day retention)
- Fails on critical issues
- Warns on performance regressions

### Trace Filtering

For clean syscall views without initialization noise:

```bash
# Method 1: Filter by file path
strace -f -P /src/file -P /dst/file arsync /src/file /dst/file -a

# Method 2: Filter from first directory read
./benchmarks/trace_from_getdents.sh /src /dst -a
# → Removes ~42% of initialization syscalls
```

## Next Steps

1. Complete Phase 2 implementation (dirfd + io_uring statx)
2. Run syscall analysis to verify improvements
3. Update benchmarks with new metrics
4. Document performance gains

## References

- Architecture plan: `docs/DIRFD_IO_URING_ARCHITECTURE.md`
- Trace filtering: `docs/SYSCALL_TRACE_FILTERING.md`
- Analysis script: `benchmarks/syscall_analysis.sh`
- CI workflow: `.github/workflows/syscall-analysis.yml`

