//! AsyncFile trait for unified file operations
//!
//! This trait provides a unified interface for file operations that can be
//! implemented by both local filesystem backends and remote protocol backends.
//!
//! See `docs/projects/trait-filesystem-abstraction/design.md` for architecture.

use super::AsyncMetadata;
use crate::error::Result;

/// Unified file interface for both local and remote operations
///
/// This trait provides a consistent interface for file operations regardless
/// of whether the underlying storage is local (using compio-fs-extended) or remote
/// (using protocol transports like SSH, rsync, etc.).
///
/// # Design
///
/// Uses compio's buffer ownership pattern where buffers are passed by value
/// and returned, enabling zero-copy I/O with io_uring.
///
/// # Type Parameters
///
/// * `Metadata` - The metadata type for this file
///
/// # Examples
///
/// ```rust,ignore
/// use arsync::traits::AsyncFile;
///
/// // Read from file
/// let file = filesystem.open_file(path).await?;
/// let buffer = vec![0u8; 4096];
/// let (bytes_read, buffer) = file.read_at(buffer, 0).await?;
///
/// // Write to file
/// let file = filesystem.create_file(path).await?;
/// let data = b"Hello, World!".to_vec();
/// let (bytes_written, buffer) = file.write_at(data, 0).await?;
/// ```
#[allow(async_fn_in_trait)] // Intentional design for compio-style async I/O
#[allow(dead_code)] // Will be used in PR #4 (file wrapper)
pub trait AsyncFile: Send + Sync + 'static {
    /// The metadata type for this file
    type Metadata: AsyncMetadata;

    /// Read data from the file at a specific offset
    ///
    /// Uses compio's buffer ownership pattern for zero-copy I/O.
    ///
    /// # Parameters
    ///
    /// * `buf` - Buffer to read data into (compio takes ownership)
    /// * `offset` - Offset in the file to read from
    ///
    /// # Returns
    ///
    /// Returns `Ok((bytes_read, buffer))` where:
    /// - `bytes_read` is the number of bytes actually read (0 at EOF)
    /// - `buffer` is the buffer returned by compio (caller can reuse)
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The file is not open for reading
    /// - I/O error occurs
    async fn read_at(&self, buf: Vec<u8>, offset: u64) -> Result<(usize, Vec<u8>)>;

    /// Write data to the file at a specific offset
    ///
    /// Uses compio's buffer ownership pattern for zero-copy I/O.
    ///
    /// # Parameters
    ///
    /// * `buf` - Buffer containing data to write (compio takes ownership)
    /// * `offset` - Offset in the file to write to
    ///
    /// # Returns
    ///
    /// Returns `Ok((bytes_written, buffer))` where:
    /// - `bytes_written` is the number of bytes actually written
    /// - `buffer` is the buffer returned by compio (caller can reuse)
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The file is not open for writing
    /// - I/O error occurs
    async fn write_at(&self, buf: Vec<u8>, offset: u64) -> Result<(usize, Vec<u8>)>;

    /// Sync all pending data to storage
    ///
    /// Ensures all written data is persisted to disk.
    ///
    /// # Errors
    ///
    /// Returns an error if I/O error occurs during sync.
    async fn sync_all(&self) -> Result<()>;

    /// Get file metadata
    ///
    /// # Returns
    ///
    /// Returns metadata for this file.
    ///
    /// # Errors
    ///
    /// Returns an error if I/O error occurs.
    async fn metadata(&self) -> Result<Self::Metadata>;

    /// Copy data from this file to another file using copy_file_range
    ///
    /// This is the most efficient way to copy data between files on the same filesystem.
    /// Implementations should use the OS's zero-copy mechanism (e.g., copy_file_range on Linux).
    ///
    /// # Parameters
    ///
    /// * `dst` - Destination file
    /// * `src_offset` - Offset in source file to start copying from
    /// * `dst_offset` - Offset in destination file to start copying to
    /// * `len` - Number of bytes to copy
    ///
    /// # Returns
    ///
    /// Returns the number of bytes actually copied.
    ///
    /// # Errors
    ///
    /// Returns an error if I/O error occurs.
    async fn copy_file_range(
        &self,
        dst: &mut Self,
        src_offset: u64,
        dst_offset: u64,
        len: u64,
    ) -> Result<u64>;

    // =========================================================================
    // Provided methods with default implementations
    // =========================================================================

    /// Get the file size
    ///
    /// Convenience method that gets metadata and returns size.
    ///
    /// # Errors
    ///
    /// Returns an error if metadata cannot be retrieved.
    async fn size(&self) -> Result<u64> {
        let metadata = self.metadata().await?;
        Ok(metadata.size())
    }

    /// Write data to file at specific offset, ensuring all bytes are written
    ///
    /// Unlike `write_at()` which may perform partial writes, this method ensures
    /// all data is written by looping until complete.
    ///
    /// # Design Rationale
    ///
    /// **Why `write_all_at()` is included but `read_all()` is NOT:**
    ///
    /// - **write_all_at()**: Once you have data to write, you MUST write all of it
    ///   before moving on. Partial writes would corrupt the file. This is a critical
    ///   correctness requirement for copy/sync operations.
    ///
    /// - **read_all()**: NOT needed because you can process data as you read it in
    ///   chunks (streaming). For copy operations, you read a chunk and immediately
    ///   write it - no need to load the entire file into memory. Streaming is more
    ///   efficient and works for files of any size.
    ///
    /// **Usage pattern (correct streaming approach):**
    /// ```rust,ignore
    /// let mut offset = 0;
    /// let mut buffer = vec![0u8; 64 * 1024]; // Reusable buffer
    /// loop {
    ///     let (n, buf) = src.read_at(buffer, offset).await?;
    ///     if n == 0 { break; } // EOF
    ///     
    ///     dst.write_all_at(&buf[..n], offset).await?; // Must write ALL data
    ///     buffer = buf; // Reuse buffer
    ///     offset += n as u64;
    /// }
    /// ```
    ///
    /// # Parameters
    ///
    /// * `data` - Data to write (will be copied into owned buffer)
    /// * `offset` - Offset in file to write at
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - I/O error occurs
    /// - write_at() returns 0 (indicates write failure)
    async fn write_all_at(&self, data: &[u8], offset: u64) -> Result<()> {
        let mut buffer = data.to_vec();
        let mut current_offset = offset;

        while !buffer.is_empty() {
            let (bytes_written, returned_buffer) = self.write_at(buffer, current_offset).await?;

            if bytes_written == 0 {
                return Err(crate::error::SyncError::FileSystem(
                    "Failed to write data: no bytes written".to_string(),
                ));
            }

            buffer = returned_buffer;
            buffer.drain(..bytes_written);
            current_offset += bytes_written as u64;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    // Mock implementations for testing provided methods
    #[allow(dead_code)] // Test fixture
    struct MockFile {
        content: Vec<u8>,
        metadata: MockMetadata,
    }

    #[allow(dead_code)] // Test fixture
    struct MockMetadata {
        size: u64,
    }

    impl AsyncMetadata for MockMetadata {
        fn size(&self) -> u64 {
            self.size
        }
        fn is_file(&self) -> bool {
            true
        }
        fn is_dir(&self) -> bool {
            false
        }
        fn is_symlink(&self) -> bool {
            false
        }
        fn permissions(&self) -> u32 {
            0o644
        }
        fn uid(&self) -> u32 {
            1000
        }
        fn gid(&self) -> u32 {
            1000
        }
        fn modified(&self) -> SystemTime {
            SystemTime::now()
        }
        fn accessed(&self) -> SystemTime {
            SystemTime::now()
        }
        fn device_id(&self) -> u64 {
            1
        }
        fn inode_number(&self) -> u64 {
            12345
        }
        fn link_count(&self) -> u64 {
            1
        }
    }

    impl AsyncFile for MockFile {
        type Metadata = MockMetadata;

        async fn read_at(&self, mut buf: Vec<u8>, offset: u64) -> Result<(usize, Vec<u8>)> {
            let start = offset as usize;
            let end = std::cmp::min(start + buf.len(), self.content.len());

            if start >= self.content.len() {
                return Ok((0, buf));
            }

            let to_copy = end - start;
            buf[..to_copy].copy_from_slice(&self.content[start..end]);
            Ok((to_copy, buf))
        }

        #[allow(clippy::unimplemented)]
        async fn write_at(&self, _buf: Vec<u8>, _offset: u64) -> Result<(usize, Vec<u8>)> {
            unimplemented!("write_at not needed for provided method tests")
        }

        async fn sync_all(&self) -> Result<()> {
            Ok(())
        }

        async fn metadata(&self) -> Result<Self::Metadata> {
            // Return stored metadata, not derived from content
            // This correctly mimics real file behavior where metadata is stored separately
            Ok(MockMetadata {
                size: self.metadata.size,
            })
        }

        #[allow(clippy::unimplemented)]
        async fn copy_file_range(
            &self,
            _dst: &mut Self,
            _src_offset: u64,
            _dst_offset: u64,
            _len: u64,
        ) -> Result<u64> {
            unimplemented!("copy_file_range not needed for provided method tests")
        }
    }

    #[compio::test]
    async fn test_size_provided_method() {
        let file = MockFile {
            content: vec![1, 2, 3, 4, 5],
            metadata: MockMetadata { size: 5 },
        };

        let size = file.size().await.unwrap();
        assert_eq!(size, 5);
    }
}
