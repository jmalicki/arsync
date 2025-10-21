//! AsyncDirectory trait for unified directory operations
//!
//! This trait provides a unified interface for directory operations that can be
//! implemented by both local filesystem backends and remote protocol backends.

use crate::error::Result;
use std::path::Path;
use super::{AsyncFile, AsyncMetadata};

/// Unified directory interface for both local and remote operations
///
/// This trait provides a consistent interface for directory operations regardless
/// of whether the underlying storage is local (using compio-fs-extended) or remote
/// (using protocol transports like SSH, rsync, etc.).
///
/// # Type Parameters
///
/// * `File` - The file type for this filesystem
/// * `Metadata` - The metadata type for this filesystem
///
/// # Examples
///
/// ```rust,ignore
/// // List directory contents
/// let dir = filesystem.open_directory(path).await?;
/// let entries = dir.read_dir().await?;
/// for entry in entries {
///     println!("{}", entry.name());
/// }
/// ```
pub trait AsyncDirectory: Send + Sync + 'static {
    /// The file type for this filesystem
    type File: AsyncFile<Metadata = Self::Metadata>;
    
    /// The metadata type for this filesystem
    type Metadata: AsyncMetadata;
    
    /// The directory entry type for this filesystem
    type Entry: AsyncDirectoryEntry<File = Self::File, Metadata = Self::Metadata>;

    /// Read directory contents
    ///
    /// # Returns
    ///
    /// Returns `Ok(entries)` with a list of directory entries, or `Err(SyncError)` if:
    /// - The directory is not accessible
    /// - I/O error occurs
    async fn read_dir(&self) -> Result<Vec<Self::Entry>>;

    /// Create a new file in this directory
    ///
    /// # Parameters
    ///
    /// * `name` - Name of the file to create
    ///
    /// # Returns
    ///
    /// Returns `Ok(File)` if the file is created successfully, or `Err(SyncError)` if:
    /// - Permission is denied
    /// - The file already exists
    /// - I/O error occurs
    async fn create_file(&self, name: &str) -> Result<Self::File>;

    /// Create a new directory in this directory
    ///
    /// # Parameters
    ///
    /// * `name` - Name of the directory to create
    ///
    /// # Returns
    ///
    /// Returns `Ok(Directory)` if the directory is created successfully, or `Err(SyncError)` if:
    /// - Permission is denied
    /// - The directory already exists
    /// - I/O error occurs
    async fn create_directory(&self, name: &str) -> Result<Self::Directory>;

    /// Open a file in this directory
    ///
    /// # Parameters
    ///
    /// * `name` - Name of the file to open
    ///
    /// # Returns
    ///
    /// Returns `Ok(File)` if the file exists and can be opened, or `Err(SyncError)` if:
    /// - The file doesn't exist
    /// - Permission is denied
    /// - I/O error occurs
    async fn open_file(&self, name: &str) -> Result<Self::File>;

    /// Open a directory in this directory
    ///
    /// # Parameters
    ///
    /// * `name` - Name of the directory to open
    ///
    /// # Returns
    ///
    /// Returns `Ok(Directory)` if the directory exists and can be opened, or `Err(SyncError)` if:
    /// - The directory doesn't exist
    /// - Permission is denied
    /// - I/O error occurs
    async fn open_directory(&self, name: &str) -> Result<Self::Directory>;

    /// Get metadata for a file or directory in this directory
    ///
    /// # Parameters
    ///
    /// * `name` - Name of the file or directory to get metadata for
    ///
    /// # Returns
    ///
    /// Returns `Ok(Metadata)` if the path exists and metadata can be retrieved, or `Err(SyncError)` if:
    /// - The path doesn't exist
    /// - Permission is denied
    /// - I/O error occurs
    async fn metadata(&self, name: &str) -> Result<Self::Metadata>;

    /// Remove a file from this directory
    ///
    /// # Parameters
    ///
    /// * `name` - Name of the file to remove
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the file is removed successfully, or `Err(SyncError)` if:
    /// - The file doesn't exist
    /// - Permission is denied
    /// - I/O error occurs
    async fn remove_file(&self, name: &str) -> Result<()>;

    /// Remove a directory from this directory
    ///
    /// # Parameters
    ///
    /// * `name` - Name of the directory to remove
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the directory is removed successfully, or `Err(SyncError)` if:
    /// - The directory doesn't exist
    /// - Permission is denied
    /// - The directory is not empty
    /// - I/O error occurs
    async fn remove_directory(&self, name: &str) -> Result<()>;

    /// Check if a file or directory exists in this directory
    ///
    /// # Parameters
    ///
    /// * `name` - Name of the file or directory to check
    ///
    /// # Returns
    ///
    /// Returns `true` if the path exists, `false` otherwise.
    async fn exists(&self, name: &str) -> bool {
        self.metadata(name).await.is_ok()
    }

    /// Get the directory path
    ///
    /// # Returns
    ///
    /// Returns the path of this directory.
    fn path(&self) -> &Path;
}

/// Unified directory entry interface
///
/// This trait provides a consistent interface for directory entries regardless
/// of the underlying filesystem implementation.
///
/// # Type Parameters
///
/// * `File` - The file type for this filesystem
/// * `Metadata` - The metadata type for this filesystem
pub trait AsyncDirectoryEntry: Send + Sync + 'static {
    /// The file type for this filesystem
    type File: AsyncFile<Metadata = Self::Metadata>;
    
    /// The metadata type for this filesystem
    type Metadata: AsyncMetadata;

    /// Get the name of this entry
    ///
    /// # Returns
    ///
    /// Returns the name of the file or directory.
    fn name(&self) -> &str;

    /// Get the full path of this entry
    ///
    /// # Returns
    ///
    /// Returns the full path of the file or directory.
    fn path(&self) -> &Path;

    /// Get metadata for this entry
    ///
    /// # Returns
    ///
    /// Returns `Ok(Metadata)` if metadata can be retrieved, or `Err(SyncError)` if:
    /// - I/O error occurs
    async fn metadata(&self) -> Result<Self::Metadata>;

    /// Check if this entry is a file
    ///
    /// # Returns
    ///
    /// Returns `true` if this entry is a file, `false` otherwise.
    async fn is_file(&self) -> Result<bool> {
        let metadata = self.metadata().await?;
        Ok(metadata.is_file())
    }

    /// Check if this entry is a directory
    ///
    /// # Returns
    ///
    /// Returns `true` if this entry is a directory, `false` otherwise.
    async fn is_directory(&self) -> Result<bool> {
        let metadata = self.metadata().await?;
        Ok(metadata.is_dir())
    }

    /// Check if this entry is a symlink
    ///
    /// # Returns
    ///
    /// Returns `true` if this entry is a symlink, `false` otherwise.
    async fn is_symlink(&self) -> Result<bool> {
        let metadata = self.metadata().await?;
        Ok(metadata.is_symlink())
    }

    /// Open this entry as a file
    ///
    /// # Returns
    ///
    /// Returns `Ok(File)` if this entry is a file and can be opened, or `Err(SyncError)` if:
    /// - This entry is not a file
    /// - Permission is denied
    /// - I/O error occurs
    async fn open_file(&self) -> Result<Self::File>;

    /// Open this entry as a directory
    ///
    /// # Returns
    ///
    /// Returns `Ok(Directory)` if this entry is a directory and can be opened, or `Err(SyncError)` if:
    /// - This entry is not a directory
    /// - Permission is denied
    /// - I/O error occurs
    async fn open_directory(&self) -> Result<Self::Directory>;
}