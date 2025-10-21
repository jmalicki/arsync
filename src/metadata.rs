//! Metadata preservation configuration and operations
//!
//! This module provides metadata preservation functionality for file copying operations.
//! It defines what metadata should be preserved (via `MetadataConfig`) and implements
//! the actual preservation operations using file descriptor-based syscalls for security.
//!
//! # Architecture
//!
//! - `MetadataConfig`: Configuration struct (used in CLI with `clap::Args`)
//! - Preservation functions: File descriptor-based operations (secure, no TOCTOU)
//! - Helper utilities: Timestamp handling, syscall wrappers
//!
//! # Usage
//!
//! ```rust,ignore
//! use arsync::metadata::{MetadataConfig, preserve_file_metadata};
//!
//! let config = MetadataConfig {
//!     archive: true,  // Preserve all metadata
//!     ..Default::default()
//! };
//!
//! // Preserve metadata using file descriptors
//! preserve_file_metadata(&src_file, &dst_file, &src_path, &dst_path, &config).await?;
//! ```

use crate::error::{Result, SyncError};
use crate::traits::AsyncMetadata;
use std::path::Path;
use std::time::SystemTime;

// ============================================================================
// CONFIGURATION
// ============================================================================

/// Metadata preservation configuration
///
/// Used by: `copy_file()`, `preserve_directory_metadata()`
/// These flags control what metadata gets preserved during copying
#[derive(clap::Args, Debug, Clone)]
#[command(next_help_heading = "Metadata Preservation (rsync-compatible)")]
#[allow(clippy::struct_excessive_bools)] // Metadata flags are inherently boolean
pub struct MetadataConfig {
    /// Archive mode; same as -rlptgoD (recursive, links, perms, times, group, owner, devices)
    #[arg(short = 'a', long)]
    pub archive: bool,

    /// Recurse into directories
    #[arg(short = 'r', long)]
    pub recursive: bool,

    /// Copy symlinks as symlinks
    #[arg(short = 'l', long)]
    pub links: bool,

    /// Preserve permissions
    #[arg(short = 'p', long)]
    pub perms: bool,

    /// Preserve modification times
    #[arg(short = 't', long)]
    pub times: bool,

    /// Preserve group
    #[arg(short = 'g', long)]
    pub group: bool,

    /// Preserve owner (super-user only)
    #[arg(short = 'o', long)]
    pub owner: bool,

    /// Preserve device files (super-user only) and special files
    #[arg(short = 'D', long)]
    pub devices: bool,

    /// Sync each file to disk after writing (like rsync --fsync)
    ///
    /// By default, arsync relies on OS page cache (like rsync).
    /// This flag forces fsync on each file for durability at the cost of performance.
    #[arg(long)]
    pub fsync: bool,

    /// Preserve extended attributes
    #[arg(short = 'X', long)]
    pub xattrs: bool,

    /// Preserve ACLs (implies --perms)
    #[arg(short = 'A', long)]
    pub acls: bool,

    /// Preserve hard links
    #[arg(short = 'H', long)]
    pub hard_links: bool,

    /// Preserve access (use) times
    #[arg(short = 'U', long)]
    pub atimes: bool,

    /// Preserve creation times (when supported)
    #[arg(long)]
    pub crtimes: bool,

    // Deprecated flags (hidden, for backwards compatibility)
    /// Preserve extended attributes (deprecated: use -X/--xattrs)
    #[arg(long, hide = true)]
    pub preserve_xattr: bool,

    /// Preserve POSIX ACLs (deprecated: use -A/--acls)
    #[arg(long, hide = true)]
    pub preserve_acl: bool,
}

impl MetadataConfig {
    /// Check if permissions should be preserved
    #[must_use]
    pub const fn should_preserve_permissions(&self) -> bool {
        self.perms || self.archive || self.acls
    }

    /// Check if ownership (user and/or group) should be preserved
    #[must_use]
    pub const fn should_preserve_ownership(&self) -> bool {
        self.owner || self.group || self.archive
    }

    /// Check if timestamps should be preserved
    #[must_use]
    pub const fn should_preserve_timestamps(&self) -> bool {
        self.times || self.archive
    }

    /// Check if extended attributes should be preserved
    #[must_use]
    pub const fn should_preserve_xattrs(&self) -> bool {
        self.xattrs || self.preserve_xattr
    }

    /// Check if symlinks should be copied as symlinks
    #[must_use]
    pub const fn should_preserve_links(&self) -> bool {
        self.links || self.archive
    }

    /// Check if hard links should be preserved
    #[must_use]
    pub const fn should_preserve_hard_links(&self) -> bool {
        self.hard_links
    }

    /// Check if recursive copying should be performed
    #[allow(dead_code)] // Public API for future use
    #[must_use]
    pub const fn should_recurse(&self) -> bool {
        self.recursive || self.archive
    }
}

// ============================================================================
// FILE METADATA PRESERVATION OPERATIONS
// ============================================================================

/// Preserve all file metadata from source to destination file descriptors
///
/// This is a convenience function that preserves all metadata based on config.
///
/// # Arguments
///
/// * `src_file` - Source file descriptor
/// * `dst_file` - Destination file descriptor
/// * `dst_path` - Destination path (for timestamp setting)
/// * `src_accessed` - Source access time
/// * `src_modified` - Source modification time
/// * `config` - Metadata preservation configuration
///
/// # Errors
///
/// Returns error if any metadata preservation operation fails
#[allow(clippy::future_not_send)]
#[allow(clippy::too_many_arguments)] // Needed for complete metadata preservation
pub async fn preserve_file_metadata(
    src_file: &compio::fs::File,
    dst_file: &compio::fs::File,
    _dst_path: &Path,
    src_accessed: SystemTime,
    src_modified: SystemTime,
    config: &MetadataConfig,
) -> Result<()> {
    // Preserve file metadata only if explicitly requested (rsync behavior)
    if config.should_preserve_permissions() {
        preserve_permissions_from_fd(src_file, dst_file).await?;
    }

    if config.should_preserve_ownership() {
        preserve_ownership_from_fd(src_file, dst_file).await?;
    }

    if config.should_preserve_xattrs() {
        preserve_xattr_from_fd(src_file, dst_file).await?;
    }

    if config.should_preserve_timestamps() {
        preserve_timestamps_from_fd(dst_file, src_accessed, src_modified).await?;
    }

    Ok(())
}

/// Preserve only file permissions from source to destination
///
/// This function preserves file permissions including special bits (setuid, setgid, sticky)
/// using fchmod (file descriptor-based) for security and to avoid umask interference.
///
/// # Errors
///
/// Returns error if permissions cannot be read from source or set on destination
#[allow(clippy::future_not_send)]
pub async fn preserve_permissions_from_fd(
    src_file: &compio::fs::File,
    dst_file: &compio::fs::File,
) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    // Get source file permissions using file descriptor
    let src_metadata = src_file
        .metadata()
        .await
        .map_err(|e| SyncError::FileSystem(format!("Failed to get source file metadata: {e}")))?;

    let std_permissions = src_metadata.permissions();
    let mode = std_permissions.mode();

    // Convert to compio::fs::Permissions
    let compio_permissions = compio::fs::Permissions::from_mode(mode);

    // Use compio::fs::File::set_permissions which uses fchmod (file descriptor-based)
    dst_file
        .set_permissions(compio_permissions)
        .await
        .map_err(|e| SyncError::FileSystem(format!("Failed to preserve permissions: {e}")))
}

/// Preserve file ownership using file descriptors
///
/// Uses fchown (file descriptor-based) to avoid TOCTOU race conditions.
///
/// # Errors
///
/// Returns error if ownership cannot be preserved (requires appropriate privileges)
#[allow(clippy::future_not_send)]
pub async fn preserve_ownership_from_fd(
    src_file: &compio::fs::File,
    dst_file: &compio::fs::File,
) -> Result<()> {
    use compio_fs_extended::OwnershipOps;

    // Use compio-fs-extended for ownership preservation (fchown internally)
    dst_file
        .preserve_ownership_from(src_file)
        .await
        .map_err(|e| SyncError::FileSystem(format!("Failed to preserve file ownership: {e}")))?;
    Ok(())
}

/// Preserve file extended attributes using file descriptors
///
/// Uses fgetxattr/fsetxattr (file descriptor-based) for security.
///
/// # Arguments
///
/// * `src_file` - Source file handle
/// * `dst_file` - Destination file handle
///
/// # Errors
///
/// This function will return an error if:
/// - Extended attributes cannot be read from source
/// - Extended attributes cannot be written to destination
#[allow(clippy::future_not_send)]
pub async fn preserve_xattr_from_fd(
    src_file: &compio::fs::File,
    dst_file: &compio::fs::File,
) -> Result<()> {
    use compio_fs_extended::{ExtendedFile, XattrOps};

    // Convert to ExtendedFile to access xattr operations
    let extended_src = ExtendedFile::from_ref(src_file);
    let extended_dst = ExtendedFile::from_ref(dst_file);

    // Get all extended attribute names from source file
    let Ok(xattr_names) = extended_src.list_xattr().await else {
        // If xattr is not supported or no xattrs exist, that's fine
        return Ok(());
    };

    // Copy each extended attribute
    for name in xattr_names {
        match extended_src.get_xattr(&name).await {
            Ok(value) => {
                if let Err(e) = extended_dst.set_xattr(&name, &value).await {
                    // Log warning but continue with other xattrs
                    tracing::warn!("Failed to preserve extended attribute '{}': {}", name, e);
                }
            }
            Err(e) => {
                tracing::warn!("Failed to read extended attribute '{}': {}", name, e);
            }
        }
    }

    Ok(())
}

/// Preserve timestamps with nanosecond precision using FD
///
/// FD-based timestamp preservation using futimens(2) - TOCTOU-free!
///
/// # Errors
///
/// Returns error if futimens syscall fails
pub async fn preserve_timestamps_from_fd(
    dst_file: &compio::fs::File,
    accessed: SystemTime,
    modified: SystemTime,
) -> Result<()> {
    // Use compio-fs-extended's FD-based futimens
    compio_fs_extended::metadata::futimens_fd(dst_file, accessed, modified)
        .await
        .map_err(|e| SyncError::FileSystem(format!("Failed to preserve timestamps via FD: {e}")))
}

/// Get precise timestamps from a file path
///
/// Uses statx when available (nanosecond precision), falls back to stat.
///
/// # Returns
///
/// Returns `Ok((accessed, modified))` with nanosecond-precision timestamps
///
/// # Errors
///
/// Returns error if statx/stat syscall fails or path is invalid
#[allow(dead_code)] // Used in copy.rs
pub async fn get_precise_timestamps(path: &Path) -> Result<(SystemTime, SystemTime)> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    // Convert path to CString for syscall
    let path_cstr = CString::new(path.as_os_str().as_bytes())
        .map_err(|e| SyncError::FileSystem(format!("Invalid path for timestamp reading: {e}")))?;

    // Prefer statx when available (Linux only)
    #[cfg(target_os = "linux")]
    let statx_result: Result<(SystemTime, SystemTime)> = compio::runtime::spawn_blocking({
        let path_cstr = path_cstr.clone();
        move || {
            let path_ptr = path_cstr.as_ptr();
            // statx flags: AT_FDCWD, path, AT_SYMLINK_NOFOLLOW (0), STATX_BASIC_STATS
            let mut buf: libc::statx = unsafe { std::mem::zeroed() };
            let rc = unsafe {
                libc::statx(
                    libc::AT_FDCWD,
                    path_ptr,
                    0,
                    0x0000_07ffu32 as libc::c_uint,
                    &raw mut buf,
                )
            };
            if rc == 0 {
                // Use stx_atime and stx_mtime with nanoseconds
                let atime_secs = u64::try_from(buf.stx_atime.tv_sec).unwrap_or(0);
                let atime_nanos = buf.stx_atime.tv_nsec;
                let mtime_secs = u64::try_from(buf.stx_mtime.tv_sec).unwrap_or(0);
                let mtime_nanos = buf.stx_mtime.tv_nsec;
                let atime =
                    SystemTime::UNIX_EPOCH + std::time::Duration::new(atime_secs, atime_nanos);
                let mtime =
                    SystemTime::UNIX_EPOCH + std::time::Duration::new(mtime_secs, mtime_nanos);
                Ok((atime, mtime))
            } else {
                let errno = std::io::Error::last_os_error();
                Err(SyncError::FileSystem(format!(
                    "statx failed: {errno} (errno: {})",
                    errno.raw_os_error().unwrap_or(-1)
                )))
            }
        }
    })
    .await
    .map_err(|e| SyncError::FileSystem(format!("spawn_blocking failed: {e:?}")))?;

    #[cfg(target_os = "linux")]
    match statx_result {
        Ok((atime, mtime)) => {
            return Ok((atime, mtime));
        }
        Err(_) => {
            // Fallthrough to stat fallback
        }
    }

    // macOS or Linux fallback: use stat
    compio::runtime::spawn_blocking(move || {
        let mut stat_buf: libc::stat = unsafe { std::mem::zeroed() };
        let result = unsafe { libc::stat(path_cstr.as_ptr(), &raw mut stat_buf) };

        if result == -1 {
            let errno = std::io::Error::last_os_error();
            Err(SyncError::FileSystem(format!(
                "stat failed: {errno} (errno: {})",
                errno.raw_os_error().unwrap_or(-1)
            )))
        } else {
            // Convert timespec to SystemTime
            let accessed_nanos: u32 = u32::try_from(stat_buf.st_atime_nsec).unwrap_or(0);
            let modified_nanos: u32 = u32::try_from(stat_buf.st_mtime_nsec).unwrap_or(0);
            #[allow(clippy::cast_sign_loss)]
            let accessed = SystemTime::UNIX_EPOCH
                + std::time::Duration::new(stat_buf.st_atime as u64, accessed_nanos);
            #[allow(clippy::cast_sign_loss)]
            let modified = SystemTime::UNIX_EPOCH
                + std::time::Duration::new(stat_buf.st_mtime as u64, modified_nanos);
            Ok((accessed, modified))
        }
    })
    .await
    .map_err(|e| SyncError::FileSystem(format!("spawn_blocking failed: {e:?}")))
    .and_then(|r| r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metadata_config_defaults() {
        let config = MetadataConfig {
            archive: false,
            recursive: false,
            links: false,
            perms: false,
            times: false,
            group: false,
            owner: false,
            devices: false,
            fsync: false,
            xattrs: false,
            acls: false,
            hard_links: false,
            atimes: false,
            crtimes: false,
            preserve_xattr: false,
            preserve_acl: false,
        };

        // Nothing should be preserved
        assert!(!config.should_preserve_permissions());
        assert!(!config.should_preserve_ownership());
        assert!(!config.should_preserve_timestamps());
        assert!(!config.should_preserve_xattrs());
    }

    #[test]
    fn test_metadata_config_archive() {
        let config = MetadataConfig {
            archive: true,
            recursive: false,
            links: false,
            perms: false,
            times: false,
            group: false,
            owner: false,
            devices: false,
            fsync: false,
            xattrs: false,
            acls: false,
            hard_links: false,
            atimes: false,
            crtimes: false,
            preserve_xattr: false,
            preserve_acl: false,
        };

        // Archive enables most things
        assert!(config.should_preserve_permissions());
        assert!(config.should_preserve_ownership());
        assert!(config.should_preserve_timestamps());
        assert!(config.should_preserve_links());
    }
}

// ============================================================================
// Trait Implementations
// ============================================================================

/// Implementation of AsyncMetadata for FileMetadata
///
/// This enables FileMetadata to be used with the trait-based filesystem abstraction.
/// All methods delegate to existing FileMetadata methods.
impl AsyncMetadata for compio_fs_extended::FileMetadata {
    fn size(&self) -> u64 {
        self.size
    }

    fn is_file(&self) -> bool {
        compio_fs_extended::FileMetadata::is_file(self)
    }

    fn is_dir(&self) -> bool {
        compio_fs_extended::FileMetadata::is_dir(self)
    }

    fn is_symlink(&self) -> bool {
        compio_fs_extended::FileMetadata::is_symlink(self)
    }

    fn permissions(&self) -> u32 {
        compio_fs_extended::FileMetadata::permissions(self)
    }

    fn uid(&self) -> u32 {
        self.uid
    }

    fn gid(&self) -> u32 {
        self.gid
    }

    fn modified(&self) -> SystemTime {
        self.modified
    }

    fn accessed(&self) -> SystemTime {
        self.accessed
    }

    fn device_id(&self) -> u64 {
        self.dev
    }

    fn inode_number(&self) -> u64 {
        self.ino
    }

    fn link_count(&self) -> u64 {
        self.nlink
    }
}
