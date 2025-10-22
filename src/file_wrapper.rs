//! Wrapper around compio::fs::File that implements AsyncFile trait
//!
//! This module provides `AsyncFileWrapper`, which wraps a `compio::fs::File`
//! and implements the `AsyncFile` trait for use with the trait-based
//! filesystem abstraction.

use crate::error::{Result, SyncError};
use crate::traits::AsyncFile;
use compio::fs::File;
use compio::io::{AsyncReadAt, AsyncWriteAt};

/// Wrapper around compio::fs::File that implements AsyncFile trait
///
/// This wrapper enables using compio files through the AsyncFile trait interface,
/// allowing uniform handling of local and remote file operations.
///
/// # Examples
///
/// ```rust,ignore
/// use arsync::file_wrapper::AsyncFileWrapper;
/// use arsync::traits::AsyncFile;
/// use compio::fs::File;
///
/// let file = File::open("example.txt").await?;
/// let wrapper = AsyncFileWrapper::new(file);
///
/// // Use via trait
/// let buffer = vec![0u8; 4096];
/// let (bytes_read, buffer) = wrapper.read_at(buffer, 0).await?;
/// ```
pub struct AsyncFileWrapper {
    /// The underlying compio file
    file: File,
}

impl AsyncFileWrapper {
    /// Create a new wrapper around a compio file
    ///
    /// # Parameters
    ///
    /// * `file` - The compio file to wrap
    ///
    /// # Returns
    ///
    /// Returns a new `AsyncFileWrapper` instance
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let file = File::open("data.bin").await?;
    /// let wrapper = AsyncFileWrapper::new(file);
    /// ```
    #[must_use]
    pub fn new(file: File) -> Self {
        Self { file }
    }

    /// Get a reference to the underlying file
    ///
    /// # Returns
    ///
    /// Returns a reference to the wrapped `compio::fs::File`
    #[must_use]
    pub fn inner(&self) -> &File {
        &self.file
    }

    /// Consume the wrapper and return the underlying file
    ///
    /// # Returns
    ///
    /// Returns the wrapped `compio::fs::File`
    #[must_use]
    pub fn into_inner(self) -> File {
        self.file
    }
}

impl AsyncFile for AsyncFileWrapper {
    type Metadata = compio_fs_extended::FileMetadata;

    async fn read_at(&self, buf: Vec<u8>, offset: u64) -> Result<(usize, Vec<u8>)> {
        let buf_result = self.file.read_at(buf, offset).await;
        let bytes_read = buf_result.0.map_err(|e| {
            SyncError::FileSystem(format!("Failed to read from file at offset {offset}: {e}"))
        })?;
        Ok((bytes_read, buf_result.1))
    }

    async fn write_at(&mut self, buf: Vec<u8>, offset: u64) -> Result<(usize, Vec<u8>)> {
        let buf_result = self.file.write_at(buf, offset).await;
        let bytes_written = buf_result.0.map_err(|e| {
            SyncError::FileSystem(format!("Failed to write to file at offset {offset}: {e}"))
        })?;
        Ok((bytes_written, buf_result.1))
    }

    async fn sync_all(&self) -> Result<()> {
        self.file
            .sync_all()
            .await
            .map_err(|e| SyncError::FileSystem(format!("Failed to sync file: {e}")))
    }

    async fn metadata(&self) -> Result<Self::Metadata> {
        use std::os::unix::fs::MetadataExt;
        use std::time::SystemTime;

        // Get standard metadata and convert to FileMetadata
        let m = self
            .file
            .metadata()
            .await
            .map_err(|e| SyncError::FileSystem(format!("Failed to get file metadata: {e}")))?;

        // Convert to FileMetadata
        Ok(compio_fs_extended::FileMetadata {
            size: m.len(),
            mode: m.mode(),
            uid: m.uid(),
            gid: m.gid(),
            nlink: m.nlink(),
            ino: m.ino(),
            dev: m.dev(),
            accessed: m.accessed().unwrap_or(SystemTime::UNIX_EPOCH),
            modified: m.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            created: m.created().ok(),
            flags: None,      // Not available from standard metadata
            generation: None, // Not available from standard metadata
        })
    }

    async fn copy_file_range(
        &self,
        _target: &mut Self,
        _offset: u64,
        _target_offset: u64,
        _len: u64,
    ) -> Result<u64> {
        // Note: copy_file_range is a Linux-specific syscall not directly exposed by compio
        // For now, we'll return an error indicating it's not implemented
        // In a full implementation, this would use copy_file_range syscall on Linux
        // or fall back to read/write loop on other platforms
        Err(SyncError::FileSystem(
            "copy_file_range not yet implemented for AsyncFileWrapper".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::AsyncMetadata;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[compio::test]
    async fn test_wrapper_read_at() -> anyhow::Result<()> {
        // Create a temp file with known content
        let mut temp_file = NamedTempFile::new()?;
        temp_file.write_all(b"Hello, World!")?;
        temp_file.flush()?;

        let file = File::open(temp_file.path()).await?;
        let wrapper = AsyncFileWrapper::new(file);

        // Read from offset 0
        let buffer = vec![0u8; 5];
        let (bytes_read, buffer) = wrapper.read_at(buffer, 0).await?;

        assert_eq!(bytes_read, 5);
        assert_eq!(&buffer[..bytes_read], b"Hello");

        Ok(())
    }

    #[compio::test]
    async fn test_wrapper_read_at_offset() -> anyhow::Result<()> {
        // Create a temp file with known content
        let mut temp_file = NamedTempFile::new()?;
        temp_file.write_all(b"Hello, World!")?;
        temp_file.flush()?;

        let file = File::open(temp_file.path()).await?;
        let wrapper = AsyncFileWrapper::new(file);

        // Read from offset 7
        let buffer = vec![0u8; 6];
        let (bytes_read, buffer) = wrapper.read_at(buffer, 7).await?;

        assert_eq!(bytes_read, 6);
        assert_eq!(&buffer[..bytes_read], b"World!");

        Ok(())
    }

    #[compio::test]
    async fn test_wrapper_write_at() -> anyhow::Result<()> {
        // Create a temp file
        let temp_file = NamedTempFile::new()?;

        let file = File::create(temp_file.path()).await?;
        let mut wrapper = AsyncFileWrapper::new(file);

        // Write data
        let data = b"Test data".to_vec();
        let (bytes_written, _) = wrapper.write_at(data, 0).await?;

        assert_eq!(bytes_written, 9);

        // Verify by reading back
        let file = File::open(temp_file.path()).await?;
        let wrapper = AsyncFileWrapper::new(file);

        let buffer = vec![0u8; 9];
        let (bytes_read, buffer) = wrapper.read_at(buffer, 0).await?;

        assert_eq!(bytes_read, 9);
        assert_eq!(&buffer[..bytes_read], b"Test data");

        Ok(())
    }

    #[compio::test]
    async fn test_wrapper_metadata() -> anyhow::Result<()> {
        // Create a temp file with known size
        let mut temp_file = NamedTempFile::new()?;
        temp_file.write_all(b"1234567890")?;
        temp_file.flush()?;

        let file = File::open(temp_file.path()).await?;
        let wrapper = AsyncFileWrapper::new(file);

        // Get metadata
        let metadata = wrapper.metadata().await?;

        assert_eq!(metadata.size(), 10);
        assert!(metadata.is_file());
        assert!(!metadata.is_dir());

        Ok(())
    }

    #[compio::test]
    async fn test_wrapper_size_convenience() -> anyhow::Result<()> {
        // Create a temp file
        let mut temp_file = NamedTempFile::new()?;
        temp_file.write_all(b"12345")?;
        temp_file.flush()?;

        let file = File::open(temp_file.path()).await?;
        let wrapper = AsyncFileWrapper::new(file);

        // Use convenience method
        let size = wrapper.size().await?;

        assert_eq!(size, 5);

        Ok(())
    }

    #[compio::test]
    async fn test_wrapper_sync_all() -> anyhow::Result<()> {
        // Create a temp file
        let temp_file = NamedTempFile::new()?;

        let file = File::create(temp_file.path()).await?;
        let mut wrapper = AsyncFileWrapper::new(file);

        // Write and sync
        let data = b"Test sync".to_vec();
        wrapper.write_at(data, 0).await?;
        wrapper.sync_all().await?;

        // Verify data persisted
        let file = File::open(temp_file.path()).await?;
        let wrapper = AsyncFileWrapper::new(file);

        let buffer = vec![0u8; 9];
        let (bytes_read, buffer) = wrapper.read_at(buffer, 0).await?;

        assert_eq!(bytes_read, 9);
        assert_eq!(&buffer[..bytes_read], b"Test sync");

        Ok(())
    }
}
