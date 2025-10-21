//! Core types for directory traversal and copying
//!
//! This module contains the data structures used throughout directory operations:
//! - `FileLocation`: Groups path, parent `DirectoryFd`, and filename
//! - `TraversalContext`: Shared state passed through recursion
//! - `DirectoryStats`: Statistics tracking
//! - `ExtendedMetadata`: Wrapper around compio's file metadata

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

/// Extended metadata using `io_uring` statx or compio metadata
///
/// This wraps `compio_fs_extended::FileMetadata` which uses `io_uring` STATX
/// for maximum performance and TOCTOU-safety when created via dirfd.
pub struct ExtendedMetadata {
    /// The underlying filesystem metadata (from `io_uring` statx or fallback)
    pub metadata: compio_fs_extended::FileMetadata,
}

impl ExtendedMetadata {
    /// Create extended metadata from path (fallback - uses blocking statx syscall)
    ///
    /// ⚠️ DEPRECATED: This uses path-based statx which is TOCTOU-vulnerable
    /// and doesn't use `io_uring`. Prefer creating from `DirectoryFd` when possible.
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The path does not exist
    /// - Permission is denied to read the path
    /// - The path is not accessible
    #[allow(clippy::future_not_send)]
    #[allow(clippy::items_after_statements)]
    pub async fn new(path: &Path) -> Result<Self> {
        // TODO: Use DirectoryFd-based approach instead
        // For now, fall back to compio::fs::metadata and convert
        let compio_metadata = compio::fs::symlink_metadata(path).await.map_err(|e| {
            SyncError::FileSystem(format!(
                "Failed to get metadata for {}: {}",
                path.display(),
                e
            ))
        })?;

        use std::os::unix::fs::MetadataExt;
        let metadata = compio_fs_extended::FileMetadata {
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
        };
        Ok(Self { metadata })
    }

    /// Create from `io_uring` statx result (TOCTOU-safe, async)
    ///
    /// This is the preferred constructor - uses `io_uring` and is TOCTOU-safe.
    #[must_use]
    pub const fn from_statx(metadata: compio_fs_extended::FileMetadata) -> Self {
        Self { metadata }
    }

    /// Create using `DirectoryFd` (TOCTOU-safe, `io_uring`)
    ///
    /// This is the most efficient constructor - uses dirfd + `io_uring` statx.
    ///
    /// # Errors
    ///
    /// Returns error if statx operation fails
    #[cfg(target_os = "linux")]
    #[allow(clippy::future_not_send)]
    pub async fn from_dirfd(
        dir: &compio_fs_extended::DirectoryFd,
        filename: &std::ffi::OsStr,
    ) -> Result<Self> {
        let metadata = dir.statx_full(filename).await.map_err(|e| {
            SyncError::FileSystem(format!(
                "Failed to get metadata via dirfd for {}: {e}",
                filename.to_string_lossy()
            ))
        })?;
        Ok(Self::from_statx(metadata))
    }

    /// Check if this is a directory
    #[must_use]
    pub fn is_dir(&self) -> bool {
        self.metadata.is_dir()
    }

    /// Check if this is a regular file
    #[must_use]
    pub fn is_file(&self) -> bool {
        self.metadata.is_file()
    }

    /// Check if this is a symlink
    #[must_use]
    pub fn is_symlink(&self) -> bool {
        self.metadata.is_symlink()
    }

    /// Get file size
    #[must_use]
    pub const fn len(&self) -> u64 {
        self.metadata.size
    }

    /// Check if file is empty
    #[allow(dead_code)]
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.metadata.size == 0
    }

    /// Get device ID (for filesystem boundary detection)
    #[must_use]
    pub const fn device_id(&self) -> u64 {
        self.metadata.dev
    }

    /// Get inode number (for hardlink detection)
    #[must_use]
    pub const fn inode_number(&self) -> u64 {
        self.metadata.ino
    }

    /// Get link count (for hardlink detection)
    #[must_use]
    pub const fn link_count(&self) -> u64 {
        self.metadata.nlink
    }
}
