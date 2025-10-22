//! Shared metadata preservation using DirectoryFd
//!
//! Provides TOCTOU-safe metadata preservation using DirectoryFd and *at syscalls.

use crate::error::{Result, SyncError};
use crate::metadata::MetadataConfig;
use compio_fs_extended::DirectoryFd;
use std::ffi::OsStr;

/// Preserve file metadata using DirectoryFd (TOCTOU-safe)
///
/// Preserves file metadata using *at syscalls relative to a directory file descriptor,
/// ensuring TOCTOU safety and avoiding symlink attacks.
///
/// # Parameters
///
/// * `dir_fd` - Parent directory file descriptor
/// * `filename` - File name relative to directory (basename only, no path separators)
/// * `metadata` - Source metadata to preserve
/// * `config` - Configuration for which metadata to preserve
///
/// # Returns
///
/// Returns `Ok(())` on success
///
/// # Errors
///
/// Returns `Err(SyncError)` if:
/// - Permissions cannot be set
/// - Ownership cannot be set
/// - Timestamps cannot be set
/// - I/O error occurs
///
/// # Security
///
/// Uses *at syscalls (fchmodat, fchownat, utimensat) which are TOCTOU-safe.
/// Does not use path-based operations.
///
/// # Examples
///
/// ```rust,ignore
/// use arsync::filesystem::preserve_metadata;
/// use arsync::metadata::MetadataConfig;
/// use compio_fs_extended::DirectoryFd;
///
/// #[compio::main]
/// async fn main() -> arsync::Result<()> {
///     let src_fd = DirectoryFd::open("/src/dir").await?;
///     let dst_fd = DirectoryFd::open("/dst/dir").await?;
///     
///     let src_metadata = src_fd.statx_full("file.txt").await?;
///     let config = MetadataConfig::default();
///     
///     preserve_metadata(&dst_fd, "file.txt", &src_metadata, &config).await?;
///     Ok(())
/// }
/// ```
pub async fn preserve_metadata(
    dir_fd: &DirectoryFd,
    filename: &OsStr,
    metadata: &compio_fs_extended::FileMetadata,
    config: &MetadataConfig,
) -> Result<()> {
    // Preserve ownership (if configured)
    if config.should_preserve_ownership() {
        let filename_str = filename.to_str().ok_or_else(|| {
            SyncError::FileSystem(format!(
                "Invalid filename (not UTF-8): {}",
                filename.to_string_lossy()
            ))
        })?;

        dir_fd
            .lfchownat(filename_str, metadata.uid, metadata.gid)
            .await
            .map_err(|e| {
                SyncError::FileSystem(format!("Failed to set ownership for '{filename_str}': {e}"))
            })?;
    }

    // Preserve permissions (if configured)
    if config.should_preserve_permissions() {
        let filename_str = filename.to_str().ok_or_else(|| {
            SyncError::FileSystem(format!(
                "Invalid filename (not UTF-8): {}",
                filename.to_string_lossy()
            ))
        })?;

        dir_fd
            .lfchmodat(filename_str, metadata.permissions())
            .await
            .map_err(|e| {
                SyncError::FileSystem(format!(
                    "Failed to set permissions for '{filename_str}': {e}"
                ))
            })?;
    }

    // Preserve timestamps (if configured)
    if config.should_preserve_timestamps() {
        let filename_str = filename.to_str().ok_or_else(|| {
            SyncError::FileSystem(format!(
                "Invalid filename (not UTF-8): {}",
                filename.to_string_lossy()
            ))
        })?;

        dir_fd
            .lutimensat(filename_str, metadata.accessed, metadata.modified)
            .await
            .map_err(|e| {
                SyncError::FileSystem(format!(
                    "Failed to set timestamps for '{filename_str}': {e}"
                ))
            })?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;

    #[compio::test]
    async fn test_preserve_permissions() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;

        // Create source file with specific permissions
        let src_file = temp_dir.path().join("source.txt");
        fs::write(&src_file, b"test")?;
        fs::set_permissions(&src_file, fs::Permissions::from_mode(0o755))?;

        // Create dest file
        let dst_file = temp_dir.path().join("dest.txt");
        fs::write(&dst_file, b"test")?;

        // Get source metadata
        let dir_fd = DirectoryFd::open(temp_dir.path()).await?;
        let src_metadata = dir_fd.statx_full(OsStr::new("source.txt")).await?;

        // Preserve metadata
        let config = MetadataConfig {
            archive: false,
            recursive: false,
            links: false,
            perms: true,
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
        preserve_metadata(&dir_fd, OsStr::new("dest.txt"), &src_metadata, &config).await?;

        // Verify
        let dst_metadata = fs::metadata(&dst_file)?;
        assert_eq!(dst_metadata.permissions().mode() & 0o777, 0o755);

        Ok(())
    }

    #[compio::test]
    async fn test_preserve_timestamps() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;

        // Create source file
        let src_file = temp_dir.path().join("source.txt");
        fs::write(&src_file, b"test")?;

        // Create dest file
        let dst_file = temp_dir.path().join("dest.txt");
        fs::write(&dst_file, b"test")?;

        // Get source metadata
        let dir_fd = DirectoryFd::open(temp_dir.path()).await?;
        let src_metadata = dir_fd.statx_full(OsStr::new("source.txt")).await?;

        // Preserve metadata
        let config = MetadataConfig {
            archive: false,
            recursive: false,
            links: false,
            perms: false,
            times: true,
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
        preserve_metadata(&dir_fd, OsStr::new("dest.txt"), &src_metadata, &config).await?;

        // Verify (timestamps should match)
        let dst_metadata = dir_fd.statx_full(OsStr::new("dest.txt")).await?;

        // Allow small difference due to precision
        let src_mod = src_metadata.modified;
        let dst_mod = dst_metadata.modified;
        let diff = if src_mod > dst_mod {
            src_mod.duration_since(dst_mod)?
        } else {
            dst_mod.duration_since(src_mod)?
        };
        assert!(diff.as_secs() < 2);

        Ok(())
    }

    #[compio::test]
    async fn test_preserve_no_config() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;

        // Create files
        let src_file = temp_dir.path().join("source.txt");
        fs::write(&src_file, b"test")?;
        fs::set_permissions(&src_file, fs::Permissions::from_mode(0o755))?;

        let dst_file = temp_dir.path().join("dest.txt");
        fs::write(&dst_file, b"test")?;
        fs::set_permissions(&dst_file, fs::Permissions::from_mode(0o644))?;

        // Get source metadata
        let dir_fd = DirectoryFd::open(temp_dir.path()).await?;
        let src_metadata = dir_fd.statx_full(OsStr::new("source.txt")).await?;

        // Preserve metadata with no config (should be no-op)
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
        preserve_metadata(&dir_fd, OsStr::new("dest.txt"), &src_metadata, &config).await?;

        // Verify permissions unchanged
        let dst_metadata = fs::metadata(&dst_file)?;
        assert_eq!(dst_metadata.permissions().mode() & 0o777, 0o644);

        Ok(())
    }
}
