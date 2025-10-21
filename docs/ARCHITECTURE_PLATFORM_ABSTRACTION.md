# Platform Abstraction Architecture

## Overview

arsync follows a clean layered architecture where platform-specific code is contained within the `compio-fs-extended` crate, and the main `arsync` codebase operates exclusively through platform-agnostic APIs.

## Architecture Layers

```
┌─────────────────────────────────────────────────────────────┐
│  arsync/src/ - Application Logic                            │
│  • Never directly accesses platform-specific APIs           │
│  • Uses compio-fs-extended abstractions                     │
│  • Platform-agnostic business logic                         │
└─────────────────────┬───────────────────────────────────────┘
                      │
                      ▼
┌─────────────────────────────────────────────────────────────┐
│  compio-fs-extended - Platform Abstraction Layer            │
│  • Provides unified API across platforms                    │
│  • Handles platform-specific syscalls                       │
│  • Exposes platform-specific fields conditionally           │
└─────────────────────┬───────────────────────────────────────┘
                      │
          ┌───────────┴───────────┐
          ▼                       ▼
┌──────────────────┐    ┌──────────────────┐
│  Linux           │    │  macOS           │
│  • io_uring      │    │  • kqueue        │
│  • statx         │    │  • stat          │
│  • fadvise       │    │  • fcopyfile     │
│  • fallocate     │    │  • clonefile     │
└──────────────────┘    └──────────────────┘
```

## Key Principles

### 1. Separation of Concerns

**`compio-fs-extended`** (Platform Layer):
- Contains all `#[cfg(target_os = "...")]` for platform-specific implementations
- Exposes unified APIs that work across platforms
- Handles platform-specific syscalls and features
- Returns platform-agnostic types with optional platform fields

**`arsync/src/`** (Application Layer):
- Never directly calls platform-specific syscalls
- Never imports `std::os::linux` or `std::os::macos` modules (except MetadataExt for common Unix fields)
- Uses only `compio-fs-extended` abstractions
- Conditionally compiles features that don't exist on all platforms, but through abstract APIs

### 2. Platform-Specific Metadata Pattern

Following Rust's standard library pattern (`std::fs::Metadata` + `MetadataExt` traits):

```rust
// compio-fs-extended/src/metadata.rs
pub struct FileMetadata {
    // Common Unix fields (all platforms)
    pub size: u64,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    // ... more common fields
    
    // Platform-specific fields (conditionally compiled)
    #[cfg(target_os = "linux")]
    pub attributes: Option<u64>,
    
    #[cfg(target_os = "macos")]
    pub flags: Option<u32>,
}

impl FileMetadata {
    // Abstraction method - handles platform differences internally
    pub fn from_std_metadata(metadata: &std::fs::Metadata) -> Self {
        // Platform-specific code here
    }
}
```

**Usage in arsync/src/:**
```rust
// Clean - no platform-specific code!
let metadata = compio_fs_extended::FileMetadata::from_std_metadata(&std_metadata);

// Can access common fields directly
let size = metadata.size;
let mode = metadata.mode;

// Platform-specific fields available when compiled for that platform
#[cfg(target_os = "macos")]
if let Some(flags) = metadata.flags {
    // Handle macOS flags
}
```

## Examples

### ✅ Good: Using Abstraction

```rust
// arsync/src/directory.rs

// Good: Using compio-fs-extended abstraction
let metadata = compio_fs_extended::FileMetadata::from_std_metadata(&compio_metadata);
```

### ❌ Bad: Direct Platform Access (Violation)

```rust
// arsync/src/directory.rs

// BAD: Don't do this in arsync/src/!
#[cfg(target_os = "macos")]
let flags = compio_metadata.st_flags();  // ❌ Platform-specific in application layer

// Instead, use abstraction:
let metadata = FileMetadata::from_std_metadata(&compio_metadata);
// Platform-specific fields are already extracted
```

### ✅ Good: Conditional Platform Features

```rust
// arsync/src/copy.rs

// Good: Using abstracted API, conditionally compiled feature
#[cfg(target_os = "linux")]
{
    use compio_fs_extended::{Fadvise, FadviseAdvice};
    extended_file.fadvise(FadviseAdvice::NoReuse, 0, size).await?;
}
// Gracefully skipped on platforms without fadvise
```

## Platform-Specific Feature Support Matrix

| Feature | Linux | macOS | API Location |
|---------|-------|-------|--------------|
| **File Metadata** | statx | stat | `compio-fs-extended::FileMetadata` |
| **Async I/O** | io_uring | kqueue | `compio` runtime |
| **File Copy** | copy_file_range | fcopyfile/clonefile | `compio-fs-extended::copy` (planned) |
| **Preallocation** | fallocate | F_PREALLOCATE | `compio-fs-extended::Fallocate` |
| **I/O Hints** | fadvise | F_RDAHEAD/F_NOCACHE | `compio-fs-extended::Fadvise` |
| **Extended Attributes** | getxattr | getxattr | `compio-fs-extended::XattrOps` |
| **Symlinks** | symlinkat | symlinkat | `compio-fs-extended::SymlinkOps` |
| **Hardlinks** | linkat | linkat | `compio-fs-extended::HardlinkOps` |

## Benefits of This Architecture

### 1. **Clean Separation**
- Application logic in `arsync/src/` is platform-agnostic
- All platform complexity isolated in `compio-fs-extended`
- Easy to reason about and maintain

### 2. **Testability**
- Can mock `compio-fs-extended` for testing
- Application logic doesn't need platform-specific test branches
- Platform-specific code tested separately

### 3. **Portability**
- Adding new platform (e.g., Windows) only requires changes in `compio-fs-extended`
- Application code requires minimal changes
- Clear contract between layers

### 4. **Maintainability**
- Platform-specific bugs isolated to one crate
- Easier to track platform differences
- Clearer code reviews

### 5. **Reusability**
- `compio-fs-extended` can be used by other projects
- Well-defined API surface
- Documentation focuses on abstractions, not platform details

## Guidelines for Contributors

### When Adding New Features

1. **Determine Layer:**
   - Is this a platform-specific syscall/API? → `compio-fs-extended`
   - Is this business logic? → `arsync/src/`

2. **Check Existing Abstractions:**
   - Does `compio-fs-extended` already provide an API?
   - Can you extend an existing trait?

3. **Add Platform Support:**
   - Implement for all supported platforms when possible
   - Use `Option<>` for platform-specific fields
   - Document platform differences clearly

4. **Test Both Layers:**
   - Test platform abstraction in `compio-fs-extended/tests/`
   - Test application logic in `arsync/tests/`

### Code Review Checklist

- [ ] No `#[cfg(target_os = "...")]` in `arsync/src/` for platform-specific implementations
- [ ] No direct `std::os::linux` or `std::os::macos` imports in `arsync/src/` (except Unix common traits)
- [ ] Platform-specific code uses `compio-fs-extended` APIs
- [ ] New platform-specific fields are `Option<>` types
- [ ] Documentation explains platform differences
- [ ] Tests cover both platforms

## File Organization

```
arsync/
├── src/                           # Application layer (platform-agnostic)
│   ├── lib.rs
│   ├── directory.rs               # No platform-specific code
│   ├── copy.rs                    # Uses compio-fs-extended APIs
│   └── metadata.rs                # Uses compio-fs-extended types
│
└── crates/
    └── compio-fs-extended/        # Platform abstraction layer
        ├── src/
        │   ├── lib.rs
        │   ├── metadata.rs        # Platform-specific implementations
        │   ├── fadvise.rs         # Linux-specific (gracefully unavailable on macOS)
        │   ├── fallocate.rs       # Platform-specific implementations
        │   ├── copy.rs            # Platform-specific copy methods (planned)
        │   └── xattr.rs           # Platform-specific xattr handling
        │
        └── tests/                 # Platform-specific tests
            ├── linux/
            └── macos/
```

## Current Status

### ✅ Completed
- FileMetadata with platform-specific fields
- Clean abstraction for metadata construction
- Removed platform-specific code from arsync/src/directory.rs
- Established architecture pattern

### 🚧 In Progress
- macOS implementation of statx_impl
- macOS copy optimizations (clonefile, fcopyfile)

### 📋 Planned
- macOS F_PREALLOCATE implementation
- Platform-specific copy method selection
- Cross-platform test suite

---

**Last Updated:** 2025-10-21  
**Architecture Version:** 1.0  
**Status:** Implemented and enforced

