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

    // Mock file that simulates short reads (always returns less data than requested)
    struct ShortReadMockFile {
        content: Vec<u8>,
    }

    impl AsyncFile for ShortReadMockFile {
        type Metadata = MockMetadata;

        async fn read_at(&self, mut buf: Vec<u8>, offset: u64) -> Result<(usize, Vec<u8>)> {
            let start = offset as usize;
            if start >= self.content.len() {
                return Ok((0, buf)); // EOF
            }

            // Simulate short read: return at most half the buffer size or remaining data
            let requested = buf.len();
            let available = self.content.len() - start;
            let to_read = std::cmp::min(requested / 2, available).max(1); // At least 1 byte if available

            buf[..to_read].copy_from_slice(&self.content[start..start + to_read]);
            Ok((to_read, buf))
        }

        async fn write_at(&self, buf: Vec<u8>, _offset: u64) -> Result<(usize, Vec<u8>)> {
            Ok((buf.len(), buf))
        }

        async fn sync_all(&self) -> Result<()> {
            Ok(())
        }

        async fn metadata(&self) -> Result<Self::Metadata> {
            Ok(MockMetadata {
                size: self.content.len() as u64,
            })
        }

        async fn copy_file_range(
            &self,
            _dst: &mut Self,
            _src_offset: u64,
            _dst_offset: u64,
            _len: u64,
        ) -> Result<u64> {
            unimplemented!()
        }
    }

    #[compio::test]
    async fn test_streaming_read_with_short_reads() {
        // Create file with 100 bytes
        let content: Vec<u8> = (0..100).collect();
        let file = ShortReadMockFile {
            content: content.clone(),
        };

        // Test the recommended streaming pattern with short reads
        // This demonstrates correct usage when read_at() returns less than requested
        let mut result = Vec::new();
        let mut offset = 0;
        let mut buffer = vec![0u8; 64]; // Request 64 bytes at a time

        loop {
            let (n, buf) = file.read_at(buffer, offset).await.unwrap();
            if n == 0 {
                break; // EOF
            }

            // Process the data we got (in real code, would write it)
            result.extend_from_slice(&buf[..n]);

            buffer = buf; // Reuse buffer
            offset += n as u64;
        }

        // Verify we read all data correctly despite short reads
        assert_eq!(result, content);
        assert_eq!(result.len(), 100);
    }

    // Mock destination file that tracks what data was written where
    struct MockDestFile {
        written_data: std::sync::Arc<std::sync::Mutex<std::collections::BTreeMap<u64, Vec<u8>>>>,
    }

    impl AsyncFile for MockDestFile {
        type Metadata = MockMetadata;

        async fn read_at(&self, buf: Vec<u8>, _offset: u64) -> Result<(usize, Vec<u8>)> {
            Ok((0, buf))
        }

        async fn write_at(&self, buf: Vec<u8>, offset: u64) -> Result<(usize, Vec<u8>)> {
            // Write all the data (no partial writes for this test)
            let len = buf.len();
            self.written_data
                .lock()
                .unwrap()
                .insert(offset, buf[..len].to_vec());
            Ok((len, buf))
        }

        async fn sync_all(&self) -> Result<()> {
            Ok(())
        }

        async fn metadata(&self) -> Result<Self::Metadata> {
            Ok(MockMetadata { size: 0 })
        }

        async fn copy_file_range(
            &self,
            _dst: &mut Self,
            _src_offset: u64,
            _dst_offset: u64,
            _len: u64,
        ) -> Result<u64> {
            unimplemented!()
        }
    }

    #[compio::test]
    async fn test_copy_loop_with_short_reads() {
        // Create source file with known data
        let source_data: Vec<u8> = (0..200).collect(); // 200 bytes
        let source = ShortReadMockFile {
            content: source_data.clone(),
        };

        // Create destination file that tracks writes
        let written_data = std::sync::Arc::new(std::sync::Mutex::new(std::collections::BTreeMap::new()));
        let dest = MockDestFile {
            written_data: written_data.clone(),
        };

        // Implement the recommended copy pattern with streaming
        // This is the pattern we document - it must work correctly with short reads
        let mut offset = 0;
        let mut buffer = vec![0u8; 64]; // Request 64 bytes at a time

        loop {
            let (n, buf) = source.read_at(buffer, offset).await.unwrap();
            if n == 0 {
                break; // EOF
            }

            // Write what we read (using write_all_at to handle partial writes)
            dest.write_all_at(&buf[..n], offset).await.unwrap();

            buffer = buf; // Reuse buffer
            offset += n as u64;
        }

        // THE CRITICAL TEST: Reconstruct destination data and verify it matches source
        let writes = written_data.lock().unwrap();
        let mut reconstructed = Vec::new();
        
        // BTreeMap keeps writes sorted by offset
        let mut expected_offset = 0u64;
        for (offset, chunk) in writes.iter() {
            assert_eq!(*offset, expected_offset, 
                "Write at offset {} but expected {}", offset, expected_offset);
            reconstructed.extend_from_slice(chunk);
            expected_offset += chunk.len() as u64;
        }

        // Verify the copied data matches the source exactly
        assert_eq!(reconstructed, source_data, 
            "Copied data must match source data exactly, even with short reads");
        
        // Verify we copied all 200 bytes
        assert_eq!(reconstructed.len(), 200);
        
        // Verify multiple read operations occurred (due to short reads)
        // With 64-byte buffer and 50% short reads, should take many iterations
        assert!(writes.len() > 3, 
            "Expected multiple write operations due to short reads, got {}", 
            writes.len());
    }

    // Mock file that simulates partial writes and tracks actual data written
    struct PartialWriteMockFile {
        written_data: std::sync::Arc<std::sync::Mutex<Vec<(u64, Vec<u8>)>>>, // (offset, data)
    }

    impl AsyncFile for PartialWriteMockFile {
        type Metadata = MockMetadata;

        async fn read_at(&self, buf: Vec<u8>, _offset: u64) -> Result<(usize, Vec<u8>)> {
            Ok((0, buf))
        }

        async fn write_at(&self, buf: Vec<u8>, offset: u64) -> Result<(usize, Vec<u8>)> {
            // Simulate partial write: only write half the data (but at least 1 byte)
            let requested = buf.len();
            let written = (requested / 2).max(1);
            
            // Track the actual data written (first 'written' bytes of buffer)
            let data_written = buf[..written].to_vec();
            self.written_data
                .lock()
                .unwrap()
                .push((offset, data_written));
            
            Ok((written, buf))
        }

        async fn sync_all(&self) -> Result<()> {
            Ok(())
        }

        async fn metadata(&self) -> Result<Self::Metadata> {
            Ok(MockMetadata { size: 0 })
        }

        async fn copy_file_range(
            &self,
            _dst: &mut Self,
            _src_offset: u64,
            _dst_offset: u64,
            _len: u64,
        ) -> Result<u64> {
            unimplemented!()
        }
    }

    #[compio::test]
    async fn test_write_all_at_with_partial_writes() {
        let written_data = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let file = PartialWriteMockFile {
            written_data: written_data.clone(),
        };

        // Test write_all_at with data that will require multiple partial writes
        let original_data = b"Hello, World! This is a test of partial writes.";
        let start_offset = 100u64;

        // write_all_at should handle partial writes automatically by looping
        file.write_all_at(original_data, start_offset).await.unwrap();

        // Verify that multiple write_at calls occurred due to partial writes
        let writes = written_data.lock().unwrap();
        
        // Should have made multiple calls since each write only writes half
        assert!(writes.len() > 1, "Expected multiple write calls due to partial writes, got {}", writes.len());
        
        // Reconstruct the data that was actually written by concatenating all chunks
        let mut reconstructed = Vec::new();
        let mut expected_offset = start_offset;
        
        for (i, (offset, data)) in writes.iter().enumerate() {
            // Verify offset advances correctly
            assert_eq!(*offset, expected_offset, 
                "Call {}: offset should be {}, got {}", 
                i, expected_offset, offset);
            
            // Append this chunk to reconstructed data
            reconstructed.extend_from_slice(data);
            
            // Next write should be at this offset + bytes written
            expected_offset += data.len() as u64;
        }
        
        // THE CRITICAL TEST: Verify the reconstructed data matches the original
        assert_eq!(reconstructed, original_data, 
            "Reconstructed data from partial writes must match original data exactly");
        
        // Also verify each individual chunk is correct
        let mut data_position = 0;
        for (i, (_offset, chunk)) in writes.iter().enumerate() {
            let chunk_len = chunk.len();
            let expected_chunk = &original_data[data_position..data_position + chunk_len];
            assert_eq!(chunk.as_slice(), expected_chunk,
                "Call {}: chunk data should match original data at position {}", 
                i, data_position);
            data_position += chunk_len;
        }
    }

    #[compio::test]
    async fn test_write_all_at_zero_write_error() {
        // Mock that returns 0 bytes written (write failure)
        struct ZeroWriteMockFile;

        impl AsyncFile for ZeroWriteMockFile {
            type Metadata = MockMetadata;

            async fn read_at(&self, buf: Vec<u8>, _offset: u64) -> Result<(usize, Vec<u8>)> {
                Ok((0, buf))
            }

            async fn write_at(&self, buf: Vec<u8>, _offset: u64) -> Result<(usize, Vec<u8>)> {
                Ok((0, buf)) // Simulate write failure (0 bytes written)
            }

            async fn sync_all(&self) -> Result<()> {
                Ok(())
            }

            async fn metadata(&self) -> Result<Self::Metadata> {
                Ok(MockMetadata { size: 0 })
            }

            async fn copy_file_range(
                &self,
                _dst: &mut Self,
                _src_offset: u64,
                _dst_offset: u64,
                _len: u64,
            ) -> Result<u64> {
                unimplemented!()
            }
        }

        let file = ZeroWriteMockFile;
        let result = file.write_all_at(b"test", 0).await;

        // Should error when write_at returns 0
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("no bytes written"));
    }
}
