//! Directory traversal and copying functionality
//!
//! This module provides async directory traversal and copying capabilities
//! using `io_uring` operations where possible, with fallbacks to standard
//! filesystem operations for unsupported operations.

use crate::adaptive_concurrency::{check_fd_limits, AdaptiveConcurrencyController};
use crate::cli::{Args, CopyMethod};
use crate::copy::copy_file;
use crate::error::{Result, SyncError};
use crate::io_uring::FileOperations;
use crate::metadata::MetadataConfig;
use crate::stats::SharedStats;
// io_uring_extended removed - using compio directly
use compio::dispatcher::Dispatcher;
use compio_sync::Semaphore;
use dashmap::DashMap;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tracing::{debug, info, warn};

/// Wrapper for shared hardlink tracking across async tasks
///
/// This struct wraps `FilesystemTracker` in `Arc<Mutex<>>` to allow shared access
/// across multiple async tasks dispatched by compio's dispatcher. It provides
/// thread-safe hardlink detection and tracking for efficient file copying.
///
/// # Hardlink Detection
///
/// The tracker maintains a mapping of inodes to file paths, allowing it to:
/// - Detect when multiple files share the same content (hardlinks)
/// - Create hardlinks instead of copying the same content multiple times
/// - Track which inodes have already been copied
///
/// # Thread Safety
///
/// All methods are thread-safe and can be called concurrently from different
/// async tasks without additional synchronization.
///
/// # Usage
///
/// ```rust,ignore
/// let tracker = SharedHardlinkTracker::new(FilesystemTracker::new());
/// tracker.register_file(path, device_id, inode, link_count);
/// if tracker.is_inode_copied(inode) {
///     // Create hardlink instead of copying
/// }
/// ```
#[derive(Clone)]
pub struct SharedHardlinkTracker {
    /// Inner tracker wrapped in Arc for thread-safe shared access
    /// No Mutex needed - `DashMap` provides interior mutability
    inner: Arc<FilesystemTracker>,
}

impl SharedHardlinkTracker {
    /// Create a new `SharedHardlinkTracker` wrapper
    ///
    /// # Arguments
    ///
    /// * `tracker` - The initial filesystem tracker to wrap
    #[must_use]
    pub fn new(tracker: FilesystemTracker) -> Self {
        Self {
            inner: Arc::new(tracker),
        }
    }

    /// Create a new `SharedHardlinkTracker` with source filesystem set
    ///
    /// # Arguments
    ///
    /// * `source_filesystem` - Optional source filesystem device ID
    #[allow(dead_code)]
    #[must_use]
    pub fn with_source_filesystem(source_filesystem: Option<u64>) -> Self {
        let mut tracker = FilesystemTracker::new();
        if let Some(dev) = source_filesystem {
            tracker.set_source_filesystem(dev);
        }
        Self::new(tracker)
    }

    /// Signal that an inode's copy is complete
    ///
    /// This sets the destination path and wakes all tasks waiting to create hardlinks.
    pub fn signal_copy_complete(&self, inode: u64, path: &Path) {
        self.inner.signal_copy_complete(inode, path);
    }

    /// Get destination path for a copied inode (after waiting on condvar)
    #[must_use]
    pub fn get_dst_path_for_inode(&self, inode: u64) -> Option<PathBuf> {
        self.inner.get_dst_path_for_inode(inode)
    }

    /// Register a file with the hardlink tracker (race-free)
    ///
    /// Returns (`is_copier`, `optional_condvar`) to determine role and synchronization
    #[must_use]
    pub fn register_file(
        &self,
        path: &Path,
        device_id: u64,
        inode: u64,
        link_count: u64,
    ) -> (bool, Option<Arc<compio_sync::Condvar>>) {
        self.inner.register_file(path, device_id, inode, link_count)
    }

    #[allow(dead_code)]
    /// Get filesystem tracking statistics
    #[must_use]
    pub fn get_stats(&self) -> FilesystemStats {
        self.inner.get_stats()
    }

    /// Extract the inner `FilesystemTracker` from the shared wrapper
    ///
    /// # Errors
    ///
    /// This function will return an error if multiple references to the Arc exist.
    pub fn into_inner(self) -> Result<FilesystemTracker> {
        Arc::try_unwrap(self.inner).map_err(|_| {
            SyncError::FileSystem("Failed to unwrap Arc - multiple references exist".to_string())
        })
    }
}

/// Wrapper for shared semaphore to limit concurrent operations
///
/// This struct wraps a `Semaphore` in an `Arc` to allow shared access
/// across multiple async tasks dispatched by compio's dispatcher. It provides
/// concurrency control to prevent unbounded queue growth during BFS traversal.
///
/// # Thread Safety
///
/// The semaphore is thread-safe and can be used concurrently from different
/// async tasks without additional synchronization.
///
/// # Usage
///
/// ```rust,ignore
/// let semaphore = SharedSemaphore::new(100);
/// let permit = semaphore.acquire().await;
/// // ... perform bounded concurrent operation ...
/// drop(permit); // Release permit
/// ```
#[derive(Clone)]
pub struct SharedSemaphore {
    /// Inner semaphore wrapped in Arc for shared access
    inner: Arc<Semaphore>,
}

impl SharedSemaphore {
    /// Create a new `SharedSemaphore` wrapper
    ///
    /// # Arguments
    ///
    /// * `permits` - The maximum number of concurrent permits
    #[must_use]
    pub fn new(permits: usize) -> Self {
        Self {
            inner: Arc::new(Semaphore::new(permits)),
        }
    }

    /// Acquire a permit from the semaphore
    ///
    /// This will block until a permit is available.
    pub async fn acquire(&self) -> compio_sync::SemaphorePermit<'_> {
        self.inner.acquire().await
    }

    /// Get the number of available permits
    #[must_use]
    #[allow(dead_code)] // Used by adaptive concurrency controller (not yet integrated)
    pub fn available_permits(&self) -> usize {
        self.inner.available_permits()
    }

    /// Get the maximum number of permits
    #[must_use]
    #[allow(dead_code)] // Used by adaptive concurrency controller (not yet integrated)
    pub fn max_permits(&self) -> usize {
        self.inner.max_permits()
    }

    /// Reduce available permits (for adaptive concurrency control)
    ///
    /// Returns the actual number of permits reduced.
    #[must_use]
    #[allow(dead_code)] // Used by adaptive concurrency controller (not yet integrated)
    pub fn reduce_permits(&self, count: usize) -> usize {
        self.inner.reduce_permits(count)
    }

    /// Add permits back (for adaptive concurrency control)
    #[allow(dead_code)] // Used by adaptive concurrency controller (not yet integrated)
    pub fn add_permits(&self, count: usize) {
        self.inner.add_permits(count);
    }
}

/// Extended metadata using `compio::fs` metadata support
pub struct ExtendedMetadata {
    /// The underlying filesystem metadata (from `compio::fs`)
    pub metadata: compio::fs::Metadata,
}

impl ExtendedMetadata {
    /// Create extended metadata using compio's built-in metadata support
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The path does not exist
    /// - Permission is denied to read the path
    /// - The path is not accessible
    #[allow(clippy::future_not_send)]
    pub async fn new(path: &Path) -> Result<Self> {
        // Use compio::fs::symlink_metadata for async metadata retrieval
        let metadata = compio::fs::symlink_metadata(path).await.map_err(|e| {
            SyncError::FileSystem(format!(
                "Failed to get metadata for {}: {}",
                path.display(),
                e
            ))
        })?;
        Ok(Self { metadata })
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
        self.metadata.file_type().is_symlink()
    }

    /// Get file size
    #[must_use]
    pub fn len(&self) -> u64 {
        self.metadata.len()
    }

    /// Check if file is empty
    #[allow(dead_code)]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.metadata.len() == 0
    }

    /// Get device ID (for filesystem boundary detection)
    #[must_use]
    pub fn device_id(&self) -> u64 {
        self.metadata.dev()
    }

    /// Get inode number (for hardlink detection)
    #[must_use]
    pub fn inode_number(&self) -> u64 {
        self.metadata.ino()
    }

    /// Get link count (for hardlink detection)
    #[must_use]
    pub fn link_count(&self) -> u64 {
        self.metadata.nlink()
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
) -> Result<()> {
    // Create a dispatcher for async operations
    let dispatcher = Box::leak(Box::new(Dispatcher::new()?));

    // Create Arc-wrapped FileOperations and configs for safe sharing across async tasks
    // No more unsafe transmute needed!
    let file_ops_arc = Arc::new(file_ops.clone());
    let metadata_config_arc = Arc::new(metadata_config.clone());

    // Wrap shared state in wrapper types for static lifetimes
    let stats_value = std::mem::take(stats);
    let shared_stats = Arc::new(SharedStats::new(&stats_value));
    let shared_hardlink_tracker = SharedHardlinkTracker::new(std::mem::take(hardlink_tracker));

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
    let result = process_directory_entry_with_compio(
        dispatcher,
        initial_src,
        initial_dst,
        file_ops_arc,
        _copy_method,
        shared_stats.clone(),
        shared_hardlink_tracker.clone(),
        concurrency_controller,
        metadata_config_arc,
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
    *hardlink_tracker = shared_hardlink_tracker.into_inner()?;

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
#[allow(clippy::too_many_arguments)]
#[allow(clippy::future_not_send)]
#[allow(clippy::used_underscore_binding)]
async fn process_directory_entry_with_compio(
    dispatcher: &'static Dispatcher,
    src_path: PathBuf,
    dst_path: PathBuf,
    file_ops: Arc<FileOperations>,
    _copy_method: CopyMethod,
    stats: Arc<SharedStats>,
    hardlink_tracker: SharedHardlinkTracker,
    concurrency_controller: Arc<AdaptiveConcurrencyController>,
    metadata_config: Arc<MetadataConfig>,
) -> Result<()> {
    // Clone controller before acquiring permit to avoid borrow/move conflict
    let controller = Arc::clone(&concurrency_controller);

    // Acquire permit from adaptive concurrency controller
    // This prevents unbounded queue growth and adapts to resource constraints (e.g., FD exhaustion)
    // The permit is held for the entire operation (directory, file, or symlink)
    let _permit = controller.acquire().await;

    // Get comprehensive metadata using compio's async operations
    let extended_metadata = ExtendedMetadata::new(&src_path).await?;

    if extended_metadata.is_dir() {
        // ========================================================================
        // DIRECTORY PROCESSING: Handle directory entries
        // ========================================================================
        debug!("Processing directory: {}", src_path.display());

        // Create destination directory using compio's dispatcher
        if !dst_path.exists() {
            compio::fs::create_dir(&dst_path).await.map_err(|e| {
                SyncError::FileSystem(format!(
                    "Failed to create directory {}: {}",
                    dst_path.display(),
                    e
                ))
            })?;
            stats.increment_directories_created();

            // Preserve directory metadata (permissions, ownership, timestamps) if requested
            preserve_directory_metadata(&src_path, &dst_path, &extended_metadata, &metadata_config)
                .await?;
        }

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
        )
        .await?;
    } else if extended_metadata.is_symlink() {
        // ========================================================================
        // SYMLINK PROCESSING: Handle symbolic links
        // ========================================================================
        // Symlinks are copied with their target preserved, including
        // broken symlinks (which is the correct behavior)
        process_symlink(src_path, dst_path, stats).await?;
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
    hardlink_tracker: SharedHardlinkTracker,
    concurrency_controller: Arc<AdaptiveConcurrencyController>,
    metadata_config: Arc<MetadataConfig>,
) -> Result<()> {
    debug!(
        "Processing file: {} (link_count: {})",
        src_path.display(),
        metadata.link_count()
    );

    let device_id = metadata.device_id();
    let inode_number = metadata.inode_number();
    let link_count = metadata.link_count();

    // RACE-FREE HARDLINK PATTERN: Atomically determine copier vs linker
    let (is_copier, maybe_condvar) =
        hardlink_tracker.register_file(&src_path, device_id, inode_number, link_count);

    if is_copier {
        // We're the copier - copy the file and signal completion
        debug!("Copying file content (copier): {}", src_path.display());

        match copy_file(&src_path, &dst_path, &metadata_config).await {
            Ok(()) => {
                stats.increment_files_copied();
                stats.increment_bytes_copied(metadata.len());

                // Signal all waiting linkers that copy is complete
                hardlink_tracker.signal_copy_complete(inode_number, dst_path.as_path());
                debug!("Copied file and signaled linkers: {}", dst_path.display());
            }
            Err(e) => {
                // Handle error - controller will either adapt or fail based on configuration
                concurrency_controller.handle_error(&e)?;

                warn!(
                    "Failed to copy file {} -> {}: {}",
                    src_path.display(),
                    dst_path.display(),
                    e
                );
                stats.increment_errors();

                // Still signal condvar even on error (with empty dst_path)
                // This prevents linkers from waiting forever
                hardlink_tracker.signal_copy_complete(inode_number, Path::new(""));
            }
        }
    } else if let Some(condvar) = maybe_condvar {
        // We're a linker - wait for copier to finish, then create hardlink
        debug!(
            "Waiting for copy to complete (linker): {}",
            src_path.display()
        );

        // Wait for copier to signal completion
        condvar.wait().await;

        // Get the destination path from the copier
        if let Some(original_dst) = hardlink_tracker.get_dst_path_for_inode(inode_number) {
            if original_dst.as_os_str().is_empty() {
                // Copier failed - we should also fail
                warn!(
                    "Original copy failed, skipping hardlink creation for {}",
                    src_path.display()
                );
                stats.increment_errors();
            } else {
                handle_existing_hardlink(&dst_path, &original_dst, inode_number, &stats).await?;
            }
        }
    } else {
        // Not a hardlink (link_count == 1) - copy normally
        debug!(
            "Copying file content (non-hardlink): {}",
            src_path.display()
        );

        match copy_file(&src_path, &dst_path, &metadata_config).await {
            Ok(()) => {
                stats.increment_files_copied();
                stats.increment_bytes_copied(metadata.len());
                debug!("Copied file: {}", dst_path.display());
            }
            Err(e) => {
                concurrency_controller.handle_error(&e)?;

                warn!(
                    "Failed to copy file {} -> {}: {}",
                    src_path.display(),
                    dst_path.display(),
                    e
                );
                stats.increment_errors();
            }
        }
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
    inode_number: u64,
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
    match compio_fs_extended::hardlink::create_hardlink_at_path(original_dst, dst_path).await {
        Ok(()) => {
            stats.increment_files_copied();
            debug!(
                "Created hardlink: {} -> {}",
                dst_path.display(),
                original_dst.display()
            );
        }
        Err(e) => {
            warn!(
                "Failed to create hardlink for inode {}: {}",
                inode_number, e
            );
            stats.increment_errors();
        }
    }

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
    stats: Arc<SharedStats>,
) -> Result<()> {
    debug!("Processing symlink: {}", src_path.display());

    match copy_symlink(&src_path, &dst_path).await {
        Ok(()) => {
            stats.increment_symlinks_processed();
            Ok(())
        }
        Err(e) => {
            stats.increment_errors();
            warn!("Failed to copy symlink {}: {}", src_path.display(), e);
            Err(e)
        }
    }
}

/// Copy a symlink preserving its target
#[allow(clippy::future_not_send)]
async fn copy_symlink(src: &Path, dst: &Path) -> Result<()> {
    use compio_fs_extended::directory::DirectoryFd;
    use compio_fs_extended::symlink::{create_symlink_at_dirfd, read_symlink_at_dirfd};

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

    // Read symlink target using io_uring DirectoryFd operations
    let target = read_symlink_at_dirfd(&src_dir_fd, &src_name)
        .await
        .map_err(|e| {
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

    // Create symlink with same target using io_uring DirectoryFd operations
    let target_str = target.to_string_lossy();
    create_symlink_at_dirfd(&dst_dir_fd, &target_str, &dst_name)
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
/// Filesystem boundary detection and hardlink tracking
///
/// This module provides functionality for detecting filesystem boundaries
/// and tracking hardlink relationships to ensure proper file copying behavior.
/// Filesystem device ID and inode number pair for hardlink detection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InodeInfo {
    /// Filesystem device ID
    pub dev: u64,
    /// Inode number
    pub ino: u64,
}

/// Hardlink tracking information with concurrent synchronization
///
/// Uses `Condvar` to ensure race-free hardlink creation:
/// - First task to register becomes the "copier" (inserts with `dst_path`)
/// - Subsequent tasks wait on the condvar until copy completes
/// - Copier signals condvar when destination file is created
pub struct HardlinkInfo {
    /// Original file path (immutable after creation)
    #[allow(dead_code)]
    pub original_path: std::path::PathBuf,
    /// Inode number (immutable after creation)
    pub inode_number: u64,
    /// Number of hardlinks found (incremented atomically)
    pub link_count: AtomicU64,
    /// Destination path set by copier task (written once, read many)
    pub dst_path: Mutex<Option<std::path::PathBuf>>,
    /// Condition variable signaled when copy completes
    /// Linker tasks wait on this before creating hardlinks
    pub copy_complete: Arc<compio_sync::Condvar>,
}

impl std::fmt::Debug for HardlinkInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HardlinkInfo")
            .field("original_path", &self.original_path)
            .field("inode_number", &self.inode_number)
            .field("link_count", &self.link_count.load(Ordering::Relaxed))
            .field("dst_path", &self.dst_path.lock().ok())
            .finish_non_exhaustive() // Omitting copy_complete condvar
    }
}

/// Filesystem boundary and hardlink tracker
#[derive(Debug, Default)]
#[allow(dead_code)]
pub struct FilesystemTracker {
    /// Map of (dev, ino) pairs to hardlink information
    hardlinks: DashMap<InodeInfo, HardlinkInfo>,
    /// Source filesystem device ID (for boundary detection)
    source_filesystem: Option<u64>,
}

#[allow(dead_code)]
impl FilesystemTracker {
    /// Create a new filesystem tracker
    #[must_use]
    pub fn new() -> Self {
        Self {
            hardlinks: DashMap::new(),
            source_filesystem: None,
        }
    }

    /// Set the source filesystem device ID
    ///
    /// This should be called once at the beginning of a copy operation
    /// to establish the source filesystem boundary.
    pub fn set_source_filesystem(&mut self, dev: u64) {
        self.source_filesystem = Some(dev);
        debug!("Set source filesystem device ID: {}", dev);
    }

    /// Check if a path is on the same filesystem as the source
    ///
    /// Returns true if the path is on the same filesystem, false otherwise.
    /// This prevents cross-filesystem operations that could cause issues.
    #[must_use]
    pub fn is_same_filesystem(&self, dev: u64) -> bool {
        self.source_filesystem.map_or_else(
            || {
                warn!("No source filesystem set, allowing cross-filesystem operation");
                true
            },
            |source_dev| source_dev == dev,
        )
    }

    /// Register a file for hardlink tracking
    ///
    /// This atomically determines if this task should be the "copier" or a "linker":
    /// - If this is the first registration: returns `(true, None)` - caller is copier
    /// - If already registered: returns `(false, Some(condvar))` - caller is linker, must wait
    ///
    /// # Arguments
    /// * `path` - Path to the file being registered
    /// * `dev` - Device ID
    /// * `ino` - Inode number
    /// * `link_count` - Number of hardlinks (from stat)
    ///
    /// # Returns
    /// * `(true, None)` - First registration, caller should copy file
    /// * `(false, Some(condvar))` - Already registered, caller should wait on condvar then link
    /// * `(false, None)` - Not a hardlink (`link_count` == 1), caller should copy normally
    pub fn register_file(
        &self,
        path: &Path,
        dev: u64,
        ino: u64,
        link_count: u64,
    ) -> (bool, Option<Arc<compio_sync::Condvar>>) {
        // Skip files with link count of 1 - they're not hardlinks
        if link_count == 1 {
            return (false, None);
        }

        let inode_info = InodeInfo { dev, ino };

        // ATOMIC PATTERN: Try to insert, get back whether we won the race
        match self.hardlinks.entry(inode_info) {
            dashmap::mapref::entry::Entry::Occupied(entry) => {
                // This inode is already registered - we're a linker
                let hardlink_info = entry.get();
                hardlink_info.link_count.fetch_add(1, Ordering::Relaxed);
                debug!(
                    "Found hardlink #{} for inode ({}, {}): {} (will wait for copy)",
                    hardlink_info.link_count.load(Ordering::Relaxed),
                    dev,
                    ino,
                    path.display()
                );
                (false, Some(Arc::clone(&hardlink_info.copy_complete)))
            }
            dashmap::mapref::entry::Entry::Vacant(entry) => {
                // We're the first - we're the copier
                entry.insert(HardlinkInfo {
                    original_path: path.to_path_buf(),
                    inode_number: ino,
                    link_count: AtomicU64::new(1),
                    dst_path: Mutex::new(None),
                    copy_complete: Arc::new(compio_sync::Condvar::new()),
                });
                debug!(
                    "Registered new hardlink inode ({}, {}): {} (will copy)",
                    dev,
                    ino,
                    path.display()
                );
                (true, None)
            }
        }
    }

    /// Get all hardlink groups that have multiple links
    ///
    /// Returns a vector of (`inode_number`, `link_count`) tuples for inodes with multiple hardlinks.
    #[must_use]
    pub fn get_hardlink_groups(&self) -> Vec<(u64, u64)> {
        self.hardlinks
            .iter()
            .filter(|entry| entry.value().link_count.load(Ordering::Relaxed) > 1)
            .map(|entry| {
                let val = entry.value();
                (val.inode_number, val.link_count.load(Ordering::Relaxed))
            })
            .collect()
    }

    /// Signal that an inode's copy is complete and store its destination path
    ///
    /// This should be called by the copier task after successfully creating the destination file.
    /// It sets the destination path and signals all waiting linker tasks via the condvar.
    ///
    /// # Arguments
    /// * `ino` - Inode number that was copied
    /// * `dst_path` - Path where the file was copied to
    pub fn signal_copy_complete(&self, ino: u64, dst_path: &Path) {
        // Find the hardlink info for this inode (drop iterator before accessing)
        let entry_opt = self
            .hardlinks
            .iter()
            .find(|e| e.value().inode_number == ino)
            .map(|e| (*e.key(), Arc::clone(&e.value().copy_complete)));

        if let Some((_key, condvar)) = entry_opt {
            // Set destination path (re-lookup and drop iterator quickly)
            {
                let found = self
                    .hardlinks
                    .iter()
                    .find(|e| e.value().inode_number == ino);
                if let Some(entry) = found {
                    let hardlink_info = entry.value();
                    if let Ok(mut dst) = hardlink_info.dst_path.lock() {
                        *dst = Some(dst_path.to_path_buf());
                    }
                }
            } // Iterator dropped here

            // Signal all waiting linker tasks (iterator is dropped)
            condvar.notify_all();

            debug!(
                "Signaled copy complete for inode {} at {}",
                ino,
                dst_path.display()
            );
        }
    }

    /// Get the destination path where an inode was copied
    ///
    /// Returns the destination path if it has been set by the copier task.
    /// This should only be called after waiting on the condvar.
    #[must_use]
    pub fn get_dst_path_for_inode(&self, ino: u64) -> Option<PathBuf> {
        self.hardlinks
            .iter()
            .find(|entry| entry.value().inode_number == ino)
            .and_then(|entry| entry.value().dst_path.lock().ok()?.clone())
    }

    /// Get statistics about the filesystem tracking
    #[must_use]
    pub fn get_stats(&self) -> FilesystemStats {
        let total_files = self.hardlinks.len();
        let hardlink_groups = self.get_hardlink_groups().len();
        let total_hardlinks: u64 = self
            .hardlinks
            .iter()
            .map(|entry| entry.value().link_count.load(Ordering::Relaxed))
            .sum();

        FilesystemStats {
            total_files,
            hardlink_groups,
            total_hardlinks,
            source_filesystem: self.source_filesystem,
        }
    }
}

/// Statistics about filesystem tracking
#[derive(Debug)]
pub struct FilesystemStats {
    /// Total number of unique files (by inode)
    pub total_files: usize,
    /// Number of hardlink groups (files with multiple links)
    pub hardlink_groups: usize,
    /// Total number of hardlinks (including originals)
    pub total_hardlinks: u64,
    /// Source filesystem device ID
    #[allow(dead_code)]
    pub source_filesystem: Option<u64>,
}

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
pub async fn preserve_directory_metadata(
    src_path: &Path,
    dst_path: &Path,
    extended_metadata: &ExtendedMetadata,
    metadata_config: &MetadataConfig,
) -> Result<()> {
    use compio_fs_extended::{metadata, OwnershipOps};

    // Preserve directory permissions if requested
    if metadata_config.should_preserve_permissions() {
        let src_permissions = extended_metadata.metadata.permissions();
        let mode = src_permissions.mode();
        let compio_permissions = compio::fs::Permissions::from_mode(mode);

        // Open destination directory for permission operations
        let dst_dir = compio::fs::File::open(dst_path).await.map_err(|e| {
            SyncError::FileSystem(format!(
                "Failed to open destination directory for permissions: {e}"
            ))
        })?;

        // Use file descriptor-based set_permissions to avoid umask interference
        dst_dir
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
        let source_uid = extended_metadata.metadata.uid();
        let source_gid = extended_metadata.metadata.gid();

        // Open destination directory for ownership operations
        let dst_dir = compio::fs::File::open(dst_path).await.map_err(|e| {
            SyncError::FileSystem(format!(
                "Failed to open destination directory for ownership: {e}"
            ))
        })?;

        // Set ownership using fchown
        dst_dir.fchown(source_uid, source_gid).await.map_err(|e| {
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
        let src_accessed = extended_metadata.metadata.accessed().map_err(|e| {
            SyncError::FileSystem(format!("Failed to get source directory access time: {e}"))
        })?;
        let src_modified = extended_metadata.metadata.modified().map_err(|e| {
            SyncError::FileSystem(format!(
                "Failed to get source directory modification time: {e}"
            ))
        })?;

        // Use compio-fs-extended for timestamp preservation
        metadata::futimesat(dst_path, src_accessed, src_modified)
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    #![allow(clippy::expect_used)]
    use super::*;
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

    /// Test FilesystemTracker basic functionality
    #[compio::test]
    async fn test_filesystem_tracker_basic() {
        let mut tracker = FilesystemTracker::new();

        // Test initial state (no mutation needed for get_stats)
        assert_eq!(tracker.get_stats().total_files, 0);
        assert_eq!(tracker.get_stats().hardlink_groups, 0);

        // Test setting source filesystem (requires mut)
        tracker.set_source_filesystem(123);
        assert!(tracker.is_same_filesystem(123));
        assert!(!tracker.is_same_filesystem(456));
    }

    /// Test FilesystemTracker hardlink detection
    #[compio::test]
    async fn test_filesystem_tracker_hardlinks() {
        let tracker = FilesystemTracker::new();
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let file1 = temp_dir.path().join("file1.txt");
        let file2 = temp_dir.path().join("file2.txt");

        // Create a file
        std::fs::write(&file1, "content").expect("Failed to write file");

        // Create hardlink
        std::fs::hard_link(&file1, &file2).expect("Failed to create hardlink");

        // Register first file (should be copier)
        let (is_copier, condvar) = tracker.register_file(&file1, 1, 100, 2);
        assert!(is_copier); // Should be copier (first registration)
        assert!(condvar.is_none()); // Copier doesn't get condvar

        // Register hardlink (should be linker)
        let (is_copier, condvar) = tracker.register_file(&file2, 1, 100, 2);
        assert!(!is_copier); // Should not be copier (it's a hardlink)
        assert!(condvar.is_some()); // Linker gets condvar to wait on

        // Check stats
        let stats = tracker.get_stats();
        assert_eq!(stats.total_files, 1);
        assert_eq!(stats.hardlink_groups, 1);
        assert_eq!(stats.total_hardlinks, 2);
    }
}
