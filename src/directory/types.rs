//! Core types for directory traversal and copying
//!
//! This module contains the data structures used throughout directory operations:
//! - `FileLocation`: Groups path, parent `DirectoryFd`, and filename
//! - `TraversalContext`: Shared state passed through recursion
//! - `DirectoryStats`: Statistics tracking

use crate::adaptive_concurrency::AdaptiveConcurrencyController;
use crate::cli::CopyMethod;
use crate::error::{Result, SyncError};
use crate::io_uring::FileOperations;
use crate::metadata::MetadataConfig;
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
