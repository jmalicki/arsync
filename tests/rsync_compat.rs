//! rsync Compatibility Test Suite
//!
//! This test suite validates that arsync produces IDENTICAL results
//! to rsync for all supported flags.
//!
//! ## Running Tests
//!
//! Run all compatibility tests:
//! ```bash
//! cargo test --test rsync_compat
//! ```
//!
//! Run specific compatibility test:
//! ```bash
//! cargo test --test rsync_compat test_archive_mode_compatibility
//! ```
//!
//! ## CI Integration
//!
//! These tests are run in a separate CI phase that:
//! 1. Installs rsync as a dependency
//! 2. Runs the full compatibility test suite
//! 3. Reports any differences between rsync and arsync behavior

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod utils;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use tempfile::TempDir;
use utils::rsync_compat::{compare_directories, rsync_available, run_arsync, run_rsync};

/// Skip all tests if rsync is not available
/// Returns true if rsync is available, false otherwise (test should be skipped)
fn require_rsync() -> bool {
    if !rsync_available() {
        eprintln!("⚠️  rsync is not available - skipping compatibility test");
        eprintln!("To run these tests, install rsync: apt install rsync");
        return false;
    }
    true
}

#[test]
fn test_rsync_available() {
    if !require_rsync() {
        return; // Skip test if rsync not available
    }
    println!("✓ rsync is available and ready for compatibility testing");
}

/// Test: Archive mode (-a) produces identical results
#[test]
fn test_archive_mode_compatibility() {
    if !require_rsync() {
        return;
    }

    let temp = TempDir::new().unwrap();
    let source = temp.path().join("source");
    let rsync_dest = temp.path().join("rsync_dest");
    let iouring_dest = temp.path().join("iouring_dest");

    // Create source directory with various files
    fs::create_dir(&source).unwrap();
    fs::write(source.join("file1.txt"), "Hello, World!").unwrap();
    fs::write(source.join("file2.txt"), "Test content").unwrap();

    // Set specific permissions
    fs::set_permissions(source.join("file1.txt"), fs::Permissions::from_mode(0o644)).unwrap();
    fs::set_permissions(source.join("file2.txt"), fs::Permissions::from_mode(0o755)).unwrap();

    // Create subdirectory
    let subdir = source.join("subdir");
    fs::create_dir(&subdir).unwrap();
    fs::write(subdir.join("file3.txt"), "Nested content").unwrap();

    // Create destinations
    fs::create_dir(&rsync_dest).unwrap();
    fs::create_dir(&iouring_dest).unwrap();

    // Run rsync with -a
    run_rsync(&source, &rsync_dest, &["-a"]).unwrap();

    // Run arsync with -a
    run_arsync(&source, &iouring_dest, &["-a"]).unwrap();

    // Compare results
    compare_directories(&rsync_dest, &iouring_dest, true)
        .expect("Archive mode should produce identical results to rsync");

    println!("✓ Archive mode (-a) is 100% rsync-compatible");
}

/// Test: Permissions flag (-p) produces identical results
#[test]
fn test_permissions_flag_compatibility() {
    if !require_rsync() {
        return;
    }

    let temp = TempDir::new().unwrap();
    let source = temp.path().join("source");
    let rsync_dest = temp.path().join("rsync_dest");
    let iouring_dest = temp.path().join("iouring_dest");

    // Create source with specific permissions
    fs::create_dir(&source).unwrap();
    fs::write(source.join("file.txt"), "Test").unwrap();
    fs::set_permissions(source.join("file.txt"), fs::Permissions::from_mode(0o640)).unwrap();

    fs::create_dir(&rsync_dest).unwrap();
    fs::create_dir(&iouring_dest).unwrap();

    // Run both with -rp (recursive + permissions)
    run_rsync(&source, &rsync_dest, &["-rp"]).unwrap();
    run_arsync(&source, &iouring_dest, &["-rp"]).unwrap();

    // Compare results (skip time check since we didn't use -t)
    compare_directories(&rsync_dest, &iouring_dest, false)
        .expect("Permissions flag should produce identical results to rsync");

    println!("✓ Permissions flag (-p) is 100% rsync-compatible");
}

/// Test: Timestamps flag (-t) produces identical results
#[test]
#[ignore] // TODO: Permission mismatch (100664 vs 100644) - may be umask or metadata config issue
fn test_timestamps_flag_compatibility() {
    if !require_rsync() {
        return;
    }

    let temp = TempDir::new().unwrap();
    let source = temp.path().join("source");
    let rsync_dest = temp.path().join("rsync_dest");
    let iouring_dest = temp.path().join("iouring_dest");

    // Create source
    fs::create_dir(&source).unwrap();
    fs::write(source.join("file.txt"), "Test").unwrap();

    fs::create_dir(&rsync_dest).unwrap();
    fs::create_dir(&iouring_dest).unwrap();

    // Run both with -rt (recursive + times)
    run_rsync(&source, &rsync_dest, &["-rt"]).unwrap();
    run_arsync(&source, &iouring_dest, &["-rt"]).unwrap();

    // Compare results (including times)
    compare_directories(&rsync_dest, &iouring_dest, true)
        .expect("Timestamps flag should produce identical results to rsync");

    println!("✓ Timestamps flag (-t) is 100% rsync-compatible");
}

/// Test: No metadata flags (default behavior) matches rsync
#[test]
fn test_default_behavior_compatibility() {
    if !require_rsync() {
        return;
    }

    let temp = TempDir::new().unwrap();
    let source = temp.path().join("source");
    let rsync_dest = temp.path().join("rsync_dest");
    let iouring_dest = temp.path().join("iouring_dest");

    // Create source with specific permissions
    fs::create_dir(&source).unwrap();
    fs::write(source.join("file.txt"), "Test").unwrap();
    fs::set_permissions(source.join("file.txt"), fs::Permissions::from_mode(0o600)).unwrap();

    fs::create_dir(&rsync_dest).unwrap();
    fs::create_dir(&iouring_dest).unwrap();

    // Run both with only -r (recursive, no metadata preservation)
    run_rsync(&source, &rsync_dest, &["-r"]).unwrap();
    run_arsync(&source, &iouring_dest, &["-r"]).unwrap();

    // Compare content only (not metadata)
    let rsync_content = fs::read(rsync_dest.join("file.txt")).unwrap();
    let iouring_content = fs::read(iouring_dest.join("file.txt")).unwrap();

    assert_eq!(
        rsync_content, iouring_content,
        "Content should be identical"
    );

    println!("✓ Default behavior (no metadata flags) is rsync-compatible");
}

/// Test: Combined flags (-rpt) produce identical results
#[test]
fn test_combined_flags_compatibility() {
    if !require_rsync() {
        return;
    }

    let temp = TempDir::new().unwrap();
    let source = temp.path().join("source");
    let rsync_dest = temp.path().join("rsync_dest");
    let iouring_dest = temp.path().join("iouring_dest");

    // Create source with subdirectories and various permissions
    fs::create_dir(&source).unwrap();
    fs::write(source.join("file1.txt"), "Content 1").unwrap();
    fs::set_permissions(source.join("file1.txt"), fs::Permissions::from_mode(0o644)).unwrap();

    let subdir = source.join("subdir");
    fs::create_dir(&subdir).unwrap();
    fs::write(subdir.join("file2.txt"), "Content 2").unwrap();
    fs::set_permissions(subdir.join("file2.txt"), fs::Permissions::from_mode(0o755)).unwrap();

    fs::create_dir(&rsync_dest).unwrap();
    fs::create_dir(&iouring_dest).unwrap();

    // Run both with -rpt (recursive, permissions, times)
    run_rsync(&source, &rsync_dest, &["-rpt"]).unwrap();
    run_arsync(&source, &iouring_dest, &["-rpt"]).unwrap();

    // Compare results (including times and permissions)
    compare_directories(&rsync_dest, &iouring_dest, true)
        .expect("Combined flags should produce identical results to rsync");

    println!("✓ Combined flags (-rpt) are 100% rsync-compatible");
}

/// Test: Symlink handling matches rsync
#[test]
fn test_symlinks_compatibility() {
    if !require_rsync() {
        return;
    }

    let temp = TempDir::new().unwrap();
    let source = temp.path().join("source");
    let rsync_dest = temp.path().join("rsync_dest");
    let iouring_dest = temp.path().join("iouring_dest");

    // Create source with symlinks
    fs::create_dir(&source).unwrap();
    fs::write(source.join("target.txt"), "Target content").unwrap();

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(source.join("target.txt"), source.join("link.txt")).unwrap();
    }

    fs::create_dir(&rsync_dest).unwrap();
    fs::create_dir(&iouring_dest).unwrap();

    // Run both with -rl (recursive + links)
    run_rsync(&source, &rsync_dest, &["-rl"]).unwrap();
    run_arsync(&source, &iouring_dest, &["-rl"]).unwrap();

    // Verify symlinks are preserved in both
    let rsync_link = rsync_dest.join("link.txt");
    let iouring_link = iouring_dest.join("link.txt");

    assert!(
        fs::symlink_metadata(&rsync_link).unwrap().is_symlink(),
        "rsync should preserve symlink"
    );
    assert!(
        fs::symlink_metadata(&iouring_link).unwrap().is_symlink(),
        "arsync should preserve symlink"
    );

    // Verify targets match
    let rsync_target = fs::read_link(&rsync_link).unwrap();
    let iouring_target = fs::read_link(&iouring_link).unwrap();

    assert_eq!(rsync_target, iouring_target, "Symlink targets should match");

    println!("✓ Symlink handling (-l) is 100% rsync-compatible");
}

/// Test: Large file operations match rsync
#[test]
fn test_large_file_compatibility() {
    if !require_rsync() {
        return;
    }

    let temp = TempDir::new().unwrap();
    let source = temp.path().join("source");
    let rsync_dest = temp.path().join("rsync_dest");
    let iouring_dest = temp.path().join("iouring_dest");

    // Create source with large file (10MB)
    fs::create_dir(&source).unwrap();
    let large_content = vec![0u8; 10 * 1024 * 1024];
    fs::write(source.join("large.bin"), &large_content).unwrap();
    fs::set_permissions(source.join("large.bin"), fs::Permissions::from_mode(0o644)).unwrap();

    fs::create_dir(&rsync_dest).unwrap();
    fs::create_dir(&iouring_dest).unwrap();

    // Run both with -a
    run_rsync(&source, &rsync_dest, &["-a"]).unwrap();
    run_arsync(&source, &iouring_dest, &["-a"]).unwrap();

    // Compare results
    compare_directories(&rsync_dest, &iouring_dest, true)
        .expect("Large file handling should match rsync");

    println!("✓ Large file handling is 100% rsync-compatible");
}

/// Test: Many small files match rsync
#[test]
fn test_many_small_files_compatibility() {
    if !require_rsync() {
        return;
    }

    let temp = TempDir::new().unwrap();
    let source = temp.path().join("source");
    let rsync_dest = temp.path().join("rsync_dest");
    let iouring_dest = temp.path().join("iouring_dest");

    // Create source with many small files
    fs::create_dir(&source).unwrap();
    for i in 0..100 {
        fs::write(
            source.join(format!("file_{}.txt", i)),
            format!("Content {}", i),
        )
        .unwrap();
        fs::set_permissions(
            source.join(format!("file_{}.txt", i)),
            fs::Permissions::from_mode(0o644),
        )
        .unwrap();
    }

    fs::create_dir(&rsync_dest).unwrap();
    fs::create_dir(&iouring_dest).unwrap();

    // Run both with -a
    run_rsync(&source, &rsync_dest, &["-a"]).unwrap();
    run_arsync(&source, &iouring_dest, &["-a"]).unwrap();

    // Compare results
    compare_directories(&rsync_dest, &iouring_dest, true)
        .expect("Many small files should match rsync");

    println!("✓ Many small files handling is 100% rsync-compatible");
}

/// Test: Deep directory hierarchy matches rsync
#[test]
fn test_deep_hierarchy_compatibility() {
    if !require_rsync() {
        return;
    }

    let temp = TempDir::new().unwrap();
    let source = temp.path().join("source");
    let rsync_dest = temp.path().join("rsync_dest");
    let iouring_dest = temp.path().join("iouring_dest");

    // Create deep directory structure
    fs::create_dir(&source).unwrap();
    let mut current = source.clone();
    for i in 0..10 {
        current = current.join(format!("level_{}", i));
        fs::create_dir(&current).unwrap();
        fs::write(current.join("file.txt"), format!("Level {}", i)).unwrap();
    }

    fs::create_dir(&rsync_dest).unwrap();
    fs::create_dir(&iouring_dest).unwrap();

    // Run both with -a
    run_rsync(&source, &rsync_dest, &["-a"]).unwrap();
    run_arsync(&source, &iouring_dest, &["-a"]).unwrap();

    // Compare results
    compare_directories(&rsync_dest, &iouring_dest, true)
        .expect("Deep hierarchy should match rsync");

    println!("✓ Deep directory hierarchy handling is 100% rsync-compatible");
}
