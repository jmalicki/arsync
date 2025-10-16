//! Tests for lock-free atomic statistics tracking
//!
//! This module tests that the atomic statistics counters work correctly
//! under concurrent access without requiring locks.

use arsync::directory::{DirectoryStats, SharedStats};
use std::sync::Arc;

/// Test basic atomic stats increment
#[compio::test]
async fn test_atomic_stats_single_thread() {
    let initial_stats = DirectoryStats::default();
    let stats = Arc::new(SharedStats::new(&initial_stats));

    // Increment counters (infallible operations)
    stats.increment_files_copied();
    stats.increment_files_copied();
    stats.increment_directories_created();
    stats.increment_bytes_copied(1024);
    stats.increment_bytes_copied(2048);
    stats.increment_symlinks_processed();
    stats.increment_errors();

    // Verify counts
    let final_stats = Arc::try_unwrap(stats).unwrap().into_inner();
    assert_eq!(final_stats.files_copied, 2);
    assert_eq!(final_stats.directories_created, 1);
    assert_eq!(final_stats.bytes_copied, 3072);
    assert_eq!(final_stats.symlinks_processed, 1);
    assert_eq!(final_stats.errors, 1);
}

/// Test concurrent atomic stats updates from multiple tasks
#[compio::test]
async fn test_atomic_stats_concurrent() {
    let initial_stats = DirectoryStats::default();
    let stats = Arc::new(SharedStats::new(&initial_stats));

    // Spawn 100 concurrent tasks, each incrementing counters 10 times
    let mut handles = Vec::new();
    for _ in 0..100 {
        let stats_clone = Arc::clone(&stats);
        let handle = compio::runtime::spawn(async move {
            for _ in 0..10 {
                stats_clone.increment_files_copied();
                stats_clone.increment_bytes_copied(100);
            }
        });
        handles.push(handle);
    }

    // Wait for all tasks to complete
    for handle in handles {
        handle.await.unwrap();
    }

    // Verify final counts (should be 100 tasks * 10 increments = 1000)
    let final_stats = Arc::try_unwrap(stats).unwrap().into_inner();
    assert_eq!(final_stats.files_copied, 1000, "Files copied mismatch");
    assert_eq!(
        final_stats.bytes_copied, 100_000,
        "Bytes copied mismatch (100 * 1000)"
    );
}

/// Test that atomic operations don't lose updates under heavy contention
#[compio::test]
async fn test_atomic_stats_no_lost_updates() {
    let initial_stats = DirectoryStats::default();
    let stats = Arc::new(SharedStats::new(&initial_stats));

    // Create high contention: 1000 concurrent tasks
    let mut handles = Vec::new();
    for _ in 0..1000 {
        let stats_clone = Arc::clone(&stats);
        let handle = compio::runtime::spawn(async move {
            stats_clone.increment_files_copied();
            stats_clone.increment_directories_created();
            stats_clone.increment_bytes_copied(1);
            stats_clone.increment_symlinks_processed();
            stats_clone.increment_errors();
        });
        handles.push(handle);
    }

    // Wait for all tasks
    for handle in handles {
        handle.await.unwrap();
    }

    // Verify NO updates were lost (atomic operations are guaranteed)
    let final_stats = Arc::try_unwrap(stats).unwrap().into_inner();
    assert_eq!(final_stats.files_copied, 1000);
    assert_eq!(final_stats.directories_created, 1000);
    assert_eq!(final_stats.bytes_copied, 1000);
    assert_eq!(final_stats.symlinks_processed, 1000);
    assert_eq!(final_stats.errors, 1000);
}

/// Test that getter methods return current values during concurrent updates
#[compio::test]
async fn test_atomic_stats_concurrent_reads() {
    let initial_stats = DirectoryStats::default();
    let stats = Arc::new(SharedStats::new(&initial_stats));

    // Spawn concurrent tasks that increment and read simultaneously
    let mut handles = Vec::new();

    // Writers
    for _ in 0..50 {
        let stats_clone = Arc::clone(&stats);
        let handle = compio::runtime::spawn(async move {
            for _ in 0..20 {
                stats_clone.increment_files_copied();
            }
        });
        handles.push(handle);
    }

    // Readers (verify monotonic increase)
    for _ in 0..10 {
        let stats_clone = Arc::clone(&stats);
        let handle = compio::runtime::spawn(async move {
            let mut last_value = 0;
            for _ in 0..50 {
                let current = stats_clone.files_copied();
                // Value should never decrease
                assert!(
                    current >= last_value,
                    "Stats went backwards: {current} < {last_value}"
                );
                last_value = current;
            }
        });
        handles.push(handle);
    }

    // Wait for all tasks
    for handle in handles {
        handle.await.unwrap();
    }

    // Final value should be 50 writers * 20 increments = 1000
    let final_stats = Arc::try_unwrap(stats).unwrap().into_inner();
    assert_eq!(final_stats.files_copied, 1000);
}

/// Test infallible operations: increment methods don't return Result
#[compio::test]
async fn test_atomic_stats_infallible_operations() {
    let initial_stats = DirectoryStats::default();
    let stats = Arc::new(SharedStats::new(&initial_stats));

    // All increment operations are infallible (no Result to check)
    stats.increment_files_copied();
    stats.increment_directories_created();
    stats.increment_bytes_copied(100);
    stats.increment_symlinks_processed();
    stats.increment_errors();

    // Verify the operations succeeded
    let final_stats = Arc::try_unwrap(stats).unwrap().into_inner();
    assert_eq!(final_stats.files_copied, 1);
    assert_eq!(final_stats.directories_created, 1);
    assert_eq!(final_stats.bytes_copied, 100);
    assert_eq!(final_stats.symlinks_processed, 1);
    assert_eq!(final_stats.errors, 1);
}
