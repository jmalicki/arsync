# Platform Abstraction Architecture

## Core Principle

**arsync/src/** NEVER touches platform-specific code.  
**compio-fs-extended/** is THE platform abstraction layer.

## Architecture Layers

```
┌─────────────────────────────────────────────┐
│  arsync/src/ - Application Logic            │
│  • Platform-agnostic business logic         │
│  • Uses compio-fs-extended APIs only        │
└─────────────┬───────────────────────────────┘
              │
              ▼
┌─────────────────────────────────────────────┐
│  compio-fs-extended - Abstraction Layer     │
│  • Unified cross-platform API               │
│  • Handles all platform differences         │
│  • Platform-specific fields                 │
└─────────────┬───────────────────────────────┘
              │
      ┌───────┴────────┐
      ▼                ▼
┌──────────┐    ┌──────────┐
│  Linux   │    │  macOS   │
│ io_uring │    │ kqueue   │
│  statx   │    │  stat    │
└──────────┘    └──────────┘
```

## Pattern: Platform-Specific Metadata

Following Rust's `std::fs::Metadata` + `MetadataExt` pattern:

```rust
// compio-fs-extended/src/metadata.rs
pub struct FileMetadata {
    // Common Unix fields
    pub size: u64,
    pub mode: u32,
    // ... more common fields
    
    // Platform-specific fields (conditionally compiled)
    #[cfg(target_os = "linux")]
    pub attributes: Option<u64>,
    
    #[cfg(target_os = "macos")]
    pub flags: Option<u32>,
}
```

## Rules

### ✅ Good: Platform Code in compio-fs-extended

```rust
// crates/compio-fs-extended/src/metadata.rs
#[cfg(target_os = "macos")]
pub(crate) async fn statx_impl(...) -> Result<FileMetadata> {
    // macOS-specific implementation
}
```

### ❌ Bad: Platform Code in arsync/src

```rust
// src/directory/types.rs
#[cfg(target_os = "macos")]  // ❌ VIOLATION
let flags = metadata.st_flags();
```

### ✅ Good: Using Abstraction

```rust
// src/directory/types.rs
let metadata = dir.statx_full(filename).await?;  // ✅ Platform-agnostic API
```

## Benefits

1. **Separation of concerns** - Platform complexity isolated
2. **Testability** - Application logic platform-agnostic
3. **Maintainability** - Clear ownership of platform code
4. **Reusability** - compio-fs-extended can be used elsewhere

---

*Version: 1.0*  
*Status: Enforced*

