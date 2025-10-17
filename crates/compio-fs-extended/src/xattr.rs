//! Extended attributes (xattr) operations using io_uring opcodes

use crate::error::{xattr_error, Result};
use compio::driver::OpCode;
use compio::fs::File;
use compio::runtime::submit;
use io_uring::{opcode, types};
use std::ffi::CString;
use std::path::Path;
use std::pin::Pin;

/// Trait for xattr operations
#[allow(async_fn_in_trait)]
pub trait XattrOps {
    /// Get an extended attribute value
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the extended attribute
    ///
    /// # Returns
    ///
    /// The value of the extended attribute
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The extended attribute doesn't exist
    /// - Permission is denied
    /// - The operation fails due to I/O errors
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use compio_fs_extended::{ExtendedFile, XattrOps};
    /// use compio::fs::File;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let file = File::open("file.txt").await?;
    /// let extended_file = ExtendedFile::new(file);
    ///
    /// let value = extended_file.get_xattr("user.custom").await?;
    /// println!("xattr value: {:?}", value);
    /// # Ok(())
    /// # }
    /// ```
    async fn get_xattr(&self, name: &str) -> Result<Vec<u8>>;

    /// Set an extended attribute value
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the extended attribute
    /// * `value` - Value to set
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - Permission is denied
    /// - The operation fails due to I/O errors
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use compio_fs_extended::{ExtendedFile, XattrOps};
    /// use compio::fs::File;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let file = File::open("file.txt").await?;
    /// let extended_file = ExtendedFile::new(file);
    ///
    /// extended_file.set_xattr("user.custom", b"value").await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn set_xattr(&self, name: &str, value: &[u8]) -> Result<()>;

    /// List all extended attributes
    ///
    /// # Returns
    ///
    /// Vector of extended attribute names
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - Permission is denied
    /// - The operation fails due to I/O errors
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use compio_fs_extended::{ExtendedFile, XattrOps};
    /// use compio::fs::File;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let file = File::open("file.txt").await?;
    /// let extended_file = ExtendedFile::new(file);
    ///
    /// let names = extended_file.list_xattr().await?;
    /// for name in names {
    ///     println!("xattr: {}", name);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    async fn list_xattr(&self) -> Result<Vec<String>>;
}

/// io_uring getxattr operation
struct GetXattrOp {
    /// File descriptor
    fd: std::os::unix::io::RawFd,
    /// Attribute name (null-terminated)
    name: CString,
    /// Buffer for value
    buffer: Vec<u8>,
}

impl GetXattrOp {
    /// Create a new GetXattrOp for retrieving an extended attribute
    fn new(fd: std::os::unix::io::RawFd, name: CString, size: usize) -> Self {
        Self {
            fd,
            name,
            buffer: vec![0u8; size],
        }
    }
}

impl OpCode for GetXattrOp {
    fn create_entry(mut self: Pin<&mut Self>) -> compio::driver::OpEntry {
        compio::driver::OpEntry::Submission(
            opcode::FGetXattr::new(
                types::Fd(self.fd),
                self.name.as_ptr(),
                self.buffer.as_mut_ptr() as *mut libc::c_void,
                self.buffer.len() as u32,
            )
            .build(),
        )
    }
}

/// io_uring setxattr operation
struct SetXattrOp {
    /// File descriptor
    fd: std::os::unix::io::RawFd,
    /// Attribute name (null-terminated)
    name: CString,
    /// Attribute value
    value: Vec<u8>,
}

impl SetXattrOp {
    /// Create a new SetXattrOp for setting an extended attribute
    fn new(fd: std::os::unix::io::RawFd, name: CString, value: Vec<u8>) -> Self {
        Self { fd, name, value }
    }
}

impl OpCode for SetXattrOp {
    fn create_entry(self: Pin<&mut Self>) -> compio::driver::OpEntry {
        compio::driver::OpEntry::Submission(
            opcode::FSetXattr::new(
                types::Fd(self.fd),
                self.name.as_ptr(),
                self.value.as_ptr() as *const libc::c_void,
                self.value.len() as u32,
            )
            .flags(0) // No flags
            .build(),
        )
    }
}

/// Implementation of xattr operations using io_uring opcodes
///
/// # Errors
///
/// This function will return an error if the xattr operation fails
pub async fn get_xattr_impl(file: &File, name: &str) -> Result<Vec<u8>> {
    use std::os::fd::AsRawFd;

    let name_cstr =
        CString::new(name).map_err(|e| xattr_error(&format!("Invalid xattr name: {e}")))?;

    let fd = file.as_raw_fd();

    // io_uring FGETXATTR requires two calls: first to get size, then to get value
    // (unlike read_at which accepts a large buffer - xattr opcode behaves differently)

    // First call: Get the size with empty buffer (size=0)
    let size_op = GetXattrOp::new(fd, name_cstr.clone(), 0);
    let size_result = submit(size_op).await;

    let size = match size_result.0 {
        Ok(s) => s,
        Err(e) => {
            // ENODATA means attribute doesn't exist
            return Err(xattr_error(&format!("fgetxattr size query failed: {}", e)));
        }
    };

    if size == 0 {
        return Ok(Vec::new());
    }

    // Second call: Get the value with correctly sized buffer
    let value_op = GetXattrOp::new(fd, name_cstr, size);
    let value_result = submit(value_op).await;

    match value_result.0 {
        Ok(actual_size) => {
            let mut buffer = value_result.1.buffer;
            buffer.truncate(actual_size);
            Ok(buffer)
        }
        Err(e) => Err(xattr_error(&format!("fgetxattr failed: {}", e))),
    }
}

/// Implementation of xattr setting using io_uring opcodes
///
/// # Errors
///
/// This function will return an error if the xattr operation fails
pub async fn set_xattr_impl(file: &File, name: &str, value: &[u8]) -> Result<()> {
    use std::os::fd::AsRawFd;

    let name_cstr =
        CString::new(name).map_err(|e| xattr_error(&format!("Invalid xattr name: {e}")))?;

    let fd = file.as_raw_fd();
    let value_vec = value.to_vec();

    // Use io_uring IORING_OP_SETXATTR for setting extended attributes
    let op = SetXattrOp::new(fd, name_cstr, value_vec);
    let result = submit(op).await;

    match result.0 {
        Ok(_) => Ok(()),
        Err(e) => Err(xattr_error(&format!("fsetxattr failed: {}", e))),
    }
}

/// Implementation of xattr listing using safe xattr crate
///
/// NOTE: IORING_OP_FLISTXATTR doesn't exist in the Linux kernel (as of 6.x).
/// The kernel only has FGETXATTR and FSETXATTR, not FLISTXATTR.
/// Using the safe `xattr` crate wrapper instead of unsafe libc.
///
/// # Errors
///
/// This function will return an error if the xattr operation fails
pub async fn list_xattr_impl(file: &File) -> Result<Vec<String>> {
    use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd};
    use xattr::FileExt; // Extension trait for FD-based xattr operations

    let fd = file.as_raw_fd();

    // Using spawn + xattr crate's FileExt trait since kernel lacks IORING_OP_FLISTXATTR
    compio::runtime::spawn(async move {
        // Create a temporary std::fs::File to use FileExt trait
        // SAFETY: fd is valid for the duration of this call
        let temp_file = unsafe { std::fs::File::from_raw_fd(fd) };

        // Use the xattr crate's safe FileExt::list_xattr method
        let attrs = temp_file
            .list_xattr()
            .map_err(|e| xattr_error(&format!("flistxattr failed: {}", e)))?;

        // Prevent temp_file from closing the fd
        let _ = temp_file.into_raw_fd();

        let names: Vec<String> = attrs
            .filter_map(|os_str| os_str.to_str().map(|s| s.to_string()))
            .collect();

        Ok(names)
    })
    .await
    .map_err(|e| xattr_error(&format!("spawn failed: {e:?}")))?
}

/// Get an extended attribute value at the given path
///
/// # Arguments
///
/// * `path` - Path to the file
/// * `name` - Name of the extended attribute
///
/// # Returns
///
/// The value of the extended attribute
///
/// # Errors
///
/// This function will return an error if:
/// - The extended attribute doesn't exist
/// - Permission is denied
/// - The operation fails due to I/O errors
pub async fn get_xattr_at_path(path: &Path, name: &str) -> Result<Vec<u8>> {
    let path_cstr = std::ffi::CString::new(path.to_string_lossy().as_bytes())
        .map_err(|e| xattr_error(&format!("Invalid path: {}", e)))?;
    let name_cstr =
        std::ffi::CString::new(name).map_err(|e| xattr_error(&format!("Invalid name: {}", e)))?;

    // Get the size first
    let size = unsafe {
        libc::getxattr(
            path_cstr.as_ptr(),
            name_cstr.as_ptr(),
            std::ptr::null_mut(),
            0,
        )
    };

    if size < 0 {
        let errno = std::io::Error::last_os_error();
        return Err(xattr_error(&format!("getxattr failed: {}", errno)));
    }

    // Allocate buffer and get the value
    let mut buffer = vec![0u8; size as usize];
    let actual_size = unsafe {
        libc::getxattr(
            path_cstr.as_ptr(),
            name_cstr.as_ptr(),
            buffer.as_mut_ptr() as *mut libc::c_void,
            buffer.len(),
        )
    };

    if actual_size < 0 {
        let errno = std::io::Error::last_os_error();
        return Err(xattr_error(&format!("getxattr failed: {}", errno)));
    }

    buffer.truncate(actual_size as usize);
    Ok(buffer)
}

/// Set an extended attribute value at the given path
///
/// # Arguments
///
/// * `path` - Path to the file
/// * `name` - Name of the extended attribute
/// * `value` - Value to set
///
/// # Returns
///
/// `Ok(())` if the extended attribute was set successfully
///
/// # Errors
///
/// This function will return an error if:
/// - Permission is denied
/// - The operation fails due to I/O errors
pub async fn set_xattr_at_path(path: &Path, name: &str, value: &[u8]) -> Result<()> {
    let path_cstr = std::ffi::CString::new(path.to_string_lossy().as_bytes())
        .map_err(|e| xattr_error(&format!("Invalid path: {}", e)))?;
    let name_cstr =
        std::ffi::CString::new(name).map_err(|e| xattr_error(&format!("Invalid name: {}", e)))?;

    let result = unsafe {
        libc::setxattr(
            path_cstr.as_ptr(),
            name_cstr.as_ptr(),
            value.as_ptr() as *const libc::c_void,
            value.len(),
            0, // flags
        )
    };

    if result != 0 {
        let errno = std::io::Error::last_os_error();
        return Err(xattr_error(&format!("setxattr failed: {}", errno)));
    }

    Ok(())
}

/// List all extended attributes at the given path
///
/// # Arguments
///
/// * `path` - Path to the file
///
/// # Returns
///
/// Vector of extended attribute names
///
/// # Errors
///
/// This function will return an error if:
/// - Permission is denied
/// - The operation fails due to I/O errors
pub async fn list_xattr_at_path(path: &Path) -> Result<Vec<String>> {
    let path_cstr = std::ffi::CString::new(path.to_string_lossy().as_bytes())
        .map_err(|e| xattr_error(&format!("Invalid path: {}", e)))?;

    // Get the size first
    let size = unsafe { libc::listxattr(path_cstr.as_ptr(), std::ptr::null_mut(), 0) };

    if size < 0 {
        let errno = std::io::Error::last_os_error();
        return Err(xattr_error(&format!("listxattr failed: {}", errno)));
    }

    if size == 0 {
        return Ok(Vec::new());
    }

    // Allocate buffer and get the list
    let mut buffer = vec![0u8; size as usize];
    let actual_size = unsafe {
        libc::listxattr(
            path_cstr.as_ptr(),
            buffer.as_mut_ptr() as *mut libc::c_char,
            buffer.len(),
        )
    };

    if actual_size < 0 {
        let errno = std::io::Error::last_os_error();
        return Err(xattr_error(&format!("listxattr failed: {}", errno)));
    }

    // Parse the null-separated list
    let mut names = Vec::new();
    let mut start = 0;
    for (i, &byte) in buffer.iter().enumerate() {
        if byte == 0 {
            if start < i {
                if let Ok(name) = String::from_utf8(buffer[start..i].to_vec()) {
                    names.push(name);
                }
            }
            start = i + 1;
        }
    }

    Ok(names)
}

/// Remove an extended attribute
///
/// # Arguments
///
/// * `path` - Path to the file
/// * `name` - Name of the extended attribute to remove
///
/// # Returns
///
/// `Ok(())` if the extended attribute was removed successfully
///
/// # Errors
///
/// This function will return an error if:
/// - The extended attribute doesn't exist
/// - Permission is denied
/// - The operation fails due to I/O errors
pub async fn remove_xattr_at_path(path: &Path, name: &str) -> Result<()> {
    let path_cstr = std::ffi::CString::new(path.to_string_lossy().as_bytes())
        .map_err(|e| xattr_error(&format!("Invalid path: {}", e)))?;
    let name_cstr =
        std::ffi::CString::new(name).map_err(|e| xattr_error(&format!("Invalid name: {}", e)))?;

    let result = unsafe { libc::removexattr(path_cstr.as_ptr(), name_cstr.as_ptr()) };

    if result != 0 {
        let errno = std::io::Error::last_os_error();
        return Err(xattr_error(&format!("removexattr failed: {}", errno)));
    }

    Ok(())
}

/// Check if extended attributes are supported on the filesystem
///
/// # Arguments
///
/// * `path` - Path to check
///
/// # Returns
///
/// `true` if extended attributes are supported, `false` otherwise
pub async fn is_xattr_supported(path: &Path) -> bool {
    // Try to set a test attribute
    let test_name = "user.compio_fs_extended_test";
    let test_value = b"test";

    match set_xattr_at_path(path, test_name, test_value).await {
        Ok(_) => {
            // Clean up the test attribute
            let _ = remove_xattr_at_path(path, test_name).await;
            true
        }
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_xattr_operations() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        // Create test file
        fs::write(&file_path, "test content").unwrap();

        // Test xattr support
        if is_xattr_supported(&file_path).await {
            // Test set and get
            set_xattr_at_path(&file_path, "user.test", b"test_value")
                .await
                .unwrap();
            let value = get_xattr_at_path(&file_path, "user.test").await.unwrap();
            assert_eq!(value, b"test_value");

            // Test list
            let names = list_xattr_at_path(&file_path).await.unwrap();
            assert!(names.contains(&"user.test".to_string()));

            // Test remove
            remove_xattr_at_path(&file_path, "user.test").await.unwrap();
            let names_after = list_xattr_at_path(&file_path).await.unwrap();
            assert!(!names_after.contains(&"user.test".to_string()));
        } else {
            println!("Extended attributes not supported on this filesystem");
        }
    }

    #[tokio::test]
    async fn test_xattr_support_detection() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        // Create test file
        fs::write(&file_path, "test content").unwrap();

        // Test support detection
        let supported = is_xattr_supported(&file_path).await;
        println!("xattr supported: {}", supported);
    }
}
