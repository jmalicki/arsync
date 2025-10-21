//! Local filesystem backend implementation
//!
//! This module provides a local filesystem backend that uses compio-fs-extended
//! for high-performance async I/O operations with io_uring.

use crate::error::{Result, SyncError};
use crate::traits::{AsyncFileSystem, AsyncFile, AsyncDirectory, AsyncMetadata};
use compio_fs_extended::{ExtendedFile, DirectoryFd, FileMetadata as CompioFileMetadata};
use compio::fs::File;
use std::path::Path;
use std::time::SystemTime;

/// Local filesystem backend using compio-fs-extended
///
/// This backend provides high-performance local filesystem operations using
/// compio's io_uring backend and the compio-fs-extended crate for additional
/// filesystem operations.
pub struct LocalFileSystem;

impl LocalFileSystem {
    /// Create a new LocalFileSystem instance
    ///
    /// # Returns
    ///
    /// Returns a new LocalFileSystem instance.
    pub fn new() -> Self {
        Self
    }
}

impl Default for LocalFileSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl AsyncFileSystem for LocalFileSystem {
    type File = LocalFile;
    type Directory = LocalDirectory;
    type Metadata = LocalMetadata;

    async fn open_file(&self, path: &Path) -> Result<Self::File> {
        let file = File::open(path).await.map_err(|e| {
            SyncError::FileSystem(format!("Failed to open file {}: {}", path.display(), e))
        })?;
        Ok(LocalFile::new(file))
    }

    async fn create_file(&self, path: &Path) -> Result<Self::File> {
        let file = File::create(path).await.map_err(|e| {
            SyncError::FileSystem(format!("Failed to create file {}: {}", path.display(), e))
        })?;
        Ok(LocalFile::new(file))
    }

    async fn open_directory(&self, path: &Path) -> Result<Self::Directory> {
        let dir_fd = DirectoryFd::open(path).await.map_err(|e| {
            SyncError::FileSystem(format!("Failed to open directory {}: {}", path.display(), e))
        })?;
        Ok(LocalDirectory::new(dir_fd, path.to_path_buf()))
    }

    async fn create_directory(&self, path: &Path) -> Result<Self::Directory> {
        compio::fs::create_dir(path).await.map_err(|e| {
            SyncError::FileSystem(format!("Failed to create directory {}: {}", path.display(), e))
        })?;
        self.open_directory(path).await
    }

    async fn create_directory_all(&self, path: &Path) -> Result<Self::Directory> {
        compio::fs::create_dir_all(path).await.map_err(|e| {
            SyncError::FileSystem(format!("Failed to create directory {}: {}", path.display(), e))
        })?;
        self.open_directory(path).await
    }

    async fn metadata(&self, path: &Path) -> Result<Self::Metadata> {
        let metadata = compio::fs::metadata(path).await.map_err(|e| {
            SyncError::FileSystem(format!("Failed to get metadata for {}: {}", path.display(), e))
        })?;
        Ok(LocalMetadata::new(metadata))
    }

    async fn remove_file(&self, path: &Path) -> Result<()> {
        compio::fs::remove_file(path).await.map_err(|e| {
            SyncError::FileSystem(format!("Failed to remove file {}: {}", path.display(), e))
        })?;
        Ok(())
    }

    async fn remove_directory(&self, path: &Path) -> Result<()> {
        compio::fs::remove_dir(path).await.map_err(|e| {
            SyncError::FileSystem(format!("Failed to remove directory {}: {}", path.display(), e))
        })?;
        Ok(())
    }

    fn name(&self) -> &'static str {
        "local"
    }

    fn supports_copy_file_range(&self) -> bool {
        true
    }

    fn supports_hardlinks(&self) -> bool {
        true
    }

    fn supports_symlinks(&self) -> bool {
        true
    }
}

/// Local file implementation using compio
pub struct LocalFile {
    file: File,
}

impl LocalFile {
    fn new(file: File) -> Self {
        Self { file }
    }
}

impl AsyncFile for LocalFile {
    type Metadata = LocalMetadata;

    async fn read_at(&self, buf: Vec<u8>, offset: u64) -> Result<(usize, Vec<u8>)> {
        let result = self.file.read_at(buf, offset).await;
        match result {
            Ok((bytes_read, buffer)) => Ok((bytes_read, buffer)),
            Err(e) => Err(SyncError::FileSystem(format!("Failed to read from file: {}", e))),
        }
    }

    async fn write_at(&self, buf: Vec<u8>, offset: u64) -> Result<(usize, Vec<u8>)> {
        let result = self.file.write_at(buf, offset).await;
        match result {
            Ok((bytes_written, buffer)) => Ok((bytes_written, buffer)),
            Err(e) => Err(SyncError::FileSystem(format!("Failed to write to file: {}", e))),
        }
    }

    async fn sync_all(&self) -> Result<()> {
        self.file.sync_all().await.map_err(|e| {
            SyncError::FileSystem(format!("Failed to sync file: {}", e))
        })?;
        Ok(())
    }

    async fn metadata(&self) -> Result<Self::Metadata> {
        let metadata = self.file.metadata().await.map_err(|e| {
            SyncError::FileSystem(format!("Failed to get file metadata: {}", e))
        })?;
        Ok(LocalMetadata::new(metadata))
    }

    async fn copy_file_range(
        &self,
        dst: &mut Self,
        src_offset: u64,
        dst_offset: u64,
        len: u64,
    ) -> Result<u64> {
        // Use compio's copy_file_range if available
        let result = self.file.copy_file_range(&dst.file, src_offset, dst_offset, len).await;
        match result {
            Ok(bytes_copied) => Ok(bytes_copied),
            Err(e) => Err(SyncError::FileSystem(format!("Failed to copy file range: {}", e))),
        }
    }
}

/// Local directory implementation using compio-fs-extended
pub struct LocalDirectory {
    dir_fd: DirectoryFd,
    path: std::path::PathBuf,
}

impl LocalDirectory {
    fn new(dir_fd: DirectoryFd, path: std::path::PathBuf) -> Self {
        Self { dir_fd, path }
    }
}

impl AsyncDirectory for LocalDirectory {
    type File = LocalFile;
    type Metadata = LocalMetadata;
    type Entry = LocalDirectoryEntry;

    async fn read_dir(&self) -> Result<Vec<Self::Entry>> {
        let entries = self.dir_fd.read_dir().await.map_err(|e| {
            SyncError::FileSystem(format!("Failed to read directory: {}", e))
        })?;
        
        let mut result = Vec::new();
        for entry in entries {
            let entry_path = self.path.join(&entry.file_name());
            result.push(LocalDirectoryEntry::new(entry, entry_path));
        }
        
        Ok(result)
    }

    async fn create_file(&self, name: &str) -> Result<Self::File> {
        let path = self.path.join(name);
        let file = File::create(&path).await.map_err(|e| {
            SyncError::FileSystem(format!("Failed to create file {}: {}", name, e))
        })?;
        Ok(LocalFile::new(file))
    }

    async fn create_directory(&self, name: &str) -> Result<Self::Directory> {
        let path = self.path.join(name);
        compio::fs::create_dir(&path).await.map_err(|e| {
            SyncError::FileSystem(format!("Failed to create directory {}: {}", name, e))
        })?;
        
        let dir_fd = DirectoryFd::open(&path).await.map_err(|e| {
            SyncError::FileSystem(format!("Failed to open created directory {}: {}", name, e))
        })?;
        
        Ok(LocalDirectory::new(dir_fd, path))
    }

    async fn open_file(&self, name: &str) -> Result<Self::File> {
        let path = self.path.join(name);
        let file = File::open(&path).await.map_err(|e| {
            SyncError::FileSystem(format!("Failed to open file {}: {}", name, e))
        })?;
        Ok(LocalFile::new(file))
    }

    async fn open_directory(&self, name: &str) -> Result<Self::Directory> {
        let path = self.path.join(name);
        let dir_fd = DirectoryFd::open(&path).await.map_err(|e| {
            SyncError::FileSystem(format!("Failed to open directory {}: {}", name, e))
        })?;
        Ok(LocalDirectory::new(dir_fd, path))
    }

    async fn metadata(&self, name: &str) -> Result<Self::Metadata> {
        let path = self.path.join(name);
        let metadata = compio::fs::metadata(&path).await.map_err(|e| {
            SyncError::FileSystem(format!("Failed to get metadata for {}: {}", name, e))
        })?;
        Ok(LocalMetadata::new(metadata))
    }

    async fn remove_file(&self, name: &str) -> Result<()> {
        let path = self.path.join(name);
        compio::fs::remove_file(&path).await.map_err(|e| {
            SyncError::FileSystem(format!("Failed to remove file {}: {}", name, e))
        })?;
        Ok(())
    }

    async fn remove_directory(&self, name: &str) -> Result<()> {
        let path = self.path.join(name);
        compio::fs::remove_dir(&path).await.map_err(|e| {
            SyncError::FileSystem(format!("Failed to remove directory {}: {}", name, e))
        })?;
        Ok(())
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

/// Local directory entry implementation
pub struct LocalDirectoryEntry {
    entry: compio::fs::DirEntry,
    path: std::path::PathBuf,
}

impl LocalDirectoryEntry {
    fn new(entry: compio::fs::DirEntry, path: std::path::PathBuf) -> Self {
        Self { entry, path }
    }
}

impl crate::traits::directory::AsyncDirectoryEntry for LocalDirectoryEntry {
    type File = LocalFile;
    type Metadata = LocalMetadata;

    fn name(&self) -> &str {
        self.entry.file_name().to_str().unwrap_or("")
    }

    fn path(&self) -> &Path {
        &self.path
    }

    async fn metadata(&self) -> Result<Self::Metadata> {
        let metadata = self.entry.metadata().await.map_err(|e| {
            SyncError::FileSystem(format!("Failed to get entry metadata: {}", e))
        })?;
        Ok(LocalMetadata::new(metadata))
    }

    async fn open_file(&self) -> Result<Self::File> {
        let file = File::open(&self.path).await.map_err(|e| {
            SyncError::FileSystem(format!("Failed to open file {}: {}", self.path.display(), e))
        })?;
        Ok(LocalFile::new(file))
    }

    async fn open_directory(&self) -> Result<LocalDirectory> {
        let dir_fd = DirectoryFd::open(&self.path).await.map_err(|e| {
            SyncError::FileSystem(format!("Failed to open directory {}: {}", self.path.display(), e))
        })?;
        Ok(LocalDirectory::new(dir_fd, self.path.clone()))
    }
}

/// Local metadata implementation using compio
pub struct LocalMetadata {
    metadata: compio::fs::Metadata,
}

impl LocalMetadata {
    fn new(metadata: compio::fs::Metadata) -> Self {
        Self { metadata }
    }
}

impl AsyncMetadata for LocalMetadata {
    fn size(&self) -> u64 {
        self.metadata.len()
    }

    fn is_file(&self) -> bool {
        self.metadata.is_file()
    }

    fn is_dir(&self) -> bool {
        self.metadata.is_dir()
    }

    fn is_symlink(&self) -> bool {
        self.metadata.is_symlink()
    }

    fn permissions(&self) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        self.metadata.permissions().mode() & 0o7777
    }

    fn uid(&self) -> u32 {
        use std::os::unix::fs::MetadataExt;
        self.metadata.uid()
    }

    fn gid(&self) -> u32 {
        use std::os::unix::fs::MetadataExt;
        self.metadata.gid()
    }

    fn modified(&self) -> SystemTime {
        self.metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH)
    }

    fn accessed(&self) -> SystemTime {
        self.metadata.accessed().unwrap_or(SystemTime::UNIX_EPOCH)
    }

    fn device_id(&self) -> u64 {
        use std::os::unix::fs::MetadataExt;
        self.metadata.dev()
    }

    fn inode_number(&self) -> u64 {
        use std::os::unix::fs::MetadataExt;
        self.metadata.ino()
    }

    fn link_count(&self) -> u64 {
        use std::os::unix::fs::MetadataExt;
        self.metadata.nlink()
    }
}