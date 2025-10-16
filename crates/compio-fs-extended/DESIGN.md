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
    // Use read_at/write_at if available in compio::fs; otherwise fall back to read/write + seek
    let n = /* src.read_at or src.read */
            src.read_at(&mut buf[..to_read], offset).await?; // kqueue-driven
    if n == 0 { break; }
    /* dst.write_at or dst.write */
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

## Darwin kqueue capability catalog

What kqueue is: a kernel event notification mechanism that signals readiness and state changes. You register kevents (ident + filter + flags), and the kernel enqueues events when conditions occur. It is not a replacement for syscalls like clone, preallocate, or xattr; it complements them by telling you when I/O can proceed without blocking.

- EVFILT_READ / EVFILT_WRITE: readiness for reads/writes on fds (files, sockets, pipes). Used by `compio::fs` to drive async I/O.
- EVFILT_VNODE: path changes on files/dirs (NOTE_DELETE, NOTE_WRITE, NOTE_EXTEND, NOTE_ATTRIB, NOTE_LINK, NOTE_RENAME, NOTE_REVOKE).
- EVFILT_PROC: process events (NOTE_EXIT, NOTE_FORK, NOTE_EXEC, NOTE_TRACK, ...).
- EVFILT_SIGNAL: POSIX signal notifications.
- EVFILT_TIMER: timers (periodic/one-shot, incl. NOTE_USECONDS/NANOSECONDS variants).
- EVFILT_USER: user-triggered events (manual post/clear).
- EVFILT_AIO: legacy POSIX AIO completion notifications.
- EVFILT_FS: global filesystem mount/unmount notifications.

Applicability to compio-fs-extended operations:
- Streaming copy fallback (read/write loops): yes, via EVFILT_READ/WRITE readiness (through `compio::fs`).
- `fadvise`: no direct kqueue filter; it’s an advisory control (`fcntl`).
- `fallocate`/preallocate/hole punching: no kqueue filter; use `fcntl(F_PREALLOCATE)`.
- xattr get/set/list/remove: no kqueue filter for performing these; EVFILT_VNODE can notify NOTE_ATTRIB changes only.
- symlink/hardlink create/readlink: no kqueue filter; EVFILT_VNODE may notify link count changes (NOTE_LINK), not perform them.
- directory create/remove/read: no kqueue filter to perform these; EVFILT_VNODE can notify NOTE_WRITE/NOTE_LINK; directory entry enumeration is separate (getdents).
- metadata/stat/time changes: no kqueue filter to fetch/apply; EVFILT_VNODE can notify NOTE_ATTRIB.
- device files/ownership (chown): no kqueue filter; EVFILT_VNODE may notify NOTE_ATTRIB.

Bottom line: on macOS, kqueue is ideal for readiness and change notifications. Use it (via `compio::fs`) for the data path; keep administrative one-shot syscalls for actions.

---

## macOS matrix: compio::fs vs kqueue support

Legend
- Implemented by compio::fs: compio exposes an async API we can use.
- Supported by kqueue: the operation’s data path can be readiness-driven with kqueue; purely administrative syscalls are not “supported by kqueue”.

1) Implemented by compio::fs AND NOT supported by kqueue
- None (compio’s async file I/O on macOS is readiness-based and kqueue-backed by design).

2) Implemented by compio::fs AND supported by kqueue
- File reads/writes used in copy fallbacks (sequential or offset-based transfers via `read`/`write`/`read_at`/`write_at`).
- Socket/pipe async I/O (if used by higher layers).

3) NOT implemented by compio::fs AND NOT supported by kqueue
- `fadvise` (fcntl advisory control)
- `fallocate` / `F_PREALLOCATE` (fcntl)
- xattr get/set/list/remove
- symlink/hardlink creation and `readlink`
- directory `mkdirat`/enumeration operations
- metadata/stat fetch/update (`stat`, `futimens`, `utimensat`)
- device file creation (`mknod`, `mkfifo`)
- ownership change (`chown`, `fchown`)

4) NOT implemented by compio::fs BUT supported by kqueue
- File/dir change notifications: EVFILT_VNODE (NOTE_DELETE, NOTE_WRITE, NOTE_EXTEND, NOTE_ATTRIB, NOTE_LINK, NOTE_RENAME, NOTE_REVOKE)
- Process/signal/timer/user events: EVFILT_PROC, EVFILT_SIGNAL, EVFILT_TIMER, EVFILT_USER
- Global FS notifications: EVFILT_FS

Note: These are event/notification capabilities. They do not perform filesystem actions but can complement them (e.g., watch a directory for NOTE_WRITE after creating files via syscalls).

---

## Windows IOCP integration (how this works)

On Windows, `compio` integrates with IOCP (I/O Completion Ports). We express streaming operations using `compio::fs` async file APIs so that reads/writes are issued as overlapped I/O, and completions are delivered by IOCP to the runtime.

- Streaming data paths (copy fallbacks, chunked transfers) use async `compio::fs::File` methods. These translate to overlapped I/O; completions are handled by IOCP, providing scalable async performance without blocking threads.
- One-shot administrative calls that are not overlapped (e.g., `CopyFileExW`, `SetFileInformationByHandle` for allocation, metadata tweaks) will be invoked via `compio::runtime::spawn` so we do not block the IOCP reactor thread.

Example (Windows ranged copy fallback, IOCP-driven):
```rust
// Pseudocode (Windows): compio::fs async read/write => overlapped I/O via IOCP
let mut offset = src_offset;
let mut remaining = len;
let mut buf = vec![0u8; 1 << 20];
while remaining > 0 {
    let to_read = remaining.min(buf.len() as u64) as usize;
    let n = /* src.read_at or src.read */
            src.read_at(&mut buf[..to_read], offset).await?; // IOCP-driven
    if n == 0 { break; }
    /* dst.write_at or dst.write */
    dst.write_at(&buf[..n], offset).await?; // IOCP-driven
    offset += n as u64;
    remaining -= n as u64;
}
```

Administrative ops example (Windows preallocate with `FILE_ALLOCATION_INFO`):
```rust
compio::runtime::spawn(async move {
    // Call SetFileInformationByHandle(FILE_ALLOCATION_INFO)
}).await?;
```

This ensures Windows builds benefit from IOCP for data movement while keeping non-overlapped syscalls off the hot path.

---

## Windows IOCP capability catalog and matrix

What IOCP is: a Windows mechanism for scalable async I/O completion. You issue overlapped I/O (ReadFile/WriteFile/… with OVERLAPPED), associate handles with a completion port, and receive completions asynchronously. IOCP is not a replacement for administrative syscalls like `CopyFileExW` or `SetFileInformationByHandle`; it complements them by making data transfer non-blocking and scalable.

Core applicability:
- Overlapped reads/writes on file handles, sockets, named pipes, etc.
- File mapping and some metadata ops are separate APIs; they do not produce IOCP completions unless overlapped I/O is involved.

Applicability to compio-fs-extended operations:
- Streaming copy fallback (read/write loops): yes, via overlapped `ReadFile`/`WriteFile`; completions are delivered to IOCP (exposed via `compio::fs` async file ops).
- `CopyFileExW` whole-file copies: separate API; does not use our IOCP loop (we can run it off-thread if desired).
- Preallocation (`FILE_ALLOCATION_INFO`), truncate/extend (`SetEndOfFile`): administrative; not IOCP.
- xattr (not POSIX on Windows): NotSupported in v1.
- symlink/hardlink creation: administrative; not IOCP.
- directory creation/enumeration: administrative; not IOCP (FindFirstFile/FindNextFile are synchronous enumeration APIs).
- metadata/time updates: administrative; not IOCP.
- ownership/ACLs: administrative; not IOCP.

Matrix

1) Implemented by compio::fs AND NOT supported by IOCP
- None (compio’s async file I/O on Windows is built on overlapped I/O completed via IOCP).

2) Implemented by compio::fs AND supported by IOCP
- File reads/writes used in copy fallbacks (sequential or offset-based transfers via `read`/`write`/`read_at`/`write_at`).
- Socket/pipe async I/O (if used by higher layers).

3) NOT implemented by compio::fs AND NOT supported by IOCP
- `CopyFileExW` (separate API; not expressed as overlapped I/O in our stack)
- Preallocation via `SetFileInformationByHandle(FILE_ALLOCATION_INFO)`
- Truncate/extend via `SetEndOfFile`
- xattr (no POSIX xattr support; NTFS EAs out-of-scope v1)
- symlink/hardlink creation
- directory creation/enumeration
- metadata/time updates (`SetFileTime` / `GetFileInformationByHandleEx`)
- ownership/ACL changes

4) NOT implemented by compio::fs BUT supported by IOCP
- Named pipe async I/O, socket async I/O (available to higher layers; not directly in this crate’s scope)

Note: IOCP is a completion mechanism for overlapped operations. Administrative filesystem calls typically do not produce IOCP completions and should be run off-thread if they might block.

---

## Operation Design by Platform
Below: current Linux approach and proposed macOS/Windows designs. Where an operation is not applicable, return a clear `ExtendedError::NotSupported`.

### 1) Copy file range (same-FS efficient copy)
- Linux (current): `libc::copy_file_range` (fd-to-fd) with offsets; best-effort; fallback TBD.
- macOS (Darwin):
  - Prefer APFS clone when possible: `fclonefileat` (fd/dirfd). If unavailable or cross-FS, fallback.
  - Fallbacks: `copyfile(3)` for whole-file copies; for ranged copies, use a kqueue-driven `compio::fs` async read/write loop (prefer `read_at/write_at` if available). This ensures the data path is scheduled via kqueue.
  - Strategy:
    - If length == 0 (probe): try `fclonefileat` or return capability boolean.
    - Otherwise, attempt clone, else kqueue-driven read/write fallback.
- Windows:
  - Whole-file: `CopyFileExW` (fast path, system optimized) when copying entire file with offsets 0..EOF.
  - Ranged copies: No direct `copy_file_range` equivalent. Use IOCP-driven `compio::fs` async read/write loop (prefer `read_at/write_at` if available).
  - Optional (future): `FSCTL_DUPLICATE_EXTENTS_TO_FILE` (extent cloning) on supported FS (ReFS/NTFS) for same-volume clones.
  - Runtime integration:
    - macOS: data path via kqueue (async file ops); admin ops (`fclonefileat`, `copyfile`) via `spawn`.
    - Windows: data path via IOCP (async file ops); admin ops (`CopyFileExW`) via `spawn`.

### 2) fadvise (access pattern hints)
- Linux (current): io_uring `Fadvise` opcode.
- macOS:
  - `posix_fadvise` exists but may be a stub; prefer `fcntl(F_RDADVISE, radvisory)` for read-ahead hints.
  - Invoke via `spawn` (administrative). Hints do not map to readiness; they don’t use kqueue directly.
- Windows:
  - No direct equivalent. Hints are typically provided at open (`FILE_FLAG_SEQUENTIAL_SCAN`/`FILE_FLAG_RANDOM_ACCESS`).
  - Design: best-effort no-op; document that callers should choose open flags for Windows.
  - Runtime integration:
    - macOS: not kqueue; run in `spawn`.
    - Windows: not IOCP; run in `spawn` or no-op.

### 3) fallocate (preallocation / hole punching / zero range)
- Linux (current): io_uring `Fallocate`.
- macOS:
  - Use `fcntl(F_PREALLOCATE, &fstore)` for space preallocation.
  - Map modes:
    - DEFAULT/KEEP_SIZE -> `F_PREALLOCATE` with/without extending the file; zeroing not guaranteed.
    - ZERO_RANGE/PUNCH_HOLE -> no direct; emulate with `ftruncate` + writes or return NotSupported.
- Windows:
  - Use `SetFileInformationByHandle` with `FILE_ALLOCATION_INFO` to preallocate space.
  - Runtime integration:
    - macOS: preallocation is administrative; run in `spawn` (not kqueue).
    - Windows: preallocation is administrative; run in `spawn` (not IOCP).
  - `SetEndOfFile` to extend logical file size.
  - Punch/zero range not generally supported; return NotSupported or emulate via writes.

### 4) xattr (extended attributes)
- Linux (current): io_uring `FGetXattr`/`FSetXattr`; list via `xattr` crate.
- macOS:
  - Darwin supports xattrs (`getxattr`, `setxattr`, `listxattr`, `removexattr`) and `f*` variants.
  - Implement using `xattr` crate or `libc` functions; no io_uring.
- Windows:
  - Runtime integration:
    - macOS: xattr calls are administrative; run in `spawn` (not kqueue).
    - Windows: NotSupported.
  - POSIX xattrs are not supported. NTFS EAs/ADS exist but semantics differ.
  - Design: return `ExtendedError::NotSupported("xattr unsupported on Windows")` for all xattr ops (feature still compiles but is a no-op on Windows).

### 5) Symlink / Hardlink
- Linux (current): io_uring `SymlinkAt` and `LinkAt`; `nix::fcntl::readlinkat`.
- macOS:
  - Use `nix`/`libc` or `std::os::unix::fs::{symlink, symlinkat}` and `std::fs::hard_link`.
- Windows:
  - Use `std::os::windows::fs::{symlink_file, symlink_dir}` and `std::fs::hard_link`.
  - Note: symlink creation may require developer mode/admin; document errors.
  - Runtime integration:
    - macOS: administrative; run in `spawn` (not kqueue).
    - Windows: administrative; run in `spawn` (not IOCP).

### 6) Directory operations (`DirectoryFd`) and *at variants
- Linux (current): `DirectoryFd` uses Unix fd; `mkdirat` via `nix`.
- macOS:
  - Maintain `DirectoryFd` as a Unix fd; `mkdirat` supported on modern macOS.
- Windows:
  - No raw fd; use raw HANDLEs via `AsRawHandle`.
  - Design: introduce `DirectoryHandle` on Windows (wrapper around `HANDLE` opened with `FILE_FLAG_BACKUP_SEMANTICS`). For now, provide path-based helpers and document reduced security vs *at.
  - Runtime integration:
    - macOS: directory mutations typically administrative; run in `spawn` or use `compio::fs` helpers.
    - Windows: administrative; run in `spawn` (not IOCP).

### 7) Metadata (statx, utimens)
- Linux (current): io_uring `Statx`; `nix` for utimensat/futimens.
- macOS:
  - Use `stat`/`fstat` via `std::fs::metadata` and `filetime` crate for times.
  - `futimens`/`utimensat` may be available; otherwise use `filetime` fallbacks.
- Windows:
  - Use `std::fs::metadata` for basic info; times via `filetime` crate (`SetFileTime`).
  - No `statx`; return equivalent `(atime, mtime)` via available APIs.
  - Runtime integration:
    - macOS: administrative; run in `spawn` (not kqueue).
    - Windows: administrative; run in `spawn` (not IOCP).

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
