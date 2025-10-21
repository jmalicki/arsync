# macOS Support Plan

## Goal

Get arsync compiling and passing `cargo test --lib` on macOS with minimal changes.

## Current Status

- ❌ Does not compile on macOS (io_uring Linux-only)
- ❌ Tests don't run

## Architecture Principle

**arsync/src/** NEVER touches platform-specific code.  
**compio-fs-extended/** handles ALL platform differences.

```
arsync/src/          ← Application logic (platform-agnostic)
     ↓ uses
compio-fs-extended   ← Platform abstraction (handles all OS-specific code)
     ↓ calls
Linux/macOS syscalls ← Platform-specific implementations
```

## Phase 1: Minimal Compilation (This PR)

**Goal:** Get `cargo build --lib` and `cargo test --lib` working on macOS

### Required Changes

#### 1. Add Platform-Specific Fields to FileMetadata

**File:** `crates/compio-fs-extended/src/metadata.rs`

```rust
pub struct FileMetadata {
    // Common Unix fields
    pub size: u64,
    pub mode: u32,
    // ... existing fields ...
    pub created: Option<SystemTime>,
    
    // Platform-specific fields
    #[cfg(target_os = "linux")]
    pub attributes: Option<u64>,
    #[cfg(target_os = "linux")]
    pub attributes_mask: Option<u64>,
    
    #[cfg(target_os = "macos")]
    pub flags: Option<u32>,
    #[cfg(target_os = "macos")]
    pub generation: Option<u32>,
}
```

#### 2. Implement macOS statx_impl

**File:** `crates/compio-fs-extended/src/metadata.rs`

Add macOS version using `fstatat()`:

```rust
#[cfg(target_os = "macos")]
pub(crate) async fn statx_impl(
    dir: &DirectoryFd,
    pathname: &OsStr,
) -> Result<FileMetadata> {
    // Use fstatat() instead of io_uring statx
    // Extract st_flags, st_gen
}
```

#### 3. Make statx_full() Available on macOS

**File:** `crates/compio-fs-extended/src/directory.rs`

Change from `#[cfg(target_os = "linux")]` to `#[cfg(unix)]`

#### 4. Make fadvise No-Op on macOS

**File:** `crates/compio-fs-extended/src/fadvise.rs`

Add:
```rust
#[cfg(not(target_os = "linux"))]
pub async fn fadvise(...) -> Result<()> {
    Ok(())  // No-op
}
```

#### 5. Fix Platform-Specific Guards

**File:** `src/directory/types.rs`

Update `ExtendedMetadata::from_dirfd()` to work on macOS

### Success Criteria

- ✅ `cargo build --lib` succeeds on macOS
- ✅ `cargo test --lib` passes on macOS
- ✅ No platform-specific code in `src/`
- ✅ All platform code in `compio-fs-extended`

**Time Estimate:** 4-6 hours

## Phase 2: Clean Up API Duplication (Future PR)

**After Phase 1 merges:**

### Goal: Remove duplicate structures, use compio-fs-extended types directly

1. **Remove ExtendedMetadata wrapper**
   - Use `compio_fs_extended::FileMetadata` directly in arsync/src
   - Eliminate double-indirection (`metadata.metadata.field` → `metadata.field`)
   - Clean up API surface

2. **Consolidate metadata handling**
   - Single source of truth for file metadata
   - Remove redundant conversion layers

**Time Estimate:** 1-2 days

## Phase 3: Advanced macOS Features (Future PR)

**After Phase 2:**

1. Implement macOS copy optimizations (clonefile, fcopyfile)
2. Implement F_PREALLOCATE
3. Add macOS CI/CD
4. Performance benchmarks

**Time Estimate:** 2-3 weeks

## References

- **Full Plan:** See detailed MACOS_COMPATIBILITY_PLAN.md (comprehensive)
- **Architecture:** See ARCHITECTURE_PLATFORM_ABSTRACTION.md

---

*Updated: 2025-10-21*  
*Phase: 1 (Compilation)*

