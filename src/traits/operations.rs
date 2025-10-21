//! FileOperations trait for unified file and directory operations
//!
//! This trait provides a unified interface for high-level file and directory operations
//! that can work with any filesystem backend implementing the AsyncFileSystem trait.

use crate::error::Result;
use crate::directory::DirectoryStats;
use crate::metadata::MetadataConfig;
use crate::cli::ParallelCopyConfig;
use std::path::Path;
use super::AsyncFileSystem;

/// Unified file operations interface
///
/// This trait provides high-level file and directory operations that can work
/// with any filesystem backend implementing the AsyncFileSystem trait.
///
/// # Type Parameters
///
/// * `FS` - The filesystem type implementing AsyncFileSystem
///
/// # Examples
///
/// ```rust,ignore
/// // Local filesystem
/// let local_fs = LocalFileSystem::new();
/// let local_ops = GenericFileOperations::new(local_fs, 64 * 1024);
/// local_ops.copy_file(src, dst).await?;
///
/// // Remote filesystem via SSH
/// let transport = SshTransport::new("user@host").await?;
/// let remote_fs = ProtocolFileSystem::new(transport);
/// let remote_ops = GenericFileOperations::new(remote_fs, 64 * 1024);
/// remote_ops.copy_file(src, dst).await?;
/// ```
pub trait FileOperations<FS: AsyncFileSystem> {
    /// Copy a single file from source to destination
    ///
    /// # Parameters
    ///
    /// * `src` - Source file path
    /// * `dst` - Destination file path
    ///
    /// # Returns
    ///
    /// Returns `Ok(bytes_copied)` if the file is copied successfully, or `Err(SyncError)` if:
    /// - Source file doesn't exist
    /// - Destination cannot be created
    /// - I/O error occurs during copy
    async fn copy_file(&self, src: &Path, dst: &Path) -> Result<u64>;

    /// Copy a file with metadata preservation
    ///
    /// # Parameters
    ///
    /// * `src` - Source file path
    /// * `dst` - Destination file path
    /// * `metadata_config` - Metadata preservation configuration
    ///
    /// # Returns
    ///
    /// Returns `Ok(bytes_copied)` if the file is copied successfully, or `Err(SyncError)` if:
    /// - Source file doesn't exist
    /// - Destination cannot be created
    /// - I/O error occurs during copy
    /// - Metadata preservation fails
    async fn copy_file_with_metadata(
        &self,
        src: &Path,
        dst: &Path,
        metadata_config: &MetadataConfig,
    ) -> Result<u64>;

    /// Copy an entire directory tree
    ///
    /// # Parameters
    ///
    /// * `src` - Source directory path
    /// * `dst` - Destination directory path
    /// * `metadata_config` - Metadata preservation configuration
    /// * `parallel_config` - Parallel copy configuration
    ///
    /// # Returns
    ///
    /// Returns `Ok(DirectoryStats)` if the directory is copied successfully, or `Err(SyncError)` if:
    /// - Source directory doesn't exist
    /// - Destination cannot be created
    /// - I/O error occurs during copy
    /// - Metadata preservation fails
    async fn copy_directory(
        &self,
        src: &Path,
        dst: &Path,
        metadata_config: &MetadataConfig,
        parallel_config: &ParallelCopyConfig,
    ) -> Result<DirectoryStats>;

    /// Preserve metadata for a file or directory
    ///
    /// # Parameters
    ///
    /// * `src` - Source path
    /// * `dst` - Destination path
    /// * `metadata_config` - Metadata preservation configuration
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if metadata is preserved successfully, or `Err(SyncError)` if:
    /// - Source path doesn't exist
    /// - Destination path doesn't exist
    /// - I/O error occurs during metadata preservation
    async fn preserve_metadata(
        &self,
        src: &Path,
        dst: &Path,
        metadata_config: &MetadataConfig,
    ) -> Result<()>;

    /// Get file or directory metadata
    ///
    /// # Parameters
    ///
    /// * `path` - Path to get metadata for
    ///
    /// # Returns
    ///
    /// Returns `Ok(Metadata)` if metadata can be retrieved, or `Err(SyncError)` if:
    /// - Path doesn't exist
    /// - Permission is denied
    /// - I/O error occurs
    async fn metadata(&self, path: &Path) -> Result<FS::Metadata>;

    /// Check if a path exists
    ///
    /// # Parameters
    ///
    /// * `path` - Path to check
    ///
    /// # Returns
    ///
    /// Returns `true` if the path exists, `false` otherwise.
    async fn exists(&self, path: &Path) -> bool;

    /// Create a directory and all parent directories
    ///
    /// # Parameters
    ///
    /// * `path` - Path where the directory should be created
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the directory is created successfully, or `Err(SyncError)` if:
    /// - Permission is denied
    /// - I/O error occurs
    async fn create_directory_all(&self, path: &Path) -> Result<()>;

    /// Remove a file
    ///
    /// # Parameters
    ///
    /// * `path` - Path to the file to remove
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the file is removed successfully, or `Err(SyncError)` if:
    /// - File doesn't exist
    /// - Permission is denied
    /// - I/O error occurs
    async fn remove_file(&self, path: &Path) -> Result<()>;

    /// Remove a directory
    ///
    /// # Parameters
    ///
    /// * `path` - Path to the directory to remove
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the directory is removed successfully, or `Err(SyncError)` if:
    /// - Directory doesn't exist
    /// - Permission is denied
    /// - Directory is not empty
    /// - I/O error occurs
    async fn remove_directory(&self, path: &Path) -> Result<()>;

    /// Get the filesystem name for debugging/logging
    ///
    /// # Returns
    ///
    /// Returns a string identifier for this filesystem type.
    fn filesystem_name(&self) -> &'static str;

    /// Get the buffer size used for I/O operations
    ///
    /// # Returns
    ///
    /// Returns the buffer size in bytes.
    fn buffer_size(&self) -> usize;
}

/// Generic implementation of FileOperations for any AsyncFileSystem
///
/// This struct provides a generic implementation of FileOperations that works
/// with any filesystem backend implementing the AsyncFileSystem trait.
///
/// # Type Parameters
///
/// * `FS` - The filesystem type implementing AsyncFileSystem
///
/// # Examples
///
/// ```rust,ignore
/// let filesystem = LocalFileSystem::new();
/// let operations = GenericFileOperations::new(filesystem, 64 * 1024);
/// let bytes_copied = operations.copy_file(src, dst).await?;
/// ```
pub struct GenericFileOperations<FS: AsyncFileSystem> {
    filesystem: FS,
    buffer_size: usize,
}

impl<FS: AsyncFileSystem> GenericFileOperations<FS> {
    /// Create a new GenericFileOperations instance
    ///
    /// # Parameters
    ///
    /// * `filesystem` - The filesystem backend to use
    /// * `buffer_size` - Buffer size for I/O operations in bytes
    ///
    /// # Returns
    ///
    /// Returns a new GenericFileOperations instance.
    pub fn new(filesystem: FS, buffer_size: usize) -> Self {
        Self {
            filesystem,
            buffer_size,
        }
    }

    /// Get a reference to the underlying filesystem
    ///
    /// # Returns
    ///
    /// Returns a reference to the filesystem backend.
    pub fn filesystem(&self) -> &FS {
        &self.filesystem
    }

    /// Get a mutable reference to the underlying filesystem
    ///
    /// # Returns
    ///
    /// Returns a mutable reference to the filesystem backend.
    pub fn filesystem_mut(&mut self) -> &mut FS {
        &mut self.filesystem
    }
}

impl<FS: AsyncFileSystem> FileOperations<FS> for GenericFileOperations<FS> {
    async fn copy_file(&self, src: &Path, dst: &Path) -> Result<u64> {
        self.filesystem.copy_file(src, dst).await
    }

    async fn copy_file_with_metadata(
        &self,
        src: &Path,
        dst: &Path,
        metadata_config: &MetadataConfig,
    ) -> Result<u64> {
        // First copy the file content
        let bytes_copied = self.filesystem.copy_file(src, dst).await?;
        
        // Then preserve metadata if requested
        if metadata_config.archive || metadata_config.perms || metadata_config.times {
            self.preserve_metadata(src, dst, metadata_config).await?;
        }
        
        Ok(bytes_copied)
    }

    async fn copy_directory(
        &self,
        src: &Path,
        dst: &Path,
        metadata_config: &MetadataConfig,
        parallel_config: &ParallelCopyConfig,
    ) -> Result<DirectoryStats> {
        // This is a simplified implementation
        // In practice, this would delegate to the existing directory copying logic
        // but adapted to work with the trait-based filesystem interface
        
        let mut stats = DirectoryStats::default();
        
        // Create destination directory
        self.filesystem.create_directory_all(dst).await?;
        stats.directories_created += 1;
        
        // Get source directory
        let src_dir = self.filesystem.open_directory(src).await?;
        let entries = src_dir.read_dir().await?;
        
        for entry in entries {
            let entry_path = entry.path();
            let relative_path = entry_path.strip_prefix(src).unwrap();
            let dst_path = dst.join(relative_path);
            
            if entry.is_directory().await? {
                // Recursively copy subdirectory
                let sub_stats = self.copy_directory(
                    &entry_path,
                    &dst_path,
                    metadata_config,
                    parallel_config,
                ).await?;
                stats.merge(&sub_stats);
            } else if entry.is_file().await? {
                // Copy file
                let bytes_copied = self.copy_file_with_metadata(
                    &entry_path,
                    &dst_path,
                    metadata_config,
                ).await?;
                stats.bytes_copied += bytes_copied;
                stats.files_copied += 1;
            } else if entry.is_symlink().await? {
                // Handle symlinks (simplified - would need proper symlink handling)
                stats.symlinks_processed += 1;
            }
        }
        
        Ok(stats)
    }

    async fn preserve_metadata(
        &self,
        src: &Path,
        dst: &Path,
        metadata_config: &MetadataConfig,
    ) -> Result<()> {
        // This is a simplified implementation
        // In practice, this would delegate to the existing metadata preservation logic
        // but adapted to work with the trait-based filesystem interface
        
        let src_metadata = self.filesystem.metadata(src).await?;
        let dst_metadata = self.filesystem.metadata(dst).await?;
        
        // For now, just verify that both paths exist and have metadata
        // Real implementation would copy permissions, ownership, timestamps, etc.
        if src_metadata.is_file() != dst_metadata.is_file() {
            return Err(crate::error::SyncError::FileSystem(
                "Source and destination must be the same type".to_string()
            ));
        }
        
        Ok(())
    }

    async fn metadata(&self, path: &Path) -> Result<FS::Metadata> {
        self.filesystem.metadata(path).await
    }

    async fn exists(&self, path: &Path) -> bool {
        self.filesystem.exists(path).await
    }

    async fn create_directory_all(&self, path: &Path) -> Result<()> {
        self.filesystem.create_directory_all(path).await?;
        Ok(())
    }

    async fn remove_file(&self, path: &Path) -> Result<()> {
        self.filesystem.remove_file(path).await
    }

    async fn remove_directory(&self, path: &Path) -> Result<()> {
        self.filesystem.remove_directory(path).await
    }

    fn filesystem_name(&self) -> &'static str {
        self.filesystem.name()
    }

    fn buffer_size(&self) -> usize {
        self.buffer_size
    }
}