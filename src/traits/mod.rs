//! Core traits for filesystem and protocol integration
//!
//! This module provides the foundational traits that enable unified operations
//! between local filesystem and remote protocol backends. The traits are designed
//! to work with compio's async I/O model and provide a consistent interface
//! regardless of the underlying storage mechanism.

pub mod filesystem;
pub mod file;
pub mod directory;
pub mod metadata;
pub mod operations;

#[cfg(test)]
mod tests;

// Re-export main traits for convenience
pub use filesystem::AsyncFileSystem;
pub use file::AsyncFile;
pub use directory::AsyncDirectory;
pub use metadata::AsyncMetadata;
pub use operations::{FileOperations, GenericFileOperations};

/// Common result type for filesystem operations
pub type Result<T> = std::result::Result<T, crate::error::SyncError>;