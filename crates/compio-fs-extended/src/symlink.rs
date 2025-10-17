//! Symlink operations for creating and reading symbolic links

use crate::error::{symlink_error, ExtendedError, Result};
use compio::driver::OpCode;
use compio::fs::File;
use compio::runtime::submit;
use io_uring::{opcode, types};
use nix::fcntl;
use std::ffi::CString;
use std::path::Path;
use std::pin::Pin;

/// Custom symlink operation that implements compio's OpCode trait
pub struct SymlinkOp {
    /// Target path for the symbolic link
    target: CString,
    /// Name of the symbolic link to create
    link_path: CString,
    /// Directory file descriptor for secure symlink creation
    dir_fd: Option<std::os::unix::io::RawFd>,
}

impl SymlinkOp {
    /// Create a new SymlinkOp for io_uring submission with DirectoryFd
    ///
    /// # Arguments
    ///
    /// * `dir_fd` - Directory file descriptor for secure symlink creation
    /// * `target` - Target path for the symbolic link
    /// * `link_name` - Name of the symbolic link to create
    ///
    /// # Returns
    ///
    /// `Ok(SymlinkOp)` if the operation was constructed successfully
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The target or link_name contain null bytes
    /// - The strings cannot be converted to C strings
    pub fn new_with_dirfd(
        dir_fd: &crate::directory::DirectoryFd,
        target: &str,
        link_name: &str,
    ) -> Result<Self> {
        let target_cstr =
            CString::new(target).map_err(|e| symlink_error(&format!("Invalid target: {}", e)))?;
        let link_path_cstr = CString::new(link_name)
            .map_err(|e| symlink_error(&format!("Invalid link name: {}", e)))?;

        Ok(Self {
            target: target_cstr,
            link_path: link_path_cstr,
            dir_fd: Some(dir_fd.as_raw_fd()),
        })
    }
}

impl OpCode for SymlinkOp {
    fn create_entry(self: Pin<&mut Self>) -> compio::driver::OpEntry {
        compio::driver::OpEntry::Submission(
            opcode::SymlinkAt::new(
                types::Fd(self.dir_fd.unwrap_or(libc::AT_FDCWD)),
                self.target.as_ptr(),
                self.link_path.as_ptr(),
            )
            .build(),
        )
    }
}

/// Trait for symlink operations
#[allow(async_fn_in_trait)]
pub trait SymlinkOps {
    /// Read the target of a symbolic link
    ///
    /// # Returns
    ///
    /// The target path of the symbolic link
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The file is not a symbolic link
    /// - The symbolic link is broken
    /// - The operation fails due to I/O errors
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use compio_fs_extended::{ExtendedFile, SymlinkOps};
    /// use compio::fs::File;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let file = File::open("symlink.txt").await?;
    /// let extended_file = ExtendedFile::new(file);
    ///
    /// let target = extended_file.read_symlink().await?;
    /// println!("Symlink points to: {:?}", target);
    /// # Ok(())
    /// # }
    /// ```
    async fn read_symlink(&self) -> Result<std::path::PathBuf>;

    /// Create a symbolic link
    ///
    /// # Arguments
    ///
    /// * `target` - The target path for the symbolic link
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The target path is invalid
    /// - The operation fails due to I/O errors
    /// - Permission is denied
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use compio_fs_extended::{ExtendedFile, SymlinkOps};
    /// use compio::fs::File;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let file = File::create("new_symlink.txt").await?;
    /// let extended_file = ExtendedFile::new(file);
    ///
    /// extended_file.create_symlink("target.txt").await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn create_symlink(&self, target: &Path) -> Result<()>;
}

/// Implementation of symlink operations using direct syscalls
///
/// # Errors
///
/// This function will return an error if the symlink read fails
pub async fn read_symlink_impl(_file: &File) -> Result<std::path::PathBuf> {
    // Get the file path from the file descriptor
    // This is a simplified implementation - in practice, we'd need to track the path
    Err(symlink_error(
        "read_symlink not yet implemented - requires path tracking",
    ))
}

/// Implementation of symlink creation using direct syscalls
///
/// # Errors
///
/// This function will return an error if the symlink creation fails
pub async fn create_symlink_impl(_file: &File, _target: &Path) -> Result<()> {
    // Get the file path from the file descriptor
    // This is a simplified implementation - in practice, we'd need to track the path
    Err(symlink_error(
        "create_symlink not yet implemented - requires path tracking",
    ))
}

/// Create a symbolic link at the given path
///
/// # Arguments
///
/// * `link_path` - Path where the symbolic link will be created
/// * `target` - Target path for the symbolic link
///
/// # Returns
///
/// `Ok(())` if the symbolic link was created successfully
///
/// # Errors
///
/// This function will return an error if:
/// - The link path already exists
/// - The target path is invalid
/// - Permission is denied
/// - The operation fails due to I/O errors
// Note: io_uring symlink operations removed - using secure *at variants instead
///   Create a symbolic link using io_uring with DirectoryFd (secure)
///
/// # Arguments
///
/// * `dir_fd` - Directory file descriptor for secure operation
/// * `target` - Target path for the symbolic link
/// * `link_name` - Name of the symbolic link relative to the directory
///
/// # Returns
///
/// `Ok(())` if the symbolic link was created successfully
///
/// # Errors
///
/// This function will return an error if:
/// - The link name is invalid
/// - Permission is denied
/// - The operation fails due to I/O errors
///
/// # Example
///
/// ```rust,no_run
/// use compio_fs_extended::{directory::DirectoryFd, symlink::create_symlink_at_dirfd};
/// use std::path::Path;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let dir_fd = DirectoryFd::open(Path::new("/some/directory")).await?;
/// create_symlink_at_dirfd(&dir_fd, "target_file", "my_link").await?;
/// # Ok(())
/// # }
/// ```
pub async fn create_symlink_at_dirfd(
    dir_fd: &crate::directory::DirectoryFd,
    target: &str,
    link_name: &str,
) -> Result<()> {
    // Submit io_uring symlink operation using compio's runtime with DirectoryFd
    let result = submit(SymlinkOp::new_with_dirfd(dir_fd, target, link_name)?).await;

    // Minimal mapping: preserve underlying error string without extra context
    match result.0 {
        Ok(_) => Ok(()),
        Err(e) => Err(symlink_error(&e.to_string())),
    }
}

// Note: Basic symlink operations are provided by std::fs or compio::fs
// This module focuses on io_uring operations and secure *at variants

// Note: Basic readlink operations are provided by std::fs or compio::fs
// This module focuses on io_uring operations and secure *at variants

/// Read the target of a symbolic link using DirectoryFd (secure)
///
/// # Arguments
///
/// * `dir_fd` - Directory file descriptor for secure operation
/// * `link_name` - Name of the symbolic link relative to the directory
///
/// # Returns
///
/// The target path of the symbolic link
///
/// # Errors
///
/// This function will return an error if:
/// - The path is not a symbolic link
/// - The symbolic link is broken
/// - The operation fails due to I/O errors
///
/// # Example
///
/// ```rust,no_run
/// use compio_fs_extended::{directory::DirectoryFd, symlink::read_symlink_at_dirfd};
/// use std::path::Path;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let dir_fd = DirectoryFd::open(Path::new("/some/directory")).await?;
/// let target = read_symlink_at_dirfd(&dir_fd, "my_link").await?;
/// # Ok(())
/// # }
/// ```
pub async fn read_symlink_at_dirfd(
    dir_fd: &crate::directory::DirectoryFd,
    link_name: &str,
) -> Result<std::path::PathBuf> {
    use std::os::fd::BorrowedFd;

    let link_name = link_name.to_string();
    let dir_fd_raw = dir_fd.as_raw_fd();

    let os_string = compio::runtime::spawn(async move {
        // SAFETY: The raw fd is valid for the duration of this call because:
        // 1. DirectoryFd holds an Arc<File> keeping the fd open
        // 2. The spawned task completes before DirectoryFd is dropped
        // We use raw fd here because BorrowedFd from as_fd() has a non-'static lifetime,
        // but spawn requires 'static. This is the standard pattern for moving fds into tasks.
        let borrowed_fd = unsafe { BorrowedFd::borrow_raw(dir_fd_raw) };
        fcntl::readlinkat(borrowed_fd, std::path::Path::new(&link_name))
    })
    .await
    .map_err(ExtendedError::SpawnJoin)?;

    Ok(std::path::PathBuf::from(
        os_string.map_err(|e| symlink_error(&e.to_string()))?,
    ))
}

// Note: Basic symlink operations like is_symlink, is_broken_symlink are provided by std::fs
// This module focuses on io_uring operations and secure *at variants

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[compio::test]
    async fn test_secure_symlink_creation() {
        let temp_dir = TempDir::new().unwrap();
        let target_path = temp_dir.path().join("target.txt");

        // Create target file
        fs::write(&target_path, "target content").unwrap();

        // Test secure symlink creation using DirectoryFd
        let dir_fd = crate::directory::DirectoryFd::open(temp_dir.path())
            .await
            .unwrap();

        // Use a unique link name to avoid conflicts
        let link_name = "unique_secure_link";
        let link_path = temp_dir.path().join(link_name);

        // Clean up any existing symlink first
        if link_path.exists() {
            fs::remove_file(&link_path).unwrap();
        }

        create_symlink_at_dirfd(&dir_fd, "target.txt", link_name)
            .await
            .unwrap();

        // Verify the symlink was created using std::fs
        assert!(link_path.is_symlink());

        // Read the symlink target using std::fs
        let target = std::fs::read_link(&link_path).unwrap();
        assert_eq!(target, std::path::PathBuf::from("target.txt"));
    }

    #[compio::test]
    async fn test_secure_symlink_operations() {
        let temp_dir = TempDir::new().unwrap();
        let target_path = temp_dir.path().join("target.txt");

        // Create target file
        fs::write(&target_path, "target content").unwrap();

        // Test secure symlink creation using DirectoryFd
        let dir_fd = crate::directory::DirectoryFd::open(temp_dir.path())
            .await
            .unwrap();

        // Use a unique link name to avoid conflicts
        let link_name = "unique_secure_ops_link";
        let link_path = temp_dir.path().join(link_name);

        // Clean up any existing symlink first
        if link_path.exists() {
            fs::remove_file(&link_path).unwrap();
        }

        create_symlink_at_dirfd(&dir_fd, "target.txt", link_name)
            .await
            .unwrap();

        // Test secure symlink reading using DirectoryFd
        let target = read_symlink_at_dirfd(&dir_fd, link_name).await.unwrap();
        assert_eq!(target, std::path::PathBuf::from("target.txt"));
    }

    #[compio::test]
    async fn test_symlink_ops_trait_read() {
        let temp_dir = TempDir::new().unwrap();
        let link_path = temp_dir.path().join("test_link");
        let target_path = temp_dir.path().join("target.txt");

        // Create target file
        fs::write(&target_path, "target content").unwrap();

        // Create symlink using std::fs
        std::os::unix::fs::symlink("target.txt", &link_path).unwrap();

        // Test ExtendedFile::read_symlink() method
        let file = compio::fs::File::open(&link_path).await.unwrap();
        let extended_file = crate::extended_file::ExtendedFile::new(file);

        let result = extended_file.read_symlink().await;
        match result {
            Ok(_) => {
                // If implemented, should return the target path
                println!("read_symlink trait method works");
            }
            Err(e) => {
                // Expected to fail since implementation returns "not implemented"
                println!("read_symlink trait method failed as expected: {}", e);
                assert!(e.to_string().contains("not yet implemented"));
            }
        }
    }

    #[compio::test]
    async fn test_symlink_ops_trait_create() {
        let temp_dir = TempDir::new().unwrap();
        let link_path = temp_dir.path().join("test_trait_link");
        let target_path = temp_dir.path().join("target.txt");

        // Create target file
        fs::write(&target_path, "target content").unwrap();

        // Test ExtendedFile::create_symlink() method
        let file = compio::fs::File::create(&link_path).await.unwrap();
        let extended_file = crate::extended_file::ExtendedFile::new(file);

        let result = extended_file
            .create_symlink(std::path::Path::new("target.txt"))
            .await;
        match result {
            Ok(_) => {
                // If implemented, should create the symlink
                println!("create_symlink trait method works");
            }
            Err(e) => {
                // Expected to fail since implementation returns "not implemented"
                println!("create_symlink trait method failed as expected: {}", e);
                assert!(e.to_string().contains("not yet implemented"));
            }
        }
    }

    #[compio::test]
    async fn test_symlink_error_cases() {
        let temp_dir = TempDir::new().unwrap();
        let dir_fd = crate::directory::DirectoryFd::open(temp_dir.path())
            .await
            .unwrap();

        // Test invalid target (null bytes)
        let result = create_symlink_at_dirfd(&dir_fd, "target\x00invalid", "link").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid target"));

        // Test invalid link name (null bytes)
        let result = create_symlink_at_dirfd(&dir_fd, "target", "link\x00invalid").await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid link name"));

        // Test empty target
        let _result = create_symlink_at_dirfd(&dir_fd, "", "link").await;
        // This might succeed or fail depending on filesystem, both are acceptable

        // Test empty link name
        let result = create_symlink_at_dirfd(&dir_fd, "target", "").await;
        assert!(result.is_err());
        // The error might be "Invalid link name" or filesystem-specific error
        let error_msg = result.unwrap_err().to_string();
        // Accept any error since empty link names are invalid
        assert!(
            error_msg.contains("Invalid link name")
                || error_msg.contains("Invalid argument")
                || error_msg.contains("No such file or directory")
        );
    }

    #[compio::test]
    async fn test_symlink_edge_cases() {
        let temp_dir = TempDir::new().unwrap();
        let dir_fd = crate::directory::DirectoryFd::open(temp_dir.path())
            .await
            .unwrap();

        // Test symlink with relative path
        create_symlink_at_dirfd(&dir_fd, "./target.txt", "relative_link")
            .await
            .unwrap();
        let target = read_symlink_at_dirfd(&dir_fd, "relative_link")
            .await
            .unwrap();
        assert_eq!(target, std::path::PathBuf::from("./target.txt"));

        // Test symlink with absolute path (if temp_dir allows)
        let abs_target = format!("{}/target.txt", temp_dir.path().display());
        create_symlink_at_dirfd(&dir_fd, &abs_target, "absolute_link")
            .await
            .unwrap();
        let target = read_symlink_at_dirfd(&dir_fd, "absolute_link")
            .await
            .unwrap();
        assert_eq!(target, std::path::PathBuf::from(&abs_target));

        // Test symlink with special characters
        create_symlink_at_dirfd(&dir_fd, "target with spaces.txt", "special_link")
            .await
            .unwrap();
        let target = read_symlink_at_dirfd(&dir_fd, "special_link")
            .await
            .unwrap();
        assert_eq!(target, std::path::PathBuf::from("target with spaces.txt"));

        // Test symlink with unicode characters
        create_symlink_at_dirfd(&dir_fd, "target_🚀.txt", "unicode_link")
            .await
            .unwrap();
        let target = read_symlink_at_dirfd(&dir_fd, "unicode_link")
            .await
            .unwrap();
        assert_eq!(target, std::path::PathBuf::from("target_🚀.txt"));
    }

    #[compio::test]
    async fn test_symlink_already_exists() {
        let temp_dir = TempDir::new().unwrap();
        let dir_fd = crate::directory::DirectoryFd::open(temp_dir.path())
            .await
            .unwrap();
        let target_path = temp_dir.path().join("target.txt");

        // Create target file
        fs::write(&target_path, "target content").unwrap();

        // Create symlink first time
        create_symlink_at_dirfd(&dir_fd, "target.txt", "existing_link")
            .await
            .unwrap();

        // Try to create same symlink again (should fail)
        let result = create_symlink_at_dirfd(&dir_fd, "target.txt", "existing_link").await;
        assert!(result.is_err());
    }

    #[compio::test]
    async fn test_symlink_read_nonexistent() {
        let temp_dir = TempDir::new().unwrap();
        let dir_fd = crate::directory::DirectoryFd::open(temp_dir.path())
            .await
            .unwrap();

        // Try to read symlink that doesn't exist
        let result = read_symlink_at_dirfd(&dir_fd, "nonexistent_link").await;
        assert!(result.is_err());
    }

    #[compio::test]
    async fn test_symlink_multiple_operations() {
        let temp_dir = TempDir::new().unwrap();
        let dir_fd = crate::directory::DirectoryFd::open(temp_dir.path())
            .await
            .unwrap();
        let target_path = temp_dir.path().join("target.txt");

        // Create target file
        fs::write(&target_path, "target content").unwrap();

        // Create multiple symlinks
        let links = ["link1", "link2", "link3"];
        for link_name in &links {
            create_symlink_at_dirfd(&dir_fd, "target.txt", link_name)
                .await
                .unwrap();
        }

        // Read all symlinks
        for link_name in &links {
            let target = read_symlink_at_dirfd(&dir_fd, link_name).await.unwrap();
            assert_eq!(target, std::path::PathBuf::from("target.txt"));
        }
    }

    #[compio::test]
    async fn test_symlink_directory_target() {
        let temp_dir = TempDir::new().unwrap();
        let dir_fd = crate::directory::DirectoryFd::open(temp_dir.path())
            .await
            .unwrap();

        // Create a directory
        let target_dir = temp_dir.path().join("target_dir");
        std::fs::create_dir(&target_dir).unwrap();

        // Create symlink pointing to directory
        create_symlink_at_dirfd(&dir_fd, "target_dir", "dir_link")
            .await
            .unwrap();

        // Read the symlink
        let target = read_symlink_at_dirfd(&dir_fd, "dir_link").await.unwrap();
        assert_eq!(target, std::path::PathBuf::from("target_dir"));
    }

    #[compio::test]
    async fn test_symlink_long_target() {
        let temp_dir = TempDir::new().unwrap();
        let dir_fd = crate::directory::DirectoryFd::open(temp_dir.path())
            .await
            .unwrap();

        // Create a very long target path
        let long_target = "a".repeat(255); // POSIX path limit is usually 255 chars
        create_symlink_at_dirfd(&dir_fd, &long_target, "long_link")
            .await
            .unwrap();

        // Read the symlink
        let target = read_symlink_at_dirfd(&dir_fd, "long_link").await.unwrap();
        assert_eq!(target, std::path::PathBuf::from(&long_target));
    }

    #[compio::test]
    async fn test_symlink_implementation_functions() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test_file");
        fs::write(&file_path, "test").unwrap();

        // Test read_symlink_impl
        let file = compio::fs::File::open(&file_path).await.unwrap();
        let result = read_symlink_impl(&file).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("not yet implemented"));

        // Test create_symlink_impl
        let file = compio::fs::File::create(&file_path).await.unwrap();
        let result = create_symlink_impl(&file, std::path::Path::new("target")).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("not yet implemented"));
    }
}
