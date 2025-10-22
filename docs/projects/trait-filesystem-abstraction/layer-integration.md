# Layer Integration: How Components Work Together

## The Key Principle

**Higher layers use lower layers as building blocks.**

The rsync protocol should **not** implement its own file I/O or directory walking. Instead, it should use the well-designed local filesystem abstractions with secure patterns like `DirectoryFd` and `*at` syscalls.

## Architecture Layers

```
┌──────────────────────────────────────────────────────────┐
│  Layer 4: High-Level Sync Operations                     │
│  - sync_directory()                                       │
│  - Orchestrates backends, progress reporting             │
└────────────┬─────────────────────────────────────────────┘
             │
      ┌──────┴───────────────┐
      │                      │
┌─────▼───────────┐   ┌──────▼──────────────┐
│ Layer 3a:       │   │ Layer 3b:           │
│ Local Backend   │   │ Protocol Backend    │
│                 │   │                     │
│ Uses Layer 1-2  │   │ Uses Layer 1-2      │ ← KEY!
│ for local ops   │   │ for local ops       │
└─────┬───────────┘   └──────┬──────────────┘
      │                      │
      │     ┌────────────────┘
      │     │
┌─────▼─────▼──────────────────────────────────┐
│ Layer 2: Shared Filesystem Operations        │
│ - walk_tree() using DirectoryFd              │
│ - read_file() using secure *at syscalls      │
│ - write_file() with DirectoryFd              │
│ - preserve_metadata()                        │
└────────────┬──────────────────────────────────┘
             │
┌────────────▼──────────────────────────────────┐
│ Layer 1: Low-Level Primitives                 │
│ - DirectoryFd (compio-fs-extended)            │
│ - AsyncFile, AsyncDirectory traits            │
│ - Secure *at syscalls (fstatat, openat, etc.)│
└───────────────────────────────────────────────┘
```

## How rsync Protocol Uses Local Abstractions

### Anti-Pattern: Duplicate File I/O

```rust
// BAD: rsync protocol reimplements file I/O
impl RsyncProtocol {
    async fn send_file(&mut self, path: &Path) -> Result<()> {
        // ❌ Direct fs::read - no DirectoryFd, no security
        let content = fs::read(path)?;
        
        // ❌ Duplicates logic that exists in local backend
        self.protocol.send_data(&content).await?;
    }
}
```

### Correct Pattern: Reuse Local Abstractions

```rust
// GOOD: rsync protocol uses local filesystem abstractions
impl RsyncProtocol {
    async fn send_file(
        &mut self, 
        dir_fd: &DirectoryFd,  // ← Secure DirectoryFd
        relative_path: &str,
    ) -> Result<()> {
        // ✅ Use local filesystem abstraction
        let file = dir_fd.open_file_at(relative_path, true, false, false, false).await?;
        
        // ✅ Read using secure *at operations
        let mut buffer = vec![0u8; 64 * 1024];
        let mut offset = 0;
        
        loop {
            let (bytes_read, buf) = file.read_at(buffer, offset).await?;
            if bytes_read == 0 { break; }
            
            // Protocol-specific: Send data
            self.protocol.send_data(&buf[..bytes_read]).await?;
            
            offset += bytes_read as u64;
            buffer = buf;
        }
        
        Ok(())
    }
}
```

## Shared Filesystem Operations (Layer 2)

These are the **building blocks** used by both backends:

### 1. Secure Directory Walking

```rust
// Shared by both local sync and rsync protocol
pub struct SecureTreeWalker {
    root: DirectoryFd,  // ← Uses DirectoryFd!
}

impl SecureTreeWalker {
    pub async fn new(path: &Path) -> Result<Self> {
        Ok(Self {
            root: DirectoryFd::open(path).await?,
        })
    }
    
    pub async fn walk(&self) -> impl Stream<Item = Result<FileEntry>> {
        // Implementation uses DirectoryFd and *at syscalls
        // Safe from TOCTOU, symlink attacks, etc.
        
        // Recursive walk using openat for subdirectories
        self.walk_recursive(&self.root, Path::new("")).await
    }
    
    async fn walk_recursive(
        &self,
        dir_fd: &DirectoryFd,
        relative_path: &Path,
    ) -> impl Stream<Item = Result<FileEntry>> {
        // Use compio-fs-extended's read_dir
        let entries = compio_fs_extended::directory::read_dir(dir_fd.path()).await?;
        
        for entry_result in entries {
            let entry = entry_result?;
            let name = entry.file_name();
            
            // Use statx_full for metadata (secure *at operation)
            let metadata = dir_fd.statx_full(&name).await?;
            
            yield FileEntry {
                relative_path: relative_path.join(&name),
                metadata,
                dir_fd: dir_fd.clone(),  // Keep DirectoryFd for file access
            };
            
            // Recurse into subdirectories using openat
            if metadata.is_dir() {
                let subdir = dir_fd.open_directory_at(&name).await?;
                // Recurse with new DirectoryFd
            }
        }
    }
}
```

### 2. Secure File Reading

```rust
// Shared by both backends
pub async fn read_file_content(
    dir_fd: &DirectoryFd,
    relative_path: &str,
) -> Result<Vec<u8>> {
    // ✅ Uses secure openat
    let file = dir_fd.open_file_at(
        relative_path.as_ref(),
        true,   // read
        false,  // write
        false,  // create
        false,  // truncate
    ).await?;
    
    // Read entire file
    let metadata = file.metadata().await?;
    let size = metadata.len();
    
    let mut content = Vec::with_capacity(size as usize);
    let mut offset = 0;
    let mut buffer = vec![0u8; 64 * 1024];
    
    loop {
        let (bytes_read, buf) = file.read_at(buffer, offset).await?;
        if bytes_read == 0 { break; }
        
        content.extend_from_slice(&buf[..bytes_read]);
        offset += bytes_read as u64;
        buffer = buf;
    }
    
    Ok(content)
}
```

### 3. Secure File Writing

```rust
// Shared by both backends
pub async fn write_file_content(
    dir_fd: &DirectoryFd,
    relative_path: &str,
    content: &[u8],
) -> Result<()> {
    // ✅ Uses secure openat with O_NOFOLLOW
    let file = dir_fd.open_file_at(
        relative_path.as_ref(),
        false,  // read
        true,   // write
        true,   // create
        true,   // truncate
    ).await?;
    
    // Write entire content
    let mut offset = 0;
    while offset < content.len() {
        let chunk = &content[offset..];
        let buffer = chunk.to_vec();
        let (bytes_written, _) = file.write_at(buffer, offset as u64).await?;
        offset += bytes_written;
    }
    
    file.sync_all().await?;
    Ok(())
}
```

### 4. Metadata Preservation

```rust
// Shared by both backends
pub async fn preserve_metadata(
    dir_fd: &DirectoryFd,
    relative_path: &str,
    metadata: &FileMetadata,
) -> Result<()> {
    // ✅ Uses secure *at syscalls
    
    // Set times using utimensat
    dir_fd.lutimensat(
        relative_path,
        metadata.accessed,
        metadata.modified,
    ).await?;
    
    // Set permissions using fchmodat
    dir_fd.lfchmodat(
        relative_path,
        metadata.permissions(),
    ).await?;
    
    // Set ownership using fchownat
    dir_fd.lfchownat(
        relative_path,
        metadata.uid(),
        metadata.gid(),
    ).await?;
    
    Ok(())
}
```

## Local Backend: Direct Use

```rust
impl LocalBackend {
    async fn sync_directory(&self, src: &Path, dst: &Path) -> Result<SyncStats> {
        // Open secure directory handles
        let src_root = DirectoryFd::open(src).await?;
        let dst_root = DirectoryFd::open(dst).await?;
        
        // Use shared walker
        let walker = SecureTreeWalker::new(&src_root).await?;
        let mut stats = SyncStats::default();
        
        for entry in walker.walk().await {
            let entry = entry?;
            
            // Check if copy needed
            let dst_exists = dst_root.statx_full(entry.relative_path.as_ref()).await.is_ok();
            
            if !dst_exists || needs_sync(&entry.metadata, /*...*/) {
                // ✅ Use shared read/write functions with DirectoryFd
                let content = read_file_content(&src_root, entry.relative_path.to_str().unwrap()).await?;
                write_file_content(&dst_root, entry.relative_path.to_str().unwrap(), &content).await?;
                
                // ✅ Use shared metadata preservation
                preserve_metadata(&dst_root, entry.relative_path.to_str().unwrap(), &entry.metadata).await?;
                
                stats.files_copied += 1;
            }
        }
        
        Ok(stats)
    }
}
```

## Protocol Backend: Reuses Local Abstractions

```rust
impl RsyncProtocol<T: Transport> {
    async fn send_files(&mut self, src: &Path) -> Result<SyncStats> {
        // ✅ Open secure directory handle
        let src_root = DirectoryFd::open(src).await?;
        
        // ✅ Use shared walker (same as local!)
        let walker = SecureTreeWalker::new(&src_root).await?;
        
        // Collect file list for protocol
        let mut files = Vec::new();
        for entry in walker.walk().await {
            let entry = entry?;
            files.push(FileEntry {
                path: entry.relative_path,
                size: entry.metadata.size,
                mtime: entry.metadata.modified,
                mode: entry.metadata.permissions(),
                uid: entry.metadata.uid(),
                gid: entry.metadata.gid(),
                // ...
            });
        }
        
        // Protocol-specific: Send file list
        self.send_file_list(&files).await?;
        
        // For each file, read using secure abstractions
        for file in &files {
            // ✅ Use shared read function with DirectoryFd
            let content = read_file_content(&src_root, file.path.to_str().unwrap()).await?;
            
            // Protocol-specific: Compute delta and send
            let delta = self.compute_delta(&content).await?;
            self.send_delta(&delta).await?;
        }
        
        Ok(SyncStats {
            files_copied: files.len() as u64,
            // ...
        })
    }
    
    async fn receive_files(&mut self, dst: &Path) -> Result<SyncStats> {
        // ✅ Open secure directory handle
        let dst_root = DirectoryFd::open(dst).await?;
        
        // Protocol-specific: Receive file list
        let files = self.receive_file_list().await?;
        
        for file in &files {
            // Protocol-specific: Receive delta
            let delta = self.receive_delta().await?;
            
            // Reconstruct file content
            let content = self.apply_delta(&delta).await?;
            
            // ✅ Use shared write function with DirectoryFd
            write_file_content(&dst_root, file.path.to_str().unwrap(), &content).await?;
            
            // ✅ Use shared metadata preservation
            let metadata = FileMetadata {
                size: file.size,
                modified: file.mtime,
                // ... convert from FileEntry
            };
            preserve_metadata(&dst_root, file.path.to_str().unwrap(), &metadata).await?;
        }
        
        Ok(SyncStats {
            files_copied: files.len() as u64,
            // ...
        })
    }
}
```

## Benefits of This Approach

### 1. Security Through Reuse

```rust
// ✅ All file access uses DirectoryFd
// ✅ All operations use secure *at syscalls
// ✅ TOCTOU-safe, symlink-attack resistant
// ✅ No path-based operations after initial open

// Both backends benefit from same security properties
let src_root = DirectoryFd::open(src).await?;  // ← Pinned directory
let file = src_root.open_file_at(path, ...).await?;  // ← openat (secure)
let meta = src_root.statx_full(path).await?;  // ← statx (secure)
```

### 2. Code Reuse

```rust
// Functions used by BOTH local and protocol backends:
- SecureTreeWalker::walk()       // Directory traversal
- read_file_content()             // File reading
- write_file_content()            // File writing
- preserve_metadata()             // Metadata preservation

// Protocol only adds:
- Handshake logic
- Delta computation
- Wire format encoding/decoding
```

### 3. Consistent Behavior

```rust
// Both backends:
// - Walk directories the same way
// - Read files the same way
// - Write files the same way
// - Preserve metadata the same way

// Only difference:
// - Local: Direct copy
// - Protocol: Delta transfer over network
```

### 4. Testing

```rust
// Test shared components once
#[test]
async fn test_secure_tree_walker() {
    let walker = SecureTreeWalker::new(test_dir).await?;
    // Verify DirectoryFd usage, security properties
}

#[test]
async fn test_read_file_content() {
    let dir_fd = DirectoryFd::open(test_dir).await?;
    let content = read_file_content(&dir_fd, "test.txt").await?;
    // Verify secure openat usage
}

// Both backends automatically benefit from these tests
```

## Implementation Structure

```
src/
├── filesystem/                  # Layer 2: Shared Operations
│   ├── mod.rs
│   ├── walker.rs               # SecureTreeWalker using DirectoryFd
│   ├── read.rs                 # read_file_content with openat
│   ├── write.rs                # write_file_content with openat
│   └── metadata.rs             # preserve_metadata with *at syscalls
│
├── backends/                    # Layer 3: Backend Implementations
│   ├── local.rs                # Uses filesystem/* functions
│   └── protocol.rs             # Also uses filesystem/* functions!
│
├── traits/                      # Layer 1: Low-level traits
│   ├── metadata.rs
│   ├── file.rs
│   └── directory.rs
│
└── sync/                        # Layer 4: High-level API
    └── engine.rs               # Orchestrates backends
```

## Migration Strategy

### Current State
```rust
// rsync.rs currently does:
let content = fs::read(&file_path)?;  // ❌ Insecure path-based
```

### Target State
```rust
// rsync should do:
let dir_fd = DirectoryFd::open(base_path).await?;  // ✅ Secure
let content = read_file_content(&dir_fd, relative_path).await?;  // ✅ Reuse
```

### Steps

1. **Phase 1**: Extract secure operations to `src/filesystem/`
   - `walker.rs` with `DirectoryFd`
   - `read.rs` with `openat`
   - `write.rs` with `openat`
   - `metadata.rs` with `*at` syscalls

2. **Phase 2**: Update local backend to use shared operations
   - Replace direct `compio::fs` calls
   - Use shared walker, read, write functions

3. **Phase 3**: Update protocol backend to use shared operations
   - Replace `fs::read` with `read_file_content`
   - Replace `fs::write` with `write_file_content`
   - Use `SecureTreeWalker` for directory traversal

4. **Phase 4**: Remove duplicated code
   - Delete local-only implementations
   - Protocol-specific code only does protocol work

## Key Takeaways

1. **DirectoryFd Everywhere**
   - Open directory once, use for all operations
   - Both local and protocol backends use same pattern

2. **Shared Operations are Building Blocks**
   - `SecureTreeWalker` used by both
   - `read_file_content` used by both
   - `write_file_content` used by both
   - `preserve_metadata` used by both

3. **Protocol Layer is Thin**
   - Only handles wire format
   - Uses local abstractions for actual I/O
   - No duplicate file operations

4. **Security Through Architecture**
   - All file access through DirectoryFd
   - All operations use secure *at syscalls
   - TOCTOU-safe by design
   - Benefits all backends automatically

5. **Easy Testing**
   - Test shared components once
   - Mock at filesystem layer
   - Both backends get tested implicitly

This is the **correct layering**: Protocol uses filesystem abstractions, doesn't reinvent them.

