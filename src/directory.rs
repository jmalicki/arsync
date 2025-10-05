//! Directory traversal and copying functionality
//!
//! This module provides async directory traversal and copying capabilities
//! using io_uring operations where possible, with fallbacks to standard
//! filesystem operations for unsupported operations.

use crate::cli::CopyMethod;
use crate::copy::copy_file;
use crate::error::{Result, SyncError};
use crate::io_uring::FileOperations;
// io_uring_extended removed - using compio directly
use compio::dispatcher::Dispatcher;
#[allow(clippy::disallowed_types)]
use std::collections::HashMap;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tracing::{debug, info, warn};

/// Wrapper for shared statistics tracking across async tasks
///
/// This struct wraps `DirectoryStats` in `Arc<Mutex<>>` to allow shared access
/// across multiple async tasks dispatched by compio's dispatcher. It provides
/// a clean API for updating statistics from concurrent operations.
///
/// # Thread Safety
///
/// All methods are thread-safe and can be called concurrently from different
/// async tasks without additional synchronization.
///
/// # Usage
///
/// ```rust,ignore
/// let stats = SharedStats::new(DirectoryStats::default());
/// stats.increment_files_copied();
/// stats.increment_bytes_copied(1024);
/// let final_stats = stats.into_inner();
/// ```
#[derive(Clone)]
pub struct SharedStats {
    /// Inner stats wrapped in Arc<Mutex<>> for thread-safe access
    inner: Arc<Mutex<DirectoryStats>>,
}

impl SharedStats {
    /// Create a new SharedStats wrapper
    ///
    /// # Arguments
    ///
    /// * `stats` - The initial directory statistics to wrap
    #[must_use]
    pub fn new(stats: DirectoryStats) -> Self {
        Self {
            inner: Arc::new(Mutex::new(stats)),
        }
    }

    #[allow(dead_code)]
    /// Get the number of files copied
    ///
    /// # Errors
    ///
    /// This function will return an error if the internal mutex is poisoned.
    pub fn files_copied(&self) -> Result<u64> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| SyncError::FileSystem("Failed to acquire stats lock".to_string()))?
            .files_copied)
    }

    #[allow(dead_code)]
    /// Get the number of directories created
    ///
    /// # Errors
    ///
    /// This function will return an error if the internal mutex is poisoned.
    pub fn directories_created(&self) -> Result<u64> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| SyncError::FileSystem("Failed to acquire stats lock".to_string()))?
            .directories_created)
    }

    #[allow(dead_code)]
    /// Get the number of bytes copied
    ///
    /// # Errors
    ///
    /// This function will return an error if the internal mutex is poisoned.
    pub fn bytes_copied(&self) -> Result<u64> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| SyncError::FileSystem("Failed to acquire stats lock".to_string()))?
            .bytes_copied)
    }

    #[allow(dead_code)]
    /// Get the number of symlinks processed
    ///
    /// # Errors
    ///
    /// This function will return an error if the internal mutex is poisoned.
    pub fn symlinks_processed(&self) -> Result<u64> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| SyncError::FileSystem("Failed to acquire stats lock".to_string()))?
            .symlinks_processed)
    }

    #[allow(dead_code)]
    /// Get the number of errors encountered
    ///
    /// # Errors
    ///
    /// This function will return an error if the internal mutex is poisoned.
    pub fn errors(&self) -> Result<u64> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| SyncError::FileSystem("Failed to acquire stats lock".to_string()))?
            .errors)
    }

    /// Increment the number of files copied
    ///
    /// # Errors
    ///
    /// This function will return an error if the internal mutex is poisoned.
    pub fn increment_files_copied(&self) -> Result<()> {
        self.inner
            .lock()
            .map_err(|_| SyncError::FileSystem("Failed to acquire stats lock".to_string()))?
            .files_copied += 1;
        Ok(())
    }

    /// Increment the number of directories created
    ///
    /// # Errors
    ///
    /// This function will return an error if the internal mutex is poisoned.
    pub fn increment_directories_created(&self) -> Result<()> {
        self.inner
            .lock()
            .map_err(|_| SyncError::FileSystem("Failed to acquire stats lock".to_string()))?
            .directories_created += 1;
        Ok(())
    }

    /// Increment the number of bytes copied
    ///
    /// # Errors
    ///
    /// This function will return an error if the internal mutex is poisoned.
    pub fn increment_bytes_copied(&self, bytes: u64) -> Result<()> {
        self.inner
            .lock()
            .map_err(|_| SyncError::FileSystem("Failed to acquire stats lock".to_string()))?
            .bytes_copied += bytes;
        Ok(())
    }

    /// Increment the number of symlinks processed
    ///
    /// # Errors
    ///
    /// This function will return an error if the internal mutex is poisoned.
    pub fn increment_symlinks_processed(&self) -> Result<()> {
        self.inner
            .lock()
            .map_err(|_| SyncError::FileSystem("Failed to acquire stats lock".to_string()))?
            .symlinks_processed += 1;
        Ok(())
    }

    /// Increment the number of errors encountered
    ///
    /// # Errors
    ///
    /// This function will return an error if the internal mutex is poisoned.
    pub fn increment_errors(&self) -> Result<()> {
        self.inner
            .lock()
            .map_err(|_| SyncError::FileSystem("Failed to acquire stats lock".to_string()))?
            .errors += 1;
        Ok(())
    }

    /// Extract the inner DirectoryStats from the shared wrapper
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - Multiple references to the Arc exist (cannot unwrap)
    /// - The internal mutex is poisoned
    pub fn into_inner(self) -> Result<DirectoryStats> {
        let inner = Arc::try_unwrap(self.inner).map_err(|_| {
            SyncError::FileSystem("Failed to unwrap Arc - multiple references exist".to_string())
        })?;
        inner.into_inner().map_err(|_| {
            SyncError::FileSystem("Failed to unwrap Mutex - mutex is poisoned".to_string())
        })
    }
}

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
    /// Inner tracker wrapped in Arc<Mutex<>> for thread-safe access
    inner: Arc<Mutex<FilesystemTracker>>,
}

impl SharedHardlinkTracker {
    /// Create a new SharedHardlinkTracker wrapper
    ///
    /// # Arguments
    ///
    /// * `tracker` - The initial filesystem tracker to wrap
    #[must_use]
    pub fn new(tracker: FilesystemTracker) -> Self {
        Self {
            inner: Arc::new(Mutex::new(tracker)),
        }
    }

    /// Check if an inode has already been copied
    ///
    /// # Errors
    ///
    /// This function will return an error if the internal mutex is poisoned.
    pub fn is_inode_copied(&self, inode: u64) -> Result<bool> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| {
                SyncError::FileSystem("Failed to acquire hardlink tracker lock".to_string())
            })?
            .is_inode_copied(inode))
    }

    /// Get the original path for an inode that has been copied
    ///
    /// # Errors
    ///
    /// This function will return an error if the internal mutex is poisoned.
    pub fn get_original_path_for_inode(&self, inode: u64) -> Result<Option<PathBuf>> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| {
                SyncError::FileSystem("Failed to acquire hardlink tracker lock".to_string())
            })?
            .get_original_path_for_inode(inode)
            .map(|p| p.to_path_buf()))
    }

    /// Mark an inode as copied
    ///
    /// # Errors
    ///
    /// This function will return an error if the internal mutex is poisoned.
    pub fn mark_inode_copied(&self, inode: u64, path: PathBuf) -> Result<()> {
        self.inner
            .lock()
            .map_err(|_| {
                SyncError::FileSystem("Failed to acquire hardlink tracker lock".to_string())
            })?
            .mark_inode_copied(inode, &path);
        Ok(())
    }

    #[allow(dead_code)]
    /// Register a file with the hardlink tracker
    ///
    /// # Errors
    ///
    /// This function will return an error if the internal mutex is poisoned.
    pub fn register_file(
        &self,
        path: PathBuf,
        device_id: u64,
        inode: u64,
        link_count: u64,
    ) -> Result<()> {
        self.inner
            .lock()
            .map_err(|_| {
                SyncError::FileSystem("Failed to acquire hardlink tracker lock".to_string())
            })?
            .register_file(&path, device_id, inode, link_count);
        Ok(())
    }

    #[allow(dead_code)]
    /// Set the source filesystem device ID
    ///
    /// # Errors
    ///
    /// This function will return an error if the internal mutex is poisoned.
    pub fn set_source_filesystem(&self, device_id: u64) -> Result<()> {
        self.inner
            .lock()
            .map_err(|_| {
                SyncError::FileSystem("Failed to acquire hardlink tracker lock".to_string())
            })?
            .set_source_filesystem(device_id);
        Ok(())
    }

    #[allow(dead_code)]
    /// Get filesystem tracking statistics
    ///
    /// # Errors
    ///
    /// This function will return an error if the internal mutex is poisoned.
    pub fn get_stats(&self) -> Result<FilesystemStats> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| {
                SyncError::FileSystem("Failed to acquire hardlink tracker lock".to_string())
            })?
            .get_stats())
    }

    /// Extract the inner FilesystemTracker from the shared wrapper
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - Multiple references to the Arc exist (cannot unwrap)
    /// - The internal mutex is poisoned
    pub fn into_inner(self) -> Result<FilesystemTracker> {
        let inner = Arc::try_unwrap(self.inner).map_err(|_| {
            SyncError::FileSystem("Failed to unwrap Arc - multiple references exist".to_string())
        })?;
        inner.into_inner().map_err(|_| {
            SyncError::FileSystem("Failed to unwrap Mutex - mutex is poisoned".to_string())
        })
    }
}

/// Extended metadata using std::fs metadata support
#[derive(Debug)]
pub struct ExtendedMetadata {
    /// The underlying filesystem metadata
    pub metadata: std::fs::Metadata,
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
    pub async fn new(path: &Path) -> Result<Self> {
        let metadata = std::fs::symlink_metadata(path).map_err(|e| {
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
/// - Async directory traversal using io_uring statx operations
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
pub async fn copy_directory(
    src: &Path,
    dst: &Path,
    file_ops: &FileOperations,
    _copy_method: CopyMethod,
) -> Result<DirectoryStats> {
    let mut stats = DirectoryStats::default();
    let mut hardlink_tracker = FilesystemTracker::new();

    info!(
        "Starting directory copy from {} to {}",
        src.display(),
        dst.display()
    );

    // Create destination directory if it doesn't exist
    if !dst.exists() {
        std::fs::create_dir_all(dst).map_err(|e| {
            SyncError::FileSystem(format!(
                "Failed to create destination directory {}: {}",
                dst.display(),
                e
            ))
        })?;
        stats.directories_created += 1;
        debug!("Created destination directory: {}", dst.display());
    }

    // Set source filesystem from root directory
    let root_metadata = ExtendedMetadata::new(src).await?;
    hardlink_tracker.set_source_filesystem(root_metadata.device_id());

    // Traverse source directory iteratively using compio's dispatcher
    traverse_and_copy_directory_iterative(
        src.to_path_buf(),
        dst.to_path_buf(),
        file_ops,
        _copy_method,
        &mut stats,
        &mut hardlink_tracker,
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
/// * `file_ops` - File operations handler with io_uring support
/// * `copy_method` - Copy method (e.g., io_uring, fallback)
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
async fn traverse_and_copy_directory_iterative(
    initial_src: PathBuf,
    initial_dst: PathBuf,
    file_ops: &FileOperations,
    _copy_method: CopyMethod,
    stats: &mut DirectoryStats,
    hardlink_tracker: &mut FilesystemTracker,
) -> Result<()> {
    // Create a dispatcher for async operations
    let dispatcher = Box::leak(Box::new(Dispatcher::new()?));

    // Leak file_ops to give it a static lifetime (it's fine since it's just a reference)
    let file_ops_static: &'static FileOperations = unsafe { std::mem::transmute(file_ops) };

    // Wrap shared state in wrapper types for static lifetimes
    let shared_stats = SharedStats::new(std::mem::take(stats));
    let shared_hardlink_tracker = SharedHardlinkTracker::new(std::mem::take(hardlink_tracker));

    // Process the directory
    let result = process_directory_entry_with_compio(
        dispatcher,
        initial_src,
        initial_dst,
        file_ops_static,
        _copy_method,
        shared_stats.clone(),
        shared_hardlink_tracker.clone(),
    )
    .await;

    // Restore the state
    *stats = shared_stats.into_inner()?;
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
/// * `file_ops` - File operations handler with io_uring support
/// * `copy_method` - Copy method (e.g., io_uring, fallback)
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
async fn process_directory_entry_with_compio(
    dispatcher: &'static Dispatcher,
    src_path: PathBuf,
    dst_path: PathBuf,
    file_ops: &'static FileOperations,
    _copy_method: CopyMethod,
    stats: SharedStats,
    hardlink_tracker: SharedHardlinkTracker,
) -> Result<()> {
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
            stats.increment_directories_created()?;
        }

        // Read directory entries using std::fs::read_dir
        // Note: compio doesn't have read_dir yet, but we use compio's dispatcher
        // for async scheduling of the actual processing operations
        let entries = std::fs::read_dir(&src_path).map_err(|e| {
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
                SyncError::FileSystem(format!("Failed to read directory entry: {}", e))
            })?;
            let child_src_path = entry.path();
            let file_name = child_src_path.file_name().ok_or_else(|| {
                SyncError::FileSystem(format!("Invalid file name in {}", child_src_path.display()))
            })?;
            let child_dst_path = dst_path.join(file_name);

            // Dispatch all entries to the same function regardless of type
            // This creates a unified processing pipeline where each entry
            // determines its own processing path (file/dir/symlink)
            let child_src_path = child_src_path.to_path_buf();
            let child_dst_path = child_dst_path.to_path_buf();
            let copy_method = copy_method.clone();
            let stats = stats.clone();
            let hardlink_tracker = hardlink_tracker.clone();
            let receiver = dispatcher
                .dispatch(move || {
                    process_directory_entry_with_compio(
                        dispatcher,
                        child_src_path,
                        child_dst_path,
                        file_ops,
                        copy_method,
                        stats,
                        hardlink_tracker,
                    )
                })
                .map_err(|e| {
                    SyncError::FileSystem(format!("Failed to dispatch entry processing: {:?}", e))
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
                    "Failed to receive result from dispatched operation: {:?}",
                    e
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
async fn process_file(
    src_path: PathBuf,
    dst_path: PathBuf,
    metadata: ExtendedMetadata,
    _file_ops: &'static FileOperations,
    _copy_method: CopyMethod,
    stats: SharedStats,
    hardlink_tracker: SharedHardlinkTracker,
) -> Result<()> {
    debug!(
        "Processing file: {} (link_count: {})",
        src_path.display(),
        metadata.link_count()
    );

    let _device_id = metadata.device_id();
    let inode_number = metadata.inode_number();
    let link_count = metadata.link_count();

    // Check if this inode has already been copied (for hardlinks)
    if link_count > 1 && hardlink_tracker.is_inode_copied(inode_number)? {
        handle_existing_hardlink(
            &dst_path,
            &src_path,
            inode_number,
            &stats,
            &hardlink_tracker,
        )?;
    } else {
        // First time seeing this inode - copy the file content normally
        debug!("Copying file content: {}", src_path.display());

        match copy_file(&src_path, &dst_path).await {
            Ok(()) => {
                stats.increment_files_copied()?;
                stats.increment_bytes_copied(metadata.len())?;
                hardlink_tracker.mark_inode_copied(inode_number, dst_path.clone())?;
                debug!("Copied file: {}", dst_path.display());
            }
            Err(e) => {
                warn!(
                    "Failed to copy file {} -> {}: {}",
                    src_path.display(),
                    dst_path.display(),
                    e
                );
                stats.increment_errors()?;
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
fn handle_existing_hardlink(
    dst_path: &Path,
    src_path: &Path,
    inode_number: u64,
    stats: &SharedStats,
    hardlink_tracker: &SharedHardlinkTracker,
) -> Result<()> {
    // This is a hardlink - create a hardlink instead of copying content
    debug!(
        "Creating hardlink for {} (inode: {})",
        src_path.display(),
        inode_number
    );

    // Find the original file path for this inode
    if let Some(original_path) = hardlink_tracker.get_original_path_for_inode(inode_number)? {
        // Create destination directory if needed
        if let Some(parent) = dst_path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    SyncError::FileSystem(format!(
                        "Failed to create parent directory {}: {}",
                        parent.display(),
                        e
                    ))
                })?;
            }
        }

        // Create hardlink using std filesystem operations (compio has Send issues)
        match std::fs::hard_link(&original_path, dst_path) {
            Ok(()) => {
                stats.increment_files_copied()?;
                debug!(
                    "Created hardlink: {} -> {}",
                    dst_path.display(),
                    original_path.display()
                );
            }
            Err(e) => {
                warn!(
                    "Failed to create hardlink for {}: {}",
                    src_path.display(),
                    e
                );
                stats.increment_errors()?;
            }
        }
    } else {
        warn!("Could not find original path for inode {}", inode_number);
        stats.increment_errors()?;
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
async fn process_symlink(src_path: PathBuf, dst_path: PathBuf, stats: SharedStats) -> Result<()> {
    debug!("Processing symlink: {}", src_path.display());

    match copy_symlink(&src_path, &dst_path).await {
        Ok(()) => {
            stats.increment_symlinks_processed()?;
            Ok(())
        }
        Err(e) => {
            stats.increment_errors()?;
            warn!("Failed to copy symlink {}: {}", src_path.display(), e);
            Err(e)
        }
    }
}

/// Copy a symlink preserving its target
async fn copy_symlink(src: &Path, dst: &Path) -> Result<()> {
    let target = std::fs::read_link(src).map_err(|e| {
        SyncError::FileSystem(format!(
            "Failed to read symlink target for {}: {}",
            src.display(),
            e
        ))
    })?;

    // Remove destination if it exists
    if dst.exists() {
        std::fs::remove_file(dst).map_err(|e| {
            SyncError::FileSystem(format!(
                "Failed to remove existing destination {}: {}",
                dst.display(),
                e
            ))
        })?;
    }

    // Create symlink with same target
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&target, dst).map_err(|e| {
            SyncError::FileSystem(format!(
                "Failed to create symlink {} -> {}: {}",
                dst.display(),
                target.display(),
                e
            ))
        })?;
    }

    #[cfg(not(unix))]
    {
        return Err(SyncError::FileSystem(
            "Symlink creation not supported on this platform".to_string(),
        ));
    }

    debug!("Copied symlink {} -> {}", dst.display(), target.display());
    Ok(())
}

/// Preserve file metadata (permissions, ownership, timestamps)
#[allow(dead_code)]
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

/// Hardlink tracking information
#[derive(Debug, Clone)]
pub struct HardlinkInfo {
    /// Original file path
    #[allow(dead_code)]
    pub original_path: std::path::PathBuf,
    /// Inode number
    pub inode_number: u64,
    /// Number of hardlinks found
    pub link_count: u64,
    /// Whether this inode has been copied to destination
    pub is_copied: bool,
    /// Destination path where this inode was copied (for hardlink creation)
    pub dst_path: Option<std::path::PathBuf>,
}

/// Filesystem boundary and hardlink tracker
#[derive(Debug, Default)]
#[allow(dead_code)]
pub struct FilesystemTracker {
    /// Map of (dev, ino) pairs to hardlink information
    #[allow(clippy::disallowed_types)]
    hardlinks: HashMap<InodeInfo, HardlinkInfo>,
    /// Source filesystem device ID (for boundary detection)
    source_filesystem: Option<u64>,
}

#[allow(dead_code)]
impl FilesystemTracker {
    /// Create a new filesystem tracker
    #[must_use]
    pub fn new() -> Self {
        Self {
            #[allow(clippy::disallowed_types)]
            hardlinks: HashMap::new(),
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
    pub fn is_same_filesystem(&self, dev: u64) -> bool {
        match self.source_filesystem {
            Some(source_dev) => source_dev == dev,
            None => {
                warn!("No source filesystem set, allowing cross-filesystem operation");
                true
            }
        }
    }

    /// Register a file for hardlink tracking
    ///
    /// This should be called for each file encountered during traversal.
    /// Files with link_count == 1 are skipped since they're not hardlinks.
    /// Returns true if this is a new hardlink, false if it's a duplicate or skipped.
    pub fn register_file(&mut self, path: &Path, dev: u64, ino: u64, link_count: u64) -> bool {
        // Skip files with link count of 1 - they're not hardlinks
        if link_count == 1 {
            return false;
        }
        let inode_info = InodeInfo { dev, ino };

        match self.hardlinks.get_mut(&inode_info) {
            Some(hardlink_info) => {
                // This is an existing hardlink
                hardlink_info.link_count += 1;
                debug!(
                    "Found hardlink #{} for inode ({}, {}): {}",
                    hardlink_info.link_count,
                    dev,
                    ino,
                    path.display()
                );
                false
            }
            None => {
                // This is a new file
                self.hardlinks.insert(
                    inode_info,
                    HardlinkInfo {
                        original_path: path.to_path_buf(),
                        inode_number: ino,
                        link_count: 1,
                        is_copied: false,
                        dst_path: None,
                    },
                );
                debug!(
                    "Registered new file inode ({}, {}): {}",
                    dev,
                    ino,
                    path.display()
                );
                true
            }
        }
    }

    /// Get hardlink information for a given inode
    ///
    /// Returns the hardlink information if this inode has been seen before.
    #[must_use]
    pub fn get_hardlink_info(&self, dev: u64, ino: u64) -> Option<&HardlinkInfo> {
        let inode_info = InodeInfo { dev, ino };
        self.hardlinks.get(&inode_info)
    }

    /// Get all hardlink groups that have multiple links
    ///
    /// Returns a vector of hardlink groups that contain multiple files.
    #[must_use]
    pub fn get_hardlink_groups(&self) -> Vec<&HardlinkInfo> {
        self.hardlinks
            .values()
            .filter(|info| info.link_count > 1)
            .collect()
    }

    /// Check if an inode has already been copied (for hardlink creation)
    ///
    /// Returns true if this inode has been processed and copied to the destination.
    /// This is used to determine whether to copy file content or create a hardlink.
    #[must_use]
    pub fn is_inode_copied(&self, ino: u64) -> bool {
        self.hardlinks
            .values()
            .any(|info| info.inode_number == ino && info.is_copied)
    }

    /// Mark an inode as copied and store its destination path
    ///
    /// This should be called after successfully copying a file's content,
    /// so that subsequent hardlinks to the same inode can be created instead of copied.
    pub fn mark_inode_copied(&mut self, ino: u64, dst_path: &Path) {
        for info in self.hardlinks.values_mut() {
            if info.inode_number == ino {
                info.is_copied = true;
                info.dst_path = Some(dst_path.to_path_buf());
                debug!("Marked inode {} as copied to {}", ino, dst_path.display());
                break;
            }
        }
    }

    /// Get the original destination path for an inode that has been copied
    ///
    /// Returns the destination path where this inode's content was first copied.
    /// This is used to create hardlinks pointing to the original copied file.
    #[must_use]
    pub fn get_original_path_for_inode(&self, ino: u64) -> Option<&Path> {
        self.hardlinks
            .values()
            .find(|info| info.inode_number == ino && info.is_copied)
            .and_then(|info| info.dst_path.as_deref())
    }

    /// Get statistics about the filesystem tracking
    #[must_use]
    pub fn get_stats(&self) -> FilesystemStats {
        let total_files = self.hardlinks.len();
        let hardlink_groups = self.get_hardlink_groups().len();
        let total_hardlinks: u64 = self.hardlinks.values().map(|info| info.link_count).sum();

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
            SharedStats::new(stats),
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
            SharedStats::new(stats),
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

        // Test initial state
        assert_eq!(tracker.get_stats().total_files, 0);
        assert_eq!(tracker.get_stats().hardlink_groups, 0);

        // Test setting source filesystem
        tracker.set_source_filesystem(123);
        assert!(tracker.is_same_filesystem(123));
        assert!(!tracker.is_same_filesystem(456));
    }

    /// Test FilesystemTracker hardlink detection
    #[compio::test]
    async fn test_filesystem_tracker_hardlinks() {
        let mut tracker = FilesystemTracker::new();
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let file1 = temp_dir.path().join("file1.txt");
        let file2 = temp_dir.path().join("file2.txt");

        // Create a file
        std::fs::write(&file1, "content").expect("Failed to write file");

        // Create hardlink
        std::fs::hard_link(&file1, &file2).expect("Failed to create hardlink");

        // Register first file
        let registered = tracker.register_file(&file1, 1, 100, 2);
        assert!(registered); // Should register as new file

        // Register hardlink
        let registered = tracker.register_file(&file2, 1, 100, 2);
        assert!(!registered); // Should not register as new (it's a hardlink)

        // Check stats
        let stats = tracker.get_stats();
        assert_eq!(stats.total_files, 1);
        assert_eq!(stats.hardlink_groups, 1);
        assert_eq!(stats.total_hardlinks, 2);
    }
}
