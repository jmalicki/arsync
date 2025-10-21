//! # compio-fs-extended
//!
//! Extended filesystem operations for compio with support for:
//! - `fadvise` for file access pattern optimization
//! - `fallocate` for space preallocation
//! - Symlink operations (create, read, metadata)
//! - Hardlink operations
//! - Extended attributes (xattr) using io_uring opcodes
//! - Directory operations with secure *at syscalls
//! - File ownership operations
//!
//! This crate extends `compio::fs::File` with additional operations that are not
//! available in the base compio-fs crate, using direct syscalls integrated with
//! compio's runtime for optimal performance.
//!
//! ## Example
//!
//! ```rust,no_run
//! # #[cfg(target_os = "linux")]
//! # {
//! use compio_fs_extended::{ExtendedFile, Fadvise};
//! use compio_fs_extended::fadvise::FadviseAdvice;
//! use compio::fs::File;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Open a file
//! let file = File::open("data.txt").await?;
//! let extended = ExtendedFile::new(file);
//!
//! // Give kernel advice about access pattern (advice, offset, length)
//! extended.fadvise(FadviseAdvice::Sequential, 0, 0).await?;
//! # Ok(())
//! # }
//! # }
//! ```
//!
//! Note: `fadvise` operations are only available on Linux.
//!
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
pub mod write_managed;
pub mod xattr;

// Platform-specific shims (none required at module level yet)

// Re-export main types
pub use directory::DirectoryFd;
pub use error::{ExtendedError, Result};
pub use extended_file::ExtendedFile;
pub use metadata::FileMetadata;

// Re-export specific operation modules
#[cfg(target_os = "linux")]
pub use fadvise::Fadvise;
pub use fallocate::Fallocate;
pub use hardlink::HardlinkOps;
pub use ownership::OwnershipOps;
pub use symlink::SymlinkOps;
pub use write_managed::AsyncWriteManagedAt;
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
