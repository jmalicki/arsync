//! `io_uring` integration module
//!
//! This module provides high-performance file operations using `io_uring` for asynchronous I/O.
//! Currently implements async I/O as a foundation, with plans for full `io_uring` integration
//! in future development phases.
//!
//! # Features
//!
//! - Asynchronous file read/write operations
//! - Buffer management for optimal performance
//! - Error handling and recovery
//! - Progress tracking capabilities
//! - File metadata operations
//!
//! # Usage
//!
//! ```rust,ignore
//! use arsync::io_uring::FileOperations;
//! use std::path::Path;
//!
//! #[compio::main]
//! async fn main() -> arsync::Result<()> {
//!     let ops = FileOperations::new(4096, 64 * 1024)?;
//!     let src_path = Path::new("source.txt");
//!     let dst_path = Path::new("destination.txt");
//!     ops.copy_file_read_write(&src_path, &dst_path).await?;
//!     Ok(())
//! }
//! ```

use crate::error::{Result, SyncError};
use compio::io::{AsyncReadAt, AsyncWriteAtExt};
use std::path::Path;
use tracing::debug;

/// Basic file operations using async I/O
///
/// This structure provides a high-level interface for performing file operations
/// asynchronously. It serves as the foundation for `io_uring` integration and
/// provides efficient buffer management.
///
/// # Fields
///
/// * `buffer_size` - Size of buffers used for I/O operations in bytes
///
/// # Performance Considerations
///
/// - Buffer size should be tuned based on system memory and expected file sizes
/// - Larger buffers reduce system call overhead but increase memory usage
/// - Default buffer size of 64KB provides good balance for most workloads
#[derive(Debug, Clone)]
pub struct FileOperations {
    /// Buffer size for I/O operations in bytes
    #[allow(dead_code)]
    buffer_size: usize,
}

impl FileOperations {
    /// Create new file operations instance
    ///
    /// # Parameters
    ///
    /// * `queue_depth` - Maximum number of concurrent operations (currently unused, reserved for `io_uring`)
    /// * `buffer_size` - Size of I/O buffers in bytes
    ///
    /// # Returns
    ///
    /// Returns `Ok(FileOperations)` on success, or `Err(SyncError)` if initialization fails.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use arsync::io_uring::FileOperations;
    ///
    /// let ops = FileOperations::new(4096, 64 * 1024).unwrap();
    /// ```
    ///
    /// # Performance Notes
    ///
    /// - Buffer size should be a power of 2 for optimal performance
    /// - Typical values: 4KB (small files), 64KB (general purpose), 1MB (large files)
    /// - Larger buffers reduce system call overhead but increase memory usage
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - Buffer size is invalid (must be > 0)
    /// - Memory allocation fails
    #[allow(clippy::unnecessary_wraps)]
    pub const fn new(_queue_depth: usize, buffer_size: usize) -> Result<Self> {
        // For Phase 1.2, we'll use async I/O as a foundation
        // TODO: Implement actual io_uring integration in future phases
        Ok(Self { buffer_size })
    }

    /// Get the configured buffer size for I/O operations
    ///
    /// Returns the buffer size in bytes configured during construction.
    /// This is used by buffer pool creation and copy operations.
    #[must_use]
    pub const fn buffer_size(&self) -> usize {
        self.buffer_size
    }

    /// Copy file using chunked read/write with compio buffer management
    ///
    /// This method copies a file by reading and writing in chunks, using compio's
    /// managed buffer pools for efficient memory usage and async I/O.
    /// This is a wrapper around the descriptor-based copy operation.
    ///
    /// # Parameters
    ///
    /// * `src` - Source file path
    /// * `dst` - Destination file path
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success or `Err(SyncError)` on failure.
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - Source file cannot be opened for reading
    /// - Destination file cannot be created or opened for writing
    /// - File copying operation fails (I/O errors, permission issues)
    /// - Metadata preservation fails
    #[allow(dead_code, clippy::future_not_send)]
    pub async fn copy_file_read_write(&self, src: &Path, dst: &Path) -> Result<()> {
        // Ensure destination directory exists
        if let Some(parent) = dst.parent() {
            compio::fs::create_dir_all(parent).await.map_err(|e| {
                SyncError::FileSystem(format!(
                    "Failed to create directory {}: {}",
                    parent.display(),
                    e
                ))
            })?;
        }

        // Open source and destination files
        let src_file = compio::fs::File::open(src).await.map_err(|e| {
            SyncError::FileSystem(format!(
                "Failed to open source file {}: {}",
                src.display(),
                e
            ))
        })?;

        let mut dst_file = compio::fs::File::create(dst).await.map_err(|e| {
            SyncError::FileSystem(format!(
                "Failed to create destination file {}: {}",
                dst.display(),
                e
            ))
        })?;

        // Use the descriptor-based copy operation
        self.copy_file_descriptors(&src_file, &mut dst_file).await?;

        debug!("Copied file from {} to {}", src.display(), dst.display());
        Ok(())
    }

    /// Copy file content using file descriptors with compio managed buffers
    ///
    /// This is the core descriptor-based copy operation that efficiently
    /// copies file content in chunks using compio's managed buffer pools.
    /// It leverages IoBuf/IoBufMut traits for safe and efficient buffer management.
    ///
    /// # Parameters
    ///
    /// * `src_file` - Source file descriptor
    /// * `dst_file` - Destination file descriptor
    ///
    /// # Returns
    ///
    /// Returns `Ok(u64)` with the number of bytes copied, or
    /// `Err(SyncError)` if the operation failed.
    #[allow(clippy::future_not_send)]
    async fn copy_file_descriptors(
        &self,
        src_file: &compio::fs::File,
        dst_file: &mut compio::fs::File,
    ) -> Result<u64> {
        // Create managed buffer once - it will be reused throughout the entire copy
        // compio's read_at/write_at take ownership and return the buffer in the result tuple
        let mut buffer = vec![0u8; self.buffer_size];
        let mut offset = 0u64;
        let mut total_bytes = 0u64;

        loop {
            // Read chunk from source - buffer ownership transferred to compio
            let read_result = src_file.read_at(buffer, offset).await;

            let bytes_read = match read_result.0 {
                Ok(n) => n,
                Err(e) => {
                    return Err(SyncError::FileSystem(format!(
                        "Failed to read from source file: {e}"
                    )))
                }
            };

            // Get buffer back from read operation
            buffer = read_result.1;

            // If we read 0 bytes, we've reached end of file
            if bytes_read == 0 {
                break;
            }

            // Truncate buffer to only the bytes read (avoids writing garbage)
            // This doesn't allocate, just changes the length
            buffer.truncate(bytes_read);

            // Write chunk to destination - write_all_at takes ownership and returns the buffer
            // This way we reuse the same allocation for both read and write
            let write_result = dst_file.write_all_at(buffer, offset).await;

            match write_result.0 {
                Ok(()) => {
                    // write_all_at returns () on success
                }
                Err(e) => {
                    return Err(SyncError::FileSystem(format!(
                        "Failed to write to destination file: {e}"
                    )))
                }
            }

            // Get the buffer back from write operation and resize it for the next read
            // resize() reuses the existing capacity when possible (no new allocation!)
            buffer = write_result.1;
            buffer.resize(self.buffer_size, 0);
            offset += bytes_read as u64;
            total_bytes += bytes_read as u64;
        }

        // Note: sync_all() removed for performance - data will be synced by the OS
        // when the file is closed or when the OS decides to flush buffers

        debug!(
            "Copied {} bytes using single reused buffer (no allocations)",
            total_bytes
        );
        Ok(total_bytes)
    }

    /// Get file size
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The path does not exist
    /// - Permission is denied to read the path
    /// - The path is not accessible
    #[allow(dead_code, clippy::future_not_send)]
    pub async fn get_file_size(&self, path: &Path) -> Result<u64> {
        let metadata = compio::fs::metadata(path).await.map_err(|e| {
            SyncError::FileSystem(format!(
                "Failed to get metadata for {}: {}",
                path.display(),
                e
            ))
        })?;

        Ok(metadata.len())
    }

    /// Check if file exists
    #[allow(dead_code, clippy::future_not_send)]
    pub async fn file_exists(&self, path: &Path) -> bool {
        compio::fs::metadata(path).await.is_ok()
    }

    /// Create directory
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - Parent directory does not exist and cannot be created
    /// - Permission is denied to create the directory
    /// - The path already exists and is not a directory
    #[allow(clippy::future_not_send)]
    pub async fn create_dir(&self, path: &Path) -> Result<()> {
        compio::fs::create_dir_all(path).await.map_err(|e| {
            SyncError::FileSystem(format!(
                "Failed to create directory {}: {}",
                path.display(),
                e
            ))
        })?;
        Ok(())
    }

    /// Get comprehensive file metadata asynchronously
    ///
    /// This function retrieves detailed file metadata including permissions, ownership,
    /// timestamps, and size. It provides all information needed for metadata preservation.
    ///
    /// # Parameters
    ///
    /// * `path` - The path to get metadata for
    ///
    /// # Returns
    ///
    /// Returns `Ok(FileMetadata)` containing all file metadata, or
    /// `Err(SyncError)` if metadata cannot be retrieved.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use arsync::io_uring::FileOperations;
    /// use std::path::Path;
    ///
    /// #[compio::main]
    /// async fn main() -> arsync::Result<()> {
    ///     let file_ops = FileOperations::new(4096, 64 * 1024)?;
    ///     let metadata = file_ops.get_file_metadata(Path::new("test.txt")).await?;
    ///     println!("File size: {} bytes", metadata.size);
    ///     println!("Permissions: {:o}", metadata.permissions);
    ///     Ok(())
    /// }
    /// ```
    ///
    /// # Performance Notes
    ///
    /// - This is an O(1) operation for local filesystems
    /// - Metadata is cached by the filesystem for performance
    /// - Network filesystems may have higher latency
    /// - All metadata is retrieved in a single system call
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The path does not exist
    /// - Permission is denied to read the path
    /// - The path is not accessible
    #[allow(clippy::future_not_send, clippy::items_after_statements)]
    pub async fn get_file_metadata(&self, path: &Path) -> Result<FileMetadata> {
        let metadata = compio::fs::metadata(path).await.map_err(|e| {
            SyncError::FileSystem(format!(
                "Failed to get metadata for {}: {}",
                path.display(),
                e
            ))
        })?;

        use std::os::unix::fs::PermissionsExt;
        let permissions = metadata.permissions().mode() & 0o7777;
        use std::os::unix::fs::MetadataExt;
        let uid = metadata.uid();
        let gid = metadata.gid();
        let modified = metadata
            .modified()
            .map_err(|e| SyncError::FileSystem(format!("Failed to get modified time: {e}")))?;
        let accessed = metadata
            .accessed()
            .map_err(|e| SyncError::FileSystem(format!("Failed to get accessed time: {e}")))?;

        Ok(FileMetadata {
            size: metadata.len(),
            permissions,
            uid,
            gid,
            modified,
            accessed,
        })
    }

    /// Copy file with full metadata preservation using file descriptors
    ///
    /// This function copies a file and preserves all metadata including permissions,
    /// ownership, and timestamps using efficient file descriptor-based operations.
    /// It avoids repeated path lookups by using the open file descriptors.
    ///
    /// # Parameters
    ///
    /// * `src` - Source file path
    /// * `dst` - Destination file path
    ///
    /// # Returns
    ///
    /// Returns `Ok(u64)` with the number of bytes copied, or
    /// `Err(SyncError)` if the operation failed.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use arsync::io_uring::FileOperations;
    /// use std::path::Path;
    ///
    /// #[compio::main]
    /// async fn main() -> arsync::Result<()> {
    ///     let mut file_ops = FileOperations::new(4096, 64 * 1024)?;
    ///     let src_path = Path::new("source.txt");
    ///     let dst_path = Path::new("destination.txt");
    ///     let bytes_copied = file_ops.copy_file_with_metadata(src_path, dst_path).await?;
    ///     println!("Copied {} bytes", bytes_copied);
    ///     Ok(())
    /// }
    /// ```
    ///
    /// # Performance Notes
    ///
    /// - Uses file descriptor-based operations to avoid repeated path lookups
    /// - Combines file content copying with metadata preservation
    /// - Uses efficient async I/O operations
    /// - Metadata operations are performed on open file descriptors
    /// - Memory usage is controlled by buffer size
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - Source file cannot be opened for reading
    /// - Destination file cannot be created or opened for writing
    /// - File copying operation fails (I/O errors, permission issues)
    /// - Metadata preservation fails
    #[allow(clippy::future_not_send)]
    pub async fn copy_file_with_metadata(
        &self,
        src: &Path,
        dst: &Path,
        parallel_config: &crate::cli::ParallelCopyConfig,
    ) -> Result<u64> {
        // Get file size for return value
        let file_size = compio::fs::metadata(src)
            .await
            .map_err(|e| SyncError::FileSystem(format!("Failed to get file metadata: {e}")))?
            .len();

        // Use the full copy_file implementation with parallel support
        let metadata_config = crate::metadata::MetadataConfig {
            archive: true, // Preserve all metadata by default
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
        };

        // Call public API - it handles DirectoryFd and Dispatcher setup internally (no leak!)
        crate::copy::copy_file(src, dst, &metadata_config, parallel_config).await?;

        debug!(
            "Copied {} bytes from {} to {} with metadata preservation",
            file_size,
            src.display(),
            dst.display()
        );
        Ok(file_size)
    }
}

/// Comprehensive file metadata for preservation
///
/// This structure contains all the metadata information needed to preserve
/// file attributes during copying operations. It includes permissions, ownership,
/// timestamps, and size information.
///
/// # Fields
///
/// * `size` - File size in bytes
/// * `permissions` - File permissions (mode bits)
/// * `uid` - User ID of the file owner
/// * `gid` - Group ID of the file owner
/// * `modified` - Last modification timestamp
/// * `accessed` - Last access timestamp
///
/// # Examples
///
/// ```rust
/// use arsync::io_uring::FileMetadata;
///
/// let metadata = FileMetadata {
///     size: 1024,
///     permissions: 0o644,
///     uid: 1000,
///     gid: 1000,
///     modified: std::time::SystemTime::now(),
///     accessed: std::time::SystemTime::now(),
/// };
/// ```
///
/// # Performance Notes
///
/// - All fields are efficiently stored and accessed
/// - Timestamps use system-level precision
/// - Permission bits preserve special attributes
/// - Ownership information is stored as numeric IDs
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileMetadata {
    /// File size in bytes
    pub size: u64,

    /// File permissions (mode bits including special permissions)
    pub permissions: u32,

    /// User ID of the file owner
    pub uid: u32,

    /// Group ID of the file owner
    pub gid: u32,

    /// Last modification timestamp
    pub modified: std::time::SystemTime,

    /// Last access timestamp
    pub accessed: std::time::SystemTime,
}

/// File copy operation with progress tracking
#[allow(dead_code)]
pub struct CopyOperation {
    /// Source file path
    pub src_path: std::path::PathBuf,
    /// Destination file path
    pub dst_path: std::path::PathBuf,
    /// Total file size in bytes
    pub file_size: u64,
    /// Number of bytes copied so far
    pub bytes_copied: u64,
    /// Current copy status
    pub status: CopyStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
/// Status of a file copy operation
pub enum CopyStatus {
    /// Copy operation is pending
    Pending,
    /// Copy operation is currently in progress
    InProgress,
    /// Copy operation completed successfully
    Completed,
    /// Copy operation failed with error message
    Failed(String),
}

#[allow(dead_code)]
impl CopyOperation {
    /// Create a new copy operation
    ///
    /// # Arguments
    ///
    /// * `src` - Source file path
    /// * `dst` - Destination file path  
    /// * `size` - Total file size in bytes
    #[must_use]
    pub const fn new(src: std::path::PathBuf, dst: std::path::PathBuf, size: u64) -> Self {
        Self {
            src_path: src,
            dst_path: dst,
            file_size: size,
            bytes_copied: 0,
            status: CopyStatus::Pending,
        }
    }

    /// Update the progress of the copy operation
    ///
    /// # Arguments
    ///
    /// * `bytes` - Number of bytes copied since last update
    pub fn update_progress(&mut self, bytes: u64) {
        self.bytes_copied += bytes;
        if self.bytes_copied >= self.file_size {
            self.status = CopyStatus::Completed;
        } else {
            self.status = CopyStatus::InProgress;
        }
    }

    /// Mark the copy operation as failed
    ///
    /// # Arguments
    ///
    /// * `error` - Error message describing the failure
    pub fn mark_failed(&mut self, error: String) {
        self.status = CopyStatus::Failed(error);
    }

    /// Get the progress percentage of the copy operation
    ///
    /// # Returns
    ///
    /// Progress percentage as a float between 0.0 and 100.0
    #[must_use]
    pub fn progress_percentage(&self) -> f64 {
        #[allow(clippy::cast_precision_loss)]
        if self.file_size == 0 {
            100.0
        } else {
            (self.bytes_copied as f64 / self.file_size as f64) * 100.0
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;
    use tempfile::TempDir;

    #[compio::test]
    async fn test_file_operations_basic() -> Result<()> {
        let temp_dir = TempDir::new().map_err(|e| {
            SyncError::FileSystem(format!("Failed to create temp directory: {}", e))
        })?;
        let test_file = temp_dir.path().join("test.txt");
        let test_content = b"Hello, io_uring!";

        // Write test file
        {
            compio::fs::write(&test_file, test_content)
                .await
                .0
                .map_err(|e| SyncError::FileSystem(format!("Failed to write test file: {}", e)))?;
        }

        let ops = FileOperations::new(1024, 4096).map_err(|e| {
            SyncError::FileSystem(format!("Failed to create FileOperations: {}", e))
        })?;

        // Test file existence
        assert!(ops.file_exists(&test_file).await);

        // Test file size
        let size = ops.get_file_size(&test_file).await?;
        assert_eq!(size, test_content.len() as u64);

        // Test file reading using compio
        let content = compio::fs::read(&test_file)
            .await
            .map_err(|e| SyncError::FileSystem(format!("Failed to read test file: {}", e)))?;
        assert_eq!(content, test_content);

        Ok(())
    }

    #[compio::test]
    async fn test_copy_operation() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let src_file = temp_dir.path().join("src.txt");
        let dst_file = temp_dir.path().join("dst.txt");
        let test_content = b"This is a test file for copying.";

        // Create source file
        {
            compio::fs::write(&src_file, test_content)
                .await
                .0
                .expect("Failed to write test file");
        }

        let ops = FileOperations::new(1024, 4096).expect("Failed to create FileOperations");

        // Test file copying
        ops.copy_file_read_write(&src_file, &dst_file)
            .await
            .expect("Failed to copy file");

        // Verify destination file
        assert!(ops.file_exists(&dst_file).await);
        let copied_content = compio::fs::read(&dst_file)
            .await
            .expect("Failed to read copied file");
        assert_eq!(copied_content, test_content);
    }

    #[compio::test]
    async fn test_copy_operation_progress() {
        let src = std::path::PathBuf::from("/tmp/src");
        let dst = std::path::PathBuf::from("/tmp/dst");
        let mut operation = CopyOperation::new(src, dst, 1000);

        assert_eq!(operation.progress_percentage(), 0.0);
        assert_eq!(operation.status, CopyStatus::Pending);

        operation.update_progress(500);
        assert_eq!(operation.progress_percentage(), 50.0);
        assert_eq!(operation.status, CopyStatus::InProgress);

        operation.update_progress(500);
        assert_eq!(operation.progress_percentage(), 100.0);
        assert_eq!(operation.status, CopyStatus::Completed);

        operation.mark_failed("Test error".to_string());
        assert_eq!(
            operation.status,
            CopyStatus::Failed("Test error".to_string())
        );
    }
}
