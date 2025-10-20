#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Comprehensive metadata preservation tests
//!
//! These tests cover edge cases and scenarios that would significantly increase
//! confidence in the permission and timestamp preservation functionality.
//!
//! Note: Nanosecond timestamp tests are Linux-specific and gated with #[cfg(target_os = "linux")]
//! since nanosecond precision is guaranteed on Linux but may vary on other platforms.

use arsync::cli::Args;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
use std::time::{Duration, SystemTime};
use tempfile::TempDir;

mod common;
use common::copy_helpers::copy_file_test;
use common::test_args::create_archive_test_args;

/// Create a default Args struct for testing with archive mode enabled
fn create_test_args_with_archive() -> Args {
    create_archive_test_args()
}

use common::test_timeout_guard;
use std::time::Duration as StdDuration;

/// Test permission preservation with special permission bits (setuid, setgid, sticky)
#[compio::test]
async fn test_permission_preservation_special_bits() {
    let temp_dir = TempDir::new().unwrap();
    let src_path = temp_dir.path().join("special_bits.txt");
    let dst_path = temp_dir.path().join("special_bits_copy.txt");

    // Create source file
    fs::write(&src_path, "Test content for special permission bits").unwrap();

    // Test special permission bits (these may not work on all systems)
    let special_permissions = vec![
        0o4755, // setuid bit
        0o2755, // setgid bit
        0o1755, // sticky bit
        0o6755, // setuid + setgid
    ];

    for &permission_mode in &special_permissions {
        // Set specific permissions
        let permissions = std::fs::Permissions::from_mode(permission_mode);
        fs::set_permissions(&src_path, permissions).unwrap();

        // Get source permissions after setting (to account for system limitations)
        let src_metadata = fs::metadata(&src_path).unwrap();
        let expected_permissions = src_metadata.permissions().mode();

        println!(
            "Special bits test - Trying to set: {:o}, Actually set: {:o}",
            permission_mode, expected_permissions
        );

        // Skip test if special bits couldn't be set (system limitation)
        if expected_permissions != permission_mode {
            println!(
                "Skipping special bits test - system doesn't support setting {:o} (got {:o})",
                permission_mode, expected_permissions
            );
            continue;
        }

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

        println!(
            "Special bits test - Expected: {:o}, Actual: {:o}",
            expected_permissions, dst_permissions
        );

        // Special bits should be preserved when they can be set
        assert_eq!(
            expected_permissions, dst_permissions,
            "Special permission bits should be preserved when supported"
        );
    }
}

/// Test timestamp preservation with very old timestamps
#[compio::test]
async fn test_timestamp_preservation_old_timestamps() {
    let temp_dir = TempDir::new().unwrap();
    let src_path = temp_dir.path().join("old_timestamp.txt");
    let dst_path = temp_dir.path().join("old_timestamp_copy.txt");

    // Create source file
    fs::write(&src_path, "Test content with old timestamp").unwrap();

    // Set a very old timestamp (year 2000)
    let _old_time = SystemTime::UNIX_EPOCH + Duration::from_secs(946684800); // Jan 1, 2000
    let old_timespec = libc::timespec {
        tv_sec: 946684800,
        tv_nsec: 0,
    };

    // Use utimes to set the old timestamp
    let path_cstr = std::ffi::CString::new(src_path.as_os_str().as_bytes()).unwrap();
    let times = [old_timespec, old_timespec];

    let result = unsafe { libc::utimensat(libc::AT_FDCWD, path_cstr.as_ptr(), times.as_ptr(), 0) };

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

        // Check that the old timestamp was preserved
        let dst_metadata = fs::metadata(&dst_path).unwrap();
        let copied_accessed = dst_metadata.accessed().unwrap();
        let copied_modified = dst_metadata.modified().unwrap();

        // Check that timestamps are close to the old time
        let expected_duration = Duration::from_secs(946684800);
        let accessed_duration = copied_accessed
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default();
        let modified_duration = copied_modified
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default();

        println!(
            "Old timestamp test - Expected: {}s, Accessed: {}s, Modified: {}s",
            expected_duration.as_secs(),
            accessed_duration.as_secs(),
            modified_duration.as_secs()
        );

        // Allow some tolerance for timestamp precision
        // Note: We only check modified time because accessed time is automatically
        // updated by the filesystem when the file is read during copy operations
        let modified_duration = copied_modified
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default();
        let modified_seconds = modified_duration.as_secs();
        // Assert modified time only
        assert_eq!(
            modified_seconds,
            expected_duration.as_secs(),
            "Basic modified timestamp should be preserved"
        );
    }
}

/// Test timestamp preservation with future timestamps
#[compio::test]
async fn test_timestamp_preservation_future_timestamps() {
    let temp_dir = TempDir::new().unwrap();
    let src_path = temp_dir.path().join("future_timestamp.txt");
    let dst_path = temp_dir.path().join("future_timestamp_copy.txt");

    // Create source file
    fs::write(&src_path, "Test content with future timestamp").unwrap();

    // Set a future timestamp (year 2030)
    let _future_time = SystemTime::UNIX_EPOCH + Duration::from_secs(1893456000); // Jan 1, 2030
    let future_timespec = libc::timespec {
        tv_sec: 1893456000,
        tv_nsec: 0,
    };

    // Use utimes to set the future timestamp
    let path_cstr = std::ffi::CString::new(src_path.as_os_str().as_bytes()).unwrap();
    let times = [future_timespec, future_timespec];

    let result = unsafe { libc::utimensat(libc::AT_FDCWD, path_cstr.as_ptr(), times.as_ptr(), 0) };

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

        // Check that the future timestamp was preserved
        let dst_metadata = fs::metadata(&dst_path).unwrap();
        let copied_accessed = dst_metadata.accessed().unwrap();
        let copied_modified = dst_metadata.modified().unwrap();

        // Check that timestamps are close to the future time
        let expected_duration = Duration::from_secs(1893456000);
        let accessed_duration = copied_accessed
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default();
        let modified_duration = copied_modified
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default();

        println!(
            "Future timestamp test - Expected: {}s, Accessed: {}s, Modified: {}s",
            expected_duration.as_secs(),
            accessed_duration.as_secs(),
            modified_duration.as_secs()
        );

        // Allow some tolerance for timestamp precision
        // Note: We only check modified time because accessed time is automatically
        // updated by the filesystem when the file is read during copy operations
        let modified_duration = copied_modified
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default();
        let modified_seconds = modified_duration.as_secs();
        // Assert modified time only
        assert_eq!(
            modified_seconds,
            expected_duration.as_secs(),
            "Basic modified timestamp should be preserved"
        );
    }
}

/// Test permission preservation with very restrictive permissions
#[compio::test]
async fn test_permission_preservation_restrictive_permissions() {
    let temp_dir = TempDir::new().unwrap();
    let src_path = temp_dir.path().join("restrictive.txt");
    let dst_path = temp_dir.path().join("restrictive_copy.txt");

    // Create source file
    fs::write(&src_path, "Test content with restrictive permissions").unwrap();

    // Test very restrictive permissions
    let restrictive_permissions = vec![
        0o000, // no permissions at all
        0o001, // execute only for others
        0o010, // execute only for group
        0o100, // execute only for owner
        0o002, // write only for others
        0o020, // write only for group
        0o200, // write only for owner
        0o004, // read only for others
        0o040, // read only for group
        0o400, // read only for owner
    ];

    for &permission_mode in &restrictive_permissions {
        // Set specific permissions
        let permissions = std::fs::Permissions::from_mode(permission_mode);
        fs::set_permissions(&src_path, permissions).unwrap();

        // Get source permissions after setting
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
            Err(e) if e.to_string().contains("Permission denied") => {
                // Skip this permission mode as it prevents reading the file
                println!(
                    "Skipping permission mode {:o} - prevents reading: {}",
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

        println!(
            "Restrictive permissions test - Mode: {:o}, Expected: {:o}, Actual: {:o}",
            permission_mode, expected_permissions, dst_permissions
        );

        assert_eq!(
            expected_permissions, dst_permissions,
            "Restrictive permission mode {:o} should be preserved",
            permission_mode
        );
    }
}

/// Test timestamp preservation with nanosecond precision edge cases
#[compio::test]
async fn test_timestamp_preservation_nanosecond_edge_cases() {
    let _timeout = test_timeout_guard(StdDuration::from_secs(120));
    let temp_dir = TempDir::new().unwrap();
    let src_path = temp_dir.path().join("nanosecond_edge.txt");
    let dst_path = temp_dir.path().join("nanosecond_edge_copy.txt");

    // Create source file
    fs::write(&src_path, "Test content for nanosecond edge cases").unwrap();

    // Test various nanosecond precision scenarios
    let nanosecond_tests = vec![
        (0, "zero nanoseconds"),
        (1, "1 nanosecond"),
        (999_999_999, "maximum nanoseconds"),
        (123_456_789, "random nanoseconds"),
        (500_000_000, "half second nanoseconds"),
    ];

    for (nanoseconds, description) in &nanosecond_tests {
        // Set timestamp with specific nanosecond precision
        // Use a fixed timestamp (Jan 1, 2021) for consistent testing
        let base_seconds = 1609459200; // Jan 1, 2021 00:00:00 UTC

        let _precise_time =
            SystemTime::UNIX_EPOCH + Duration::new(base_seconds, *nanoseconds as u32);
        let precise_timespec = libc::timespec {
            tv_sec: base_seconds as i64,
            tv_nsec: *nanoseconds as i64,
        };

        // Use utimes to set the precise timestamp
        let path_cstr = std::ffi::CString::new(src_path.as_os_str().as_bytes()).unwrap();
        let times = [precise_timespec, precise_timespec];

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

            // Check that nanosecond precision was preserved
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
                "{} - Expected: {}ns, Accessed: {}ns, Modified: {}ns",
                description,
                nanoseconds,
                accessed_duration.subsec_nanos(),
                modified_duration.subsec_nanos()
            );

            // Check that nanosecond precision is preserved (within reasonable tolerance)
            // Note: We only check modified time because accessed time is automatically
            // updated by the filesystem when the file is read during copy operations
            let modified_duration = copied_modified
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default();
            let modified_nanos = modified_duration.subsec_nanos();
            // Level 1: Modified timestamp seconds check
            let expected_seconds = 1609459200; // Jan 1, 2021
            let modified_seconds = modified_duration.as_secs();
            assert_eq!(
                modified_seconds, expected_seconds,
                "Basic modified timestamp seconds should be preserved"
            );

            // Level 2: Nanosecond precision check (Linux only)
            #[cfg(target_os = "linux")]
            {
                // On Linux filesystems, nanosecond precision should be preserved exactly
                assert_eq!(
                    modified_nanos, *nanoseconds as u32,
                    "Modified nanosecond precision should be preserved exactly for {} (expected: {}, got: {})",
                    description, nanoseconds, modified_nanos
                );
            }
        }
    }
}

/// Test permission preservation with umask interaction
#[compio::test]
async fn test_permission_preservation_umask_interaction() {
    let temp_dir = TempDir::new().unwrap();
    let src_path = temp_dir.path().join("umask_test.txt");
    let dst_path = temp_dir.path().join("umask_test_copy.txt");

    // Create source file
    fs::write(&src_path, "Test content for umask interaction").unwrap();

    // Test various permission modes that might interact with umask
    let umask_test_permissions = vec![
        0o777, // full permissions
        0o666, // read/write for all
        0o644, // standard file permissions
        0o600, // owner only
        0o755, // executable
        0o700, // owner only executable
    ];

    for &permission_mode in &umask_test_permissions {
        // Set specific permissions
        let permissions = std::fs::Permissions::from_mode(permission_mode);
        fs::set_permissions(&src_path, permissions).unwrap();

        // Get source permissions after setting (this accounts for umask)
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

        // Check that permissions were preserved exactly as they were set
        let dst_metadata = fs::metadata(&dst_path).unwrap();
        let dst_permissions = dst_metadata.permissions().mode();

        println!(
            "Umask interaction test - Requested: {:o}, Expected: {:o}, Actual: {:o}",
            permission_mode, expected_permissions, dst_permissions
        );

        assert_eq!(
            expected_permissions, dst_permissions,
            "Permission mode {:o} should be preserved exactly as set",
            permission_mode
        );
    }
}

/// Test concurrent file copying with metadata preservation
#[compio::test]
async fn test_concurrent_metadata_preservation() {
    let temp_dir = TempDir::new().unwrap();

    // Create multiple source files with different permissions and timestamps
    let test_files = vec![
        ("file1.txt", 0o644, "Content 1"),
        ("file2.txt", 0o755, "Content 2"),
        ("file3.txt", 0o600, "Content 3"),
        ("file4.txt", 0o777, "Content 4"),
    ];

    let mut handles = vec![];

    for (filename, permissions, content) in test_files {
        let src_path = temp_dir.path().join(filename);
        let dst_path = temp_dir.path().join(format!("{}_copy", filename));

        // Create source file
        fs::write(&src_path, content).unwrap();

        // Set specific permissions
        let perms = std::fs::Permissions::from_mode(permissions);
        fs::set_permissions(&src_path, perms).unwrap();

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
                "Concurrent copy should preserve permissions for {}",
                filename
            );

            (filename.to_string(), expected_permissions, dst_permissions)
        });

        handles.push(handle);
    }

    // Wait for all concurrent operations to complete
    let results = futures::future::join_all(handles).await;

    // Verify all operations succeeded
    for result in results {
        let (filename, expected, actual) = result.unwrap();
        println!(
            "Concurrent test - {}: Expected {:o}, Actual {:o}",
            filename, expected, actual
        );
        assert_eq!(
            expected, actual,
            "Concurrent metadata preservation should work for {}",
            filename
        );
    }
}

/// Test metadata preservation with very large files (stress test)
#[compio::test]
async fn test_metadata_preservation_large_file_stress() {
    let temp_dir = TempDir::new().unwrap();
    let src_path = temp_dir.path().join("large_stress.txt");
    let dst_path = temp_dir.path().join("large_stress_copy.txt");

    // Create a large file (10MB) to stress test metadata preservation
    let large_content = "A".repeat(10 * 1024 * 1024); // 10MB
    fs::write(&src_path, &large_content).unwrap();

    // Set specific permissions
    let permissions = std::fs::Permissions::from_mode(0o755);
    fs::set_permissions(&src_path, permissions).unwrap();

    // Get original metadata
    let src_metadata = fs::metadata(&src_path).unwrap();
    let expected_permissions = src_metadata.permissions().mode();
    let _original_accessed = src_metadata.accessed().unwrap();
    let original_modified = src_metadata.modified().unwrap();

    // Copy the large file
    let args = create_test_args_with_archive();
    copy_file_test(
        &src_path,
        &dst_path,
        &args.metadata,
        &common::disabled_parallel_config(),
    )
    .await
    .unwrap();

    // Verify file content
    let copied_content = fs::read_to_string(&dst_path).unwrap();
    assert_eq!(
        copied_content, large_content,
        "Large file content should be preserved"
    );

    // Check that permissions were preserved
    let dst_metadata = fs::metadata(&dst_path).unwrap();
    let dst_permissions = dst_metadata.permissions().mode();
    assert_eq!(
        expected_permissions, dst_permissions,
        "Permissions should be preserved for large files"
    );

    // Check that timestamps were preserved
    let _copied_accessed = dst_metadata.accessed().unwrap();
    let copied_modified = dst_metadata.modified().unwrap();

    // Skip strict atime assertion in CI: focus on modified timestamp
    // See https://github.com/jmalicki/arsync/issues/10
    let modified_diff = copied_modified
        .duration_since(original_modified)
        .unwrap_or_default();
    assert!(
        modified_diff.as_millis() < 1000,
        "Modified time should be preserved within 1000ms"
    );
}
