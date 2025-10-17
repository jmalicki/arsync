# Project Status - compio-fs-extended cross-platform

Branch: investigate/compi-fs-extended-cross-platform
Date: 2025-10-16

## Summary
- Audit complete; cross-platform R&D/design captured in `crates/compi-fs-extended/DESIGN.md`.
- macOS/Windows runtime integration documented (kqueue/IOCP matrices).
- Windows scaffolding implemented:
  - cfg-gated Linux-only code; Windows stubs for xattr/device/ownership.
  - IOCP-friendly streaming copy fallback via `compio-io` owned-buffer APIs.
  - CI updated to include windows-latest (build), macos-latest (tests).

## Next Steps
- macOS: implement FD-based xattr where useful; keep fallbacks via path APIs.
- Windows: enable and expand Windows test suite; refine preallocation mode mapping.
- General: continue extracting per-OS backends under `src/sys/{linux,darwin,windows}`.
