//! Hardlink operations for creating hard links

use crate::error::{hardlink_error, Result};
use compio::driver::OpCode;
use compio::fs::File;
#[cfg(target_os = "linux")]
use io_uring::{opcode, types};
#[cfg(unix)]
use std::ffi::CString;
use std::path::Path;
use std::pin::Pin;

/// Trait for hardlink operations
#[allow(async_fn_in_trait)]
pub trait HardlinkOps {
    /// Create a hard link to the file
    ///
    /// # Arguments
    ///
    /// * `target` - The target path for the hard link
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The target path already exists
    /// - The target is on a different filesystem
    /// - Permission is denied
    /// - The operation fails due to I/O errors
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use compio_fs_extended::{ExtendedFile, HardlinkOps};
    /// use compio::fs::File;
    /// use std::path::Path;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let file = File::open("original.txt").await?;
    /// let extended_file = ExtendedFile::new(file);
    ///
    /// extended_file.create_hardlink(Path::new("hardlink.txt")).await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn create_hardlink(&self, target: &Path) -> Result<()>;
}

/// Implementation of hardlink operations using direct syscalls
///
/// # Errors
///
/// This function will return an error if the hardlink creation fails
pub async fn create_hardlink_impl(_file: &File, _target: &Path) -> Result<()> {
    // Get the file path from the file descriptor
    // This is a simplified implementation - in practice, we'd need to track the path
    Err(hardlink_error(
        "create_hardlink not yet implemented - requires path tracking",
    ))
}

/// Create a hard link at the given path
///
/// # Arguments
///
/// * `original_path` - Path to the original file
/// * `link_path` - Path where the hard link will be created
///
/// # Returns
///
/// `Ok(())` if the hard link was created successfully
///
/// # Errors
///
/// This function will return an error if:
/// - The link path already exists
/// - The original and link are on different filesystems
/// - Permission is denied
/// - The operation fails due to I/O errors
pub async fn create_hardlink_at_path(original_path: &Path, link_path: &Path) -> Result<()> {
    let original_cstr = CString::new(original_path.to_string_lossy().as_bytes())
        .map_err(|e| hardlink_error(&e.to_string()))?;
    let link_cstr = CString::new(link_path.to_string_lossy().as_bytes())
        .map_err(|e| hardlink_error(&e.to_string()))?;

    // Submit io_uring LINKAT operation via compio
    let result = compio::runtime::submit(HardlinkOp::new(original_cstr, link_cstr)).await;

    match result.0 {
        Ok(_) => Ok(()),
        Err(e) => Err(hardlink_error(&e.to_string())),
    }
}

/// io_uring hardlink (linkat) operation
struct HardlinkOp {
    /// Source path to link from
    oldpath: CString,
    /// Destination path for the new hardlink
    newpath: CString,
}

impl HardlinkOp {
    /// Create a new hardlink operation for submission to io_uring
    #[must_use]
    fn new(oldpath: CString, newpath: CString) -> Self {
        Self { oldpath, newpath }
    }
}

impl OpCode for HardlinkOp {
    fn create_entry(self: Pin<&mut Self>) -> compio::driver::OpEntry {
        // Use AT_FDCWD for both paths
        compio::driver::OpEntry::Submission(
            opcode::LinkAt::new(
                types::Fd(libc::AT_FDCWD),
                self.oldpath.as_ptr(),
                types::Fd(libc::AT_FDCWD),
                self.newpath.as_ptr(),
            )
            .build(),
        )
    }
}
