//! AsyncFile trait for unified file operations
//!
//! This trait provides a unified interface for file operations that can be
//! implemented by both local filesystem backends and remote protocol backends.

use crate::error::Result;
use super::AsyncMetadata;

/// Unified file interface for both local and remote operations
///
/// This trait provides a consistent interface for file operations regardless
/// of whether the underlying storage is local (using compio-fs-extended) or remote
/// (using protocol transports like SSH, rsync, etc.).
///
/// # Type Parameters
///
/// * `Metadata` - The metadata type for this file
///
/// # Examples
///
/// ```rust,ignore
/// // Read from file
/// let file = filesystem.open_file(path).await?;
/// let (bytes_read, buffer) = file.read_at(buffer, 0).await?;
///
/// // Write to file
/// let file = filesystem.create_file(path).await?;
/// let (bytes_written, buffer) = file.write_at(buffer, 0).await?;
/// ```
pub trait AsyncFile: Send + Sync + 'static {
    /// The metadata type for this file
    type Metadata: AsyncMetadata;

    /// Read data from the file at a specific offset
    ///
    /// # Parameters
    ///
    /// * `buf` - Buffer to read data into (compio takes ownership)
    /// * `offset` - Offset in the file to read from
    ///
    /// # Returns
    ///
    /// Returns `Ok((bytes_read, buffer))` where:
    /// - `bytes_read` is the number of bytes actually read
    /// - `buffer` is the buffer returned by compio (may be different from input)
    ///
    /// Returns `Err(SyncError)` if:
    /// - The file is not open for reading
    /// - The offset is beyond the end of file
    /// - I/O error occurs
    async fn read_at(&self, buf: Vec<u8>, offset: u64) -> Result<(usize, Vec<u8>)>;

    /// Write data to the file at a specific offset
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
    /// - `buffer` is the buffer returned by compio (may be different from input)
    ///
    /// Returns `Err(SyncError)` if:
    /// - The file is not open for writing
    /// - I/O error occurs
    async fn write_at(&self, buf: Vec<u8>, offset: u64) -> Result<(usize, Vec<u8>)>;

    /// Write all data to the file at a specific offset
    ///
    /// This is a convenience method that ensures all data is written.
    ///
    /// # Parameters
    ///
    /// * `buf` - Buffer containing data to write (compio takes ownership)
    /// * `offset` - Offset in the file to write to
    ///
    /// # Returns
    ///
    /// Returns `Ok(((), buffer))` if all data is written successfully, or `Err(SyncError)` if:
    /// - The file is not open for writing
    /// - I/O error occurs
    async fn write_all_at(&self, buf: Vec<u8>, offset: u64) -> Result<((), Vec<u8>)> {
        let mut remaining = buf;
        let mut current_offset = offset;
        
        while !remaining.is_empty() {
            let (bytes_written, returned_buffer) = self.write_at(remaining, current_offset).await?;
            remaining = returned_buffer;
            
            if bytes_written == 0 {
                return Err(crate::error::SyncError::FileSystem(
                    "Failed to write all data: no bytes written".to_string()
                ));
            }
            
            remaining = remaining.split_off(bytes_written);
            current_offset += bytes_written as u64;
        }
        
        Ok(((), remaining))
    }

    /// Sync all pending data to storage
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if sync is successful, or `Err(SyncError)` if:
    /// - I/O error occurs during sync
    async fn sync_all(&self) -> Result<()>;

    /// Get file metadata
    ///
    /// # Returns
    ///
    /// Returns `Ok(Metadata)` if metadata can be retrieved, or `Err(SyncError)` if:
    /// - I/O error occurs
    async fn metadata(&self) -> Result<Self::Metadata>;

    /// Copy data from this file to another file using copy_file_range
    ///
    /// This is the most efficient way to copy data between files on the same filesystem.
    ///
    /// # Parameters
    ///
    /// * `dst` - Destination file
    /// * `src_offset` - Offset in source file to start copying from
    /// * `dst_offset` - Offset in destination file to start copying to
    /// * `len` - Maximum number of bytes to copy (0 = copy to end of file)
    ///
    /// # Returns
    ///
    /// Returns `Ok(bytes_copied)` if copy is successful, or `Err(SyncError)` if:
    /// - copy_file_range is not supported
    /// - I/O error occurs
    async fn copy_file_range(
        &self,
        dst: &mut Self,
        src_offset: u64,
        dst_offset: u64,
        len: u64,
    ) -> Result<u64>;

    /// Get the file size
    ///
    /// # Returns
    ///
    /// Returns `Ok(size)` if size can be retrieved, or `Err(SyncError)` if:
    /// - I/O error occurs
    async fn size(&self) -> Result<u64> {
        let metadata = self.metadata().await?;
        Ok(metadata.size())
    }

    /// Check if the file is empty
    ///
    /// # Returns
    ///
    /// Returns `true` if the file is empty, `false` otherwise.
    async fn is_empty(&self) -> Result<bool> {
        Ok(self.size().await? == 0)
    }

    /// Read the entire file into memory
    ///
    /// # Returns
    ///
    /// Returns `Ok(data)` if the file can be read completely, or `Err(SyncError)` if:
    /// - The file is too large to fit in memory
    /// - I/O error occurs
    async fn read_to_end(&self) -> Result<Vec<u8>> {
        let size = self.size().await?;
        if size > usize::MAX as u64 {
            return Err(crate::error::SyncError::FileSystem(
                "File too large to read into memory".to_string()
            ));
        }
        
        let mut buffer = vec![0u8; size as usize];
        let (bytes_read, _) = self.read_at(buffer, 0).await?;
        buffer.truncate(bytes_read);
        Ok(buffer)
    }

    /// Write data to the file, replacing all existing content
    ///
    /// # Parameters
    ///
    /// * `data` - Data to write to the file
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if write is successful, or `Err(SyncError)` if:
    /// - I/O error occurs
    async fn write_all(&self, data: &[u8]) -> Result<()> {
        let buffer = data.to_vec();
        let (_, _) = self.write_all_at(buffer, 0).await?;
        Ok(())
    }
}