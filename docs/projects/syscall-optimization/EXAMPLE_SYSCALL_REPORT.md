# 📊 Syscall Analysis Report

**Date:** 2025-10-18 09:17:08 -07:00
**Test:** 2 files × 3MB
**Binary:** `./target/release/arsync`

---

## 🔄 io_uring Usage

- **io_uring_setup calls:** 129 (one per worker thread + main)
- **io_uring_enter calls:** 1184

✅ **PASS:** Heavy io_uring usage

### Batching Efficiency

| Metric | Value |
|--------|-------|
| Single-op submissions (batch=1) | 432 |
| Multi-op submissions (batch≥2) | 2 |
| Average batch size | 0.7 ops/submit |
| Maximum batch size | 2 ops/submit |

⚠️  **WARNING:** Poor batching (avg≤1.5, mostly single-op submissions)
> Better batching could reduce syscall overhead

## 🔄 io_uring Operations Breakdown

ℹ️  **INFO:** io_uring operation types not visible in standard strace output.

> **Note:** To see detailed io_uring operation breakdown, use `bpftrace` or kernel tracing.
> High `io_uring_enter` count + low direct syscalls indicates operations are async via io_uring.

### Inferred io_uring Usage

Based on syscall patterns:

- **io_uring_enter calls**: 1184 (operations submitted)
- **Direct read() calls**: 168 (should be low)
- **Direct write() calls**: 168 (should be low)
- **Direct statx calls**: 9 (mixed with io_uring statx)

⚠️  **High direct syscall counts** - may not be fully utilizing io_uring

## 📋 Metadata Operations

| Metric | Count |
|--------|-------|
| Total statx calls | 9 |
| Path-based (AT_FDCWD + path) | 9 |
| FD-based (dirfd + filename) | 0 |
| **Average per file** | **4.5** |

⚠️  **WARNING:** High path-based statx count (TOCTOU-vulnerable)
- Expected: ≤4 (1-2 per file)
- Got: 9 (~4.5 per file)

### Per-File Breakdown

**file1.bin:**
- statx: 1
- openat: 0
- total mentions: 2

**file2.bin:**
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
| fchown (FD-based ownership) | 18 |
| utimensat (total) | 17 |
| └─ FD-based (fd, NULL, ...) | 9 |
| └─ Path-based (AT_FDCWD, path, ...) | 0 |

✅ **EXCELLENT:** 52% FD-based timestamp preservation (TOCTOU-safe)

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
- fchown: 18 (includes files)

## ⚠️  Unexpected/Legacy Syscalls

**Found unexpected syscalls:**

- `read()`: 68 calls (high count, should use io_uring)
- `write()`: 68 calls (high count, should use io_uring)

> These syscalls indicate potential performance or security issues.

## 📊 All Syscalls (Complete Inventory)

<details>
<summary>Click to expand full syscall list</summary>

| Syscall | Count | Category |
|---------|-------|----------|
| `io_uring_enter` | 524 | 🔄 io_uring |
| `futex` | 350 | 🧵 Threading |
| `read` | 168 | 📁 File I/O |
| `write` | 168 | 📁 File I/O |
| `clock_gettime` | 93 | 🔧 System |
| `close` | 34 | 📁 File I/O |
| `rt_sigprocmask` | 16 | 🚦 Signal |
| `mprotect` | 15 | 💾 Memory |
| `mmap` | 15 | 💾 Memory |
| `sched_yield` | 14 | 🔧 System |
| `sigaltstack` | 12 | 🚦 Signal |
| `getdents64` | 10 | 📂 Directory |
| `fchown` | 9 | 📋 Metadata |
| `utimensat` | 9 | 📋 Metadata |
| `fchmod` | 9 | 📋 Metadata |
| `statx` | 9 | 📁 File I/O |
| `munmap` | 6 | 💾 Memory |
| `set_robust_list` | 5 | 🧵 Threading |
| `clone3` | 5 | ⚙️  Process |
| `rseq` | 5 | 🚦 Signal |
| `sched_getaffinity` | 5 | 🔧 System |
| `openat` | 4 | 📁 File I/O |
| `fstat` | 4 | 📋 Metadata |
| `exit_group` | 1 | ⚙️  Process |

</details>

## 🔐 Security Assessment

**Security Score:** 70/100 🟠 Fair

⚠️  Path-based statx: TOCTOU risk

## 💡 Recommendations

- **Reduce redundant statx calls** (currently ~4.5 per file)
- Target: 1 statx per file via `DirectoryFd::statx()`

- **Use dirfd-relative operations** instead of `AT_FDCWD` + absolute paths
- Benefits: TOCTOU-safe, potentially async via io_uring

- **Improve io_uring batching** (currently low batching efficiency)
- Target: Average batch size ≥2 ops/submit

## 📊 Summary Table

| Operation | Count | Target | Status |
|-----------|-------|--------|--------|
| io_uring_enter | 1184 | >100 | ✅ PASS |
| statx (total) | 9 | <4 | ⚠️  WARN |
| statx (path-based) | 9 | =0 | ⚠️  WARN |
| openat (user files) | 0 | <8 | ✅ PASS |
| fallocate (direct) | 0 | =0 | ✅ PASS |
| utimensat (path-based) | 0 | =0 | ✅ PASS |
| utimensat (FD-based) | 9 | =2 | ⚠️  WARN |


---

📄 **Full Traces:**
- Detailed: `/tmp/syscall-analysis-raw.txt`
