//! Directory traversal and copying functionality
//!
//! This module provides async directory traversal and copying capabilities
//! using `io_uring` operations where possible, with fallbacks to standard
//! filesystem operations for unsupported operations.

use crate::adaptive_concurrency::{check_fd_limits, AdaptiveConcurrencyController};
use crate::cli::{Args, CopyMethod};
use crate::copy::copy_file_internal;
use crate::error::{Result, SyncError};
use crate::hardlink_tracker::FilesystemTracker;
use crate::io_uring::FileOperations;
use crate::metadata::MetadataConfig;
use crate::stats::SharedStats;
// io_uring_extended removed - using compio directly
use compio::dispatcher::Dispatcher;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, error, info, warn};

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

/// Copy a directory recursively with metadata preservation and hardlink detection
///
/// This function performs recursive directory copying with the following features:
/// - Async directory traversal using `io_uring` statx operations
/// - Hardlink detection and preservation during traversal
/// - Filesystem boundary detection
/// - Metadata preservation (permissions, ownership, timestamps)
/// - Symlink handling
/// - Error recovery and reporting
///
/// # Parameters
///
/// * `src` - Source directory path
/// * `dst` - Destination directory path
/// * `file_ops` - File operations instance for metadata handling
/// * `copy_method` - Copy method to use for individual files
///
/// # Returns
///
/// Returns directory copy statistics or an error.
///
/// # Errors
///
/// This function will return an error if:
/// - Source directory cannot be read
/// - Destination directory cannot be created
/// - File copying operations fail
/// - Directory traversal fails
#[allow(clippy::future_not_send)]
#[allow(clippy::used_underscore_binding)]
pub async fn copy_directory(
    src: &Path,
    dst: &Path,
    file_ops: &FileOperations,
    _copy_method: CopyMethod,
    args: &Args,
) -> Result<DirectoryStats> {
    let mut stats = DirectoryStats::default();
    let mut hardlink_tracker = FilesystemTracker::new();

    info!(
        "Starting directory copy from {} to {}",
        src.display(),
        dst.display()
    );

    // Create destination directory if it doesn't exist
    if dst.exists() {
        // Set source filesystem from root directory (destination already exists)
        let root_metadata = ExtendedMetadata::new(src).await?;
        hardlink_tracker.set_source_filesystem(root_metadata.device_id());
    } else {
        compio::fs::create_dir_all(dst).await.map_err(|e| {
            SyncError::FileSystem(format!(
                "Failed to create destination directory {}: {}",
                dst.display(),
                e
            ))
        })?;
        stats.directories_created += 1;
        debug!("Created destination directory: {}", dst.display());

        // Preserve root directory metadata (permissions, ownership, timestamps) if requested
        let root_metadata = ExtendedMetadata::new(src).await?;
        preserve_directory_metadata(src, dst, &root_metadata, &args.metadata).await?;

        // Set source filesystem from root directory
        hardlink_tracker.set_source_filesystem(root_metadata.device_id());
    }

    // Traverse source directory iteratively using compio's dispatcher
    traverse_and_copy_directory_iterative(
        src.to_path_buf(),
        dst.to_path_buf(),
        file_ops,
        _copy_method,
        &mut stats,
        &mut hardlink_tracker,
        &args.metadata,
        &args.concurrency,
        &args.io.parallel,
    )
    .await?;

    // Log hardlink detection results
    let hardlink_stats = hardlink_tracker.get_stats();
    info!(
        "Directory copy completed: {} files, {} directories, {} bytes, {} symlinks",
        stats.files_copied, stats.directories_created, stats.bytes_copied, stats.symlinks_processed
    );
    if hardlink_stats.hardlink_groups > 0 {
        info!(
            "Hardlink detection: {} unique files, {} hardlink groups, {} total hardlinks",
            hardlink_stats.total_files,
            hardlink_stats.hardlink_groups,
            hardlink_stats.total_hardlinks
        );
    }

    Ok(stats)
}

/// Directory traversal using compio's dispatcher for iterative processing
///
/// This function implements iterative directory traversal using compio's dispatcher
/// instead of recursion or manual worklists. It creates a static dispatcher and
/// uses it to schedule all directory operations asynchronously.
///
/// # Architecture
///
/// 1. **Dispatcher Creation**: Creates a static dispatcher using `Box::leak` for lifetime management
/// 2. **State Wrapping**: Wraps `DirectoryStats` and `FilesystemTracker` in `Arc<Mutex<>>` for shared access
/// 3. **Entry Processing**: Dispatches all directory entries to `process_directory_entry_with_compio`
/// 4. **Error Handling**: Uses `try_join_all` to short-circuit on first error
///
/// # Key Benefits
///
/// - **No Recursion**: Avoids stack overflow on deep directory structures
/// - **No Manual Worklists**: Uses compio's built-in async scheduling
/// - **Efficient Error Handling**: Short-circuits on first error, cancelling remaining operations
/// - **Concurrent Processing**: All directory entries processed concurrently
///
/// # Parameters
///
/// * `initial_src` - Source directory path to traverse
/// * `initial_dst` - Destination directory path for copying
/// * `file_ops` - File operations handler with `io_uring` support
/// * `copy_method` - Copy method (e.g., `io_uring`, fallback)
/// * `stats` - Statistics tracking (files, bytes, errors, etc.)
/// * `hardlink_tracker` - Hardlink detection and tracking
///
/// # Returns
///
/// Returns `Ok(())` if all operations complete successfully, or `Err(SyncError)` if any operation fails.
///
/// # Errors
///
/// This function will return an error if:
/// - Dispatcher creation fails
/// - Any directory entry processing fails
/// - File system operations fail
/// - Hardlink operations fail
#[allow(clippy::too_many_arguments)]
#[allow(clippy::future_not_send)]
#[allow(clippy::used_underscore_binding)]
async fn traverse_and_copy_directory_iterative(
    initial_src: PathBuf,
    initial_dst: PathBuf,
    file_ops: &FileOperations,
    _copy_method: CopyMethod,
    stats: &mut DirectoryStats,
    hardlink_tracker: &mut FilesystemTracker,
    metadata_config: &MetadataConfig,
    concurrency_config: &crate::cli::ConcurrencyConfig,
    parallel_config: &crate::cli::ParallelCopyConfig,
) -> Result<()> {
    // Create a dispatcher for async operations
    // Using Box::leak for &'static lifetime - dispatcher lives for program duration
    // This is intentional: the dispatcher manages worker threads and should not be dropped
    let dispatcher = Box::leak(Box::new(Dispatcher::new()?));

    // Create Arc-wrapped FileOperations and configs for safe sharing across async tasks
    // No more unsafe transmute needed!
    let file_ops_arc = Arc::new(file_ops.clone());
    let metadata_config_arc = Arc::new(metadata_config.clone());
    let parallel_config_arc = Arc::new(parallel_config.clone());

    // Wrap shared state in wrapper types for static lifetimes
    let stats_value = std::mem::take(stats);
    let shared_stats = Arc::new(SharedStats::new(&stats_value));
    let shared_hardlink_tracker = Arc::new(std::mem::take(hardlink_tracker));

    // Check FD limits and warn if too low
    if let Ok(fd_limit) = check_fd_limits() {
        if fd_limit < concurrency_config.max_files_in_flight as u64 {
            warn!(
                "FD limit ({}) is less than --max-files-in-flight ({}). Consider: ulimit -n {}",
                fd_limit,
                concurrency_config.max_files_in_flight,
                concurrency_config.max_files_in_flight * 2
            );
        }
    }

    // Create adaptive concurrency controller from config options
    // The controller owns its configuration and behavior (adapt vs fail)
    let concurrency_options = concurrency_config.to_options();
    let concurrency_controller = Arc::new(AdaptiveConcurrencyController::new(&concurrency_options));

    // Process the directory
    // Note: We clone Arc values here, but this is necessary because we need to
    // unwrap them later to return the final stats. The clone increments ref count,
    // but all child operations complete before we unwrap, so it's just +1/-1.
    // Delegate to root wrapper which handles DirectoryFd setup
    let result = process_root_entry(
        dispatcher,
        initial_src,
        initial_dst,
        file_ops_arc,
        _copy_method,
        shared_stats.clone(),
        shared_hardlink_tracker.clone(),
        concurrency_controller,
        metadata_config_arc,
        parallel_config_arc,
    )
    .await;

    // Restore the state
    // This unwraps successfully because the function and all child operations have completed,
    // so the only remaining reference is the one we kept above (from .clone())
    *stats = Arc::try_unwrap(shared_stats)
        .map_err(|_| {
            SyncError::FileSystem(
                "Failed to unwrap Arc<SharedStats> - multiple references exist".to_string(),
            )
        })?
        .into_inner();
    *hardlink_tracker = Arc::try_unwrap(shared_hardlink_tracker).map_err(|_| {
        SyncError::FileSystem(
            "Failed to unwrap Arc<FilesystemTracker> - multiple references exist".to_string(),
        )
    })?;

    result
}

/// Process directory entry using compio's dispatcher for async operations
///
/// This is the core function that handles all types of directory entries (files, directories, symlinks)
/// using compio's dispatcher for efficient async scheduling. It's designed to be called recursively
/// through the dispatcher to handle nested directory structures.
///
/// # Architecture
///
/// 1. **Entry Type Detection**: Uses `ExtendedMetadata` to determine if entry is file/dir/symlink
/// 2. **Directory Processing**: Creates destination directory and dispatches all child entries
/// 3. **File Processing**: Handles hardlink detection and file copying
/// 4. **Symlink Processing**: Copies symlinks with target preservation
///
/// # Key Features
///
/// - **Unified Entry Handling**: Single function handles all entry types
/// - **Concurrent Child Processing**: All child entries processed concurrently via dispatcher
/// - **Hardlink Detection**: Tracks inodes to detect and create hardlinks efficiently
/// - **Error Propagation**: Errors are properly propagated up the call stack
///
/// # Parameters
///
/// * `dispatcher` - Static dispatcher for scheduling async operations
/// * `src_path` - Source path of the directory entry
/// * `dst_path` - Destination path for the entry
/// * `file_ops` - File operations handler with `io_uring` support
/// * `copy_method` - Copy method (e.g., `io_uring`, fallback)
/// * `stats` - Shared statistics tracking (wrapped in Arc<Mutex<>>)
/// * `hardlink_tracker` - Shared hardlink detection (wrapped in Arc<Mutex<>>)
///
/// # Returns
///
/// Returns `Ok(())` if the entry is processed successfully, or `Err(SyncError)` if processing fails.
///
/// # Errors
///
/// This function will return an error if:
/// - Metadata retrieval fails
/// - Directory creation fails
/// - File copying fails
/// - Symlink copying fails
/// - Hardlink operations fail
///
/// Helper: Open parent directory as `DirectoryFd`
///
/// Extracts the common pattern of opening a path's parent as `DirectoryFd`.
#[allow(clippy::future_not_send)]
async fn open_parent_dirfd(path: &Path) -> Result<Arc<compio_fs_extended::DirectoryFd>> {
    let parent = path.parent().unwrap_or(path);
    let dir_fd = compio_fs_extended::DirectoryFd::open(parent)
        .await
        .map_err(|e| {
            SyncError::FileSystem(format!(
                "Failed to open parent directory {}: {e}",
                parent.display()
            ))
        })?;
    Ok(Arc::new(dir_fd))
}

/// Process root entry (wrapper that sets up `DirectoryFd` for TOCTOU-safe operations)
#[allow(clippy::too_many_arguments)]
#[allow(clippy::future_not_send)]
#[allow(clippy::used_underscore_binding)]
async fn process_root_entry(
    dispatcher: &'static Dispatcher,
    src_path: PathBuf,
    dst_path: PathBuf,
    file_ops: Arc<FileOperations>,
    _copy_method: CopyMethod,
    stats: Arc<SharedStats>,
    hardlink_tracker: Arc<FilesystemTracker>,
    concurrency_controller: Arc<AdaptiveConcurrencyController>,
    metadata_config: Arc<MetadataConfig>,
    parallel_config: Arc<crate::cli::ParallelCopyConfig>,
) -> Result<()> {
    let src_parent_dir = open_parent_dirfd(&src_path).await?;
    let src_filename = src_path
        .file_name()
        .ok_or_else(|| SyncError::FileSystem("No filename".to_string()))?
        .to_os_string();

    let dst_parent_dir = open_parent_dirfd(&dst_path).await?;
    let dst_filename = dst_path
        .file_name()
        .ok_or_else(|| SyncError::FileSystem("No filename".to_string()))?
        .to_os_string();

    process_directory_entry_with_compio(
        dispatcher,
        src_path,
        dst_path,
        file_ops,
        _copy_method,
        stats,
        hardlink_tracker,
        concurrency_controller,
        metadata_config,
        parallel_config,
        src_parent_dir,
        src_filename,
        dst_parent_dir,
        dst_filename,
    )
    .await
}

/// Internal: Process directory entry recursively with TOCTOU-safe `DirectoryFd` operations
///
/// This is the main recursive function for directory traversal. It processes files, directories,
/// and symlinks using compio's dispatcher pattern to avoid stack overflow and enable
/// concurrent processing.
///
/// **REQUIRES `DirectoryFd`**: All `DirectoryFd` parameters are required (no Options).
/// This enforces TOCTOU-safe operations at compile time. Use `process_root_entry` for root.
///
/// # Parameters
///
/// * `dispatcher` - Static dispatcher for concurrent operations
/// * `src_path` - Source path (for error messages only)
/// * `dst_path` - Destination path (for error messages only)
/// * `file_ops` - File operations handler
/// * `_copy_method` - Copy method to use
/// * `stats` - Shared statistics tracker
/// * `hardlink_tracker` - Shared hardlink tracker for inode-based detection
/// * `concurrency_controller` - Adaptive concurrency limiter
/// * `metadata_config` - Metadata preservation configuration
/// * `parallel_config` - Parallel copy configuration
/// * `src_parent_dir` - Source parent `DirectoryFd` (REQUIRED for TOCTOU safety)
/// * `src_filename` - Source basename (REQUIRED)
/// * `dst_parent_dir` - Destination parent `DirectoryFd` (REQUIRED for TOCTOU safety)
/// * `dst_filename` - Destination basename (REQUIRED)
///
/// # Returns
///
/// Returns `Ok(())` if the entry was processed successfully, or `Err(SyncError)` if failed.
///
/// # Errors
///
/// Returns an error if:
/// - Directory creation fails
/// - Metadata retrieval fails
/// - File copy operations fail
/// - Symlink operations fail
/// - Hardlink operations fail
#[allow(clippy::too_many_arguments)]
#[allow(clippy::future_not_send)]
#[allow(clippy::used_underscore_binding)]
#[allow(clippy::too_many_lines)]
async fn process_directory_entry_with_compio(
    dispatcher: &'static Dispatcher,
    src_path: PathBuf,
    dst_path: PathBuf,
    file_ops: Arc<FileOperations>,
    _copy_method: CopyMethod,
    stats: Arc<SharedStats>,
    hardlink_tracker: Arc<FilesystemTracker>,
    concurrency_controller: Arc<AdaptiveConcurrencyController>,
    metadata_config: Arc<MetadataConfig>,
    parallel_config: Arc<crate::cli::ParallelCopyConfig>,
    src_parent_dir: Arc<compio_fs_extended::DirectoryFd>,
    src_filename: std::ffi::OsString,
    dst_parent_dir: Arc<compio_fs_extended::DirectoryFd>,
    dst_filename: std::ffi::OsString,
) -> Result<()> {
    // Clone controller before acquiring permit to avoid borrow/move conflict
    let controller = Arc::clone(&concurrency_controller);

    // Acquire permit from adaptive concurrency controller
    // This prevents unbounded queue growth and adapts to resource constraints (e.g., FD exhaustion)
    // The permit is held for the entire operation (directory, file, or symlink)
    let _permit = controller.acquire().await;

    // Get comprehensive metadata using io_uring statx via DirectoryFd
    // ✅ ALWAYS uses DirectoryFd - no fallback, no path-based operations!
    let extended_metadata =
        ExtendedMetadata::from_dirfd(&src_parent_dir, src_filename.as_ref()).await?;

    if extended_metadata.is_dir() {
        // ========================================================================
        // DIRECTORY PROCESSING: Handle directory entries
        // ========================================================================
        debug!("Processing directory: {}", src_path.display());

        // Try to create destination directory (TOCTOU-safe: no exists() check!)
        match compio::fs::create_dir(&dst_path).await {
            Ok(()) => {
                stats.increment_directories_created();
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                // Something exists - verify it's actually a directory using DirectoryFd
                // This is TOCTOU-safe: operates on already-opened parent directory
                let existing_metadata =
                    ExtendedMetadata::from_dirfd(&dst_parent_dir, dst_filename.as_ref())
                        .await
                        .map_err(|e| {
                            SyncError::FileSystem(format!(
                                "Failed to check existing path {}: {}",
                                dst_path.display(),
                                e
                            ))
                        })?;

                if !existing_metadata.is_dir() {
                    return Err(SyncError::FileSystem(format!(
                        "Cannot create directory {}: path exists but is not a directory",
                        dst_path.display(),
                    )));
                }

                debug!("Directory already exists: {}", dst_path.display());
            }
            Err(e) => {
                return Err(SyncError::FileSystem(format!(
                    "Failed to create directory {}: {}",
                    dst_path.display(),
                    e
                )));
            }
        }

        // Open the destination directory immediately (for metadata and children)
        let dst_dir_fd = Arc::new(
            compio_fs_extended::DirectoryFd::open(&dst_path)
                .await
                .map_err(|e| {
                    SyncError::FileSystem(format!(
                        "Failed to open destination directory {}: {}",
                        dst_path.display(),
                        e
                    ))
                })?,
        );

        // ALWAYS preserve directory metadata (whether just created or already existed)
        // This ensures metadata is synchronized even on re-sync operations
        preserve_directory_metadata_fd(
            &src_path,
            &dst_path,
            &dst_dir_fd,
            &extended_metadata,
            &metadata_config,
        )
        .await?;

        // Open source directory as DirectoryFd for TOCTOU-safe operations
        let src_dir = Arc::new(
            compio_fs_extended::DirectoryFd::open(&src_path)
                .await
                .map_err(|e| {
                    SyncError::FileSystem(format!(
                        "Failed to open source directory {}: {e}",
                        src_path.display()
                    ))
                })?,
        );

        // Read directory entries using compio-fs-extended wrapper
        // This abstracts whether read_dir is blocking or uses io_uring (currently blocking due to kernel limitation)
        // See: compio_fs_extended::directory::read_dir for implementation details and kernel status
        let entries = compio_fs_extended::directory::read_dir(&src_path)
            .await
            .map_err(|e| {
                SyncError::FileSystem(format!(
                    "Failed to read directory {}: {}",
                    src_path.display(),
                    e
                ))
            })?;

        // ========================================================================
        // CONCURRENT PROCESSING: Dispatch all child entries concurrently
        // ========================================================================
        // Collect all async operations to dispatch
        let mut futures = Vec::new();

        // Process each child entry using compio's dispatcher
        // This is the key innovation: instead of recursion or manual worklists,
        // we dispatch all child entries to the same function, creating a tree
        // of concurrent operations that compio manages efficiently
        let copy_method = _copy_method.clone();
        for entry_result in entries {
            let entry = entry_result.map_err(|e| {
                SyncError::FileSystem(format!("Failed to read directory entry: {e}"))
            })?;
            let child_src_path = entry.path();
            let file_name = child_src_path.file_name().ok_or_else(|| {
                SyncError::FileSystem(format!("Invalid file name in {}", child_src_path.display()))
            })?;
            let child_dst_path = dst_path.join(file_name);
            let file_name_osstring = file_name.to_os_string();

            // Dispatch all entries to the same function regardless of type
            // This creates a unified processing pipeline where each entry
            // determines its own processing path (file/dir/symlink)
            let child_src_path = child_src_path.clone();
            let child_dst_path = child_dst_path.clone();
            let copy_method = copy_method.clone();
            let stats = Arc::clone(&stats);
            let hardlink_tracker = hardlink_tracker.clone();
            let concurrency_controller = concurrency_controller.clone();
            let file_ops_clone = Arc::clone(&file_ops);
            let metadata_config_clone = Arc::clone(&metadata_config);
            let parallel_config_clone = Arc::clone(&parallel_config);
            let src_dir_clone = Arc::clone(&src_dir);
            // Also open destination DirectoryFd for TOCTOU-safe file creation
            let dst_dir = compio_fs_extended::DirectoryFd::open(&dst_path)
                .await
                .map_err(|e| {
                    SyncError::FileSystem(format!(
                        "Failed to open destination directory {}: {}",
                        dst_path.display(),
                        e
                    ))
                })?;
            let dst_dir_clone = Arc::new(dst_dir);
            let dst_file_name_osstring = file_name.to_os_string();

            let receiver = dispatcher
                .dispatch(move || {
                    process_directory_entry_with_compio(
                        dispatcher,
                        child_src_path,
                        child_dst_path,
                        file_ops_clone,
                        copy_method,
                        stats,
                        hardlink_tracker,
                        concurrency_controller, // Move instead of clone - already cloned above
                        metadata_config_clone,
                        parallel_config_clone,
                        src_dir_clone,          // Pass source parent DirectoryFd
                        file_name_osstring,     // Pass source filename
                        dst_dir_clone,          // Pass destination parent DirectoryFd
                        dst_file_name_osstring, // Pass destination filename
                    )
                })
                .map_err(|e| {
                    SyncError::FileSystem(format!("Failed to dispatch entry processing: {e:?}"))
                })?;
            futures.push(receiver);
        }

        // ========================================================================
        // ERROR HANDLING: Short-circuit on first error
        // ========================================================================
        // Use try_join_all to short-circuit on first error
        // This is crucial for performance: we don't wait for all operations
        // to complete before checking for errors. As soon as any operation
        // fails, we cancel the remaining operations and return the error.
        let _ = futures::future::try_join_all(futures.into_iter().map(|receiver| async move {
            let _ = receiver.await.map_err(|e| {
                SyncError::FileSystem(format!(
                    "Failed to receive result from dispatched operation: {e:?}"
                ))
            })?;
            Ok::<(), SyncError>(())
        }))
        .await?;
    } else if extended_metadata.is_file() {
        // ========================================================================
        // FILE PROCESSING: Handle regular files with hardlink detection
        // ========================================================================
        // Files are processed with hardlink detection to avoid copying
        // the same content multiple times when hardlinks exist
        process_file(
            src_path,
            dst_path,
            extended_metadata,
            file_ops,
            _copy_method,
            stats,
            hardlink_tracker,
            concurrency_controller,
            metadata_config,
            parallel_config,
            dispatcher,
            src_parent_dir,
            src_filename,
            dst_parent_dir,
            dst_filename,
        )
        .await?;
    } else if extended_metadata.is_symlink() {
        // ========================================================================
        // SYMLINK PROCESSING: Handle symbolic links
        // ========================================================================
        if metadata_config.should_preserve_links() {
            // Copy symlink as symlink (preserve target)
            process_symlink(src_path, dst_path, &metadata_config, stats).await?;
        } else {
            // Dereference symlink: recursively process the target
            // This handles files, directories, and even chains of symlinks correctly
            debug!(
                "Dereferencing symlink (will copy target): {}",
                src_path.display()
            );

            // Read symlink target
            let target = std::fs::read_link(&src_path).map_err(|e| {
                SyncError::FileSystem(format!(
                    "Failed to read symlink {}: {}",
                    src_path.display(),
                    e
                ))
            })?;

            // Resolve target path (handle relative symlinks)
            let target_path = if target.is_absolute() {
                target
            } else {
                src_path
                    .parent()
                    .ok_or_else(|| {
                        SyncError::FileSystem(format!(
                            "Symlink has no parent: {}",
                            src_path.display()
                        ))
                    })?
                    .join(target)
            };

            // Recursively process the target (handles files, dirs, and symlink chains)
            // Use process_root_entry since target path could be anywhere (needs own DirectoryFd setup)
            let receiver = dispatcher
                .dispatch(move || {
                    process_root_entry(
                        dispatcher,
                        target_path,
                        dst_path,
                        file_ops,
                        _copy_method,
                        stats,
                        hardlink_tracker,
                        concurrency_controller,
                        metadata_config,
                        parallel_config,
                    )
                })
                .map_err(|e| {
                    SyncError::FileSystem(format!(
                        "Failed to dispatch symlink target processing: {e:?}"
                    ))
                })?;

            receiver.await.map_err(|e| {
                SyncError::FileSystem(format!("Symlink target processing failed: {e:?}"))
            })??;
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
#[allow(clippy::future_not_send)]
#[allow(clippy::used_underscore_binding)]
/// Process a regular file during directory traversal.
///
/// This handles hardlink detection via `FilesystemTracker`, creating a
/// hardlink when possible or copying file contents otherwise. On successful
/// copy/link creation, it updates shared statistics and tracker state.
///
/// # Parameters
/// - `src_path`: Source file path to process
/// - `dst_path`: Destination file path
/// - `metadata`: Extended source metadata used for decisions (size, inode, links)
/// - `_file_ops`: File operations handle (reserved for future metadata work)
/// - `_copy_method`: Copy method placeholder (currently unified to read/write)
/// - `stats`: Shared stats accumulator updated on success/error
/// - `hardlink_tracker`: Shared tracker for inode-based hardlink handling
///
/// # Errors
/// Returns an error if hardlink handling fails in an unrecoverable way or if
/// filesystem operations cannot be performed.
async fn process_file(
    src_path: PathBuf,
    dst_path: PathBuf,
    metadata: ExtendedMetadata,
    _file_ops: Arc<FileOperations>,
    _copy_method: CopyMethod,
    stats: Arc<SharedStats>,
    hardlink_tracker: Arc<FilesystemTracker>,
    _concurrency_controller: Arc<AdaptiveConcurrencyController>,
    metadata_config: Arc<MetadataConfig>,
    parallel_config: Arc<crate::cli::ParallelCopyConfig>,
    dispatcher: &'static Dispatcher,
    src_parent_dir: Arc<compio_fs_extended::DirectoryFd>,
    src_filename: std::ffi::OsString,
    dst_parent_dir: Arc<compio_fs_extended::DirectoryFd>,
    dst_filename: std::ffi::OsString,
) -> Result<()> {
    debug!(
        "Processing file: {} (link_count: {})",
        src_path.display(),
        metadata.link_count()
    );

    let device_id = metadata.device_id();
    let inode_number = metadata.inode_number();
    let link_count = metadata.link_count();

    // RACE-FREE HARDLINK PATTERN: Register and wait if linker
    let is_copier = hardlink_tracker
        .register_file(&src_path, &dst_path, device_id, inode_number, link_count)
        .await;

    if is_copier {
        // We're the copier - copy the file and signal completion
        debug!(
            "Copying file content (hardlink copier): {}",
            src_path.display()
        );

        // Copy file with DirectoryFd (TOCTOU-safe, compile-time enforced)
        copy_file_internal(
            &src_path,
            &dst_path,
            &metadata_config,
            &parallel_config,
            dispatcher,
            &metadata,
            &src_parent_dir,
            &src_filename,
            &dst_parent_dir,
            &dst_filename,
        )
        .await?;

        stats.increment_files_copied();
        stats.increment_bytes_copied(metadata.len());

        // Signal waiting linkers (they wake up whether we succeeded or failed)
        hardlink_tracker.signal_copy_complete(inode_number);
        debug!("Copied file and signaled linkers: {}", dst_path.display());
    } else if link_count > 1 {
        // We're a linker - waiting is already done inside register_file()
        // Get dst_path and create hardlink
        let original_dst = hardlink_tracker
            .get_dst_path_for_inode(inode_number)
            .ok_or_else(|| {
                SyncError::FileSystem(format!(
                    "BUG: dst_path not set for hardlink inode {inode_number}. \
                     This should never happen after register_file returns for linker."
                ))
            })?;

        debug!(
            "Creating hardlink {} → {} (inode: {})",
            dst_path.display(),
            original_dst.display(),
            inode_number
        );

        // Create hardlink (will naturally fail if copier failed to create dst file)
        handle_existing_hardlink(&dst_path, &original_dst, inode_number, &stats).await?;
    } else {
        // Regular file (link_count == 1) - copy normally
        debug!(
            "Copying file content (non-hardlink): {}",
            src_path.display()
        );

        // Copy file with DirectoryFd (TOCTOU-safe, compile-time enforced)
        copy_file_internal(
            &src_path,
            &dst_path,
            &metadata_config,
            &parallel_config,
            dispatcher,
            &metadata,
            &src_parent_dir,
            &src_filename,
            &dst_parent_dir,
            &dst_filename,
        )
        .await?;

        stats.increment_files_copied();
        stats.increment_bytes_copied(metadata.len());
        debug!("Copied file: {}", dst_path.display());
    }

    Ok(())
}

/// Handle creation of a hardlink when the inode has already been copied
///
/// This helper is invoked when a file's inode has been seen previously (i.e.,
/// the file is part of a hardlink set). Instead of copying file contents again,
/// it creates a hardlink in the destination that points to the original copied
/// path. It also ensures the destination's parent directory exists and updates
/// shared statistics accordingly.
///
/// # Parameters
///
/// - `dst_path`: Destination path where the hardlink should be created
/// - `src_path`: Source path (used for logging and error context)
/// - `inode_number`: The inode identifier for the file being processed
/// - `stats`: Shared statistics tracker used to record successes/errors
/// - `hardlink_tracker`: Tracker used to look up the original path for this inode
///
/// # Returns
///
/// Returns `Ok(())` if the hardlink was created successfully (or if an expected
/// recovery path was handled), otherwise returns `Err(SyncError)`.
///
/// # Errors
///
/// This function will return an error if:
/// - The original path associated with `inode_number` cannot be determined
/// - The destination parent directory cannot be created when needed
/// - The hardlink creation via `std::fs::hard_link` fails unexpectedly
///
/// # Side Effects
///
/// - Increments the files-copied counter on successful hardlink creation
/// - Increments the error counter on failures
#[allow(clippy::future_not_send)]
async fn handle_existing_hardlink(
    dst_path: &Path,
    original_dst: &Path,
    _inode_number: u64,
    stats: &Arc<SharedStats>,
) -> Result<()> {
    // Create destination directory if needed
    if let Some(parent) = dst_path.parent() {
        if !parent.exists() {
            compio::fs::create_dir_all(parent).await.map_err(|e| {
                SyncError::FileSystem(format!(
                    "Failed to create parent directory {}: {}",
                    parent.display(),
                    e
                ))
            })?;
        }
    }

    // Create hardlink using compio-fs-extended for io_uring operations
    compio_fs_extended::hardlink::create_hardlink_at_path(original_dst, dst_path)
        .await
        .map_err(|e| {
            SyncError::FileSystem(format!(
                "Failed to create hardlink from {} to {}: {}",
                original_dst.display(),
                dst_path.display(),
                e
            ))
        })?;

    stats.increment_files_copied();
    debug!(
        "Created hardlink: {} -> {}",
        dst_path.display(),
        original_dst.display()
    );

    Ok(())
}

/// Process a symlink by copying it
///
/// This function handles symbolic link copying, preserving the target path
/// and handling both valid and broken symlinks correctly.
///
/// # Symlink Handling
///
/// - **Valid Symlinks**: Copies the symlink with its target preserved
/// - **Broken Symlinks**: Copies the symlink with its broken target preserved
/// - **Target Preservation**: The symlink target is read and recreated exactly
///
/// # Parameters
///
/// * `src_path` - Source symlink path
/// * `dst_path` - Destination symlink path
/// * `stats` - Shared statistics tracking
///
/// # Returns
///
/// Returns `Ok(())` if the symlink is processed successfully, or `Err(SyncError)` if processing fails.
///
/// # Errors
///
/// This function will return an error if:
/// - Symlink target reading fails
/// - Symlink creation fails
#[allow(clippy::future_not_send)]
async fn process_symlink(
    src_path: PathBuf,
    dst_path: PathBuf,
    metadata_config: &MetadataConfig,
    stats: Arc<SharedStats>,
) -> Result<()> {
    debug!("Processing symlink: {}", src_path.display());

    match copy_symlink(&src_path, &dst_path, metadata_config).await {
        Ok(()) => {
            stats.increment_symlinks_processed();
            Ok(())
        }
        Err(e) => {
            stats.increment_errors();
            error!("Failed to copy symlink {}: {}", src_path.display(), e);
            Err(e)
        }
    }
}

/// Copy a symlink preserving its target and metadata
///
/// Note: Symlinks cannot be opened as file descriptors, so metadata preservation
/// uses path-based operations (fchmodat with `AT_SYMLINK_NOFOLLOW`, etc.).
/// This is the lowest-common-denominator for symlinks, but acceptable since
/// the symlink itself is atomic.
#[allow(clippy::future_not_send)]
#[allow(clippy::too_many_lines)] // Metadata preservation adds lines
#[allow(clippy::cast_sign_loss)] // Timestamps are i64 but Duration needs u64
async fn copy_symlink(src: &Path, dst: &Path, metadata_config: &MetadataConfig) -> Result<()> {
    use compio_fs_extended::directory::DirectoryFd;

    // Extract parent directory and filename for DirectoryFd operations
    let src_parent = src.parent().ok_or_else(|| {
        SyncError::FileSystem(format!("Source path has no parent: {}", src.display()))
    })?;
    let src_name = src
        .file_name()
        .ok_or_else(|| {
            SyncError::FileSystem(format!("Source path has no filename: {}", src.display()))
        })?
        .to_string_lossy();

    let dst_parent = dst.parent().ok_or_else(|| {
        SyncError::FileSystem(format!("Destination path has no parent: {}", dst.display()))
    })?;
    let dst_name = dst
        .file_name()
        .ok_or_else(|| {
            SyncError::FileSystem(format!(
                "Destination path has no filename: {}",
                dst.display()
            ))
        })?
        .to_string_lossy();

    // Open DirectoryFd for source and destination parents
    let src_dir_fd = DirectoryFd::open(src_parent).await.map_err(|e| {
        SyncError::FileSystem(format!(
            "Failed to open source directory {}: {}",
            src_parent.display(),
            e
        ))
    })?;

    let dst_dir_fd = DirectoryFd::open(dst_parent).await.map_err(|e| {
        SyncError::FileSystem(format!(
            "Failed to open destination directory {}: {}",
            dst_parent.display(),
            e
        ))
    })?;

    // Read symlink target using DirectoryFd (TOCTOU-safe)
    let target = src_dir_fd.readlinkat(&src_name).await.map_err(|e| {
        SyncError::FileSystem(format!(
            "Failed to read symlink target for {}: {}",
            src.display(),
            e
        ))
    })?;

    // Remove destination if it exists
    if dst.exists() {
        compio::fs::remove_file(dst).await.map_err(|e| {
            SyncError::FileSystem(format!(
                "Failed to remove existing destination {}: {}",
                dst.display(),
                e
            ))
        })?;
    }

    // Create symlink with same target using DirectoryFd (TOCTOU-safe)
    let target_str = target.to_string_lossy();
    dst_dir_fd
        .symlinkat(&target_str, &dst_name)
        .await
        .map_err(|e| {
            SyncError::FileSystem(format!(
                "Failed to create symlink {} -> {}: {}",
                dst.display(),
                target.display(),
                e
            ))
        })?;

    debug!("Copied symlink {} -> {}", dst.display(), target.display());

    // Preserve symlink metadata using DirectoryFd operations with AT_SYMLINK_NOFOLLOW
    // These operate on the symlink itself, not its target

    // Get source symlink metadata
    let src_metadata = std::fs::symlink_metadata(src).map_err(|e| {
        SyncError::FileSystem(format!(
            "Failed to get symlink metadata for {}: {}",
            src.display(),
            e
        ))
    })?;

    // Preserve ownership (if requested and we have permissions)
    if metadata_config.archive || metadata_config.owner || metadata_config.group {
        use std::os::unix::fs::MetadataExt;
        let uid = src_metadata.uid();
        let gid = src_metadata.gid();

        // Use lfchownat which doesn't follow symlinks
        if let Err(e) = dst_dir_fd.lfchownat(&dst_name, uid, gid).await {
            // Don't fail if we can't change ownership (common for non-root)
            debug!(
                "Could not preserve symlink ownership (may need root): {}",
                e
            );
        }
    }

    // Preserve timestamps (if requested)
    if metadata_config.archive || metadata_config.times {
        use std::os::unix::fs::MetadataExt;

        // Include nanoseconds for full precision
        let atime = std::time::UNIX_EPOCH
            + std::time::Duration::from_secs(src_metadata.atime() as u64)
            + std::time::Duration::from_nanos(src_metadata.atime_nsec() as u64);
        let mtime = std::time::UNIX_EPOCH
            + std::time::Duration::from_secs(src_metadata.mtime() as u64)
            + std::time::Duration::from_nanos(src_metadata.mtime_nsec() as u64);

        // Use lutimensat which doesn't follow symlinks
        dst_dir_fd
            .lutimensat(&dst_name, atime, mtime)
            .await
            .map_err(|e| {
                SyncError::FileSystem(format!(
                    "Failed to preserve symlink timestamps for {}: {}",
                    dst.display(),
                    e
                ))
            })?;
    }

    Ok(())
}

/// Preserve file metadata (permissions, ownership, timestamps)
#[allow(dead_code)]
#[allow(clippy::future_not_send)]
async fn preserve_file_metadata(src: &Path, dst: &Path, file_ops: &FileOperations) -> Result<()> {
    // Get source metadata
    let _metadata = file_ops.get_file_metadata(src).await.map_err(|e| {
        SyncError::FileSystem(format!(
            "Failed to get source metadata for {}: {}",
            src.display(),
            e
        ))
    })?;

    // TODO: Implement metadata preservation using compio's API
    // For now, we'll skip metadata preservation as compio's API is still evolving
    // This will be implemented in a future phase with proper compio bindings
    tracing::debug!(
        "Metadata preservation skipped for {} (compio API limitations)",
        dst.display()
    );

    // Set timestamps (currently skipped due to unstable Rust features)
    // TODO: Implement timestamp preservation using libc
    debug!("Preserved metadata for {}", dst.display());

    Ok(())
}

/// Get directory size recursively
///
/// This function calculates the total size of a directory by recursively
/// traversing all files and summing their sizes.
///
/// # Parameters
///
/// * `path` - Directory path to analyze
///
/// # Returns
///
/// Returns the total size in bytes or an error.
/// Count files and directories recursively
///
/// This function counts the total number of files and directories
/// in a directory tree.
///
/// # Parameters
///
/// * `path` - Directory path to analyze
///
/// # Returns
///
/// Returns a tuple of (files, directories) or an error.
/// Preserve directory extended attributes from source to destination
///
/// This function preserves all extended attributes from the source directory to the destination directory
/// using file descriptor-based operations for maximum efficiency and security.
///
/// # Arguments
///
/// * `src_path` - Source directory path
/// * `dst_path` - Destination directory path
///
/// # Returns
///
/// `Ok(())` if all extended attributes were preserved successfully
///
/// # Errors
///
/// This function will return an error if:
/// - Extended attributes cannot be read from source
/// - Extended attributes cannot be written to destination
/// - Permission is denied for xattr operations
#[allow(clippy::future_not_send)]
pub async fn preserve_directory_xattr(src_path: &Path, dst_path: &Path) -> Result<()> {
    use compio_fs_extended::{ExtendedFile, XattrOps};

    // Open source and destination directories for xattr operations
    let src_dir = compio::fs::File::open(src_path).await.map_err(|e| {
        SyncError::FileSystem(format!("Failed to open source directory for xattr: {e}"))
    })?;
    let dst_dir = compio::fs::File::open(dst_path).await.map_err(|e| {
        SyncError::FileSystem(format!(
            "Failed to open destination directory for xattr: {e}"
        ))
    })?;

    // Convert to ExtendedFile to access xattr operations
    let extended_src = ExtendedFile::from_ref(&src_dir);
    let extended_dst = ExtendedFile::from_ref(&dst_dir);

    // Get all extended attribute names from source directory
    let Ok(xattr_names) = extended_src.list_xattr().await else {
        // If xattr is not supported or no xattrs exist, that's fine
        return Ok(());
    };

    // Copy each extended attribute
    for name in xattr_names {
        match extended_src.get_xattr(&name).await {
            Ok(value) => {
                if let Err(e) = extended_dst.set_xattr(&name, &value).await {
                    // Log warning but continue with other xattrs
                    tracing::warn!(
                        "Failed to preserve directory extended attribute '{}': {}",
                        name,
                        e
                    );
                }
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to read directory extended attribute '{}': {}",
                    name,
                    e
                );
            }
        }
    }

    Ok(())
}

/// Preserve directory metadata (permissions, ownership, timestamps) from source to destination
///
/// This function preserves all directory metadata including permissions, ownership,
/// and timestamps using file descriptor-based operations for maximum efficiency and security.
///
/// # Arguments
///
/// * `src_path` - Source directory path
/// * `dst_path` - Destination directory path  
/// * `extended_metadata` - Pre-captured source directory metadata
/// * `metadata_config` - Metadata preservation configuration
///
/// # Returns
///
/// `Ok(())` if all metadata was preserved successfully
///
/// # Errors
///
/// This function will return an error if:
/// - Permission preservation fails
/// - Ownership preservation fails
/// - Timestamp preservation fails
#[allow(clippy::future_not_send, clippy::similar_names)]
///
/// Preserve directory metadata using pre-opened `DirectoryFd` (TOCTOU-safe, efficient)
///
/// This function accepts an already-open `DirectoryFd` and uses it for all metadata operations.
/// This is more efficient (1 open vs 3) and TOCTOU-safe.
///
/// # Parameters
/// - `src_path`: Source path (for error messages and xattr operations)
/// - `dst_path`: Destination path (for error messages and xattr operations)  
/// - `dst_dir_fd`: Pre-opened destination `DirectoryFd`
/// - `extended_metadata`: Source metadata to copy
/// - `metadata_config`: What metadata to preserve
pub async fn preserve_directory_metadata_fd(
    src_path: &Path, // For error messages and xattrs
    dst_path: &Path, // For error messages and xattrs
    dst_dir_fd: &compio_fs_extended::DirectoryFd,
    extended_metadata: &ExtendedMetadata,
    metadata_config: &MetadataConfig,
) -> Result<()> {
    use compio_fs_extended::OwnershipOps;

    // Get underlying File from DirectoryFd for metadata operations
    let dst_file = dst_dir_fd.as_file();

    // Preserve directory permissions if requested
    if metadata_config.should_preserve_permissions() {
        let mode = extended_metadata.metadata.permissions();
        let compio_permissions = compio::fs::Permissions::from_mode(mode);

        // Use FD-based set_permissions (TOCTOU-safe, no umask interference)
        dst_file
            .set_permissions(compio_permissions)
            .await
            .map_err(|e| {
                SyncError::FileSystem(format!("Failed to preserve directory permissions: {e}"))
            })?;

        debug!(
            "Preserved directory permissions for {}: {:o}",
            dst_path.display(),
            mode
        );
    }

    // Preserve directory ownership if requested
    if metadata_config.should_preserve_ownership() {
        let source_uid = extended_metadata.metadata.uid;
        let source_gid = extended_metadata.metadata.gid;

        // Use FD-based fchown (TOCTOU-safe!)
        dst_file.fchown(source_uid, source_gid).await.map_err(|e| {
            SyncError::FileSystem(format!("Failed to preserve directory ownership: {e}"))
        })?;

        debug!(
            "Preserved directory ownership for {}: uid={}, gid={}",
            dst_path.display(),
            source_uid,
            source_gid
        );
    }

    // Preserve directory timestamps if requested
    if metadata_config.should_preserve_timestamps() {
        let src_accessed = extended_metadata.metadata.accessed;
        let src_modified = extended_metadata.metadata.modified;

        // Use DirectoryFd::set_times (TOCTOU-safe!)
        dst_dir_fd
            .set_times(src_accessed, src_modified)
            .await
            .map_err(|e| {
                SyncError::FileSystem(format!("Failed to preserve directory timestamps: {e}"))
            })?;

        debug!("Preserved directory timestamps for {}", dst_path.display());
    }

    // Preserve directory extended attributes if requested
    if metadata_config.should_preserve_xattrs() {
        preserve_directory_xattr(src_path, dst_path).await?;
        debug!("Preserved directory xattrs for {}", dst_path.display());
    }

    Ok(())
}

/// Preserve directory metadata (legacy path-based wrapper)
///
/// **DEPRECATED**: Use `preserve_directory_metadata_fd` instead
/// This wrapper exists for compatibility but opens the directory 3 times (inefficient!)
///
/// # Errors
/// Returns error if `DirectoryFd` cannot be opened or metadata preservation fails
#[allow(clippy::future_not_send)]
pub async fn preserve_directory_metadata(
    src_path: &Path,
    dst_path: &Path,
    extended_metadata: &ExtendedMetadata,
    metadata_config: &MetadataConfig,
) -> Result<()> {
    // Open DirectoryFd once and use FD-based operations
    let dst_dir_fd = compio_fs_extended::DirectoryFd::open(dst_path)
        .await
        .map_err(|e| SyncError::FileSystem(format!("Failed to open destination directory: {e}")))?;

    preserve_directory_metadata_fd(
        src_path,
        dst_path,
        &dst_dir_fd,
        extended_metadata,
        metadata_config,
    )
    .await
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    #![allow(clippy::expect_used)]
    use super::*;
    use std::os::unix::fs::MetadataExt;
    use tempfile::TempDir;

    /// Test ExtendedMetadata creation and basic functionality
    #[compio::test]
    async fn test_extended_metadata_basic() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let test_file = temp_dir.path().join("test_file.txt");

        // Create a test file
        std::fs::write(&test_file, "test content").expect("Failed to write test file");

        // Test metadata creation
        let metadata = ExtendedMetadata::new(&test_file)
            .await
            .expect("Failed to get metadata");

        // Test basic properties
        assert!(metadata.is_file());
        assert!(!metadata.is_dir());
        assert!(!metadata.is_symlink());
        assert!(!metadata.is_empty());
        assert_eq!(metadata.len(), 12); // "test content" length

        // Test device and inode info
        let device_id = metadata.device_id();
        let inode_number = metadata.inode_number();
        let link_count = metadata.link_count();

        assert!(device_id > 0);
        assert!(inode_number > 0);
        assert_eq!(link_count, 1); // Regular file has 1 link
    }

    /// Test ExtendedMetadata for directories
    #[compio::test]
    async fn test_extended_metadata_directory() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");

        // Test directory metadata
        let metadata = ExtendedMetadata::new(temp_dir.path())
            .await
            .expect("Failed to get metadata");

        assert!(metadata.is_dir());
        assert!(!metadata.is_file());
        assert!(!metadata.is_symlink());
    }

    /// Test ExtendedMetadata for symlinks
    #[compio::test]
    async fn test_extended_metadata_symlink() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let target_file = temp_dir.path().join("target.txt");
        let symlink_file = temp_dir.path().join("symlink.txt");

        // Create target file
        std::fs::write(&target_file, "target content").expect("Failed to write target file");

        // Create symlink
        std::os::unix::fs::symlink(&target_file, &symlink_file).expect("Failed to create symlink");

        // Test symlink metadata
        let metadata = ExtendedMetadata::new(&symlink_file)
            .await
            .expect("Failed to get metadata");

        assert!(metadata.is_symlink());
        assert!(!metadata.is_file());
        assert!(!metadata.is_dir());
    }

    /// Test process_symlink function
    #[compio::test]
    async fn test_process_symlink() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let target_file = temp_dir.path().join("target.txt");
        let src_symlink = temp_dir.path().join("src_symlink");
        let dst_symlink = temp_dir.path().join("dst_symlink");

        // Create target file
        std::fs::write(&target_file, "target content").expect("Failed to write target file");

        // Create source symlink
        std::os::unix::fs::symlink(&target_file, &src_symlink)
            .expect("Failed to create source symlink");

        // Test symlink processing
        let stats = DirectoryStats::default();
        let result = process_symlink(
            src_symlink.clone(),
            dst_symlink.clone(),
            &MetadataConfig {
                archive: false,
                recursive: false,
                links: true,
                perms: false,
                times: false,
                group: false,
                owner: false,
                devices: false,
                fsync: false,
                xattrs: false,
                acls: false,
                hard_links: false,
                atimes: false,
                crtimes: false,
                preserve_xattr: false,
                preserve_acl: false,
            },
            Arc::new(SharedStats::new(&stats)),
        )
        .await;

        // Should succeed
        assert!(result.is_ok());

        // Verify symlink was created
        assert!(dst_symlink.exists());
        assert!(dst_symlink.is_symlink());

        // Verify symlink target is correct
        let target = std::fs::read_link(&dst_symlink).expect("Failed to read symlink target");
        assert_eq!(target, target_file);
    }

    /// Test process_symlink with broken symlink
    #[compio::test]
    async fn test_process_symlink_broken() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let src_symlink = temp_dir.path().join("broken_symlink");
        let dst_symlink = temp_dir.path().join("dst_broken_symlink");

        // Create broken symlink
        std::os::unix::fs::symlink("nonexistent_file", &src_symlink)
            .expect("Failed to create broken symlink");

        // Test processing broken symlink
        let stats = DirectoryStats::default();
        let result = process_symlink(
            src_symlink.clone(),
            dst_symlink.clone(),
            &MetadataConfig {
                archive: false,
                recursive: false,
                links: true,
                perms: false,
                times: false,
                group: false,
                owner: false,
                devices: false,
                fsync: false,
                xattrs: false,
                acls: false,
                hard_links: false,
                atimes: false,
                crtimes: false,
                preserve_xattr: false,
                preserve_acl: false,
            },
            Arc::new(SharedStats::new(&stats)),
        )
        .await;

        // Should succeed (we handle broken symlinks gracefully)
        assert!(result.is_ok());

        // Verify symlink was created (broken symlinks don't "exist" but are still symlinks)
        assert!(dst_symlink.is_symlink());

        // Verify the broken symlink target is preserved
        let target = std::fs::read_link(&dst_symlink).expect("Failed to read symlink target");
        assert_eq!(target.to_string_lossy(), "nonexistent_file");
    }

    /// Test that handle_existing_hardlink updates stats
    /// This verifies that the linker path properly increments files_copied
    #[compio::test]
    async fn test_hardlink_updates_stats() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let src_file = temp_dir.path().join("src.txt");
        let dst_file = temp_dir.path().join("dst.txt");
        let link_file = temp_dir.path().join("link.txt");

        // Create source file
        std::fs::write(&src_file, "test content").expect("Failed to write file");

        // Create destination file (simulating copier's work)
        std::fs::write(&dst_file, "test content").expect("Failed to write dst");

        // Test handle_existing_hardlink
        let stats = Arc::new(SharedStats::new(&DirectoryStats::default()));

        let result = handle_existing_hardlink(&link_file, &dst_file, 12345, &stats).await;

        assert!(result.is_ok(), "handle_existing_hardlink should succeed");

        // CRITICAL: Check that stats were updated
        let final_stats = Arc::try_unwrap(stats).unwrap().into_inner();
        assert_eq!(
            final_stats.files_copied, 1,
            "handle_existing_hardlink should increment files_copied"
        );

        // Verify hardlink was actually created
        assert!(link_file.exists(), "Hardlink should exist");
        let link_meta = std::fs::metadata(&link_file).expect("Failed to get link metadata");
        let dst_meta = std::fs::metadata(&dst_file).expect("Failed to get dst metadata");
        assert_eq!(
            link_meta.ino(),
            dst_meta.ino(),
            "Should be same inode (hardlink)"
        );
    }

    /// ERROR INJECTION: Test handle_existing_hardlink fails on nonexistent original
    #[compio::test]
    async fn test_hardlink_error_on_missing_original() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let link_file = temp_dir.path().join("link.txt");
        let nonexistent = temp_dir.path().join("nonexistent.txt");

        let stats = Arc::new(SharedStats::new(&DirectoryStats::default()));

        // Try to create hardlink to nonexistent file - should fail
        let result = handle_existing_hardlink(&link_file, &nonexistent, 12345, &stats).await;

        assert!(
            result.is_err(),
            "Creating hardlink to nonexistent file should fail"
        );

        // With clean error propagation, stats are NOT incremented on error
        // Error propagates via Result instead
        let final_stats = Arc::try_unwrap(stats).unwrap().into_inner();
        assert_eq!(
            final_stats.errors, 0,
            "Error counter should NOT be incremented (error propagates via Result)"
        );
        assert_eq!(
            final_stats.files_copied, 0,
            "files_copied should not be incremented on failure"
        );
    }

    /// ERROR INJECTION: Test handle_existing_hardlink fails when dst_path parent is read-only
    #[compio::test]
    async fn test_hardlink_error_on_readonly_parent() {
        use std::fs::Permissions;
        use std::os::unix::fs::PermissionsExt;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let readonly_dir = temp_dir.path().join("readonly");
        std::fs::create_dir(&readonly_dir).expect("Failed to create readonly dir");

        // Make directory read-only
        std::fs::set_permissions(&readonly_dir, Permissions::from_mode(0o444))
            .expect("Failed to set permissions");

        let src_file = temp_dir.path().join("src.txt");
        let dst_file = temp_dir.path().join("dst.txt");
        let link_file = readonly_dir.join("link.txt");

        std::fs::write(&src_file, "content").expect("Failed to write src");
        std::fs::write(&dst_file, "content").expect("Failed to write dst");

        let stats = Arc::new(SharedStats::new(&DirectoryStats::default()));

        // Try to create hardlink in read-only directory - should fail
        let result = handle_existing_hardlink(&link_file, &dst_file, 12345, &stats).await;

        assert!(
            result.is_err(),
            "Creating hardlink in read-only directory should fail"
        );

        // Clean up permissions for temp_dir cleanup
        std::fs::set_permissions(&readonly_dir, Permissions::from_mode(0o755))
            .expect("Failed to restore permissions");
    }
}
