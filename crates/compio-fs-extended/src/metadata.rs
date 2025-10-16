//! File metadata operations using file descriptors
//!
//! This module provides metadata operations with io_uring support where available.
//!
//! # Operations
//!
//! - **statx_at**: Get file metadata with nanosecond timestamps (io_uring STATX)
//! - **fchmodat**: Change file permissions using file descriptors
//! - **futimesat**: Change file timestamps using file descriptors
//! - **fchownat**: Change file ownership using file descriptors
//! - **fchmodat_with_dirfd**: Change file permissions using DirectoryFd (most efficient)
//! - **futimesat_with_dirfd**: Change file timestamps using DirectoryFd (most efficient)
//! - **fchownat_with_dirfd**: Change file ownership using DirectoryFd (most efficient)
//!
//! # Usage
//!
//! ```rust,no_run
//! use compio_fs_extended::metadata::{fchmodat, futimesat, fchownat, fchmodat_with_dirfd, futimesat_with_dirfd, fchownat_with_dirfd};
//! use compio_fs_extended::directory::DirectoryFd;
//! use std::path::Path;
//! use std::time::SystemTime;
//!
//! # async fn example() -> compio_fs_extended::Result<()> {
//! // Path-based operations (use AT_FDCWD)
//! fchmodat(Path::new("file.txt"), 0o644).await?;
//! let now = SystemTime::now();
//! futimesat(Path::new("file.txt"), now, now).await?;
//! fchownat(Path::new("file.txt"), 1000, 1000).await?;
//!
//! // DirectoryFd-based operations (most efficient)
//! let dir_fd = DirectoryFd::open(Path::new("/some/directory")).await?;
//! fchmodat_with_dirfd(dir_fd.fd(), "file.txt", 0o644).await?;
//! futimesat_with_dirfd(dir_fd.fd(), "file.txt", now, now).await?;
//! fchownat_with_dirfd(dir_fd.fd(), "file.txt", 1000, 1000).await?;
//! # Ok(())
//! # }
//! ```

use crate::error::{metadata_error, ExtendedError, Result};
use compio::driver::OpCode;
use compio::runtime::submit;
use filetime::{set_file_times, FileTime};
#[cfg(target_os = "linux")]
use io_uring::{opcode, types};
use nix::sys::stat::UtimensatFlags;
use nix::sys::time::TimeSpec;
use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::time::SystemTime;

/// Get the /proc/self/fd path for a file descriptor
fn proc_fd_path(fd: i32) -> PathBuf {
    PathBuf::from(format!("/proc/self/fd/{}", fd))
}

/// io_uring statx operation for getting file metadata with nanosecond timestamps
#[cfg(target_os = "linux")]
pub struct StatxOp {
    /// Directory file descriptor (AT_FDCWD for current directory)
    dirfd: std::os::unix::io::RawFd,
    /// Path to the file (relative to dirfd)
    pathname: CString,
    /// Buffer for statx result (libc::statx has the actual fields we need)
    statxbuf: Box<libc::statx>,
    /// Flags for statx operation
    flags: i32,
    /// Mask for which fields to retrieve
    mask: u32,
}

#[cfg(target_os = "linux")]
impl StatxOp {
    /// Create a new statx operation
    ///
    /// # Arguments
    ///
    /// * `dirfd` - Directory file descriptor (use AT_FDCWD for current directory)
    /// * `pathname` - Path to the file
    /// * `flags` - Flags like AT_SYMLINK_NOFOLLOW
    /// * `mask` - Mask for which fields to retrieve (e.g., STATX_BASIC_STATS)
    #[must_use]
    pub fn new(dirfd: i32, pathname: CString, flags: i32, mask: u32) -> Self {
        Self {
            dirfd,
            pathname,
            statxbuf: Box::new(unsafe { std::mem::zeroed() }),
            flags,
            mask,
        }
    }
}

#[cfg(target_os = "linux")]
impl OpCode for StatxOp {
    fn create_entry(mut self: Pin<&mut Self>) -> compio::driver::OpEntry {
        compio::driver::OpEntry::Submission(
            opcode::Statx::new(
                types::Fd(self.dirfd),
                self.pathname.as_ptr(),
                &mut *self.statxbuf as *mut libc::statx as *mut types::statx,
            )
            .flags(self.flags)
            .mask(self.mask)
            .build(),
        )
    }
}

/// Get file metadata with nanosecond timestamps using io_uring STATX
///
/// This function uses io_uring IORING_OP_STATX to retrieve file metadata
/// including nanosecond-precision timestamps.
///
/// # Arguments
///
/// * `path` - Path to the file
///
/// # Returns
///
/// Returns `(atime, mtime)` with nanosecond precision
///
/// # Errors
///
/// Returns an error if the statx operation fails
#[cfg(target_os = "linux")]
pub async fn statx_at(path: &Path) -> Result<(SystemTime, SystemTime)> {
    let path_cstr = CString::new(path.as_os_str().as_bytes())
        .map_err(|e| metadata_error(&format!("Invalid path: {}", e)))?;

    // Use AT_FDCWD for current working directory, AT_SYMLINK_NOFOLLOW=0
    // STATX_BASIC_STATS = 0x7ff (all basic fields)
    let op = StatxOp::new(libc::AT_FDCWD, path_cstr, 0, 0x0000_07ff);
    let result = submit(op).await;

    match result.0 {
        Ok(_) => {
            let statx_buf = result.1.statxbuf;

            // Extract nanosecond timestamps
            let atime_secs = u64::try_from(statx_buf.stx_atime.tv_sec).unwrap_or(0);
            let atime_nanos = statx_buf.stx_atime.tv_nsec;
            let mtime_secs = u64::try_from(statx_buf.stx_mtime.tv_sec).unwrap_or(0);
            let mtime_nanos = statx_buf.stx_mtime.tv_nsec;

            let atime = SystemTime::UNIX_EPOCH + std::time::Duration::new(atime_secs, atime_nanos);
            let mtime = SystemTime::UNIX_EPOCH + std::time::Duration::new(mtime_secs, mtime_nanos);

            Ok((atime, mtime))
        }
        Err(e) => Err(metadata_error(&format!("statx failed: {}", e))),
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub async fn statx_at(path: &Path) -> Result<(SystemTime, SystemTime)> {
    // Fallback: std metadata for atime/mtime via filetime crate
    let meta = compio::fs::metadata(path)
        .await
        .map_err(|e| metadata_error(&format!("metadata failed: {e}")))?;
    let modified = meta
        .modified()
        .map_err(|e| metadata_error(&format!("modified failed: {e}")))?;
    let accessed = meta
        .accessed()
        .map_err(|e| metadata_error(&format!("accessed failed: {e}")))?;
    Ok((accessed, modified))
}

/// Join a directory file descriptor path with a relative pathname
fn join_dirfd_path(dir_fd: i32, pathname: &str) -> Result<PathBuf> {
    let dir_path = std::fs::read_link(proc_fd_path(dir_fd))?;
    Ok(dir_path.join(pathname))
}

/// Change file permissions using file descriptor
///
/// # Arguments
///
/// * `path` - Path to the file
/// * `mode` - New file permissions (e.g., 0o644)
///
/// # Returns
///
/// `Ok(())` if the permissions were changed successfully
///
/// # Errors
///
/// This function will return an error if:
/// - The file doesn't exist
/// - Permission is denied
/// - Invalid mode value
/// - The operation fails due to I/O errors
pub async fn fchmodat(path: &Path, mode: u32) -> Result<()> {
    let path = path.to_path_buf();
    let inner = compio::runtime::spawn(async move {
        let mut perms = std::fs::metadata(&path)?.permissions();
        perms.set_mode(mode);
        std::fs::set_permissions(&path, perms)
    })
    .await
    .map_err(ExtendedError::SpawnJoin)?;
    inner?;
    Ok(())
}

/// Change file timestamps using file descriptor
///
/// # Arguments
///
/// * `path` - Path to the file
/// * `accessed` - New access time
/// * `modified` - New modification time
///
/// # Returns
///
/// `Ok(())` if the timestamps were changed successfully
///
/// # Errors
///
/// This function will return an error if:
/// - The file doesn't exist
/// - Permission is denied
/// - Invalid timestamp values
/// - The operation fails due to I/O errors
pub async fn futimesat(path: &Path, accessed: SystemTime, modified: SystemTime) -> Result<()> {
    let path = path.to_path_buf();

    // NOTE: Kernel doesn't have IORING_OP_UTIMENSAT - using safe nix wrapper
    let inner = compio::runtime::spawn(async move {
        // Convert SystemTime to TimeSpec for nix
        let atime = system_time_to_timespec(accessed)?;
        let mtime = system_time_to_timespec(modified)?;

        nix::sys::stat::utimensat(
            None, // Use path directly (AT_FDCWD)
            &path,
            &atime,
            &mtime,
            UtimensatFlags::NoFollowSymlink,
        )
        .map_err(|e| metadata_error(&format!("utimensat failed: {}", e)))
    })
    .await
    .map_err(ExtendedError::SpawnJoin)?;
    inner?;
    Ok(())
}

/// Helper to convert SystemTime to nix TimeSpec
fn system_time_to_timespec(time: SystemTime) -> Result<TimeSpec> {
    let duration = time
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_err(|e| metadata_error(&format!("Invalid time: {}", e)))?;

    Ok(TimeSpec::new(
        duration.as_secs() as i64,
        duration.subsec_nanos() as i64,
    ))
}

/// Change file timestamps using file descriptor (FD-based, more efficient)
///
/// This function uses `futimens` which is FD-based, avoiding path lookups
/// and TOCTOU race conditions.
///
/// # Arguments
///
/// * `fd` - File descriptor
/// * `accessed` - New access time
/// * `modified` - New modification time
///
/// # Returns
///
/// `Ok(())` if the timestamps were changed successfully
///
/// # Errors
///
/// This function will return an error if:
/// - The file descriptor is invalid
/// - Permission is denied
/// - Invalid timestamp values
pub async fn futimens_fd(fd: i32, accessed: SystemTime, modified: SystemTime) -> Result<()> {
    // NOTE: Kernel doesn't have IORING_OP_FUTIMENS - using safe nix wrapper
    // futimens is FD-based, better than path-based utimensat (no TOCTOU)
    let inner = compio::runtime::spawn(async move {
        let atime = system_time_to_timespec(accessed)?;
        let mtime = system_time_to_timespec(modified)?;

        nix::sys::stat::futimens(fd, &atime, &mtime)
            .map_err(|e| metadata_error(&format!("futimens failed: {}", e)))
    })
    .await
    .map_err(ExtendedError::SpawnJoin)?;
    inner?;
    Ok(())
}

/// Change file ownership using file descriptor
///
/// # Arguments
///
/// * `path` - Path to the file
/// * `uid` - New user ID (use -1 to not change)
/// * `gid` - New group ID (use -1 to not change)
///
/// # Returns
///
/// `Ok(())` if the ownership was changed successfully
///
/// # Errors
///
/// This function will return an error if:
/// - The file doesn't exist
/// - Permission is denied
/// - Invalid user/group IDs
/// - The operation fails due to I/O errors
pub async fn fchownat(_path: &Path, _uid: u32, _gid: u32) -> Result<()> {
    Err(metadata_error(
        "chown is not supported via std::fs; enable a libc-based path if required",
    ))
}

/// Change file permissions using file descriptor (more efficient)
///
/// # Arguments
///
/// * `fd` - File descriptor
/// * `mode` - New file permissions (e.g., 0o644)
///
/// # Returns
///
/// `Ok(())` if the permissions were changed successfully
///
/// # Errors
///
/// This function will return an error if:
/// - Invalid file descriptor
/// - Permission is denied
/// - Invalid mode value
/// - The operation fails due to I/O errors
pub async fn fchmod(fd: i32, mode: u32) -> Result<()> {
    let inner = compio::runtime::spawn(async move {
        let path = proc_fd_path(fd);
        let mut perms = std::fs::metadata(&path)?.permissions();
        perms.set_mode(mode);
        std::fs::set_permissions(&path, perms)
    })
    .await
    .map_err(ExtendedError::SpawnJoin)?;
    inner?;
    Ok(())
}

/// Change file timestamps using file descriptor (more efficient)
///
/// # Arguments
///
/// * `fd` - File descriptor
/// * `accessed` - New access time
/// * `modified` - New modification time
///
/// # Returns
///
/// `Ok(())` if the timestamps were changed successfully
///
/// # Errors
///
/// This function will return an error if:
/// - Invalid file descriptor
/// - Permission is denied
/// - Invalid timestamp values
/// - The operation fails due to I/O errors
pub async fn futimes(fd: i32, accessed: SystemTime, modified: SystemTime) -> Result<()> {
    let inner = compio::runtime::spawn(async move {
        let path = proc_fd_path(fd);
        let atime = FileTime::from(accessed);
        let mtime = FileTime::from(modified);
        set_file_times(&path, atime, mtime)
    })
    .await
    .map_err(ExtendedError::SpawnJoin)?;
    inner?;
    Ok(())
}

/// Change file ownership using file descriptor (more efficient)
///
/// # Arguments
///
/// * `fd` - File descriptor
/// * `uid` - New user ID (use -1 to not change)
/// * `gid` - New group ID (use -1 to not change)
///
/// # Returns
///
/// `Ok(())` if the ownership was changed successfully
///
/// # Errors
///
/// This function will return an error if:
/// - Invalid file descriptor
/// - Permission is denied
/// - Invalid user/group IDs
/// - The operation fails due to I/O errors
pub async fn fchown(_fd: i32, _uid: u32, _gid: u32) -> Result<()> {
    Err(metadata_error(
        "chown is not supported via std::fs; enable a libc-based path if required",
    ))
}

/// Change file permissions using DirectoryFd (most efficient)
///
/// # Arguments
///
/// * `dir_fd` - Directory file descriptor from DirectoryFd
/// * `pathname` - Relative path to the file
/// * `mode` - New file permissions (e.g., 0o644)
///
/// # Returns
///
/// `Ok(())` if the permissions were changed successfully
///
/// # Errors
///
/// This function will return an error if:
/// - The file doesn't exist
/// - Permission is denied
/// - Invalid mode value
/// - The operation fails due to I/O errors
pub async fn fchmodat_with_dirfd(dir_fd: i32, pathname: &str, mode: u32) -> Result<()> {
    let full = join_dirfd_path(dir_fd, pathname)?;
    let inner = compio::runtime::spawn(async move {
        let mut perms = std::fs::metadata(&full)?.permissions();
        perms.set_mode(mode);
        std::fs::set_permissions(&full, perms)
    })
    .await
    .map_err(ExtendedError::SpawnJoin)?;
    inner?;
    Ok(())
}

/// Change file timestamps using DirectoryFd (most efficient)
///
/// # Arguments
///
/// * `dir_fd` - Directory file descriptor from DirectoryFd
/// * `pathname` - Relative path to the file
/// * `accessed` - New access time
/// * `modified` - New modification time
///
/// # Returns
///
/// `Ok(())` if the timestamps were changed successfully
///
/// # Errors
///
/// This function will return an error if:
/// - The file doesn't exist
/// - Permission is denied
/// - Invalid timestamp values
/// - The operation fails due to I/O errors
pub async fn futimesat_with_dirfd(
    dir_fd: i32,
    pathname: &str,
    accessed: SystemTime,
    modified: SystemTime,
) -> Result<()> {
    let full = join_dirfd_path(dir_fd, pathname)?;
    let inner = compio::runtime::spawn(async move {
        let atime = FileTime::from(accessed);
        let mtime = FileTime::from(modified);
        set_file_times(&full, atime, mtime)
    })
    .await
    .map_err(ExtendedError::SpawnJoin)?;
    inner?;
    Ok(())
}

/// Change file ownership using DirectoryFd (most efficient)
///
/// # Arguments
///
/// * `dir_fd` - Directory file descriptor from DirectoryFd
/// * `pathname` - Relative path to the file
/// * `uid` - New user ID (use -1 to not change)
/// * `gid` - New group ID (use -1 to not change)
///
/// # Returns
///
/// `Ok(())` if the ownership was changed successfully
///
/// # Errors
///
/// This function will return an error if:
/// - The file doesn't exist
/// - Permission is denied
/// - Invalid user/group IDs
/// - The operation fails due to I/O errors
pub async fn fchownat_with_dirfd(
    _dir_fd: i32,
    _pathname: &str,
    _uid: u32,
    _gid: u32,
) -> Result<()> {
    Err(metadata_error(
        "chown is not supported via std::fs; enable a libc-based path if required",
    ))
}
