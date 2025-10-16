# compio-fs-extended — Cross-Platform Support (Linux, macOS, Windows)

Date: 2025-10-16
Status: Draft (R&D, design in progress)
Owner: investigate/compi-fs-extended-cross-platform

## Goals
- Enable this crate to compile and provide a coherent API on Linux, macOS, and Windows.
- Keep the public API stable where possible (`ExtendedFile`, traits like `CopyFileRange`, `Fadvise`, `Fallocate`, `XattrOps`, `OwnershipOps`).
- Use the best native mechanism per OS where feasible; otherwise provide safe fallbacks or return NotSupported.
- Maintain good async ergonomics with `compio` backends: io_uring on Linux, kqueue on macOS, IOCP on Windows. On macOS specifically, ensure heavy I/O paths (e.g., copy fallbacks) are expressed through `compio::fs` async reads/writes so they are driven by kqueue rather than blocking threads.

## Non-goals (initial phase)
- Feature-parity for every operation across all OSes (some are inherently Linux-only).
- Optimal zero-copy clones everywhere (e.g., Windows/NTFS/REFS extent duplication) in v1.
- Full device file management on non-Unix platforms.

---

## Current State (Audit Summary)
The crate is Linux-centric and uses Unix-specific APIs directly or via io_uring:

- Dependencies
  - `io-uring` (Linux-only)
  - `nix` (Unix-only)
  - `libc` (Unix APIs referenced)
  - `xattr` (Unix; used for list; impl uses io_uring for get/set)
- Modules with Linux/Unix assumptions
  - `copy.rs`: uses `libc::copy_file_range` and `std::os::unix::io::AsRawFd`.
  - `fadvise.rs`: io_uring `opcode::Fadvise`; Unix fd APIs.
  - `fallocate.rs`: io_uring `opcode::Fallocate`; Unix fd APIs.
  - `xattr.rs`: io_uring `FGetXattr`/`FSetXattr`; Unix libc for path ops; `xattr` crate for list.
  - `metadata.rs`: io_uring `Statx`; Unix-specific fallbacks and `nix` for time functions.
  - `symlink.rs`: io_uring `SymlinkAt`; `nix::fcntl::readlinkat`.
  - `hardlink.rs`: io_uring `LinkAt`.
  - `directory.rs`: `DirectoryFd` uses `std::os::unix::io::AsRawFd`; `nix::sys::stat::mkdirat`.
  - `device.rs`: `nix::sys::stat::mknod`, `mkfifo` (Unix-only).
  - `ownership.rs`: `nix::unistd::{fchown, chown}`; Unix-only metadata (`MetadataExt`).

Key implication: As-is, macOS may compile for some Unix paths with tweaks, but Windows will not build due to Unix-only crates, io_uring, and `std::os::unix` usage.

---

## High-Level Architecture Proposal
Mirror compio's strategy: keep a stable public API while routing to per-OS backends. On macOS, leverage the kqueue-backed `compio` runtime by implementing I/O-heavy code paths via `compio::fs` asynchronous APIs (read/write/open), which the runtime schedules on kqueue.

- Directory layout
  - `src/` (public API: traits, wrappers, re-exports)
- `src/sys/`
  - `linux/` — io_uring + Linux syscalls
  - `darwin/` — kqueue-backed implementations using `compio::fs` async operations for read/write paths, plus Darwin libc (`fcntl`, `copyfile`, `fclonefileat`, xattr) invoked via lightweight `spawn` when not representable as readiness-driven I/O
  - `windows/` — IOCP runtime, Win32 APIs (`CopyFileExW`, `SetFileInformationByHandle`, etc.)

- Dependency gating
  - Move Linux-only deps under target cfg:
    ```toml
    [target.'cfg(target_os = "linux")'.dependencies]
    io-uring = "0.7"
    nix = { version = "0.28", features = ["fs", "user", "time"] }
    libc = "0.2"
    xattr = { version = "1.0", optional = true }
    
    [target.'cfg(any(target_os = "macos", target_os = "ios"))'.dependencies]
    libc = "0.2"
    nix = { version = "0.28", features = ["fs", "user", "time"] }
    xattr = { version = "1.0", optional = true }
    
    [target.'cfg(target_os = "windows")'.dependencies]
    windows-sys = { version = "0.59", features = [
      "Win32_Foundation",
      "Win32_Storage_FileSystem",
      "Win32_System_IO",
    ] }
    ```
  - Top-level `features`
    - Keep `xattr` feature; enable only where supported (`unix`), or return NotSupported on Windows.

- Module routing
  - In `lib.rs`, expose traits/types and `pub(crate)` route to `sys::*` via `#[cfg]`:
    ```rust
    #[cfg(target_os = "linux")] mod sys; // re-export linux impls
    #[cfg(target_os = "macos")] mod sys; // darwin impls (kqueue-backed)
    #[cfg(target_os = "windows")] mod sys; // windows impls
    ```
  - Keep trait methods the same. Implementations delegate into `sys::<op>::impl_*`.
  - On macOS, avoid io_uring-specific `submit` opcodes; use `compio::fs` async operations so the runtime naturally utilizes kqueue.

---

## macOS kqueue integration (how this works)

`compio` uses kqueue on macOS to drive readiness-based I/O. We will structure Darwin implementations so that:

- I/O-heavy paths (copy fallbacks, chunked transfers, any looping reads/writes) are expressed with `compio::fs::File::{read_at, write_at, read, write}`. The runtime polls these via kqueue; no blocking threads are involved on the hot path.
- One-shot syscalls that don’t map to readiness (e.g., `fcntl(F_PREALLOCATE)`, `getxattr`, `setxattr`, `copyfile`, `fclonefileat`) are quick administrative calls. We invoke them via `compio::runtime::spawn` to keep the async contract without stalling the reactor. These aren’t kqueue events, but they are short-lived and not part of data transfer loops.

Example (Darwin ranged copy fallback, kqueue-driven):
```rust
// Pseudocode (Darwin): use compio::fs async read/write, scheduled on kqueue
let mut offset = src_offset;
let mut remaining = len;
let mut buf = vec![0u8; 1 << 20]; // 1 MiB
while remaining > 0 {
    let to_read = remaining.min(buf.len() as u64) as usize;
    let n = src.read_at(&mut buf[..to_read], offset).await?; // kqueue-driven
    if n == 0 { break; }
    dst.write_at(&buf[..n], offset).await?; // kqueue-driven
    offset += n as u64;
    remaining -= n as u64;
}
```

Administrative ops example (Darwin preallocate with `F_PREALLOCATE`):
```rust
compio::runtime::spawn(async move {
    // Call fcntl(F_PREALLOCATE) via libc/nix here
}).await?;
```

This approach ensures we “use kqueue” on macOS for the performance-critical streaming parts while keeping overall APIs async and cross-platform.

---

## Operation Design by Platform
Below: current Linux approach and proposed macOS/Windows designs. Where an operation is not applicable, return a clear `ExtendedError::NotSupported`.

### 1) Copy file range (same-FS efficient copy)
- Linux (current): `libc::copy_file_range` (fd-to-fd) with offsets; best-effort; fallback TBD.
- macOS (Darwin):
  - Prefer APFS clone when possible: `fclonefileat` (fd/dirfd). If unavailable or cross-FS, fallback.
  - Fallbacks: `copyfile(3)` for whole-file copies; for ranged copies, use kqueue-driven `compio::fs::File::{read_at, write_at}` loop (see pseudocode above). This ensures the data path is scheduled via kqueue.
  - Strategy:
    - If length == 0 (probe): try `fclonefileat` or return capability boolean.
    - Otherwise, attempt clone, else kqueue-driven read/write fallback.
- Windows:
  - Whole-file: `CopyFileExW` (fast path, system optimized) when copying entire file with offsets 0..EOF.
  - Ranged copies: No direct `copy_file_range` equivalent. Use overlapped I/O with compio (IOCP) `read_at`/`write_at` loop.
  - Optional (future): `FSCTL_DUPLICATE_EXTENTS_TO_FILE` (extent cloning) on supported FS (ReFS/NTFS) for same-volume clones.

### 2) fadvise (access pattern hints)
- Linux (current): io_uring `Fadvise` opcode.
- macOS:
  - `posix_fadvise` exists but may be a stub; prefer `fcntl(F_RDADVISE, radvisory)` for read-ahead hints.
  - Invoke via `spawn` (administrative), while the actual data path remains kqueue-driven where relevant.
- Windows:
  - No direct equivalent. Hints are typically provided at open (`FILE_FLAG_SEQUENTIAL_SCAN`/`FILE_FLAG_RANDOM_ACCESS`).
  - Design: best-effort no-op; document that callers should choose open flags for Windows.

### 3) fallocate (preallocation / hole punching / zero range)
- Linux (current): io_uring `Fallocate`.
- macOS:
  - Use `fcntl(F_PREALLOCATE, &fstore)` for space preallocation.
  - Map modes:
    - DEFAULT/KEEP_SIZE -> `F_PREALLOCATE` with/without extending the file; zeroing not guaranteed.
    - ZERO_RANGE/PUNCH_HOLE -> no direct; emulate with `ftruncate` + writes or return NotSupported.
- Windows:
  - Use `SetFileInformationByHandle` with `FILE_ALLOCATION_INFO` to preallocate space.
  - `SetEndOfFile` to extend logical file size.
  - Punch/zero range not generally supported; return NotSupported or emulate via writes.

### 4) xattr (extended attributes)
- Linux (current): io_uring `FGetXattr`/`FSetXattr`; list via `xattr` crate.
- macOS:
  - Darwin supports xattrs (`getxattr`, `setxattr`, `listxattr`, `removexattr`) and `f*` variants.
  - Implement using `xattr` crate or `libc` functions; no io_uring.
- Windows:
  - POSIX xattrs are not supported. NTFS EAs/ADS exist but semantics differ.
  - Design: return `ExtendedError::NotSupported("xattr unsupported on Windows")` for all xattr ops (feature still compiles but is a no-op on Windows).

### 5) Symlink / Hardlink
- Linux (current): io_uring `SymlinkAt` and `LinkAt`; `nix::fcntl::readlinkat`.
- macOS:
  - Use `nix`/`libc` or `std::os::unix::fs::{symlink, symlinkat}` and `std::fs::hard_link`.
- Windows:
  - Use `std::os::windows::fs::{symlink_file, symlink_dir}` and `std::fs::hard_link`.
  - Note: symlink creation may require developer mode/admin; document errors.

### 6) Directory operations (`DirectoryFd`) and *at variants
- Linux (current): `DirectoryFd` uses Unix fd; `mkdirat` via `nix`.
- macOS:
  - Maintain `DirectoryFd` as a Unix fd; `mkdirat` supported on modern macOS.
- Windows:
  - No raw fd; use raw HANDLEs via `AsRawHandle`.
  - Design: introduce `DirectoryHandle` on Windows (wrapper around `HANDLE` opened with `FILE_FLAG_BACKUP_SEMANTICS`). For now, provide path-based helpers and document reduced security vs *at.

### 7) Metadata (statx, utimens)
- Linux (current): io_uring `Statx`; `nix` for utimensat/futimens.
- macOS:
  - Use `stat`/`fstat` via `std::fs::metadata` and `filetime` crate for times.
  - `futimens`/`utimensat` may be available; otherwise use `filetime` fallbacks.
- Windows:
  - Use `std::fs::metadata` for basic info; times via `filetime` crate (`SetFileTime`).
  - No `statx`; return equivalent `(atime, mtime)` via available APIs.

### 8) Device / special files
- Linux (current): `mknod`, `mkfifo`, sockets.
- macOS:
  - `mknod`, `mkfifo` available; keep behind `#[cfg(unix)]`.
- Windows:
  - Not applicable. Return NotSupported.

### 9) Ownership (chown/fchown)
- Linux/macOS:
  - Keep Unix implementation via `nix`.
- Windows:
  - No POSIX UID/GID. Could map to ACLs (complex, out-of-scope v1).
  - Return NotSupported on Windows.

---

## Public API Compatibility
- Keep `ExtendedFile` wrapper and traits stable.
- On unsupported platforms, trait methods return `ExtendedError::NotSupported` with clear messages.
- Where feasible, provide best-effort fallbacks (e.g., read/write copy when clone is unavailable).

---

## Incremental Implementation Plan
1) Compile-on-all-OS scaffolding
   - Add target-specific dependency gating in `Cargo.toml`.
   - Introduce `src/sys/{linux,darwin,windows}/` with minimal shims.
   - Route existing Linux code into `sys/linux/*` with `#[cfg(target_os = "linux")]`.
   - Provide Darwin implementations where straightforward (xattr via libc/xattr crate; symlink/hardlink via std; directory via `mkdirat` or std).
   - Provide Windows stubs that return NotSupported for complex ops; implement simple ones via std/win32 (hard links, symlinks where permitted, preallocation via `FILE_ALLOCATION_INFO`).

2) Fallbacks & best-effort paths
   - Copy: implement generic `read_at/write_at` fallback on all platforms (compio-friendly) when native clone is unavailable.
   - fadvise: no-op on unsupported systems.
   - fallocate: preallocation fallback where possible (Darwin/Windows) with clear mode mapping.

3) CI matrix
   - Build & test on `ubuntu-latest`, `macos-latest`, `windows-latest`.
   - On Windows, skip Unix-only tests via `#[cfg(unix)]` and feature guards.

4) Documentation & examples
   - Document per-OS behavior and limitations.
   - Provide examples per platform with cfg-gated snippets.

---

## Testing & CI
- Add GitHub Actions matrix:
  - OS: ubuntu-latest, macos-latest, windows-latest
  - Jobs: format, clippy, unit tests, doc build
- Gate tests with `#[cfg]` to avoid compiling Unix-only code on Windows.

---

## Risks & Open Questions
- Windows xattr parity is not realistic; recommend NotSupported.
- Windows ownership/ACL mapping is non-trivial; defer.
- Darwin clone (`fclonefileat`) availability varies; robust fallback needed.
- Ensuring no accidental inclusion of `io-uring` on non-Linux targets (Cargo cfg hygiene).
- Designing a `DirectoryHandle` abstraction on Windows that fits compio ergonomics.

---

## Mapping Cheatsheet (quick reference)

| Operation        | Linux                        | macOS (Darwin)                         | Windows                                  |
|------------------|------------------------------|----------------------------------------|-------------------------------------------|
| copy_file_range  | `copy_file_range` (syscall)  | `fclonefileat` → `copyfile` → RW       | `CopyFileExW` (whole) → RW fallback       |
| fadvise          | io_uring `Fadvise`           | `fcntl(F_RDADVISE)` → no-op            | no-op                                     |
| fallocate        | io_uring `Fallocate`         | `fcntl(F_PREALLOCATE)`                 | `FILE_ALLOCATION_INFO` / `SetEndOfFile`   |
| xattr get/set    | io_uring + libc              | libc/xattr crate                       | NotSupported                              |
| xattr list       | `xattr` crate                | `xattr` crate                          | NotSupported                              |
| symlink          | io_uring `SymlinkAt`         | `std::os::unix::fs::symlink`           | `std::os::windows::fs::{symlink_*}`       |
| hardlink         | io_uring `LinkAt`            | `std::fs::hard_link`                   | `std::fs::hard_link`                      |
| dir *at ops      | `DirectoryFd` + `mkdirat`    | same (Unix)                            | Path-based helpers (HANDLE optional v2)   |
| metadata (statx) | io_uring `Statx`             | `std::fs::metadata` + `filetime`       | `std::fs::metadata` + `filetime`          |
| device files     | `mknod`, `mkfifo`            | `mknod`, `mkfifo`                      | NotSupported                              |
| ownership        | `fchown`, `chown`            | same (Unix)                            | NotSupported (ACLs future work)           |

---

## Next Steps (engineering tasks)
- Cargo cfg-gating for Linux-only deps; introduce Windows/Darwin deps.
- Create `src/sys/{linux,darwin,windows}` and move/introduce implementations.
- Implement Darwin: xattr via libc, fallocate via `F_PREALLOCATE`, symlink/hardlink/std, directory via `mkdirat`/std.
- Implement Windows: minimal viable set (copy RW fallback, hardlink/symlink via std, preallocate via `FILE_ALLOCATION_INFO`), return NotSupported for xattr/ownership/device.
- Add CI matrix and cfg-gate tests.

Appendix A: Useful APIs
- Darwin: `fclonefileat(2)`, `copyfile(3)`, `fcntl(F_PREALLOCATE)`, `fcntl(F_RDADVISE)`, `getxattr(2)`/`setxattr(2)`
- Windows: `CopyFileExW`, `SetFileInformationByHandle(FILE_ALLOCATION_INFO)`, `SetEndOfFile`, `CreateHardLinkW`, `CreateSymbolicLinkW`
