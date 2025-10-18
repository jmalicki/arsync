# 📊 Syscall Analysis Report

**Date:** 2025-10-18 08:54:23 -07:00
**Test:** 3 files × 5MB
**Binary:** `./target/release/arsync`

---

## 🔄 io_uring Usage

- **io_uring_setup calls:** 129 (one per worker thread + main)
- **io_uring_enter calls:** 1856

✅ **PASS:** Heavy io_uring usage

### Batching Efficiency

| Metric | Value |
|--------|-------|
| Single-op submissions (batch=1) | 752 |
| Multi-op submissions (batch≥2) | 23 |
| Average batch size | 0.9 ops/submit |
| Maximum batch size | 2 ops/submit |

⚠️  **WARNING:** Poor batching (avg≤1.5, mostly single-op submissions)
> Better batching could reduce syscall overhead

## 🔄 io_uring Operations Breakdown

ℹ️  **INFO:** io_uring operation types not visible in standard strace output.

> **Note:** To see detailed io_uring operation breakdown, use `bpftrace` or kernel tracing.
> High `io_uring_enter` count + low direct syscalls indicates operations are async via io_uring.

### Inferred io_uring Usage

Based on syscall patterns:

- **io_uring_enter calls**: 1856 (operations submitted)
- **Direct read() calls**: 206 (should be low)
- **Direct write() calls**: 171 (should be low)
- **Direct statx calls**: 31 (mixed with io_uring statx)

⚠️  **High direct syscall counts** - may not be fully utilizing io_uring

## 📋 Metadata Operations

| Metric | Count |
|--------|-------|
| Total statx calls | 31 |
| Path-based (AT_FDCWD + path) | 18 |
| FD-based (dirfd + filename) | 13 |
| **Average per file** | **10.3** |

⚠️  **WARNING:** High path-based statx count (TOCTOU-vulnerable)
- Expected: ≤6 (1-2 per file)
- Got: 18 (~6.0 per file)

### Per-File Breakdown

**file1.bin:**
- statx: 0
- openat: 0
- total mentions: 2

**file2.bin:**
- statx: 0
- openat: 0
- total mentions: 1

**file3.bin:**
- statx: 0
- openat: 0
- total mentions: 1

## 📁 File Operations

| Metric | Count |
|--------|-------|
| Total openat calls | 27 |
| User file opens (path-based) | 0 |
| **Average per file** | **0.0** |

✅ **PASS:** Reasonable openat count

**Direct fallocate syscalls:** 0

✅ **PASS:** fallocate via io_uring (no direct syscalls)

## 🔒 Metadata Preservation

| Operation | Count |
|-----------|-------|
| fchmod (FD-based permissions) | 20 |
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
- fchmod: 20 (includes files)
- fchown: 20 (includes files)

## ⚠️  Unexpected/Legacy Syscalls

**Found unexpected syscalls:**

- `access()`: 1 calls (TOCTOU-vulnerable, avoid)
- `read()`: 206 calls (high count, should use io_uring)
- `write()`: 171 calls (high count, should use io_uring)
- `pread()`: 2 calls (use io_uring `read_at` instead)

> These syscalls indicate potential performance or security issues.

## 📊 All Syscalls (Complete Inventory)

<details>
<summary>Click to expand full syscall list</summary>

| Syscall | Count | Category |
|---------|-------|----------|
| `io_uring_enter` | 929 | 🔄 io_uring |
| `futex` | 381 | 🧵 Threading |
| `mmap` | 355 | 💾 Memory |
| `rt_sigprocmask` | 338 | 🚦 Signal |
| `mprotect` | 213 | 💾 Memory |
| `read` | 206 | 📁 File I/O |
| `write` | 171 | 📁 File I/O |
| `sigaltstack` | 142 | 🚦 Signal |
| `clock_gettime` | 104 | 🔧 System |
| `sched_getaffinity` | 73 | 🔧 System |
| `set_robust_list` | 70 | 🧵 Threading |
| `rseq` | 70 | 🚦 Signal |
| `clone3` | 69 | ⚙️  Process |
| `getrandom` | 66 | 🔧 System |
| `io_uring_setup` | 65 | 🔄 io_uring |
| `eventfd2` | 65 | 🔧 System |
| `close` | 56 | 📁 File I/O |
| `munmap` | 42 | 💾 Memory |
| `statx` | 31 | 📁 File I/O |
| `openat` | 27 | 📁 File I/O |
| `sched_yield` | 15 | 🔧 System |
| `lseek` | 13 | 📁 File I/O |
| `fchmod` | 10 | 📋 Metadata |
| `utimensat` | 10 | 📋 Metadata |
| `getdents64` | 10 | 📂 Directory |
| `fchown` | 10 | 📋 Metadata |
| `fstat` | 9 | 📋 Metadata |
| `rt_sigaction` | 6 | 🚦 Signal |
| `brk` | 4 | 💾 Memory |
| `prlimit64` | 3 | 🔧 System |
| `pread64` | 2 | 📁 File I/O |
| `set_tid_address` | 1 | 🧵 Threading |
| `arch_prctl` | 1 | 🔧 System |
| `poll` | 1 | 🔧 System |
| `execve` | 1 | ⚙️  Process |
| `access` | 1 | ❓ **Unknown** |
| `exit_group` | 1 | ⚙️  Process |

</details>

### ❓ Unknown/Uncategorized Syscalls

- **`access`**: 1 calls

> These syscalls are not in our expected categories. Review to ensure they're intentional.

## 🔐 Security Assessment

**Security Score:** 76/100 🟠 Fair

⚠️  Path-based statx: TOCTOU risk

## 💡 Recommendations

- **Reduce redundant statx calls** (currently ~10.3 per file)
- Target: 1 statx per file via `DirectoryFd::statx()`

- **Use dirfd-relative operations** instead of `AT_FDCWD` + absolute paths
- Benefits: TOCTOU-safe, potentially async via io_uring

- **Improve io_uring batching** (currently low batching efficiency)
- Target: Average batch size ≥2 ops/submit

## 📊 Summary Table

| Operation | Count | Target | Status |
|-----------|-------|--------|--------|
| io_uring_enter | 1856 | >100 | ✅ PASS |
| statx (total) | 31 | <6 | ⚠️  WARN |
| statx (path-based) | 18 | =0 | ⚠️  WARN |
| openat (user files) | 0 | <12 | ✅ PASS |
| fallocate (direct) | 0 | =0 | ✅ PASS |
| utimensat (path-based) | 0 | =0 | ✅ PASS |
| utimensat (FD-based) | 10 | =3 | ⚠️  WARN |


---

📄 **Full Traces:**
- Detailed: `/tmp/syscall-analysis-raw.txt`
