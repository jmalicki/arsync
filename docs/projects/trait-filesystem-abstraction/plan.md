# Trait-Based Filesystem Abstraction - Implementation Plan

## Overview

This plan breaks down the trait-based filesystem abstraction into incremental, reviewable PRs. Each PR builds on the previous one and can be tested independently.

## Principles

1. **One trait per PR** (mostly)
2. **Each PR must compile and pass tests**
3. **Stacked PRs** for easier review
4. **Bottom-up approach** (simple to complex)
5. **Minimal changes** per PR to reduce risk

## Phase 1: AsyncMetadata Trait

### PR #1: Add AsyncMetadata trait and implement for existing Metadata

**Branch**: `feat/async-metadata-trait`

**Files Changed**:
- `src/lib.rs` - add `pub mod traits;`
- `src/traits/mod.rs` (new)
- `src/traits/metadata.rs` (new)
- `src/metadata.rs` - add `impl AsyncMetadata for Metadata`

**Changes**:
```rust
// src/traits/mod.rs
pub mod metadata;
pub use metadata::AsyncMetadata;

// src/traits/metadata.rs
pub trait AsyncMetadata: Send + Sync + 'static {
    fn size(&self) -> u64;
    fn is_file(&self) -> bool;
    fn is_dir(&self) -> bool;
    fn is_symlink(&self) -> bool;
    fn permissions(&self) -> u32;
    fn uid(&self) -> u32;
    fn gid(&self) -> u32;
    fn modified(&self) -> SystemTime;
    fn accessed(&self) -> SystemTime;
    fn device_id(&self) -> u64;
    fn inode_number(&self) -> u64;
    fn link_count(&self) -> u64;
    
    // Provided methods with default implementations
    fn is_empty(&self) -> bool { self.size() == 0 }
    fn file_type(&self) -> &'static str { /* ... */ }
    fn file_type_description(&self) -> String { /* ... */ }
    fn is_special(&self) -> bool { /* ... */ }
    fn is_same_file(&self, other: &Self) -> bool { /* ... */ }
    fn summary(&self) -> String { /* ... */ }
}

// src/metadata.rs - add at end
impl crate::traits::AsyncMetadata for Metadata {
    fn size(&self) -> u64 { self.len() }
    fn is_file(&self) -> bool { Metadata::is_file(self) }
    fn is_dir(&self) -> bool { Metadata::is_dir(self) }
    fn is_symlink(&self) -> bool { Metadata::is_symlink(self) }
    fn permissions(&self) -> u32 { self.permissions().mode() }
    // ... etc
}
```

**Tests**:
- Add `src/traits/metadata.rs` tests verifying default implementations
- Add integration test verifying `Metadata` implements trait correctly

**Success Criteria**:
- ✅ Compiles without errors
- ✅ All existing tests pass
- ✅ New trait tests pass
- ✅ `Metadata` type implements `AsyncMetadata`

**Estimated Time**: 4-6 hours

---

## Phase 2: AsyncFile Trait

### PR #2: Add AsyncFile trait

**Branch**: `feat/async-file-trait`

**Files Changed**:
- `src/traits/mod.rs` - add `pub mod file;`
- `src/traits/file.rs` (new)

**Changes**:
```rust
// src/traits/file.rs
use crate::error::Result;
use super::AsyncMetadata;

pub trait AsyncFile: Send + Sync + 'static {
    type Metadata: AsyncMetadata;
    
    async fn read_at(&self, buf: Vec<u8>, offset: u64) 
        -> Result<(usize, Vec<u8>)>;
    
    async fn write_at(&self, buf: Vec<u8>, offset: u64) 
        -> Result<(usize, Vec<u8>)>;
    
    async fn sync_all(&self) -> Result<()>;
    
    async fn metadata(&self) -> Result<Self::Metadata>;
    
    async fn copy_file_range(
        &self,
        dst: &mut Self,
        src_offset: u64,
        dst_offset: u64,
        len: u64,
    ) -> Result<u64>;
    
    // Provided methods
    async fn read_all(&self) -> Result<Vec<u8>> {
        let metadata = self.metadata().await?;
        let size = metadata.size();
        if size > 100 * 1024 * 1024 {
            return Err(crate::error::SyncError::FileSystem(
                "File too large for read_all".to_string()
            ));
        }
        let buffer = vec![0u8; size as usize];
        let (bytes_read, buffer) = self.read_at(buffer, 0).await?;
        Ok(buffer[..bytes_read].to_vec())
    }
    
    async fn write_all_at(&self, data: &[u8], offset: u64) -> Result<()> {
        let buffer = data.to_vec();
        let (bytes_written, _) = self.write_at(buffer, offset).await?;
        if bytes_written != data.len() {
            return Err(crate::error::SyncError::FileSystem(
                "Partial write".to_string()
            ));
        }
        Ok(())
    }
}
```

**Tests**:
- Mock implementation of AsyncFile for testing
- Test provided methods (read_all, write_all_at)
- Verify trait bounds compile correctly

**Success Criteria**:
- ✅ Compiles without errors
- ✅ All existing tests pass
- ✅ Trait definition is clean and usable
- ✅ Mock implementation works

**Estimated Time**: 4-6 hours

---

## Phase 3: AsyncDirectory Trait

### PR #3: Add AsyncDirectory and AsyncDirectoryEntry traits

**Branch**: `feat/async-directory-trait`

**Files Changed**:
- `src/traits/mod.rs` - add `pub mod directory;`
- `src/traits/directory.rs` (new)

**Changes**:
```rust
// src/traits/directory.rs
use crate::error::Result;
use std::path::Path;
use super::AsyncMetadata;

pub trait AsyncDirectoryEntry: Send + Sync + 'static {
    type Metadata: AsyncMetadata;
    
    fn name(&self) -> &str;
    fn path(&self) -> &Path;
    async fn metadata(&self) -> Result<Self::Metadata>;
    
    // Provided methods
    async fn is_file(&self) -> Result<bool> {
        Ok(self.metadata().await?.is_file())
    }
    
    async fn is_directory(&self) -> Result<bool> {
        Ok(self.metadata().await?.is_dir())
    }
    
    async fn is_symlink(&self) -> Result<bool> {
        Ok(self.metadata().await?.is_symlink())
    }
}

pub trait AsyncDirectory: Send + Sync + 'static {
    type Entry: AsyncDirectoryEntry;
    type Metadata: AsyncMetadata;
    
    async fn read_entries(&self) -> Result<Vec<Self::Entry>>;
    async fn metadata(&self) -> Result<Self::Metadata>;
    fn path(&self) -> &Path;
}
```

**Tests**:
- Mock implementations for testing
- Test trait bounds
- Verify entry iteration works

**Success Criteria**:
- ✅ Compiles without errors
- ✅ All existing tests pass
- ✅ Mock implementation works
- ✅ Can iterate over directory entries

**Estimated Time**: 4-6 hours

---

## Phase 4: AsyncFileSystem Trait

### PR #4: Add AsyncFileSystem trait

**Branch**: `feat/async-filesystem-trait`

**Files Changed**:
- `src/traits/mod.rs` - add `pub mod filesystem;`
- `src/traits/filesystem.rs` (new)

**Changes**:
```rust
// src/traits/filesystem.rs
use crate::error::Result;
use std::path::Path;
use super::{AsyncFile, AsyncDirectory, AsyncMetadata};

pub trait AsyncFileSystem: Send + Sync + 'static {
    type File: AsyncFile<Metadata = Self::Metadata>;
    type Directory: AsyncDirectory<Metadata = Self::Metadata>;
    type Metadata: AsyncMetadata;
    
    // File operations
    async fn open_file(&self, path: &Path) -> Result<Self::File>;
    async fn create_file(&self, path: &Path) -> Result<Self::File>;
    
    // Directory operations
    async fn open_directory(&self, path: &Path) -> Result<Self::Directory>;
    async fn create_directory(&self, path: &Path) -> Result<()>;
    async fn create_directory_all(&self, path: &Path) -> Result<()>;
    
    // Metadata operations
    async fn metadata(&self, path: &Path) -> Result<Self::Metadata>;
    
    // Remove operations
    async fn remove_file(&self, path: &Path) -> Result<()>;
    async fn remove_directory(&self, path: &Path) -> Result<()>;
    
    // Capability queries
    fn supports_copy_file_range(&self) -> bool { false }
    fn supports_hardlinks(&self) -> bool { false }
    fn supports_symlinks(&self) -> bool { false }
}
```

**Tests**:
- Mock filesystem implementation
- Test trait bounds
- Verify associated types work correctly

**Success Criteria**:
- ✅ Compiles without errors
- ✅ All existing tests pass
- ✅ Trait definition is clean
- ✅ Associated types work

**Estimated Time**: 4-6 hours

---

## Phase 5: Local Filesystem Backend

### PR #5: Add LocalFileSystem backend

**Branch**: `feat/local-filesystem-backend`

**Files Changed**:
- `src/lib.rs` - add `pub mod backends;`
- `src/backends/mod.rs` (new)
- `src/backends/local.rs` (new)

**Changes**:
```rust
// src/backends/mod.rs
pub mod local;
pub use local::LocalFileSystem;

// src/backends/local.rs
pub struct LocalFileSystem;

pub struct LocalFile {
    file: compio::fs::File,
}

pub struct LocalDirectory {
    path: PathBuf,
}

pub struct LocalDirectoryEntry {
    name: String,
    path: PathBuf,
    metadata: Option<Metadata>, // Cached
}

impl AsyncFileSystem for LocalFileSystem {
    type File = LocalFile;
    type Directory = LocalDirectory;
    type Metadata = Metadata;
    
    async fn open_file(&self, path: &Path) -> Result<Self::File> {
        let file = compio::fs::File::open(path).await?;
        Ok(LocalFile { file })
    }
    
    // ... implement all methods
}

impl AsyncFile for LocalFile {
    type Metadata = Metadata;
    
    async fn read_at(&self, buf: Vec<u8>, offset: u64) 
        -> Result<(usize, Vec<u8>)> {
        let buf_result = self.file.read_at(buf, offset).await;
        Ok((buf_result.0?, buf_result.1))
    }
    
    // ... implement all methods
}

impl AsyncDirectory for LocalDirectory {
    type Entry = LocalDirectoryEntry;
    type Metadata = Metadata;
    
    async fn read_entries(&self) -> Result<Vec<Self::Entry>> {
        let entries = compio_fs_extended::directory::read_dir(&self.path).await?;
        let mut result = Vec::new();
        for entry_result in entries {
            let entry = entry_result?;
            let name = entry.file_name().to_string_lossy().to_string();
            let path = self.path.join(&name);
            result.push(LocalDirectoryEntry {
                name,
                path,
                metadata: None,
            });
        }
        Ok(result)
    }
    
    // ... implement all methods
}

impl AsyncDirectoryEntry for LocalDirectoryEntry {
    type Metadata = Metadata;
    
    fn name(&self) -> &str { &self.name }
    fn path(&self) -> &Path { &self.path }
    
    async fn metadata(&self) -> Result<Self::Metadata> {
        // Use cached if available, otherwise fetch
        if let Some(ref meta) = self.metadata {
            Ok(meta.clone())
        } else {
            Ok(compio::fs::metadata(&self.path).await?)
        }
    }
}
```

**Tests**:
- Comprehensive integration tests with temp files
- Test all filesystem operations
- Compare with direct compio usage
- Test error cases

**Success Criteria**:
- ✅ All trait methods implemented
- ✅ All tests pass
- ✅ Performance is comparable to direct compio
- ✅ Error handling works correctly

**Estimated Time**: 8-12 hours

---

## Phase 6: Protocol Backend Stub

### PR #6: Add ProtocolFileSystem stub backend

**Branch**: `feat/protocol-filesystem-stub`

**Files Changed**:
- `src/backends/mod.rs` - add `pub mod protocol;`
- `src/backends/protocol.rs` (new)

**Changes**:
```rust
// src/backends/protocol.rs
use crate::protocol::transport::Transport;

pub struct ProtocolFileSystem<T: Transport> {
    transport: T,
}

impl<T> ProtocolFileSystem<T>
where
    T: Transport + Send + Sync + 'static,
{
    pub fn new(transport: T) -> Self {
        Self { transport }
    }
}

// Stub implementations that return NotImplemented
impl<T> AsyncFileSystem for ProtocolFileSystem<T>
where
    T: Transport + Send + Sync + 'static,
{
    type File = ProtocolFile<T>;
    type Directory = ProtocolDirectory<T>;
    type Metadata = ProtocolMetadata;
    
    async fn open_file(&self, _path: &Path) -> Result<Self::File> {
        Err(SyncError::NotImplemented(
            "Protocol filesystem not yet implemented".into()
        ))
    }
    
    // ... all other methods return NotImplemented
}

// Similar stubs for ProtocolFile, ProtocolDirectory, etc.
```

**Tests**:
- Verify trait implementation compiles
- Test that methods return NotImplemented error
- Verify generic constraints work with Transport

**Success Criteria**:
- ✅ Compiles with Transport trait
- ✅ All trait methods stubbed
- ✅ Tests verify NotImplemented errors

**Estimated Time**: 4-6 hours

---

## Phase 7: Generic Operations

### PR #7: Add generic filesystem operations

**Branch**: `feat/generic-operations`

**Files Changed**:
- `src/traits/mod.rs` - add `pub mod operations;`
- `src/traits/operations.rs` (new)

**Changes**:
```rust
// src/traits/operations.rs
use super::AsyncFileSystem;

pub trait FileOperations {
    async fn copy_file(&self, src: &Path, dst: &Path) -> Result<u64>;
    async fn copy_directory(&self, src: &Path, dst: &Path) -> Result<()>;
}

pub struct GenericFileOperations<FS: AsyncFileSystem> {
    filesystem: FS,
    buffer_size: usize,
}

impl<FS: AsyncFileSystem> GenericFileOperations<FS> {
    pub fn new(filesystem: FS, buffer_size: usize) -> Self {
        Self { filesystem, buffer_size }
    }
    
    pub fn filesystem(&self) -> &FS {
        &self.filesystem
    }
}

impl<FS: AsyncFileSystem> FileOperations for GenericFileOperations<FS> {
    async fn copy_file(&self, src: &Path, dst: &Path) -> Result<u64> {
        let src_file = self.filesystem.open_file(src).await?;
        let dst_file = self.filesystem.create_file(dst).await?;
        
        let src_metadata = src_file.metadata().await?;
        let size = src_metadata.size();
        
        let mut offset = 0;
        let mut total_copied = 0;
        
        while offset < size {
            let to_read = std::cmp::min(self.buffer_size, (size - offset) as usize);
            let buffer = vec![0u8; to_read];
            
            let (bytes_read, buffer) = src_file.read_at(buffer, offset).await?;
            if bytes_read == 0 {
                break;
            }
            
            let data = &buffer[..bytes_read];
            dst_file.write_all_at(data, offset).await?;
            
            offset += bytes_read as u64;
            total_copied += bytes_read as u64;
        }
        
        dst_file.sync_all().await?;
        Ok(total_copied)
    }
    
    async fn copy_directory(&self, src: &Path, dst: &Path) -> Result<()> {
        // Implementation...
        todo!()
    }
}
```

**Tests**:
- Test copy_file with LocalFileSystem
- Test with various file sizes
- Verify byte-for-byte correctness

**Success Criteria**:
- ✅ Generic operations work with any AsyncFileSystem
- ✅ copy_file works correctly
- ✅ Tests pass with LocalFileSystem

**Estimated Time**: 6-8 hours

---

## Phase 8: Integration & Migration

### PR #8: Add trait-based alternative for file copying

**Branch**: `feat/trait-copy-alternative`

**Files Changed**:
- `src/copy.rs` - add trait-based functions alongside existing
- Examples showing usage

**Changes**:
```rust
// Add to src/copy.rs
pub async fn copy_file_with_traits(
    src: &Path,
    dst: &Path,
    buffer_size: usize,
) -> Result<u64> {
    use crate::backends::LocalFileSystem;
    use crate::traits::{FileOperations, GenericFileOperations};
    
    let fs = LocalFileSystem;
    let ops = GenericFileOperations::new(fs, buffer_size);
    ops.copy_file(src, dst).await
}
```

**Tests**:
- Comparative tests (trait-based vs original)
- Performance benchmarks
- Verify identical behavior

**Success Criteria**:
- ✅ Trait-based version works
- ✅ Performance is equivalent
- ✅ Tests pass

**Estimated Time**: 4-6 hours

---

## Later Phases (Future Work)

### Phase 9: Complete Protocol Backend
- Implement full protocol filesystem
- Add remote file operations
- Integrate with SSH transport

### Phase 10: Full Migration
- Migrate remaining code
- Remove old implementations
- Update documentation

### Phase 11: Optimizations
- Add caching where beneficial
- Optimize hot paths
- Add advanced features

## Summary

**Total PRs**: 8 core PRs + future work

**Timeline**:
- Phase 1-4: ~2-3 weeks (traits only)
- Phase 5-7: ~2-3 weeks (implementations)
- Phase 8: ~1 week (integration)
- **Total**: ~5-7 weeks

**Dependencies**:
- Each PR depends on previous one
- Can be reviewed sequentially
- Can pause between phases for feedback

## Review Process

For each PR:
1. Self-review checklist:
   - ✅ Compiles without warnings
   - ✅ All tests pass
   - ✅ Documentation complete
   - ✅ Examples work
   - ✅ No breaking changes (unless planned)

2. Create PR with:
   - Clear description
   - Link to design doc
   - Test results
   - Performance comparison (if applicable)

3. Address feedback and iterate

4. Merge when approved

5. Start next PR from merged branch

## Success Metrics

- All PRs merged successfully
- No performance regressions
- Code coverage maintained or improved
- Documentation is clear
- Team understands the new system

