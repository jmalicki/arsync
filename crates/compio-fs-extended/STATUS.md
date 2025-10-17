# compio-fs-extended cross-platform effort

## What you asked
- Investigate extending `crates/compi-fs-extended` to support Linux, macOS (kqueue), and Windows (IOCP), mirroring how `compio::fs` supports multiple backends.
- Create a new branch for the investigation and produce a research & design document describing architecture, macOS kqueue usage, Windows IOCP usage, and per-operation designs.
- Add a testing strategy and CI, preferably with Windows runners.
- Begin implementation starting with Windows.

## What I delivered so far

- Branch: `investigate/compi-fs-extended-cross-platform` (active)

- Research & Design
  - Added `crates/compio-fs-extended/DESIGN.md` covering:
    - Audit of Linux-centric code and deps.
    - Cross-platform module strategy with cfg gating.
    - Detailed macOS kqueue integration: which ops are kqueue-driven vs one-shot syscalls, pseudocode for kqueue-backed streaming.
    - Detailed Windows IOCP integration: overlapped I/O via `compio::fs` for streaming, admin ops via spawn.
    - Darwin kqueue capability catalog and matrix mapping compio::fs vs kqueue support.
    - Windows IOCP capability catalog and matrix mapping compio::fs vs IOCP support.
    - Testing strategy and CI plan (matrix across Linux/macOS/Windows).

- CI
  - Updated `.github/workflows/ci.yml` to run tests on `ubuntu-latest` and `macos-latest`, and to build on `windows-latest` (can enable tests later when ready).

- Implementation (Windows-first scaffolding)
  - Cargo cfg-gating:
    - Moved io_uring and nix under Linux-only.
    - Added `windows-sys` for Windows and `compio-io` for owned-buffer async I/O.
    - Kept `xattr` limited to Unix targets.
  - copy: Implemented cross-platform fallback path using `compio-io` owned-buffer `AsyncReadAt`/`AsyncWriteAt` (IOCP/kqueue friendly). On Unix still tries `copy_file_range`; Windows declares `copy_file_range` unsupported and uses fallback.
  - xattr: Linux FD-based path via io_uring remains; Unix path-based helpers remain; macOS FD-based path marked not-yet-implemented; Windows returns NotSupported for xattr.
  - metadata: `statx_at` on Linux via io_uring; macOS/Windows fallback to std metadata.
  - device: Unix-only; Windows returns NotSupported for special files.
  - directory: Raw `as_raw_fd` and `mkdirat` on Unix; Windows `create_directory` via `std::fs::create_dir` in a spawned task.
  - ownership: Unix `fchown/chown/preserve_ownership_from` intact; Windows returns NotSupported.
  - symlink: Linux io_uring path kept under cfg; macOS creation via `std::os::unix::fs::symlink`; Windows creation tries `symlink_file` then `symlink_dir`, with read via `std::fs::read_link`.
  - Gated Unix-only tests and ensured `cargo check` passes on Linux with Windows/macOS code paths present.

## What remains

- Mac
  - Implement macOS FD-based xattr methods or decide to keep only path-based variants in this crate.
  - Consider Darwin-specific fallocate nuances (smarter `F_PREALLOCATE` modes and error mapping).
  - Optional: Add VNODE-based watcher examples in a separate module/crate.

- Windows
  - Expand tests for Windows (IOCP) and enable `cargo test` on `windows-latest` in CI once passing.
  - Polish preallocation behavior (mode mapping, size semantics) and add tests.
  - Improve symlink handling (file vs dir detection up front) and add more robust error messages around developer mode requirements.

- General
  - Introduce `src/sys/{linux,darwin,windows}` directory structure if we further split implementations.
  - Documentation polish: per-OS examples for each operation in README.
  - Ensure feature flags (`xattr`, `metrics`, `logging`) are consistently respected cross-platform.
  - Add compile-time `#[cfg]` doc examples to show OS-specific code paths.

## Quick status
- compiles on Linux with Windows/mac gates in place.
- CI includes Windows build job and macOS test job.
- DESIGN.md contains full kqueue/IOCP matrices and test/CI plan.
