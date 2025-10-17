# Add cross-platform scaffolding for compio-fs-extended (Linux, macOS, Windows)

## Summary
- Introduces platform-gated implementations and fallbacks to extend `crates/compio-fs-extended` beyond Linux, aligning with how `compio::fs` supports kqueue (macOS) and IOCP (Windows).
- Adds a Windows-first implementation focus: async streaming copy via IOCP-friendly owned-buffer `read_at`/`write_at`, stubs or safe fallbacks for non-portable ops, and CI matrix entries for Windows and macOS.

## Key changes
- Cargo cfg-gating: moves io_uring/nix to Linux-only; adds `windows-sys` and `compio-io`; restricts xattr to Unix targets.
- copy: keeps Linux `copy_file_range`; adds cross-platform fallback using `compio-io` `AsyncReadAt`/`AsyncWriteAt` (IOCP/kqueue driven); reports unsupported on Windows for raw syscall.
- xattr: Linux FD path via io_uring; Unix path helpers retained; macOS FD path marked todo; Windows returns NotSupported.
- metadata: `statx_at` on Linux; std metadata fallback on macOS/Windows.
- device: Unix-only; Windows returns NotSupported.
- directory: fd-based helpers on Unix; Windows `create_directory` via std spawn.
- ownership: Unix `fchown/chown/preserve_ownership_from`; Windows NotSupported.
- symlink: Linux io_uring path; macOS uses `std::os::unix::fs::symlink`; Windows tries `symlink_file` then `symlink_dir`, reading via `std::fs::read_link`.
- Tests: gated Unix-only tests; Linux build passes with Windows/mac code paths present.
- CI: updates to run tests on Linux/mac, build on Windows.

## Docs
- Adds `crates/compio-fs-extended/DESIGN.md`:
  - Detailed macOS kqueue and Windows IOCP integration strategies
  - Capability catalogs/matrices (what kqueue/IOCP support vs compio::fs ops)
  - Per-operation designs and fallbacks
  - Testing & CI plan
- Adds `crates/compio-fs-extended/STATUS.md` summarizing ask/done/todo.

## Rationale
- Maintains a stable public API while routing platform-specific capabilities under `#[cfg]` and providing best-effort fallbacks.
- Leverages compio’s backends properly: readiness-driven streaming via kqueue/IOCP; one-shot administrative syscalls via lightweight spawn.

## Remaining work
- macOS: implement FD-based xattr or keep path-only; refine F_PREALLOCATE handling.
- Windows: expand tests; refine preallocation semantics; robust symlink behavior and documentation.
- Optional: move per-OS code into `src/sys/{linux,darwin,windows}`.

## Test plan
- CI matrix: ubuntu-latest (full tests), macos-latest (common+mac), windows-latest (build-only initially).
- Local: verify `cargo check` cross-platform gates; run Linux tests; verify gating on non-Linux targets.
