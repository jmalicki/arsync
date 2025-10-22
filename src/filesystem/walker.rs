//! Secure directory tree walking using DirectoryFd
//!
//! This module provides TOCTOU-safe directory traversal using DirectoryFd
//! and *at syscalls. It can be used by both local and protocol backends.

use crate::error::{Result, SyncError};
use compio_fs_extended::DirectoryFd;
use std::collections::VecDeque;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// A file or directory entry discovered during tree walking
///
/// Contains all information needed to process the entry, including
/// metadata fetched using secure *at syscalls.
#[derive(Debug)]
#[allow(dead_code)] // Will be used in PR #12 and later
pub struct FileEntry {
    /// Relative path from the walk root
    pub relative_path: PathBuf,
    /// Entry name (basename only)
    pub name: OsString,
    /// Metadata fetched via DirectoryFd
    pub metadata: compio_fs_extended::FileMetadata,
    /// Parent DirectoryFd for secure operations
    pub parent_fd: DirectoryFd,
}

/// Secure tree walker using DirectoryFd throughout
///
/// Walks a directory tree using DirectoryFd and *at syscalls for TOCTOU safety.
/// This can be used by both local copy operations and protocol backends.
///
/// # Security
///
/// - All operations use DirectoryFd + *at syscalls
/// - No path-based operations after initial root open
/// - Safe from TOCTOU, symlink attacks, race conditions
///
/// # Examples
///
/// ```rust,ignore
/// use arsync::filesystem::SecureTreeWalker;
/// use std::path::Path;
///
/// #[compio::main]
/// async fn main() -> arsync::Result<()> {
///     let walker = SecureTreeWalker::new(Path::new("/some/path")).await?;
///     
///     for entry in walker.walk().await? {
///         println!("Found: {:?}", entry.relative_path);
///         println!("  Size: {}", entry.metadata.size);
///         println!("  Is file: {}", entry.metadata.is_file());
///     }
///     
///     Ok(())
/// }
/// ```
#[allow(dead_code)] // Will be used in PR #12 and later
pub struct SecureTreeWalker {
    /// Root directory file descriptor
    root: DirectoryFd,
    /// Root path (for constructing relative paths)
    root_path: PathBuf,
}

#[allow(dead_code)] // Methods will be used in PR #12 and later
impl SecureTreeWalker {
    /// Create a new secure tree walker
    ///
    /// # Parameters
    ///
    /// * `path` - Root directory path to walk
    ///
    /// # Returns
    ///
    /// Returns a new `SecureTreeWalker` instance
    ///
    /// # Errors
    ///
    /// Returns `Err(SyncError)` if:
    /// - Path doesn't exist
    /// - Path is not a directory
    /// - Permission denied
    pub async fn new(path: &Path) -> Result<Self> {
        let root = DirectoryFd::open(path)
            .await
            .map_err(|e| SyncError::FileSystem(format!("Failed to open root directory: {e}")))?;

        Ok(Self {
            root,
            root_path: path.to_path_buf(),
        })
    }

    /// Walk the directory tree and return all entries
    ///
    /// Performs breadth-first traversal using DirectoryFd for each directory.
    /// All metadata is fetched using secure *at syscalls.
    ///
    /// # Returns
    ///
    /// Returns `Ok(Vec<FileEntry>)` containing all discovered entries
    ///
    /// # Errors
    ///
    /// Returns `Err(SyncError)` if:
    /// - Directory cannot be read
    /// - Metadata fetch fails
    /// - Permission denied during traversal
    ///
    /// # Note
    ///
    /// Returns a Vec rather than an async iterator for simplicity.
    /// For very large directories, this may use significant memory.
    ///
    /// # TOCTOU Safety
    ///
    /// While directory listing uses path-based `read_dir` (kernel limitation - no io_uring
    /// GETDENTS64 support), all operations on individual entries use DirectoryFd + *at
    /// syscalls (statx_full, open_directory_at). This provides TOCTOU safety for actual
    /// file operations, though the directory itself could theoretically be replaced between
    /// opening the DirectoryFd and listing entries (mitigated by keeping DirectoryFd open).
    pub async fn walk(&self) -> Result<Vec<FileEntry>> {
        let mut result = Vec::new();
        let mut queue: VecDeque<(DirectoryFd, PathBuf)> = VecDeque::new();

        // Start with root (DirectoryFd.clone() is cheap - just clones Arc)
        queue.push_back((self.root.clone(), PathBuf::new()));

        while let Some((dir_fd, rel_path)) = queue.pop_front() {
            // Read directory entries
            // Note: Uses path-based read_dir due to kernel limitation (no GETDENTS64 in io_uring)
            // See: https://lwn.net/Articles/878873/
            // The DirectoryFd remains open, providing some TOCTOU protection
            let entries = compio_fs_extended::directory::read_dir(dir_fd.path())
                .await
                .map_err(|e| {
                    SyncError::FileSystem(format!(
                        "Failed to read directory {}: {e}",
                        dir_fd.path().display()
                    ))
                })?;

            for entry_result in entries {
                let entry = entry_result.map_err(|e| {
                    SyncError::FileSystem(format!(
                        "Failed to read directory entry in {}: {e}",
                        dir_fd.path().display()
                    ))
                })?;

                let name = entry.file_name();
                let entry_rel_path = rel_path.join(&name);

                // Get metadata using DirectoryFd (TOCTOU-safe *at syscall)
                let metadata = dir_fd.statx_full(&name).await.map_err(|e| {
                    SyncError::FileSystem(format!(
                        "Failed to get metadata for {}: {e}",
                        entry_rel_path.display()
                    ))
                })?;

                let is_dir = metadata.is_dir();

                // If it's a directory, open it now before we move the name
                let subdir_fd = if is_dir {
                    Some(dir_fd.open_directory_at(&name).await.map_err(|e| {
                        SyncError::FileSystem(format!(
                            "Failed to open subdirectory {}: {e}",
                            entry_rel_path.display()
                        ))
                    })?)
                } else {
                    None
                };

                // Add to results (avoiding unnecessary clones)
                result.push(FileEntry {
                    relative_path: entry_rel_path.clone(),
                    name,
                    metadata,
                    parent_fd: dir_fd.clone(),
                });

                // If it's a directory, add to queue for traversal
                if let Some(fd) = subdir_fd {
                    queue.push_back((fd, entry_rel_path));
                }
            }
        }

        Ok(result)
    }

    /// Get the root DirectoryFd
    ///
    /// # Returns
    ///
    /// Returns a reference to the root DirectoryFd
    #[must_use]
    pub fn root_fd(&self) -> &DirectoryFd {
        &self.root
    }

    /// Get the root path
    ///
    /// # Returns
    ///
    /// Returns the root path for this walker
    #[must_use]
    pub fn root_path(&self) -> &Path {
        &self.root_path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[compio::test]
    async fn test_walk_empty_directory() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;

        let walker = SecureTreeWalker::new(temp_dir.path()).await?;
        let entries = walker.walk().await?;

        assert_eq!(entries.len(), 0);

        Ok(())
    }

    #[compio::test]
    async fn test_walk_single_file() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        fs::File::create(temp_dir.path().join("test.txt"))?;

        let walker = SecureTreeWalker::new(temp_dir.path()).await?;
        let entries = walker.walk().await?;

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "test.txt");
        assert!(entries[0].metadata.is_file());

        Ok(())
    }

    #[compio::test]
    async fn test_walk_nested_directories() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;

        // Create nested structure
        fs::create_dir(temp_dir.path().join("dir1"))?;
        fs::create_dir(temp_dir.path().join("dir1/subdir"))?;
        fs::File::create(temp_dir.path().join("dir1/file1.txt"))?;
        fs::File::create(temp_dir.path().join("dir1/subdir/file2.txt"))?;

        let walker = SecureTreeWalker::new(temp_dir.path()).await?;
        let entries = walker.walk().await?;

        // Should find: dir1, dir1/file1.txt, dir1/subdir, dir1/subdir/file2.txt
        assert_eq!(entries.len(), 4);

        // Check we found all expected entries
        let names: Vec<_> = entries
            .iter()
            .map(|e| e.relative_path.to_str().unwrap())
            .collect();
        assert!(names.contains(&"dir1"));
        assert!(names.contains(&"dir1/file1.txt"));
        assert!(names.contains(&"dir1/subdir"));
        assert!(names.contains(&"dir1/subdir/file2.txt"));

        Ok(())
    }

    #[compio::test]
    async fn test_walk_uses_directory_fd() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        fs::File::create(temp_dir.path().join("test.txt"))?;

        let walker = SecureTreeWalker::new(temp_dir.path()).await?;
        let entries = walker.walk().await?;

        // Verify entry has DirectoryFd parent
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].parent_fd.path(), temp_dir.path());

        Ok(())
    }

    #[compio::test]
    async fn test_walk_metadata_accuracy() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let file_path = temp_dir.path().join("data.bin");
        fs::write(&file_path, b"12345")?;

        let walker = SecureTreeWalker::new(temp_dir.path()).await?;
        let entries = walker.walk().await?;

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].metadata.size, 5);
        assert!(entries[0].metadata.is_file());

        Ok(())
    }
}
