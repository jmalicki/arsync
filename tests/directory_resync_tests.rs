#![cfg(unix)]
//! Tests for re-syncing to existing directories
//!
//! These tests verify that arsync correctly updates metadata when syncing
//! to directories that already exist (re-sync scenario).

mod common;

use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
use tempfile::TempDir;

/// Test: Re-syncing to existing directory should UPDATE metadata
///
/// This verifies rsync-compatible behavior: when destination directory exists,
/// metadata should be updated to match source (not skipped).
#[compio::test]
async fn test_resync_updates_directory_metadata() {
    let temp_dir = TempDir::new().unwrap();
    let src_dir = temp_dir.path().join("src");
    let dst_dir = temp_dir.path().join("dst");

    // Create source directory with specific permissions
    fs::create_dir(&src_dir).unwrap();
    fs::set_permissions(&src_dir, fs::Permissions::from_mode(0o755)).unwrap();

    // Create destination directory with DIFFERENT permissions
    fs::create_dir(&dst_dir).unwrap();
    fs::set_permissions(&dst_dir, fs::Permissions::from_mode(0o700)).unwrap();

    // Verify different before sync
    let dst_before = fs::metadata(&dst_dir).unwrap().permissions().mode() & 0o777;
    assert_eq!(dst_before, 0o700, "Destination should start with 700");

    // Sync with --archive (should update metadata)
    let mut args = common::test_args::create_archive_test_args();
    args.paths.source = src_dir.clone();
    args.paths.destination = dst_dir.clone();

    arsync::sync::sync_files(&args).await.map(|_| ()).unwrap();

    // Verify metadata was UPDATED to match source
    let dst_after = fs::metadata(&dst_dir).unwrap().permissions().mode() & 0o777;
    assert_eq!(
        dst_after, 0o755,
        "Destination metadata should be updated to match source on re-sync"
    );

    println!("✅ Re-sync correctly updated directory metadata: 700 → 755");
}

/// Test: Type conflict (file exists, trying to create directory) should FAIL
///
/// This verifies rsync-compatible behavior: cannot overwrite file with directory
///
/// **Current implementation:** Type conflict detection IS implemented via DirectoryFd.
/// When `create_dir` fails with AlreadyExists, we verify the existing path is a directory.
/// If it's not (e.g., it's a file), we return an error.
#[compio::test]
#[ignore] // TODO: Test scenario may not match actual traversal (needs verification)
async fn test_type_conflict_file_to_directory_fails() {
    let temp_dir = TempDir::new().unwrap();
    let src_dir = temp_dir.path().join("src");
    let dst_base = temp_dir.path().join("dst");

    // Create source as directory
    fs::create_dir(&src_dir).unwrap();

    // Create destination base directory
    fs::create_dir(&dst_base).unwrap();

    // Create conflicting file where directory should go
    let conflict_path = dst_base.join("subdir");
    fs::write(&conflict_path, "existing file").unwrap();

    // Create source subdirectory
    let src_subdir = src_dir.join("subdir");
    fs::create_dir(&src_subdir).unwrap();

    // Try to sync - should FAIL with type conflict
    let mut args = common::test_args::create_archive_test_args();
    args.paths.source = src_dir.clone();
    args.paths.destination = dst_base.clone();

    let result = arsync::sync::sync_files(&args).await.map(|_| ());

    assert!(
        result.is_err(),
        "Should fail when trying to create directory over existing file"
    );

    let err_msg = format!("{:?}", result.unwrap_err());
    assert!(
        err_msg.contains("not a directory") || err_msg.contains("exists but is not"),
        "Error should indicate type conflict, got: {}",
        err_msg
    );

    println!("✅ Correctly rejected type conflict (file → directory)");
}

/// Test: File into directory - rsync nesting behavior
///
/// When source is file but destination is directory, rsync creates file INSIDE directory.
/// This is NOT a type conflict - it's intentional nesting behavior.
///
/// **Implementation status:** File copying already uses DirectoryFd-based operations,
/// but this specific nesting scenario (source=file, dest=dir) needs verification.
#[compio::test]
#[ignore] // TODO: Verify rsync's nesting behavior is correctly implemented
async fn test_file_into_existing_directory_creates_nested() {
    let temp_dir = TempDir::new().unwrap();
    let src_file = temp_dir.path().join("file.txt");
    let dst_dir = temp_dir.path().join("dst");

    // Create source file
    fs::write(&src_file, "content").unwrap();

    // Create destination as directory
    fs::create_dir(&dst_dir).unwrap();

    // Sync file to directory (rsync creates file INSIDE)
    let mut args = common::test_args::create_archive_test_args();
    args.paths.source = src_file.clone();
    args.paths.destination = dst_dir.clone();

    // This should succeed - file goes inside directory
    arsync::sync::sync_files(&args).await.map(|_| ()).unwrap();

    // Verify file was created inside directory
    let nested_file = dst_dir.join("file.txt");
    assert!(
        nested_file.exists(),
        "File should be created inside existing directory"
    );

    let content = fs::read_to_string(&nested_file).unwrap();
    assert_eq!(content, "content");

    println!("✅ Correctly created file inside existing directory (rsync-compatible nesting)");
}

/// Test: Re-syncing preserves timestamps on existing directories
#[compio::test]
async fn test_resync_preserves_directory_timestamps() {
    let temp_dir = TempDir::new().unwrap();
    let src_dir = temp_dir.path().join("src");
    let dst_dir = temp_dir.path().join("dst");

    // Create source directory
    fs::create_dir(&src_dir).unwrap();

    // Set specific old timestamp on source
    let old_time = libc::timespec {
        tv_sec: 1609459200, // Jan 1, 2021
        tv_nsec: 123456789,
    };
    let src_cstr = std::ffi::CString::new(src_dir.as_os_str().as_bytes()).unwrap();
    let times = [old_time, old_time];
    let rc = unsafe { libc::utimensat(libc::AT_FDCWD, src_cstr.as_ptr(), times.as_ptr(), 0) };
    assert_eq!(
        rc,
        0,
        "utimensat failed: {}",
        std::io::Error::last_os_error()
    );

    // Create destination directory with current timestamp (will differ from source's 2021 timestamp)
    fs::create_dir(&dst_dir).unwrap();

    // Sync with --archive (should update timestamps)
    let mut args = common::test_args::create_archive_test_args();
    args.paths.source = src_dir.clone();
    args.paths.destination = dst_dir.clone();

    arsync::sync::sync_files(&args).await.map(|_| ()).unwrap();

    // Verify destination timestamps match source
    use std::os::unix::fs::MetadataExt;
    let src_meta = fs::metadata(&src_dir).unwrap();
    let dst_meta = fs::metadata(&dst_dir).unwrap();

    assert_eq!(
        src_meta.mtime(),
        dst_meta.mtime(),
        "Directory mtime should be updated on re-sync"
    );

    #[cfg(target_os = "linux")]
    assert_eq!(
        src_meta.mtime_nsec(),
        dst_meta.mtime_nsec(),
        "Directory mtime nanoseconds should be updated on re-sync (Linux)"
    );

    println!("✅ Re-sync correctly updated directory timestamps");
}
