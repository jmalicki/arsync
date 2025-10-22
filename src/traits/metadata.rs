//! AsyncMetadata trait for unified metadata operations
//!
//! This trait provides a unified interface for file and directory metadata that can be
//! implemented by both local filesystem backends and remote protocol backends.
//!
//! See `docs/projects/trait-filesystem-abstraction/design.md` for architecture.

use std::time::SystemTime;

/// Unified metadata interface for both local and remote operations
///
/// This trait provides a consistent interface for file and directory metadata regardless
/// of whether the underlying storage is local (using compio-fs-extended) or remote
/// (using protocol transports like SSH, rsync, etc.).
///
/// # Design
///
/// All methods are synchronous because metadata is typically already fetched.
/// If fetching is needed, do it when obtaining the metadata object, not when
/// querying individual fields.
///
/// # Examples
///
/// ```rust,ignore
/// use arsync::traits::AsyncMetadata;
///
/// let metadata = get_file_metadata(path).await?;
/// println!("Size: {} bytes", metadata.size());
/// println!("Is file: {}", metadata.is_file());
/// println!("Permissions: {:o}", metadata.permissions());
/// ```
#[allow(dead_code)] // Will be used in PR #4 (file wrapper)
pub trait AsyncMetadata: Send + Sync + 'static {
    /// Get the size of the file or directory
    ///
    /// # Returns
    ///
    /// Returns the size in bytes. For directories, this is typically 0 or the
    /// directory entry size.
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
    /// Returns the file permissions as a mode value (e.g., 0o644 for rw-r--r--).
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
    /// Used for detecting filesystem boundaries and identifying files uniquely.
    fn device_id(&self) -> u64;

    /// Get inode number
    ///
    /// # Returns
    ///
    /// Returns the inode number of this file/directory.
    /// Combined with device_id, uniquely identifies a file.
    fn inode_number(&self) -> u64;

    /// Get link count
    ///
    /// # Returns
    ///
    /// Returns the number of hard links to this file/directory.
    fn link_count(&self) -> u64;

    // =========================================================================
    // Provided methods with default implementations
    // =========================================================================

    /// Check if the file or directory is empty
    ///
    /// # Returns
    ///
    /// Returns `true` if the file is empty (size 0), `false` otherwise.
    /// For directories, this only checks size, not whether they contain entries.
    fn is_empty(&self) -> bool {
        self.size() == 0
    }

    /// Get file type as a string
    ///
    /// # Returns
    ///
    /// Returns a string describing the file type: "file", "directory", "symlink", or "unknown".
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

    /// Check if this is a special file (device, socket, pipe, etc.)
    ///
    /// # Returns
    ///
    /// Returns `true` if this is a special file (not file/directory/symlink), `false` otherwise.
    fn is_special(&self) -> bool {
        !self.is_file() && !self.is_dir() && !self.is_symlink()
    }

    /// Get a human-readable description of the file type
    ///
    /// # Returns
    ///
    /// Returns a string describing the file type with additional details like size.
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
    /// Compares device ID and inode number to determine if two metadata objects
    /// refer to the same underlying file (useful for hardlink detection).
    ///
    /// # Parameters
    ///
    /// * `other` - Other metadata to compare with
    ///
    /// # Returns
    ///
    /// Returns `true` if both metadata represent the same file (same device and inode),
    /// `false` otherwise.
    fn is_same_file(&self, other: &Self) -> bool {
        self.device_id() == other.device_id() && self.inode_number() == other.inode_number()
    }

    /// Get a summary of the metadata
    ///
    /// # Returns
    ///
    /// Returns a string containing a summary of the metadata including type, size,
    /// permissions, ownership, and file identifiers.
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

#[cfg(test)]
mod tests {
    use super::*;

    // Mock implementation for testing
    #[derive(Debug)]
    struct MockMetadata {
        size: u64,
        is_file: bool,
        is_dir: bool,
        is_symlink: bool,
        permissions: u32,
        uid: u32,
        gid: u32,
        modified: SystemTime,
        accessed: SystemTime,
        device_id: u64,
        inode_number: u64,
        link_count: u64,
    }

    impl AsyncMetadata for MockMetadata {
        fn size(&self) -> u64 {
            self.size
        }
        fn is_file(&self) -> bool {
            self.is_file
        }
        fn is_dir(&self) -> bool {
            self.is_dir
        }
        fn is_symlink(&self) -> bool {
            self.is_symlink
        }
        fn permissions(&self) -> u32 {
            self.permissions
        }
        fn uid(&self) -> u32 {
            self.uid
        }
        fn gid(&self) -> u32 {
            self.gid
        }
        fn modified(&self) -> SystemTime {
            self.modified
        }
        fn accessed(&self) -> SystemTime {
            self.accessed
        }
        fn device_id(&self) -> u64 {
            self.device_id
        }
        fn inode_number(&self) -> u64 {
            self.inode_number
        }
        fn link_count(&self) -> u64 {
            self.link_count
        }
    }

    #[test]
    fn test_is_empty_for_empty_file() {
        let meta = MockMetadata {
            size: 0,
            is_file: true,
            is_dir: false,
            is_symlink: false,
            permissions: 0o644,
            uid: 1000,
            gid: 1000,
            modified: SystemTime::now(),
            accessed: SystemTime::now(),
            device_id: 1,
            inode_number: 12345,
            link_count: 1,
        };

        assert!(meta.is_empty());
    }

    #[test]
    fn test_is_empty_for_non_empty_file() {
        let meta = MockMetadata {
            size: 1024,
            is_file: true,
            is_dir: false,
            is_symlink: false,
            permissions: 0o644,
            uid: 1000,
            gid: 1000,
            modified: SystemTime::now(),
            accessed: SystemTime::now(),
            device_id: 1,
            inode_number: 12345,
            link_count: 1,
        };

        assert!(!meta.is_empty());
    }

    #[test]
    fn test_file_type_for_regular_file() {
        let meta = MockMetadata {
            size: 100,
            is_file: true,
            is_dir: false,
            is_symlink: false,
            permissions: 0o644,
            uid: 1000,
            gid: 1000,
            modified: SystemTime::now(),
            accessed: SystemTime::now(),
            device_id: 1,
            inode_number: 12345,
            link_count: 1,
        };

        assert_eq!(meta.file_type(), "file");
    }

    #[test]
    fn test_file_type_for_directory() {
        let meta = MockMetadata {
            size: 4096,
            is_file: false,
            is_dir: true,
            is_symlink: false,
            permissions: 0o755,
            uid: 1000,
            gid: 1000,
            modified: SystemTime::now(),
            accessed: SystemTime::now(),
            device_id: 1,
            inode_number: 12345,
            link_count: 2,
        };

        assert_eq!(meta.file_type(), "directory");
    }

    #[test]
    fn test_file_type_for_symlink() {
        let meta = MockMetadata {
            size: 10,
            is_file: false,
            is_dir: false,
            is_symlink: true,
            permissions: 0o777,
            uid: 1000,
            gid: 1000,
            modified: SystemTime::now(),
            accessed: SystemTime::now(),
            device_id: 1,
            inode_number: 12345,
            link_count: 1,
        };

        assert_eq!(meta.file_type(), "symlink");
    }

    #[test]
    fn test_is_special() {
        let meta = MockMetadata {
            size: 0,
            is_file: false,
            is_dir: false,
            is_symlink: false,
            permissions: 0o644,
            uid: 1000,
            gid: 1000,
            modified: SystemTime::now(),
            accessed: SystemTime::now(),
            device_id: 1,
            inode_number: 12345,
            link_count: 1,
        };

        assert!(meta.is_special());
    }

    #[test]
    fn test_file_type_description_empty_file() {
        let meta = MockMetadata {
            size: 0,
            is_file: true,
            is_dir: false,
            is_symlink: false,
            permissions: 0o644,
            uid: 1000,
            gid: 1000,
            modified: SystemTime::now(),
            accessed: SystemTime::now(),
            device_id: 1,
            inode_number: 12345,
            link_count: 1,
        };

        assert_eq!(meta.file_type_description(), "empty file");
    }

    #[test]
    fn test_file_type_description_non_empty_file() {
        let meta = MockMetadata {
            size: 1024,
            is_file: true,
            is_dir: false,
            is_symlink: false,
            permissions: 0o644,
            uid: 1000,
            gid: 1000,
            modified: SystemTime::now(),
            accessed: SystemTime::now(),
            device_id: 1,
            inode_number: 12345,
            link_count: 1,
        };

        assert_eq!(meta.file_type_description(), "file (1024 bytes)");
    }

    #[test]
    fn test_is_same_file_true() {
        let meta1 = MockMetadata {
            size: 100,
            is_file: true,
            is_dir: false,
            is_symlink: false,
            permissions: 0o644,
            uid: 1000,
            gid: 1000,
            modified: SystemTime::now(),
            accessed: SystemTime::now(),
            device_id: 1,
            inode_number: 12345,
            link_count: 2,
        };

        let meta2 = MockMetadata {
            size: 100,
            is_file: true,
            is_dir: false,
            is_symlink: false,
            permissions: 0o644,
            uid: 1000,
            gid: 1000,
            modified: SystemTime::now(),
            accessed: SystemTime::now(),
            device_id: 1,
            inode_number: 12345, // Same inode
            link_count: 2,
        };

        assert!(meta1.is_same_file(&meta2));
    }

    #[test]
    fn test_is_same_file_false_different_inode() {
        let meta1 = MockMetadata {
            size: 100,
            is_file: true,
            is_dir: false,
            is_symlink: false,
            permissions: 0o644,
            uid: 1000,
            gid: 1000,
            modified: SystemTime::now(),
            accessed: SystemTime::now(),
            device_id: 1,
            inode_number: 12345,
            link_count: 1,
        };

        let meta2 = MockMetadata {
            size: 100,
            is_file: true,
            is_dir: false,
            is_symlink: false,
            permissions: 0o644,
            uid: 1000,
            gid: 1000,
            modified: SystemTime::now(),
            accessed: SystemTime::now(),
            device_id: 1,
            inode_number: 54321, // Different inode
            link_count: 1,
        };

        assert!(!meta1.is_same_file(&meta2));
    }

    #[test]
    fn test_is_same_file_false_different_device() {
        let meta1 = MockMetadata {
            size: 100,
            is_file: true,
            is_dir: false,
            is_symlink: false,
            permissions: 0o644,
            uid: 1000,
            gid: 1000,
            modified: SystemTime::now(),
            accessed: SystemTime::now(),
            device_id: 1,
            inode_number: 12345,
            link_count: 1,
        };

        let meta2 = MockMetadata {
            size: 100,
            is_file: true,
            is_dir: false,
            is_symlink: false,
            permissions: 0o644,
            uid: 1000,
            gid: 1000,
            modified: SystemTime::now(),
            accessed: SystemTime::now(),
            device_id: 2, // Different device
            inode_number: 12345,
            link_count: 1,
        };

        assert!(!meta1.is_same_file(&meta2));
    }

    #[test]
    fn test_summary() {
        let meta = MockMetadata {
            size: 1024,
            is_file: true,
            is_dir: false,
            is_symlink: false,
            permissions: 0o644,
            uid: 1000,
            gid: 1000,
            modified: SystemTime::now(),
            accessed: SystemTime::now(),
            device_id: 1,
            inode_number: 12345,
            link_count: 1,
        };

        let summary = meta.summary();
        assert!(summary.contains("file (1024 bytes)"));
        assert!(summary.contains("perms: 644"));
        assert!(summary.contains("uid: 1000"));
        assert!(summary.contains("gid: 1000"));
        assert!(summary.contains("dev: 1"));
        assert!(summary.contains("ino: 12345"));
    }
}
