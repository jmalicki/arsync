//! Symlink copying and metadata preservation

use crate::error::{Result, SyncError};
use crate::metadata::MetadataConfig;
use crate::stats::SharedStats;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, error};

/// Process a symlink by copying it
///
/// This function handles symbolic link copying, preserving the target path
/// and handling both valid and broken symlinks correctly.
///
/// # Symlink Handling
///
/// - **Valid Symlinks**: Copies the symlink with its target preserved
/// - **Broken Symlinks**: Copies the symlink with its broken target preserved
/// - **Target Preservation**: The symlink target is read and recreated exactly
///
/// # Parameters
///
/// * `src_path` - Source symlink path
/// * `dst_path` - Destination symlink path
/// * `metadata_config` - Configuration for metadata preservation
/// * `stats` - Shared statistics tracking
///
/// # Returns
///
/// Returns `Ok(())` if the symlink is processed successfully, or `Err(SyncError)` if processing fails.
///
/// # Errors
///
/// This function will return an error if:
/// - Symlink target reading fails
/// - Symlink creation fails
#[allow(clippy::future_not_send)]
pub(super) async fn process_symlink(
    src_path: PathBuf,
    dst_path: PathBuf,
    metadata_config: &MetadataConfig,
    stats: Arc<SharedStats>,
) -> Result<()> {
    debug!("Processing symlink: {}", src_path.display());

    match copy_symlink(&src_path, &dst_path, metadata_config).await {
        Ok(()) => {
            stats.increment_symlinks_processed();
            Ok(())
        }
        Err(e) => {
            stats.increment_errors();
            error!("Failed to copy symlink {}: {}", src_path.display(), e);
            Err(e)
        }
    }
}

/// Copy a symlink preserving its target and metadata
///
/// Note: Symlinks cannot be opened as file descriptors, so metadata preservation
/// uses path-based operations (fchmodat with `AT_SYMLINK_NOFOLLOW`, etc.).
/// This is the lowest-common-denominator for symlinks, but acceptable since
/// the symlink itself is atomic.
#[allow(clippy::future_not_send)]
#[allow(clippy::too_many_lines)] // Metadata preservation adds lines
#[allow(clippy::cast_sign_loss)] // Timestamps are i64 but Duration needs u64
pub(super) async fn copy_symlink(
    src: &Path,
    dst: &Path,
    metadata_config: &MetadataConfig,
) -> Result<()> {
    use compio_fs_extended::directory::DirectoryFd;

    // Extract parent directory and filename for DirectoryFd operations
    let src_parent = src.parent().ok_or_else(|| {
        SyncError::FileSystem(format!("Source path has no parent: {}", src.display()))
    })?;
    let src_name = src
        .file_name()
        .ok_or_else(|| {
            SyncError::FileSystem(format!("Source path has no filename: {}", src.display()))
        })?
        .to_string_lossy();

    let dst_parent = dst.parent().ok_or_else(|| {
        SyncError::FileSystem(format!("Destination path has no parent: {}", dst.display()))
    })?;
    let dst_name = dst
        .file_name()
        .ok_or_else(|| {
            SyncError::FileSystem(format!(
                "Destination path has no filename: {}",
                dst.display()
            ))
        })?
        .to_string_lossy();

    // Open DirectoryFd for source and destination parents
    let src_dir_fd = DirectoryFd::open(src_parent).await.map_err(|e| {
        SyncError::FileSystem(format!(
            "Failed to open source directory {}: {}",
            src_parent.display(),
            e
        ))
    })?;

    let dst_dir_fd = DirectoryFd::open(dst_parent).await.map_err(|e| {
        SyncError::FileSystem(format!(
            "Failed to open destination directory {}: {}",
            dst_parent.display(),
            e
        ))
    })?;

    // Read symlink target using DirectoryFd (TOCTOU-safe)
    let target = src_dir_fd.readlinkat(&src_name).await.map_err(|e| {
        SyncError::FileSystem(format!(
            "Failed to read symlink target for {}: {}",
            src.display(),
            e
        ))
    })?;

    // Remove destination if it exists
    if dst.exists() {
        compio::fs::remove_file(dst).await.map_err(|e| {
            SyncError::FileSystem(format!(
                "Failed to remove existing destination {}: {}",
                dst.display(),
                e
            ))
        })?;
    }

    // Create symlink with same target using DirectoryFd (TOCTOU-safe)
    let target_str = target.to_string_lossy();
    dst_dir_fd
        .symlinkat(&target_str, &dst_name)
        .await
        .map_err(|e| {
            SyncError::FileSystem(format!(
                "Failed to create symlink {} -> {}: {}",
                dst.display(),
                target.display(),
                e
            ))
        })?;

    debug!("Copied symlink {} -> {}", dst.display(), target.display());

    // Preserve symlink metadata using DirectoryFd operations with AT_SYMLINK_NOFOLLOW
    // These operate on the symlink itself, not its target

    // Get source symlink metadata
    let src_metadata = std::fs::symlink_metadata(src).map_err(|e| {
        SyncError::FileSystem(format!(
            "Failed to get symlink metadata for {}: {}",
            src.display(),
            e
        ))
    })?;

    // Preserve ownership (if requested and we have permissions)
    if metadata_config.archive || metadata_config.owner || metadata_config.group {
        use std::os::unix::fs::MetadataExt;
        let uid = src_metadata.uid();
        let gid = src_metadata.gid();

        // Use lfchownat which doesn't follow symlinks
        if let Err(e) = dst_dir_fd.lfchownat(&dst_name, uid, gid).await {
            // Don't fail if we can't change ownership (common for non-root)
            debug!(
                "Could not preserve symlink ownership (may need root): {}",
                e
            );
        }
    }

    // Preserve timestamps (if requested)
    if metadata_config.archive || metadata_config.times {
        use std::os::unix::fs::MetadataExt;

        // Include nanoseconds for full precision
        let atime = std::time::UNIX_EPOCH
            + std::time::Duration::from_secs(src_metadata.atime() as u64)
            + std::time::Duration::from_nanos(src_metadata.atime_nsec() as u64);
        let mtime = std::time::UNIX_EPOCH
            + std::time::Duration::from_secs(src_metadata.mtime() as u64)
            + std::time::Duration::from_nanos(src_metadata.mtime_nsec() as u64);

        // Use lutimensat which doesn't follow symlinks
        dst_dir_fd
            .lutimensat(&dst_name, atime, mtime)
            .await
            .map_err(|e| {
                SyncError::FileSystem(format!(
                    "Failed to preserve symlink timestamps for {}: {}",
                    dst.display(),
                    e
                ))
            })?;
    }

    Ok(())
}
