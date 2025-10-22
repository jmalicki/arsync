//! AsyncDirectory and AsyncDirectoryEntry traits for unified directory operations
//!
//! This module provides traits for directory operations that can be implemented
//! by both local filesystem backends and remote protocol backends.
//!
//! See `docs/projects/trait-filesystem-abstraction/design.md` for architecture.

use super::AsyncMetadata;
use crate::error::Result;
use std::path::Path;

/// Unified directory entry interface for both local and remote operations
///
/// This trait provides a consistent interface for directory entries regardless
/// of whether the underlying storage is local (using compio-fs-extended) or remote
/// (using protocol transports like SSH, rsync, etc.).
///
/// # Examples
///
/// ```rust,ignore
/// use arsync::traits::AsyncDirectoryEntry;
///
/// let entry = directory.read_entries().await?.into_iter().next().unwrap();
/// println!("Entry name: {}", entry.name());
/// let metadata = entry.metadata().await?;
/// println!("Is file: {}", metadata.is_file());
/// ```
#[allow(async_fn_in_trait)] // Intentional design for compio-style async I/O
#[allow(dead_code)] // Will be used in PR #7 (directory wrapper)
pub trait AsyncDirectoryEntry: Send + Sync + 'static {
    /// The metadata type for this entry
    type Metadata: AsyncMetadata;

    /// Get the entry name (basename only, no path separators)
    ///
    /// # Returns
    ///
    /// Returns the entry name as a string slice. The name contains only
    /// the basename without any path separators.
    fn name(&self) -> &str;

    /// Get the full path to this entry
    ///
    /// # Returns
    ///
    /// Returns the full path including parent directory
    fn path(&self) -> &Path;

    /// Get metadata for this entry
    ///
    /// # Returns
    ///
    /// Returns the metadata for this directory entry
    ///
    /// # Errors
    ///
    /// Returns `Err(SyncError)` if metadata cannot be retrieved
    async fn metadata(&self) -> Result<Self::Metadata>;

    /// Check if this entry is a file
    ///
    /// This is a convenience method that calls `metadata().is_file()`.
    /// Override if you can determine file type without fetching full metadata.
    ///
    /// # Returns
    ///
    /// Returns `true` if the entry is a regular file
    ///
    /// # Errors
    ///
    /// Returns `Err(SyncError)` if metadata cannot be retrieved
    async fn is_file(&self) -> Result<bool> {
        Ok(self.metadata().await?.is_file())
    }

    /// Check if this entry is a directory
    ///
    /// This is a convenience method that calls `metadata().is_dir()`.
    /// Override if you can determine file type without fetching full metadata.
    ///
    /// # Returns
    ///
    /// Returns `true` if the entry is a directory
    ///
    /// # Errors
    ///
    /// Returns `Err(SyncError)` if metadata cannot be retrieved
    async fn is_dir(&self) -> Result<bool> {
        Ok(self.metadata().await?.is_dir())
    }

    /// Check if this entry is a symlink
    ///
    /// This is a convenience method that calls `metadata().is_symlink()`.
    /// Override if you can determine file type without fetching full metadata.
    ///
    /// # Returns
    ///
    /// Returns `true` if the entry is a symlink
    ///
    /// # Errors
    ///
    /// Returns `Err(SyncError)` if metadata cannot be retrieved
    async fn is_symlink(&self) -> Result<bool> {
        Ok(self.metadata().await?.is_symlink())
    }
}

/// Unified directory interface for both local and remote operations
///
/// This trait provides a consistent interface for directory operations regardless
/// of whether the underlying storage is local (using compio-fs-extended) or remote
/// (using protocol transports like SSH, rsync, etc.).
///
/// # Examples
///
/// ```rust,ignore
/// use arsync::traits::AsyncDirectory;
///
/// let directory = filesystem.open_directory(path).await?;
/// let entries = directory.read_entries().await?;
///
/// for entry in entries {
///     println!("Entry: {}", entry.name());
/// }
/// ```
#[allow(async_fn_in_trait)] // Intentional design for compio-style async I/O
#[allow(dead_code)] // Will be used in PR #7 (directory wrapper)
pub trait AsyncDirectory: Send + Sync + 'static {
    /// The directory entry type
    type Entry: AsyncDirectoryEntry;

    /// The metadata type for this directory
    type Metadata: AsyncMetadata;

    /// Read all entries in the directory
    ///
    /// Returns a vector of directory entries. For large directories, this may
    /// be memory-intensive. Future versions might add streaming iteration.
    ///
    /// # Returns
    ///
    /// Returns `Ok(Vec<Entry>)` containing all directory entries.
    /// The `.` and `..` entries are excluded.
    ///
    /// # Errors
    ///
    /// Returns `Err(SyncError)` if:
    /// - Directory cannot be read
    /// - Permission denied
    /// - I/O error occurs
    async fn read_entries(&self) -> Result<Vec<Self::Entry>>;

    /// Get metadata for this directory
    ///
    /// # Returns
    ///
    /// Returns the metadata for the directory itself
    ///
    /// # Errors
    ///
    /// Returns `Err(SyncError)` if metadata cannot be retrieved
    async fn metadata(&self) -> Result<Self::Metadata>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::SystemTime;

    // Mock implementations for testing

    #[allow(dead_code)] // Test fixture
    struct MockDirectoryEntry {
        name: String,
        path: PathBuf,
        is_file: bool,
    }

    #[allow(dead_code)] // Test fixture
    struct MockMetadata {
        is_file: bool,
        is_dir: bool,
        size: u64,
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
            false
        }

        fn permissions(&self) -> u32 {
            0o644
        }

        fn uid(&self) -> u32 {
            1000
        }

        fn gid(&self) -> u32 {
            1000
        }

        fn modified(&self) -> SystemTime {
            SystemTime::UNIX_EPOCH
        }

        fn accessed(&self) -> SystemTime {
            SystemTime::UNIX_EPOCH
        }

        fn device_id(&self) -> u64 {
            0
        }

        fn inode_number(&self) -> u64 {
            0
        }

        fn link_count(&self) -> u64 {
            1
        }
    }

    impl AsyncDirectoryEntry for MockDirectoryEntry {
        type Metadata = MockMetadata;

        fn name(&self) -> &str {
            &self.name
        }

        fn path(&self) -> &Path {
            &self.path
        }

        async fn metadata(&self) -> Result<Self::Metadata> {
            Ok(MockMetadata {
                is_file: self.is_file,
                is_dir: !self.is_file,
                size: 0,
            })
        }
    }

    #[allow(dead_code)] // Test fixture
    struct MockDirectory {
        entries: Vec<MockDirectoryEntry>,
    }

    impl AsyncDirectory for MockDirectory {
        type Entry = MockDirectoryEntry;
        type Metadata = MockMetadata;

        async fn read_entries(&self) -> Result<Vec<Self::Entry>> {
            Ok(self
                .entries
                .iter()
                .map(|e| MockDirectoryEntry {
                    name: e.name.clone(),
                    path: e.path.clone(),
                    is_file: e.is_file,
                })
                .collect())
        }

        async fn metadata(&self) -> Result<Self::Metadata> {
            Ok(MockMetadata {
                is_file: false,
                is_dir: true,
                size: 0,
            })
        }
    }

    #[compio::test]
    async fn test_directory_read_entries() -> anyhow::Result<()> {
        let dir = MockDirectory {
            entries: vec![
                MockDirectoryEntry {
                    name: "file1.txt".to_string(),
                    path: PathBuf::from("/test/file1.txt"),
                    is_file: true,
                },
                MockDirectoryEntry {
                    name: "file2.txt".to_string(),
                    path: PathBuf::from("/test/file2.txt"),
                    is_file: true,
                },
            ],
        };

        let entries = dir.read_entries().await?;
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name(), "file1.txt");
        assert_eq!(entries[1].name(), "file2.txt");

        Ok(())
    }

    #[compio::test]
    async fn test_directory_entry_metadata() -> anyhow::Result<()> {
        let entry = MockDirectoryEntry {
            name: "test.txt".to_string(),
            path: PathBuf::from("/test/test.txt"),
            is_file: true,
        };

        let metadata = entry.metadata().await?;
        assert!(metadata.is_file());
        assert!(!metadata.is_dir());

        Ok(())
    }

    #[compio::test]
    async fn test_directory_entry_convenience_methods() -> anyhow::Result<()> {
        let file_entry = MockDirectoryEntry {
            name: "file.txt".to_string(),
            path: PathBuf::from("/test/file.txt"),
            is_file: true,
        };

        let dir_entry = MockDirectoryEntry {
            name: "subdir".to_string(),
            path: PathBuf::from("/test/subdir"),
            is_file: false,
        };

        assert!(file_entry.is_file().await?);
        assert!(!file_entry.is_dir().await?);

        assert!(!dir_entry.is_file().await?);
        assert!(dir_entry.is_dir().await?);

        Ok(())
    }

    #[compio::test]
    async fn test_directory_metadata() -> anyhow::Result<()> {
        let dir = MockDirectory { entries: vec![] };

        let metadata = dir.metadata().await?;
        assert!(!metadata.is_file());
        assert!(metadata.is_dir());

        Ok(())
    }
}
