# 📊 Syscall Analysis Report

**Date:** 2025-10-18 09:21:03 -07:00
**Test:** 3 files × 5MB
**Binary:** `./target/release/arsync`

---

## 🔄 io_uring Usage

- **io_uring_setup calls:** 129 (one per worker thread + main)
- **io_uring_enter calls:** 1561

✅ **PASS:** Heavy io_uring usage

### Batching Efficiency

| Metric | Value |
|--------|-------|
| Single-op submissions (batch=1) | 458 |
| Multi-op submissions (batch≥2) | 170 |
| Average batch size | 1.0 ops/submit |
| Maximum batch size | 2 ops/submit |

⚠️  **WARNING:** Poor batching (avg≤1.5, mostly single-op submissions)
> Better batching could reduce syscall overhead

## 🔄 io_uring Operations Breakdown

ℹ️  **INFO:** io_uring operation types not visible in standard strace output.

> **Note:** To see detailed io_uring operation breakdown, use `bpftrace` or kernel tracing.
> High `io_uring_enter` count + low direct syscalls indicates operations are async via io_uring.

### Inferred io_uring Usage

Based on syscall patterns:

- **io_uring_enter calls**: 1561 (operations submitted)
- **File read() calls (FD≥100)**: 0 (should be 0 with io_uring)
- **File write() calls (FD≥100)**: 0 (should be 0 with io_uring)
- **pread/pwrite calls**: 0 (should use io_uring read_at/write_at)
- **Direct statx calls**: 10 (some may be io_uring)

> Note: 174 read() and 174 write() calls on low FDs (eventfd/pipe for thread sync) excluded from file I/O counts

✅ **EXCELLENT:** All file I/O via io_uring (no direct read/write syscalls)

## 📋 Metadata Operations

| Metric | Count |
|--------|-------|
| Total statx calls | 10 |
| Path-based (AT_FDCWD + path) | 10 |
| FD-based (dirfd + filename) | 0 |
| **Average per file** | **3.3** |

⚠️  **WARNING:** High path-based statx count (TOCTOU-vulnerable)
- Expected: ≤6 (1-2 per file)
- Got: 10 (~3.3 per file)

### Per-File Breakdown

**file1.bin:**
- statx: 1
- openat: 0
- total mentions: 2

**file2.bin:**
- statx: 1
- openat: 0
- total mentions: 1

**file3.bin:**
- statx: 1
- openat: 0
- total mentions: 1

## 📁 File Operations

| Metric | Count |
|--------|-------|
| Total openat calls | 4 |
| User file opens (path-based) | 0 |
| **Average per file** | **0.0** |

✅ **PASS:** Reasonable openat count

**Direct fallocate syscalls:** 0

✅ **PASS:** fallocate via io_uring (no direct syscalls)

## 🔒 Metadata Preservation

| Operation | Count |
|-----------|-------|
| fchmod (FD-based permissions) | 18 |
| fchown (FD-based ownership) | 20 |
| utimensat (total) | 20 |
| └─ FD-based (fd, NULL, ...) | 10 |
| └─ Path-based (AT_FDCWD, path, ...) | 0 |

✅ **EXCELLENT:** 50% FD-based timestamp preservation (TOCTOU-safe)

## 📁 Directory Creation

| Operation | Count | Type |
|-----------|-------|------|
| mkdir | 0 | Path-based |
| mkdirat | 0 | FD-based |
| **Total directory creates** | **0** | |

## 🔗 Symlink Operations

| Operation | Count | Type |
|-----------|-------|------|
| symlink | 0 | Path-based |
| symlinkat | 0 | FD-based |
| readlink | 0 | Path-based |
| readlinkat | 0 | FD-based |
| lstat | 0 | Path-based (symlink metadata) |

ℹ️  **INFO:** No symlink operations detected in test

## 📂 Directory Traversal Details

**Source directory** (`/tmp/syscall-test-src`):
- statx: 0
- openat (O_DIRECTORY): 0
- getdents64 (directory reads): 20

**Destination directory** (`/tmp/syscall-test-dst`):
- fchmod: 18 (includes files)
- fchown: 20 (includes files)

## ⚠️  Unexpected/Legacy Syscalls

✅ **EXCELLENT:** No unexpected or legacy syscalls detected!

## 📊 All Syscalls (Complete Inventory)

<details>
<summary>Click to expand full syscall list</summary>

| Syscall | Count | Category |
|---------|-------|----------|
| `io_uring_enter` | 715 | 🔄 io_uring |
| `futex` | 355 | 🧵 Threading |
| `write` | 174 | 📁 File I/O |
| `read` | 174 | 📁 File I/O |
| `clock_gettime` | 96 | 🔧 System |
| `close` | 36 | 📁 File I/O |
| `mprotect` | 11 | 💾 Memory |
| `sched_yield` | 11 | 🔧 System |
| `statx` | 10 | 📁 File I/O |
| `fchmod` | 10 | 📋 Metadata |
| `utimensat` | 10 | 📋 Metadata |
| `getdents64` | 10 | 📂 Directory |
| `fchown` | 10 | 📋 Metadata |
| `mmap` | 9 | 💾 Memory |
| `rt_sigprocmask` | 9 | 🚦 Signal |
| `sigaltstack` | 8 | 🚦 Signal |
| `munmap` | 6 | 💾 Memory |
| `fstat` | 4 | 📋 Metadata |
| `openat` | 4 | 📁 File I/O |
| `rseq` | 3 | 🚦 Signal |
| `clone3` | 3 | ⚙️  Process |
| `set_robust_list` | 3 | 🧵 Threading |
| `sched_getaffinity` | 3 | 🔧 System |
| `exit_group` | 1 | ⚙️  Process |

</details>

## 🔐 Security Assessment

**Security Score:** 70/100 🟠 Fair

⚠️  Path-based statx: TOCTOU risk

## 💡 Recommendations

- **Reduce redundant statx calls** (currently ~3.3 per file)
- Target: 1 statx per file via `DirectoryFd::statx()`

- **Use dirfd-relative operations** instead of `AT_FDCWD` + absolute paths
- Benefits: TOCTOU-safe, potentially async via io_uring

- **Improve io_uring batching** (currently low batching efficiency)
- Target: Average batch size ≥2 ops/submit

## 📊 Summary Table

| Operation | Count | Target | Status |
|-----------|-------|--------|--------|
| io_uring_enter | 1561 | >100 | ✅ PASS |
| statx (total) | 10 | <6 | ⚠️  WARN |
| statx (path-based) | 10 | =0 | ⚠️  WARN |
| openat (user files) | 0 | <12 | ✅ PASS |
| fallocate (direct) | 0 | =0 | ✅ PASS |
| utimensat (path-based) | 0 | =0 | ✅ PASS |
| utimensat (FD-based) | 10 | =3 | ⚠️  WARN |


---

📄 **Full Traces:**
- Detailed: `/tmp/syscall-analysis-raw.txt`
