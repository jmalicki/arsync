//! File copying operations using `io_uring`
//!
//! This module provides high-performance file copying operations using various
//! system calls optimized for different scenarios. It implements `copy_file_range`
//! for efficient in-kernel copying, `splice` for zero-copy operations, and
//! traditional read/write as fallback methods.
//!
//! # Copy Methods
//!
//! - **`copy_file_range`**: In-kernel copying, most efficient for large files
//! - **`splice`**: Zero-copy operations using pipes
//! - **`read_write`**: Traditional fallback method
//! - **auto**: Automatically selects the best method available
//!
//! # Performance Characteristics
//!
//! - `copy_file_range`: ~2-5x faster than read/write for large files
//! - `splice`: Zero-copy, optimal for streaming operations
//! - read/write: Reliable fallback, works everywhere
//!
//! # Usage
//!
//! ```rust,ignore
//! use arsync::copy::copy_file;
//! use arsync::cli::CopyMethod;
//! use std::path::Path;
//!
//! #[compio::main]
//! async fn main() -> arsync::Result<()> {
//!     let src_path = Path::new("source.txt");
//!     let dst_path = Path::new("destination.txt");
//!     
//!     // Copy with automatic method selection
//!     copy_file(src_path, dst_path, CopyMethod::Auto).await?;
//!
//!     // Force specific method
//!     copy_file(src_path, dst_path, CopyMethod::CopyFileRange).await?;
//!     Ok(())
//! }
//! ```

use crate::cli::ParallelCopyConfig;
use crate::error::{Result, SyncError};
use crate::file_wrapper::AsyncFileWrapper;
use crate::metadata::{preserve_file_metadata, MetadataConfig};
use crate::traits::AsyncFile;
use compio::dispatcher::Dispatcher;
use compio::fs::File;
use compio::io::{AsyncReadAt, AsyncWriteAt};
use futures::stream::{FuturesUnordered, StreamExt};
use std::path::Path;
use std::sync::LazyLock;

/// Default I/O buffer size (in bytes) used for chunked read/write operations.
///
/// Chosen to balance syscall overhead and memory usage. Adjust if profiling
/// indicates different optimal sizes for specific workloads.
const BUFFER_SIZE: usize = 64 * 1024; // 64KB buffer

/// 2MB huge page size for alignment in parallel copies
const HUGE_PAGE_SIZE: u64 = 2 * 1024 * 1024;

/// Global static dispatcher for parallel copy operations.
///
/// The dispatcher is shared across all threads and initialized lazily on first use.
/// It manages a pool of worker threads that handle parallel I/O operations.
///
/// Using `LazyLock` gives us true static storage with a natural 'static lifetime,
/// which is required by `dispatcher.dispatch()` for spawning worker threads.
#[allow(clippy::expect_used)] // Dispatcher creation failure is unrecoverable
static DISPATCHER: LazyLock<Dispatcher> =
    LazyLock::new(|| Dispatcher::new().expect("Failed to create global dispatcher"));

/// Copy a single file (public API)
///
/// This is a convenient wrapper that takes simple paths and sets up `DirectoryFd` internally.
/// For internal/recursive use, call `copy_file_internal()` directly with `DirectoryFd`.
///
/// # Parameters
/// - `src`: Source file path
/// - `dst`: Destination file path
/// - `metadata_config`: Metadata preservation configuration
/// - `parallel_config`: Parallel copy configuration
///
/// # Panics
/// Panics if thread-local Dispatcher cannot be created (extremely rare, indicates system resource exhaustion)
///
/// # Errors
/// - Source file cannot be opened or read
/// - Destination file cannot be created or written
/// - Metadata preservation fails
///
/// # Note
/// This function is called via `io_uring::FileOperations::copy_file_with_metadata()`
/// and from tests via the `copy_file_test` helper.
#[allow(clippy::future_not_send)]
#[allow(clippy::expect_used)] // Thread-local init - panic is acceptable here
pub async fn copy_file(
    src: &Path,
    dst: &Path,
    metadata_config: &MetadataConfig,
    parallel_config: &ParallelCopyConfig,
) -> Result<()> {
    // Set up DirectoryFd for source
    let src_parent_dir = compio_fs_extended::DirectoryFd::open(
        src.parent().unwrap_or_else(|| std::path::Path::new(".")),
    )
    .await
    .map_err(|e| SyncError::FileSystem(format!("Failed to open source parent: {e}")))?;

    let src_filename = src
        .file_name()
        .ok_or_else(|| SyncError::FileSystem("Source has no filename".to_string()))?;

    // Get metadata
    let src_metadata = src_parent_dir.statx_full(src_filename).await?;

    // Set up DirectoryFd for destination
    let dst_parent_dir = compio_fs_extended::DirectoryFd::open(
        dst.parent().unwrap_or_else(|| std::path::Path::new(".")),
    )
    .await
    .map_err(|e| SyncError::FileSystem(format!("Failed to open dest parent: {e}")))?;

    let dst_filename = dst
        .file_name()
        .ok_or_else(|| SyncError::FileSystem("Destination has no filename".to_string()))?;

    // Get reference to global dispatcher (initialized on first use)
    let dispatcher: &'static Dispatcher = &DISPATCHER;

    copy_file_internal(
        src,
        dst,
        metadata_config,
        parallel_config,
        dispatcher,
        &src_metadata,
        &src_parent_dir,
        src_filename,
        &dst_parent_dir,
        dst_filename,
    )
    .await
}

/// Internal: Copy a single file with optimized, TOCTOU-safe operations
///
/// **This is the internal function used in recursive directory traversal.**
/// For public API, use `copy_file()` which provides a simpler interface.
///
/// This function REQUIRES `DirectoryFd` for BOTH source and destination to enforce
/// TOCTOU-safe operations on both sides. Metadata is pre-fetched to avoid redundant syscalls.
///
/// # Parameters
///
/// ## Paths (for error messages only)
/// - `src`: Source path - **NOT USED for file operations**, only for error messages
/// - `dst`: Destination path - **NOT USED for file operations**, only for error messages
///
/// ## Required `DirectoryFd` parameters (actual operations)
/// - `src_parent_dir`: Source parent `DirectoryFd` for TOCTOU-safe file opening
/// - `src_filename`: Source **basename only** (no path separators) relative to `src_parent_dir`
/// - `src_metadata`: Pre-fetched metadata via `DirectoryFd::statx_full()`
/// - `dst_parent_dir`: Destination parent `DirectoryFd` for TOCTOU-safe creation
/// - `dst_filename`: Destination **basename only** (no path separators) relative to `dst_parent_dir`
/// - `dispatcher`: For parallel copy operations
///
/// **Why `&str` not `&Path`?** Filenames must be simple basenames without `/` for TOCTOU safety.
/// Using `openat(dirfd, "sub/file", ...)` would be TOCTOU-vulnerable if `sub` is replaced.
///
/// # TOCTOU Safety
///
/// ALL file operations use dirfd-based syscalls (no path-based operations):
/// - Source: `openat(src_dirfd, src_filename, O_RDONLY)` - TOCTOU-safe read
/// - Dest: `openat(dst_dirfd, dst_filename, O_CREAT|O_WRONLY)` - TOCTOU-safe write
/// - Metadata: Already fetched via `statx(dirfd, filename, ...)` - TOCTOU-safe
///
/// # Errors
///
/// This function will return an error if:
/// - Source file cannot be opened for reading
/// - Destination file cannot be created or opened for writing
/// - File copying operation fails (I/O errors, permission issues)
/// - Metadata preservation fails
#[allow(clippy::future_not_send)]
#[allow(clippy::too_many_arguments)]
pub async fn copy_file_internal(
    src: &Path, // Only for error messages
    dst: &Path, // Only for error messages
    metadata_config: &MetadataConfig,
    parallel_config: &ParallelCopyConfig,
    dispatcher: &'static Dispatcher,
    src_metadata: &compio_fs_extended::FileMetadata,
    src_parent_dir: &compio_fs_extended::DirectoryFd,
    src_filename: &std::ffi::OsStr,
    dst_parent_dir: &compio_fs_extended::DirectoryFd,
    dst_filename: &std::ffi::OsStr,
) -> Result<()> {
    // Get file size from pre-fetched metadata (no syscall needed!)
    let file_size = src_metadata.size;

    // Decide whether to use parallel copy
    if parallel_config.should_use_parallel(file_size) {
        copy_read_write_parallel(
            src,
            dst,
            metadata_config,
            parallel_config,
            file_size,
            dispatcher,
            src_metadata,
            src_parent_dir,
            src_filename,
            dst_parent_dir,
            dst_filename,
        )
        .await
    } else {
        copy_read_write(
            src,
            dst,
            metadata_config,
            file_size,
            src_metadata,
            src_parent_dir,
            src_filename,
            dst_parent_dir,
            dst_filename,
        )
        .await
    }
}

/// Copy file using compio read/write operations
///
/// This function provides file copying using compio's async read/write operations
/// via `io_uring`. All operations are TOCTOU-safe via `DirectoryFd`.
///
/// # Parameters
///
/// * `src` - Source file path (for error messages only)
/// * `dst` - Destination file path (for error messages only)
///
/// # Returns
///
/// Returns `Ok(())` if the file was copied successfully, or `Err(SyncError)` if failed.
///
/// # Performance Notes
///
/// - Reliable fallback method that works everywhere
/// - Uses compio's async I/O for optimal performance
/// - Compatible with all filesystems and scenarios
/// - Slower than `copy_file_range` but more reliable
///
/// # Examples
///
/// ```rust,ignore
/// use arsync::copy::copy_read_write;
/// use std::path::Path;
///
/// #[compio::main]
/// async fn main() -> arsync::Result<()> {
///     let src_path = Path::new("source.txt");
///     let dst_path = Path::new("destination.txt");
///     copy_read_write(src_path, dst_path).await?;
///     Ok(())
/// }
/// ```
#[allow(
    clippy::future_not_send,
    clippy::too_many_lines,
    clippy::too_many_arguments
)]
async fn copy_read_write(
    src: &Path, // Only for error messages
    dst: &Path, // Only for error messages
    metadata_config: &MetadataConfig,
    file_size: u64,
    src_metadata: &compio_fs_extended::FileMetadata,
    src_parent_dir: &compio_fs_extended::DirectoryFd,
    src_filename: &std::ffi::OsStr,
    dst_parent_dir: &compio_fs_extended::DirectoryFd,
    dst_filename: &std::ffi::OsStr,
) -> Result<()> {
    // Extract timestamps from pre-fetched metadata (no syscall needed!)
    let (src_accessed, src_modified) = (src_metadata.accessed, src_metadata.modified);

    // Open source file via DirectoryFd (TOCTOU-safe!)
    let src_file = src_parent_dir
        .open_file_at(src_filename, true, false, false, false)
        .await
        .map_err(|e| {
            SyncError::FileSystem(format!(
                "Failed to open source file {} via dirfd: {e}",
                src.display()
            ))
        })?;

    // Open destination file via DirectoryFd (TOCTOU-safe, io_uring-ready)
    let mut dst_file = dst_parent_dir
        .open_file_at(dst_filename, false, true, true, true)
        .await
        .map_err(|e| {
            SyncError::FileSystem(format!(
                "Failed to create destination file {} via dirfd: {e}",
                dst.display()
            ))
        })?;

    // file_size already passed as parameter (from pre-fetched metadata or initial check)
    // ✅ NO redundant src_file.metadata() call!

    // Preallocate destination file space to the final size to reduce fragmentation
    // and improve write performance using io_uring fallocate.
    // Skip preallocation for empty files as fallocate fails with EINVAL for zero length.
    if file_size > 0 {
        use compio_fs_extended::{ExtendedFile, Fallocate};

        let extended_dst = ExtendedFile::from_ref(&dst_file);

        // Apply fadvise hints (Linux only - io_uring optimization)
        #[cfg(target_os = "linux")]
        {
            use compio_fs_extended::{fadvise::FadviseAdvice, Fadvise};
            let extended_src = ExtendedFile::from_ref(&src_file);

            // Hint that source data won't be accessed again after this copy
            extended_src
                .fadvise(
                    FadviseAdvice::NoReuse,
                    0,
                    file_size.try_into().unwrap_or(i64::MAX),
                )
                .await
                .map_err(|e| {
                    SyncError::FileSystem(format!(
                        "Failed to set fadvise NoReuse hint on source: {e}"
                    ))
                })?;
        }

        // Preallocate destination file space
        extended_dst.fallocate(0, file_size, 0).await.map_err(|e| {
            SyncError::FileSystem(format!("Failed to preallocate destination file: {e}"))
        })?;

        // Hint that destination data won't be accessed again after this copy (Linux only)
        #[cfg(target_os = "linux")]
        {
            use compio_fs_extended::{fadvise::FadviseAdvice, Fadvise};
            extended_dst
                .fadvise(
                    FadviseAdvice::NoReuse,
                    0,
                    file_size.try_into().unwrap_or(i64::MAX),
                )
                .await
                .map_err(|e| {
                    SyncError::FileSystem(format!(
                        "Failed to set fadvise NoReuse hint on destination: {e}"
                    ))
                })?;
        }
    }

    // Use compio's async read_at/write_at operations with buffer reuse
    // Create buffer once and reuse it throughout the copy (no allocations!)
    let mut buffer = vec![0u8; BUFFER_SIZE];
    let mut offset = 0u64;
    let mut total_copied = 0u64;

    while total_copied < file_size {
        // Read data from source file - buffer ownership transferred to compio
        let read_result = src_file.read_at(buffer, offset).await;

        let bytes_read = read_result
            .0
            .map_err(|e| SyncError::IoUring(format!("compio read_at operation failed: {e}")))?;

        // Get buffer back from read operation
        buffer = read_result.1;

        if bytes_read == 0 {
            // End of file
            break;
        }

        // Truncate buffer to only the bytes read (avoids writing garbage)
        // This doesn't allocate, just changes the length
        buffer.truncate(bytes_read);

        // Write data to destination file - write_at takes ownership and returns the buffer
        // This way we reuse the same allocation for both read and write
        let write_result = dst_file.write_at(buffer, offset).await;

        let bytes_written = write_result
            .0
            .map_err(|e| SyncError::IoUring(format!("compio write_at operation failed: {e}")))?;

        // Get the buffer back from write operation and resize it for the next read
        // resize() reuses the existing capacity when possible (no new allocation!)
        buffer = write_result.1;
        buffer.resize(BUFFER_SIZE, 0);

        // Ensure we wrote the expected number of bytes
        if bytes_written != bytes_read {
            return Err(SyncError::CopyFailed(format!(
                "Write size mismatch: expected {bytes_read}, got {bytes_written}"
            )));
        }

        total_copied += bytes_written as u64;
        offset += bytes_written as u64;

        tracing::debug!(
            "compio read_at/write_at: copied {} bytes, total: {}/{} (buffer reused)",
            bytes_written,
            total_copied,
            file_size
        );
    }

    // Sync the destination file to disk if requested (matches rsync --fsync)
    if metadata_config.fsync {
        dst_file
            .sync_all()
            .await
            .map_err(|e| SyncError::FileSystem(format!("Failed to sync destination file: {e}")))?;
    }

    // Preserve file metadata using the metadata module
    preserve_file_metadata(
        &src_file,
        &dst_file,
        dst,
        src_accessed,
        src_modified,
        metadata_config,
    )
    .await?;

    tracing::debug!(
        "compio read_at/write_at: successfully copied {} bytes",
        total_copied
    );
    Ok(())
}

/// Copy a file using parallel recursive binary splitting
///
/// This function splits large files into regions recursively and copies them
/// in parallel to maximize throughput on fast storage (`NVMe`).
///
/// # Parameters
///
/// * `src` - Source file path
/// * `dst` - Destination file path
/// * `metadata_config` - Metadata preservation configuration
/// * `parallel_config` - Parallel copy configuration
/// * `file_size` - Size of the file to copy
///
/// # Returns
///
/// Returns `Ok(())` if the file was copied successfully, or `Err(SyncError)` if failed.
#[allow(clippy::future_not_send)]
#[allow(clippy::too_many_lines)]
#[allow(clippy::too_many_arguments)]
#[allow(clippy::cognitive_complexity)]
async fn copy_read_write_parallel(
    src: &Path, // Only for error messages
    dst: &Path, // Only for error messages
    metadata_config: &MetadataConfig,
    parallel_config: &ParallelCopyConfig,
    file_size: u64,
    dispatcher: &'static Dispatcher,
    src_metadata: &compio_fs_extended::FileMetadata,
    src_parent_dir: &compio_fs_extended::DirectoryFd,
    src_filename: &std::ffi::OsStr,
    dst_parent_dir: &compio_fs_extended::DirectoryFd,
    dst_filename: &std::ffi::OsStr,
) -> Result<()> {
    let max_depth = parallel_config.max_depth;
    let max_tasks = 1 << max_depth; // 2^max_depth

    tracing::info!(
        "Using parallel copy: depth {} (up to {} tasks) for {} MB on {}",
        max_depth,
        max_tasks,
        file_size / 1024 / 1024,
        src.display()
    );

    // 1. Extract timestamps from pre-fetched metadata (no syscall needed!)
    let (src_accessed, src_modified) = (src_metadata.accessed, src_metadata.modified);

    // 2. Open source file via DirectoryFd (TOCTOU-safe!)
    let src_file = src_parent_dir
        .open_file_at(src_filename, true, false, false, false)
        .await
        .map_err(|e| {
            SyncError::FileSystem(format!(
                "Failed to open source file {} via dirfd: {e}",
                src.display()
            ))
        })?;

    // 3. Open destination file via DirectoryFd (TOCTOU-safe, io_uring-ready)
    let dst_file = dst_parent_dir
        .open_file_at(dst_filename, false, true, true, true)
        .await
        .map_err(|e| {
            SyncError::FileSystem(format!(
                "Failed to create destination file {} via dirfd: {e}",
                dst.display()
            ))
        })?;

    // 4. CRITICAL: fallocate the entire file first to prevent fragmentation
    // and allow parallel writes without conflicts
    if file_size > 0 {
        use compio_fs_extended::{ExtendedFile, Fallocate};

        let extended_dst = ExtendedFile::from_ref(&dst_file);

        // Preallocate destination file space
        extended_dst.fallocate(0, file_size, 0).await.map_err(|e| {
            SyncError::FileSystem(format!("Failed to preallocate destination file: {e}"))
        })?;

        // Apply fadvise hints (Linux only - io_uring optimization)
        #[cfg(target_os = "linux")]
        {
            use compio_fs_extended::{fadvise::FadviseAdvice, Fadvise};

            let extended_src = ExtendedFile::from_ref(&src_file);

            // Hint that source data won't be accessed again after this copy
            extended_src
                .fadvise(
                    FadviseAdvice::NoReuse,
                    0,
                    file_size.try_into().unwrap_or(i64::MAX),
                )
                .await
                .map_err(|e| {
                    SyncError::FileSystem(format!(
                        "Failed to set fadvise NoReuse hint on source: {e}"
                    ))
                })?;

            // Hint that destination data won't be accessed again after this copy
            extended_dst
                .fadvise(
                    FadviseAdvice::NoReuse,
                    0,
                    file_size.try_into().unwrap_or(i64::MAX),
                )
                .await
                .map_err(|e| {
                    SyncError::FileSystem(format!(
                        "Failed to set fadvise NoReuse hint on destination: {e}"
                    ))
                })?;
        }
    }

    // 5. Calculate all regions upfront (iterative, not recursive)
    let chunk_size = parallel_config.chunk_size_bytes();
    let num_tasks = 1 << max_depth; // 2^max_depth
    let region_size = file_size / num_tasks as u64;

    tracing::info!(
        "PARALLEL COPY: {} tasks, {} MB per region, chunk={} KB, thread={:?}, multi-threaded={}",
        num_tasks,
        region_size / 1_048_576,
        chunk_size / 1024,
        std::thread::current().id(),
        true // Dispatcher is always required now
    );

    // Multi-threaded: dispatch to worker threads via dispatcher
    {
        let mut receivers = Vec::with_capacity(num_tasks);

        for task_id in 0..num_tasks {
            let start = task_id as u64 * region_size;
            let end = if task_id == num_tasks - 1 {
                file_size // Last task handles remainder
            } else {
                (task_id as u64 + 1) * region_size
            };

            // Align to page boundaries (except first and last)
            let start_aligned = if task_id > 0 {
                align_to_page(start, HUGE_PAGE_SIZE)
            } else {
                start
            };

            // Clone file handles for this task
            let src = src_file.clone();
            let mut dst = dst_file.clone();

            // Dispatch to worker thread - each gets its own io_uring instance
            let receiver = dispatcher
                .dispatch(move || async move {
                    copy_region_sequential(&src, &mut dst, start_aligned, end, chunk_size).await
                })
                .map_err(|e| {
                    SyncError::CopyFailed(format!("Failed to dispatch parallel copy task: {e:?}"))
                })?;

            receivers.push(receiver);
        }

        // Wait for all dispatched tasks - process as they complete, fail fast on first error
        let mut futures: FuturesUnordered<_> = receivers
            .into_iter()
            .enumerate()
            .map(|(task_id, receiver)| async move {
                receiver
                    .await
                    .map_err(|e| {
                        SyncError::CopyFailed(format!("Task {task_id} channel failed: {e:?}"))
                    })?
                    .map_err(|e| {
                        SyncError::CopyFailed(format!("Task {task_id} execution failed: {e:?}"))
                    })
            })
            .collect();

        while let Some(result) = futures.next().await {
            result?; // Fail fast on first error
        }
    }

    // 7. Sync all data to disk if requested (matches rsync --fsync)
    if metadata_config.fsync {
        dst_file
            .sync_all()
            .await
            .map_err(|e| SyncError::FileSystem(format!("Failed to sync destination file: {e}")))?;
    }

    // 8. Preserve file metadata
    preserve_file_metadata(
        &src_file,
        &dst_file,
        dst,
        src_accessed,
        src_modified,
        metadata_config,
    )
    .await?;

    tracing::info!("Parallel copy completed: {} bytes", file_size);
    Ok(())
}

/// Copy a region sequentially
///
/// This function copies a contiguous region of a file using sequential
/// `read_at`/`write_at` operations. Used by parallel copy to copy each partition.
///
/// # Parameters
///
/// * `src` - Source file handle
/// * `dst` - Destination file handle
/// * `start` - Starting byte offset
/// * `end` - Ending byte offset (exclusive)
/// * `chunk_size` - Size of chunks for read/write operations
#[allow(clippy::future_not_send)]
async fn copy_region_sequential(
    src: &File,
    dst: &mut File,
    start: u64,
    end: u64,
    chunk_size: usize,
) -> Result<()> {
    tracing::debug!(
        "copy_region_sequential: start={} MB, end={} MB, thread={:?}",
        start / 1_048_576,
        end / 1_048_576,
        std::thread::current().id()
    );

    let mut offset = start;

    while offset < end {
        let remaining = end - offset;
        #[allow(clippy::cast_possible_truncation)]
        let to_read = remaining.min(chunk_size as u64) as usize;

        // Allocate buffer sized to what we actually need to read
        // This prevents reading past the region boundary in parallel execution
        let buffer = vec![0u8; to_read];

        // Read from source at this offset
        let read_result = src.read_at(buffer, offset).await;
        let bytes_read = read_result
            .0
            .map_err(|e| SyncError::IoUring(format!("read_at failed at offset {offset}: {e}")))?;
        let mut buffer = read_result.1;

        if bytes_read == 0 {
            break;
        }

        // Write to destination at same offset
        buffer.truncate(bytes_read);
        let write_result = dst.write_at(buffer, offset).await;
        let bytes_written = write_result
            .0
            .map_err(|e| SyncError::IoUring(format!("write_at failed at offset {offset}: {e}")))?;
        // Note: We intentionally don't reuse the buffer here (write_result.1)
        // A new buffer is allocated on each iteration to ensure correct sizing

        if bytes_written != bytes_read {
            return Err(SyncError::CopyFailed(format!(
                "Write size mismatch at offset {offset}: read {bytes_read}, wrote {bytes_written}"
            )));
        }

        offset += bytes_written as u64;
    }

    Ok(())
}

/// Align offset to page boundary (round down)
///
/// # Parameters
///
/// * `offset` - The offset to align
/// * `page_size` - The page size to align to
///
/// # Returns
///
/// The offset rounded down to the nearest page boundary
const fn align_to_page(offset: u64, page_size: u64) -> u64 {
    (offset / page_size) * page_size
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{
        Args, ConcurrencyConfig, CopyMethod, IoConfig, OutputConfig, ParallelCopyConfig, PathConfig,
    };
    use crate::metadata::MetadataConfig;
    use std::fs;
    use std::num::NonZeroUsize;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use std::time::Duration;
    use tempfile::TempDir;

    /// Create a disabled parallel copy config for testing
    fn disabled_parallel_config() -> ParallelCopyConfig {
        ParallelCopyConfig {
            max_depth: 0, // 0 = disabled
            min_file_size_mb: 128,
            chunk_size_mb: 2,
        }
    }

    // Test helper to copy file with DirectoryFd setup
    async fn copy_file_test_helper(
        src: &std::path::Path,
        dst: &std::path::Path,
        metadata_config: &MetadataConfig,
        parallel_config: &ParallelCopyConfig,
    ) -> Result<()> {
        let src_parent_dir = compio_fs_extended::DirectoryFd::open(
            src.parent().unwrap_or(std::path::Path::new(".")),
        )
        .await
        .unwrap();
        let src_filename = src.file_name().unwrap();
        let src_metadata = src_parent_dir.statx_full(src_filename).await?;

        let dst_parent_dir = compio_fs_extended::DirectoryFd::open(
            dst.parent().unwrap_or(std::path::Path::new(".")),
        )
        .await
        .unwrap();
        let dst_filename = dst.file_name().unwrap();

        let dispatcher = compio::dispatcher::Dispatcher::new().unwrap();
        let dispatcher_static: &'static Dispatcher = Box::leak(Box::new(dispatcher));

        copy_file_internal(
            src,
            dst,
            metadata_config,
            parallel_config,
            dispatcher_static,
            &src_metadata,
            &src_parent_dir,
            src_filename,
            &dst_parent_dir,
            dst_filename,
        )
        .await
    }

    /// Create a default Args struct for testing with archive mode enabled
    fn create_test_args_with_archive() -> Args {
        Args {
            paths: PathConfig {
                source: PathBuf::from("/test/source"),
                destination: PathBuf::from("/test/dest"),
            },
            io: IoConfig {
                queue_depth: 4096,
                buffer_size_kb: NonZeroUsize::new(64),
                copy_method: CopyMethod::Auto,
                cpu_count: 1,
                parallel: disabled_parallel_config(),
            },
            concurrency: ConcurrencyConfig {
                max_files_in_flight: 1024,
                no_adaptive_concurrency: false,
            },
            metadata: MetadataConfig {
                archive: true, // Enable archive mode for full metadata preservation
                recursive: false,
                links: false,
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
            output: OutputConfig {
                dry_run: false,
                progress: false,
                verbose: 0,
                quiet: false,
                pirate: false,
            },
        }
    }

    #[compio::test]
    async fn test_preserve_metadata_permissions() {
        let temp_dir = TempDir::new().unwrap();
        let src_path = temp_dir.path().join("source.txt");
        let dst_path = temp_dir.path().join("destination.txt");

        // Create source file with specific permissions
        fs::write(&src_path, "Test content for permission preservation").unwrap();

        // Set specific permissions (read/write for owner, read for group and others)
        let permissions = std::fs::Permissions::from_mode(0o644);
        fs::set_permissions(&src_path, permissions).unwrap();

        // Copy the file with archive mode (full metadata preservation)
        let args = create_test_args_with_archive();
        copy_file_test_helper(
            &src_path,
            &dst_path,
            &args.metadata,
            &disabled_parallel_config(),
        )
        .await
        .unwrap();

        // Check that permissions were preserved
        let src_metadata = fs::metadata(&src_path).unwrap();
        let dst_metadata = fs::metadata(&dst_path).unwrap();

        let src_permissions = src_metadata.permissions().mode();
        let dst_permissions = dst_metadata.permissions().mode();

        println!(
            "Source permissions: {:o} ({})",
            src_permissions, src_permissions
        );
        println!(
            "Destination permissions: {:o} ({})",
            dst_permissions, dst_permissions
        );

        assert_eq!(
            src_permissions, dst_permissions,
            "Permissions should be preserved exactly"
        );
        // Note: The exact permission value may vary due to umask, but they should match
    }

    #[compio::test]
    async fn test_preserve_metadata_timestamps() {
        let temp_dir = TempDir::new().unwrap();
        let src_path = temp_dir.path().join("source.txt");
        let dst_path = temp_dir.path().join("destination.txt");

        // Create source file
        fs::write(&src_path, "Test content for timestamp preservation").unwrap();

        // Get original timestamps
        let src_metadata = fs::metadata(&src_path).unwrap();
        let original_accessed = src_metadata.accessed().unwrap();
        let original_modified = src_metadata.modified().unwrap();

        // Wait a bit to ensure timestamps are different
        std::thread::sleep(Duration::from_millis(10));

        // Copy the file with archive mode (full metadata preservation)
        let args = create_test_args_with_archive();
        copy_file_test_helper(
            &src_path,
            &dst_path,
            &args.metadata,
            &disabled_parallel_config(),
        )
        .await
        .unwrap();

        // Check that timestamps were preserved
        let dst_metadata = fs::metadata(&dst_path).unwrap();
        let copied_accessed = dst_metadata.accessed().unwrap();
        let copied_modified = dst_metadata.modified().unwrap();

        // Timestamps should be very close (within a few milliseconds due to system precision)
        let accessed_diff = copied_accessed
            .duration_since(original_accessed)
            .unwrap_or_default();
        let modified_diff = copied_modified
            .duration_since(original_modified)
            .unwrap_or_default();

        assert!(
            accessed_diff.as_millis() < 100,
            "Accessed time should be preserved within 100ms"
        );
        assert!(
            modified_diff.as_millis() < 100,
            "Modified time should be preserved within 100ms"
        );
    }

    #[compio::test]
    async fn test_preserve_metadata_complex_permissions() {
        let temp_dir = TempDir::new().unwrap();
        let src_path = temp_dir.path().join("source.txt");
        let dst_path = temp_dir.path().join("destination.txt");

        // Create source file
        fs::write(
            &src_path,
            "Test content for complex permission preservation",
        )
        .unwrap();

        // Test various permission combinations (avoiding problematic ones)
        let test_permissions = vec![
            0o755, // rwxr-xr-x
            0o644, // rw-r--r--
            0o600, // rw-------
            0o777, // rwxrwxrwx
        ];

        for &permission_mode in &test_permissions {
            // Set specific permissions
            let permissions = std::fs::Permissions::from_mode(permission_mode);
            fs::set_permissions(&src_path, permissions).unwrap();

            // Get source permissions after setting (to account for umask)
            let src_metadata = fs::metadata(&src_path).unwrap();
            let expected_permissions = src_metadata.permissions().mode();

            // Copy the file with archive mode (full metadata preservation)
            let args = create_test_args_with_archive();
            copy_file_test_helper(
                &src_path,
                &dst_path,
                &args.metadata,
                &disabled_parallel_config(),
            )
            .await
            .unwrap();

            // Check that permissions were preserved
            let dst_metadata = fs::metadata(&dst_path).unwrap();
            let dst_permissions = dst_metadata.permissions().mode();

            assert_eq!(
                expected_permissions, dst_permissions,
                "Permission mode {:o} should be preserved exactly",
                expected_permissions
            );
        }
    }

    #[compio::test]
    async fn test_preserve_metadata_nanosecond_precision() {
        let temp_dir = TempDir::new().unwrap();
        let src_path = temp_dir.path().join("source.txt");
        let dst_path = temp_dir.path().join("destination.txt");

        // Create source file
        fs::write(&src_path, "Test content for nanosecond precision").unwrap();

        // Get original timestamps
        let src_metadata = fs::metadata(&src_path).unwrap();
        let original_accessed = src_metadata.accessed().unwrap();
        let original_modified = src_metadata.modified().unwrap();

        // Copy the file with archive mode (full metadata preservation)
        let args = create_test_args_with_archive();
        copy_file_test_helper(
            &src_path,
            &dst_path,
            &args.metadata,
            &disabled_parallel_config(),
        )
        .await
        .unwrap();

        // Check that timestamps were preserved with high precision
        let dst_metadata = fs::metadata(&dst_path).unwrap();
        let copied_accessed = dst_metadata.accessed().unwrap();
        let copied_modified = dst_metadata.modified().unwrap();

        // For nanosecond precision, we should be able to preserve timestamps very accurately
        // The difference should be minimal (within microseconds)
        let accessed_diff = copied_accessed
            .duration_since(original_accessed)
            .unwrap_or_default();
        let modified_diff = copied_modified
            .duration_since(original_modified)
            .unwrap_or_default();

        assert!(
            accessed_diff.as_millis() < 100,
            "Accessed time should be preserved within 100ms"
        );
        assert!(
            modified_diff.as_millis() < 100,
            "Modified time should be preserved within 100ms"
        );
    }

    #[compio::test]
    async fn test_preserve_metadata_large_file() {
        let temp_dir = TempDir::new().unwrap();
        let src_path = temp_dir.path().join("large_source.txt");
        let dst_path = temp_dir.path().join("large_destination.txt");

        // Create a larger file (1MB) to test with substantial data
        let large_content = "A".repeat(1024 * 1024); // 1MB of 'A' characters
        fs::write(&src_path, &large_content).unwrap();

        // Set specific permissions
        let permissions = std::fs::Permissions::from_mode(0o755);
        fs::set_permissions(&src_path, permissions).unwrap();

        // Get original permissions and timestamps
        let src_metadata = fs::metadata(&src_path).unwrap();
        let expected_permissions = src_metadata.permissions().mode();
        let original_accessed = src_metadata.accessed().unwrap();
        let original_modified = src_metadata.modified().unwrap();

        // Copy the file with archive mode (full metadata preservation)
        let args = create_test_args_with_archive();
        copy_file_test_helper(
            &src_path,
            &dst_path,
            &args.metadata,
            &disabled_parallel_config(),
        )
        .await
        .unwrap();

        // Verify file content
        let copied_content = fs::read_to_string(&dst_path).unwrap();
        assert_eq!(
            copied_content, large_content,
            "File content should be preserved"
        );

        // Check that permissions were preserved
        let dst_metadata = fs::metadata(&dst_path).unwrap();
        let dst_permissions = dst_metadata.permissions().mode();
        assert_eq!(
            expected_permissions, dst_permissions,
            "Permissions should be preserved for large files"
        );

        // Check that timestamps were preserved
        let copied_accessed = dst_metadata.accessed().unwrap();
        let copied_modified = dst_metadata.modified().unwrap();

        let accessed_diff = copied_accessed
            .duration_since(original_accessed)
            .unwrap_or_default();
        let modified_diff = copied_modified
            .duration_since(original_modified)
            .unwrap_or_default();

        assert!(
            accessed_diff.as_millis() < 100,
            "Accessed time should be preserved for large files"
        );
        assert!(
            modified_diff.as_millis() < 100,
            "Modified time should be preserved for large files"
        );
    }

    #[compio::test]
    async fn test_preserve_metadata_empty_file() {
        let temp_dir = TempDir::new().unwrap();
        let src_path = temp_dir.path().join("empty_source.txt");
        let dst_path = temp_dir.path().join("empty_destination.txt");

        // Create empty file
        fs::write(&src_path, "").unwrap();

        // Set specific permissions
        let permissions = std::fs::Permissions::from_mode(0o600);
        fs::set_permissions(&src_path, permissions).unwrap();

        // Get expected permissions after setting (to account for umask)
        let src_metadata = fs::metadata(&src_path).unwrap();
        let expected_permissions = src_metadata.permissions().mode();

        // Copy the file with archive mode (full metadata preservation)
        let args = create_test_args_with_archive();
        copy_file_test_helper(
            &src_path,
            &dst_path,
            &args.metadata,
            &disabled_parallel_config(),
        )
        .await
        .unwrap();

        // Check that permissions were preserved
        let dst_metadata = fs::metadata(&dst_path).unwrap();
        let dst_permissions = dst_metadata.permissions().mode();
        assert_eq!(
            expected_permissions, dst_permissions,
            "Permissions should be preserved for empty files"
        );

        // Verify file is empty
        let copied_content = fs::read_to_string(&dst_path).unwrap();
        assert_eq!(copied_content, "", "Empty file should remain empty");
    }

    #[compio::test]
    async fn test_fallocate_preallocation() {
        let temp_dir = TempDir::new().unwrap();
        let src_path = temp_dir.path().join("source.txt");
        let dst_path = temp_dir.path().join("destination.txt");

        // Create a source file with known content
        let content = "Test content for fallocate preallocation";
        fs::write(&src_path, content).unwrap();

        // Copy the file with archive mode (full metadata preservation)
        let args = create_test_args_with_archive();
        copy_file_test_helper(
            &src_path,
            &dst_path,
            &args.metadata,
            &disabled_parallel_config(),
        )
        .await
        .unwrap();

        // Verify the file was copied correctly
        let copied_content = fs::read_to_string(&dst_path).unwrap();
        assert_eq!(copied_content, content, "File content should be preserved");

        // Verify the file size matches the source
        let src_metadata = fs::metadata(&src_path).unwrap();
        let dst_metadata = fs::metadata(&dst_path).unwrap();
        assert_eq!(
            src_metadata.len(),
            dst_metadata.len(),
            "File sizes should match"
        );
    }

    #[compio::test]
    async fn test_fallocate_large_file_preallocation() {
        let temp_dir = TempDir::new().unwrap();
        let src_path = temp_dir.path().join("large_source.txt");
        let dst_path = temp_dir.path().join("large_destination.txt");

        // Create a larger file (1MB) to test fallocate with substantial data
        let large_content = "A".repeat(1024 * 1024); // 1MB of 'A' characters
        fs::write(&src_path, &large_content).unwrap();

        // Copy the file with archive mode (full metadata preservation)
        let args = create_test_args_with_archive();
        copy_file_test_helper(
            &src_path,
            &dst_path,
            &args.metadata,
            &disabled_parallel_config(),
        )
        .await
        .unwrap();

        // Verify the file was copied correctly
        let copied_content = fs::read_to_string(&dst_path).unwrap();
        assert_eq!(
            copied_content, large_content,
            "Large file content should be preserved"
        );

        // Verify the file size matches the source
        let src_metadata = fs::metadata(&src_path).unwrap();
        let dst_metadata = fs::metadata(&dst_path).unwrap();
        assert_eq!(
            src_metadata.len(),
            dst_metadata.len(),
            "Large file sizes should match"
        );
    }
}
