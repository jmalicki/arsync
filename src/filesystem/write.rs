//! Secure file writing using DirectoryFd
//!
//! Provides TOCTOU-safe file writing operations using DirectoryFd and openat.

use crate::error::{Result, SyncError};
use compio::io::AsyncWriteAt;
use compio_fs_extended::DirectoryFd;
use std::ffi::OsStr;

/// Write file content using DirectoryFd (TOCTOU-safe)
///
/// Writes data to a file using `openat` relative to a directory file descriptor,
/// ensuring TOCTOU safety and avoiding symlink attacks.
///
/// # Parameters
///
/// * `dir_fd` - Parent directory file descriptor
/// * `filename` - File name relative to directory (basename only, no path separators)
/// * `content` - Data to write
///
/// # Returns
///
/// Returns `Ok(())` on success
///
/// # Errors
///
/// Returns `Err(SyncError)` if:
/// - File cannot be created or opened
/// - File cannot be written
/// - Sync fails
/// - I/O error occurs
///
/// # Security
///
/// Uses `openat(dirfd, filename, O_CREAT|O_WRONLY, ...)` which is TOCTOU-safe.
/// Does not use path-based operations.
///
/// # Examples
///
/// ```rust,ignore
/// use arsync::filesystem::write_file_content;
/// use compio_fs_extended::DirectoryFd;
///
/// #[compio::main]
/// async fn main() -> arsync::Result<()> {
///     let dir_fd = DirectoryFd::open("/some/directory").await?;
///     write_file_content(&dir_fd, "file.txt", b"Hello, World!").await?;
///     Ok(())
/// }
/// ```
pub async fn write_file_content(
    dir_fd: &DirectoryFd,
    filename: &OsStr,
    content: &[u8],
) -> Result<()> {
    // Open/create file using DirectoryFd (TOCTOU-safe openat)
    let mut file = dir_fd
        .open_file_at(
            filename, false, // read
            true,  // write
            true,  // create
            true,  // truncate
        )
        .await
        .map_err(|e| {
            SyncError::FileSystem(format!(
                "Failed to create/open file '{}' in {}: {e}",
                filename.to_string_lossy(),
                dir_fd.path().display()
            ))
        })?;

    // Write content
    let mut offset = 0u64;
    let mut remaining = content;

    while !remaining.is_empty() {
        let chunk_size = remaining.len().min(1024 * 1024); // 1MB chunks
        let chunk = remaining[..chunk_size].to_vec();

        let buf_result = file.write_at(chunk, offset).await;
        let bytes_written = buf_result.0.map_err(|e| {
            SyncError::FileSystem(format!("Failed to write to file at offset {offset}: {e}"))
        })?;

        if bytes_written == 0 {
            return Err(SyncError::FileSystem(
                "Failed to write: no bytes written".to_string(),
            ));
        }

        offset += bytes_written as u64;
        remaining = &remaining[bytes_written..];
    }

    // Sync to disk
    file.sync_all()
        .await
        .map_err(|e| SyncError::FileSystem(format!("Failed to sync file: {e}")))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[compio::test]
    async fn test_write_small_file() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;

        let dir_fd = DirectoryFd::open(temp_dir.path()).await?;
        write_file_content(&dir_fd, OsStr::new("test.txt"), b"Hello, World!").await?;

        // Verify
        let content = fs::read(temp_dir.path().join("test.txt"))?;
        assert_eq!(content, b"Hello, World!");

        Ok(())
    }

    #[compio::test]
    async fn test_write_empty_file() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;

        let dir_fd = DirectoryFd::open(temp_dir.path()).await?;
        write_file_content(&dir_fd, OsStr::new("empty.txt"), b"").await?;

        // Verify
        let content = fs::read(temp_dir.path().join("empty.txt"))?;
        assert_eq!(content.len(), 0);

        Ok(())
    }

    #[compio::test]
    async fn test_write_binary_file() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let data: Vec<u8> = (0..=255).collect();

        let dir_fd = DirectoryFd::open(temp_dir.path()).await?;
        write_file_content(&dir_fd, OsStr::new("data.bin"), &data).await?;

        // Verify
        let content = fs::read(temp_dir.path().join("data.bin"))?;
        assert_eq!(content, data);

        Ok(())
    }

    #[compio::test]
    async fn test_write_large_file() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let data = vec![0x42u8; 5 * 1024 * 1024]; // 5MB (multiple chunks)

        let dir_fd = DirectoryFd::open(temp_dir.path()).await?;
        write_file_content(&dir_fd, OsStr::new("large.dat"), &data).await?;

        // Verify
        let content = fs::read(temp_dir.path().join("large.dat"))?;
        assert_eq!(content.len(), 5 * 1024 * 1024);
        assert!(content.iter().all(|&b| b == 0x42));

        Ok(())
    }

    #[compio::test]
    async fn test_write_overwrites_existing() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let file_path = temp_dir.path().join("overwrite.txt");

        // Create initial file
        fs::write(&file_path, b"Original content")?;

        let dir_fd = DirectoryFd::open(temp_dir.path()).await?;
        write_file_content(&dir_fd, OsStr::new("overwrite.txt"), b"New content").await?;

        // Verify overwritten
        let content = fs::read(&file_path)?;
        assert_eq!(content, b"New content");

        Ok(())
    }
}
