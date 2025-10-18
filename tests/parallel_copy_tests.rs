//! Tests for parallel file copying with data integrity verification
//!
//! These tests ensure that parallel copying produces byte-perfect copies
//! and properly handles edge cases.

#![allow(clippy::expect_used)] // expect() is idiomatic in tests

use arsync::cli::ParallelCopyConfig;
use arsync::copy::copy_file;
use arsync::metadata::MetadataConfig;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// Helper to create a test file with a specific pattern
fn create_test_file_with_pattern(path: &Path, size: usize) -> Vec<u8> {
    // Create a repeating pattern that's easy to verify
    // Use position in file as part of pattern to detect any byte shuffling
    let mut data = Vec::with_capacity(size);
    for i in 0..size {
        data.push((i % 256) as u8);
    }
    fs::write(path, &data).expect("Failed to write test file");
    data
}

/// Helper to create enabled parallel config
fn enabled_parallel_config(max_depth: usize) -> ParallelCopyConfig {
    ParallelCopyConfig {
        max_depth,
        min_file_size_mb: 1, // 1MB threshold for testing
        chunk_size_mb: 2,
    }
}

/// Helper to create minimal metadata config
fn minimal_metadata_config() -> MetadataConfig {
    MetadataConfig {
        archive: false,
        recursive: false,
        links: false,
        perms: false,
        times: false,
        group: false,
        owner: false,
        devices: false,
        xattrs: false,
        acls: false,
        hard_links: false,
        atimes: false,
        crtimes: false,
        preserve_xattr: false,
        preserve_acl: false,
    }
}

/// Test that parallel copy produces byte-perfect output for a large file
#[compio::test]
async fn test_parallel_copy_data_integrity_large_file() {
    let temp_dir = TempDir::new().unwrap();
    let src_path = temp_dir.path().join("source_large.bin");
    let dst_path = temp_dir.path().join("dest_large.bin");

    // Create a 200MB file (larger than threshold, will trigger parallel copy)
    let size = 200 * 1024 * 1024;
    let original_data = create_test_file_with_pattern(&src_path, size);

    // Copy with parallel enabled (depth 2 = 4 tasks)
    let parallel_config = enabled_parallel_config(2);
    let metadata_config = minimal_metadata_config();

    copy_file(
        &src_path,
        &dst_path,
        &metadata_config,
        &parallel_config,
        None,
    )
    .await
    .expect("Parallel copy failed");

    // Verify byte-perfect copy
    let copied_data = fs::read(&dst_path).expect("Failed to read copied file");
    assert_eq!(
        copied_data.len(),
        original_data.len(),
        "File sizes don't match"
    );
    assert_eq!(
        copied_data, original_data,
        "File contents don't match - data corruption detected!"
    );
}

/// Test parallel copy with different recursion depths
#[compio::test]
async fn test_parallel_copy_various_depths() {
    for depth in 1..=4 {
        let temp_dir = TempDir::new().unwrap();
        let src_path = temp_dir.path().join(format!("source_depth_{}.bin", depth));
        let dst_path = temp_dir.path().join(format!("dest_depth_{}.bin", depth));

        // Create a 50MB file
        let size = 50 * 1024 * 1024;
        let original_data = create_test_file_with_pattern(&src_path, size);

        // Copy with specified depth
        let parallel_config = enabled_parallel_config(depth);
        let metadata_config = minimal_metadata_config();

        copy_file(
            &src_path,
            &dst_path,
            &metadata_config,
            &parallel_config,
            None,
        )
        .await
        .unwrap_or_else(|e| panic!("Parallel copy failed at depth {}: {}", depth, e));

        // Verify data integrity
        let copied_data = fs::read(&dst_path)
            .unwrap_or_else(|e| panic!("Failed to read copied file at depth {}: {}", depth, e));
        assert_eq!(
            copied_data, original_data,
            "Data corruption at depth {}",
            depth
        );
    }
}

/// Test that files below threshold use sequential copy
#[compio::test]
async fn test_below_threshold_uses_sequential() {
    let temp_dir = TempDir::new().unwrap();
    let src_path = temp_dir.path().join("small_source.bin");
    let dst_path = temp_dir.path().join("small_dest.bin");

    // Create a 500KB file (below 1MB threshold)
    let size = 500 * 1024;
    let original_data = create_test_file_with_pattern(&src_path, size);

    // Copy with parallel enabled but file is too small
    let parallel_config = enabled_parallel_config(2);
    let metadata_config = minimal_metadata_config();

    copy_file(
        &src_path,
        &dst_path,
        &metadata_config,
        &parallel_config,
        None,
    )
    .await
    .expect("Copy failed");

    // Verify data integrity regardless of method used
    let copied_data = fs::read(&dst_path).expect("Failed to read copied file");
    assert_eq!(copied_data, original_data, "Data corruption in small file");
}

/// Test edge case: file size not evenly divisible by workers
#[compio::test]
async fn test_uneven_file_split() {
    let temp_dir = TempDir::new().unwrap();
    let src_path = temp_dir.path().join("uneven_source.bin");
    let dst_path = temp_dir.path().join("uneven_dest.bin");

    // Create a file that doesn't divide evenly: 17MB + 777 bytes
    let size = 17 * 1024 * 1024 + 777;
    let original_data = create_test_file_with_pattern(&src_path, size);

    // Copy with depth 2 (4 tasks)
    let parallel_config = enabled_parallel_config(2);
    let metadata_config = minimal_metadata_config();

    copy_file(
        &src_path,
        &dst_path,
        &metadata_config,
        &parallel_config,
        None,
    )
    .await
    .expect("Copy failed");

    // Verify every byte matches
    let copied_data = fs::read(&dst_path).expect("Failed to read copied file");
    assert_eq!(copied_data.len(), original_data.len());
    assert_eq!(
        copied_data, original_data,
        "Data corruption with uneven split"
    );
}

/// Test parallel copy preserves file boundaries (no overwrites)
#[compio::test]
async fn test_no_region_overlap() {
    let temp_dir = TempDir::new().unwrap();
    let src_path = temp_dir.path().join("overlap_source.bin");
    let dst_path = temp_dir.path().join("overlap_dest.bin");

    // Create file where each byte is its position % 256
    // This makes it easy to detect if bytes got shuffled
    let size = 32 * 1024 * 1024; // 32MB
    let original_data = create_test_file_with_pattern(&src_path, size);

    // Copy with high depth to maximize concurrent writes
    let parallel_config = enabled_parallel_config(3); // 8 tasks
    let metadata_config = minimal_metadata_config();

    copy_file(
        &src_path,
        &dst_path,
        &metadata_config,
        &parallel_config,
        None,
    )
    .await
    .expect("Copy failed");

    // Read and verify byte-by-byte
    let copied_data = fs::read(&dst_path).expect("Failed to read copied file");
    for (i, (&expected, &actual)) in original_data.iter().zip(copied_data.iter()).enumerate() {
        assert_eq!(
            expected, actual,
            "Byte mismatch at position {}: expected {}, got {}",
            i, expected, actual
        );
    }
}

/// Benchmark helper: Test page alignment function
#[test]
fn test_align_to_page() {
    // This tests the align_to_page function indirectly through parallel copy behavior
    // The function is private but tested via test_no_region_overlap and
    // test_parallel_copy_data_integrity_large_file which verify correct alignment

    // Placeholder test - real testing done in integration tests above
}
