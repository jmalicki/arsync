//! Error handling and types

use thiserror::Error;

/// Synchronization and file operation errors
#[derive(Error, Debug)]
pub enum SyncError {
    /// Standard I/O error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// `io_uring` specific error
    #[error("io_uring error: {0}")]
    IoUring(String),

    /// File copy operation failed
    #[error("Copy operation failed: {0}")]
    CopyFailed(String),

    /// Metadata operation failed
    #[error("Metadata operation failed: {0}")]
    #[allow(dead_code)]
    MetadataFailed(String),

    /// Directory traversal failed
    #[error("Directory traversal failed: {0}")]
    #[allow(dead_code)]
    DirectoryTraversal(String),

    /// Invalid configuration error
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    /// Permission denied error
    #[error("Permission denied: {0}")]
    #[allow(dead_code)]
    PermissionDenied(String),

    /// General filesystem error
    #[error("File system error: {0}")]
    FileSystem(String),

    /// File descriptor exhaustion (EMFILE)
    #[error("File descriptor exhaustion: {0}")]
    #[allow(dead_code)]
    FdExhaustion(String),

    /// Internal application error
    #[error("Internal error: {0}")]
    #[allow(dead_code)]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, SyncError>;
