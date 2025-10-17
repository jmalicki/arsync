//! Simple integration tests for compio-fs-extended
//!
//! These tests exercise the core functionality with real files to verify
//! that io_uring operations work correctly in practice.

use compio::fs::File;
use compio_fs_extended::*;
use std::fs;
use tempfile::TempDir;

/// Test fadvise operations on real files
#[compio::test]
async fn test_fadvise_basic() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.txt");

    // Create a test file
    fs::write(&file_path, "Test content for fadvise").unwrap();

    // Open file and test fadvise
    let file = File::open(&file_path).await.unwrap();

    // Test different fadvise patterns
    fadvise::fadvise(&file, fadvise::FadviseAdvice::Sequential, 0, 0)
        .await
        .unwrap();
    fadvise::fadvise(&file, fadvise::FadviseAdvice::Random, 0, 0)
        .await
        .unwrap();
    fadvise::fadvise(&file, fadvise::FadviseAdvice::Normal, 0, 0)
        .await
        .unwrap();
}

/// Test symlink operations
#[compio::test]
async fn test_symlink_basic() {
    let temp_dir = TempDir::new().unwrap();
    let source_file = temp_dir.path().join("source.txt");
    let symlink_path = temp_dir.path().join("test_symlink");

    // Create source file
    fs::write(&source_file, "Source content").unwrap();

    // Remove symlink if it exists
    if symlink_path.exists() {
        fs::remove_file(&symlink_path).unwrap();
    }

    // Create symlink
    async {
        let dir_fd = crate::directory::DirectoryFd::open(temp_dir.path())
            .await
            .unwrap();
        dir_fd
            .symlinkat(
                &source_file.file_name().unwrap().to_string_lossy(),
                "test_symlink",
            )
            .await
    }
    .await
    .unwrap();

    // Verify symlink exists
    assert!(symlink_path.exists());

    // Check if it's actually a symlink
    let metadata = fs::symlink_metadata(&symlink_path).unwrap();
    println!("Symlink metadata: {:?}", metadata.file_type());
    println!("Is symlink: {}", metadata.file_type().is_symlink());

    // Read symlink target
    let target = async {
        let dir_fd = crate::directory::DirectoryFd::open(temp_dir.path())
            .await
            .unwrap();
        dir_fd.readlinkat("test_symlink").await
    }
    .await
    .unwrap();
    assert_eq!(target, std::path::PathBuf::from("source.txt"));

    // Verify symlink content
    let content = fs::read_to_string(&symlink_path).unwrap();
    assert_eq!(content, "Source content");
}

/// Test directory operations
#[compio::test]
async fn test_directory_basic() {
    let temp_dir = TempDir::new().unwrap();
    let dir_path = temp_dir.path().join("test_dir");

    // Create directory
    compio::fs::create_dir(&dir_path).await.unwrap();
    assert!(dir_path.exists());

    // Create directory with specific mode
    let dir_path2 = temp_dir.path().join("test_dir2");
    compio::fs::create_dir(&dir_path2).await.unwrap();
    assert!(dir_path2.exists());

    // Remove directory
    compio::fs::remove_dir(&dir_path2).await.unwrap();
    assert!(!dir_path2.exists());
}

/// Test extended attributes
#[compio::test]
async fn test_xattr_basic() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.txt");

    // Create test file
    fs::write(&file_path, "Test content").unwrap();

    // Test path-based xattr operations
    xattr::set_xattr_at_path(&file_path, "user.test", b"test_value")
        .await
        .unwrap();
    let value = xattr::get_xattr_at_path(&file_path, "user.test")
        .await
        .unwrap();
    assert_eq!(value, b"test_value");

    // Test additional xattr operations
    xattr::set_xattr_at_path(&file_path, "user.test2", b"test_value2")
        .await
        .unwrap();
    let value2 = xattr::get_xattr_at_path(&file_path, "user.test2")
        .await
        .unwrap();
    assert_eq!(value2, b"test_value2");
}

/// Test metadata operations
#[compio::test]
async fn test_metadata_basic() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.txt");

    // Create test file
    fs::write(&file_path, "Test content").unwrap();

    // Test file descriptor-based metadata operations
    let file = File::open(&file_path).await.unwrap();
    metadata::fchmod(&file, 0o644).await.unwrap();
}

/// Test device file operations
#[compio::test]
async fn test_device_basic() {
    // Device creation requires root - skip entire test if not root
    let is_root = unsafe { libc::geteuid() } == 0;

    if !is_root {
        eprintln!("test_device_basic: SKIPPED (requires root privileges)");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    let base_path = temp_dir.path();

    // Test named pipe creation
    let pipe_path = base_path.join("test_pipe");
    device::create_named_pipe_at_path(&pipe_path, 0o644)
        .await
        .unwrap();
    assert!(pipe_path.exists());

    // Test socket creation
    let socket_path = base_path.join("test_socket");
    device::create_socket_at_path(&socket_path, 0o644)
        .await
        .unwrap();
    assert!(socket_path.exists());

    // Test character device creation (use valid device numbers)
    let char_dev_path = base_path.join("test_char_dev");
    device::create_char_device_at_path(&char_dev_path, 0o644, 1, 1)
        .await
        .unwrap();
    assert!(char_dev_path.exists());

    // Test block device creation (use valid device numbers)
    let block_dev_path = base_path.join("test_block_dev");
    device::create_block_device_at_path(&block_dev_path, 0o644, 8, 0)
        .await
        .unwrap();
    assert!(block_dev_path.exists());
}

/// Test parallel operations
#[compio::test]
async fn test_parallel_operations() {
    let temp_dir = TempDir::new().unwrap();
    let base_path = temp_dir.path();

    // Create multiple files
    let file_paths: Vec<_> = (0..5)
        .map(|i| {
            let path = base_path.join(format!("file_{}.txt", i));
            fs::write(&path, format!("Content {}", i)).unwrap();
            path
        })
        .collect();

    // Open all files
    let open_ops: Vec<_> = file_paths.iter().map(File::open).collect();

    let files: Vec<File> = futures::future::join_all(open_ops)
        .await
        .into_iter()
        .collect::<std::result::Result<Vec<_>, _>>()
        .unwrap();

    // Perform parallel fadvise operations
    let fadvise_ops: Vec<_> = files
        .iter()
        .map(|file| fadvise::fadvise(file, fadvise::FadviseAdvice::Sequential, 0, 0))
        .collect();

    futures::future::join_all(fadvise_ops).await;

    // Perform parallel xattr operations
    let xattr_ops: Vec<_> = file_paths
        .iter()
        .enumerate()
        .map(|(i, path)| {
            let attr_name = format!("user.parallel_test_{}", i);
            let attr_value = format!("value_{}", i);
            let path = path.clone();
            async move { xattr::set_xattr_at_path(&path, &attr_name, attr_value.as_bytes()).await }
        })
        .collect();

    futures::future::join_all(xattr_ops).await;

    // Verify all operations succeeded
    for (i, path) in file_paths.iter().enumerate() {
        let attr_name = format!("user.parallel_test_{}", i);
        let expected_value = format!("value_{}", i);
        let actual_value = xattr::get_xattr_at_path(path, &attr_name).await.unwrap();
        assert_eq!(actual_value, expected_value.as_bytes());
    }
}

/// Test error handling
#[compio::test]
async fn test_error_handling() {
    let temp_dir = TempDir::new().unwrap();
    let non_existent = temp_dir.path().join("does_not_exist.txt");

    // Test operations on non-existent files
    let result = async {
        let dir_fd = crate::directory::DirectoryFd::open(temp_dir.path())
            .await
            .unwrap();
        dir_fd.readlinkat("nonexistent").await
    }
    .await;
    assert!(result.is_err());

    let result = xattr::get_xattr_at_path(&non_existent, "user.test").await;
    assert!(result.is_err());

    let result = compio::fs::remove_dir(&non_existent).await;
    assert!(result.is_err());
}

/// Test performance characteristics
#[compio::test]
async fn test_performance() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("large_file.txt");

    // Create a large file (1MB)
    let large_data = vec![b'X'; 1024 * 1024];
    fs::write(&file_path, &large_data).unwrap();

    let file = File::open(&file_path).await.unwrap();

    // Measure fadvise performance
    let start = std::time::Instant::now();
    fadvise::fadvise(&file, fadvise::FadviseAdvice::Sequential, 0, 0)
        .await
        .unwrap();
    let duration = start.elapsed();

    // Should be very fast
    assert!(duration.as_millis() < 100);

    // Measure xattr operations performance
    let start = std::time::Instant::now();
    for i in 0..10 {
        let attr_name = format!("user.perf_test_{}", i);
        let attr_value = format!("value_{}", i);
        xattr::set_xattr_at_path(&file_path, &attr_name, attr_value.as_bytes())
            .await
            .unwrap();
    }
    let duration = start.elapsed();

    // Should be reasonably fast
    assert!(duration.as_millis() < 1000);
}
