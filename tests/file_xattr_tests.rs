//! Tests for file extended attributes (xattr) preservation

use arsync::metadata::preserve_xattr_from_fd;
use compio::fs;
use compio_fs_extended::{ExtendedFile, XattrOps};
use tempfile::TempDir;

/// Test basic extended attributes preservation
#[compio::test]
async fn test_file_xattr_preservation() {
    let temp_dir = TempDir::new().unwrap();
    let src_path = temp_dir.path().join("source.txt");
    let dst_path = temp_dir.path().join("destination.txt");

    // Create source file with content
    fs::write(&src_path, "Hello, World!").await.unwrap();

    // Set extended attributes on source file
    let src_file = fs::File::open(&src_path).await.unwrap();
    let extended_src = ExtendedFile::from_ref(&src_file);

    // Set some test xattrs
    extended_src
        .set_xattr("user.test", b"test_value")
        .await
        .unwrap();
    extended_src
        .set_xattr("user.description", b"Test file for xattr preservation")
        .await
        .unwrap();

    // Create destination file
    fs::write(&dst_path, "Hello, World!").await.unwrap();

    // Test xattr preservation
    let dst_file = fs::File::open(&dst_path).await.unwrap();
    preserve_xattr_from_fd(&src_file, &dst_file).await.unwrap();

    // Verify xattrs were preserved
    let extended_dst = ExtendedFile::from_ref(&dst_file);
    let test_value = extended_dst.get_xattr("user.test").await.unwrap();
    let description_value = extended_dst.get_xattr("user.description").await.unwrap();

    assert_eq!(test_value, b"test_value");
    assert_eq!(description_value, b"Test file for xattr preservation");
}

/// Test xattr preservation with no xattrs
#[compio::test]
async fn test_file_xattr_preservation_no_xattrs() {
    let temp_dir = TempDir::new().unwrap();
    let src_path = temp_dir.path().join("source.txt");
    let dst_path = temp_dir.path().join("destination.txt");

    // Create source file with content (no xattrs)
    fs::write(&src_path, "Hello, World!").await.unwrap();

    // Create destination file
    fs::write(&dst_path, "Hello, World!").await.unwrap();

    // Test xattr preservation (should not fail)
    let src_file = fs::File::open(&src_path).await.unwrap();
    let dst_file = fs::File::open(&dst_path).await.unwrap();
    preserve_xattr_from_fd(&src_file, &dst_file).await.unwrap();

    // Verify no xattrs were set
    let extended_dst = ExtendedFile::from_ref(&dst_file);
    let xattr_list = extended_dst.list_xattr().await.unwrap();
    assert!(xattr_list.is_empty());
}

/// Test xattr preservation with multiple xattrs
#[compio::test]
async fn test_file_xattr_preservation_multiple() {
    let temp_dir = TempDir::new().unwrap();
    let src_path = temp_dir.path().join("source.txt");
    let dst_path = temp_dir.path().join("destination.txt");

    // Create source file with content
    fs::write(&src_path, "Hello, World!").await.unwrap();

    // Set multiple extended attributes on source file
    let src_file = fs::File::open(&src_path).await.unwrap();
    let extended_src = ExtendedFile::from_ref(&src_file);

    let xattrs = vec![
        ("user.test1", b"value1".as_slice()),
        ("user.test2", b"value2".as_slice()),
        ("user.test3", b"value3".as_slice()),
        ("user.description", b"Multiple xattrs test".as_slice()),
    ];

    for (name, value) in &xattrs {
        extended_src.set_xattr(name, value).await.unwrap();
    }

    // Create destination file
    fs::write(&dst_path, "Hello, World!").await.unwrap();

    // Test xattr preservation
    let dst_file = fs::File::open(&dst_path).await.unwrap();
    preserve_xattr_from_fd(&src_file, &dst_file).await.unwrap();

    // Verify all xattrs were preserved
    let extended_dst = ExtendedFile::from_ref(&dst_file);
    for (name, expected_value) in &xattrs {
        let actual_value = extended_dst.get_xattr(name).await.unwrap();
        assert_eq!(actual_value, *expected_value);
    }
}

/// Test xattr preservation with binary data
#[compio::test]
async fn test_file_xattr_preservation_binary_data() {
    let temp_dir = TempDir::new().unwrap();
    let src_path = temp_dir.path().join("source.txt");
    let dst_path = temp_dir.path().join("destination.txt");

    // Create source file with content
    fs::write(&src_path, "Hello, World!").await.unwrap();

    // Set binary extended attribute on source file
    let src_file = fs::File::open(&src_path).await.unwrap();
    let extended_src = ExtendedFile::from_ref(&src_file);

    let binary_data = vec![0x00, 0x01, 0x02, 0x03, 0xFF, 0xFE, 0xFD, 0xFC];
    extended_src
        .set_xattr("user.binary", &binary_data)
        .await
        .unwrap();

    // Create destination file
    fs::write(&dst_path, "Hello, World!").await.unwrap();

    // Test xattr preservation
    let dst_file = fs::File::open(&dst_path).await.unwrap();
    preserve_xattr_from_fd(&src_file, &dst_file).await.unwrap();

    // Verify binary xattr was preserved
    let extended_dst = ExtendedFile::from_ref(&dst_file);
    let preserved_data = extended_dst.get_xattr("user.binary").await.unwrap();
    assert_eq!(preserved_data, binary_data);
}

/// Test xattr preservation error handling
#[compio::test]
async fn test_file_xattr_preservation_error_handling() {
    let temp_dir = TempDir::new().unwrap();
    let src_path = temp_dir.path().join("source.txt");
    let dst_path = temp_dir.path().join("destination.txt");

    // Create source file with content
    fs::write(&src_path, "Hello, World!").await.unwrap();

    // Set extended attribute on source file
    let src_file = fs::File::open(&src_path).await.unwrap();
    let extended_src = ExtendedFile::from_ref(&src_file);
    extended_src
        .set_xattr("user.test", b"test_value")
        .await
        .unwrap();

    // Create destination file
    fs::write(&dst_path, "Hello, World!").await.unwrap();

    // Test xattr preservation (should not fail even if some xattrs can't be set)
    let dst_file = fs::File::open(&dst_path).await.unwrap();
    let result = preserve_xattr_from_fd(&src_file, &dst_file).await;

    // Should succeed (warnings are logged but don't fail the operation)
    assert!(result.is_ok());
}

/// Test that demonstrates the bug: get_xattr_at_path follows symlinks
///
/// This test SHOULD FAIL with current implementation because:
/// - We set different xattrs on the symlink vs its target
/// - get_xattr_at_path incorrectly follows the symlink
/// - So it returns the target's xattr instead of the symlink's xattr
///
/// **BUG**: Current implementation uses getxattr() which follows symlinks by default.
/// **FIX**: Use lget_xattr_at_path() which uses lgetxattr() on Linux or XATTR_NOFOLLOW on macOS.
#[compio::test]
#[cfg(unix)] // xattrs are Unix-only
async fn test_symlink_xattr_bug_demonstration() {
    use compio_fs_extended::xattr::{get_xattr_at_path, set_xattr_at_path};

    let temp_dir = TempDir::new().unwrap();
    let target_path = temp_dir.path().join("target.txt");
    let link_path = temp_dir.path().join("link.txt");

    // Create target file
    std::fs::write(&target_path, "target content").unwrap();

    // Create symlink
    #[cfg(unix)]
    std::os::unix::fs::symlink(&target_path, &link_path).unwrap();

    // Set xattr on target directly
    set_xattr_at_path(&target_path, "user.type", b"target_file")
        .await
        .unwrap();

    // Now try to set DIFFERENT xattr via symlink path
    // Current implementation WRONGLY follows the symlink and overwrites the target's xattr!
    set_xattr_at_path(&link_path, "user.type", b"symlink_node")
        .await
        .unwrap();

    // Read xattr directly from target
    let target_xattr = get_xattr_at_path(&target_path, "user.type").await.unwrap();

    // THIS TEST DEMONSTRATES THE BUG:
    // When we set xattr via symlink path, it follows the symlink and modifies the target
    // So target's xattr is now "symlink_node" instead of "target_file"
    //
    // Expected behavior: symlink operations should NOT affect the target
    // Actual behavior: set_xattr_at_path follows symlinks (WRONG!)
    assert_eq!(
        target_xattr,
        b"target_file",
        "BUG DEMONSTRATED: set_xattr_at_path followed the symlink and overwrote target's xattr! \
         Expected target to still have 'target_file', but got '{}'",
        String::from_utf8_lossy(&target_xattr)
    );
}

/// Test the FIX: lget_xattr_at_path doesn't follow symlinks
///
/// This test SHOULD PASS when using the new l* functions.
/// It demonstrates the correct behavior for symlink xattrs.
#[compio::test]
#[cfg(unix)]
async fn test_symlink_xattr_correct_behavior() {
    use compio_fs_extended::xattr::{lget_xattr_at_path, lset_xattr_at_path};

    let temp_dir = TempDir::new().unwrap();
    let target_path = temp_dir.path().join("target.txt");
    let link_path = temp_dir.path().join("link.txt");

    // Create target file
    std::fs::write(&target_path, "target content").unwrap();

    // Create symlink
    #[cfg(unix)]
    std::os::unix::fs::symlink(&target_path, &link_path).unwrap();

    // Set xattr on target directly
    lset_xattr_at_path(&target_path, "user.type", b"target_file")
        .await
        .unwrap();

    // Try to set DIFFERENT xattr via symlink path
    // With l* functions, this should NOT follow the symlink
    // Note: On many filesystems, symlinks can't have xattrs, so this may fail
    let symlink_set_result = lset_xattr_at_path(&link_path, "user.type", b"symlink_node").await;

    // Read xattr directly from target using l* function
    let target_xattr = lget_xattr_at_path(&target_path, "user.type").await.unwrap();

    // The fix ensures target's xattr is NOT modified when we operate on symlink
    assert_eq!(
        target_xattr, b"target_file",
        "Target's xattr should remain unchanged when operating on symlink path"
    );

    // If the filesystem supports symlink xattrs, verify they're separate
    if symlink_set_result.is_ok() {
        let symlink_xattr = lget_xattr_at_path(&link_path, "user.type").await.unwrap();
        assert_eq!(
            symlink_xattr, b"symlink_node",
            "Symlink should have its own xattr"
        );
        assert_ne!(
            symlink_xattr, target_xattr,
            "Symlink and target should have different xattrs"
        );
    }
    // Note: If symlink_set_result.is_err(), the filesystem doesn't support symlink xattrs
    // which is fine - the important thing is that we DON'T modify the target
}
