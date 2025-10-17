//! # compio-fs-extended
//!
//! Extended filesystem operations for compio with support for:
//! - `copy_file_range` for efficient same-filesystem copies
//! - `fadvise` for file access pattern optimization
//! - Symlink operations (create, read, metadata)
//! - Hardlink operations
//! - Extended attributes (xattr) using io_uring opcodes
//! - Directory operations
//!
//! This crate extends `compio::fs::File` with additional operations that are not
//! available in the base compio-fs crate, using direct syscalls integrated with
//! compio's runtime for optimal performance.
//!
//! ## Example
//!
//! ```rust,no_run
//! use compio_fs_extended::{ExtendedFile, CopyFileRange};
//! use compio::fs::File;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Open source and destination files
//! let src_file = File::open("source.txt").await?;
//! let dst_file = File::create("destination.txt").await?;
//!
//! // Create extended file wrapper
//! let src_extended = ExtendedFile::new(src_file);
//! let dst_extended = ExtendedFile::new(dst_file);
//!
//! // Use copy_file_range for efficient copying
//! let bytes_copied = src_extended.copy_file_range(&dst_extended, 0, 0, 1024).await?;
//! println!("Copied {} bytes", bytes_copied);
//! # Ok(())
//! # }
//! ```

pub mod copy;
pub mod device;
pub mod directory;
pub mod error;
pub mod extended_file;
pub mod fadvise;
pub mod fallocate;
pub mod hardlink;
pub mod metadata;
pub mod ownership;
pub mod symlink;
pub mod xattr;

// Platform-specific shims (none required at module level yet)

// Re-export main types
pub use error::{ExtendedError, Result};
pub use extended_file::ExtendedFile;

// Re-export specific operation modules
pub use copy::CopyFileRange;
// DirectoryOps removed - use compio::fs directly for basic directory operations
pub use fadvise::Fadvise;
pub use fallocate::Fallocate;
pub use hardlink::HardlinkOps;
pub use ownership::OwnershipOps;
pub use symlink::SymlinkOps;
pub use xattr::XattrOps;

/// Version information
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Feature flags available
pub mod features {
    /// xattr support using io_uring opcodes
    pub const XATTR: &str = "xattr";
    /// Performance metrics collection
    pub const METRICS: &str = "metrics";
    /// Logging integration
    pub const LOGGING: &str = "logging";
}
