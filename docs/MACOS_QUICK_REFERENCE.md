# macOS Compatibility - Quick Reference

> **Full Plan:** See [MACOS_COMPATIBILITY_PLAN.md](MACOS_COMPATIBILITY_PLAN.md)

## Current Status

- ❌ **Not functional on macOS** (Linux-only, io_uring dependent)
- 🚧 **Partial macOS support** in `compio-fs-extended` crate
- 🎯 **Target:** Full macOS support with native optimizations

## Key Blockers

1. **io_uring dependency** - Linux-only, need to use kqueue via compio
2. **Linux-specific syscalls** - Need macOS equivalents:
   - `statx` → `stat` + extensions
   - `copy_file_range` → `fcopyfile`
   - `fallocate` → `fcntl(F_PREALLOCATE)`
   - `posix_fadvise` → `fcntl(F_NOCACHE/F_RDAHEAD)`

## macOS Native Optimizations to Implement

| Feature | API | Benefit |
|---------|-----|---------|
| **CoW Copy** | `clonefile()` | **Instant** file copy on APFS |
| **Kernel Copy** | `fcopyfile()` | 2-3x faster than read/write |
| **Preallocation** | `F_PREALLOCATE` | Reduce fragmentation |
| **Async I/O** | kqueue (via compio) | Event-driven efficiency |

## Work Breakdown

### Phase 1: Foundation (2 weeks)
- Audit Linux-specific code
- Complete `compio-fs-extended` macOS backend
- Set up macOS CI/CD

### Phase 2: Core Functionality (2 weeks)
- Abstract io_uring dependencies
- Implement platform-specific copy/metadata
- Add macOS optimizations

### Phase 3: Testing (2 weeks)
- Write macOS-specific tests
- Run full test suite
- Performance benchmarking vs rsync

### Phase 4: Documentation (1 week)
- Update all docs for cross-platform
- Create macOS-specific guides
- Prepare release

**Total Estimate:** 6-7 weeks

## Expected Performance (vs rsync on macOS)

| Workload | Expected Improvement |
|----------|---------------------|
| 1GB file on APFS | **100x faster** (CoW) |
| 1GB cross-filesystem | **2-3x faster** (kernel copy) |
| 10k small files | **2-3x faster** (async kqueue) |

## Quick Start for Development

### Prerequisites
```bash
# macOS 10.15+ with APFS
xcode-select --install
rustup update
```

### Current Status Check
```bash
# This will FAIL on macOS currently:
cargo build
# Error: io_uring not available

# To start fixing:
# 1. Add platform abstractions
# 2. Implement macOS backends
# 3. Update dependencies
```

### Key Files to Modify

**Priority 1 (Core):**
- `src/io_uring.rs` → rename to `io_operations.rs`, add platform abstraction
- `src/copy.rs` → add macOS copy methods
- `src/metadata.rs` → add macOS metadata methods
- `crates/compio-fs-extended/src/sys/darwin/` → complete implementations

**Priority 2 (Build):**
- `Cargo.toml` → add macOS dependencies
- `.github/workflows/ci.yml` → add macOS jobs

**Priority 3 (Testing):**
- `tests/macos/` → add macOS-specific tests

## Testing Checklist

- [ ] Compiles on macOS
- [ ] Basic copy works
- [ ] Metadata preserved correctly
- [ ] Symlinks handled correctly
- [ ] Hardlinks detected and preserved
- [ ] Extended attributes work
- [ ] clonefile optimization works on APFS
- [ ] fcopyfile fallback works
- [ ] Performance >= rsync
- [ ] All rsync compatibility tests pass
- [ ] CI/CD pipeline green

## Next Actions

1. **Start with audit** - Use `grep` to find all Linux-specific code
2. **Complete compio-fs-extended** - Implement missing macOS operations
3. **Test compilation** - Get it building on macOS first
4. **Add tests** - Ensure correctness before optimizing
5. **Benchmark** - Compare with rsync, iterate

## Resources

- **Full Plan:** [MACOS_COMPATIBILITY_PLAN.md](MACOS_COMPATIBILITY_PLAN.md)
- **compio docs:** https://docs.rs/compio
- **macOS fcopyfile:** `man fcopyfile`
- **macOS clonefile:** `man clonefile`
- **kqueue:** `man kqueue`

---

*Created: 2025-10-21*  
*See full plan for detailed implementation steps*

