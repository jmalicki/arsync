# DirectoryFd + io_uring Architecture Plan

**Goal:** Eliminate TOCTOU vulnerabilities and syscall overhead by using directory file descriptors and io_uring operations throughout.

## Current Problems (2025-10-18)

### Issue 1: Path-based operations (TOCTOU vulnerable)
```bash
# Current - TOCTOU vulnerable:
statx(AT_FDCWD, "/src/dir/file.txt", ...)  ❌ Can be replaced with symlink
openat(AT_FDCWD, "/src/dir/file.txt", ...) ❌ Might open different file

# Target - TOCTOU-safe:
dirfd = openat(AT_FDCWD, "/src/dir", O_DIRECTORY)  ✓ Directory pinned
statx(dirfd, "file.txt", ...)                      ✓ Relative to pinned dir
openat(dirfd, "file.txt", ...)                     ✓ Same file guaranteed
```

### Issue 2: Direct syscalls instead of io_uring
```bash
# Current - blocking syscalls:
statx(...)  ← Direct syscall, blocks thread
openat(...) ← Direct syscall, blocks thread

# Target - async io_uring:
io_uring_enter([STATX(dirfd, "file")])  ← Async, non-blocking
io_uring_enter([OPENAT(dirfd, "file")]) ← Async, non-blocking
```

### Issue 3: Redundant metadata calls
```bash
# Current - 4x statx calls per file:
statx(...)  # 1. Check file size for parallel decision
statx(...)  # 2. Get timestamps before copy
statx(...)  # 3. src_file.metadata() during copy
statx(...)  # 4. Worker thread metadata check

# Target - 1x io_uring statx per file:
io_uring STATX(dirfd, "file") → full metadata once
```

## Target Architecture

### Phase 1: Extend compio-fs-extended to return full metadata

**Current:**
```rust
// compio-fs-extended/src/metadata.rs line 235
pub(crate) async fn statx_impl(
    dir: &DirectoryFd,
    pathname: &str,
) -> Result<(SystemTime, SystemTime)> {
    // Only returns timestamps! ❌
}
```

**Target:**
```rust
pub struct FileMetadata {
    pub size: u64,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub nlink: u64,
    pub ino: u64,
    pub dev: u64,
    pub accessed: SystemTime,
    pub modified: SystemTime,
    pub created: Option<SystemTime>,
}

pub(crate) async fn statx_impl(
    dir: &DirectoryFd,
    pathname: &str,
) -> Result<FileMetadata> {
    // Return full metadata from statx buffer ✓
    let statx_buf = result.1.statxbuf;
    Ok(FileMetadata {
        size: statx_buf.stx_size,
        mode: statx_buf.stx_mode,
        uid: statx_buf.stx_uid,
        gid: statx_buf.stx_gid,
        nlink: statx_buf.stx_nlink,
        ino: statx_buf.stx_ino,
        dev: statx_buf.stx_dev_major as u64 * 256 + statx_buf.stx_dev_minor as u64,
        accessed: ...,
        modified: ...,
        created: ...,
    })
}
```

### Phase 2: Add DirectoryFd::open_file_at()

```rust
// compio-fs-extended/src/directory.rs
impl DirectoryFd {
    /// Open a file relative to this directory using io_uring
    pub async fn open_file_at(
        &self,
        name: &str,
        read: bool,
        write: bool,
        create: bool,
        truncate: bool,
    ) -> Result<compio::fs::File> {
        // TODO: Use io_uring OPENAT when compio supports it
        // For now, use spawn with openat(2)
        let dir_fd = self.as_raw_fd();
        let flags = /* build flags */;
        let fd = spawn_blocking(|| unsafe {
            libc::openat(dir_fd, name_cstr.as_ptr(), flags, 0o644)
        }).await?;
        
        // SAFETY: We own this fd
        Ok(unsafe { compio::fs::File::from_raw_fd(fd) })
    }
}
```

### Phase 3: Change directory traversal to use DirectoryFd

**Current:**
```rust
// src/directory.rs
async fn traverse_and_copy_directory_iterative(
    src: &Path,  // ❌ Path-based
    dst: &Path,
    // ...
) -> Result<()> {
    let metadata = ExtendedMetadata::new(src).await?;  // ❌ statx(AT_FDCWD, path)
    // ...
}
```

**Target:**
```rust
async fn traverse_and_copy_directory_iterative(
    src_parent: &DirectoryFd,  // ✓ DirectoryFd
    src_name: &str,
    dst_parent: &DirectoryFd,
    dst_name: &str,
    // ...
) -> Result<()> {
    // Open source directory relative to parent
    let src_dir = src_parent.open_directory(src_name).await?;  // ✓ openat(parentfd, name)
    
    // Get metadata using io_uring
    let metadata = src_dir.statx(".").await?;  // ✓ io_uring STATX(dirfd, ".")
    
    // Read directory entries
    for entry in src_dir.read_dir().await? {
        let entry_metadata = src_dir.statx(&entry.name).await?;  // ✓ io_uring STATX
        
        if entry_metadata.is_file() {
            // Open file relative to directory
            let src_file = src_dir.open_file_at(&entry.name, true, false, false, false).await?;
            let dst_file = dst_dir.open_file_at(&entry.name, false, true, true, true).await?;
            
            // Copy with FDs, no more path lookups!
            copy_file_with_fds(src_file, dst_file, entry_metadata).await?;
        }
    }
}
```

### Phase 4: Update copy_file() signature

**Current:**
```rust
pub async fn copy_file(
    src: &Path,        // ❌ Path
    dst: &Path,        // ❌ Path
    metadata_config: &MetadataConfig,
    parallel_config: &ParallelCopyConfig,
    dispatcher: Option<&'static Dispatcher>,
) -> Result<()> {
    let file_size = compio::fs::metadata(src).await?.len();  // ❌ Redundant statx
    let (accessed, modified) = get_precise_timestamps(src).await?;  // ❌ Another statx
    // ...
}
```

**Target:**
```rust
pub async fn copy_file_with_dirfd(
    src_dir: &DirectoryFd,     // ✓ DirectoryFd
    src_name: &str,
    dst_dir: &DirectoryFd,     // ✓ DirectoryFd
    dst_name: &str,
    src_metadata: FileMetadata, // ✓ Already have it!
    metadata_config: &MetadataConfig,
    parallel_config: &ParallelCopyConfig,
    dispatcher: Option<&'static Dispatcher>,
) -> Result<()> {
    // Use metadata we already have - NO redundant statx!
    let file_size = src_metadata.size;
    let accessed = src_metadata.accessed;
    let modified = src_metadata.modified;
    
    // Open files relative to dirfd
    let src_file = src_dir.open_file_at(src_name, true, false, false, false).await?;
    let dst_file = dst_dir.open_file_at(dst_name, false, true, true, true).await?;
    
    // Copy with FDs
    // ...
}
```

## Implementation Order

1. **Extend compio-fs-extended** (crates/compio-fs-extended/src/metadata.rs)
   - Add `FileMetadata` struct
   - Update `statx_impl()` to return full metadata
   - Add `DirectoryFd::statx()` method

2. **Add DirectoryFd::open_file_at()** (crates/compio-fs-extended/src/directory.rs)
   - Implement openat(2) wrapper
   - Return `compio::fs::File`

3. **Update ExtendedMetadata** (src/directory.rs)
   - Change from `compio::fs::Metadata` to `compio_fs_extended::FileMetadata`
   - Use DirectoryFd-based construction

4. **Refactor directory traversal** (src/directory.rs)
   - Change function signatures to use DirectoryFd
   - Pass DirectoryFd down the call chain
   - Use DirectoryFd::statx() for metadata

5. **Update copy_file()** (src/copy.rs)
   - Add dirfd-based version
   - Remove redundant metadata calls
   - Keep path-based version as deprecated fallback

6. **Update tests**
   - Add dirfd-based tests
   - Verify TOCTOU resistance
   - Measure syscall reduction

## Expected Benefits

### Security
- ✅ TOCTOU-safe: directory pinned before operations
- ✅ No symlink attacks between stat and open
- ✅ Matches rsync security model

### Performance
- ✅ 75% reduction in statx calls (4 → 1 per file)
- ✅ io_uring statx (async, non-blocking)
- ✅ io_uring openat (when compio adds support)
- ✅ Reduced kernel path resolution overhead

### Correctness
- ✅ Hardlink detection more reliable (ino/dev from same stat)
- ✅ No race conditions in metadata
- ✅ Atomic view of directory structure

## Verification

### strace before:
```bash
statx(AT_FDCWD, "/src/dir/file", ...) = 0  # Call 1
statx(AT_FDCWD, "/src/dir/file", ...) = 0  # Call 2
statx(AT_FDCWD, "/src/dir/file", ...) = 0  # Call 3
statx(AT_FDCWD, "/src/dir/file", ...) = 0  # Call 4
openat(AT_FDCWD, "/src/dir/file", ...) = 7
openat(AT_FDCWD, "/dst/dir/file", ...) = 8
```

### strace after:
```bash
openat(AT_FDCWD, "/src/dir", O_DIRECTORY) = 6  # Parent dir
io_uring_enter([STATX(6, "file")])            # One statx via io_uring
openat(6, "file", O_RDONLY) = 7                # Relative open
openat(7_dst, "file", O_WRONLY|O_CREAT) = 8    # Relative open
```

### Syscall reduction:
- **statx calls:** 4 → 1 (75% reduction)
- **Using io_uring:** Direct syscalls → io_uring ops
- **TOCTOU-safe:** AT_FDCWD → dirfd

## Notes

- This is a significant architectural change but necessary for correctness and performance
- We already have the building blocks in `compio-fs-extended`
- Need to extend them to return full metadata
- All changes are backward compatible (keep path-based APIs deprecated)

