//! File metadata operations using file descriptors
//!
//! This module provides metadata operations with io_uring support where available.
//!
//! # Operations
//!
//! All operations are available as methods on `DirectoryFd` for TOCTOU-safe execution:
//!
//! - **DirectoryFd::statx**: Get file metadata with nanosecond timestamps (io_uring STATX)
//! - **DirectoryFd::fchmodat**: Change file permissions using `fchmodat(2)`
//! - **DirectoryFd::utimensat**: Change file timestamps using `utimensat(2)`
//! - **DirectoryFd::fchownat**: Change file ownership using `fchownat(2)`
//!
//! File descriptor-based operations (for already-open files):
//!
//! - **futimens_fd**: Change timestamps on an open file (Note: use File::set_permissions and OwnershipOps for permissions/ownership)
//!
//! # Usage
//!
//! ```rust,no_run
//! use compio_fs_extended::directory::DirectoryFd;
//! use std::path::Path;
//! use std::time::SystemTime;
//!
//! # async fn example() -> compio_fs_extended::Result<()> {
//! // DirectoryFd-based operations (TOCTOU-safe, no trait imports needed!)
//! let dir = DirectoryFd::open(Path::new("/some/directory")).await?;
//! let (atime, mtime) = dir.statx("file.txt").await?;
//! dir.fchmodat("file.txt", 0o644).await?;
//! let now = SystemTime::now();
//! dir.utimensat("file.txt", now, now).await?;
//! dir.fchownat("file.txt", 1000, 1000).await?;
//! # Ok(())
//! # }
//! ```

use crate::directory::DirectoryFd;
use crate::error::{metadata_error, ExtendedError, Result};
use compio::driver::OpCode;
use compio::fs::File;
use compio::runtime::submit;
use io_uring::{opcode, types};
use nix::sys::stat::UtimensatFlags;
use nix::sys::time::TimeSpec;
use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::io::AsRawFd;
use std::path::Path;
use std::pin::Pin;
use std::time::SystemTime;

/// io_uring statx operation for getting file metadata with nanosecond timestamps
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
/// including nanosecond-precision timestamps. Uses AT_FDCWD (current directory).
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
/// * `file` - File reference
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
pub async fn futimens_fd(file: &File, accessed: SystemTime, modified: SystemTime) -> Result<()> {
    // NOTE: Kernel doesn't have IORING_OP_FUTIMENS - using safe nix wrapper
    // futimens is FD-based, better than path-based utimensat (no TOCTOU)
    let fd = file.as_raw_fd();
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

/// Private trait for types that provide a directory file descriptor
///
/// This allows metadata operations to work with any type that can provide
/// a directory FD, while keeping the implementation details private.
trait DirectoryFdOps {
    fn as_dirfd(&self) -> std::os::unix::io::RawFd;
}

/// Implement DirectoryFdOps for DirectoryFd
impl DirectoryFdOps for DirectoryFd {
    fn as_dirfd(&self) -> std::os::unix::io::RawFd {
        self.as_raw_fd()
    }
}

/// Get file metadata with nanosecond timestamps using DirectoryFd
///
/// Uses io_uring IORING_OP_STATX with a directory FD and relative path,
/// avoiding TOCTOU race conditions.
///
/// # Arguments
///
/// * `dir` - Directory file descriptor provider
/// * `pathname` - Relative path to the file (relative to dir)
///
/// # Returns
///
/// Returns `(atime, mtime)` with nanosecond precision
///
/// # Errors
///
/// Returns an error if the statx operation fails
#[allow(private_bounds)]
pub(crate) async fn statx_impl(
    dir: &impl DirectoryFdOps,
    pathname: &str,
) -> Result<(SystemTime, SystemTime)> {
    let dir_fd = dir.as_dirfd();
    let path_cstr =
        CString::new(pathname).map_err(|e| metadata_error(&format!("Invalid pathname: {}", e)))?;

    // Use directory FD with relative path
    // AT_SYMLINK_NOFOLLOW=0 (follow symlinks)
    // STATX_BASIC_STATS = 0x7ff (all basic fields)
    let op = StatxOp::new(dir_fd, path_cstr, 0, 0x0000_07ff);
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

/// Change file permissions using DirectoryFd
#[allow(private_bounds)]
pub(crate) async fn fchmodat_impl(
    dir: &impl DirectoryFdOps,
    pathname: &str,
    mode: u32,
) -> Result<()> {
    let dir_fd = dir.as_dirfd();
    let pathname_cstring = std::ffi::CString::new(pathname)
        .map_err(|e| metadata_error(&format!("Invalid pathname: {}", e)))?;

    let inner = compio::runtime::spawn(async move {
        use nix::sys::stat::{fchmodat, FchmodatFlags, Mode};

        fchmodat(
            Some(dir_fd),
            pathname_cstring.as_c_str(),
            Mode::from_bits_truncate(mode),
            FchmodatFlags::FollowSymlink,
        )
        .map_err(|e| metadata_error(&format!("fchmodat failed: {}", e)))
    })
    .await
    .map_err(ExtendedError::SpawnJoin)?;
    inner?;
    Ok(())
}

/// Change file timestamps using DirectoryFd
#[allow(private_bounds)]
pub(crate) async fn utimensat_impl(
    dir: &impl DirectoryFdOps,
    pathname: &str,
    accessed: SystemTime,
    modified: SystemTime,
) -> Result<()> {
    let dir_fd = dir.as_dirfd();
    let pathname_owned = pathname.to_string();

    let inner = compio::runtime::spawn(async move {
        let atime = system_time_to_timespec(accessed)?;
        let mtime = system_time_to_timespec(modified)?;

        nix::sys::stat::utimensat(
            Some(dir_fd),
            pathname_owned.as_str(),
            &atime,
            &mtime,
            UtimensatFlags::FollowSymlink,
        )
        .map_err(|e| metadata_error(&format!("utimensat failed: {}", e)))
    })
    .await
    .map_err(ExtendedError::SpawnJoin)?;
    inner?;
    Ok(())
}

/// Change file ownership using DirectoryFd
#[allow(private_bounds)]
pub(crate) async fn fchownat_impl(
    dir: &impl DirectoryFdOps,
    pathname: &str,
    uid: u32,
    gid: u32,
) -> Result<()> {
    let dir_fd = dir.as_dirfd();
    let pathname_owned = pathname.to_string();

    let inner = compio::runtime::spawn(async move {
        use nix::fcntl::AtFlags;
        use nix::unistd::{fchownat, Gid, Uid};

        fchownat(
            Some(dir_fd),
            pathname_owned.as_str(),
            Some(Uid::from_raw(uid)),
            Some(Gid::from_raw(gid)),
            AtFlags::empty(), // Follow symlinks by default (no AT_SYMLINK_NOFOLLOW)
        )
        .map_err(|e| metadata_error(&format!("fchownat failed: {}", e)))
    })
    .await
    .map_err(ExtendedError::SpawnJoin)?;
    inner?;
    Ok(())
}
