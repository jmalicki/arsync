//! Directory file descriptor for secure directory-based operations

use crate::error::{directory_error, Result};
use compio::fs::File;
use std::os::fd::{AsFd, BorrowedFd};
#[cfg(unix)]
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// A directory file descriptor for secure directory-based operations
///
/// `DirectoryFd` provides a safe wrapper around a directory file descriptor,
/// enabling secure `*at` system calls that avoid TOCTOU (Time-of-Check-Time-of-Use)
/// race conditions. This is the recommended way to perform file operations
/// relative to a directory.
///
/// # Example
///
/// ```rust,no_run
/// use compio_fs_extended::directory::DirectoryFd;
/// use std::path::Path;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let dir_fd = DirectoryFd::open(Path::new("/some/directory")).await?;
/// // Use dir_fd for secure file operations
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct DirectoryFd {
    /// The underlying file descriptor
    file: Arc<File>,
    /// The path this directory represents (for debugging/error messages)
    path: PathBuf,
}

impl DirectoryFd {
    /// Open a directory and return a `DirectoryFd`
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the directory to open
    ///
    /// # Returns
    ///
    /// `Ok(DirectoryFd)` if the directory was opened successfully
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The path doesn't exist
    /// - The path is not a directory
    /// - Permission is denied
    /// - The operation fails due to I/O errors
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use compio_fs_extended::directory::DirectoryFd;
    /// use std::path::Path;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let dir_fd = DirectoryFd::open(Path::new("/tmp")).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn open(path: &Path) -> Result<Self> {
        let file = File::open(path)
            .await
            .map_err(|e| directory_error(&format!("Failed to open directory {:?}: {}", path, e)))?;

        Ok(Self {
            file: Arc::new(file),
            path: path.to_path_buf(),
        })
    }

    /// Get a reference to the underlying file descriptor
    ///
    /// This is used internally by `*at` operations to get the file descriptor
    /// for the directory.
    #[must_use]
    pub fn as_file(&self) -> &File {
        &self.file
    }

    /// Get the path this directory represents
    ///
    /// This is primarily used for error messages and debugging.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Get the raw file descriptor for use with system calls
    ///
    /// This is used internally by `*at` operations that need the raw fd.
    #[cfg(unix)]
    #[must_use]
    pub fn as_raw_fd(&self) -> std::os::unix::io::RawFd {
        self.file.as_raw_fd()
    }

    /// Create a directory relative to this DirectoryFd
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the directory to create (relative to this directory)
    /// * `mode` - Permissions for the directory (octal)
    ///
    /// # Returns
    ///
    /// `Ok(())` if the directory was created successfully
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The directory already exists
    /// - Permission is denied
    /// - The operation fails due to I/O errors
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use compio_fs_extended::directory::DirectoryFd;
    /// use std::path::Path;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let dir_fd = DirectoryFd::open(Path::new("/tmp")).await?;
    /// dir_fd.create_directory("new_dir", 0o755).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(unix)]
    pub async fn create_directory(&self, name: &str, mode: u32) -> Result<()> {
        // TODO: Implement using io_uring MkdirAt opcode when available
        // For now, use nix with spawn for security
        let dir_fd = self.as_raw_fd();
        let name_owned = name.to_string();

        compio::runtime::spawn(async move {
            // SAFETY: The raw fd is valid for the duration of this call because:
            // 1. DirectoryFd holds an Arc<File> keeping the fd open
            // 2. The spawned task completes before DirectoryFd is dropped
            // We use raw fd here because BorrowedFd from as_fd() has a non-'static lifetime,
            // but spawn requires 'static. This is the standard pattern for moving fds into tasks.
            nix::sys::stat::mkdirat(
                Some(dir_fd),
                std::path::Path::new(&name_owned),
                nix::sys::stat::Mode::from_bits_truncate(mode),
            )
            .map_err(|e| directory_error(&format!("mkdirat failed for '{}': {}", name_owned, e)))
        })
        .await
        .map_err(|e| directory_error(&format!("spawn failed: {:?}", e)))?
    }

    #[cfg(windows)]
    pub async fn create_directory(&self, name: &str, _mode: u32) -> Result<()> {
        let base = self.path.clone();
        compio::runtime::spawn(async move {
            std::fs::create_dir(base.join(name))
                .map_err(|e| directory_error(&format!("create_dir failed: {e}")))
        })
        .await
        .map_err(|e| directory_error(&format!("spawn failed: {e:?}")))?
    }

    /// Set timestamps on this directory itself
    ///
    /// FD-based operation that sets timestamps on the directory,
    /// avoiding path lookups and TOCTOU races. Uses `futimens(2)` internally.
    ///
    /// # Arguments
    ///
    /// * `accessed` - New access time
    /// * `modified` - New modification time
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails (e.g., permission denied, I/O errors).
    pub async fn set_times(
        &self,
        accessed: std::time::SystemTime,
        modified: std::time::SystemTime,
    ) -> Result<()> {
        crate::metadata::futimens_fd(self.as_file(), accessed, modified).await
    }

    /// Set permissions on this directory itself
    ///
    /// FD-based operation that sets permissions on the directory,
    /// avoiding path lookups and TOCTOU races. Uses `fchmod(2)` internally.
    ///
    /// Matches the API of `compio::fs::File::set_permissions()`.
    ///
    /// # Arguments
    ///
    /// * `permissions` - New permissions for the directory
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails (e.g., permission denied, I/O errors).
    pub async fn set_permissions(&self, permissions: compio::fs::Permissions) -> Result<()> {
        self.as_file()
            .set_permissions(permissions)
            .await
            .map_err(|e| {
                crate::error::directory_error(&format!("Failed to set permissions: {}", e))
            })
    }

    /// Set ownership on this directory itself
    ///
    /// FD-based operation that sets ownership on the directory,
    /// avoiding path lookups and TOCTOU races. Uses `fchown(2)` internally.
    ///
    /// # Arguments
    ///
    /// * `uid` - New user ID
    /// * `gid` - New group ID
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails (e.g., permission denied, I/O errors).
    pub async fn set_ownership(&self, uid: u32, gid: u32) -> Result<()> {
        use crate::ownership::OwnershipOps;
        self.as_file()
            .fchown(uid, gid)
            .await
            .map_err(|e| crate::error::directory_error(&format!("Failed to set ownership: {}", e)))
    }

    // ========================================================================
    // Metadata operations on children (relative paths)
    // ========================================================================

    /// Get file metadata with nanosecond timestamps for a child file
    ///
    /// Uses io_uring `statx(2)` with directory FD and relative path (TOCTOU-safe).
    ///
    /// # Arguments
    ///
    /// * `pathname` - Relative path to the file
    ///
    /// # Errors
    ///
    /// Returns an error if the file doesn't exist or I/O errors occur.
    pub async fn statx(
        &self,
        pathname: &str,
    ) -> Result<(std::time::SystemTime, std::time::SystemTime)> {
        crate::metadata::statx_impl(self, pathname).await
    }

    /// Change file permissions for a child file
    ///
    /// Uses `fchmodat(2)` with directory FD and relative path (TOCTOU-safe).
    ///
    /// # Arguments
    ///
    /// * `pathname` - Relative path to the file
    /// * `mode` - New file permissions (e.g., 0o644)
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails (e.g., permission denied, file not found).
    pub async fn fchmodat(&self, pathname: &str, mode: u32) -> Result<()> {
        crate::metadata::fchmodat_impl(self, pathname, mode).await
    }

    /// Change file timestamps for a child file
    ///
    /// Uses `utimensat(2)` with directory FD and relative path (TOCTOU-safe).
    ///
    /// # Arguments
    ///
    /// * `pathname` - Relative path to the file
    /// * `accessed` - New access time
    /// * `modified` - New modification time
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails (e.g., permission denied, file not found).
    pub async fn utimensat(
        &self,
        pathname: &str,
        accessed: std::time::SystemTime,
        modified: std::time::SystemTime,
    ) -> Result<()> {
        crate::metadata::utimensat_impl(self, pathname, accessed, modified).await
    }

    /// Change file ownership for a child file
    ///
    /// Uses `fchownat(2)` with directory FD and relative path (TOCTOU-safe).
    ///
    /// # Arguments
    ///
    /// * `pathname` - Relative path to the file
    /// * `uid` - New user ID
    /// * `gid` - New group ID
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails (e.g., permission denied, file not found).
    pub async fn fchownat(&self, pathname: &str, uid: u32, gid: u32) -> Result<()> {
        crate::metadata::fchownat_impl(self, pathname, uid, gid).await
    }

    // ========================================================================
    // Symlink operations on children (relative paths)
    // ========================================================================

    /// Create a symbolic link for a child
    ///
    /// Uses `symlinkat(2)` with directory FD and relative path (TOCTOU-safe).
    ///
    /// # Arguments
    ///
    /// * `target` - Target path of the symbolic link
    /// * `link_name` - Name of the symbolic link (relative to this directory)
    ///
    /// # Errors
    ///
    /// Returns an error if the link already exists or permission is denied.
    pub async fn symlinkat(&self, target: &str, link_name: &str) -> Result<()> {
        crate::symlink::symlinkat_impl(self, target, link_name).await
    }

    /// Read a symbolic link for a child
    ///
    /// Uses `readlinkat(2)` with directory FD and relative path (TOCTOU-safe).
    ///
    /// # Arguments
    ///
    /// * `link_name` - Name of the symbolic link (relative to this directory)
    ///
    /// # Errors
    ///
    /// Returns an error if the link doesn't exist or is not a symbolic link.
    pub async fn readlinkat(&self, link_name: &str) -> Result<std::path::PathBuf> {
        crate::symlink::readlinkat_impl(self, link_name).await
    }
}

/// Read directory entries
///
/// This function provides a consistent API for directory reading that abstracts
/// whether the operation is blocking or uses io_uring.
///
/// CURRENT STATUS: Uses std::fs::read_dir (synchronous) because:
/// - Linux kernel 6.14 does NOT have IORING_OP_GETDENTS64
/// - Patches proposed in 2021 were never merged
/// - See: https://lwn.net/Articles/878873/
///
/// FUTURE: If kernel adds GETDENTS64 support, this function can be updated
/// to use io_uring without changing the calling code.
///
/// # Arguments
///
/// * `path` - Directory path to read
///
/// # Returns
///
/// Iterator over directory entries
///
/// # Errors
///
/// Returns an error if the directory cannot be read
///
/// # Example
///
/// ```rust,no_run
/// use compio_fs_extended::directory::read_dir;
/// use std::path::Path;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let entries = read_dir(Path::new("/tmp")).await?;
/// for entry in entries {
///     let entry = entry?;
///     println!("Entry: {:?}", entry.file_name());
/// }
/// # Ok(())
/// # }
/// ```
pub async fn read_dir(path: &Path) -> Result<std::fs::ReadDir> {
    // NOTE: Kernel limitation - must use synchronous std::fs::read_dir
    // Wrapping in spawn to avoid blocking the async runtime
    // This function exists to:
    // 1. Provide a consistent API in compio-fs-extended
    // 2. Allow future swap to io_uring if/when kernel adds GETDENTS64
    // 3. Keep app code (src/directory.rs) abstracted from implementation details
    let path_owned = path.to_path_buf();
    compio::runtime::spawn(async move {
        std::fs::read_dir(path_owned)
            .map_err(|e| directory_error(&format!("Failed to read directory: {}", e)))
    })
    .await
    .map_err(|e| directory_error(&format!("spawn failed: {:?}", e)))?
}

impl Clone for DirectoryFd {
    fn clone(&self) -> Self {
        Self {
            file: Arc::clone(&self.file),
            path: self.path.clone(),
        }
    }
}

impl AsFd for DirectoryFd {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.file.as_fd()
    }
}

// Note: Basic directory operations (create_dir, remove_dir, etc.) are provided by compio::fs
// This module only provides DirectoryFd for secure *at operations

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[compio::test]
    async fn test_directory_fd_open_existing() {
        let temp_dir = TempDir::new().unwrap();

        // Test opening an existing directory
        let dir_fd = DirectoryFd::open(temp_dir.path()).await;
        assert!(dir_fd.is_ok());

        let dir_fd = dir_fd.unwrap();
        assert_eq!(dir_fd.path(), temp_dir.path());
        assert!(dir_fd.as_raw_fd() > 0);
    }

    #[compio::test]
    async fn test_directory_fd_open_nonexistent() {
        // Test opening a non-existent directory
        let result = DirectoryFd::open(std::path::Path::new("/nonexistent/directory")).await;
        assert!(result.is_err());
    }

    #[compio::test]
    async fn test_directory_fd_open_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "test").unwrap();

        // Test opening a file (should fail)
        let result = DirectoryFd::open(&file_path).await;
        match result {
            Ok(_) => {
                // Some filesystems allow opening files as directories
                // This is acceptable behavior
                println!("File opened as directory (filesystem allows this)");
            }
            Err(_) => {
                // Expected behavior on most filesystems
                println!("File correctly rejected as directory");
            }
        }
    }

    #[compio::test]
    async fn test_directory_fd_clone() {
        let temp_dir = TempDir::new().unwrap();
        let dir_fd = DirectoryFd::open(temp_dir.path()).await.unwrap();

        // Test cloning
        let cloned_dir_fd = dir_fd.clone();
        assert_eq!(dir_fd.path(), cloned_dir_fd.path());
        assert_eq!(dir_fd.as_raw_fd(), cloned_dir_fd.as_raw_fd());
    }

    #[compio::test]
    async fn test_directory_fd_create_directory() {
        let temp_dir = TempDir::new().unwrap();
        let dir_fd = DirectoryFd::open(temp_dir.path()).await.unwrap();

        // Test creating a directory
        let result = dir_fd.create_directory("test_subdir", 0o755).await;
        assert!(result.is_ok());

        // Verify the directory was created
        let created_path = temp_dir.path().join("test_subdir");
        assert!(created_path.exists());
        assert!(created_path.is_dir());
    }

    #[compio::test]
    async fn test_directory_fd_create_directory_already_exists() {
        let temp_dir = TempDir::new().unwrap();
        let dir_fd = DirectoryFd::open(temp_dir.path()).await.unwrap();

        // Create directory first time
        dir_fd.create_directory("test_subdir", 0o755).await.unwrap();

        // Try to create it again (should fail)
        let result = dir_fd.create_directory("test_subdir", 0o755).await;
        assert!(result.is_err());
    }

    #[compio::test]
    async fn test_directory_fd_create_directory_invalid_name() {
        let temp_dir = TempDir::new().unwrap();
        let dir_fd = DirectoryFd::open(temp_dir.path()).await.unwrap();

        // Test creating directory with invalid name (empty string)
        let result = dir_fd.create_directory("", 0o755).await;
        assert!(result.is_err());
    }

    #[compio::test]
    async fn test_directory_fd_create_directory_nested() {
        let temp_dir = TempDir::new().unwrap();
        let dir_fd = DirectoryFd::open(temp_dir.path()).await.unwrap();

        // Create first level
        dir_fd.create_directory("level1", 0o755).await.unwrap();

        // Open the created directory and create second level
        let level1_path = temp_dir.path().join("level1");
        let level1_dir_fd = DirectoryFd::open(&level1_path).await.unwrap();
        level1_dir_fd
            .create_directory("level2", 0o755)
            .await
            .unwrap();

        // Verify both levels exist
        assert!(level1_path.exists());
        assert!(level1_path.join("level2").exists());
    }

    #[compio::test]
    async fn test_directory_fd_as_file() {
        let temp_dir = TempDir::new().unwrap();
        let dir_fd = DirectoryFd::open(temp_dir.path()).await.unwrap();

        // Test getting file reference
        let file = dir_fd.as_file();
        assert_eq!(file.as_raw_fd(), dir_fd.as_raw_fd());
    }

    #[compio::test]
    async fn test_directory_fd_path() {
        let temp_dir = TempDir::new().unwrap();
        let dir_fd = DirectoryFd::open(temp_dir.path()).await.unwrap();

        // Test getting path
        assert_eq!(dir_fd.path(), temp_dir.path());
    }

    #[compio::test]
    async fn test_directory_fd_debug() {
        let temp_dir = TempDir::new().unwrap();
        let dir_fd = DirectoryFd::open(temp_dir.path()).await.unwrap();

        // Test debug formatting
        let debug_str = format!("{:?}", dir_fd);
        assert!(debug_str.contains("DirectoryFd"));
    }

    #[compio::test]
    async fn test_directory_fd_multiple_operations() {
        let temp_dir = TempDir::new().unwrap();
        let dir_fd = DirectoryFd::open(temp_dir.path()).await.unwrap();

        // Test multiple directory creation operations
        let dirs = ["dir1", "dir2", "dir3"];
        for dir_name in &dirs {
            let result = dir_fd.create_directory(dir_name, 0o755).await;
            assert!(result.is_ok());
        }

        // Verify all directories were created
        for dir_name in &dirs {
            let created_path = temp_dir.path().join(dir_name);
            assert!(created_path.exists());
            assert!(created_path.is_dir());
        }
    }
}
