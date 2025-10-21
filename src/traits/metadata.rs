//! AsyncMetadata trait for unified metadata operations
//!
//! This trait provides a unified interface for file and directory metadata that can be
//! implemented by both local filesystem backends and remote protocol backends.

use crate::error::Result;
use std::time::SystemTime;

/// Unified metadata interface for both local and remote operations
///
/// This trait provides a consistent interface for file and directory metadata regardless
/// of whether the underlying storage is local (using compio-fs-extended) or remote
/// (using protocol transports like SSH, rsync, etc.).
///
/// # Examples
///
/// ```rust,ignore
/// let metadata = filesystem.metadata(path).await?;
/// println!("Size: {} bytes", metadata.size());
/// println!("Is file: {}", metadata.is_file());
/// println!("Permissions: {:o}", metadata.permissions());
/// ```
pub trait AsyncMetadata: Send + Sync + 'static {
    /// Get the size of the file or directory
    ///
    /// # Returns
    ///
    /// Returns the size in bytes. For directories, this is typically 0.
    fn size(&self) -> u64;

    /// Check if this is a regular file
    ///
    /// # Returns
    ///
    /// Returns `true` if this is a regular file, `false` otherwise.
    fn is_file(&self) -> bool;

    /// Check if this is a directory
    ///
    /// # Returns
    ///
    /// Returns `true` if this is a directory, `false` otherwise.
    fn is_dir(&self) -> bool;

    /// Check if this is a symlink
    ///
    /// # Returns
    ///
    /// Returns `true` if this is a symlink, `false` otherwise.
    fn is_symlink(&self) -> bool;

    /// Get file permissions
    ///
    /// # Returns
    ///
    /// Returns the file permissions as a mode value (e.g., 0o644).
    fn permissions(&self) -> u32;

    /// Get user ID of the file owner
    ///
    /// # Returns
    ///
    /// Returns the UID of the file owner.
    fn uid(&self) -> u32;

    /// Get group ID of the file owner
    ///
    /// # Returns
    ///
    /// Returns the GID of the file owner.
    fn gid(&self) -> u32;

    /// Get last modification time
    ///
    /// # Returns
    ///
    /// Returns the last modification time as a SystemTime.
    fn modified(&self) -> SystemTime;

    /// Get last access time
    ///
    /// # Returns
    ///
    /// Returns the last access time as a SystemTime.
    fn accessed(&self) -> SystemTime;

    /// Get device ID
    ///
    /// # Returns
    ///
    /// Returns the device ID where this file/directory resides.
    fn device_id(&self) -> u64;

    /// Get inode number
    ///
    /// # Returns
    ///
    /// Returns the inode number of this file/directory.
    fn inode_number(&self) -> u64;

    /// Get link count
    ///
    /// # Returns
    ///
    /// Returns the number of hard links to this file/directory.
    fn link_count(&self) -> u64;

    /// Check if the file or directory is empty
    ///
    /// # Returns
    ///
    /// Returns `true` if the file is empty (size 0) or directory is empty, `false` otherwise.
    fn is_empty(&self) -> bool {
        self.size() == 0
    }

    /// Get file type as a string
    ///
    /// # Returns
    ///
    /// Returns a string describing the file type (e.g., "file", "directory", "symlink").
    fn file_type(&self) -> &'static str {
        if self.is_symlink() {
            "symlink"
        } else if self.is_dir() {
            "directory"
        } else if self.is_file() {
            "file"
        } else {
            "unknown"
        }
    }

    /// Check if this is a special file (device, socket, etc.)
    ///
    /// # Returns
    ///
    /// Returns `true` if this is a special file, `false` otherwise.
    fn is_special(&self) -> bool {
        !self.is_file() && !self.is_dir() && !self.is_symlink()
    }

    /// Get a human-readable description of the file type
    ///
    /// # Returns
    ///
    /// Returns a string describing the file type with additional details.
    fn file_type_description(&self) -> String {
        if self.is_symlink() {
            "symbolic link".to_string()
        } else if self.is_dir() {
            "directory".to_string()
        } else if self.is_file() {
            if self.is_empty() {
                "empty file".to_string()
            } else {
                format!("file ({} bytes)", self.size())
            }
        } else if self.is_special() {
            "special file".to_string()
        } else {
            "unknown".to_string()
        }
    }

    /// Check if this metadata represents the same file as another metadata
    ///
    /// # Parameters
    ///
    /// * `other` - Other metadata to compare with
    ///
    /// # Returns
    ///
    /// Returns `true` if both metadata represent the same file (same device and inode), `false` otherwise.
    fn is_same_file(&self, other: &Self) -> bool {
        self.device_id() == other.device_id() && self.inode_number() == other.inode_number()
    }

    /// Get a summary of the metadata
    ///
    /// # Returns
    ///
    /// Returns a string containing a summary of the metadata.
    fn summary(&self) -> String {
        format!(
            "{} (size: {}, perms: {:o}, uid: {}, gid: {}, dev: {}, ino: {})",
            self.file_type_description(),
            self.size(),
            self.permissions(),
            self.uid(),
            self.gid(),
            self.device_id(),
            self.inode_number()
        )
    }
}