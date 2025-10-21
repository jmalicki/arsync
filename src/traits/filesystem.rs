//! AsyncFileSystem trait for unified filesystem operations
//!
//! This trait provides a unified interface for filesystem operations that can be
//! implemented by both local filesystem backends (using compio-fs-extended) and
//! remote protocol backends (using the Transport trait).

use crate::error::Result;
use std::path::Path;
use std::time::SystemTime;

use super::{AsyncFile, AsyncDirectory, AsyncMetadata};

/// Unified filesystem interface for both local and remote operations
///
/// This trait provides a consistent interface for filesystem operations regardless
/// of whether the underlying storage is local (using compio-fs-extended) or remote
/// (using protocol transports like SSH, rsync, etc.).
///
/// # Type Parameters
///
/// * `File` - The file type for this filesystem
/// * `Directory` - The directory type for this filesystem  
/// * `Metadata` - The metadata type for this filesystem
///
/// # Examples
///
/// ```rust,ignore
/// // Local filesystem
/// let local_fs = LocalFileSystem::new();
/// let file = local_fs.open_file(Path::new("test.txt")).await?;
///
/// // Remote filesystem via SSH
/// let transport = SshTransport::new("user@host").await?;
/// let remote_fs = ProtocolFileSystem::new(transport);
/// let file = remote_fs.open_file(Path::new("test.txt")).await?;
/// ```
pub trait AsyncFileSystem: Send + Sync + 'static {
    /// The file type for this filesystem
    type File: AsyncFile<Metadata = Self::Metadata>;
    
    /// The directory type for this filesystem
    type Directory: AsyncDirectory<File = Self::File, Metadata = Self::Metadata>;
    
    /// The metadata type for this filesystem
    type Metadata: AsyncMetadata;

    /// Open an existing file for reading
    ///
    /// # Parameters
    ///
    /// * `path` - Path to the file to open
    ///
    /// # Returns
    ///
    /// Returns `Ok(File)` if the file exists and can be opened, or `Err(SyncError)` if:
    /// - The file doesn't exist
    /// - Permission is denied
    /// - The path is not a file
    /// - I/O error occurs
    async fn open_file(&self, path: &Path) -> Result<Self::File>;

    /// Create a new file for writing
    ///
    /// # Parameters
    ///
    /// * `path` - Path where the file should be created
    ///
    /// # Returns
    ///
    /// Returns `Ok(File)` if the file is created successfully, or `Err(SyncError)` if:
    /// - The parent directory doesn't exist
    /// - Permission is denied
    /// - The file already exists and cannot be overwritten
    /// - I/O error occurs
    async fn create_file(&self, path: &Path) -> Result<Self::File>;

    /// Open an existing directory
    ///
    /// # Parameters
    ///
    /// * `path` - Path to the directory to open
    ///
    /// # Returns
    ///
    /// Returns `Ok(Directory)` if the directory exists and can be opened, or `Err(SyncError)` if:
    /// - The directory doesn't exist
    /// - Permission is denied
    /// - The path is not a directory
    /// - I/O error occurs
    async fn open_directory(&self, path: &Path) -> Result<Self::Directory>;

    /// Create a new directory
    ///
    /// # Parameters
    ///
    /// * `path` - Path where the directory should be created
    ///
    /// # Returns
    ///
    /// Returns `Ok(Directory)` if the directory is created successfully, or `Err(SyncError)` if:
    /// - The parent directory doesn't exist
    /// - Permission is denied
    /// - The directory already exists
    /// - I/O error occurs
    async fn create_directory(&self, path: &Path) -> Result<Self::Directory>;

    /// Create a directory and all parent directories
    ///
    /// # Parameters
    ///
    /// * `path` - Path where the directory should be created
    ///
    /// # Returns
    ///
    /// Returns `Ok(Directory)` if the directory is created successfully, or `Err(SyncError)` if:
    /// - Permission is denied
    /// - I/O error occurs
    async fn create_directory_all(&self, path: &Path) -> Result<Self::Directory>;

    /// Get metadata for a path
    ///
    /// # Parameters
    ///
    /// * `path` - Path to get metadata for
    ///
    /// # Returns
    ///
    /// Returns `Ok(Metadata)` if the path exists and metadata can be retrieved, or `Err(SyncError)` if:
    /// - The path doesn't exist
    /// - Permission is denied
    /// - I/O error occurs
    async fn metadata(&self, path: &Path) -> Result<Self::Metadata>;

    /// Check if a path exists
    ///
    /// # Parameters
    ///
    /// * `path` - Path to check
    ///
    /// # Returns
    ///
    /// Returns `true` if the path exists, `false` otherwise.
    async fn exists(&self, path: &Path) -> bool {
        self.metadata(path).await.is_ok()
    }

    /// Remove a file
    ///
    /// # Parameters
    ///
    /// * `path` - Path to the file to remove
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the file is removed successfully, or `Err(SyncError)` if:
    /// - The file doesn't exist
    /// - Permission is denied
    /// - The path is not a file
    /// - I/O error occurs
    async fn remove_file(&self, path: &Path) -> Result<()>;

    /// Remove an empty directory
    ///
    /// # Parameters
    ///
    /// * `path` - Path to the directory to remove
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the directory is removed successfully, or `Err(SyncError)` if:
    /// - The directory doesn't exist
    /// - Permission is denied
    /// - The directory is not empty
    /// - The path is not a directory
    /// - I/O error occurs
    async fn remove_directory(&self, path: &Path) -> Result<()>;

    /// Copy a file from one location to another
    ///
    /// # Parameters
    ///
    /// * `src` - Source file path
    /// * `dst` - Destination file path
    ///
    /// # Returns
    ///
    /// Returns `Ok(u64)` with the number of bytes copied, or `Err(SyncError)` if:
    /// - Source file doesn't exist
    /// - Destination cannot be created
    /// - I/O error occurs during copy
    async fn copy_file(&self, src: &Path, dst: &Path) -> Result<u64> {
        let src_file = self.open_file(src).await?;
        let mut dst_file = self.create_file(dst).await?;
        
        // Use copy_file_range if available, otherwise fall back to read/write
        if let Ok(bytes_copied) = src_file.copy_file_range(&mut dst_file, 0, 0, u64::MAX).await {
            Ok(bytes_copied)
        } else {
            // Fallback to read/write copy
            self.copy_file_read_write(src, dst).await
        }
    }

    /// Copy a file using read/write operations
    ///
    /// This is a fallback method when copy_file_range is not available.
    ///
    /// # Parameters
    ///
    /// * `src` - Source file path
    /// * `dst` - Destination file path
    ///
    /// # Returns
    ///
    /// Returns `Ok(u64)` with the number of bytes copied, or `Err(SyncError)` if:
    /// - Source file doesn't exist
    /// - Destination cannot be created
    /// - I/O error occurs during copy
    async fn copy_file_read_write(&self, src: &Path, dst: &Path) -> Result<u64> {
        let src_file = self.open_file(src).await?;
        let mut dst_file = self.create_file(dst).await?;
        
        let mut buffer = vec![0u8; 64 * 1024]; // 64KB buffer
        let mut offset = 0u64;
        let mut total_bytes = 0u64;
        
        loop {
            let (bytes_read, returned_buffer) = src_file.read_at(buffer, offset).await?;
            buffer = returned_buffer;
            
            if bytes_read == 0 {
                break;
            }
            
            buffer.truncate(bytes_read);
            let (_, returned_buffer) = dst_file.write_at(buffer, offset).await?;
            buffer = returned_buffer;
            buffer.resize(64 * 1024, 0);
            
            offset += bytes_read as u64;
            total_bytes += bytes_read as u64;
        }
        
        Ok(total_bytes)
    }

    /// Get the filesystem name for debugging/logging
    ///
    /// # Returns
    ///
    /// Returns a string identifier for this filesystem type.
    fn name(&self) -> &'static str {
        "unknown"
    }

    /// Check if this filesystem supports copy_file_range
    ///
    /// # Returns
    ///
    /// Returns `true` if copy_file_range is supported, `false` otherwise.
    fn supports_copy_file_range(&self) -> bool {
        false
    }

    /// Check if this filesystem supports hardlinks
    ///
    /// # Returns
    ///
    /// Returns `true` if hardlinks are supported, `false` otherwise.
    fn supports_hardlinks(&self) -> bool {
        false
    }

    /// Check if this filesystem supports symlinks
    ///
    /// # Returns
    ///
    /// Returns `true` if symlinks are supported, `false` otherwise.
    fn supports_symlinks(&self) -> bool {
        false
    }
}