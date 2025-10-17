# Symlink Metadata Handling

## Design Principle

**Files & Directories**: Use FD-based operations (f* variants)
- One `open()` call → get fd → all metadata ops on that fd
- `fchown`, `fchmod`, `fgetxattr`, `fsetxattr`, `futimens`
- Efficient, TOCTOU-safe, minimal syscalls

**Symlinks**: Cannot use FD-based operations
- `File::open(symlink)` opens the TARGET, not the symlink!
- Must use path-based l* variants that don't follow symlinks
- `lchown`, `lgetxattr`, `lsetxattr`, `lutimes`/`lutimensat(AT_SYMLINK_NOFOLLOW)`

## Current Status

### ✅ Implemented (Symlink-aware)

**Xattrs** - Complete with l* variants:
- `lget_xattr_at_path()` - Linux: `lgetxattr`, macOS: `getxattr(XATTR_NOFOLLOW)`
- `lset_xattr_at_path()` - Linux: `lsetxattr`, macOS: `setxattr(XATTR_NOFOLLOW)`
- `llist_xattr_at_path()` - Linux: `llistxattr`, macOS: `listxattr(XATTR_NOFOLLOW)`
- `lremove_xattr_at_path()` - Linux: `lremovexattr`, macOS: `removexattr(XATTR_NOFOLLOW)`

**Symlink Operations**:
- `symlinkat()` - Create symlinks (already doesn't follow)
- `readlinkat()` - Read symlink target (already doesn't follow)

### ❌ Missing (Need l* variants)

**Ownership** - Only has fd-based operations:
- ✅ `fchown()` - fd-based (good for files/dirs)
- ✅ `chown()` - path-based but FOLLOWS symlinks
- ❌ **MISSING**: `lchown()` - path-based, doesn't follow symlinks

**Timestamps** - Only has fd-based operations:
- ✅ `futimens_fd()` - fd-based (good for files/dirs)
- ✅ `utimensat()` - path-based but uses `FollowSymlink` flag
- ❌ **MISSING**: `lutimensat()` or `utimensat(AT_SYMLINK_NOFOLLOW)`

**Permissions**:
- Note: Symlink permissions don't matter on Linux (always 0777)
- macOS/BSD: Would need `lchmod()` but it's not widely supported
- Recommendation: Skip symlink permission preservation (not meaningful)

## Implementation Plan

### 1. Add lchown to ownership.rs

```rust
/// Change symlink ownership (doesn't follow symlinks)
#[cfg(unix)]
pub async fn lchown_at_path(path: &Path, uid: u32, gid: u32) -> Result<()> {
    // Linux & macOS both support lchown
    nix::unistd::lchown(path, Some(uid), Some(gid))
}
```

### 2. Add lutimensat to metadata.rs

```rust
/// Set symlink timestamps (doesn't follow symlinks)
#[cfg(unix)]
pub async fn lutimensat(path: &Path, accessed: SystemTime, modified: SystemTime) -> Result<()> {
    // Use utimensat with AT_SYMLINK_NOFOLLOW
    nix::sys::stat::utimensat(
        None,  // dirfd
        path,
        &atime,
        &mtime,
        UtimensatFlags::NoFollowSymlink,  // Don't follow symlinks!
    )
}
```

### 3. Add symlink metadata preservation to arsync

Update `copy_symlink()` in `src/directory.rs`:

```rust
async fn copy_symlink(src: &Path, dst: &Path, config: &MetadataConfig) -> Result<()> {
    // 1. Read and create symlink (already done)
    // ...
    
    // 2. Preserve symlink metadata using l* functions
    if config.should_preserve_ownership() {
        preserve_symlink_ownership(src, dst).await?;
    }
    
    if config.should_preserve_timestamps() {
        preserve_symlink_timestamps(src, dst).await?;
    }
    
    if config.should_preserve_xattrs() {
        preserve_symlink_xattrs(src, dst).await?;
    }
}
```

## Why This Matters

**For file sync tools:**
- Symlink metadata is part of the file system state
- Accurate sync requires preserving it
- Examples:
  - Git repositories use symlink mtimes for change detection
  - Backup tools need to preserve exact symlink state
  - Security scanners check symlink ownership

**Current arsync behavior:**
- ✅ Copies symlink target correctly
- ❌ Loses all symlink metadata (ownership, timestamps, xattrs)
- This breaks faithful directory synchronization

## Testing Strategy

See `tests/symlink_metadata_tests.rs`:
- Tests prove `File::open(symlink)` opens the target
- Tests prove fd-based operations can't work on symlinks
- Tests document needed l* functions

See `crates/compio-fs-extended/tests/xattr_symlink_tests.rs`:
- Tests prove l* xattr functions work correctly
- Tests prove regular functions follow symlinks

## References

- `man 2 lchown` - Change symlink ownership
- `man 2 lutimes` - Change symlink timestamps (deprecated, use utimensat)
- `man 2 utimensat` - Change timestamps with AT_SYMLINK_NOFOLLOW
- `man 2 lgetxattr` - Get symlink xattr (Linux)
- macOS: Uses same function names with XATTR_NOFOLLOW/XATTR_NOFOLLOW flags

