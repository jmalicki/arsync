//! arsync: High-performance file copying using `io_uring`
//!
//! This library provides efficient file copying capabilities using Linux's `io_uring`
//! interface for asynchronous I/O operations, similar to rsync but optimized for
//! single-machine operations with parallelism and metadata preservation.
//!
//! ## Key Features
//!
//! - **Integrated Hardlink Detection**: Smart hardlink detection during traversal
//!   - Content is copied only once per unique inode
//!   - Subsequent files with the same inode become hardlinks
//!   - Uses `io_uring statx` for efficient metadata analysis
//!
//! - **Single-Pass Operation**: Efficient traversal that discovers and copies in one pass
//!   - No separate analysis phase required
//!   - Progress tracking shows both discovery and completion progress
//!   - Filesystem boundary detection integrated into traversal
//!
//! - **`io_uring` Throughout**: All operations use `io_uring` for maximum performance
//!   - `statx` for metadata discovery
//!   - `copy_file_range` for file copying
//!   - `linkat` for hardlink creation
//!   - `symlinkat`/`readlinkat` for symlink handling
//!
//! ## Usage Example
//!
//! ```rust,ignore
//! use arsync::directory::copy_directory;
//! use arsync::io_uring::FileOperations;
//! use arsync::cli::CopyMethod;
//!
//! #[compio::main]
//! async fn main() -> arsync::Result<()> {
//!     let file_ops = FileOperations::new(4096, 64 * 1024)?;
//!     let stats = copy_directory(
//!         &std::path::Path::new("/source"),
//!         &std::path::Path::new("/destination"),
//!         &file_ops,
//!         CopyMethod::Auto,
//!     ).await?;
//!     
//!     println!("Copied {} files, {} directories, {} bytes",
//!              stats.files_copied, stats.directories_created, stats.bytes_copied);
//!     Ok(())
//! }
//! ```

pub mod adaptive_concurrency;
pub mod cli;
pub mod copy;
pub mod directory;
pub mod error;
pub mod hardlink_tracker;
pub mod i18n;
pub mod io_uring;
pub mod metadata;
pub mod progress;
pub mod sync;

// Re-export commonly used types
pub use directory::FilesystemTracker;
pub use error::{Result, SyncError};
pub use progress::ProgressTracker;

// Re-export semaphore from compio-sync crate
pub use compio_sync::Semaphore;
