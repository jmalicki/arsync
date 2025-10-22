//! Directory traversal and copying functionality
//!
//! This module provides async directory traversal and copying capabilities
//! using `io_uring` operations where possible, with fallbacks to standard
//! filesystem operations for unsupported operations.
//!
//! # Module Organization
//!
//! - `types`: Core data structures (`FileLocation`, `TraversalContext`, etc.)
//! - `symlink`: Symlink copying and metadata preservation
//! - `metadata`: Directory metadata preservation operations
//! - `traversal`: Recursive directory traversal logic
//! - `mod`: Public API and module coordination (this file)

// Disallow std::fs usage in this module to enforce async filesystem operations
#![deny(clippy::disallowed_methods)]

mod metadata;
mod symlink;
mod traversal;
mod types;

// Re-export public types
#[allow(unused_imports)] // Used by external modules
pub use types::{metadata_from_path, DirectoryStats, FileLocation, TraversalContext};

// Re-export public functions
#[allow(unused_imports)] // Used by external modules
pub use metadata::{
    preserve_directory_metadata, preserve_directory_metadata_fd, preserve_directory_xattr,
};

use crate::cli::{Args, CopyMethod};
use crate::error::{Result, SyncError};
use crate::hardlink_tracker::FilesystemTracker;
use crate::io_uring::FileOperations;
use std::path::Path;
use tracing::{debug, info};

/// Copy an entire directory tree from source to destination
///
/// Recursively copies all files, directories, and symlinks from `src` to `dst`,
/// preserving metadata according to the configuration in `args`. Uses async
/// `io_uring` operations for optimal performance and TOCTOU-safe DirectoryFd-based
/// operations for security.
///
/// # Arguments
///
/// * `src` - Source directory path to copy from
/// * `dst` - Destination directory path to copy to
/// * `file_ops` - File operations handler containing copy configuration
/// * `_copy_method` - Copy method (e.g., auto, `copy_file_range`, splice)
/// * `args` - Command-line arguments containing metadata and concurrency config
///
/// # Returns
///
/// Returns `DirectoryStats` containing operation counts (files copied, directories
/// created, bytes transferred, symlinks processed, hardlinks detected).
///
/// # Errors
///
/// Returns error if:
/// - Source directory doesn't exist or isn't accessible
/// - Destination directory can't be created
/// - File/directory operations fail during traversal
/// - Metadata preservation fails
#[allow(clippy::future_not_send)]
#[allow(clippy::used_underscore_binding)]
pub async fn copy_directory(
    src: &Path,
    dst: &Path,
    file_ops: &FileOperations,
    _copy_method: CopyMethod,
    args: &Args,
) -> Result<DirectoryStats> {
    let mut stats = DirectoryStats::default();
    let mut hardlink_tracker = FilesystemTracker::new();

    info!(
        "Starting directory copy from {} to {}",
        src.display(),
        dst.display()
    );

    // Create destination directory if it doesn't exist
    if dst.exists() {
        // Set source filesystem from root directory (destination already exists)
        let root_metadata = types::metadata_from_path(src).await?;
        hardlink_tracker.set_source_filesystem(root_metadata.dev);
    } else {
        compio::fs::create_dir_all(dst).await.map_err(|e| {
            SyncError::FileSystem(format!(
                "Failed to create destination directory {}: {}",
                dst.display(),
                e
            ))
        })?;
        stats.directories_created += 1;
        debug!("Created destination directory: {}", dst.display());

        // Preserve root directory metadata (permissions, ownership, timestamps) if requested
        let root_metadata = types::metadata_from_path(src).await?;
        metadata::preserve_directory_metadata(src, dst, &root_metadata, &args.metadata).await?;

        // Set source filesystem from root directory
        hardlink_tracker.set_source_filesystem(root_metadata.dev);
    }

    // Traverse source directory iteratively using compio's dispatcher
    traversal::traverse_and_copy_directory_iterative(
        src.to_path_buf(),
        dst.to_path_buf(),
        file_ops,
        _copy_method,
        &mut stats,
        &mut hardlink_tracker,
        &args.metadata,
        &args.concurrency,
        &args.io.parallel,
    )
    .await?;

    // Log hardlink detection results
    let hardlink_stats = hardlink_tracker.get_stats();
    info!(
        "Directory copy completed: {} files, {} directories, {} bytes, {} symlinks",
        stats.files_copied, stats.directories_created, stats.bytes_copied, stats.symlinks_processed
    );
    if hardlink_stats.hardlink_groups > 0 {
        info!(
            "Hardlink detection: {} unique files, {} hardlink groups, {} total hardlinks",
            hardlink_stats.total_files,
            hardlink_stats.hardlink_groups,
            hardlink_stats.total_hardlinks
        );
    }

    Ok(stats)
}
#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    #![allow(clippy::expect_used)]
    #![allow(clippy::disallowed_methods)]
    use super::*;
    use crate::stats::SharedStats;
    use std::os::unix::fs::MetadataExt;
    use std::sync::Arc;
    use tempfile::TempDir;

    /// Test ExtendedMetadata creation and basic functionality
    #[compio::test]
    async fn test_file_metadata_basic() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let test_file = temp_dir.path().join("test_file.txt");

        // Create a test file
        std::fs::write(&test_file, "test content").expect("Failed to write test file");

        // Test metadata creation
        let metadata = types::metadata_from_path(&test_file)
            .await
            .expect("Failed to get metadata");

        // Test basic properties
        assert!(metadata.is_file());
        assert!(!metadata.is_dir());
        assert!(!metadata.is_symlink());
        assert!(metadata.size > 0);
        assert_eq!(metadata.size, 12); // "test content" length

        // Test device and inode info
        assert!(metadata.dev > 0);
        assert!(metadata.ino > 0);
        assert_eq!(metadata.nlink, 1); // Regular file has 1 link
    }

    /// Test FileMetadata for directories
    #[compio::test]
    async fn test_file_metadata_directory() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");

        // Test directory metadata
        let metadata = types::metadata_from_path(temp_dir.path())
            .await
            .expect("Failed to get metadata");

        assert!(metadata.is_dir());
        assert!(!metadata.is_file());
        assert!(!metadata.is_symlink());
    }

    /// Test FileMetadata for symlinks
    #[compio::test]
    async fn test_file_metadata_symlink() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let target_file = temp_dir.path().join("target.txt");
        let symlink_file = temp_dir.path().join("symlink.txt");

        // Create target file
        std::fs::write(&target_file, "target content").expect("Failed to write target file");

        // Create symlink
        std::os::unix::fs::symlink(&target_file, &symlink_file).expect("Failed to create symlink");

        // Test symlink metadata
        let metadata = types::metadata_from_path(&symlink_file)
            .await
            .expect("Failed to get metadata");

        assert!(metadata.is_symlink());
        assert!(!metadata.is_file());
        assert!(!metadata.is_dir());
    }

    /// Test process_symlink function
    #[compio::test]
    async fn test_process_symlink() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let target_file = temp_dir.path().join("target.txt");
        let src_symlink = temp_dir.path().join("src_symlink");
        let dst_symlink = temp_dir.path().join("dst_symlink");

        // Create target file
        std::fs::write(&target_file, "target content").expect("Failed to write target file");

        // Create source symlink
        std::os::unix::fs::symlink(&target_file, &src_symlink)
            .expect("Failed to create source symlink");

        // Test symlink processing
        let stats = DirectoryStats::default();
        let result = symlink::process_symlink(
            src_symlink.clone(),
            dst_symlink.clone(),
            &crate::metadata::MetadataConfig {
                archive: false,
                recursive: false,
                links: true,
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
            },
            Arc::new(SharedStats::new(&stats)),
        )
        .await;

        // Should succeed
        assert!(result.is_ok());

        // Verify symlink was created
        assert!(dst_symlink.exists());
        assert!(dst_symlink.is_symlink());

        // Verify symlink target is correct
        let target = std::fs::read_link(&dst_symlink).expect("Failed to read symlink target");
        assert_eq!(target, target_file);
    }

    /// Test process_symlink with broken symlink
    #[compio::test]
    async fn test_process_symlink_broken() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let src_symlink = temp_dir.path().join("broken_symlink");
        let dst_symlink = temp_dir.path().join("dst_broken_symlink");

        // Create broken symlink
        std::os::unix::fs::symlink("nonexistent_file", &src_symlink)
            .expect("Failed to create broken symlink");

        // Test processing broken symlink
        let stats = DirectoryStats::default();
        let result = symlink::process_symlink(
            src_symlink.clone(),
            dst_symlink.clone(),
            &crate::metadata::MetadataConfig {
                archive: false,
                recursive: false,
                links: true,
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
            },
            Arc::new(SharedStats::new(&stats)),
        )
        .await;

        // Should succeed (we handle broken symlinks gracefully)
        assert!(result.is_ok());

        // Verify symlink was created (broken symlinks don't "exist" but are still symlinks)
        assert!(dst_symlink.is_symlink());

        // Verify the broken symlink target is preserved
        let target = std::fs::read_link(&dst_symlink).expect("Failed to read symlink target");
        assert_eq!(target.to_string_lossy(), "nonexistent_file");
    }

    /// Test that handle_existing_hardlink updates stats
    /// This verifies that the linker path properly increments files_copied
    #[compio::test]
    async fn test_hardlink_updates_stats() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let src_file = temp_dir.path().join("src.txt");
        let dst_file = temp_dir.path().join("dst.txt");
        let link_file = temp_dir.path().join("link.txt");

        // Create source file
        std::fs::write(&src_file, "test content").expect("Failed to write file");

        // Create destination file (simulating copier's work)
        std::fs::write(&dst_file, "test content").expect("Failed to write dst");

        // Test handle_existing_hardlink
        let stats = Arc::new(SharedStats::new(&DirectoryStats::default()));

        let result =
            traversal::handle_existing_hardlink(&link_file, &dst_file, 12345, &stats).await;

        assert!(result.is_ok(), "handle_existing_hardlink should succeed");

        // CRITICAL: Check that stats were updated
        let final_stats = Arc::try_unwrap(stats).unwrap().into_inner();
        assert_eq!(
            final_stats.files_copied, 1,
            "handle_existing_hardlink should increment files_copied"
        );

        // Verify hardlink was actually created
        assert!(link_file.exists(), "Hardlink should exist");
        let link_meta = std::fs::metadata(&link_file).expect("Failed to get link metadata");
        let dst_meta = std::fs::metadata(&dst_file).expect("Failed to get dst metadata");
        assert_eq!(
            link_meta.ino(),
            dst_meta.ino(),
            "Should be same inode (hardlink)"
        );
    }

    /// ERROR INJECTION: Test handle_existing_hardlink fails on nonexistent original
    #[compio::test]
    async fn test_hardlink_error_on_missing_original() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let link_file = temp_dir.path().join("link.txt");
        let nonexistent = temp_dir.path().join("nonexistent.txt");

        let stats = Arc::new(SharedStats::new(&DirectoryStats::default()));

        // Try to create hardlink to nonexistent file - should fail
        let result =
            traversal::handle_existing_hardlink(&link_file, &nonexistent, 12345, &stats).await;

        assert!(
            result.is_err(),
            "Creating hardlink to nonexistent file should fail"
        );

        // With clean error propagation, stats are NOT incremented on error
        // Error propagates via Result instead
        let final_stats = Arc::try_unwrap(stats).unwrap().into_inner();
        assert_eq!(
            final_stats.errors, 0,
            "Error counter should NOT be incremented (error propagates via Result)"
        );
        assert_eq!(
            final_stats.files_copied, 0,
            "files_copied should not be incremented on failure"
        );
    }

    /// ERROR INJECTION: Test handle_existing_hardlink fails when dst_path parent is read-only
    #[compio::test]
    async fn test_hardlink_error_on_readonly_parent() {
        use std::fs::Permissions;
        use std::os::unix::fs::PermissionsExt;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let readonly_dir = temp_dir.path().join("readonly");
        std::fs::create_dir(&readonly_dir).expect("Failed to create readonly dir");

        // Make directory read-only
        std::fs::set_permissions(&readonly_dir, Permissions::from_mode(0o444))
            .expect("Failed to set permissions");

        let src_file = temp_dir.path().join("src.txt");
        let dst_file = temp_dir.path().join("dst.txt");
        let link_file = readonly_dir.join("link.txt");

        std::fs::write(&src_file, "content").expect("Failed to write src");
        std::fs::write(&dst_file, "content").expect("Failed to write dst");

        let stats = Arc::new(SharedStats::new(&DirectoryStats::default()));

        // Try to create hardlink in read-only directory - should fail
        let result =
            traversal::handle_existing_hardlink(&link_file, &dst_file, 12345, &stats).await;

        assert!(
            result.is_err(),
            "Creating hardlink in read-only directory should fail"
        );

        // Clean up permissions for temp_dir cleanup
        std::fs::set_permissions(&readonly_dir, Permissions::from_mode(0o755))
            .expect("Failed to restore permissions");
    }
}
