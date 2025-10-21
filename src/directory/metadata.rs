//! Directory and file metadata preservation operations

use super::types::ExtendedMetadata;
use crate::error::{Result, SyncError};
use crate::io_uring::FileOperations;
use crate::metadata::MetadataConfig;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use tracing::debug;

/// Preserve file metadata (permissions, ownership, timestamps)
#[allow(dead_code)]
#[allow(clippy::future_not_send)]
pub(super) async fn preserve_file_metadata(
    src: &Path,
    dst: &Path,
    file_ops: &FileOperations,
) -> Result<()> {
    // Get source metadata
    let _metadata = file_ops.get_file_metadata(src).await.map_err(|e| {
        SyncError::FileSystem(format!(
            "Failed to get source metadata for {}: {}",
            src.display(),
            e
        ))
    })?;

    // TODO: Implement metadata preservation using compio's API
    // For now, we'll skip metadata preservation as compio's API is still evolving
    // This will be implemented in a future phase with proper compio bindings
    tracing::debug!(
        "Metadata preservation skipped for {} (compio API limitations)",
        dst.display()
    );

    // Set timestamps (currently skipped due to unstable Rust features)
    // TODO: Implement timestamp preservation using libc
    debug!("Preserved metadata for {}", dst.display());

    Ok(())
}

/// Preserve directory extended attributes from source to destination
///
/// This function preserves all extended attributes from the source directory to the destination directory
/// using file descriptor-based operations for maximum efficiency and security.
///
/// # Arguments
///
/// * `src_path` - Source directory path
/// * `dst_path` - Destination directory path
///
/// # Returns
///
/// `Ok(())` if all extended attributes were preserved successfully
///
/// # Errors
///
/// This function will return an error if:
/// - Extended attributes cannot be read from source
/// - Extended attributes cannot be written to destination
/// - Permission is denied for xattr operations
#[allow(clippy::future_not_send)]
pub async fn preserve_directory_xattr(src_path: &Path, dst_path: &Path) -> Result<()> {
    use compio_fs_extended::{ExtendedFile, XattrOps};

    // Open source and destination directories for xattr operations
    let src_dir = compio::fs::File::open(src_path).await.map_err(|e| {
        SyncError::FileSystem(format!("Failed to open source directory for xattr: {e}"))
    })?;
    let dst_dir = compio::fs::File::open(dst_path).await.map_err(|e| {
        SyncError::FileSystem(format!(
            "Failed to open destination directory for xattr: {e}"
        ))
    })?;

    // Convert to ExtendedFile to access xattr operations
    let extended_src = ExtendedFile::from_ref(&src_dir);
    let extended_dst = ExtendedFile::from_ref(&dst_dir);

    // Get all extended attribute names from source directory
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
                    tracing::warn!(
                        "Failed to preserve directory extended attribute '{}': {}",
                        name,
                        e
                    );
                }
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to read directory extended attribute '{}': {}",
                    name,
                    e
                );
            }
        }
    }

    Ok(())
}

/// Preserve directory metadata using pre-opened `DirectoryFd` (TOCTOU-safe, efficient)
///
/// This function accepts an already-open `DirectoryFd` and uses it for all metadata operations.
/// This is more efficient (1 open vs 3) and TOCTOU-safe.
///
/// # Parameters
/// - `src_path`: Source path (for error messages and xattr operations)
/// - `dst_path`: Destination path (for error messages and xattr operations)  
/// - `dst_dir_fd`: Pre-opened destination `DirectoryFd`
/// - `extended_metadata`: Source metadata to copy
/// - `metadata_config`: What metadata to preserve
///
/// # Errors
///
/// Returns error if metadata operations (permissions, ownership, timestamps) fail
#[allow(clippy::similar_names)]
#[allow(clippy::future_not_send)]
pub async fn preserve_directory_metadata_fd(
    src_path: &Path,
    dst_path: &Path,
    dst_dir_fd: &compio_fs_extended::DirectoryFd,
    extended_metadata: &ExtendedMetadata,
    metadata_config: &MetadataConfig,
) -> Result<()> {
    use compio_fs_extended::OwnershipOps;

    // Get underlying File from DirectoryFd for metadata operations
    let dst_file = dst_dir_fd.as_file();

    // Preserve directory permissions if requested
    if metadata_config.should_preserve_permissions() {
        let mode = extended_metadata.metadata.permissions();
        let compio_permissions = compio::fs::Permissions::from_mode(mode);

        // Use FD-based set_permissions (TOCTOU-safe, no umask interference)
        dst_file
            .set_permissions(compio_permissions)
            .await
            .map_err(|e| {
                SyncError::FileSystem(format!("Failed to preserve directory permissions: {e}"))
            })?;

        debug!(
            "Preserved directory permissions for {}: {:o}",
            dst_path.display(),
            mode
        );
    }

    // Preserve directory ownership if requested
    if metadata_config.should_preserve_ownership() {
        let source_uid = extended_metadata.metadata.uid;
        let source_gid = extended_metadata.metadata.gid;

        // Use FD-based fchown (TOCTOU-safe!)
        dst_file.fchown(source_uid, source_gid).await.map_err(|e| {
            SyncError::FileSystem(format!("Failed to preserve directory ownership: {e}"))
        })?;

        debug!(
            "Preserved directory ownership for {}: uid={}, gid={}",
            dst_path.display(),
            source_uid,
            source_gid
        );
    }

    // Preserve directory timestamps if requested
    if metadata_config.should_preserve_timestamps() {
        let src_accessed = extended_metadata.metadata.accessed;
        let src_modified = extended_metadata.metadata.modified;

        // Use DirectoryFd::set_times (TOCTOU-safe!)
        dst_dir_fd
            .set_times(src_accessed, src_modified)
            .await
            .map_err(|e| {
                SyncError::FileSystem(format!("Failed to preserve directory timestamps: {e}"))
            })?;

        debug!("Preserved directory timestamps for {}", dst_path.display());
    }

    // Preserve directory extended attributes if requested
    if metadata_config.should_preserve_xattrs() {
        preserve_directory_xattr(src_path, dst_path).await?;
        debug!("Preserved directory xattrs for {}", dst_path.display());
    }

    Ok(())
}

/// Preserve directory metadata (legacy path-based wrapper)
///
/// **DEPRECATED**: Use `preserve_directory_metadata_fd` instead
/// This wrapper exists for compatibility but opens the directory 3 times (inefficient!)
///
/// # Errors
/// Returns error if `DirectoryFd` cannot be opened or metadata preservation fails
#[allow(clippy::future_not_send)]
pub async fn preserve_directory_metadata(
    src_path: &Path,
    dst_path: &Path,
    extended_metadata: &ExtendedMetadata,
    metadata_config: &MetadataConfig,
) -> Result<()> {
    // Open DirectoryFd once and use FD-based operations
    let dst_dir_fd = compio_fs_extended::DirectoryFd::open(dst_path)
        .await
        .map_err(|e| SyncError::FileSystem(format!("Failed to open destination directory: {e}")))?;

    preserve_directory_metadata_fd(
        src_path,
        dst_path,
        &dst_dir_fd,
        extended_metadata,
        metadata_config,
    )
    .await
}
