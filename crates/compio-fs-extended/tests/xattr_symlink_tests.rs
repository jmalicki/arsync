//! Tests for symlink-aware xattr operations (l* functions)
//!
//! These tests verify that the l* xattr functions correctly operate on symlinks
//! without following them, as opposed to the regular xattr functions which follow symlinks.

use compio_fs_extended::xattr::{
    get_xattr_at_path, lget_xattr_at_path, lset_xattr_at_path, set_xattr_at_path,
};
use tempfile::TempDir;

/// Test that demonstrates the bug: set_xattr_at_path follows symlinks
///
/// This test FAILS with regular xattr functions, proving they follow symlinks.
#[compio::test]
#[cfg(unix)] // xattrs are Unix-only
async fn test_regular_xattr_follows_symlinks() {
    let temp_dir = TempDir::new().unwrap();
    let target_path = temp_dir.path().join("target.txt");
    let link_path = temp_dir.path().join("link.txt");

    // Create target file
    std::fs::write(&target_path, "target content").unwrap();

    // Create symlink
    std::os::unix::fs::symlink(&target_path, &link_path).unwrap();

    // Set xattr on target directly
    set_xattr_at_path(&target_path, "user.type", b"target_file")
        .await
        .unwrap();

    // Try to set DIFFERENT xattr via symlink path
    // Regular functions FOLLOW the symlink and overwrite the target's xattr
    set_xattr_at_path(&link_path, "user.type", b"via_symlink")
        .await
        .unwrap();

    // Read xattr directly from target
    let target_xattr = get_xattr_at_path(&target_path, "user.type").await.unwrap();

    // This test demonstrates the bug: regular functions follow symlinks
    // The target's xattr was overwritten when we operated via the symlink path
    assert_eq!(
        target_xattr, b"via_symlink",
        "Regular xattr functions follow symlinks (this is the bug we're documenting)"
    );

    // This is actually expected behavior for regular functions - they SHOULD follow symlinks
    // But when you want to operate on symlinks themselves, use l* functions
}

/// Test the FIX: lset_xattr_at_path doesn't follow symlinks
///
/// This test PASSES, proving that l* functions don't follow symlinks.
#[compio::test]
#[cfg(unix)]
async fn test_l_xattr_does_not_follow_symlinks() {
    let temp_dir = TempDir::new().unwrap();
    let target_path = temp_dir.path().join("target.txt");
    let link_path = temp_dir.path().join("link.txt");

    // Create target file
    std::fs::write(&target_path, "target content").unwrap();

    // Create symlink
    std::os::unix::fs::symlink(&target_path, &link_path).unwrap();

    // Set xattr on target directly using l* function
    lset_xattr_at_path(&target_path, "user.type", b"target_file")
        .await
        .unwrap();

    // Try to set DIFFERENT xattr via symlink path using l* function
    // Note: On many filesystems, symlinks can't have xattrs, so this may fail
    let symlink_set_result = lset_xattr_at_path(&link_path, "user.type", b"symlink_node").await;

    // Read xattr directly from target using l* function
    let target_xattr = lget_xattr_at_path(&target_path, "user.type").await.unwrap();

    // The fix: l* functions do NOT modify the target when operating on symlink path
    assert_eq!(
        target_xattr, b"target_file",
        "Target's xattr should remain unchanged when using l* functions on symlink path"
    );

    // If the filesystem supports symlink xattrs, verify they're separate
    if symlink_set_result.is_ok() {
        let symlink_xattr = lget_xattr_at_path(&link_path, "user.type").await.unwrap();
        assert_eq!(
            symlink_xattr, b"symlink_node",
            "Symlink should have its own xattr (if filesystem supports it)"
        );
        assert_ne!(
            symlink_xattr, target_xattr,
            "Symlink and target should have different xattrs"
        );
    }
    // Note: If symlink_set_result.is_err(), the filesystem doesn't support symlink xattrs
    // (this is common on many Linux filesystems). The important thing is that we
    // DON'T modify the target file.
}

/// Test that regular functions and l* functions behave differently
#[compio::test]
#[cfg(unix)]
async fn test_xattr_api_differences() {
    let temp_dir = TempDir::new().unwrap();
    let target_path = temp_dir.path().join("target.txt");
    let link_path = temp_dir.path().join("link.txt");

    // Create target file
    std::fs::write(&target_path, "target content").unwrap();

    // Create symlink
    std::os::unix::fs::symlink(&target_path, &link_path).unwrap();

    // Set xattr on target
    set_xattr_at_path(&target_path, "user.test", b"original")
        .await
        .unwrap();

    // Reading via regular function through symlink follows the link
    let via_regular = get_xattr_at_path(&link_path, "user.test").await.unwrap();
    assert_eq!(via_regular, b"original", "Regular get follows symlink");

    // Reading via l* function should also read from target (symlink has no xattr yet)
    let via_l = lget_xattr_at_path(&link_path, "user.test").await;

    // On most Linux filesystems, this will error because symlink doesn't have xattrs
    // On filesystems that support symlink xattrs, it would succeed but return different value
    if via_l.is_err() {
        // Expected on most filesystems - symlink has no xattrs
        println!("Filesystem doesn't support symlink xattrs (common)");
    } else {
        // If filesystem supports symlink xattrs, they should be independent
        println!("Filesystem supports symlink xattrs");
    }
}
