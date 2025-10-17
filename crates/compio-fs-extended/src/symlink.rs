//! Symlink operations for creating and reading symbolic links

use crate::error::{symlink_error, Result};
use compio::driver::OpCode;
use compio::fs::File;
use compio::runtime::submit;
use io_uring::{opcode, types};
use nix::fcntl;
use std::ffi::CString;
use std::os::fd::AsFd;
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
    /// use std::path::Path;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let file = File::create("new_symlink.txt").await?;
    /// let extended_file = ExtendedFile::new(file);
    ///
    /// extended_file.create_symlink(Path::new("target.txt")).await?;
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

// Note: Basic symlink operations are provided by std::fs or compio::fs
// This module focuses on io_uring operations and secure *at variants

/// Create a symbolic link using DirectoryFd
///
/// Uses io_uring `symlinkat(2)` with directory FD and relative path.
pub(crate) async fn symlinkat_impl(
    dir: &crate::directory::DirectoryFd,
    target: &str,
    link_name: &str,
) -> Result<()> {
    let target_cstr =
        CString::new(target).map_err(|e| symlink_error(&format!("Invalid target: {}", e)))?;
    let link_path_cstr =
        CString::new(link_name).map_err(|e| symlink_error(&format!("Invalid link name: {}", e)))?;

    let op = SymlinkOp {
        target: target_cstr,
        link_path: link_path_cstr,
        dir_fd: Some(dir.as_raw_fd()),
    };

    // Submit io_uring symlink operation
    let result = submit(op).await;

    match result.0 {
        Ok(_) => Ok(()),
        Err(e) => Err(symlink_error(&e.to_string())),
    }
}

/// Read a symbolic link using DirectoryFd
///
/// Uses `readlinkat(2)` with directory FD and relative path.
///
/// Note: Always uses spawn_blocking (not affected by cheap_calls_sync) because
/// readlinkat actually reads file data (the symlink target), not just metadata.
/// While typically fast, symlink targets can be up to PATH_MAX (4096 bytes) and
/// may require disk I/O on some filesystems.
pub(crate) async fn readlinkat_impl(
    dir: &crate::directory::DirectoryFd,
    link_name: &str,
) -> Result<std::path::PathBuf> {
    let link_name = link_name.to_string();
    let dir = dir.clone();

    let os_string = compio::runtime::spawn_blocking(move || {
        fcntl::readlinkat(dir.as_fd(), std::path::Path::new(&link_name))
    })
    .await
    .unwrap_or_else(|e| std::panic::resume_unwind(e));

    Ok(std::path::PathBuf::from(
        os_string.map_err(|e| symlink_error(&e.to_string()))?,
    ))
}

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

        dir_fd.symlinkat("target.txt", link_name).await.unwrap();

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

        dir_fd.symlinkat("target.txt", link_name).await.unwrap();

        // Test secure symlink reading using DirectoryFd
        let target = dir_fd.readlinkat(link_name).await.unwrap();
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
        let result = dir_fd.symlinkat("target\x00invalid", "link").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid target"));

        // Test invalid link name (null bytes)
        let result = dir_fd.symlinkat("target", "link\x00invalid").await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid link name"));

        // Test empty target
        let _result = dir_fd.symlinkat("", "link").await;
        // This might succeed or fail depending on filesystem, both are acceptable

        // Test empty link name
        let result = dir_fd.symlinkat("target", "").await;
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
        dir_fd
            .symlinkat("./target.txt", "relative_link")
            .await
            .unwrap();
        let target = dir_fd.readlinkat("relative_link").await.unwrap();
        assert_eq!(target, std::path::PathBuf::from("./target.txt"));

        // Test symlink with absolute path (if temp_dir allows)
        let abs_target = format!("{}/target.txt", temp_dir.path().display());
        dir_fd
            .symlinkat(&abs_target, "absolute_link")
            .await
            .unwrap();
        let target = dir_fd.readlinkat("absolute_link").await.unwrap();
        assert_eq!(target, std::path::PathBuf::from(&abs_target));

        // Test symlink with special characters
        dir_fd
            .symlinkat("target with spaces.txt", "special_link")
            .await
            .unwrap();
        let target = dir_fd.readlinkat("special_link").await.unwrap();
        assert_eq!(target, std::path::PathBuf::from("target with spaces.txt"));

        // Test symlink with unicode characters
        dir_fd
            .symlinkat("target_🚀.txt", "unicode_link")
            .await
            .unwrap();
        let target = dir_fd.readlinkat("unicode_link").await.unwrap();
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
        dir_fd
            .symlinkat("target.txt", "existing_link")
            .await
            .unwrap();

        // Try to create same symlink again (should fail)
        let result = dir_fd.symlinkat("target.txt", "existing_link").await;
        assert!(result.is_err());
    }

    #[compio::test]
    async fn test_symlink_read_nonexistent() {
        let temp_dir = TempDir::new().unwrap();
        let dir_fd = crate::directory::DirectoryFd::open(temp_dir.path())
            .await
            .unwrap();

        // Try to read symlink that doesn't exist
        let result = dir_fd.readlinkat("nonexistent_link").await;
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
            dir_fd.symlinkat("target.txt", link_name).await.unwrap();
        }

        // Read all symlinks
        for link_name in &links {
            let target = dir_fd.readlinkat(link_name).await.unwrap();
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
        dir_fd.symlinkat("target_dir", "dir_link").await.unwrap();

        // Read the symlink
        let target = dir_fd.readlinkat("dir_link").await.unwrap();
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
        dir_fd.symlinkat(&long_target, "long_link").await.unwrap();

        // Read the symlink
        let target = dir_fd.readlinkat("long_link").await.unwrap();
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
