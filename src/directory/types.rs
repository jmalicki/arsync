//! Core types for directory traversal and copying
//!
//! This module contains the data structures used throughout directory operations:
//! - `FileLocation`: Groups path, parent `DirectoryFd`, and filename
//! - `TraversalContext`: Shared state passed through recursion
//! - `DirectoryStats`: Statistics tracking
//! - `DirectoryEntryWrapper`: Implements AsyncDirectoryEntry trait

use crate::adaptive_concurrency::AdaptiveConcurrencyController;
use crate::cli::CopyMethod;
use crate::error::{Result, SyncError};
use crate::io_uring::FileOperations;
use crate::metadata::MetadataConfig;
use crate::traits::AsyncDirectoryEntry;
use compio::dispatcher::Dispatcher;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Location information for a file/directory with `DirectoryFd` context
///
/// Groups the path, parent `DirectoryFd`, and filename together. This triplet appears
/// everywhere (src and dst) and should be kept together for TOCTOU-safe operations.
#[derive(Clone)]
pub struct FileLocation {
    /// Full path (used only for error messages and logging)
    pub path: PathBuf,
    /// Parent directory as `DirectoryFd` (for TOCTOU-safe operations)
    pub parent_dir: Arc<compio_fs_extended::DirectoryFd>,
    /// Filename relative to `parent_dir` (basename only, no path separators)
    pub filename: std::ffi::OsString,
}

/// Context passed through directory traversal operations
///
/// Groups all the near-global state that needs to be passed down the recursion tree.
/// This significantly reduces function argument counts and makes the code more maintainable.
///
/// # Benefits
/// - Reduces function signatures from 15+ args to ~4-5 args
/// - Easier to add new context in the future
/// - Groups logically related state together
/// - Makes function calls more readable
#[derive(Clone)]
pub struct TraversalContext {
    /// File operations handler (contains copy method)
    #[allow(dead_code)] // Reserved for future use
    pub file_ops: Arc<FileOperations>,
    /// Copy method to use (e.g., auto, `copy_file_range`, splice)
    pub copy_method: CopyMethod,
    /// Shared statistics accumulator
    pub stats: Arc<crate::stats::SharedStats>,
    /// Hardlink tracker for inode-based deduplication
    pub hardlink_tracker: Arc<crate::hardlink_tracker::FilesystemTracker>,
    /// Adaptive concurrency controller (prevents FD exhaustion)
    pub concurrency_controller: Arc<AdaptiveConcurrencyController>,
    /// Metadata preservation configuration
    pub metadata_config: Arc<MetadataConfig>,
    /// Parallel copy configuration
    pub parallel_config: Arc<crate::cli::ParallelCopyConfig>,
    /// Global dispatcher for parallel operations
    pub dispatcher: &'static Dispatcher,
}

/// Directory copy operation statistics
#[derive(Debug, Default)]
pub struct DirectoryStats {
    /// Total number of files copied
    pub files_copied: u64,
    /// Total number of directories created
    pub directories_created: u64,
    /// Total number of bytes copied
    pub bytes_copied: u64,
    /// Number of symlinks processed
    pub symlinks_processed: u64,
    /// Number of errors encountered
    pub errors: u64,
}

// ExtendedMetadata removed - use compio_fs_extended::FileMetadata directly
// Eliminates redundant wrapper and double-indirection

/// Create FileMetadata from path (fallback - path-based, TOCTOU-vulnerable)
///
/// ⚠️ DEPRECATED: This uses path-based metadata which is TOCTOU-vulnerable.
/// Prefer using `DirectoryFd::statx_full()` when possible.
///
/// # Errors
///
/// Returns error if metadata cannot be retrieved.
#[allow(clippy::future_not_send)]
pub async fn metadata_from_path(path: &Path) -> Result<compio_fs_extended::FileMetadata> {
    use std::os::unix::fs::MetadataExt;

    let compio_metadata = compio::fs::symlink_metadata(path).await.map_err(|e| {
        SyncError::FileSystem(format!(
            "Failed to get metadata for {}: {}",
            path.display(),
            e
        ))
    })?;

    Ok(compio_fs_extended::FileMetadata {
        size: compio_metadata.len(),
        mode: compio_metadata.mode(),
        uid: compio_metadata.uid(),
        gid: compio_metadata.gid(),
        nlink: compio_metadata.nlink(),
        ino: compio_metadata.ino(),
        dev: compio_metadata.dev(),
        accessed: compio_metadata.accessed().unwrap_or(std::time::UNIX_EPOCH),
        modified: compio_metadata.modified().unwrap_or(std::time::UNIX_EPOCH),
        created: compio_metadata.created().ok(),
        #[cfg(target_os = "linux")]
        attributes: None,
        #[cfg(target_os = "linux")]
        attributes_mask: None,
        #[cfg(target_os = "macos")]
        flags: None,
        #[cfg(target_os = "macos")]
        generation: None,
    })
}

/// Wrapper around std::fs::DirEntry that implements AsyncDirectoryEntry trait
///
/// This wrapper enables using standard directory entries through the
/// AsyncDirectoryEntry trait interface, allowing uniform handling of
/// directory traversal across different backends.
///
/// # Examples
///
/// ```rust,ignore
/// use arsync::directory::DirectoryEntryWrapper;
/// use arsync::traits::AsyncDirectoryEntry;
///
/// for entry in std::fs::read_dir("/some/path")? {
///     let entry = entry?;
///     let wrapper = DirectoryEntryWrapper::new(entry);
///     println!("Entry: {}", wrapper.name());
/// }
/// ```
pub struct DirectoryEntryWrapper {
    /// The underlying directory entry
    entry: std::fs::DirEntry,
    /// Cached name to avoid lifetime issues
    name: String,
    /// Cached path
    path: PathBuf,
}

impl DirectoryEntryWrapper {
    /// Create a new wrapper around a directory entry
    ///
    /// # Parameters
    ///
    /// * `entry` - The directory entry to wrap
    ///
    /// # Returns
    ///
    /// Returns a new `DirectoryEntryWrapper` instance
    #[must_use]
    pub fn new(entry: std::fs::DirEntry) -> Self {
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();

        Self { entry, name, path }
    }

    /// Get a reference to the underlying entry
    ///
    /// # Returns
    ///
    /// Returns a reference to the wrapped `std::fs::DirEntry`
    #[must_use]
    pub fn inner(&self) -> &std::fs::DirEntry {
        &self.entry
    }

    /// Consume the wrapper and return the underlying entry
    ///
    /// # Returns
    ///
    /// Returns the wrapped `std::fs::DirEntry`
    #[must_use]
    pub fn into_inner(self) -> std::fs::DirEntry {
        self.entry
    }
}

impl AsyncDirectoryEntry for DirectoryEntryWrapper {
    type Metadata = compio_fs_extended::FileMetadata;

    fn name(&self) -> &str {
        &self.name
    }

    fn path(&self) -> &Path {
        &self.path
    }

    async fn metadata(&self) -> Result<Self::Metadata> {
        // Get std metadata and convert
        use std::os::unix::fs::MetadataExt;
        use std::time::SystemTime;

        let m = self
            .entry
            .metadata()
            .map_err(|e| SyncError::FileSystem(format!("Failed to get entry metadata: {e}")))?;

        Ok(compio_fs_extended::FileMetadata {
            size: m.len(),
            mode: m.mode(),
            uid: m.uid(),
            gid: m.gid(),
            nlink: m.nlink(),
            ino: m.ino(),
            dev: m.dev(),
            accessed: m.accessed().unwrap_or(SystemTime::UNIX_EPOCH),
            modified: m.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            created: m.created().ok(),
            #[cfg(target_os = "linux")]
            attributes: None,
            #[cfg(target_os = "linux")]
            attributes_mask: None,
            #[cfg(target_os = "macos")]
            flags: None,
            #[cfg(target_os = "macos")]
            generation: None,
        })
    }
}
