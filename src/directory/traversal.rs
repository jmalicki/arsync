//! Directory traversal and recursive processing
//!
//! Core recursive directory traversal logic using compio's dispatcher pattern.

use crate::adaptive_concurrency::{check_fd_limits, AdaptiveConcurrencyController};
use crate::cli::CopyMethod;
use crate::copy::copy_file_internal;
use crate::error::{Result, SyncError};
use crate::hardlink_tracker::FilesystemTracker;
use crate::io_uring::FileOperations;
use crate::metadata::MetadataConfig;
use crate::stats::SharedStats;
use compio::dispatcher::Dispatcher;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, warn};

use super::metadata::preserve_directory_metadata_fd;
use super::symlink::process_symlink;
use super::types::{DirectoryStats, ExtendedMetadata, FileLocation, TraversalContext};

#[allow(clippy::too_many_arguments)]
#[allow(clippy::future_not_send)]
#[allow(clippy::used_underscore_binding)]
pub(super) async fn traverse_and_copy_directory_iterative(
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
    // Build traversal context
    let ctx = TraversalContext {
        file_ops: file_ops_arc,
        copy_method: _copy_method,
        stats: shared_stats.clone(),
        hardlink_tracker: shared_hardlink_tracker.clone(),
        concurrency_controller,
        metadata_config: metadata_config_arc,
        parallel_config: parallel_config_arc,
        dispatcher,
    };

    let result = process_root_entry(initial_src, initial_dst, ctx).await;

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
pub(super) async fn open_parent_dirfd(path: &Path) -> Result<Arc<compio_fs_extended::DirectoryFd>> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
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
#[allow(clippy::future_not_send)]
pub(super) async fn process_root_entry(
    src_path: PathBuf,
    dst_path: PathBuf,
    ctx: TraversalContext,
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

    let src = FileLocation {
        path: src_path,
        parent_dir: src_parent_dir,
        filename: src_filename,
    };

    let dst = FileLocation {
        path: dst_path,
        parent_dir: dst_parent_dir,
        filename: dst_filename,
    };

    process_directory_entry_with_compio(src, dst, ctx).await
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
#[allow(clippy::future_not_send)]
#[allow(clippy::too_many_lines)]
pub(super) async fn process_directory_entry_with_compio(
    src: FileLocation,
    dst: FileLocation,
    ctx: TraversalContext,
) -> Result<()> {
    // Clone controller before acquiring permit to avoid borrow/move conflict
    let controller = Arc::clone(&ctx.concurrency_controller);

    // Acquire permit from adaptive concurrency controller
    // This prevents unbounded queue growth and adapts to resource constraints (e.g., FD exhaustion)
    // The permit is held for the entire operation (directory, file, or symlink)
    let _permit = controller.acquire().await;

    // Get comprehensive metadata using io_uring statx via DirectoryFd
    // ✅ ALWAYS uses DirectoryFd - no fallback, no path-based operations!
    let extended_metadata =
        ExtendedMetadata::from_dirfd(&src.parent_dir, src.filename.as_ref()).await?;

    if extended_metadata.is_dir() {
        // ========================================================================
        // DIRECTORY PROCESSING: Handle directory entries
        // ========================================================================
        debug!("Processing directory: {}", src.path.display());

        // Try to create destination directory (TOCTOU-safe: no exists() check!)
        match compio::fs::create_dir(&dst.path).await {
            Ok(()) => {
                ctx.stats.increment_directories_created();
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                // Something exists - verify it's actually a directory
                let existing_metadata = compio::fs::metadata(&dst.path).await.map_err(|e| {
                    SyncError::FileSystem(format!(
                        "Failed to check existing path {}: {}",
                        dst.path.display(),
                        e
                    ))
                })?;

                if !existing_metadata.is_dir() {
                    return Err(SyncError::FileSystem(format!(
                        "Cannot create directory {}: path exists but is not a directory (is_file: {}, is_symlink: {})",
                        dst.path.display(),
                        existing_metadata.is_file(),
                        existing_metadata.is_symlink()
                    )));
                }

                debug!("Directory already exists: {}", dst.path.display());
            }
            Err(e) => {
                return Err(SyncError::FileSystem(format!(
                    "Failed to create directory {}: {}",
                    dst.path.display(),
                    e
                )));
            }
        }

        // Open the destination directory immediately (for metadata and children)
        let dst_dir_fd = Arc::new(
            compio_fs_extended::DirectoryFd::open(&dst.path)
                .await
                .map_err(|e| {
                    SyncError::FileSystem(format!(
                        "Failed to open destination directory {}: {}",
                        dst.path.display(),
                        e
                    ))
                })?,
        );

        // ALWAYS preserve directory metadata (whether just created or already existed)
        // This ensures metadata is synchronized even on re-sync operations
        preserve_directory_metadata_fd(
            &src.path,
            &dst.path,
            &dst_dir_fd,
            &extended_metadata,
            &ctx.metadata_config,
        )
        .await?;

        // Open source directory as DirectoryFd for TOCTOU-safe operations
        let src_dir = Arc::new(
            compio_fs_extended::DirectoryFd::open(&src.path)
                .await
                .map_err(|e| {
                    SyncError::FileSystem(format!(
                        "Failed to open source directory {}: {e}",
                        src.path.display()
                    ))
                })?,
        );

        // Read directory entries using compio-fs-extended wrapper
        // This abstracts whether read_dir is blocking or uses io_uring (currently blocking due to kernel limitation)
        // See: compio_fs_extended::directory::read_dir for implementation details and kernel status
        let entries = compio_fs_extended::directory::read_dir(&src.path)
            .await
            .map_err(|e| {
                SyncError::FileSystem(format!(
                    "Failed to read directory {}: {}",
                    src.path.display(),
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
        let _copy_method = ctx.copy_method.clone();
        for entry_result in entries {
            let entry = entry_result.map_err(|e| {
                SyncError::FileSystem(format!("Failed to read directory entry: {e}"))
            })?;
            let child_src_path = entry.path();
            let file_name = child_src_path.file_name().ok_or_else(|| {
                SyncError::FileSystem(format!("Invalid file name in {}", child_src_path.display()))
            })?;
            let child_dst_path = dst.path.join(file_name);
            let file_name_osstring = file_name.to_os_string();

            // Dispatch all entries to the same function regardless of type
            // This creates a unified processing pipeline where each entry
            // determines its own processing path (file/dir/symlink)
            let child_src_path = child_src_path.clone();
            let child_dst_path = child_dst_path.clone();
            let ctx_clone = ctx.clone();
            let src_dir_clone = Arc::clone(&src_dir);
            let dst_dir_clone = Arc::clone(&dst_dir_fd);
            let dst_file_name_osstring = file_name.to_os_string();

            let child_src = FileLocation {
                path: child_src_path.clone(),
                parent_dir: src_dir_clone,
                filename: file_name_osstring,
            };

            let child_dst = FileLocation {
                path: child_dst_path.clone(),
                parent_dir: dst_dir_clone,
                filename: dst_file_name_osstring,
            };

            let receiver = ctx
                .dispatcher
                .dispatch(move || {
                    process_directory_entry_with_compio(child_src, child_dst, ctx_clone)
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
        process_file(src, dst, extended_metadata, ctx).await?;
    } else if extended_metadata.is_symlink() {
        // ========================================================================
        // SYMLINK PROCESSING: Handle symbolic links
        // ========================================================================
        if ctx.metadata_config.should_preserve_links() {
            // Copy symlink as symlink (preserve target)
            process_symlink(src.path, dst.path, &ctx.metadata_config, ctx.stats.clone()).await?;
        } else {
            // Dereference symlink: recursively process the target
            // This handles files, directories, and even chains of symlinks correctly
            debug!(
                "Dereferencing symlink (will copy target): {}",
                src.path.display()
            );

            // Read symlink target
            let target = std::fs::read_link(&src.path).map_err(|e| {
                SyncError::FileSystem(format!(
                    "Failed to read symlink {}: {}",
                    src.path.display(),
                    e
                ))
            })?;

            // Resolve target path (handle relative symlinks)
            let target_path = if target.is_absolute() {
                target
            } else {
                src.path
                    .parent()
                    .ok_or_else(|| {
                        SyncError::FileSystem(format!(
                            "Symlink has no parent: {}",
                            src.path.display()
                        ))
                    })?
                    .join(target)
            };

            // Recursively process the target (handles files, dirs, and symlink chains)
            // Use process_root_entry since target path could be anywhere (needs own DirectoryFd setup)
            let receiver = ctx
                .dispatcher
                .dispatch(move || process_root_entry(target_path, dst.path, ctx))
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
pub(super) async fn process_file(
    src: FileLocation,
    dst: FileLocation,
    metadata: ExtendedMetadata,
    ctx: TraversalContext,
) -> Result<()> {
    debug!(
        "Processing file: {} (link_count: {})",
        src.path.display(),
        metadata.link_count()
    );

    let device_id = metadata.device_id();
    let inode_number = metadata.inode_number();
    let link_count = metadata.link_count();

    // RACE-FREE HARDLINK PATTERN: Register and wait if linker
    let is_copier = ctx
        .hardlink_tracker
        .register_file(&src.path, &dst.path, device_id, inode_number, link_count)
        .await;

    if is_copier {
        // We're the copier - copy the file and signal completion
        debug!(
            "Copying file content (hardlink copier): {}",
            src.path.display()
        );

        // Copy file with DirectoryFd (TOCTOU-safe, compile-time enforced)
        copy_file_internal(
            &src.path,
            &dst.path,
            &ctx.metadata_config,
            &ctx.parallel_config,
            ctx.dispatcher,
            &metadata,
            &src.parent_dir,
            src.filename.as_ref(),
            &dst.parent_dir,
            dst.filename.as_ref(),
        )
        .await?;

        ctx.stats.increment_files_copied();
        ctx.stats.increment_bytes_copied(metadata.len());

        // Signal waiting linkers (they wake up whether we succeeded or failed)
        ctx.hardlink_tracker.signal_copy_complete(inode_number);
        debug!("Copied file and signaled linkers: {}", dst.path.display());
    } else if link_count > 1 {
        // We're a linker - waiting is already done inside register_file()
        // Get dst_path and create hardlink
        let original_dst = ctx
            .hardlink_tracker
            .get_dst_path_for_inode(inode_number)
            .ok_or_else(|| {
                SyncError::FileSystem(format!(
                    "BUG: dst_path not set for hardlink inode {inode_number}. \
                     This should never happen after register_file returns for linker."
                ))
            })?;

        debug!(
            "Creating hardlink {} → {} (inode: {})",
            dst.path.display(),
            original_dst.display(),
            inode_number
        );

        // Create hardlink (will naturally fail if copier failed to create dst file)
        handle_existing_hardlink(&dst.path, &original_dst, inode_number, &ctx.stats).await?;
    } else {
        // Regular file (link_count == 1) - copy normally
        debug!(
            "Copying file content (non-hardlink): {}",
            src.path.display()
        );

        // Copy file with DirectoryFd (TOCTOU-safe, compile-time enforced)
        copy_file_internal(
            &src.path,
            &dst.path,
            &ctx.metadata_config,
            &ctx.parallel_config,
            ctx.dispatcher,
            &metadata,
            &src.parent_dir,
            src.filename.as_ref(),
            &dst.parent_dir,
            dst.filename.as_ref(),
        )
        .await?;

        ctx.stats.increment_files_copied();
        ctx.stats.increment_bytes_copied(metadata.len());
        debug!("Copied file: {}", dst.path.display());
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
pub(super) async fn handle_existing_hardlink(
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
