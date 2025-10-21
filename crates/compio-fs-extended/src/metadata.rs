//! File metadata operations using file descriptors
//!
//! This module provides metadata operations with io_uring support where available.
//!
//! # Operations
//!
//! All operations are available as methods on `DirectoryFd` for TOCTOU-safe execution:
//!
//! - **DirectoryFd::get_metadata**: Get file metadata with nanosecond timestamps
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
//! let (atime, mtime) = dir.get_timestamps("file.txt").await?;
//! dir.fchmodat("file.txt", 0o644).await?;
//! let now = SystemTime::now();
//! dir.utimensat("file.txt", now, now).await?;
//! dir.fchownat("file.txt", 1000, 1000).await?;
//! # Ok(())
//! # }
//! ```

#[cfg(unix)]
use crate::directory::DirectoryFd;
#[cfg(unix)]
use crate::error::{metadata_error, ExtendedError, Result};
#[cfg(target_os = "linux")]
use compio::driver::OpCode;
#[cfg(unix)]
use compio::fs::File;
#[cfg(target_os = "linux")]
use compio::runtime::submit;
#[cfg(target_os = "linux")]
use io_uring::{opcode, types};
#[cfg(unix)]
use nix::sys::stat::UtimensatFlags;
#[cfg(unix)]
use nix::sys::time::TimeSpec;
#[cfg(target_os = "linux")]
use std::ffi::CString;
#[cfg(target_os = "linux")]
use std::os::unix::ffi::OsStrExt;
#[cfg(unix)]
use std::os::unix::io::AsRawFd;
#[cfg(target_os = "linux")]
use std::path::Path;
#[cfg(target_os = "linux")]
use std::pin::Pin;
#[cfg(unix)]
use std::time::SystemTime;

/// Full file metadata with platform-specific extensions
///
/// This struct contains file metadata fields available across Unix platforms,
/// with platform-specific extensions for advanced features.
///
/// # Common Fields (all Unix platforms)
///
/// - `size`, `mode`, `uid`, `gid`, `nlink`, `ino`, `dev`: Standard Unix metadata
/// - `accessed`, `modified`: Standard timestamps (nanosecond precision)
/// - `created`: Birth time (creation time) when available
///
/// # Platform-Specific Fields
///
/// ## Linux (`statx`)
/// - `attributes`: File attributes (immutable, append-only, compressed, etc.)
/// - `attributes_mask`: Mask indicating which attributes are valid
///
/// ## macOS (`stat`)
/// - `flags`: BSD file flags (UF_IMMUTABLE, UF_APPEND, UF_NODUMP, etc.)
/// - `generation`: File generation number
///
/// # Usage
///
/// ```rust,ignore
/// let metadata = dir.get_metadata("file.txt").await?;
/// println!("Size: {}, Mode: {:o}", metadata.size, metadata.mode);
///
/// #[cfg(target_os = "linux")]
/// if let Some(attrs) = metadata.attributes {
///     println!("Linux attributes: {:#x}", attrs);
/// }
///
/// #[cfg(target_os = "macos")]
/// if let Some(flags) = metadata.flags {
///     println!("macOS flags: {:#x}", flags);
/// }
/// ```
#[derive(Debug, Clone)]
pub struct FileMetadata {
    // ============================================================================
    // Common Unix metadata (available on all platforms)
    // ============================================================================
    
    /// File size in bytes
    pub size: u64,
    /// File mode (type + permissions)
    pub mode: u32,
    /// User ID of owner
    pub uid: u32,
    /// Group ID of owner
    pub gid: u32,
    /// Number of hard links
    pub nlink: u64,
    /// Inode number
    pub ino: u64,
    /// Device ID
    pub dev: u64,
    /// Last access time
    pub accessed: SystemTime,
    /// Last modification time
    pub modified: SystemTime,
    /// Creation time (birth time) if available
    pub created: Option<SystemTime>,
    
    // ============================================================================
    // Platform-specific metadata
    // ============================================================================
    
    /// Linux file attributes (immutable, append-only, compressed, etc.)
    ///
    /// From `statx.stx_attributes`. Common values:
    /// - `STATX_ATTR_COMPRESSED` (0x4): File is compressed
    /// - `STATX_ATTR_IMMUTABLE` (0x10): File is immutable
    /// - `STATX_ATTR_APPEND` (0x20): File is append-only
    /// - `STATX_ATTR_NODUMP` (0x40): File should not be dumped
    ///
    /// Only set if the filesystem supports these attributes.
    #[cfg(target_os = "linux")]
    pub attributes: Option<u64>,
    
    /// Mask indicating which Linux attributes are valid
    ///
    /// From `statx.stx_attributes_mask`. Bits set to 1 indicate the
    /// corresponding attribute in `attributes` is supported by the filesystem.
    #[cfg(target_os = "linux")]
    pub attributes_mask: Option<u64>,
    
    /// macOS/BSD file flags (UF_IMMUTABLE, UF_APPEND, etc.)
    ///
    /// From `stat.st_flags`. Common values:
    /// - `UF_IMMUTABLE` (0x2): File is immutable (user flag)
    /// - `UF_APPEND` (0x4): File is append-only (user flag)
    /// - `UF_NODUMP` (0x1): File should not be dumped
    /// - `SF_IMMUTABLE` (0x20000): File is immutable (system flag)
    ///
    /// Only available on macOS/BSD systems.
    #[cfg(target_os = "macos")]
    pub flags: Option<u32>,
    
    /// File generation number (macOS/BSD)
    ///
    /// From `stat.st_gen`. Incremented whenever the file is modified.
    /// Used by NFS and other distributed filesystems for cache coherency.
    ///
    /// Only available on macOS/BSD systems.
    #[cfg(target_os = "macos")]
    pub generation: Option<u32>,
}

impl FileMetadata {
    /// Create FileMetadata from compio metadata (fallback path)
    ///
    /// This is a fallback that constructs FileMetadata from `compio::fs::Metadata`.
    /// Platform-specific fields are extracted when available via `MetadataExt` traits.
    ///
    /// **Note:** This is less efficient than using `DirectoryFd::get_metadata()`
    /// which gets all metadata in one async operation.
    ///
    /// # Arguments
    ///
    /// * `metadata` - Compio metadata from `compio::fs::metadata()` or similar
    ///
    /// # Returns
    ///
    /// Returns a `FileMetadata` with common fields populated and platform-specific
    /// fields extracted when available.
    #[cfg(unix)]
    #[must_use]
    pub fn from_compio_metadata(metadata: &compio::fs::Metadata) -> Self {
        use std::os::unix::fs::MetadataExt;
        
        Self {
            size: metadata.len(),
            mode: metadata.mode(),
            uid: metadata.uid(),
            gid: metadata.gid(),
            nlink: metadata.nlink(),
            ino: metadata.ino(),
            dev: metadata.dev(),
            accessed: metadata.accessed().unwrap_or(SystemTime::UNIX_EPOCH),
            modified: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            created: metadata.created().ok(),
            
            // Platform-specific fields - not available via compio::fs::Metadata wrapper
            // Use statx_full() for full platform-specific metadata
            #[cfg(target_os = "linux")]
            attributes: None,
            #[cfg(target_os = "linux")]
            attributes_mask: None,
            
            #[cfg(target_os = "macos")]
            flags: None,  // Not exposed by compio::fs::Metadata
            #[cfg(target_os = "macos")]
            generation: None,  // Not exposed by compio::fs::Metadata
        }
    }

    /// Create FileMetadata from standard Rust metadata (fallback path)
    ///
    /// This is a fallback that constructs FileMetadata from `std::fs::Metadata`.
    /// Platform-specific fields are extracted when available via `MetadataExt` traits.
    ///
    /// **Note:** This is less efficient than using `DirectoryFd::get_metadata()`
    /// which gets all metadata in one async operation.
    ///
    /// # Arguments
    ///
    /// * `metadata` - Standard Rust metadata from `fs::metadata()` or similar
    ///
    /// # Returns
    ///
    /// Returns a `FileMetadata` with common fields populated and platform-specific
    /// fields extracted when available.
    #[cfg(unix)]
    #[must_use]
    pub fn from_std_metadata(metadata: &std::fs::Metadata) -> Self {
        use std::os::unix::fs::MetadataExt;
        
        // Import platform-specific extensions
        #[cfg(target_os = "macos")]
        use std::os::darwin::fs::MetadataExt as DarwinMetadataExt;
        
        Self {
            size: metadata.len(),
            mode: metadata.mode(),
            uid: metadata.uid(),
            gid: metadata.gid(),
            nlink: metadata.nlink(),
            ino: metadata.ino(),
            dev: metadata.dev(),
            accessed: metadata.accessed().unwrap_or(SystemTime::UNIX_EPOCH),
            modified: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            created: metadata.created().ok(),
            
            // Platform-specific fields - extracted via platform-specific MetadataExt
            #[cfg(target_os = "linux")]
            attributes: None,  // Not available via std::fs::Metadata
            #[cfg(target_os = "linux")]
            attributes_mask: None,  // Not available via std::fs::Metadata
            
            #[cfg(target_os = "macos")]
            flags: Some(metadata.st_flags()),
            #[cfg(target_os = "macos")]
            generation: Some(metadata.st_gen()),
        }
    }

    /// Check if this is a regular file
    #[must_use]
    pub fn is_file(&self) -> bool {
        (self.mode & libc::S_IFMT as u32) == libc::S_IFREG as u32
    }

    /// Check if this is a directory
    #[must_use]
    pub fn is_dir(&self) -> bool {
        (self.mode & libc::S_IFMT as u32) == libc::S_IFDIR as u32
    }

    /// Check if this is a symlink
    #[must_use]
    pub fn is_symlink(&self) -> bool {
        (self.mode & libc::S_IFMT as u32) == libc::S_IFLNK as u32
    }

    /// Get file permissions (mode & 0o7777)
    #[must_use]
    pub fn permissions(&self) -> u32 {
        self.mode & 0o7777
    }
}

/// io_uring STATX operation for getting file metadata with nanosecond timestamps (Linux)
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

/// Helper to convert SystemTime to nix TimeSpec
#[cfg(unix)]
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
#[cfg(unix)]
pub async fn futimens_fd(file: &File, accessed: SystemTime, modified: SystemTime) -> Result<()> {
    // NOTE: Kernel doesn't have IORING_OP_FUTIMENS - using safe nix wrapper
    // futimens is FD-based, better than path-based utimensat (no TOCTOU)
    let fd = file.as_raw_fd();
    let inner = compio::runtime::spawn_blocking(move || {
        let atime = system_time_to_timespec(accessed)?;
        let mtime = system_time_to_timespec(modified)?;

        // SAFETY: Caller guarantees fd is valid and won't be closed during this operation.
        nix::sys::stat::futimens(fd, &atime, &mtime)
            .map_err(|e| metadata_error(&format!("futimens failed: {}", e)))
    })
    .await
    .map_err(ExtendedError::SpawnJoin)?;
    inner?;
    Ok(())
}

/// Get file metadata with nanosecond timestamps using DirectoryFd
///
/// Uses io_uring IORING_OP_STATX with a directory FD and relative path,
/// avoiding TOCTOU race conditions.
///
/// # Arguments
///
/// * `dir` - Directory file descriptor
/// * `pathname` - Relative path to the file (relative to dir)
///
/// # Returns
///
/// Returns full `FileMetadata` with nanosecond precision timestamps
///
/// # Errors
///
/// Returns an error if the statx operation fails
#[cfg(target_os = "linux")]
pub(crate) async fn statx_impl(
    dir: &DirectoryFd,
    pathname: &std::ffi::OsStr,
) -> Result<FileMetadata> {
    use std::os::unix::ffi::OsStrExt;

    let dir_fd = dir.as_raw_fd();
    let path_cstr = CString::new(pathname.as_bytes())
        .map_err(|e| metadata_error(&format!("Invalid pathname: {}", e)))?;

    // Use directory FD with relative path
    // AT_SYMLINK_NOFOLLOW = don't dereference symlinks (CRITICAL for symlink preservation!)
    // STATX_BASIC_STATS = 0x7ff (all basic fields)
    let op = StatxOp::new(dir_fd, path_cstr, libc::AT_SYMLINK_NOFOLLOW, 0x0000_07ff);
    let result = submit(op).await;

    match result.0 {
        Ok(_) => {
            let statx_buf = result.1.statxbuf;

            // Extract all metadata fields
            let size = statx_buf.stx_size;
            let mode = statx_buf.stx_mode as u32;
            let uid = statx_buf.stx_uid;
            let gid = statx_buf.stx_gid;
            let nlink = statx_buf.stx_nlink as u64;
            let ino = statx_buf.stx_ino;

            // Combine device major/minor into single dev ID
            #[allow(clippy::cast_lossless)]
            let dev = (statx_buf.stx_dev_major as u64) << 32 | (statx_buf.stx_dev_minor as u64);

            // Convert timestamps with pre-epoch handling
            let accessed = statx_ts_to_system_time(&statx_buf.stx_atime);
            let modified = statx_ts_to_system_time(&statx_buf.stx_mtime);

            // Birth time (creation time) - may not be available on all filesystems
            let created = if statx_buf.stx_mask & libc::STATX_BTIME != 0 {
                Some(statx_ts_to_system_time(&statx_buf.stx_btime))
            } else {
                None
            };

            // Platform-specific fields
            #[cfg(target_os = "linux")]
            let attributes = if statx_buf.stx_attributes_mask != 0 {
                Some(statx_buf.stx_attributes)
            } else {
                None
            };
            
            #[cfg(target_os = "linux")]
            let attributes_mask = if statx_buf.stx_attributes_mask != 0 {
                Some(statx_buf.stx_attributes_mask)
            } else {
                None
            };

            Ok(FileMetadata {
                size,
                mode,
                uid,
                gid,
                nlink,
                ino,
                dev,
                accessed,
                modified,
                created,
                #[cfg(target_os = "linux")]
                attributes,
                #[cfg(target_os = "linux")]
                attributes_mask,
            })
        }
        Err(e) => Err(metadata_error(&format!("statx failed: {}", e))),
    }
}

/// Get file metadata using fstatat (macOS)
///
/// Uses `fstatat(2)` with directory FD for TOCTOU-safe metadata retrieval.
/// This is the macOS equivalent of the Linux `statx_impl`.
///
/// # Arguments
///
/// * `dir` - Directory file descriptor
/// * `pathname` - Relative path to the file (relative to dir)
///
/// # Returns
///
/// Returns full `FileMetadata` with nanosecond precision timestamps
///
/// # Errors
///
/// Returns an error if the fstatat operation fails
#[cfg(target_os = "macos")]
pub(crate) async fn statx_impl(
    dir: &DirectoryFd,
    pathname: &std::ffi::OsStr,
) -> Result<FileMetadata> {
    use std::os::unix::ffi::OsStrExt;
    
    let dir_fd = dir.as_raw_fd();
    let pathname_cstring = std::ffi::CString::new(pathname.as_bytes())
        .map_err(|e| metadata_error(&format!("Invalid pathname: {}", e)))?;
    
    // Run fstatat in spawn_blocking since it's a syscall
    compio::runtime::spawn_blocking(move || {
        use nix::fcntl::AtFlags;
        use nix::sys::stat::fstatat;
        
        // Use fstatat with AT_SYMLINK_NOFOLLOW (critical for symlink preservation!)
        let stat_result = fstatat(
            Some(dir_fd),
            pathname_cstring.as_c_str(),
            AtFlags::AT_SYMLINK_NOFOLLOW,
        ).map_err(|e| metadata_error(&format!("fstatat failed: {}", e)))?;
        
        // Extract all metadata fields from FileStat
        let size = stat_result.st_size as u64;
        let mode = stat_result.st_mode as u32;
        let uid = stat_result.st_uid;
        let gid = stat_result.st_gid;
        let nlink = stat_result.st_nlink as u64;
        let ino = stat_result.st_ino;
        let dev = stat_result.st_dev as u64;  // macOS: i32 -> u64
        
        // Convert timestamps to SystemTime
        // macOS stat provides nanosecond precision like Linux statx
        let accessed = unix_timestamp_to_system_time(stat_result.st_atime, stat_result.st_atime_nsec);
        let modified = unix_timestamp_to_system_time(stat_result.st_mtime, stat_result.st_mtime_nsec);
        
        // Birth time (creation time) - reliably available on macOS
        let created = Some(unix_timestamp_to_system_time(stat_result.st_birthtime, stat_result.st_birthtime_nsec));
        
        // macOS-specific fields
        let flags = Some(stat_result.st_flags as u32);
        let generation = Some(stat_result.st_gen as u32);
        
        Ok(FileMetadata {
            size,
            mode,
            uid,
            gid,
            nlink,
            ino,
            dev,
            accessed,
            modified,
            created,
            flags,
            generation,
        })
    })
    .await
    .map_err(ExtendedError::SpawnJoin)?
}

/// Convert Unix timestamp (seconds + nanoseconds) to SystemTime with pre-epoch support (macOS)
///
/// Handles timestamps before 1970 (negative secs) correctly.
#[cfg(target_os = "macos")]
fn unix_timestamp_to_system_time(secs: i64, nsec: i64) -> SystemTime {
    let nsec = nsec as u32;  // Nanoseconds are always positive
    
    if secs >= 0 {
        SystemTime::UNIX_EPOCH + std::time::Duration::new(secs as u64, nsec)
    } else {
        let abs_secs = (-secs) as u64;
        // Saturate: if subtraction underflows, clamp to UNIX_EPOCH
        SystemTime::UNIX_EPOCH
            .checked_sub(std::time::Duration::new(abs_secs, nsec))
            .unwrap_or(SystemTime::UNIX_EPOCH)
    }
}

/// Convert statx timestamp to SystemTime with pre-epoch support (Linux)
///
/// Handles timestamps before 1970 (negative tv_sec) correctly.
#[cfg(target_os = "linux")]
fn statx_ts_to_system_time(ts: &libc::statx_timestamp) -> SystemTime {
    let nsec = ts.tv_nsec;
    if ts.tv_sec >= 0 {
        SystemTime::UNIX_EPOCH + std::time::Duration::new(ts.tv_sec as u64, nsec)
    } else {
        let secs = (-ts.tv_sec) as u64;
        // Saturate: if subtraction underflows, clamp to UNIX_EPOCH
        SystemTime::UNIX_EPOCH
            .checked_sub(std::time::Duration::new(secs, nsec))
            .unwrap_or(SystemTime::UNIX_EPOCH)
    }
}

/// Change file permissions using DirectoryFd
#[cfg(unix)]
/// Change file permissions without following symlinks (symlink-aware)
#[cfg(target_os = "linux")]
pub(crate) async fn lfchmodat_impl(_dir: &DirectoryFd, _pathname: &str, _mode: u32) -> Result<()> {
    // No-op on Linux: symlink permissions are always 0777 and ignored by kernel
    // This function exists for API consistency with macOS
    Ok(())
}

/// Change file permissions without following symlinks (symlink-aware) - macOS/Unix
#[cfg(all(unix, not(target_os = "linux")))]
pub(crate) async fn lfchmodat_impl(dir: &DirectoryFd, pathname: &str, mode: u32) -> Result<()> {
    let pathname_cstring = std::ffi::CString::new(pathname)
        .map_err(|e| metadata_error(&format!("Invalid pathname: {}", e)))?;
    let dir_fd = dir.as_raw_fd();

    let operation = move || {
        use nix::sys::stat::{fchmodat, FchmodatFlags, Mode};

        fchmodat(
            Some(dir_fd),
            pathname_cstring.as_c_str(),
            Mode::from_bits_truncate(mode as nix::libc::mode_t),
            FchmodatFlags::NoFollowSymlink, // Don't follow symlinks!
        )
        .map_err(|e| metadata_error(&format!("lfchmodat failed: {}", e)))
    };

    #[cfg(feature = "cheap_calls_sync")]
    {
        operation()
    }

    #[cfg(not(feature = "cheap_calls_sync"))]
    {
        compio::runtime::spawn_blocking(operation)
            .await
            .map_err(ExtendedError::SpawnJoin)?
    }
}

/// Change file timestamps using DirectoryFd
#[cfg(unix)]
/// Change file timestamps without following symlinks (symlink-aware)
pub(crate) async fn lutimensat_impl(
    dir: &DirectoryFd,
    pathname: &str,
    accessed: SystemTime,
    modified: SystemTime,
) -> Result<()> {
    let pathname_owned = pathname.to_string();
    let dir_fd = dir.as_raw_fd();

    let operation = move || {
        let atime = system_time_to_timespec(accessed)?;
        let mtime = system_time_to_timespec(modified)?;

        nix::sys::stat::utimensat(
            Some(dir_fd),
            pathname_owned.as_str(),
            &atime,
            &mtime,
            UtimensatFlags::NoFollowSymlink, // Don't follow symlinks!
        )
        .map_err(|e| metadata_error(&format!("lutimensat failed: {}", e)))
    };

    #[cfg(feature = "cheap_calls_sync")]
    {
        operation()
    }

    #[cfg(not(feature = "cheap_calls_sync"))]
    {
        compio::runtime::spawn_blocking(operation)
            .await
            .map_err(ExtendedError::SpawnJoin)?
    }
}

/// Change file ownership without following symlinks (symlink-aware)
#[cfg(unix)]
pub(crate) async fn lfchownat_impl(
    dir: &DirectoryFd,
    pathname: &str,
    uid: u32,
    gid: u32,
) -> Result<()> {
    let pathname_owned = pathname.to_string();
    let dir_fd = dir.as_raw_fd();

    let operation = move || {
        use nix::fcntl::AtFlags;
        use nix::unistd::{fchownat, Gid, Uid};

        fchownat(
            Some(dir_fd),
            pathname_owned.as_str(),
            Some(Uid::from_raw(uid)),
            Some(Gid::from_raw(gid)),
            AtFlags::AT_SYMLINK_NOFOLLOW, // Don't follow symlinks!
        )
        .map_err(|e| metadata_error(&format!("lfchownat failed: {}", e)))
    };

    #[cfg(feature = "cheap_calls_sync")]
    {
        operation()
    }

    #[cfg(not(feature = "cheap_calls_sync"))]
    {
        compio::runtime::spawn_blocking(operation)
            .await
            .map_err(ExtendedError::SpawnJoin)?
    }
}
