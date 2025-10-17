//! fadvise operations for file access pattern optimization
//!
//! ## Platform Support
//!
//! **Unix (Linux, macOS)**: Full POSIX fadvise support
//! - Uses `posix_fadvise(2)` syscall on Unix
//! - Uses io_uring IORING_OP_FADVISE on Linux (when available)
//! - Provides hints to kernel about file access patterns
//!
//! **Windows**: Not currently supported
//! - Returns `NotSupported` error
//! - POSIX fadvise constants don't exist on Windows
//!
//! ### Future: Windows Native Hints (Option 3)
//!
//! Windows has different file hint mechanisms that could be mapped:
//!
//! **At file open time** (would require reopening file):
//! - `Sequential` → `FILE_FLAG_SEQUENTIAL_SCAN`
//! - `Random` → `FILE_FLAG_RANDOM_ACCESS`
//!
//! **SetFileInformationByHandle** (for existing file handles):
//! - `DontNeed` → Could map to `FILE_INFO_BY_HANDLE_CLASS::FileDispositionInfo`
//!   to hint cached data can be discarded
//! - `WillNeed` → Could use `PrefetchVirtualMemory` on memory-mapped regions
//!
//! **Challenges**:
//! - Different API surface (flags at open vs hints on existing handle)
//! - Requires different semantics (e.g., Sequential/Random set at open time)
//! - May require file reopening or memory mapping for full functionality
//!
//! If Windows support is needed, consider:
//! 1. Make `fadvise` take effect on next open (store hints, reopen file)
//! 2. Return success but document limited Windows implementation
//! 3. Add separate Windows-specific hint API that matches native semantics
//!
//! For now, keeping Unix-only to maintain clean semantics and avoid
//! surprising cross-platform behavior differences.

#[cfg(target_os = "linux")]
use crate::error::fadvise_error;
use crate::error::Result;
#[cfg(target_os = "linux")]
use compio::driver::OpCode;
#[cfg(target_os = "linux")]
use compio::fs::File;
#[cfg(target_os = "linux")]
use compio::runtime::submit;
#[cfg(target_os = "linux")]
use io_uring::{opcode, types};
#[cfg(target_os = "linux")]
use std::os::unix::io::AsRawFd;
#[cfg(target_os = "linux")]
use std::pin::Pin;

/// Trait for fadvise operations using io_uring (Linux-only)
#[allow(async_fn_in_trait)]
#[cfg(target_os = "linux")]
pub trait Fadvise {
    /// Provide advice about file access patterns to the kernel using io_uring
    ///
    /// This allows the kernel to optimize caching and I/O behavior based on
    /// the expected access pattern. Uses io_uring IORING_OP_FADVISE operation
    /// for optimal performance.
    ///
    /// # Arguments
    ///
    /// * `advice` - The advice to give (see `FadviseAdvice` constants)
    /// * `offset` - File offset to start the advice
    /// * `len` - Length of the region to apply advice to
    ///
    /// # Returns
    ///
    /// `Ok(())` if the advice was successfully applied
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The file descriptor is invalid
    /// - The advice is not supported
    /// - The io_uring operation fails
    /// - The kernel doesn't support fadvise io_uring operations
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use compio_fs_extended::{ExtendedFile, Fadvise};
    /// use compio_fs_extended::fadvise::FadviseAdvice;
    /// use compio::fs::File;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let file = File::open("large_file.txt").await?;
    /// let extended_file = ExtendedFile::new(file);
    ///
    /// // Advise sequential access for better performance using io_uring
    /// extended_file.fadvise(FadviseAdvice::Sequential, 0, 0).await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn fadvise(&self, advice: FadviseAdvice, offset: i64, len: i64) -> Result<()>;
}

/// fadvise advice types for file access pattern optimization
///
/// Unix: Maps to POSIX fadvise constants
/// Windows: Accepted but fadvise returns NotSupported error
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FadviseAdvice {
    /// Data will be accessed sequentially
    Sequential,
    /// Data will be accessed randomly
    Random,
    /// Data will not be accessed again soon
    DontNeed,
    /// Data will be accessed again soon
    WillNeed,
    /// Data will not be accessed again
    NoReuse,
    /// Normal access pattern (default)
    Normal,
}

#[cfg(target_os = "linux")]
impl FadviseAdvice {
    /// Convert to the underlying POSIX constant (Linux only - macOS removed posix_fadvise)
    fn to_posix(self) -> i32 {
        match self {
            FadviseAdvice::Sequential => libc::POSIX_FADV_SEQUENTIAL,
            FadviseAdvice::Random => libc::POSIX_FADV_RANDOM,
            FadviseAdvice::DontNeed => libc::POSIX_FADV_DONTNEED,
            FadviseAdvice::WillNeed => libc::POSIX_FADV_WILLNEED,
            FadviseAdvice::NoReuse => libc::POSIX_FADV_NOREUSE,
            FadviseAdvice::Normal => libc::POSIX_FADV_NORMAL,
        }
    }
}

/// Custom fadvise operation that implements compio's OpCode trait
#[cfg(target_os = "linux")]
pub struct FadviseOp {
    /// File descriptor to apply advice to
    fd: i32,
    /// File offset to start the advice
    offset: i64,
    /// Length of the region to apply advice to
    len: i64,
    /// The fadvise advice constant
    advice: i32,
}

#[cfg(target_os = "linux")]
impl FadviseOp {
    /// Create a new FadviseOp for io_uring submission
    ///
    /// # Arguments
    ///
    /// * `fd` - File descriptor to apply advice to
    /// * `offset` - File offset to start the advice
    /// * `len` - Length of the region to apply advice to
    /// * `advice` - The fadvise advice constant
    #[must_use]
    pub fn new(fd: i32, offset: i64, len: i64, advice: i32) -> Self {
        Self {
            fd,
            offset,
            len,
            advice,
        }
    }
}

#[cfg(target_os = "linux")]
impl OpCode for FadviseOp {
    fn create_entry(self: Pin<&mut Self>) -> compio::driver::OpEntry {
        compio::driver::OpEntry::Submission(
            opcode::Fadvise::new(types::Fd(self.fd), self.len, self.advice)
                .offset(self.offset as u64)
                .build(),
        )
    }
}

/// Implementation of fadvise using io_uring operations
///
/// This function submits an io_uring fadvise operation and waits for completion.
/// It uses compio's runtime integration for proper async coordination.
///
/// # Arguments
///
/// * `file` - The file to apply advice to
/// * `advice` - The fadvise advice constant
/// * `offset` - File offset to start the advice
/// * `len` - Length of the region to apply advice to
///
/// # Returns
///
/// `Ok(())` if the fadvise operation completed successfully
///
/// # Errors
///
/// This function will return an error if:
/// - The io_uring operation fails
/// - The file descriptor is invalid
/// - The kernel doesn't support fadvise io_uring operations
/// - The advice parameter is invalid
///
/// # Example
///
/// ```rust,no_run
/// use compio_fs_extended::fadvise::{fadvise, FadviseAdvice};
/// use compio::fs::File;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let file = File::open("large_file.txt").await?;
/// fadvise(&file, FadviseAdvice::Sequential, 0, 0).await?;
/// # Ok(())
/// # }
/// ```
#[cfg(target_os = "linux")]
pub async fn fadvise(file: &File, advice: FadviseAdvice, offset: i64, len: i64) -> Result<()> {
    let fd = file.as_raw_fd();

    // Submit io_uring fadvise operation using compio's runtime
    let result = submit(FadviseOp::new(fd, offset, len, advice.to_posix())).await;

    // Minimal mapping: preserve underlying error string without extra context
    match result.0 {
        Ok(_) => Ok(()),
        Err(e) => Err(fadvise_error(&e.to_string())),
    }
}

// macOS/Windows: fadvise not defined
// macOS removed posix_fadvise, Windows has different file hint mechanisms
// Compile-time error if you try to use fadvise on these platforms

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;
    use compio::fs::File;
    use std::fs::write;
    use tempfile::TempDir;

    #[compio::test]
    async fn test_fadvise_sequential() {
        // Test: Verify that io_uring fadvise sequential operation works correctly
        // This test validates the core fadvise_impl function with POSIX_FADV_SEQUENTIAL advice
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        // Create test file with some data
        write(&file_path, "test data for sequential access optimization").unwrap();

        // Open file using compio async File
        let file = File::open(&file_path).await.unwrap();

        // Test io_uring fadvise sequential operation
        let result = fadvise(&file, FadviseAdvice::Sequential, 0, 0).await;
        assert!(
            result.is_ok(),
            "io_uring fadvise sequential operation should succeed"
        );
    }

    #[compio::test]
    async fn test_fadvise_random() {
        // Test: Verify that io_uring fadvise random operation works correctly
        // This test validates the core fadvise_impl function with POSIX_FADV_RANDOM advice
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        // Create test file with some data
        write(&file_path, "test data for random access optimization").unwrap();

        // Open file using compio async File
        let file = File::open(&file_path).await.unwrap();

        // Test io_uring fadvise random operation
        let result = fadvise(&file, FadviseAdvice::Random, 0, 0).await;
        assert!(
            result.is_ok(),
            "io_uring fadvise random operation should succeed"
        );
    }

    #[compio::test]
    async fn test_fadvise_dont_need() {
        // Test: Verify that io_uring fadvise dont_need operation works correctly
        // This test validates POSIX_FADV_DONTNEED advice for memory optimization
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        // Create test file with some data
        write(&file_path, "test data for dont_need optimization").unwrap();

        // Open file using compio async File
        let file = File::open(&file_path).await.unwrap();

        // Test io_uring fadvise dont_need operation
        let result = fadvise(&file, FadviseAdvice::DontNeed, 0, 0).await;
        assert!(
            result.is_ok(),
            "io_uring fadvise dont_need operation should succeed"
        );
    }

    #[compio::test]
    async fn test_fadvise_with_offset_and_length() {
        // Test: Verify that fadvise works with specific offset and length parameters
        // This test validates that the offset and length parameters are properly handled
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        // Create test file with enough data
        let test_data = "x".repeat(1024);
        write(&file_path, test_data).unwrap();

        // Open file using compio async File
        let file = File::open(&file_path).await.unwrap();

        // Test fadvise with specific offset and length
        let result = fadvise(&file, FadviseAdvice::Sequential, 512, 256).await;
        assert!(
            result.is_ok(),
            "fadvise with offset and length should succeed"
        );
    }

    #[compio::test]
    async fn test_fadvise_large_offset() {
        // Test: Verify that fadvise works with large offset values
        // Let the kernel handle validation instead of doing it ourselves
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        // Create test file
        write(&file_path, "test data").unwrap();

        // Open file using compio async File
        let file = File::open(&file_path).await.unwrap();

        // Test fadvise with large offset - let kernel decide if it's valid
        let result = fadvise(&file, FadviseAdvice::Sequential, i64::MAX, 0).await;
        // This might succeed or fail depending on kernel/filesystem, both are acceptable
        match result {
            Ok(_) => println!("fadvise with large offset succeeded"),
            Err(e) => println!("fadvise with large offset failed as expected: {}", e),
        }
    }
}
