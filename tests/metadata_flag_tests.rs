#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Tests for metadata preservation flags (on/off behavior)
//!
//! This module tests that metadata preservation flags correctly control
//! whether metadata is preserved or not. Each test verifies that:
//! 1. With flag enabled: metadata IS preserved
//! 2. With flag disabled: metadata IS NOT preserved (uses default/umask)

use arsync::cli::Args;
use arsync::copy::copy_file;
use arsync::directory::{preserve_directory_metadata, ExtendedMetadata};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use tempfile::TempDir;

mod common;
use common::test_args::{create_minimal_test_args, ArgsBuilder};

/// Create Args with NO metadata preservation (default rsync behavior)
fn create_args_no_metadata() -> Args {
    create_minimal_test_args()
}

/// Create Args with only permissions preservation enabled
fn create_args_perms_only() -> Args {
    ArgsBuilder::new().perms(true).build()
}

/// Create Args with only timestamp preservation enabled
fn create_args_times_only() -> Args {
    ArgsBuilder::new().times(true).build()
}

/// Create Args with archive mode (all metadata)
fn create_args_archive() -> Args {
    ArgsBuilder::new().archive(true).build()
}

/// Test: Permissions are NOT preserved when --perms flag is OFF
///
/// Requirement: By default (no flags), file permissions should use umask defaults,
/// not preserve the source permissions.
#[compio::test]
async fn test_permissions_not_preserved_when_flag_off() {
    let temp_dir = TempDir::new().unwrap();
    let src_path = temp_dir.path().join("source.txt");
    let dst_path = temp_dir.path().join("destination.txt");

    // Create source file with restrictive permissions
    fs::write(&src_path, "Test content").unwrap();
    let src_perms = std::fs::Permissions::from_mode(0o600); // Owner read/write only
    fs::set_permissions(&src_path, src_perms).unwrap();

    // Get source permissions
    let src_metadata = fs::metadata(&src_path).unwrap();
    let src_mode = src_metadata.permissions().mode();

    // Copy WITHOUT --perms flag
    let args = create_args_no_metadata();
    copy_file(&src_path, &dst_path, &args).await.unwrap();

    // Destination should NOT have the same permissions as source
    let dst_metadata = fs::metadata(&dst_path).unwrap();
    let dst_mode = dst_metadata.permissions().mode();

    // The modes should be different (dst will use umask)
    assert_ne!(
        src_mode, dst_mode,
        "Permissions should NOT be preserved when --perms is off"
    );

    println!(
        "✓ Verified permissions NOT preserved: src={:o}, dst={:o}",
        src_mode, dst_mode
    );
}

/// Test: Permissions ARE preserved when --perms flag is ON
///
/// Requirement: With --perms (or -p), file permissions should exactly match source.
#[compio::test]
async fn test_permissions_preserved_when_flag_on() {
    let temp_dir = TempDir::new().unwrap();
    let src_path = temp_dir.path().join("source.txt");
    let dst_path = temp_dir.path().join("destination.txt");

    // Create source file with specific permissions
    fs::write(&src_path, "Test content").unwrap();
    let src_perms = std::fs::Permissions::from_mode(0o640);
    fs::set_permissions(&src_path, src_perms).unwrap();

    // Get source permissions
    let src_metadata = fs::metadata(&src_path).unwrap();
    let src_mode = src_metadata.permissions().mode();

    // Copy WITH --perms flag
    let args = create_args_perms_only();
    copy_file(&src_path, &dst_path, &args).await.unwrap();

    // Destination SHOULD have the same permissions as source
    let dst_metadata = fs::metadata(&dst_path).unwrap();
    let dst_mode = dst_metadata.permissions().mode();

    assert_eq!(
        src_mode, dst_mode,
        "Permissions SHOULD be preserved when --perms is on"
    );

    println!(
        "✓ Verified permissions preserved: src={:o}, dst={:o}",
        src_mode, dst_mode
    );
}

/// Test: Timestamps are NOT preserved when --times flag is OFF
///
/// Requirement: By default (no flags), destination should have current timestamps,
/// not source timestamps.
#[compio::test]
async fn test_timestamps_not_preserved_when_flag_off() {
    let temp_dir = TempDir::new().unwrap();
    let src_path = temp_dir.path().join("source.txt");
    let dst_path = temp_dir.path().join("destination.txt");

    // Create source file
    fs::write(&src_path, "Test content").unwrap();

    // Get source timestamps
    let src_metadata = fs::metadata(&src_path).unwrap();
    let src_modified = src_metadata.modified().unwrap();

    // Wait to ensure different timestamps
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Copy WITHOUT --times flag
    let args = create_args_no_metadata();
    copy_file(&src_path, &dst_path, &args).await.unwrap();

    // Destination should have DIFFERENT (newer) timestamps
    let dst_metadata = fs::metadata(&dst_path).unwrap();
    let dst_modified = dst_metadata.modified().unwrap();

    // Destination timestamp should be newer than source
    assert!(
        dst_modified > src_modified,
        "Timestamps should NOT be preserved when --times is off (dst should be newer)"
    );

    println!(
        "✓ Verified timestamps NOT preserved: src={:?}, dst={:?}",
        src_modified, dst_modified
    );
}

/// Test: Timestamps ARE preserved when --times flag is ON
///
/// Requirement: With --times (or -t), modification times should match source.
#[compio::test]
async fn test_timestamps_preserved_when_flag_on() {
    let temp_dir = TempDir::new().unwrap();
    let src_path = temp_dir.path().join("source.txt");
    let dst_path = temp_dir.path().join("destination.txt");

    // Create source file
    fs::write(&src_path, "Test content").unwrap();

    // Get source timestamps
    let src_metadata = fs::metadata(&src_path).unwrap();
    let src_modified = src_metadata.modified().unwrap();

    // Wait to ensure we're not just getting lucky with fast operations
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Copy WITH --times flag
    let args = create_args_times_only();
    copy_file(&src_path, &dst_path, &args).await.unwrap();

    // Destination SHOULD have the same timestamps as source (within precision)
    let dst_metadata = fs::metadata(&dst_path).unwrap();
    let dst_modified = dst_metadata.modified().unwrap();

    // Allow for filesystem timestamp precision (within 1ms)
    let diff = dst_modified
        .duration_since(src_modified)
        .unwrap_or_else(|_| src_modified.duration_since(dst_modified).unwrap());

    assert!(
        diff < std::time::Duration::from_millis(10),
        "Timestamps SHOULD be preserved when --times is on (diff: {:?})",
        diff
    );

    println!(
        "✓ Verified timestamps preserved: src={:?}, dst={:?}, diff={:?}",
        src_modified, dst_modified, diff
    );
}

/// Test: Archive mode (-a) enables all metadata preservation
///
/// Requirement: -a should be equivalent to -rlptgoD (recursive, links, perms,
/// times, group, owner, devices)
#[compio::test]
async fn test_archive_mode_preserves_all_metadata() {
    let temp_dir = TempDir::new().unwrap();
    let src_path = temp_dir.path().join("source.txt");
    let dst_path = temp_dir.path().join("destination.txt");

    // Create source file with specific permissions
    fs::write(&src_path, "Test content").unwrap();
    let src_perms = std::fs::Permissions::from_mode(0o754);
    fs::set_permissions(&src_path, src_perms).unwrap();

    // Get source metadata
    let src_metadata = fs::metadata(&src_path).unwrap();
    let src_mode = src_metadata.permissions().mode();
    let src_modified = src_metadata.modified().unwrap();

    // Wait for timestamp difference
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Copy WITH --archive flag
    let args = create_args_archive();
    copy_file(&src_path, &dst_path, &args).await.unwrap();

    // Verify both permissions and timestamps are preserved
    let dst_metadata = fs::metadata(&dst_path).unwrap();
    let dst_mode = dst_metadata.permissions().mode();
    let dst_modified = dst_metadata.modified().unwrap();

    // Permissions should match
    assert_eq!(
        src_mode, dst_mode,
        "Archive mode should preserve permissions"
    );

    // Timestamps should match (within precision)
    let time_diff = dst_modified
        .duration_since(src_modified)
        .unwrap_or_else(|_| src_modified.duration_since(dst_modified).unwrap());

    assert!(
        time_diff < std::time::Duration::from_millis(10),
        "Archive mode should preserve timestamps (diff: {:?})",
        time_diff
    );

    println!(
        "✓ Verified archive mode preserves all metadata: perms={:o}, time_diff={:?}",
        dst_mode, time_diff
    );
}

/// Test: Directory permissions are NOT preserved when --perms flag is OFF
#[compio::test]
async fn test_directory_permissions_not_preserved_when_flag_off() {
    let temp_dir = TempDir::new().unwrap();
    let src_dir = temp_dir.path().join("src_dir");
    let dst_dir = temp_dir.path().join("dst_dir");

    // Create source directory with specific permissions
    fs::create_dir(&src_dir).unwrap();
    let src_perms = std::fs::Permissions::from_mode(0o750); // rwxr-x---
    fs::set_permissions(&src_dir, src_perms).unwrap();

    // Create destination directory
    fs::create_dir(&dst_dir).unwrap();

    // Get source metadata
    let src_metadata = fs::metadata(&src_dir).unwrap();
    let src_mode = src_metadata.permissions().mode();

    // Preserve metadata WITHOUT --perms flag
    let extended_metadata = ExtendedMetadata::new(&src_dir).await.unwrap();
    let args = create_args_no_metadata();
    preserve_directory_metadata(&src_dir, &dst_dir, &extended_metadata, &args)
        .await
        .unwrap();

    // Destination should NOT have the same permissions as source
    let dst_metadata = fs::metadata(&dst_dir).unwrap();
    let dst_mode = dst_metadata.permissions().mode();

    // Modes should be different (dst keeps its creation permissions)
    assert_ne!(
        src_mode, dst_mode,
        "Directory permissions should NOT be preserved when --perms is off"
    );

    println!(
        "✓ Verified directory permissions NOT preserved: src={:o}, dst={:o}",
        src_mode, dst_mode
    );
}

/// Test: Directory permissions ARE preserved when --perms flag is ON
#[compio::test]
async fn test_directory_permissions_preserved_when_flag_on() {
    let temp_dir = TempDir::new().unwrap();
    let src_dir = temp_dir.path().join("src_dir");
    let dst_dir = temp_dir.path().join("dst_dir");

    // Create source directory with specific permissions
    fs::create_dir(&src_dir).unwrap();
    let src_perms = std::fs::Permissions::from_mode(0o755); // rwxr-xr-x
    fs::set_permissions(&src_dir, src_perms).unwrap();

    // Create destination directory
    fs::create_dir(&dst_dir).unwrap();

    // Get source metadata
    let src_metadata = fs::metadata(&src_dir).unwrap();
    let src_mode = src_metadata.permissions().mode();

    // Preserve metadata WITH --perms flag
    let extended_metadata = ExtendedMetadata::new(&src_dir).await.unwrap();
    let args = create_args_perms_only();
    preserve_directory_metadata(&src_dir, &dst_dir, &extended_metadata, &args)
        .await
        .unwrap();

    // Destination SHOULD have the same permissions as source
    let dst_metadata = fs::metadata(&dst_dir).unwrap();
    let dst_mode = dst_metadata.permissions().mode();

    assert_eq!(
        src_mode, dst_mode,
        "Directory permissions SHOULD be preserved when --perms is on"
    );

    println!(
        "✓ Verified directory permissions preserved: src={:o}, dst={:o}",
        src_mode, dst_mode
    );
}

/// Test: Individual flags override archive mode behavior
///
/// Requirement: Individual flags like -p should work the same whether used alone
/// or as part of -a
#[compio::test]
async fn test_individual_flags_match_archive_components() {
    let temp_dir = TempDir::new().unwrap();

    // Test 1: --perms alone vs --archive (perms component)
    let src_path1 = temp_dir.path().join("source1.txt");
    let dst_path1_perms = temp_dir.path().join("dest1_perms.txt");
    let dst_path1_archive = temp_dir.path().join("dest1_archive.txt");

    fs::write(&src_path1, "Test").unwrap();
    let src_perms = std::fs::Permissions::from_mode(0o640);
    fs::set_permissions(&src_path1, src_perms).unwrap();

    let src_mode = fs::metadata(&src_path1).unwrap().permissions().mode();

    // Copy with --perms only
    let args_perms = create_args_perms_only();
    copy_file(&src_path1, &dst_path1_perms, &args_perms)
        .await
        .unwrap();

    // Copy with --archive
    let args_archive = create_args_archive();
    copy_file(&src_path1, &dst_path1_archive, &args_archive)
        .await
        .unwrap();

    // Both should preserve permissions identically
    let dst_mode_perms = fs::metadata(&dst_path1_perms).unwrap().permissions().mode();
    let dst_mode_archive = fs::metadata(&dst_path1_archive)
        .unwrap()
        .permissions()
        .mode();

    assert_eq!(
        src_mode, dst_mode_perms,
        "--perms should preserve permissions"
    );
    assert_eq!(
        src_mode, dst_mode_archive,
        "--archive should preserve permissions"
    );
    assert_eq!(
        dst_mode_perms, dst_mode_archive,
        "--perms and --archive should have same permission behavior"
    );

    println!("✓ Verified --perms and --archive permission behavior matches");
}
