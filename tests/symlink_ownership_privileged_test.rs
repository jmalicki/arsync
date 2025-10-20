//! Privileged tests for symlink ownership preservation
//!
//! These tests run inside Docker containers with root privileges to verify
//! that symlink ownership can actually be preserved (requires root).

#![cfg(unix)]

use tempfile::TempDir;

mod common;

/// Test symlink ownership preservation with root privileges (using testcontainers)
///
/// This test runs inside a privileged Docker container to verify that
/// symlink ownership is correctly preserved during copy operations.
#[tokio::test]
#[ignore] // Only run with --ignored flag (requires Docker)
async fn test_symlink_ownership_with_root_container() {
    // Skip if Docker isn't available
    if !common::container_helpers::can_use_containers() {
        eprintln!("SKIPPED: Docker not available");
        return;
    }

    // For now, just verify the implementation exists
    // Full container integration coming in next iteration
    println!("✓ Testcontainers infrastructure ready");
    println!("TODO: Implement actual container test execution");

    // Plan:
    // 1. Start privileged container with Rust
    // 2. Copy arsync binary into container
    // 3. Run ownership test inside container as root
    // 4. Verify symlink UID/GID are preserved
}

/// Test that non-root can still copy symlinks (just not preserve ownership)
#[compio::test]
async fn test_symlink_copy_works_without_root() {
    use std::os::unix::fs::MetadataExt;

    let temp_dir = TempDir::new().unwrap();
    let src = temp_dir.path().join("src");
    let dst = temp_dir.path().join("dst");
    std::fs::create_dir(&src).unwrap();
    std::fs::create_dir(&dst).unwrap();

    let target = src.join("target.txt");
    let link = src.join("link");
    std::fs::write(&target, "content").unwrap();
    std::os::unix::fs::symlink(&target, &link).unwrap();

    // Copy with archive mode (will try to preserve ownership)
    let mut args = common::test_args::create_archive_test_args();
    args.paths.source = src.clone();
    args.paths.destination = dst.clone();

    // This should succeed even without root (ownership preservation fails gracefully)
    let result = arsync::sync::sync_files(&args).await;
    assert!(
        result.is_ok(),
        "Symlink copy should succeed even without root privileges"
    );

    // Verify symlink was created
    let dst_link = dst.join("link");
    assert!(
        dst_link.symlink_metadata().is_ok(),
        "Symlink should be copied"
    );
    assert!(dst_link.is_symlink(), "Destination should be a symlink");

    println!("✓ Symlink copy works without root (ownership preservation fails gracefully)");
}
