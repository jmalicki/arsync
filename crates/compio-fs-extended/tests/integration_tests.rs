//! Comprehensive integration tests for compio-fs-extended
//!
//! These tests exercise real-world scenarios using actual files and directories
//! to verify that all io_uring operations work correctly in practice.

#[cfg(target_os = "linux")]
use compio::fs::File;
use compio_fs_extended::*;
use std::fs;
// Removed unused imports
use tempfile::TempDir;

/// Test fadvise operations on real files with different access patterns
#[compio::test]
#[cfg(target_os = "linux")] // fadvise is Linux-only
async fn test_fadvise_real_world_scenarios() {
    let temp_dir = TempDir::new().unwrap();

    // Create a large test file (1MB)
    let file_path = temp_dir.path().join("large_file.txt");
    let large_data = vec![b'A'; 1024 * 1024]; // 1MB of data
    fs::write(&file_path, &large_data).unwrap();

    // Open file and test different fadvise patterns
    let file = File::open(&file_path).await.unwrap();

    // Test sequential access optimization
    fadvise::fadvise(&file, fadvise::FadviseAdvice::Sequential, 0, 0)
        .await
        .unwrap();

    // Test random access optimization
    fadvise::fadvise(&file, fadvise::FadviseAdvice::Random, 0, 0)
        .await
        .unwrap();

    // Test will need optimization
    fadvise::fadvise(&file, fadvise::FadviseAdvice::WillNeed, 0, 1024)
        .await
        .unwrap();

    // Test dont need optimization
    fadvise::fadvise(&file, fadvise::FadviseAdvice::DontNeed, 0, 1024)
        .await
        .unwrap();

    // Test normal access
    fadvise::fadvise(&file, fadvise::FadviseAdvice::Normal, 0, 0)
        .await
        .unwrap();
}

/// Test symlink operations in a real directory structure
#[compio::test]
async fn test_symlink_real_world_scenarios() {
    let temp_dir = TempDir::new().unwrap();

    // Create a source file
    let source_file = temp_dir.path().join("source.txt");
    fs::write(&source_file, "Hello, World!").unwrap();

    // Create symlinks using different methods
    let symlink1 = temp_dir.path().join("symlink1");
    let symlink2 = temp_dir.path().join("symlink2");

    // Test path-based symlink creation
    async {
        let dir_fd = crate::directory::DirectoryFd::open(temp_dir.path())
            .await
            .unwrap();
        dir_fd
            .symlinkat(
                &source_file.file_name().unwrap().to_string_lossy(),
                "symlink1",
            )
            .await
    }
    .await
    .unwrap();

    // Test file-based symlink creation using DirectoryFd
    let dir_fd2 = crate::directory::DirectoryFd::open(temp_dir.path())
        .await
        .unwrap();
    dir_fd2
        .symlinkat(
            &source_file.file_name().unwrap().to_string_lossy(),
            "symlink2",
        )
        .await
        .unwrap();

    // Verify symlinks work
    assert!(symlink1.exists());
    assert!(symlink2.exists());

    // Test reading symlinks
    let target1 = async {
        let dir_fd = crate::directory::DirectoryFd::open(temp_dir.path())
            .await
            .unwrap();
        dir_fd.readlinkat("symlink1").await
    }
    .await
    .unwrap();
    let target2 = async {
        let dir_fd = crate::directory::DirectoryFd::open(temp_dir.path())
            .await
            .unwrap();
        dir_fd.readlinkat("symlink2").await
    }
    .await
    .unwrap();

    assert_eq!(target1, std::path::PathBuf::from("source.txt"));
    assert_eq!(target2, std::path::PathBuf::from("source.txt"));

    // Verify symlink content
    let content1 = fs::read_to_string(&symlink1).unwrap();
    let content2 = fs::read_to_string(&symlink2).unwrap();
    assert_eq!(content1, "Hello, World!");
    assert_eq!(content2, "Hello, World!");
}

/// Test directory operations with complex nested structures
#[compio::test]
async fn test_directory_complex_scenarios() {
    let temp_dir = TempDir::new().unwrap();
    let base_path = temp_dir.path();

    // Create a complex directory structure
    let dirs = vec![
        "project/src",
        "project/tests",
        "project/docs",
        "project/target/debug",
        "project/target/release",
        "data/raw",
        "data/processed",
        "logs/2024",
        "logs/2023",
    ];

    // Create all directories (create parent directories first)
    for dir in &dirs {
        let dir_path = base_path.join(dir);
        if let Some(parent) = dir_path.parent() {
            compio::fs::create_dir_all(parent).await.unwrap();
        }
        compio::fs::create_dir(&dir_path).await.unwrap();
        assert!(dir_path.exists());
    }

    // Test DirectoryFd operations
    let project_dir = base_path.join("project");
    let dir_fd = directory::DirectoryFd::open(&project_dir).await.unwrap();

    // Create files in parallel using DirectoryFd
    let file_operations = vec![
        dir_fd.create_directory("new_feature", 0o755),
        dir_fd.create_directory("bug_fixes", 0o755),
        dir_fd.create_directory("documentation", 0o755),
    ];

    // Execute all operations in parallel
    let results = futures::future::join_all(file_operations).await;
    for result in results {
        result.unwrap();
    }

    // Verify directories were created
    assert!(project_dir.join("new_feature").exists());
    assert!(project_dir.join("bug_fixes").exists());
    assert!(project_dir.join("documentation").exists());

    // Test directory removal
    compio::fs::remove_dir(&project_dir.join("new_feature"))
        .await
        .unwrap();
    assert!(!project_dir.join("new_feature").exists());
}

// TODO: Re-enable when xattr module is properly implemented
// /// Test extended attributes on real files
// #[compio::test]
// async fn test_xattr_real_world_scenarios() {
//     let temp_dir = TempDir::new().unwrap();
//     let file_path = temp_dir.path().join("test_file.txt");
//
//     // Create a test file
//     fs::write(&file_path, "Test content").unwrap();
//
//     // Test file descriptor-based xattr operations
//     let file = File::open(&file_path).await.unwrap();
//     let extended_file = ExtendedFile::new(file);
//
//     // Set extended attributes
//     extended_file
//         .set_xattr("user.author", b"compio-fs-extended")
//         .await
//         .unwrap();
//     extended_file
//         .set_xattr("user.version", b"1.0.0")
//         .await
//         .unwrap();
//     extended_file
//         .set_xattr("user.tags", b"test,integration")
//         .await
//         .unwrap();
//
//     // Get extended attributes
//     let author = extended_file.get_xattr("user.author").await.unwrap();
//     let version = extended_file.get_xattr("user.version").await.unwrap();
//     let tags = extended_file.get_xattr("user.tags").await.unwrap();
//
//     assert_eq!(author, b"compio-fs-extended");
//     assert_eq!(version, b"1.0.0");
//     assert_eq!(tags, b"test,integration");
//
//     // List all extended attributes
//     let all_attrs = extended_file.list_xattr().await.unwrap();
//     assert!(all_attrs.contains(&"user.author".to_string()));
//     assert!(all_attrs.contains(&"user.version".to_string()));
//     assert!(all_attrs.contains(&"user.tags".to_string()));
//
//     // Test path-based xattr operations
//     xattr::set_xattr_at_path(&file_path, "user.path_test", b"path_value")
//         .await
//         .unwrap();
//     let path_value = xattr::get_xattr_at_path(&file_path, "user.path_test")
//         .await
//         .unwrap();
//     assert_eq!(path_value, b"path_value");
// }

// TODO: Re-enable when metadata module is properly implemented
// /// Test metadata operations on real files
// #[compio::test]
// async fn test_metadata_real_world_scenarios() {
//     let temp_dir = TempDir::new().unwrap();
//     let file_path = temp_dir.path().join("metadata_test.txt");

//     // Create a test file
//     fs::write(&file_path, "Metadata test content").unwrap();

//     // Test file descriptor-based metadata operations
//     let file = File::open(&file_path).await.unwrap();
//     let fd = file.as_raw_fd();

//     // Test permission changes
//     metadata::fchmod(fd, 0o600).await.unwrap();

//     // Test timestamp changes
//     let now = SystemTime::now();
//     let past = now - std::time::Duration::from_secs(3600);
//     metadata::futimes(fd, past, now).await.unwrap();

//     // Test path-based metadata operations
//     metadata::fchmodat(&file_path, 0o644).await.unwrap();
//     metadata::futimesat(&file_path, past, now).await.unwrap();
// }

/// Test device file operations
#[ignore]
#[compio::test]
async fn test_device_real_world_scenarios() {
    let temp_dir = TempDir::new().unwrap();
    let base_path = temp_dir.path();

    // Test named pipe creation
    let pipe_path = base_path.join("test_pipe");
    device::create_named_pipe_at_path(&pipe_path, 0o644)
        .await
        .unwrap();
    assert!(pipe_path.exists());

    // Test character device creation
    let char_dev_path = base_path.join("test_char_dev");
    device::create_char_device_at_path(&char_dev_path, 0o644, 1, 3)
        .await
        .unwrap();
    assert!(char_dev_path.exists());

    // Test block device creation
    let block_dev_path = base_path.join("test_block_dev");
    device::create_block_device_at_path(&block_dev_path, 0o644, 8, 1)
        .await
        .unwrap();
    assert!(block_dev_path.exists());

    // Test socket creation
    let socket_path = base_path.join("test_socket");
    device::create_socket_at_path(&socket_path, 0o644)
        .await
        .unwrap();
    assert!(socket_path.exists());
}

/// Test parallel operations to verify io_uring efficiency
#[compio::test]
#[cfg(target_os = "linux")] // Uses fadvise which is Linux-only
async fn test_parallel_io_uring_operations() {
    let temp_dir = TempDir::new().unwrap();
    let base_path = temp_dir.path();

    // Create multiple files for parallel operations
    let file_paths: Vec<_> = (0..10)
        .map(|i| base_path.join(format!("file_{}.txt", i)))
        .collect();

    // Create files in parallel
    let create_ops: Vec<_> = file_paths
        .iter()
        .map(|path| async move {
            fs::write(path, format!("Content for {}", path.display())).unwrap();
        })
        .collect();

    futures::future::join_all(create_ops).await;

    // Open all files and perform parallel fadvise operations
    let open_ops: Vec<_> = file_paths.iter().map(File::open).collect();

    let file_results: Vec<_> = futures::future::join_all(open_ops).await;
    let files: Vec<_> = file_results
        .into_iter()
        .collect::<std::result::Result<Vec<_>, _>>()
        .unwrap();

    // Perform parallel fadvise operations
    let fadvise_ops: Vec<_> = files
        .iter()
        .map(|file| fadvise::fadvise(file, fadvise::FadviseAdvice::Sequential, 0, 0))
        .collect();

    futures::future::join_all(fadvise_ops).await;

    // TODO: Re-enable when xattr module is properly implemented
    // // Perform parallel xattr operations using path-based operations
    // let xattr_ops: Vec<_> = file_paths
    //     .iter()
    //     .enumerate()
    //     .map(|(i, path)| {
    //         let attr_name = format!("user.parallel_test_{}", i);
    //         let attr_value = format!("value_{}", i);
    //         xattr::set_xattr_at_path(path, &attr_name, attr_value.as_bytes())
    //     })
    //     .collect();
    //
    // futures::future::join_all(xattr_ops).await;
    //
    // // Verify all operations succeeded
    // for (i, path) in file_paths.iter().enumerate() {
    //     let attr_name = format!("user.parallel_test_{}", i);
    //     let expected_value = format!("value_{}", i);
    //     let actual_value = xattr::get_xattr_at_path(path, &attr_name).await.unwrap();
    //     assert_eq!(actual_value, expected_value.as_bytes());
    // }
}

/// Test error handling in real scenarios
#[compio::test]
async fn test_error_handling_real_scenarios() {
    let temp_dir = TempDir::new().unwrap();

    // Test non-existent file operations
    let non_existent = temp_dir.path().join("does_not_exist.txt");

    // These should fail gracefully
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

    // Test invalid xattr names
    let file_path = temp_dir.path().join("test.txt");
    fs::write(&file_path, "test").unwrap();

    // TODO: Re-enable when xattr module is properly implemented
    // let file = File::open(&file_path).await.unwrap();
    // let extended_file = ExtendedFile::new(file);
    // let result = extended_file.set_xattr("", b"value").await;
    // assert!(result.is_err());

    // Test invalid directory operations
    let result = compio::fs::remove_dir(&file_path).await;
    assert!(result.is_err()); // Can't remove a file as directory
}

/// Test performance characteristics of io_uring operations
#[compio::test]
#[cfg(target_os = "linux")] // Uses fadvise which is Linux-only
async fn test_performance_characteristics() {
    let temp_dir = TempDir::new().unwrap();
    let base_path = temp_dir.path();

    // Create a large file for performance testing
    let large_file = base_path.join("large_file.txt");
    let large_data = vec![b'X'; 10 * 1024 * 1024]; // 10MB
    fs::write(&large_file, &large_data).unwrap();

    let file = File::open(&large_file).await.unwrap();

    // Measure fadvise performance
    let start = std::time::Instant::now();
    fadvise::fadvise(&file, fadvise::FadviseAdvice::Sequential, 0, 0)
        .await
        .unwrap();
    let fadvise_duration = start.elapsed();

    // TODO: Re-enable when xattr module is properly implemented
    // // Measure xattr operations performance
    // let start = std::time::Instant::now();
    // for i in 0..100 {
    //     let attr_name = format!("user.perf_test_{}", i);
    //     let attr_value = format!("value_{}", i);
    //     xattr::set_xattr_at_path(&large_file, &attr_name, attr_value.as_bytes())
    //         .await
    //         .unwrap();
    // }
    // let xattr_duration = start.elapsed();
    //
    // // Verify operations completed
    // assert!(fadvise_duration.as_millis() < 100); // Should be very fast
    // assert!(xattr_duration.as_millis() < 1000); // Should be reasonably fast
    //
    // // Clean up xattr
    // for i in 0..100 {
    //     let attr_name = format!("user.perf_test_{}", i);
    //     xattr::remove_xattr_at_path(&large_file, &attr_name)
    //         .await
    //         .unwrap();
    // }

    // Verify fadvise operations completed
    assert!(fadvise_duration.as_millis() < 100); // Should be very fast
}
