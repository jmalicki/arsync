# Protocol Mod Integration with Compio Using Traits

## Overview

This document outlines the design for integrating the protocol mod with compio using a trait-based architecture that allows for code reuse between local filesystem operations and remote protocol operations.

## Design Goals

1. **Unified Interface**: Single trait-based interface for both local and remote operations
2. **Code Reuse**: Maximize code reuse between filesystem and protocol implementations
3. **Performance**: Leverage compio's io_uring backend for optimal performance
4. **Extensibility**: Easy to add new transport types or filesystem backends
5. **Type Safety**: Compile-time guarantees for operation compatibility

## Architecture

### Core Traits

#### 1. `AsyncFileSystem` Trait
```rust
pub trait AsyncFileSystem: Send + Sync + 'static {
    type File: AsyncFile;
    type Directory: AsyncDirectory;
    type Metadata: AsyncMetadata;
    
    async fn open_file(&self, path: &Path) -> Result<Self::File>;
    async fn create_file(&self, path: &Path) -> Result<Self::File>;
    async fn open_directory(&self, path: &Path) -> Result<Self::Directory>;
    async fn create_directory(&self, path: &Path) -> Result<Self::Directory>;
    async fn metadata(&self, path: &Path) -> Result<Self::Metadata>;
    async fn remove_file(&self, path: &Path) -> Result<()>;
    async fn remove_directory(&self, path: &Path) -> Result<()>;
}
```

#### 2. `AsyncFile` Trait
```rust
pub trait AsyncFile: Send + Sync + 'static {
    async fn read_at(&self, buf: Vec<u8>, offset: u64) -> Result<(usize, Vec<u8>)>;
    async fn write_at(&self, buf: Vec<u8>, offset: u64) -> Result<(usize, Vec<u8>)>;
    async fn sync_all(&self) -> Result<()>;
    async fn metadata(&self) -> Result<Self::Metadata>;
    async fn copy_file_range(&self, dst: &Self, src_offset: u64, dst_offset: u64, len: u64) -> Result<u64>;
}
```

#### 3. `AsyncDirectory` Trait
```rust
pub trait AsyncDirectory: Send + Sync + 'static {
    type Entry: AsyncDirectoryEntry;
    
    async fn read_dir(&self) -> Result<Vec<Self::Entry>>;
    async fn create_file(&self, name: &str) -> Result<Self::File>;
    async fn create_directory(&self, name: &str) -> Result<Self::Directory>;
}
```

#### 4. `AsyncMetadata` Trait
```rust
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
}
```

### Backend Implementations

#### 1. Local Filesystem Backend
```rust
pub struct LocalFileSystem {
    // Uses compio-fs-extended internally
}

impl AsyncFileSystem for LocalFileSystem {
    type File = LocalFile;
    type Directory = LocalDirectory;
    type Metadata = LocalMetadata;
    
    // Implementation delegates to compio-fs-extended
}
```

#### 2. Protocol Backend
```rust
pub struct ProtocolFileSystem<T: Transport> {
    transport: T,
    // Protocol-specific state
}

impl<T: Transport> AsyncFileSystem for ProtocolFileSystem<T> {
    type File = ProtocolFile<T>;
    type Directory = ProtocolDirectory<T>;
    type Metadata = ProtocolMetadata;
    
    // Implementation uses Transport trait for I/O
}
```

### Unified Operations Layer

#### 1. `FileOperations` Trait
```rust
pub trait FileOperations<FS: AsyncFileSystem> {
    async fn copy_file(&self, src: &Path, dst: &Path) -> Result<u64>;
    async fn copy_directory(&self, src: &Path, dst: &Path) -> Result<DirectoryStats>;
    async fn preserve_metadata(&self, src: &Path, dst: &Path) -> Result<()>;
}
```

#### 2. Generic Implementation
```rust
pub struct GenericFileOperations<FS: AsyncFileSystem> {
    filesystem: FS,
    buffer_size: usize,
}

impl<FS: AsyncFileSystem> FileOperations<FS> for GenericFileOperations<FS> {
    // Generic implementation that works with any filesystem backend
}
```

## Implementation Strategy

### Phase 1: Core Traits
1. Define the core traits in a new `traits` module
2. Create basic implementations for local filesystem using compio-fs-extended
3. Create protocol implementations using existing Transport trait

### Phase 2: Unified Operations
1. Implement generic file operations that work with any filesystem backend
2. Create adapter patterns for existing code
3. Add comprehensive tests

### Phase 3: Integration
1. Update existing code to use the new trait system
2. Add protocol-specific optimizations
3. Performance testing and optimization

## Benefits

1. **Code Reuse**: Single implementation of file operations works with both local and remote filesystems
2. **Type Safety**: Compile-time guarantees that operations are supported
3. **Performance**: Direct use of compio's io_uring backend
4. **Extensibility**: Easy to add new filesystem types or transport protocols
5. **Testing**: Easy to mock filesystem operations for testing

## Migration Path

1. **Gradual Migration**: Existing code can be migrated incrementally
2. **Backward Compatibility**: Old APIs can be maintained as thin wrappers
3. **Feature Flags**: Protocol features can be gated behind feature flags
4. **Performance**: No performance regression during migration

## Example Usage

```rust
// Local filesystem
let local_fs = LocalFileSystem::new();
let local_ops = GenericFileOperations::new(local_fs, 64 * 1024);
local_ops.copy_file(src, dst).await?;

// Remote filesystem via SSH
let transport = SshTransport::new("user@host").await?;
let remote_fs = ProtocolFileSystem::new(transport);
let remote_ops = GenericFileOperations::new(remote_fs, 64 * 1024);
remote_ops.copy_file(src, dst).await?;

// Same operations, different backends
```

## Future Extensions

1. **Caching**: Add caching layer for remote operations
2. **Compression**: Add compression support for protocol operations
3. **Encryption**: Add encryption support for secure transfers
4. **Parallel Operations**: Add parallel operation support
5. **Progress Tracking**: Add progress tracking for long operations