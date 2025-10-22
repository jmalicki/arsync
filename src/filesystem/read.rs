//! Secure file reading using DirectoryFd
//!
//! Provides TOCTOU-safe file reading operations using DirectoryFd and openat.

use crate::error::{Result, SyncError};
use compio::io::AsyncReadAt;
use compio_fs_extended::DirectoryFd;
use std::ffi::OsStr;

/// Read entire file content using DirectoryFd (TOCTOU-safe)
///
/// Reads a file using `openat` relative to a directory file descriptor,
/// ensuring TOCTOU safety and avoiding symlink attacks.
///
/// # Parameters
///
/// * `dir_fd` - Parent directory file descriptor
/// * `filename` - File name relative to directory (basename only, no path separators)
///
/// # Returns
///
/// Returns `Ok(Vec<u8>)` containing the entire file content
///
/// # Errors
///
/// Returns `Err(SyncError)` if:
/// - File cannot be opened
/// - File cannot be read
/// - I/O error occurs
///
/// # Security
///
/// Uses `openat(dirfd, filename, ...)` which is TOCTOU-safe.
/// Does not use path-based operations.
///
/// # Examples
///
/// ```rust,ignore
/// use arsync::filesystem::read_file_content;
/// use compio_fs_extended::DirectoryFd;
///
/// #[compio::main]
/// async fn main() -> arsync::Result<()> {
///     let dir_fd = DirectoryFd::open("/some/directory").await?;
///     let content = read_file_content(&dir_fd, "file.txt").await?;
///     println!("Read {} bytes", content.len());
///     Ok(())
/// }
/// ```
pub async fn read_file_content(dir_fd: &DirectoryFd, filename: &OsStr) -> Result<Vec<u8>> {
    // Open file using DirectoryFd (TOCTOU-safe openat)
    let file = dir_fd
        .open_file_at(filename, true, false, false, false)
        .await
        .map_err(|e| {
            SyncError::FileSystem(format!(
                "Failed to open file '{}' in {}: {e}",
                filename.to_string_lossy(),
                dir_fd.path().display()
            ))
        })?;

    // Get file size for allocation
    use std::os::unix::fs::MetadataExt;
    use std::time::SystemTime;

    let m = file
        .metadata()
        .await
        .map_err(|e| SyncError::FileSystem(format!("Failed to get file metadata: {e}")))?;

    let file_size = m.len() as usize;

    // Allocate buffer
    let mut content = vec![0u8; file_size];
    let mut offset = 0u64;

    // Read entire file
    while offset < file_size as u64 {
        let remaining = file_size - offset as usize;
        let chunk_buffer = vec![0u8; remaining];

        let buf_result = file.read_at(chunk_buffer, offset).await;
        let bytes_read = buf_result.0.map_err(|e| {
            SyncError::FileSystem(format!("Failed to read from file at offset {offset}: {e}"))
        })?;

        if bytes_read == 0 {
            break; // EOF
        }

        // Copy into result buffer
        content[offset as usize..offset as usize + bytes_read]
            .copy_from_slice(&buf_result.1[..bytes_read]);

        offset += bytes_read as u64;
    }

    // Truncate if we read less than expected
    content.truncate(offset as usize);

    Ok(content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[compio::test]
    async fn test_read_small_file() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, b"Hello, World!")?;

        let dir_fd = DirectoryFd::open(temp_dir.path()).await?;
        let content = read_file_content(&dir_fd, OsStr::new("test.txt")).await?;

        assert_eq!(content, b"Hello, World!");

        Ok(())
    }

    #[compio::test]
    async fn test_read_empty_file() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let file_path = temp_dir.path().join("empty.txt");
        fs::File::create(&file_path)?;

        let dir_fd = DirectoryFd::open(temp_dir.path()).await?;
        let content = read_file_content(&dir_fd, OsStr::new("empty.txt")).await?;

        assert_eq!(content.len(), 0);

        Ok(())
    }

    #[compio::test]
    async fn test_read_binary_file() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let file_path = temp_dir.path().join("data.bin");
        let data: Vec<u8> = (0..=255).collect();
        fs::write(&file_path, &data)?;

        let dir_fd = DirectoryFd::open(temp_dir.path()).await?;
        let content = read_file_content(&dir_fd, OsStr::new("data.bin")).await?;

        assert_eq!(content, data);

        Ok(())
    }

    #[compio::test]
    async fn test_read_large_file() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let file_path = temp_dir.path().join("large.dat");
        let data = vec![0x42u8; 1024 * 1024]; // 1MB
        fs::write(&file_path, &data)?;

        let dir_fd = DirectoryFd::open(temp_dir.path()).await?;
        let content = read_file_content(&dir_fd, OsStr::new("large.dat")).await?;

        assert_eq!(content.len(), 1024 * 1024);
        assert!(content.iter().all(|&b| b == 0x42));

        Ok(())
    }

    #[compio::test]
    async fn test_read_nonexistent_file() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;

        let dir_fd = DirectoryFd::open(temp_dir.path()).await?;
        let result = read_file_content(&dir_fd, OsStr::new("nonexistent.txt")).await;

        assert!(result.is_err());

        Ok(())
    }
}
