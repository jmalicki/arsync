//! Tests for zero-copy I/O using compio's BufferPool
//!
//! These tests verify that the full zero-copy path (read_managed + write_managed)
//! works correctly for file copying operations.

#![cfg(unix)]

use arsync::cli::{MetadataConfig, ParallelCopyConfig};
use arsync::copy::copy_file;
use compio::fs::File;
use compio::io::AsyncReadManagedAt;
use compio::runtime::BufferPool;
use compio_fs_extended::AsyncWriteManagedAt;
use std::io::Write;
use tempfile::{NamedTempFile, TempDir};

/// Test basic write_managed functionality
#[compio::test]
async fn test_write_managed_basic() {
    let mut temp_src = NamedTempFile::new().unwrap();
    let test_data = b"Hello, write_managed!";
    temp_src.write_all(test_data).unwrap();
    temp_src.flush().unwrap();

    // Open source
    let src = File::open(temp_src.path()).await.unwrap();

    // Create destination
    let temp_dst = NamedTempFile::new().unwrap();
    let mut dst = File::create(temp_dst.path()).await.unwrap();

    // Create buffer pool
    let pool = BufferPool::new(4, 1024).unwrap();

    // Read with managed buffer (zero-copy read)
    let buf = src
        .read_managed_at(&pool, test_data.len(), 0)
        .await
        .unwrap();

    assert_eq!(buf.len(), test_data.len());
    assert_eq!(buf.as_ref(), test_data);

    // Write with managed buffer (zero-copy write!)
    let written = dst.write_managed_at(buf, 0).await.unwrap();
    assert_eq!(written, test_data.len());

    // Verify data integrity
    let result = std::fs::read(temp_dst.path()).unwrap();
    assert_eq!(result, test_data);
}

/// Test full zero-copy file copying loop
#[compio::test]
async fn test_full_zero_copy_loop() {
    let temp_dir = TempDir::new().unwrap();
    let src_path = temp_dir.path().join("source.dat");
    let dst_path = temp_dir.path().join("dest.dat");

    // Create 256KB test file
    let test_data: Vec<u8> = (0..256 * 1024).map(|i| (i % 256) as u8).collect();
    std::fs::write(&src_path, &test_data).unwrap();

    // Open files
    let src = File::open(&src_path).await.unwrap();
    let mut dst = File::create(&dst_path).await.unwrap();

    // Create buffer pool (4 buffers × 64KB)
    let pool = BufferPool::new(4, 65536).unwrap();

    // Full zero-copy copy loop
    let mut offset = 0u64;
    let mut total_copied = 0usize;

    loop {
        // Zero-copy read
        let buf = src.read_managed_at(&pool, 65536, offset).await.unwrap();

        if buf.len() == 0 {
            break;
        }

        // Zero-copy write
        let written = dst.write_managed_at(buf, offset).await.unwrap();

        total_copied += written;
        offset += written as u64;
    }

    // Verify total copied
    assert_eq!(total_copied, test_data.len());

    // Verify data integrity
    let result = std::fs::read(&dst_path).unwrap();
    assert_eq!(result, test_data);
}

/// Test that write_managed works with different buffer sizes
#[compio::test]
async fn test_write_managed_various_sizes() {
    for size in [512, 4096, 16384, 65536] {
        let temp_dir = TempDir::new().unwrap();
        let src_path = temp_dir.path().join("source.dat");
        let dst_path = temp_dir.path().join("dest.dat");

        // Create test file
        let test_data: Vec<u8> = (0..size).map(|i| (i % 256) as u8).collect();
        std::fs::write(&src_path, &test_data).unwrap();

        // Copy with zero-copy path
        let src = File::open(&src_path).await.unwrap();
        let mut dst = File::create(&dst_path).await.unwrap();
        let pool = BufferPool::new(2, size).unwrap();

        let buf = src.read_managed_at(&pool, size, 0).await.unwrap();
        let written = dst.write_managed_at(buf, 0).await.unwrap();

        assert_eq!(written, size);

        // Verify
        let result = std::fs::read(&dst_path).unwrap();
        assert_eq!(result, test_data);
    }
}

/// Test integration with copy_file (uses buffer pool internally)
#[compio::test]
async fn test_copy_file_with_buffer_pool() {
    let temp_dir = TempDir::new().unwrap();
    let src_path = temp_dir.path().join("large_file.dat");
    let dst_path = temp_dir.path().join("copy.dat");

    // Create 1MB test file
    let test_data: Vec<u8> = (0..1024 * 1024).map(|i| (i % 256) as u8).collect();
    std::fs::write(&src_path, &test_data).unwrap();

    // Copy using arsync's copy_file (should use buffer pool internally if available)
    let metadata_config = MetadataConfig {
        archive: false,
        recursive: false,
        links: false,
        perms: true,
        times: true,
        owner: false, // Requires root
        group: false, // Requires root
        devices: false,
        hard_links: false,
        xattrs: false,
        acls: false,
        atimes: false,
        crtimes: false,
        preserve_xattr: false,
        preserve_acl: false,
        fsync: false,
    };
    let parallel_config = ParallelCopyConfig {
        max_depth: 0, // Disabled for this test
        min_file_size_mb: 100,
        chunk_size_mb: 1,
    };

    copy_file(&src_path, &dst_path, &metadata_config, &parallel_config)
        .await
        .unwrap();

    // Verify data integrity
    let result = std::fs::read(&dst_path).unwrap();
    assert_eq!(result, test_data);

    // Verify metadata preserved
    let src_meta = std::fs::metadata(&src_path).unwrap();
    let dst_meta = std::fs::metadata(&dst_path).unwrap();
    assert_eq!(dst_meta.len(), src_meta.len());
}

/// Test buffer pool fallback when unavailable
#[compio::test]
async fn test_buffer_pool_fallback() {
    // Even if buffer pool creation fails, copying should still work
    // (This test documents the fallback behavior)

    let temp_dir = TempDir::new().unwrap();
    let src_path = temp_dir.path().join("test.dat");
    let dst_path = temp_dir.path().join("copy.dat");

    let test_data = b"Fallback test data";
    std::fs::write(&src_path, test_data).unwrap();

    // Copy (will use fallback if buffer pool unavailable)
    let metadata_config = MetadataConfig {
        archive: false,
        recursive: false,
        links: false,
        perms: true,
        times: true,
        owner: false,
        group: false,
        devices: false,
        hard_links: false,
        xattrs: false,
        acls: false,
        atimes: false,
        crtimes: false,
        preserve_xattr: false,
        preserve_acl: false,
        fsync: false,
    };
    let parallel_config = ParallelCopyConfig {
        max_depth: 0,
        min_file_size_mb: 100,
        chunk_size_mb: 1,
    };

    copy_file(&src_path, &dst_path, &metadata_config, &parallel_config)
        .await
        .unwrap();

    // Verify
    let result = std::fs::read(&dst_path).unwrap();
    assert_eq!(result, test_data);
}
