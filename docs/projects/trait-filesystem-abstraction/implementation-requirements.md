# Implementation Requirements & Best Practices

## Critical: Do Not Regress Existing Quality

The local backend implementation **must maintain or improve** the security and performance characteristics of the existing codebase.

## Existing Code to Study

**Before implementing, review**:
- `src/directory/mod.rs` - Current directory operations
- `src/directory/types.rs` - Directory abstractions
- `src/metadata.rs` - Metadata handling
- `crates/compio-fs-extended/` - All secure operations

## Non-Negotiable Requirements

### 1. File Descriptors, Not Paths

**ALWAYS**:
- ✅ Use `DirectoryFd` from compio-fs-extended
- ✅ Use `*at` syscalls (openat, fstatat, etc.)
- ✅ Pin directories once, operate on relative paths

**NEVER**:
- ❌ Use path-based operations after initial open
- ❌ Resolve paths multiple times
- ❌ Use `std::fs` for file operations

```rust
// ✅ CORRECT - Uses DirectoryFd
let dir_fd = DirectoryFd::open(base_path).await?;
let file = dir_fd.open_file_at("relative/path", ...).await?;
let metadata = dir_fd.statx_full("relative/path").await?;

// ❌ WRONG - Path-based operations
let file = std::fs::File::open(full_path)?;  // TOCTOU vulnerable!
let metadata = std::fs::metadata(full_path)?;  // Extra stat syscall!
```

### 2. TOCTOU Safety

**Requirement**: All operations must be TOCTOU-safe (Time-of-Check-Time-of-Use)

**How**:
- Open directory once → get DirectoryFd
- All subsequent operations use that DirectoryFd
- Use `O_NOFOLLOW` flag (prevents symlink attacks)
- Never resolve paths again

**Example from `crates/compio-fs-extended/src/directory.rs`**:
```rust
// This is TOCTOU-safe because:
// 1. Directory is pinned with DirectoryFd
// 2. openat uses the fd, not path resolution
// 3. O_NOFOLLOW prevents symlink attacks
let fd = unsafe { libc::openat(dir_fd, name_cstr.as_ptr(), flags) };
```

**What to avoid**:
```rust
// ❌ TOCTOU vulnerable - path could change between checks
if std::fs::metadata(path)?.is_file() {
    let file = std::fs::File::open(path)?;  // Path might have changed!
}
```

### 3. Minimize Syscalls

**Requirement**: stat() once per file, reuse metadata

**Strategy**:
- Get metadata during directory walk
- Store in entry structure
- Reuse for all decisions

**Current pattern in `src/directory/mod.rs`**:
```rust
pub struct DirectoryEntry {
    pub path: PathBuf,
    pub metadata: Metadata,  // ← Fetched once during walk
    // ... other fields
}

// Later use cached metadata
if entry.metadata.is_file() {  // No syscall!
    copy_file(&entry.path, &entry.metadata).await?;
}
```

**What to avoid**:
```rust
// ❌ Multiple stat calls for same file
let is_file = entry.path.is_file();  // stat #1
let size = entry.path.metadata()?.size();  // stat #2
let mtime = entry.path.metadata()?.modified();  // stat #3
```

### 4. Use compio-fs-extended, Not std::fs

**Requirement**: All I/O must use compio for async and io_uring benefits

**Use**:
- ✅ `compio::fs::File`
- ✅ `compio_fs_extended::DirectoryFd`
- ✅ `compio_fs_extended::FileMetadata`
- ✅ `compio_fs_extended::directory::read_dir`

**Do NOT use**:
- ❌ `std::fs::File`
- ❌ `std::fs::read_dir`
- ❌ `std::fs::metadata`
- ❌ Direct syscalls without compio integration

**Example**:
```rust
// ✅ CORRECT - Uses compio
let file = compio::fs::File::open(path).await?;
let (bytes, buffer) = file.read_at(buffer, offset).await?;

// ❌ WRONG - Blocks async runtime
let file = std::fs::File::open(path)?;  // Blocking!
let bytes = std::io::Read::read(&mut file, buffer)?;  // Blocking!
```

### 5. Secure *at Syscalls Everywhere

**Current usage in crates/compio-fs-extended**:

| Operation | Secure Syscall | Method |
|-----------|---------------|---------|
| Open file | `openat(2)` | `DirectoryFd::open_file_at()` |
| Get metadata | `statx(2)` | `DirectoryFd::statx_full()` |
| Create directory | `mkdirat(2)` | `DirectoryFd::create_directory()` |
| Set times | `utimensat(2)` | `DirectoryFd::lutimensat()` |
| Set permissions | `fchmodat(2)` | `DirectoryFd::lfchmodat()` |
| Set ownership | `fchownat(2)` | `DirectoryFd::lfchownat()` |
| Create symlink | `symlinkat(2)` | `DirectoryFd::symlinkat()` |
| Read symlink | `readlinkat(2)` | `DirectoryFd::readlinkat()` |

**All operations must use these, not path-based equivalents**

### 6. Proper Async Integration

**Requirement**: All I/O must be truly async

**Compio patterns**:
```rust
// ✅ Buffer ownership pattern
let (result, buffer) = file.read_at(buffer, offset).await;
match result.0 {
    Ok(bytes_read) => { /* use buffer */ },
    Err(e) => { /* handle error */ },
}

// ❌ Don't block in spawn_blocking unnecessarily
compio::runtime::spawn_blocking(move || {
    std::fs::read(path)  // Only if absolutely required!
}).await?;
```

**When to use spawn_blocking**:
- Only for operations not available in compio
- Minimize - prefer compio operations
- Current examples: `mkdirat`, `openat` (until compio adds them)

### 7. Error Handling Patterns

**From existing code**:
```rust
// Preserve context in errors
let file = dir_fd.open_file_at(path, ...).await
    .map_err(|e| SyncError::FileSystem(
        format!("Failed to open {}: {}", path, e)
    ))?;

// Use ? operator for propagation
let metadata = file.metadata().await?;
```

## Performance Requirements

### 1. Directory Walking

**Current performance** (from `src/directory/mod.rs`):
- Single pass through directory tree
- Metadata fetched during walk
- No redundant stat calls

**Must maintain**:
- O(n) time for n entries
- O(1) metadata lookups per entry
- Streaming iterator (don't accumulate all entries)

### 2. File Operations

**Current** (from existing implementations):
- io_uring for all I/O
- Zero-copy where possible
- Batched operations

**Must maintain**:
- io_uring usage via compio
- Buffer reuse
- Efficient read/write loops

## Code Review Checklist for Implementation PRs

### Security:
- [ ] All operations use DirectoryFd (no path-based ops)
- [ ] All file opens use `*at` syscalls
- [ ] `O_NOFOLLOW` used where appropriate
- [ ] No TOCTOU vulnerabilities
- [ ] No symlink attacks possible

### Performance:
- [ ] stat() called once per file
- [ ] Metadata cached and reused
- [ ] No redundant syscalls
- [ ] Streaming (not accumulating) where possible

### Async/io_uring:
- [ ] All I/O uses compio, not std::fs
- [ ] No blocking operations in async context
- [ ] spawn_blocking only when necessary
- [ ] Buffer ownership pattern followed

### Code Quality:
- [ ] Uses compio-fs-extended abstractions
- [ ] Follows existing patterns
- [ ] Proper error context
- [ ] No regressions in functionality

## Examples from Existing Code

### Example 1: Secure Directory Walking (src/directory/mod.rs)

Study the existing implementation pattern:
```rust
pub async fn walk_directory(
    dir_fd: &DirectoryFd,
    relative_path: &Path,
) -> Result<Vec<DirectoryEntry>> {
    // 1. Open directory using DirectoryFd
    let entries = compio_fs_extended::directory::read_dir(dir_fd.path()).await?;
    
    // 2. For each entry, get metadata ONCE
    for entry_result in entries {
        let entry = entry_result?;
        let name = entry.file_name();
        
        // 3. Use statx for full metadata (one syscall)
        let metadata = dir_fd.statx_full(&name).await?;
        
        // 4. Store in structure for reuse
        let dir_entry = DirectoryEntry {
            path: relative_path.join(&name),
            metadata,  // ← Cached for all future use
            // ...
        };
        
        // 5. Recurse using openat for subdirectories
        if metadata.is_dir() {
            let subdir = dir_fd.open_directory_at(&name).await?;
            // Continue with new DirectoryFd
        }
    }
}
```

**Key points**:
- DirectoryFd throughout
- Metadata fetched once with statx
- openat for subdirectories
- No path-based operations

### Example 2: Secure File Operations (compio-fs-extended)

```rust
// From crates/compio-fs-extended/src/directory.rs
pub async fn open_file_at(
    &self,
    pathname: &std::ffi::OsStr,
    read: bool,
    write: bool,
    create: bool,
    truncate: bool,
) -> Result<compio::fs::File> {
    let dir_fd = self.as_raw_fd();
    
    // Build flags
    let mut flags = if read && write { O_RDWR }
                    else if write { O_WRONLY }
                    else { O_RDONLY };
    
    if create { flags |= O_CREAT; }
    if truncate { flags |= O_TRUNC; }
    flags |= O_CLOEXEC;
    flags |= O_NOFOLLOW;  // ← TOCTOU protection!
    
    // Use openat (secure *at syscall)
    let fd = unsafe { libc::openat(dir_fd, path_cstr.as_ptr(), flags, 0o644) };
    
    // Wrap in compio::fs::File for async I/O
    Ok(unsafe { compio::fs::File::from_raw_fd(fd) })
}
```

**Key points**:
- openat with DirectoryFd
- O_NOFOLLOW for security
- Returns compio::fs::File
- Error handling with context

## Migration Checklist

When implementing local backend:

1. **Before writing code**:
   - [ ] Read all of `src/directory/mod.rs`
   - [ ] Read all of `crates/compio-fs-extended/src/directory.rs`
   - [ ] Understand DirectoryFd patterns
   - [ ] Understand *at syscall usage

2. **During implementation**:
   - [ ] Use DirectoryFd for all operations
   - [ ] Verify no std::fs usage
   - [ ] Verify no path-based operations
   - [ ] Cache metadata, don't refetch
   - [ ] Use compio for all I/O

3. **Testing**:
   - [ ] Test TOCTOU safety (symlink attacks)
   - [ ] Verify syscall counts (strace/dtrace)
   - [ ] Performance comparison with existing code
   - [ ] Security audit

## References

### Existing Code:
- `src/directory/mod.rs` - Main directory operations
- `src/directory/types.rs` - Type definitions
- `crates/compio-fs-extended/src/directory.rs` - DirectoryFd implementation
- `crates/compio-fs-extended/src/metadata.rs` - Secure metadata operations

### Documentation:
- `SYMLINK_METADATA.md` - Symlink handling
- `docs/safety/` - Security documentation

### Man Pages:
- `openat(2)` - Secure file opening
- `statx(2)` - Extended file metadata
- `utimensat(2)` - Set file timestamps
- `fchmodat(2)` - Change file mode
- `fchownat(2)` - Change file ownership

