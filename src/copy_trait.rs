//! Trait-based file copying
//!
//! This module provides file copying using the AsyncFile trait abstraction.
//! It demonstrates how the trait can be used for file operations while
//! maintaining compatibility with the existing copy infrastructure.

use crate::error::{Result, SyncError};
use crate::file_wrapper::AsyncFileWrapper;
use crate::traits::AsyncFile;
use compio::fs::File;
use std::path::Path;

/// Default buffer size for trait-based copying
#[allow(dead_code)] // Used in later PRs
const COPY_BUFFER_SIZE: usize = 64 * 1024; // 64KB

/// Copy a file using the AsyncFile trait
///
/// This is a simple demonstration of using the trait-based file abstraction
/// for file copying. It uses the AsyncFile trait methods to read from source
/// and write to destination.
///
/// # Parameters
///
/// * `src` - Source file path
/// * `dst` - Destination file path
///
/// # Returns
///
/// Returns `Ok(bytes_copied)` on success, where `bytes_copied` is the total
/// number of bytes copied from source to destination.
///
/// # Errors
///
/// Returns `Err(SyncError)` if:
/// - Source file cannot be opened or read
/// - Destination file cannot be created or written
/// - I/O error occurs during copy
///
/// # Examples
///
/// ```rust,ignore
/// use arsync::copy_trait::copy_file_with_trait;
/// use std::path::Path;
///
/// #[compio::main]
/// async fn main() -> arsync::Result<()> {
///     let bytes = copy_file_with_trait(
///         Path::new("source.txt"),
///         Path::new("dest.txt")
///     ).await?;
///     println!("Copied {} bytes", bytes);
///     Ok(())
/// }
/// ```
#[allow(dead_code)] // Used in later PRs
pub async fn copy_file_with_trait(src: &Path, dst: &Path) -> Result<u64> {
    // Open source file
    let src_file = File::open(src)
        .await
        .map_err(|e| SyncError::FileSystem(format!("Failed to open source: {e}")))?;

    // Wrap in AsyncFile trait
    let src_wrapper = AsyncFileWrapper::new(src_file);

    // Open destination file
    let dst_file = File::create(dst)
        .await
        .map_err(|e| SyncError::FileSystem(format!("Failed to create destination: {e}")))?;

    // Wrap in AsyncFile trait
    let mut dst_wrapper = AsyncFileWrapper::new(dst_file);

    // Get source file size
    let total_size = src_wrapper.size().await?;

    // Copy in chunks
    let mut bytes_copied: u64 = 0;
    let mut buffer = vec![0u8; COPY_BUFFER_SIZE];

    while bytes_copied < total_size {
        // Read chunk from source
        let (bytes_read, returned_buffer) = src_wrapper.read_at(buffer, bytes_copied).await?;

        if bytes_read == 0 {
            break; // EOF
        }

        // Write chunk to destination
        let chunk_to_write = returned_buffer[..bytes_read].to_vec();
        let (bytes_written, returned_buffer) =
            dst_wrapper.write_at(chunk_to_write, bytes_copied).await?;

        if bytes_written == 0 {
            return Err(SyncError::FileSystem(
                "Failed to write: no bytes written".to_string(),
            ));
        }

        if bytes_written != bytes_read {
            return Err(SyncError::FileSystem(format!(
                "Incomplete write: wrote {} of {} bytes",
                bytes_written, bytes_read
            )));
        }

        bytes_copied += bytes_written as u64;
        buffer = returned_buffer;
    }

    // Sync to disk
    dst_wrapper.sync_all().await?;

    Ok(bytes_copied)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[compio::test]
    async fn test_copy_small_file() -> anyhow::Result<()> {
        // Create source file
        let mut src_file = NamedTempFile::new()?;
        src_file.write_all(b"Hello, World!")?;
        src_file.flush()?;

        // Create destination
        let dst_file = NamedTempFile::new()?;
        let dst_path = dst_file.path().to_path_buf();
        drop(dst_file); // Close it so we can write

        // Copy using trait
        let bytes = copy_file_with_trait(src_file.path(), &dst_path).await?;

        assert_eq!(bytes, 13);

        // Verify contents
        let contents = std::fs::read(&dst_path)?;
        assert_eq!(contents, b"Hello, World!");

        Ok(())
    }

    #[compio::test]
    async fn test_copy_empty_file() -> anyhow::Result<()> {
        // Create empty source file
        let src_file = NamedTempFile::new()?;

        // Create destination
        let dst_file = NamedTempFile::new()?;
        let dst_path = dst_file.path().to_path_buf();
        drop(dst_file);

        // Copy using trait
        let bytes = copy_file_with_trait(src_file.path(), &dst_path).await?;

        assert_eq!(bytes, 0);

        // Verify empty
        let contents = std::fs::read(&dst_path)?;
        assert_eq!(contents.len(), 0);

        Ok(())
    }

    #[compio::test]
    async fn test_copy_large_file() -> anyhow::Result<()> {
        // Create 1MB file
        let mut src_file = NamedTempFile::new()?;
        let data = vec![0x42u8; 1024 * 1024]; // 1MB of 'B's
        src_file.write_all(&data)?;
        src_file.flush()?;

        // Create destination
        let dst_file = NamedTempFile::new()?;
        let dst_path = dst_file.path().to_path_buf();
        drop(dst_file);

        // Copy using trait
        let bytes = copy_file_with_trait(src_file.path(), &dst_path).await?;

        assert_eq!(bytes, 1024 * 1024);

        // Verify contents
        let contents = std::fs::read(&dst_path)?;
        assert_eq!(contents.len(), 1024 * 1024);
        assert!(contents.iter().all(|&b| b == 0x42));

        Ok(())
    }

    #[compio::test]
    async fn test_copy_matches_original() -> anyhow::Result<()> {
        // Create source file with specific pattern
        let mut src_file = NamedTempFile::new()?;
        let pattern: Vec<u8> = (0..=255).cycle().take(10000).collect();
        src_file.write_all(&pattern)?;
        src_file.flush()?;

        // Create destination
        let dst_file = NamedTempFile::new()?;
        let dst_path = dst_file.path().to_path_buf();
        drop(dst_file);

        // Copy using trait
        let bytes = copy_file_with_trait(src_file.path(), &dst_path).await?;

        assert_eq!(bytes, 10000);

        // Verify exact match
        let src_contents = std::fs::read(src_file.path())?;
        let dst_contents = std::fs::read(&dst_path)?;
        assert_eq!(src_contents, dst_contents);

        Ok(())
    }
}
