//! Main synchronization logic
//!
//! This module provides the core synchronization functionality for arsync,
//! orchestrating file and directory copying operations with comprehensive error
//! handling, progress tracking, and performance optimization.
//!
//! # Features
//!
//! - Single file copying with metadata preservation
//! - Directory structure creation and management
//! - Comprehensive error handling and logging
//! - Performance statistics and timing
//! - Async/await support for non-blocking operations
//!
//! # Architecture
//!
//! The synchronization process follows these phases:
//! 1. **Validation**: Verify source paths and destination permissions
//! 2. **Initialization**: Set up file operations and progress tracking
//! 3. **Execution**: Perform the actual copying operations
//! 4. **Completion**: Update statistics and handle cleanup
//!
//! # Usage
//!
//! ```rust,ignore
//! use arsync::sync::sync_files;
//! use arsync::cli::Args;
//! use clap::Parser;
//!
//! #[compio::main]
//! async fn main() -> arsync::Result<()> {
//!     let args = Args::parse();
//!     let stats = sync_files(&args).await?;
//!     println!("Copied {} files, {} bytes in {:?}",
//!              stats.files_copied, stats.bytes_copied, stats.duration);
//!     Ok(())
//! }
//! ```
//!
//! # Performance Considerations
//!
//! - Uses async I/O for non-blocking operations
//! - Tracks detailed performance metrics
//! - Supports configurable buffer sizes and queue depths
//! - Optimized for both small and large file operations
//!
//! # Error Handling
//!
//! All operations use structured error handling with detailed error messages
//! and proper error propagation. Common error scenarios include:
//!
//! - Invalid source paths or permissions
//! - Destination directory creation failures
//! - File copying errors with detailed context
//! - Configuration validation failures

use crate::cli::Args;
use crate::directory::copy_directory;
use crate::error::Result;
use crate::io_uring::FileOperations;
use std::time::{Duration, Instant};
use tracing::{error, info};

/// Statistics for a synchronization operation
///
/// This structure tracks comprehensive metrics about the synchronization
/// process, including performance data and operation results.
///
/// # Fields
///
/// * `files_copied` - Number of files successfully copied
/// * `bytes_copied` - Total number of bytes copied
/// * `duration` - Total time taken for the synchronization operation
///
/// # Examples
///
/// ```rust
/// use arsync::sync::SyncStats;
/// use std::time::Duration;
///
/// let stats = SyncStats {
///     files_copied: 150,
///     bytes_copied: 1_048_576,
///     duration: Duration::from_secs(5),
/// };
/// println!("Copied {} files ({} bytes) in {:?}",
///          stats.files_copied, stats.bytes_copied, stats.duration);
/// ```
///
/// # Performance Notes
///
/// These statistics are useful for:
/// - Performance monitoring and optimization
/// - User feedback and progress reporting
/// - Benchmarking and comparison with other tools
/// - Debugging and troubleshooting slow operations
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncStats {
    /// Number of files successfully copied during the operation
    pub files_copied: u64,

    /// Total number of bytes copied during the operation
    pub bytes_copied: u64,

    /// Total duration of the synchronization operation
    pub duration: Duration,
}

/// Main synchronization function
///
/// This function orchestrates the entire synchronization process, handling both
/// single file and directory operations with comprehensive error handling and
/// performance tracking.
///
/// # Parameters
///
/// * `args` - Command-line arguments containing source/destination paths,
///   configuration options, and operation parameters
///
/// # Returns
///
/// Returns `Ok(SyncStats)` containing detailed operation statistics, or
/// `Err(SyncError)` if the synchronization fails.
///
/// # Errors
///
/// This function will return an error if:
/// - Source path doesn't exist or is inaccessible
/// - Destination directory cannot be created
/// - File operations fail during copying
/// - Configuration parameters are invalid
/// - I/O errors occur during the operation
///
/// # Examples
///
/// Basic usage:
/// ```rust,ignore
/// use arsync::cli::Args;
/// use arsync::sync::sync_files;
/// use clap::Parser;
///
/// #[compio::main]
/// async fn main() -> arsync::Result<()> {
///     let args = Args::parse();
///     let stats = sync_files(&args).await?;
///     println!("Operation completed: {} files, {} bytes, {:?}",
///              stats.files_copied, stats.bytes_copied, stats.duration);
///     Ok(())
/// }
/// ```
///
/// Error handling:
/// ```rust,ignore
/// use arsync::cli::Args;
/// use arsync::sync::sync_files;
/// use clap::Parser;
///
/// #[compio::main]
/// async fn main() -> arsync::Result<()> {
///     let args = Args::parse();
///     match sync_files(&args).await {
///         Ok(stats) => {
///             println!("Success: {} files copied", stats.files_copied);
///         }
///         Err(e) => {
///             eprintln!("Synchronization failed: {}", e);
///             std::process::exit(1);
///         }
///     }
///     Ok(())
/// }
/// ```
///
/// # Performance Considerations
///
/// - Operation time scales with file size and count
/// - Memory usage depends on buffer size configuration
/// - Concurrent operations may be limited by system resources
/// - Network filesystems may have different performance characteristics
///
/// # Thread Safety
///
/// This function is thread-safe and can be called concurrently, but each
/// call operates independently. For optimal performance, avoid concurrent
/// operations on the same destination paths.
///
/// # Implementation Details
///
/// The synchronization process:
/// 1. Validates arguments and initializes file operations
/// 2. Determines operation type (file vs directory)
/// 3. Creates destination directories as needed
/// 4. Performs the actual copying operations
/// 5. Tracks statistics and handles errors
/// 6. Returns comprehensive operation results
#[allow(clippy::future_not_send)]
pub async fn sync_files(args: &Args) -> Result<SyncStats> {
    let start_time = Instant::now();

    info!(
        "Starting synchronization from {} to {}",
        args.source().display(),
        args.destination().display()
    );

    let mut stats = SyncStats {
        files_copied: 0,
        bytes_copied: 0,
        duration: Duration::from_secs(0),
    };

    // Initialize file operations with configured parameters
    // Queue depth and buffer size are validated by the CLI module
    let file_ops = FileOperations::new(args.queue_depth(), args.buffer_size_bytes())?;

    // Handle single file copy
    if args.is_file_copy() {
        info!("Copying single file: {}", args.source().display());

        // Ensure destination directory exists
        if let Some(parent) = args.destination().parent() {
            file_ops.create_dir(parent).await?;
        }

        // Note: file size is now obtained within copy_file_with_metadata

        // Copy the file with metadata preservation
        match file_ops
            .copy_file_with_metadata(args.source(), args.destination(), &args.io.parallel)
            .await
        {
            Ok(bytes_copied) => {
                stats.files_copied = 1;
                stats.bytes_copied = bytes_copied;
                info!(
                    "Successfully copied file with metadata: {} bytes",
                    bytes_copied
                );
            }
            Err(e) => {
                error!("Failed to copy file {}: {}", args.source().display(), e);
                return Err(e);
            }
        }
    }
    // Handle directory copy
    else if args.is_directory_copy() {
        info!("Copying directory: {}", args.source().display());

        // Ensure destination directory exists
        file_ops.create_dir(args.destination()).await?;

        // Copy directory recursively
        let dir_stats = copy_directory(
            args.source(),
            args.destination(),
            &file_ops,
            args.copy_method().clone(),
            args,
        )
        .await?;

        // Update statistics
        stats.files_copied = dir_stats.files_copied;
        stats.bytes_copied = dir_stats.bytes_copied;

        info!(
            "Directory copy completed: {} files, {} directories, {} bytes, {} errors",
            dir_stats.files_copied,
            dir_stats.directories_created,
            dir_stats.bytes_copied,
            dir_stats.errors
        );
    } else {
        error!(
            "Source path is neither a file nor a directory: {}",
            args.source().display()
        );
        return Err(crate::error::SyncError::InvalidConfig(
            "Source must be a file or directory".to_string(),
        ));
    }

    stats.duration = start_time.elapsed();

    info!("Synchronization completed in {:?}", stats.duration);
    info!(
        "Files copied: {}, Bytes copied: {}",
        stats.files_copied, stats.bytes_copied
    );

    Ok(stats)
}
