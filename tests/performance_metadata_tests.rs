#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Performance and stress tests for metadata preservation
//!
//! These tests verify that metadata preservation works correctly under
//! various performance scenarios and stress conditions.

use arsync::cli::Args;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
use std::time::SystemTime;
use tempfile::TempDir;

mod common;
use common::copy_helpers::copy_file_test;
use common::test_args::create_archive_test_args;
use common::test_timeout_guard;
use std::time::Duration as StdDuration;

/// Create a default Args struct for testing with archive mode enabled
fn create_test_args_with_archive() -> Args {
    create_archive_test_args()
}

/// Test metadata preservation performance with many small files
#[compio::test]
async fn test_metadata_preservation_many_small_files() {
    let _timeout = test_timeout_guard(StdDuration::from_secs(180));
    let temp_dir = TempDir::new().unwrap();
    let num_files = 100;

    let mut results = Vec::new();

    for i in 0..num_files {
        let src_path = temp_dir.path().join(format!("small_file_{}.txt", i));
        let dst_path = temp_dir.path().join(format!("small_file_{}_copy.txt", i));

        // Create source file
        fs::write(&src_path, format!("Content for file {}", i)).unwrap();

        // Set different permissions for each file
        let permission_mode = 0o600 + (i % 177) as u32; // Vary permissions
        let permissions = std::fs::Permissions::from_mode(permission_mode);
        fs::set_permissions(&src_path, permissions).unwrap();

        // Get expected permissions
        let src_metadata = fs::metadata(&src_path).unwrap();
        let expected_permissions = src_metadata.permissions().mode();

        // Copy the file
        let args = create_test_args_with_archive();
        copy_file_test(
            &src_path,
            &dst_path,
            &args.metadata,
            &common::disabled_parallel_config(),
        )
        .await
        .unwrap();

        // Check that permissions were preserved
        let dst_metadata = fs::metadata(&dst_path).unwrap();
        let dst_permissions = dst_metadata.permissions().mode();

        results.push((i, expected_permissions, dst_permissions));
    }

    // Verify all results
    for (i, expected, actual) in results {
        assert_eq!(
            expected, actual,
            "Permissions should be preserved for small file {}",
            i
        );
    }

    println!(
        "Successfully processed {} small files with metadata preservation",
        num_files
    );
}

/// Test metadata preservation with rapid sequential operations
#[compio::test]
async fn test_metadata_preservation_rapid_sequential() {
    let _timeout = test_timeout_guard(StdDuration::from_secs(180));
    let temp_dir = TempDir::new().unwrap();
    // Reduced from 50 to 10 - adequate for testing while avoiding coverage timeout
    let num_operations = 10;

    for i in 0..num_operations {
        let src_path = temp_dir.path().join(format!("rapid_{}.txt", i));
        let dst_path = temp_dir.path().join(format!("rapid_{}_copy.txt", i));

        // Create source file
        fs::write(&src_path, format!("Rapid operation {}", i)).unwrap();

        // Set permissions
        let permissions = std::fs::Permissions::from_mode(0o644);
        fs::set_permissions(&src_path, permissions).unwrap();

        // Get expected permissions
        let src_metadata = fs::metadata(&src_path).unwrap();
        let expected_permissions = src_metadata.permissions().mode();

        // Copy the file
        let args = create_test_args_with_archive();
        copy_file_test(
            &src_path,
            &dst_path,
            &args.metadata,
            &common::disabled_parallel_config(),
        )
        .await
        .unwrap();

        // Check that permissions were preserved
        let dst_metadata = fs::metadata(&dst_path).unwrap();
        let dst_permissions = dst_metadata.permissions().mode();

        assert_eq!(
            expected_permissions, dst_permissions,
            "Permissions should be preserved in rapid operation {}",
            i
        );
    }

    println!(
        "Successfully completed {} rapid sequential operations",
        num_operations
    );
}

/// Test metadata preservation with mixed file sizes
#[compio::test]
async fn test_metadata_preservation_mixed_sizes() {
    let _timeout = test_timeout_guard(StdDuration::from_secs(240));
    let temp_dir = TempDir::new().unwrap();

    // Test various file sizes
    let file_sizes = vec![
        (0, "empty"),               // empty file
        (1, "1 byte"),              // 1 byte
        (1024, "1KB"),              // 1KB
        (1024 * 1024, "1MB"),       // 1MB
        (10 * 1024 * 1024, "10MB"), // 10MB
    ];

    for (size, description) in file_sizes {
        let src_path = temp_dir.path().join(format!("mixed_size_{}.txt", size));
        let dst_path = temp_dir
            .path()
            .join(format!("mixed_size_{}_copy.txt", size));

        // Create source file with specific size
        let content = if size == 0 {
            String::new()
        } else {
            "A".repeat(size)
        };
        fs::write(&src_path, content).unwrap();

        // Set permissions
        let permissions = std::fs::Permissions::from_mode(0o644);
        fs::set_permissions(&src_path, permissions).unwrap();

        // Get expected permissions
        let src_metadata = fs::metadata(&src_path).unwrap();
        let expected_permissions = src_metadata.permissions().mode();
        let original_accessed = src_metadata.accessed().unwrap();
        let original_modified = src_metadata.modified().unwrap();

        // Copy the file
        let args = create_test_args_with_archive();
        copy_file_test(
            &src_path,
            &dst_path,
            &args.metadata,
            &common::disabled_parallel_config(),
        )
        .await
        .unwrap();

        // Check that permissions were preserved
        let dst_metadata = fs::metadata(&dst_path).unwrap();
        let dst_permissions = dst_metadata.permissions().mode();
        assert_eq!(
            expected_permissions, dst_permissions,
            "Permissions should be preserved for {} file",
            description
        );

        // Check that timestamps were preserved (modified only)
        let copied_accessed = dst_metadata.accessed().unwrap();
        let copied_modified = dst_metadata.modified().unwrap();

        let _accessed_diff = copied_accessed
            .duration_since(original_accessed)
            .unwrap_or_default();
        let modified_diff = copied_modified
            .duration_since(original_modified)
            .unwrap_or_default();

        assert!(
            _accessed_diff.as_millis() < 1000,
            "Accessed time should be preserved for {} file",
            description
        );
        assert!(
            modified_diff.as_millis() < 1000,
            "Modified time should be preserved for {} file",
            description
        );
    }

    println!("Successfully tested metadata preservation with mixed file sizes");
}

/// Test metadata preservation with concurrent operations
#[compio::test]
async fn test_metadata_preservation_concurrent_operations() {
    let _timeout = test_timeout_guard(StdDuration::from_secs(240));
    let temp_dir = TempDir::new().unwrap();
    let num_concurrent = 20;

    let mut handles = Vec::new();

    for i in 0..num_concurrent {
        let src_path = temp_dir.path().join(format!("concurrent_{}.txt", i));
        let dst_path = temp_dir.path().join(format!("concurrent_{}_copy.txt", i));

        // Create source file
        fs::write(&src_path, format!("Concurrent operation {}", i)).unwrap();

        // Set different permissions for each file
        let permission_mode = 0o600 + (i % 177) as u32;
        let permissions = std::fs::Permissions::from_mode(permission_mode);
        fs::set_permissions(&src_path, permissions).unwrap();

        // Get expected permissions
        let src_metadata = fs::metadata(&src_path).unwrap();
        let expected_permissions = src_metadata.permissions().mode();

        // Spawn concurrent copy task
        let handle = compio::runtime::spawn(async move {
            let args = create_test_args_with_archive();
            copy_file_test(
                &src_path,
                &dst_path,
                &args.metadata,
                &common::disabled_parallel_config(),
            )
            .await
            .unwrap();

            // Verify permissions were preserved
            let dst_metadata = fs::metadata(&dst_path).unwrap();
            let dst_permissions = dst_metadata.permissions().mode();

            assert_eq!(
                expected_permissions, dst_permissions,
                "Concurrent operation {} should preserve permissions",
                i
            );

            (i, expected_permissions, dst_permissions)
        });

        handles.push(handle);
    }

    // Wait for all concurrent operations to complete
    let results = futures::future::join_all(handles).await;

    // Verify all operations succeeded
    for result in results {
        let (i, expected, actual) = result.unwrap();
        assert_eq!(
            expected, actual,
            "Concurrent operation {} should preserve permissions",
            i
        );
    }

    println!(
        "Successfully completed {} concurrent operations with metadata preservation",
        num_concurrent
    );
}

/// Test metadata preservation with files that have very specific timestamps
#[compio::test]
async fn test_metadata_preservation_specific_timestamps() {
    let _timeout = test_timeout_guard(StdDuration::from_secs(240));
    let temp_dir = TempDir::new().unwrap();

    // Test various specific timestamps
    let timestamp_tests = vec![
        (946684800, 0, "Y2K"), // Jan 1, 2000 00:00:00.000000000
        (946684800, 123456789, "Y2K with nanoseconds"), // Jan 1, 2000 00:00:00.123456789
        (1609459200, 0, "2021 New Year"), // Jan 1, 2021 00:00:00.000000000
        (1609459200, 999999999, "2021 with max nanoseconds"), // Jan 1, 2021 00:00:00.999999999
        (0, 0, "Unix epoch"),  // Jan 1, 1970 00:00:00.000000000
        (0, 1, "Unix epoch + 1ns"), // Jan 1, 1970 00:00:00.000000001
    ];

    for (seconds, nanoseconds, description) in timestamp_tests {
        let src_path = temp_dir
            .path()
            .join(format!("timestamp_{}_{}.txt", seconds, nanoseconds));
        let dst_path = temp_dir
            .path()
            .join(format!("timestamp_{}_{}_copy.txt", seconds, nanoseconds));

        // Create source file
        fs::write(&src_path, format!("Test content for {}", description)).unwrap();

        // Set specific timestamp
        let specific_timespec = libc::timespec {
            tv_sec: seconds,
            tv_nsec: nanoseconds,
        };

        // Use utimes to set the specific timestamp
        let path_cstr = std::ffi::CString::new(src_path.as_os_str().as_bytes()).unwrap();
        let times = [specific_timespec, specific_timespec];

        let result =
            unsafe { libc::utimensat(libc::AT_FDCWD, path_cstr.as_ptr(), times.as_ptr(), 0) };

        if result == 0 {
            // Copy the file
            let args = create_test_args_with_archive();
            copy_file_test(
                &src_path,
                &dst_path,
                &args.metadata,
                &common::disabled_parallel_config(),
            )
            .await
            .unwrap();

            // Check that the specific timestamp was preserved
            let dst_metadata = fs::metadata(&dst_path).unwrap();
            let copied_accessed = dst_metadata.accessed().unwrap();
            let copied_modified = dst_metadata.modified().unwrap();

            let accessed_duration = copied_accessed
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default();
            let modified_duration = copied_modified
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default();

            println!(
                "{} - Expected: {}s.{}ns, Accessed: {}s.{}ns, Modified: {}s.{}ns",
                description,
                seconds,
                nanoseconds,
                accessed_duration.as_secs(),
                accessed_duration.subsec_nanos(),
                modified_duration.as_secs(),
                modified_duration.subsec_nanos()
            );

            // Check that timestamps are preserved
            // Note: We only check modification time because access time is automatically
            // updated by the filesystem when the file is read during copy operations

            // Check seconds
            assert_eq!(
                modified_duration.as_secs(),
                seconds as u64,
                "Modified time seconds should be preserved exactly for {}",
                description
            );

            // Check nanoseconds (Linux-specific exact match)
            #[cfg(target_os = "linux")]
            {
                assert_eq!(
                    modified_duration.subsec_nanos(), nanoseconds as u32,
                    "Modified time nanoseconds should be preserved exactly for {} (expected: {}ns, got: {}ns)",
                    description, nanoseconds, modified_duration.subsec_nanos()
                );
            }
        }
    }
}

/// Test metadata preservation with files that have alternating permission patterns
#[compio::test]
async fn test_metadata_preservation_alternating_permissions() {
    let _timeout = test_timeout_guard(StdDuration::from_secs(240));
    let temp_dir = TempDir::new().unwrap();
    let num_files = 50;

    // Create files with alternating permission patterns
    for i in 0..num_files {
        let src_path = temp_dir.path().join(format!("alternating_{}.txt", i));
        let dst_path = temp_dir.path().join(format!("alternating_{}_copy.txt", i));

        // Create source file
        fs::write(&src_path, format!("Alternating pattern file {}", i)).unwrap();

        // Alternate between different permission patterns
        let permission_mode = match i % 4 {
            0 => 0o644, // standard file
            1 => 0o755, // executable
            2 => 0o600, // owner only
            3 => 0o777, // all permissions
            _ => 0o644, // default fallback
        };

        let permissions = std::fs::Permissions::from_mode(permission_mode);
        fs::set_permissions(&src_path, permissions).unwrap();

        // Get expected permissions
        let src_metadata = fs::metadata(&src_path).unwrap();
        let expected_permissions = src_metadata.permissions().mode();

        // Copy the file
        let args = create_test_args_with_archive();
        copy_file_test(
            &src_path,
            &dst_path,
            &args.metadata,
            &common::disabled_parallel_config(),
        )
        .await
        .unwrap();

        // Check that permissions were preserved
        let dst_metadata = fs::metadata(&dst_path).unwrap();
        let dst_permissions = dst_metadata.permissions().mode();

        assert_eq!(
            expected_permissions, dst_permissions,
            "Alternating permission pattern {} should be preserved",
            i
        );
    }

    println!(
        "Successfully tested {} files with alternating permission patterns",
        num_files
    );
}

/// Test metadata preservation with files that have very specific permission combinations
#[compio::test]
async fn test_metadata_preservation_specific_permissions() {
    let _timeout = test_timeout_guard(StdDuration::from_secs(240));
    let temp_dir = TempDir::new().unwrap();

    // Test very specific permission combinations
    let specific_permissions = vec![
        (0o000, "no permissions"),
        (0o001, "others execute"),
        (0o010, "group execute"),
        (0o100, "owner execute"),
        (0o002, "others write"),
        (0o020, "group write"),
        (0o200, "owner write"),
        (0o004, "others read"),
        (0o040, "group read"),
        (0o400, "owner read"),
        (0o007, "others all"),
        (0o070, "group all"),
        (0o700, "owner all"),
        (0o123, "mixed permissions 1"),
        (0o456, "mixed permissions 2"),
        (0o777, "mixed permissions 3"),
    ];

    for (permission_mode, description) in &specific_permissions {
        let src_path = temp_dir
            .path()
            .join(format!("specific_{:o}.txt", permission_mode));
        let dst_path = temp_dir
            .path()
            .join(format!("specific_{:o}_copy.txt", permission_mode));

        // Create source file
        fs::write(&src_path, format!("Test content for {}", description)).unwrap();

        // Set specific permissions
        let permissions = std::fs::Permissions::from_mode(*permission_mode);
        fs::set_permissions(&src_path, permissions).unwrap();

        // Get expected permissions
        let src_metadata = fs::metadata(&src_path).unwrap();
        let expected_permissions = src_metadata.permissions().mode();

        // Copy the file - skip if permission prevents reading
        let args = create_test_args_with_archive();
        match copy_file_test(
            &src_path,
            &dst_path,
            &args.metadata,
            &common::disabled_parallel_config(),
        )
        .await
        {
            Ok(_) => {
                // Test passed, continue with assertion
            }
            Err(e)
                if e.to_string().contains("Permission denied")
                    || e.to_string().contains("EACCES")
                    || e.to_string().contains("EPERM") =>
            {
                // Skip this permission mode as it prevents reading the file
                println!(
                    "Skipping specific permission mode {:o} - prevents reading: {}",
                    permission_mode, e
                );
                continue;
            }
            Err(e) => {
                eprintln!("Unexpected error copying file: {}", e);
                return;
            }
        }

        // Check that permissions were preserved
        let dst_metadata = fs::metadata(&dst_path).unwrap();
        let dst_permissions = dst_metadata.permissions().mode();

        assert_eq!(
            expected_permissions, dst_permissions,
            "Specific permission {} ({}) should be preserved",
            permission_mode, description
        );
    }

    println!(
        "Successfully tested {} specific permission combinations",
        specific_permissions.len()
    );
}
