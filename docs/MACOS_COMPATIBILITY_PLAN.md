# macOS Compatibility Plan for arsync

## Executive Summary

arsync is currently a Linux-only tool that relies heavily on `io_uring` (Linux kernel 5.6+). This plan outlines the work required to make arsync fully functional on macOS while maintaining performance advantages through platform-native optimizations.

**Current Status:** Linux-only (io_uring dependent)  
**Target Status:** Cross-platform (Linux + macOS, Windows future)  
**Estimated Effort:** 3-4 weeks of development + 1-2 weeks testing

---

## Table of Contents

1. [Current Platform Dependencies](#current-platform-dependencies)
2. [Cross-Platform Strategy](#cross-platform-strategy)
3. [Detailed Implementation Tasks](#detailed-implementation-tasks)
4. [macOS-Specific Optimizations](#macos-specific-optimizations)
5. [Testing Strategy](#testing-strategy)
6. [Documentation Updates](#documentation-updates)
7. [Performance Targets](#performance-targets)
8. [Timeline and Milestones](#timeline-and-milestones)

---

## Current Platform Dependencies

### Linux-Specific Features in Use

#### 1. **io_uring** (Most Critical)
- **Location:** Throughout codebase via `compio` runtime
- **Used For:** 
  - Async file I/O (read, write, open, close)
  - Metadata operations (statx)
  - Directory traversal
  - Symlink operations
  - Extended attributes
- **Impact:** Core architecture dependency
- **Replacement:** Use `compio` with kqueue backend on macOS

#### 2. **Linux-Specific Syscalls**

| Syscall | Current Usage | macOS Alternative | Priority |
|---------|--------------|-------------------|----------|
| `statx` | Metadata with nanosecond timestamps | `stat` (already has nsec) | High |
| `copy_file_range` | Efficient in-kernel copying | `fcopyfile`/`clonefile` | High |
| `fallocate` | File preallocation | `fcntl(F_PREALLOCATE)` | Medium |
| `posix_fadvise` | I/O hints | `fcntl(F_RDAHEAD)` / advisory only | Low |
| `splice` | Zero-copy pipe operations | Not available, use read/write | Medium |
| `getxattr`/`setxattr` | Extended attributes | Already has macOS impl in compio-fs-extended | Low |

#### 3. **Platform-Specific Code Patterns**

```rust
// Current Linux-only patterns:
#[cfg(target_os = "linux")]
use io_uring::...;

use std::os::unix::fs::{PermissionsExt, MetadataExt};

// fadvise Linux-only optimization
#[cfg(target_os = "linux")]
fadvise_hints(...);
```

---

## Cross-Platform Strategy

### Architecture Approach

Following the `compio-fs-extended` DESIGN.md model:

1. **Unified Public API** - Keep existing API surface unchanged
2. **Platform-Specific Backends** - Use conditional compilation for implementations
3. **Runtime Abstraction** - Leverage `compio`'s existing kqueue support on macOS
4. **Performance Preservation** - Use platform-native optimizations where available

### Directory Structure (Proposed)

```
src/
├── lib.rs              # Platform-agnostic public API
├── io_uring.rs         # Rename to io_operations.rs (platform-agnostic)
├── copy.rs             # Add platform-specific sections
├── metadata.rs         # Add platform-specific sections
└── platform/           # NEW: Platform-specific implementations
    ├── mod.rs          # Platform selection
    ├── linux.rs        # Linux-specific optimizations
    ├── macos.rs        # macOS-specific optimizations
    └── common.rs       # Shared Unix utilities

crates/compio-fs-extended/src/
├── lib.rs              # Platform-agnostic API
├── sys/                # Platform backends
│   ├── mod.rs          # Platform selection
│   ├── linux/          # io_uring implementations
│   │   ├── mod.rs
│   │   ├── copy.rs
│   │   ├── metadata.rs
│   │   └── ...
│   ├── darwin/         # kqueue implementations (TO BUILD OUT)
│   │   ├── mod.rs
│   │   ├── copy.rs     # clonefile, fcopyfile
│   │   ├── metadata.rs # stat, fcntl
│   │   └── ...
│   └── common/         # Shared Unix utilities
│       └── ...
```

---

## Detailed Implementation Tasks

### Task 1: Audit Current macOS Compatibility

**Goal:** Identify all Linux-specific code that needs platform abstraction.

**Subtasks:**
- [ ] Catalog all `#[cfg(target_os = "linux")]` blocks
- [ ] Identify all io_uring-specific operations
- [ ] List all Linux-specific syscalls in use
- [ ] Document current `compio-fs-extended` macOS support status
- [ ] Create compatibility matrix (Linux vs macOS feature parity)

**Output:** Detailed compatibility audit document

**Time Estimate:** 2-3 days

---

### Task 2: Complete compio-fs-extended macOS Implementations

**Goal:** Finish macOS backend in the extended filesystem crate.

**Current Status (from DESIGN.md):**
- ✅ xattr operations (mostly complete with macOS variants)
- ⚠️ fallocate (macOS F_PREALLOCATE stub exists)
- ❌ copy optimizations (clonefile/fcopyfile not implemented)
- ❌ File descriptor-based xattr (marked unimplemented)

**Subtasks:**

#### 2.1 File Copying Operations
```rust
// src: crates/compio-fs-extended/src/sys/darwin/copy.rs

// Implement macOS-native copy methods:
pub async fn clonefile(src: &Path, dst: &Path) -> Result<()> {
    // Use Apple's clonefile() for CoW copy on APFS
    // Falls back to fcopyfile() on non-APFS
}

pub async fn fcopyfile(src_fd: RawFd, dst_fd: RawFd, size: u64) -> Result<u64> {
    // Use fcopyfile() for efficient kernel copying
    // Equivalent to Linux copy_file_range
}
```

**Priority:** High  
**Time Estimate:** 3-4 days

#### 2.2 Metadata Operations
```rust
// Implement statx-equivalent using standard stat
pub async fn statx_impl(dir: &DirectoryFd, pathname: &OsStr) -> Result<FileMetadata> {
    // Use stat() for basic metadata (same fields as Linux statx)
    // Returns: size, mode, uid, gid, nlink, ino, dev, timestamps
    // Note: macOS stat already provides nanosecond timestamps
}
```

**Priority:** High  
**Time Estimate:** 1-2 days (simpler than planned)

#### 2.3 File Preallocation
```rust
// Complete F_PREALLOCATE implementation
pub async fn fallocate_darwin(fd: RawFd, offset: u64, len: u64) -> Result<()> {
    // Use fcntl(F_PREALLOCATE) with fstore_t
    // Helps reduce fragmentation on HFS+/APFS
}
```

**Priority:** Medium  
**Time Estimate:** 1-2 days

#### 2.4 File Descriptor-based xattr

**Note:** Extended attributes are handled separately from `FileMetadata`. The xattr module is independent.

```rust
// Implement FD-based xattr for TOCTOU safety
pub async fn fgetxattr(fd: RawFd, name: &CStr, value: &mut [u8]) -> Result<usize> {
    // Use fgetxattr() (available on macOS)
}

pub async fn fsetxattr(fd: RawFd, name: &CStr, value: &[u8]) -> Result<()> {
    // Use fsetxattr() (available on macOS)
}
```

**Priority:** High (for security parity with Linux)  
**Time Estimate:** 2 days

---

### Task 3: Replace io_uring-specific Code with Cross-Platform Abstractions

**Goal:** Abstract io_uring dependencies so `compio` can use kqueue on macOS.

#### 3.1 Rename and Refactor `io_uring.rs`

**Current:** `src/io_uring.rs` contains FileOperations struct  
**Target:** `src/io_operations.rs` (platform-agnostic)

**Changes:**
```rust
// Before (Linux-only):
pub struct FileOperations {
    buffer_size: usize,
}

// After (cross-platform):
pub struct FileOperations {
    buffer_size: usize,
    #[cfg(target_os = "linux")]
    io_uring_config: IoUringConfig,
    #[cfg(target_os = "macos")]
    kqueue_config: KqueueConfig,
}
```

**Time Estimate:** 1 day

#### 3.2 Abstract Copy Operations

**Files to Update:**
- `src/copy.rs` - Main copy logic
- `crates/compio-fs-extended/src/copy_file_range.rs`

**Strategy:**
```rust
// Platform-specific copy method selection
pub async fn copy_file_optimized(src: &Path, dst: &Path) -> Result<u64> {
    #[cfg(target_os = "linux")]
    {
        copy_file_range_linux(src, dst).await
    }
    
    #[cfg(target_os = "macos")]
    {
        // Try clonefile first (CoW), fall back to fcopyfile
        if try_clonefile(src, dst).await.is_ok() {
            return Ok(metadata(dst).await?.len());
        }
        fcopyfile_darwin(src, dst).await
    }
    
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        copy_file_fallback(src, dst).await
    }
}
```

**Time Estimate:** 3-4 days

#### 3.3 Abstract Metadata Operations

**Files to Update:**
- `src/metadata.rs`
- `src/directory.rs` (uses statx extensively)

**Strategy:**
```rust
// Unified metadata structure
pub struct ExtendedMetadata {
    pub size: u64,
    pub permissions: u32,
    pub uid: u32,
    pub gid: u32,
    pub atime: SystemTime,
    pub mtime: SystemTime,
    pub ctime: SystemTime,
    #[cfg(target_os = "macos")]
    pub btime: Option<SystemTime>, // Birth time (creation time)
    pub inode: u64,
    pub nlink: u64,
    pub dev: u64,
}

// Platform-specific implementation
#[cfg(target_os = "linux")]
async fn get_metadata_impl(path: &Path) -> Result<ExtendedMetadata> {
    // Use io_uring statx
}

#[cfg(target_os = "macos")]
async fn get_metadata_impl(path: &Path) -> Result<ExtendedMetadata> {
    // Use stat + getattrlist
}
```

**Time Estimate:** 2-3 days

---

### Task 4: Implement macOS-Native Optimizations

**Goal:** Achieve comparable performance to rsync on macOS using platform-native features.

#### 4.1 Copy-on-Write (CoW) with `clonefile()`

**Benefit:** Instant copying on APFS (Apple File System)

```rust
// Utility function to detect if filesystem supports clonefile
pub fn supports_clonefile(src: &Path, dst: &Path) -> Result<bool> {
    // Check if both paths are on APFS and same filesystem
    // This is separate from metadata operations
    let src_fs = statfs(src)?;
    let dst_fs = statfs(dst)?;
    
    Ok(src_fs.f_fstypename == "apfs" && src_fs.f_fsid == dst_fs.f_fsid)
}

// Attempt CoW copy with clonefile
pub async fn try_clonefile(src: &Path, dst: &Path) -> Result<()> {
    // Note: clonefile will fail if not on same APFS volume
    // We just try it and fall back on error
    unsafe {
        let ret = libc::clonefile(
            src_cstr.as_ptr(),
            dst_cstr.as_ptr(),
            0, // flags
        );
        if ret == 0 {
            return Ok(());
        }
    }
    Err(SyncError::NotSupported)
}
```

**Impact:** Can reduce copy time from minutes to milliseconds for large files on APFS  
**Time Estimate:** 2 days

#### 4.2 Kernel-Level Copy with `fcopyfile()`

**Benefit:** Zero-copy data transfer in kernel (like Linux `copy_file_range`)

```rust
pub async fn fcopyfile_darwin(src_fd: RawFd, dst_fd: RawFd) -> Result<u64> {
    spawn_blocking(move || {
        unsafe {
            let state = libc::copyfile_state_alloc();
            let ret = libc::fcopyfile(
                src_fd,
                dst_fd,
                state,
                libc::COPYFILE_ALL | libc::COPYFILE_METADATA,
            );
            libc::copyfile_state_free(state);
            
            if ret == 0 {
                // Get bytes copied from state
                Ok(bytes_copied)
            } else {
                Err(io::Error::last_os_error().into())
            }
        }
    }).await
}
```

**Impact:** 2-3x faster than read/write loops  
**Time Estimate:** 2-3 days

#### 4.3 File Preallocation with `F_PREALLOCATE`

**Benefit:** Reduce fragmentation, improve write performance

```rust
pub async fn preallocate_darwin(fd: RawFd, size: u64) -> Result<()> {
    spawn_blocking(move || {
        unsafe {
            let mut fstore = libc::fstore_t {
                fst_flags: libc::F_ALLOCATECONTIG,
                fst_posmode: libc::F_PEOFPOSMODE,
                fst_offset: 0,
                fst_length: size as i64,
                fst_bytesalloc: 0,
            };
            
            let mut ret = libc::fcntl(fd, libc::F_PREALLOCATE, &mut fstore);
            
            // If contiguous allocation failed, try non-contiguous
            if ret == -1 {
                fstore.fst_flags = libc::F_ALLOCATEALL;
                ret = libc::fcntl(fd, libc::F_PREALLOCATE, &mut fstore);
            }
            
            if ret == 0 {
                // Set file size
                libc::ftruncate(fd, size as i64);
                Ok(())
            } else {
                Err(io::Error::last_os_error().into())
            }
        }
    }).await
}
```

**Impact:** Better sequential write performance  
**Time Estimate:** 1-2 days

#### 4.4 Advisory Hints (Limited on macOS)

**Note:** macOS removed `posix_fadvise()`, but has limited alternatives:

```rust
#[cfg(target_os = "macos")]
pub fn apply_read_hints(fd: RawFd) -> Result<()> {
    // Enable read-ahead
    unsafe {
        libc::fcntl(fd, libc::F_RDAHEAD, 1);
    }
    Ok(())
}

#[cfg(target_os = "macos")]
pub fn apply_nocache_hint(fd: RawFd) -> Result<()> {
    // Hint that we won't reuse data (similar to POSIX_FADV_NOREUSE)
    unsafe {
        libc::fcntl(fd, libc::F_NOCACHE, 1);
    }
    Ok(())
}
```

**Impact:** Modest memory usage improvement  
**Time Estimate:** 1 day

---

### Task 5: Update Build System and Dependencies

**Goal:** Enable conditional compilation and macOS-specific dependencies.

#### 5.1 Update Cargo.toml

**Main Cargo.toml Changes:**

```toml
[target.'cfg(target_os = "macos")'.dependencies]
# macOS-specific libc features
libc = "0.2"

# Optional: Use macOS SDK bindings for advanced features
core-foundation = "0.9"
core-foundation-sys = "0.8"

[features]
# Platform-specific optimizations
linux-io-uring = []  # Explicit io_uring support
macos-native = []     # macOS optimizations (clonefile, fcopyfile)
```

**Time Estimate:** 1 day

#### 5.2 Update compio-fs-extended Cargo.toml

Already has macOS deps structure - verify completeness:

```toml
[target.'cfg(any(target_os = "macos", target_os = "ios"))'.dependencies]
nix = { version = "0.28", features = ["fs", "user", "time"] }
xattr = "1.0"

# May need to add:
# libc = { version = "0.2", features = ["extra_traits"] }
```

**Time Estimate:** 0.5 days

#### 5.3 Update Build Scripts

**File:** `build.rs`

```rust
fn main() {
    #[cfg(target_os = "macos")]
    {
        // Link against macOS frameworks if needed
        println!("cargo:rustc-link-lib=framework=CoreFoundation");
    }
    
    #[cfg(target_os = "linux")]
    {
        // Linux-specific build configuration
    }
}
```

**Time Estimate:** 0.5 days

---

### Task 6: Add macOS-Specific Tests and CI/CD

**Goal:** Ensure arsync works correctly on macOS with automated testing.

#### 6.1 Platform-Specific Test Matrix

**Expand:** `tests/` directory

```
tests/
├── common/           # Cross-platform test utilities
├── linux/            # NEW: Linux-specific tests
│   ├── io_uring_tests.rs
│   └── copy_file_range_tests.rs
├── macos/            # NEW: macOS-specific tests
│   ├── clonefile_tests.rs
│   ├── fcopyfile_tests.rs
│   ├── f_preallocate_tests.rs
│   └── apfs_cow_tests.rs
└── cross_platform/   # Tests that should pass on all platforms
    ├── copy_integration_tests.rs (moved from root)
    ├── metadata_tests.rs (moved from root)
    └── ...
```

**Time Estimate:** 3-4 days

#### 6.2 macOS-Specific Test Cases

**Key Tests to Add:**

```rust
// tests/macos/clonefile_tests.rs

#[cfg(target_os = "macos")]
#[compio::test]
async fn test_clonefile_on_apfs() {
    // Create test file on APFS
    // Verify instant CoW copy
    // Verify metadata preservation
    // Verify data integrity
}

#[cfg(target_os = "macos")]
#[compio::test]
async fn test_fcopyfile_fallback() {
    // Test fcopyfile when clonefile not available
    // Verify kernel-level copy
    // Compare performance to read/write
}

#[cfg(target_os = "macos")]
#[compio::test]
async fn test_preallocate_reduces_fragmentation() {
    // Write large file with preallocation
    // Write large file without preallocation
    // Compare fragmentation (if measurable)
}
```

**Time Estimate:** 2-3 days

#### 6.3 GitHub Actions CI for macOS

**File:** `.github/workflows/ci.yml`

```yaml
name: CI

on: [push, pull_request]

jobs:
  # Existing Linux job
  test-linux:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - name: Install Rust
        uses: actions-rs/toolchain@v1
        with:
          profile: minimal
          toolchain: stable
      - name: Run tests
        run: cargo test --all-features
      - name: Run benchmarks
        run: cargo bench --no-run

  # NEW: macOS job
  test-macos:
    runs-on: macos-latest
    steps:
      - uses: actions/checkout@v3
      - name: Install Rust
        uses: actions-rs/toolchain@v1
        with:
          profile: minimal
          toolchain: stable
      - name: Check system
        run: |
          sw_vers  # macOS version
          df -h    # Check for APFS
      - name: Run tests
        run: cargo test --all-features
      - name: Run macOS-specific tests
        run: cargo test --test macos --all-features
      - name: Run benchmarks
        run: cargo bench --no-run

  # Cross-platform compatibility check
  check-cross-platform:
    strategy:
      matrix:
        os: [ubuntu-latest, macos-latest]
        rust: [stable, beta]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v3
      - name: Install Rust
        uses: actions-rs/toolchain@v1
        with:
          profile: minimal
          toolchain: ${{ matrix.rust }}
      - name: Check compilation
        run: cargo check --all-features
      - name: Run clippy
        run: cargo clippy --all-features -- -D warnings
```

**Time Estimate:** 1-2 days

#### 6.4 Local Testing on macOS

**Create:** `scripts/test-macos.sh`

```bash
#!/bin/bash
# Local macOS testing script

set -e

echo "🍎 Running arsync macOS compatibility tests..."

# Check macOS version
echo "macOS version:"
sw_vers

# Check filesystem
echo -e "\nFilesystem info:"
df -h | head -n 2

# Run all tests
echo -e "\nRunning unit tests..."
cargo test

# Run macOS-specific tests
echo -e "\nRunning macOS-specific tests..."
cargo test --test macos

# Run integration tests
echo -e "\nRunning integration tests..."
cargo test --test copy_integration_tests
cargo test --test metadata_flag_tests

# Run rsync compatibility tests
echo -e "\nRunning rsync compatibility tests..."
cargo test --test rsync_compat

# Performance smoke test
echo -e "\nRunning performance smoke test..."
cargo run --release -- \
  --source /tmp/test-src \
  --destination /tmp/test-dst \
  -a --progress

echo "✅ All macOS tests passed!"
```

**Time Estimate:** 1 day

---

### Task 7: Update Documentation for Cross-Platform Support

**Goal:** Update all documentation to reflect macOS support.

#### 7.1 Update README.md

**Changes:**

```diff
- **Requirements**: Linux kernel 5.6+, Rust 1.70+
+ **Requirements**: 
+   - **Linux**: Kernel 5.6+, Rust 1.70+ (uses io_uring)
+   - **macOS**: macOS 10.15+, Rust 1.70+ (uses kqueue via compio)

## Platform Support

| Platform | Status | Async Backend | Native Optimizations |
|----------|--------|---------------|----------------------|
| **Linux** | ✅ Full Support | io_uring | copy_file_range, fallocate, statx, fadvise |
| **macOS** | ✅ Full Support | kqueue | clonefile (APFS CoW), fcopyfile, F_PREALLOCATE |
| **Windows** | 🚧 Planned | IOCP | (future) |

### macOS-Specific Features

arsync on macOS leverages platform-native optimizations:

- **clonefile()**: Instant Copy-on-Write for files on APFS
- **fcopyfile()**: Kernel-level file copying (similar to Linux copy_file_range)
- **F_PREALLOCATE**: File preallocation to reduce fragmentation
- **kqueue**: Event-driven async I/O via compio runtime
```

**Time Estimate:** 1 day

#### 7.2 Create macOS-Specific Documentation

**New File:** `docs/MACOS_SUPPORT.md`

```markdown
# macOS Support in arsync

## Overview

arsync provides full support for macOS using platform-native optimizations
through Apple's filesystem APIs and kqueue-based async I/O.

## Platform Optimizations

### 1. Copy-on-Write with clonefile()

On APFS (Apple File System), arsync uses `clonefile()` for instant file copying...

[Detailed explanation of APFS CoW, when it's used, fallback behavior]

### 2. Kernel Copying with fcopyfile()

For non-APFS or when clonefile isn't available...

### 3. File Preallocation with F_PREALLOCATE

...

## Performance Characteristics

[Benchmark results comparing arsync vs rsync on macOS]

## Limitations

1. No posix_fadvise (removed in macOS)...
2. ...
```

**Time Estimate:** 1-2 days

#### 7.3 Update DEVELOPER.md

**Add Section:** "Platform-Specific Development"

```markdown
## Platform-Specific Development

### Testing on macOS

To test macOS-specific functionality:
...

### Adding Platform-Specific Code

Follow these guidelines when adding platform-specific code:
...
```

**Time Estimate:** 0.5 days

---

### Task 8: Performance Benchmarking and Optimization

**Goal:** Ensure arsync on macOS performs as well as or better than rsync.

#### 8.1 Create macOS Benchmark Suite

**New File:** `benchmarks/macos_benchmarks.sh`

```bash
#!/bin/bash
# macOS-specific benchmarks

# Test 1: APFS clonefile performance
echo "=== Test 1: APFS clonefile (1GB file) ==="
time rsync -a /tmp/large-file /tmp/rsync-copy
time arsync -a --source /tmp/large-file --destination /tmp/arsync-copy

# Test 2: fcopyfile performance (cross-filesystem)
echo "=== Test 2: fcopyfile (cross-filesystem) ==="
...

# Test 3: Many small files
echo "=== Test 3: 10,000 small files ==="
...

# Test 4: Deep directory hierarchy
echo "=== Test 4: Deep hierarchy ==="
...
```

**Time Estimate:** 2 days

#### 8.2 Benchmark Against rsync

**Expected Results:**

| Workload | rsync | arsync (macOS) | Improvement |
|----------|-------|----------------|-------------|
| 1 GB file (APFS) | 5.2s | 0.05s | **104x faster** (CoW) |
| 1 GB file (cross-fs) | 5.2s | 2.1s | **2.5x faster** (fcopyfile) |
| 10k small files | 8.3s | 3.2s | **2.6x faster** (kqueue) |
| Deep hierarchy | 12.1s | 4.8s | **2.5x faster** (async) |

**Time Estimate:** 2-3 days

#### 8.3 Profile and Optimize

**Tools:**
- `cargo flamegraph` - CPU profiling
- Instruments.app (Xcode) - System-level profiling
- `dtruss` - System call tracing

**Focus Areas:**
- Minimize syscall overhead
- Optimize buffer sizes for macOS
- Tune kqueue event batch sizes
- Identify and eliminate unnecessary allocations

**Time Estimate:** 3-4 days

---

## macOS-Specific Optimizations

### Architecture: Clean Layer Separation

**Critical Principle:** `arsync/src/` NEVER touches platform-specific code directly.

```
arsync/src/          ← Application logic (platform-agnostic)
     ↓ uses
compio-fs-extended   ← Platform abstraction (handles all OS differences)
     ↓ calls
Linux/macOS APIs     ← Platform-specific syscalls
```

**See:** [ARCHITECTURE_PLATFORM_ABSTRACTION.md](ARCHITECTURE_PLATFORM_ABSTRACTION.md) for full details.

## Summary of Native Features to Use

| Feature | API | Benefit | Availability |
|---------|-----|---------|--------------|
| **Copy-on-Write** | `clonefile()` | Instant copy on APFS | macOS 10.12+ |
| **Kernel Copy** | `fcopyfile()` | 2-3x faster than read/write | macOS 10.5+ |
| **Preallocation** | `fcntl(F_PREALLOCATE)` | Reduce fragmentation | macOS 10.0+ |
| **Read-ahead** | `fcntl(F_RDAHEAD)` | Better sequential read | macOS 10.0+ |
| **No-cache Hint** | `fcntl(F_NOCACHE)` | Reduce memory pressure | macOS 10.5+ |
| **Async I/O** | kqueue via compio | Event-driven efficiency | macOS 10.3+ |
| **Extended Attr** | `fgetxattr`/`fsetxattr` | Metadata preservation | macOS 10.4+ |

---

## Testing Strategy

### Test Coverage Goals

- ✅ **Unit Tests:** 80%+ coverage on macOS-specific code
- ✅ **Integration Tests:** All existing tests pass on macOS
- ✅ **Cross-Platform Tests:** Identical behavior to Linux version
- ✅ **Compatibility Tests:** Match rsync behavior on macOS
- ✅ **Performance Tests:** Meet or exceed rsync performance

### Test Categories

1. **Functional Tests**
   - File copying (regular files, symlinks, directories)
   - Metadata preservation (permissions, ownership, timestamps, xattr)
   - Hardlink detection and preservation
   - Error handling and recovery

2. **Platform-Specific Tests**
   - APFS CoW with clonefile
   - fcopyfile fallback
   - F_PREALLOCATE behavior
   - kqueue event handling

3. **Performance Tests**
   - Large file copying
   - Many small files
   - Deep directory hierarchies
   - Cross-filesystem operations

4. **Compatibility Tests**
   - Byte-for-byte identical to rsync
   - Metadata matches rsync preservation
   - Flag behavior matches rsync

---

## Documentation Updates

### Files to Update

1. **README.md** - Add macOS to platform support table
2. **docs/DEVELOPER.md** - Add macOS development guidelines
3. **docs/MACOS_SUPPORT.md** - NEW: macOS-specific documentation
4. **docs/BENCHMARK_QUICK_START.md** - Add macOS benchmark instructions
5. **docs/RSYNC_COMPARISON.md** - Update with macOS performance data
6. **Cargo.toml** - Update description to mention cross-platform support

### Documentation Priorities

1. **User-Facing:**
   - Installation instructions for macOS (Homebrew?)
   - Platform-specific performance characteristics
   - Known limitations on macOS

2. **Developer-Facing:**
   - How to add platform-specific code
   - Testing guidelines for macOS
   - Debugging tips for kqueue/compio

---

## Performance Targets

### Minimum Acceptable Performance (vs rsync on macOS)

- ✅ **Large files:** >= 2x faster (fcopyfile)
- ✅ **Small files:** >= 2x faster (kqueue async)
- ✅ **APFS CoW:** >= 50x faster (clonefile)
- ✅ **Memory usage:** <= rsync
- ✅ **CPU usage:** <= rsync

### Stretch Goals

- 🎯 **Large files:** 3x faster
- 🎯 **Small files:** 3x faster
- 🎯 **APFS CoW:** 100x faster (instant)
- 🎯 **Memory usage:** 50% of rsync
- 🎯 **CPU usage:** 70% of rsync

---

## Timeline and Milestones

### Phase 1: Foundation (Week 1-2)

**Week 1:**
- [ ] Complete audit of Linux-specific code
- [ ] Design platform abstraction layer
- [ ] Set up macOS CI/CD pipeline
- [ ] Update build system for conditional compilation

**Week 2:**
- [ ] Implement core compio-fs-extended macOS backends
  - [ ] clonefile implementation
  - [ ] fcopyfile implementation
  - [ ] F_PREALLOCATE implementation
  - [ ] FD-based xattr completion

**Milestone 1:** Basic compilation on macOS ✅

---

### Phase 2: Core Functionality (Week 3-4)

**Week 3:**
- [ ] Abstract io_uring dependencies
- [ ] Implement platform-specific copy strategies
- [ ] Implement platform-specific metadata operations
- [ ] Update directory traversal for cross-platform

**Week 4:**
- [ ] Add macOS-specific optimizations
- [ ] Implement fallback mechanisms
- [ ] Handle platform differences in metadata
- [ ] Complete error handling for macOS

**Milestone 2:** Basic file copying works on macOS ✅

---

### Phase 3: Testing & Polish (Week 5-6)

**Week 5:**
- [ ] Write macOS-specific tests
- [ ] Run full test suite on macOS
- [ ] Fix bugs found in testing
- [ ] Validate rsync compatibility on macOS
- [ ] Run integration tests

**Week 6:**
- [ ] Performance benchmarking
- [ ] Profile and optimize
- [ ] Compare with rsync on macOS
- [ ] Tune kqueue/buffer parameters
- [ ] Final bug fixes

**Milestone 3:** Full functionality and performance parity ✅

---

### Phase 4: Documentation & Release (Week 7)

**Week 7:**
- [ ] Update all documentation
- [ ] Create macOS-specific guides
- [ ] Write migration guide for macOS users
- [ ] Create benchmark reports
- [ ] Prepare release notes

**Milestone 4:** Ready for macOS beta release 🎉

---

## Success Criteria

### Must Have (Blocking)

- ✅ Compiles and runs on macOS
- ✅ All existing tests pass on macOS
- ✅ rsync compatibility tests pass
- ✅ Performance >= rsync on standard workloads
- ✅ No data corruption or metadata loss
- ✅ CI/CD pipeline for macOS

### Should Have (Important)

- ✅ APFS CoW optimization working
- ✅ fcopyfile kernel copying working
- ✅ Documentation complete
- ✅ Performance benchmarks published
- ✅ macOS-specific tests comprehensive

### Nice to Have (Future)

- 🎯 Homebrew formula for easy installation
- 🎯 Integration with macOS spotlight
- 🎯 GUI application (already has winio support)
- 🎯 Support for macOS-specific metadata (tags, comments)

---

## Known Limitations and Tradeoffs

### macOS vs Linux Differences

| Feature | Linux | macOS | Impact |
|---------|-------|-------|--------|
| **posix_fadvise** | ✅ Full support | ❌ Not available | Minor - can use F_NOCACHE/F_RDAHEAD |
| **splice** | ✅ Available | ❌ Not available | Low - fallback to read/write |
| **statx** | ✅ Available | ❌ Not available | Low - use stat + extensions |
| **copy_file_range** | ✅ Available | ❌ Not available | None - fcopyfile is equivalent |
| **io_uring** | ✅ Available | ❌ Not available | None - kqueue is alternative |

### Accepted Tradeoffs

1. **No splice on macOS:** Use read/write loop instead (minimal performance impact)
2. **Limited fadvise:** macOS has F_RDAHEAD and F_NOCACHE but not full posix_fadvise
3. **No statx:** Use standard stat + platform extensions
4. **kqueue vs io_uring:** Different APIs but comparable performance

---

## Dependencies and Prerequisites

### Development Environment

**macOS Requirements:**
- macOS 10.15+ (Catalina or later)
- Xcode Command Line Tools
- Rust 1.70+
- APFS filesystem (for clonefile testing)

**Installation:**
```bash
# Install Xcode Command Line Tools
xcode-select --install

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Verify APFS
diskutil info / | grep "File System"
```

### Rust Crate Dependencies

**New macOS-specific Dependencies:**
```toml
[target.'cfg(target_os = "macos")'.dependencies]
libc = "0.2"
core-foundation = "0.9"  # For advanced macOS features
core-foundation-sys = "0.8"

[target.'cfg(target_os = "macos")'.dev-dependencies]
# macOS-specific testing tools (if needed)
```

---

## Risk Assessment

### High Risk Items

1. **compio kqueue backend maturity**
   - **Risk:** kqueue backend might have bugs or performance issues
   - **Mitigation:** Extensive testing, contribute fixes upstream if needed
   - **Fallback:** Can use tokio as alternative runtime if needed

2. **APFS clonefile edge cases**
   - **Risk:** clonefile might fail in unexpected scenarios
   - **Mitigation:** Comprehensive testing, robust fallback to fcopyfile
   - **Impact:** Medium - affects performance but not correctness

3. **Cross-platform metadata differences**
   - **Risk:** macOS and Linux handle some metadata differently
   - **Mitigation:** Document differences, handle gracefully
   - **Impact:** Low - mostly edge cases

### Medium Risk Items

1. **Performance on non-APFS filesystems**
   - **Risk:** Might not be faster than rsync on HFS+
   - **Mitigation:** Optimize fcopyfile path, tune buffer sizes
   - **Impact:** Medium - affects value proposition

2. **CI/CD costs**
   - **Risk:** macOS runners are expensive on GitHub Actions
   - **Mitigation:** Run macOS tests only on main/release branches
   - **Impact:** Low - budget consideration

### Low Risk Items

1. **Documentation comprehensiveness**
   - **Risk:** Might miss some macOS-specific details
   - **Mitigation:** Iterative improvement, user feedback
   - **Impact:** Low - can be updated post-release

---

## Future Enhancements

### Post-Initial Release

1. **Optimize for macOS 14+ (Sonoma)**
   - Investigate new APFS features
   - Leverage latest macOS APIs

2. **GUI Integration**
   - Complete winio macOS (AppKit) backend
   - Native macOS progress UI
   - Drag-and-drop support

3. **macOS-Specific Features**
   - Preserve macOS tags
   - Preserve Finder comments
   - Preserve Spotlight metadata
   - Handle resource forks (if needed)

4. **Advanced APFS Features**
   - Snapshot support
   - Cloning for backups
   - Space sharing awareness

---

## References

### macOS Documentation

- [Apple File System Reference](https://developer.apple.com/documentation/applefilesystem)
- [fcopyfile man page](https://www.unix.com/man-page/mojave/3/fcopyfile/)
- [clonefile man page](https://www.unix.com/man-page/mojave/2/clonefile/)
- [kqueue and kevent man pages](https://man.freebsd.org/cgi/man.cgi?kqueue)
- [F_PREALLOCATE fcntl](https://opensource.apple.com/source/xnu/xnu-4570.1.46/bsd/sys/fcntl.h.auto.html)

### compio Documentation

- [compio GitHub](https://github.com/compio-rs/compio)
- [compio Documentation](https://docs.rs/compio)
- [compio kqueue backend](https://github.com/compio-rs/compio/tree/main/compio-driver/src/kqueue)

### Related Projects

- [rsync source code](https://github.com/WayneD/rsync) - Reference implementation
- [rclone](https://github.com/rclone/rclone) - Cross-platform sync (Go)
- [fcp](https://github.com/Svetlitski/fcp) - Fast copy utility (Rust)

---

## Appendix: Platform Capability Matrix

| Capability | Linux | macOS | Windows | Notes |
|------------|-------|-------|---------|-------|
| **Async I/O** | io_uring | kqueue | IOCP | compio abstracts |
| **Fast Copy** | copy_file_range | fcopyfile | CopyFile2 | Kernel-level |
| **CoW Copy** | FICLONE | clonefile | - | Instant copy |
| **Prealloc** | fallocate | F_PREALLOCATE | - | Reduce frag |
| **Fadvise** | posix_fadvise | F_RDAHEAD | - | Limited on macOS |
| **Metadata** | statx | stat | GetFileInformationByHandleEx | Different APIs |
| **Xattr** | getxattr | getxattr | ADS | Similar APIs |
| **Symlinks** | symlinkat | symlinkat | CreateSymbolicLink | Different perms |
| **Hardlinks** | linkat | linkat | CreateHardLink | Similar |
| **ACLs** | getxattr | acl_get_fd | GetSecurityInfo | Different models |

---

## Contact and Support

**For Questions:**
- Open an issue on GitHub
- Tag with `macos` label
- Reference this plan document

**For Contributing:**
- See [DEVELOPER.md](DEVELOPER.md)
- Follow platform-specific guidelines
- Write tests for macOS-specific code

---

*Last Updated: 2025-10-21*  
*Status: Planning Phase*  
*Owner: arsync team*

