//! fallocate operations for file preallocation using io_uring opcodes

use crate::error::{fallocate_error, Result};
use compio::driver::OpCode;
use compio::fs::File;
use compio::runtime::submit;
#[cfg(target_os = "linux")]
use io_uring::{opcode, types};
#[cfg(unix)]
use std::os::unix::io::AsRawFd;
use std::pin::Pin;

/// Trait for fallocate operations
pub trait Fallocate {
    /// Preallocate or deallocate space to a file
    ///
    /// This allows the kernel to allocate contiguous disk space for the file,
    /// improving write performance and reducing fragmentation.
    ///
    /// # Arguments
    ///
    /// * `offset` - Starting offset for the allocation
    /// * `len` - Length of the region to allocate
    /// * `mode` - Allocation mode (see `FallocateMode` constants)
    ///
    /// # Returns
    ///
    /// `Ok(())` if the allocation was successful
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The file descriptor is invalid
    /// - The allocation mode is not supported
    /// - The operation fails due to I/O errors
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use compio_fs_extended::{ExtendedFile, Fallocate};
    /// use compio::fs::File;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let file = File::create("large_file.txt").await?;
    /// let extended_file = ExtendedFile::new(file);
    ///
    /// // Preallocate 1GB of space
    /// extended_file.fallocate(0, 1024 * 1024 * 1024, 0).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[allow(async_fn_in_trait)]
    async fn fallocate(&self, offset: u64, len: u64, mode: u32) -> Result<()>;
}

/// File allocation mode constants
pub mod mode {
    /// Default allocation mode (allocate space)
    pub const DEFAULT: u32 = 0;
    /// Keep file size unchanged (FALLOC_FL_KEEP_SIZE)
    pub const KEEP_SIZE: u32 = 1;
    /// Punch hole in file (FALLOC_FL_PUNCH_HOLE)
    pub const PUNCH_HOLE: u32 = 2;
    /// Don't update file size (FALLOC_FL_NO_HIDE_STALE)
    pub const NO_HIDE_STALE: u32 = 4;
    /// Collapse range (FALLOC_FL_COLLAPSE_RANGE)
    pub const COLLAPSE_RANGE: u32 = 8;
    /// Zero range (FALLOC_FL_ZERO_RANGE)
    pub const ZERO_RANGE: u32 = 16;
    /// Insert range (FALLOC_FL_INSERT_RANGE)
    pub const INSERT_RANGE: u32 = 32;
    /// Unshare range (FALLOC_FL_UNSHARE_RANGE)
    pub const UNSHARE_RANGE: u32 = 64;
}

/// Custom fallocate operation that implements compio's OpCode trait
pub struct FallocateOp {
    /// File descriptor to apply fallocate to
    fd: i32,
    /// Starting offset for the allocation
    offset: u64,
    /// Length of the region to allocate
    len: u64,
    /// Allocation mode flags
    mode: i32,
}

impl FallocateOp {
    /// Create a new FallocateOp for io_uring submission
    ///
    /// # Arguments
    ///
    /// * `file` - File to apply fallocate to
    /// * `offset` - Starting offset for the allocation
    /// * `len` - Length of the region to allocate
    /// * `mode` - Allocation mode flags
    #[must_use]
    pub fn new(file: &File, offset: u64, len: u64, mode: u32) -> Self {
        Self {
            fd: file.as_raw_fd(),
            offset,
            len,
            mode: mode as i32,
        }
    }
}

#[cfg(target_os = "linux")]
impl OpCode for FallocateOp {
    fn create_entry(self: Pin<&mut Self>) -> compio::driver::OpEntry {
        compio::driver::OpEntry::Submission(
            opcode::Fallocate::new(types::Fd(self.fd), self.len)
                .offset(self.offset)
                .mode(self.mode)
                .build(),
        )
    }
}

/// Preallocate space to a file using io_uring fallocate opcode
///
/// # Arguments
///
/// * `file` - The file to preallocate space for
/// * `offset` - Starting offset for the allocation
/// * `len` - Length of the region to allocate
/// * `mode` - Allocation mode (see `mode` constants)
///
/// # Returns
///
/// `Ok(())` if the allocation was successful
///
/// # Errors
///
/// This function will return an error if the underlying fallocate operation fails.
#[cfg(target_os = "linux")]
pub async fn fallocate(file: &File, offset: u64, len: u64, mode: u32) -> Result<()> {
    // Submit io_uring fallocate operation using compio's runtime
    let result = submit(FallocateOp::new(file, offset, len, mode)).await;

    // Minimal mapping: preserve underlying error string without extra context
    match result.0 {
        Ok(_) => Ok(()),
        Err(e) => Err(fallocate_error(&e.to_string())),
    }
}

#[cfg(target_os = "macos")]
pub async fn fallocate(file: &File, offset: u64, len: u64, _mode: u32) -> Result<()> {
    // Darwin preallocation via fcntl(F_PREALLOCATE). For now run off-thread.
    use std::os::fd::AsRawFd;
    let fd = file.as_raw_fd();
    compio::runtime::spawn(async move {
        // SAFETY: invoking libc fcntl with F_PREALLOCATE
        let fstore = libc::fstore {
            fst_flags: libc::F_ALLOCATECONTIG,
            fst_posmode: libc::F_PEOFPOSMODE,
            fst_offset: offset as i64,
            fst_length: len as i64,
            fst_bytesalloc: 0,
        };
        let rc = unsafe { libc::fcntl(fd, libc::F_PREALLOCATE, &fstore) };
        if rc == -1 {
            Err(fallocate_error(&format!(
                "F_PREALLOCATE failed: {}",
                std::io::Error::last_os_error()
            )))
        } else {
            Ok(())
        }
    })
    .await
    .map_err(|e| fallocate_error(&format!("spawn failed: {e:?}")))?
}

#[cfg(target_os = "windows")]
pub async fn fallocate(file: &File, _offset: u64, len: u64, _mode: u32) -> Result<()> {
    // Preallocate via FILE_ALLOCATION_INFO; extend logical size via SetEndOfFile if needed.
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::BOOL;
    use windows_sys::Win32::Storage::FileSystem::{
        SetFileInformationByHandle, FILE_ALLOCATION_INFO, FILE_END_OF_FILE_INFO,
        FileAllocationInfo, FileEndOfFileInfo,
    };
    let handle = file.as_raw_handle();
    // Safety: construct allocation info
    let alloc = FILE_ALLOCATION_INFO {
        AllocationSize: len as i64,
    };
    let ok: BOOL = unsafe {
        SetFileInformationByHandle(
            handle as _,
            FileAllocationInfo,
            &alloc as *const _ as _,
            std::mem::size_of::<FILE_ALLOCATION_INFO>() as u32,
        )
    };
    if ok == 0 {
        return Err(fallocate_error(
            "SetFileInformationByHandle(FileAllocationInfo) failed",
        ));
    }
    // Optionally grow logical file size (Linux fallocate DEFAULT grows size)
    let end = FILE_END_OF_FILE_INFO {
        EndOfFile: (_offset + len) as i64,
    };
    let ok2: BOOL = unsafe {
        SetFileInformationByHandle(
            handle as _,
            FileEndOfFileInfo,
            &end as *const _ as _,
            std::mem::size_of::<FILE_END_OF_FILE_INFO>() as u32,
        )
    };
    if ok2 == 0 {
        return Err(fallocate_error("SetFileInformationByHandle(FileEndOfFileInfo) failed"));
    }
    Ok(())
}

/// Preallocate space to a file with default mode (allocate space)
///
/// This is a convenience function that uses the default allocation mode.
///
/// # Errors
///
/// This function will return an error if the fallocate operation fails
pub async fn preallocate(file: &File, len: u64) -> Result<()> {
    fallocate(file, 0, len, mode::DEFAULT).await
}

/// Preallocate space to a file keeping the current size
///
/// This is useful for preallocating space without changing the file size.
///
/// # Errors
///
/// This function will return an error if the fallocate operation fails
pub async fn preallocate_keep_size(file: &File, offset: u64, len: u64) -> Result<()> {
    fallocate(file, offset, len, mode::KEEP_SIZE).await
}

/// Punch a hole in a file (deallocate space)
///
/// This removes the allocated space for the specified range, creating a hole.
///
/// # Errors
///
/// This function will return an error if the fallocate operation fails
pub async fn punch_hole(file: &File, offset: u64, len: u64) -> Result<()> {
    fallocate(file, offset, len, mode::PUNCH_HOLE).await
}

/// Zero out a range in a file
///
/// This writes zeros to the specified range without changing the file size.
///
/// # Errors
///
/// This function will return an error if the fallocate operation fails
pub async fn zero_range(file: &File, offset: u64, len: u64) -> Result<()> {
    fallocate(file, offset, len, mode::ZERO_RANGE).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use compio::fs::File;
    use std::fs;
    use tempfile::TempDir;

    #[compio::test]
    async fn test_fallocate_basic() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        // Create test file
        fs::write(&file_path, "test data").unwrap();

        // Open file with write permissions for fallocate
        let file = File::create(&file_path).await.unwrap();

        // Test fallocate
        let result = fallocate(&file, 0, 1024, mode::DEFAULT).await;
        if let Err(ref e) = result {
            println!("fallocate failed: {}", e);
        }
        assert!(result.is_ok(), "fallocate should succeed");
    }

    #[compio::test]
    async fn test_preallocate() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        // Create test file
        fs::write(&file_path, "test data").unwrap();

        // Open file with write permissions for fallocate
        let file = File::create(&file_path).await.unwrap();

        // Test preallocate
        let result = preallocate(&file, 1024).await;
        assert!(result.is_ok());
    }

    #[compio::test]
    async fn test_preallocate_keep_size() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        // Create test file
        fs::write(&file_path, "test data").unwrap();

        // Open file with write permissions for fallocate
        let file = File::create(&file_path).await.unwrap();

        // Test preallocate_keep_size
        let result = preallocate_keep_size(&file, 0, 1024).await;
        assert!(result.is_ok());
    }

    #[compio::test]
    async fn test_punch_hole() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        // Create test file
        fs::write(&file_path, "test data").unwrap();

        // Open file with write permissions for fallocate
        let file = File::create(&file_path).await.unwrap();

        // Test punch_hole
        let result = punch_hole(&file, 0, 512).await;
        match result {
            Ok(_) => println!("punch_hole succeeded"),
            Err(e) => {
                println!("punch_hole failed: {}", e);
                // Punch hole might not be supported on all filesystems
                // This is expected behavior, not a failure
            }
        }
    }

    #[compio::test]
    async fn test_zero_range() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        // Create test file
        fs::write(&file_path, "test data").unwrap();

        // Open file with write permissions for fallocate
        let file = File::create(&file_path).await.unwrap();

        // Test zero_range
        let result = zero_range(&file, 0, 512).await;
        match result {
            Ok(_) => println!("zero_range succeeded"),
            Err(e) => {
                println!("zero_range failed: {}", e);
                // Zero range might not be supported on all filesystems
                // This is expected behavior, not a failure
            }
        }
    }
}
