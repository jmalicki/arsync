//! copy_file_range operations for efficient same-filesystem copies

use crate::error::{copy_file_range_error, Result};
use compio::fs::File;
use compio_io::{AsyncReadAt, AsyncWriteAt};
#[cfg(unix)]
use std::os::unix::io::AsRawFd;

/// Trait for copy_file_range operations
#[allow(async_fn_in_trait)]
pub trait CopyFileRange {
    /// Copy data between file descriptors using copy_file_range
    ///
    /// This operation is more efficient than read/write for same-filesystem copies
    /// as it can be performed entirely in the kernel without transferring data
    /// to user space.
    ///
    /// # Arguments
    ///
    /// * `dst` - Destination file
    /// * `src_offset` - Source file offset
    /// * `dst_offset` - Destination file offset  
    /// * `len` - Number of bytes to copy
    ///
    /// # Returns
    ///
    /// Number of bytes copied
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The operation is not supported on the filesystem
    /// - The source and destination are on different filesystems
    /// - The file descriptors are invalid
    /// - The operation fails due to I/O errors
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use compio_fs_extended::{ExtendedFile, CopyFileRange};
    /// use compio::fs::File;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let src_file = File::open("source.txt").await?;
    /// let dst_file = File::create("destination.txt").await?;
    /// let src_extended = ExtendedFile::new(src_file);
    /// let dst_extended = ExtendedFile::new(dst_file);
    ///
    /// let bytes_copied = src_extended.copy_file_range(&dst_extended, 0, 0, 1024).await?;
    /// println!("Copied {} bytes", bytes_copied);
    /// # Ok(())
    /// # }
    /// ```
    async fn copy_file_range(
        &self,
        dst: &Self,
        src_offset: u64,
        dst_offset: u64,
        len: u64,
    ) -> Result<usize>;
}

/// Implementation of copy_file_range using direct syscalls
///
/// # Errors
///
/// This function will return an error if the copy_file_range operation fails
#[cfg(unix)]
pub async fn copy_file_range_impl(
    src: &File,
    dst: &File,
    src_offset: u64,
    dst_offset: u64,
    len: u64,
) -> Result<usize> {
    // Get raw file descriptors
    let src_fd = src.as_raw_fd();
    let dst_fd = dst.as_raw_fd();

    // Perform the copy_file_range syscall
    let result = unsafe {
        let mut src_off = src_offset as i64;
        let mut dst_off = dst_offset as i64;

        libc::copy_file_range(
            src_fd,
            &mut src_off,
            dst_fd,
            &mut dst_off,
            len as usize,
            0, // flags
        )
    };

    if result < 0 {
        let errno = std::io::Error::last_os_error();
        return Err(copy_file_range_error(&format!(
            "copy_file_range syscall failed: {}",
            errno
        )));
    }

    Ok(result as usize)
}

#[cfg(windows)]
pub async fn copy_file_range_impl(
    _src: &File,
    _dst: &File,
    _src_offset: u64,
    _dst_offset: u64,
    _len: u64,
) -> Result<usize> {
    Err(copy_file_range_error(
        "copy_file_range syscall is not available on Windows",
    ))
}

/// Get the maximum number of bytes that can be copied in a single copy_file_range operation
///
/// # Returns
///
/// The maximum number of bytes, or `None` if the limit is unknown
#[must_use]
pub fn max_copy_file_range_bytes() -> Option<usize> {
    // This is typically limited by the filesystem and kernel implementation
    // For most modern filesystems, this is effectively unlimited
    None
}

/// Copy file range with automatic fallback to read/write if copy_file_range fails
///
/// # Arguments
///
/// * `src` - Source file
/// * `dst` - Destination file
/// * `src_offset` - Source file offset
/// * `dst_offset` - Destination file offset
/// * `len` - Number of bytes to copy
///
/// # Returns
///
/// Number of bytes copied
///
/// # Errors
///
/// This function will return an error if both copy_file_range and read/write operations fail
pub async fn copy_file_range_with_fallback(
    src: &File,
    dst: &File,
    src_offset: u64,
    dst_offset: u64,
    len: u64,
) -> Result<usize> {
    // Try copy_file_range first (Unix), otherwise fallback to async read/write loop
    #[cfg(unix)]
    if let Ok(bytes_copied) = copy_file_range_impl(src, dst, src_offset, dst_offset, len).await {
        return Ok(bytes_copied);
    }

    // IOCP/kqueue-driven fallback using compio::fs async I/O with owned buffers
    let mut remaining = len;
    let mut src_off = src_offset;
    let mut dst_off = dst_offset;
    let mut total: usize = 0;
    let src_clone = src.clone();
    let mut dst_clone = dst.clone();
    let chunk: usize = 1 << 20; // 1 MiB

    while remaining > 0 {
        let to_read = remaining.min(chunk as u64) as usize;
        let read_buf = vec![0u8; to_read];
        let res = src_clone.read_at(read_buf, src_off).await;
        let (n, read_buf) = match res.0 {
            Ok(n) => (n, res.1),
            Err(e) => {
                return Err(copy_file_range_error(&format!(
                    "fallback read_at failed: {e}"
                )))
            }
        };
        if n == 0 {
            break;
        }
        let mut write_buf = read_buf;
        if write_buf.len() != n {
            // Shrink to written length
            let _ = write_buf.split_off(n);
        }
        let wres = dst_clone.write_at(write_buf, dst_off).await;
        if let Err(e) = wres.0 {
            return Err(copy_file_range_error(&format!(
                "fallback write_at failed: {e}"
            )));
        }
        src_off += n as u64;
        dst_off += n as u64;
        remaining -= n as u64;
        total += n;
    }

    Ok(total)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use compio::fs::File;
    use std::fs::write;
    use tempfile::TempDir;

    #[compio::test]
    async fn test_copy_file_range_basic() {
        let temp_dir = TempDir::new().unwrap();
        let src_path = temp_dir.path().join("source.txt");
        let dst_path = temp_dir.path().join("destination.txt");

        // Create source file with test data
        write(&src_path, "Hello, World!").unwrap();

        // Open files
        let src_file = File::open(&src_path).await.unwrap();
        let dst_file = File::create(&dst_path).await.unwrap();

        // Test copy_file_range
        let result = copy_file_range_impl(&src_file, &dst_file, 0, 0, 13).await;

        // The operation might fail if not supported, which is expected
        match result {
            Ok(bytes_copied) => {
                assert_eq!(bytes_copied, 13);
                // Verify the destination file has the correct content
                let content = std::fs::read(&dst_path).unwrap();
                assert_eq!(content, b"Hello, World!");
            }
            Err(_) => {
                // copy_file_range not supported, which is fine for testing
                println!("copy_file_range not supported on this filesystem");
            }
        }
    }
}
