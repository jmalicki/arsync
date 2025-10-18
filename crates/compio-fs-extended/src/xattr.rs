//! Extended attributes (xattr) operations using io_uring opcodes
//!
//! # Platform Differences
//!
//! Extended attribute APIs differ between Linux and macOS:
//!
//! ## Linux (simpler API):
//! - `getxattr(path, name, value, size)` - 4 args
//! - `setxattr(path, name, value, size, flags)` - 5 args
//! - `listxattr(path, list, size)` - 3 args
//! - `removexattr(path, name)` - 2 args
//!
//! ## macOS (BSD API with extra features):
//! - `getxattr(path, name, value, size, position, options)` - 6 args
//! - `setxattr(path, name, value, size, position, options)` - 6 args
//! - `listxattr(path, list, size, options)` - 4 args
//! - `removexattr(path, name, options)` - 3 args
//!
//! ### Extra macOS Parameters:
//!
//! - **`position`**: Offset for reading/writing large xattrs in chunks (we always use 0)
//! - **`options`**: Flags like `XATTR_NOFOLLOW` or `XATTR_CREATE` (we always use 0 for defaults)
//!
//! Our implementation uses the same behavior on both platforms by passing 0 for macOS's
//! extra parameters, making it functionally equivalent to Linux's simpler API.

use crate::error::{xattr_error, Result};

// macOS-specific constant for xattr operations on symlinks
#[cfg(target_os = "macos")]
const XATTR_NOFOLLOW: libc::c_int = 0x0001;
#[cfg(target_os = "linux")]
use compio::driver::OpCode;
use compio::fs::File;
#[cfg(target_os = "linux")]
use compio::runtime::submit;
#[cfg(target_os = "linux")]
use io_uring::{opcode, types};
#[cfg(target_os = "linux")]
use std::ffi::CString;
use std::path::Path;
#[cfg(target_os = "linux")]
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
#[cfg(target_os = "linux")]
struct GetXattrOp {
    /// File descriptor
    fd: std::os::unix::io::RawFd,
    /// Attribute name (null-terminated)
    name: CString,
    /// Buffer for value
    buffer: Vec<u8>,
}

#[cfg(target_os = "linux")]
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

#[cfg(target_os = "linux")]
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
#[cfg(target_os = "linux")]
struct SetXattrOp {
    /// File descriptor
    fd: std::os::unix::io::RawFd,
    /// Attribute name (null-terminated)
    name: CString,
    /// Attribute value
    value: Vec<u8>,
}

#[cfg(target_os = "linux")]
impl SetXattrOp {
    /// Create a new SetXattrOp for setting an extended attribute
    fn new(fd: std::os::unix::io::RawFd, name: CString, value: Vec<u8>) -> Self {
        Self { fd, name, value }
    }
}

#[cfg(target_os = "linux")]
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
#[cfg(target_os = "linux")]
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

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub async fn get_xattr_impl(_file: &File, _name: &str) -> Result<Vec<u8>> {
    #[cfg(target_os = "macos")]
    {
        // Darwin supports xattr via libc; keep path-based helpers for now
        Err(xattr_error(
            "file-descriptor based xattr not yet implemented on macOS",
        ))
    }
    #[cfg(target_os = "windows")]
    {
        Err(xattr_error("xattr unsupported on Windows"))
    }
}

/// Implementation of xattr setting using io_uring opcodes
///
/// # Errors
///
/// This function will return an error if the xattr operation fails
#[cfg(target_os = "linux")]
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

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub async fn set_xattr_impl(_file: &File, _name: &str, _value: &[u8]) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        Err(xattr_error(
            "file-descriptor based xattr not yet implemented on macOS",
        ))
    }
    #[cfg(target_os = "windows")]
    {
        Err(xattr_error("xattr unsupported on Windows"))
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
#[cfg(target_os = "linux")]
pub async fn list_xattr_impl(file: &File) -> Result<Vec<String>> {
    use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd};
    #[cfg(target_os = "linux")]
    use xattr::FileExt as _; // bring trait into scope

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

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub async fn list_xattr_impl(_file: &File) -> Result<Vec<String>> {
    #[cfg(target_os = "macos")]
    {
        Err(xattr_error(
            "file-descriptor based xattr list not yet implemented on macOS",
        ))
    }
    #[cfg(target_os = "windows")]
    {
        Err(xattr_error("xattr unsupported on Windows"))
    }
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
#[cfg(unix)]
pub async fn get_xattr_at_path(path: &Path, name: &str) -> Result<Vec<u8>> {
    let path_cstr = std::ffi::CString::new(path.to_string_lossy().as_bytes())
        .map_err(|e| xattr_error(&format!("Invalid path: {}", e)))?;
    let name_cstr =
        std::ffi::CString::new(name).map_err(|e| xattr_error(&format!("Invalid name: {}", e)))?;

    // Get the size first
    // Platform-specific: macOS getxattr takes 6 args (adds position=0, options=0)
    // Linux getxattr takes 4 args
    let size = unsafe {
        #[cfg(target_os = "macos")]
        {
            libc::getxattr(
                path_cstr.as_ptr(),
                name_cstr.as_ptr(),
                std::ptr::null_mut(),
                0,
                0, // position: offset to start reading (0 = from beginning)
                0, // options: flags like XATTR_NOFOLLOW (0 = defaults)
            )
        }
        #[cfg(not(target_os = "macos"))]
        {
            libc::getxattr(
                path_cstr.as_ptr(),
                name_cstr.as_ptr(),
                std::ptr::null_mut(),
                0,
            )
        }
    };

    if size < 0 {
        let errno = std::io::Error::last_os_error();
        return Err(xattr_error(&format!("getxattr failed: {}", errno)));
    }

    // Allocate buffer and get the value
    let mut buffer = vec![0u8; size as usize];
    let actual_size = unsafe {
        #[cfg(target_os = "macos")]
        {
            libc::getxattr(
                path_cstr.as_ptr(),
                name_cstr.as_ptr(),
                buffer.as_mut_ptr() as *mut libc::c_void,
                buffer.len(),
                0, // position
                0, // options
            )
        }
        #[cfg(not(target_os = "macos"))]
        {
            libc::getxattr(
                path_cstr.as_ptr(),
                name_cstr.as_ptr(),
                buffer.as_mut_ptr() as *mut libc::c_void,
                buffer.len(),
            )
        }
    };

    if actual_size < 0 {
        let errno = std::io::Error::last_os_error();
        return Err(xattr_error(&format!("getxattr failed: {}", errno)));
    }

    buffer.truncate(actual_size as usize);
    Ok(buffer)
}

// Windows: get_xattr_at_path not defined - compile-time error

/// Get an extended attribute value (symlink version - doesn't follow symlinks)
///
/// Like `get_xattr_at_path` but operates on the symlink itself, not its target.
///
/// # Implementation
///
/// - **Linux**: Uses `lgetxattr()` (the "l" version doesn't follow symlinks)
/// - **macOS**: Uses `getxattr()` with `XATTR_NOFOLLOW` flag
///
/// # Arguments
///
/// * `path` - Path to the file or symlink
/// * `name` - Name of the extended attribute
///
/// # Returns
///
/// The value of the extended attribute on the symlink itself
///
/// # Errors
///
/// This function will return an error if:
/// - The extended attribute doesn't exist
/// - Permission is denied
/// - The operation fails due to I/O errors
#[cfg(unix)]
pub async fn lget_xattr_at_path(path: &Path, name: &str) -> Result<Vec<u8>> {
    let path_cstr = std::ffi::CString::new(path.to_string_lossy().as_bytes())
        .map_err(|e| xattr_error(&format!("Invalid path: {}", e)))?;
    let name_cstr =
        std::ffi::CString::new(name).map_err(|e| xattr_error(&format!("Invalid name: {}", e)))?;

    // Get the size first (using l* variant or NOFOLLOW flag)
    let size = unsafe {
        #[cfg(target_os = "macos")]
        {
            libc::getxattr(
                path_cstr.as_ptr(),
                name_cstr.as_ptr(),
                std::ptr::null_mut(),
                0,
                0,              // position
                XATTR_NOFOLLOW, // options: don't follow symlinks
            )
        }
        #[cfg(target_os = "linux")]
        {
            libc::lgetxattr(
                path_cstr.as_ptr(),
                name_cstr.as_ptr(),
                std::ptr::null_mut(),
                0,
            )
        }
        #[cfg(all(unix, not(any(target_os = "macos", target_os = "linux"))))]
        {
            // Fallback for other Unix: try to use lgetxattr if available
            libc::lgetxattr(
                path_cstr.as_ptr(),
                name_cstr.as_ptr(),
                std::ptr::null_mut(),
                0,
            )
        }
    };

    if size < 0 {
        let errno = std::io::Error::last_os_error();
        return Err(xattr_error(&format!("lgetxattr failed: {}", errno)));
    }

    // Allocate buffer and get the value
    let mut buffer = vec![0u8; size as usize];
    let actual_size = unsafe {
        #[cfg(target_os = "macos")]
        {
            libc::getxattr(
                path_cstr.as_ptr(),
                name_cstr.as_ptr(),
                buffer.as_mut_ptr() as *mut libc::c_void,
                buffer.len(),
                0,              // position
                XATTR_NOFOLLOW, // options: don't follow symlinks
            )
        }
        #[cfg(target_os = "linux")]
        {
            libc::lgetxattr(
                path_cstr.as_ptr(),
                name_cstr.as_ptr(),
                buffer.as_mut_ptr() as *mut libc::c_void,
                buffer.len(),
            )
        }
        #[cfg(all(unix, not(any(target_os = "macos", target_os = "linux"))))]
        {
            libc::lgetxattr(
                path_cstr.as_ptr(),
                name_cstr.as_ptr(),
                buffer.as_mut_ptr() as *mut libc::c_void,
                buffer.len(),
            )
        }
    };

    if actual_size < 0 {
        let errno = std::io::Error::last_os_error();
        return Err(xattr_error(&format!("lgetxattr failed: {}", errno)));
    }

    buffer.truncate(actual_size as usize);
    Ok(buffer)
}

// Windows: lget_xattr_at_path not defined - compile-time error

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
#[cfg(unix)]
pub async fn set_xattr_at_path(path: &Path, name: &str, value: &[u8]) -> Result<()> {
    let path_cstr = std::ffi::CString::new(path.to_string_lossy().as_bytes())
        .map_err(|e| xattr_error(&format!("Invalid path: {}", e)))?;
    let name_cstr =
        std::ffi::CString::new(name).map_err(|e| xattr_error(&format!("Invalid name: {}", e)))?;

    let result = unsafe {
        #[cfg(target_os = "macos")]
        {
            libc::setxattr(
                path_cstr.as_ptr(),
                name_cstr.as_ptr(),
                value.as_ptr() as *const libc::c_void,
                value.len(),
                0, // position: offset to start writing (0 = from beginning)
                0, // options: flags like XATTR_CREATE/XATTR_REPLACE (0 = defaults)
            )
        }
        #[cfg(not(target_os = "macos"))]
        {
            libc::setxattr(
                path_cstr.as_ptr(),
                name_cstr.as_ptr(),
                value.as_ptr() as *const libc::c_void,
                value.len(),
                0, // flags
            )
        }
    };

    if result != 0 {
        let errno = std::io::Error::last_os_error();
        return Err(xattr_error(&format!("setxattr failed: {}", errno)));
    }

    Ok(())
}

// Windows: set_xattr_at_path not defined - compile-time error

/// Set an extended attribute value (symlink version - doesn't follow symlinks)
///
/// Like `set_xattr_at_path` but operates on the symlink itself, not its target.
///
/// # Implementation
///
/// - **Linux**: Uses `lsetxattr()` (the "l" version doesn't follow symlinks)
/// - **macOS**: Uses `setxattr()` with `XATTR_NOFOLLOW` flag
///
/// # Arguments
///
/// * `path` - Path to the file or symlink
/// * `name` - Name of the extended attribute
/// * `value` - Value to set
///
/// # Errors
///
/// This function will return an error if:
/// - Permission is denied
/// - The operation fails due to I/O errors
#[cfg(unix)]
pub async fn lset_xattr_at_path(path: &Path, name: &str, value: &[u8]) -> Result<()> {
    let path_cstr = std::ffi::CString::new(path.to_string_lossy().as_bytes())
        .map_err(|e| xattr_error(&format!("Invalid path: {}", e)))?;
    let name_cstr =
        std::ffi::CString::new(name).map_err(|e| xattr_error(&format!("Invalid name: {}", e)))?;

    let result = unsafe {
        #[cfg(target_os = "macos")]
        {
            libc::setxattr(
                path_cstr.as_ptr(),
                name_cstr.as_ptr(),
                value.as_ptr() as *const libc::c_void,
                value.len(),
                0,              // position
                XATTR_NOFOLLOW, // options: don't follow symlinks
            )
        }
        #[cfg(target_os = "linux")]
        {
            libc::lsetxattr(
                path_cstr.as_ptr(),
                name_cstr.as_ptr(),
                value.as_ptr() as *const libc::c_void,
                value.len(),
                0, // flags
            )
        }
        #[cfg(all(unix, not(any(target_os = "macos", target_os = "linux"))))]
        {
            libc::lsetxattr(
                path_cstr.as_ptr(),
                name_cstr.as_ptr(),
                value.as_ptr() as *const libc::c_void,
                value.len(),
                0, // flags
            )
        }
    };

    if result != 0 {
        let errno = std::io::Error::last_os_error();
        return Err(xattr_error(&format!("lsetxattr failed: {}", errno)));
    }

    Ok(())
}

// Windows: lset_xattr_at_path not defined - compile-time error

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
#[cfg(unix)]
pub async fn list_xattr_at_path(path: &Path) -> Result<Vec<String>> {
    let path_cstr = std::ffi::CString::new(path.to_string_lossy().as_bytes())
        .map_err(|e| xattr_error(&format!("Invalid path: {}", e)))?;

    // Get the size first
    // Platform-specific: macOS listxattr takes 4 args (adds options=0), Linux takes 3
    let size = unsafe {
        #[cfg(target_os = "macos")]
        {
            libc::listxattr(
                path_cstr.as_ptr(),
                std::ptr::null_mut(),
                0,
                0, /* options */
            )
        }
        #[cfg(not(target_os = "macos"))]
        {
            libc::listxattr(path_cstr.as_ptr(), std::ptr::null_mut(), 0)
        }
    };

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
        #[cfg(target_os = "macos")]
        {
            libc::listxattr(
                path_cstr.as_ptr(),
                buffer.as_mut_ptr() as *mut libc::c_char,
                buffer.len(),
                0, // options
            )
        }
        #[cfg(not(target_os = "macos"))]
        {
            libc::listxattr(
                path_cstr.as_ptr(),
                buffer.as_mut_ptr() as *mut libc::c_char,
                buffer.len(),
            )
        }
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

// Windows: list_xattr_at_path not defined - compile-time error

/// List all extended attributes (symlink version - doesn't follow symlinks)
///
/// Like `list_xattr_at_path` but operates on the symlink itself, not its target.
///
/// # Implementation
///
/// - **Linux**: Uses `llistxattr()` (the "l" version doesn't follow symlinks)
/// - **macOS**: Uses `listxattr()` with `XATTR_NOFOLLOW` flag
///
/// # Arguments
///
/// * `path` - Path to the file or symlink
///
/// # Returns
///
/// A vector of extended attribute names on the symlink itself
///
/// # Errors
///
/// This function will return an error if:
/// - Permission is denied
/// - The operation fails due to I/O errors
#[cfg(unix)]
pub async fn llist_xattr_at_path(path: &Path) -> Result<Vec<String>> {
    let path_cstr = std::ffi::CString::new(path.to_string_lossy().as_bytes())
        .map_err(|e| xattr_error(&format!("Invalid path: {}", e)))?;

    // Get the size first
    let size = unsafe {
        #[cfg(target_os = "macos")]
        {
            libc::listxattr(path_cstr.as_ptr(), std::ptr::null_mut(), 0, XATTR_NOFOLLOW)
        }
        #[cfg(target_os = "linux")]
        {
            libc::llistxattr(path_cstr.as_ptr(), std::ptr::null_mut(), 0)
        }
        #[cfg(all(unix, not(any(target_os = "macos", target_os = "linux"))))]
        {
            libc::llistxattr(path_cstr.as_ptr(), std::ptr::null_mut(), 0)
        }
    };

    if size < 0 {
        let errno = std::io::Error::last_os_error();
        return Err(xattr_error(&format!("llistxattr failed: {}", errno)));
    }

    if size == 0 {
        return Ok(Vec::new());
    }

    // Allocate buffer and get the list
    let mut buffer = vec![0u8; size as usize];
    let actual_size = unsafe {
        #[cfg(target_os = "macos")]
        {
            libc::listxattr(
                path_cstr.as_ptr(),
                buffer.as_mut_ptr() as *mut libc::c_char,
                buffer.len(),
                XATTR_NOFOLLOW,
            )
        }
        #[cfg(target_os = "linux")]
        {
            libc::llistxattr(
                path_cstr.as_ptr(),
                buffer.as_mut_ptr() as *mut libc::c_char,
                buffer.len(),
            )
        }
        #[cfg(all(unix, not(any(target_os = "macos", target_os = "linux"))))]
        {
            libc::llistxattr(
                path_cstr.as_ptr(),
                buffer.as_mut_ptr() as *mut libc::c_char,
                buffer.len(),
            )
        }
    };

    if actual_size < 0 {
        let errno = std::io::Error::last_os_error();
        return Err(xattr_error(&format!("llistxattr failed: {}", errno)));
    }

    // Parse the null-separated list
    let mut names = Vec::new();
    let mut start = 0;
    for i in 0..actual_size as usize {
        if buffer[i] == 0 {
            if i > start {
                if let Ok(name) = std::str::from_utf8(&buffer[start..i]) {
                    names.push(name.to_string());
                }
            }
            start = i + 1;
        }
    }

    Ok(names)
}

// Windows: llist_xattr_at_path not defined - compile-time error

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
#[cfg(unix)]
pub async fn remove_xattr_at_path(path: &Path, name: &str) -> Result<()> {
    let path_cstr = std::ffi::CString::new(path.to_string_lossy().as_bytes())
        .map_err(|e| xattr_error(&format!("Invalid path: {}", e)))?;
    let name_cstr =
        std::ffi::CString::new(name).map_err(|e| xattr_error(&format!("Invalid name: {}", e)))?;

    // Platform-specific: macOS removexattr takes 3 args (adds options=0), Linux takes 2
    let result = unsafe {
        #[cfg(target_os = "macos")]
        {
            libc::removexattr(path_cstr.as_ptr(), name_cstr.as_ptr(), 0 /* options */)
        }
        #[cfg(not(target_os = "macos"))]
        {
            libc::removexattr(path_cstr.as_ptr(), name_cstr.as_ptr())
        }
    };

    if result != 0 {
        let errno = std::io::Error::last_os_error();
        return Err(xattr_error(&format!("removexattr failed: {}", errno)));
    }

    Ok(())
}

// Windows: remove_xattr_at_path not defined - compile-time error

/// Remove an extended attribute (symlink version - doesn't follow symlinks)
///
/// Like `remove_xattr_at_path` but operates on the symlink itself, not its target.
///
/// # Implementation
///
/// - **Linux**: Uses `lremovexattr()` (the "l" version doesn't follow symlinks)
/// - **macOS**: Uses `removexattr()` with `XATTR_NOFOLLOW` flag
///
/// # Arguments
///
/// * `path` - Path to the file or symlink
/// * `name` - Name of the extended attribute to remove
///
/// # Errors
///
/// This function will return an error if:
/// - The extended attribute doesn't exist
/// - Permission is denied
/// - The operation fails due to I/O errors
#[cfg(unix)]
pub async fn lremove_xattr_at_path(path: &Path, name: &str) -> Result<()> {
    let path_cstr = std::ffi::CString::new(path.to_string_lossy().as_bytes())
        .map_err(|e| xattr_error(&format!("Invalid path: {}", e)))?;
    let name_cstr =
        std::ffi::CString::new(name).map_err(|e| xattr_error(&format!("Invalid name: {}", e)))?;

    let result = unsafe {
        #[cfg(target_os = "macos")]
        {
            libc::removexattr(path_cstr.as_ptr(), name_cstr.as_ptr(), XATTR_NOFOLLOW)
        }
        #[cfg(target_os = "linux")]
        {
            libc::lremovexattr(path_cstr.as_ptr(), name_cstr.as_ptr())
        }
        #[cfg(all(unix, not(any(target_os = "macos", target_os = "linux"))))]
        {
            libc::lremovexattr(path_cstr.as_ptr(), name_cstr.as_ptr())
        }
    };

    if result != 0 {
        let errno = std::io::Error::last_os_error();
        return Err(xattr_error(&format!("lremovexattr failed: {}", errno)));
    }

    Ok(())
}

// Windows: lremove_xattr_at_path not defined - compile-time error

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    #[cfg(unix)]
    async fn test_xattr_operations() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        // Create test file
        fs::write(&file_path, "test content").unwrap();

        // Try xattr operations - they may fail if filesystem doesn't support xattrs
        if let Ok(()) = set_xattr_at_path(&file_path, "user.test", b"test_value").await {
            // Test get
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
            println!("Extended attributes not supported on this filesystem - test skipped");
        }
    }
}
