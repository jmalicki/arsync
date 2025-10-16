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
