//! Statistics tracking for file synchronization operations
//!
//! This module provides lock-free atomic statistics tracking using `SharedStats`.
//! Statistics can be safely shared across async tasks without requiring mutexes.

use crate::directory::DirectoryStats;
use std::sync::atomic::{AtomicU64, Ordering};

/// Statistics tracking with interior mutability via atomics
///
/// This struct uses `AtomicU64` fields for lock-free statistics tracking.
/// The struct should be wrapped in `Arc<SharedStats>` when shared across tasks.
///
/// # Thread Safety
///
/// All methods are thread-safe and lock-free. Atomic operations use `Ordering::Relaxed`
/// since statistics counters don't require synchronization (eventual consistency is fine).
///
/// # Usage
///
/// ```rust,ignore
/// let stats = Arc::new(SharedStats::new(&DirectoryStats::default()));
/// stats.increment_files_copied();
/// stats.increment_bytes_copied(1024);
/// let final_stats = Arc::try_unwrap(stats).unwrap().into_inner();
/// ```
#[derive(Debug)]
pub struct SharedStats {
    /// Files copied counter using atomics for lock-free operations
    files_copied: AtomicU64,
    /// Directories created counter using atomics
    directories_created: AtomicU64,
    /// Bytes copied counter using atomics
    bytes_copied: AtomicU64,
    /// Symlinks processed counter using atomics
    symlinks_processed: AtomicU64,
    /// Errors counter using atomics
    errors: AtomicU64,
}

impl SharedStats {
    /// Create a new `SharedStats` with atomic counters
    ///
    /// # Arguments
    ///
    /// * `stats` - The initial directory statistics
    #[must_use]
    pub const fn new(stats: &DirectoryStats) -> Self {
        Self {
            files_copied: AtomicU64::new(stats.files_copied),
            directories_created: AtomicU64::new(stats.directories_created),
            bytes_copied: AtomicU64::new(stats.bytes_copied),
            symlinks_processed: AtomicU64::new(stats.symlinks_processed),
            errors: AtomicU64::new(stats.errors),
        }
    }

    #[allow(dead_code)]
    /// Get the number of files copied (lock-free atomic read)
    #[must_use]
    pub fn files_copied(&self) -> u64 {
        self.files_copied.load(Ordering::Relaxed)
    }

    #[allow(dead_code)]
    /// Get the number of directories created (lock-free atomic read)
    #[must_use]
    pub fn directories_created(&self) -> u64 {
        self.directories_created.load(Ordering::Relaxed)
    }

    #[allow(dead_code)]
    /// Get the number of bytes copied (lock-free atomic read)
    #[must_use]
    pub fn bytes_copied(&self) -> u64 {
        self.bytes_copied.load(Ordering::Relaxed)
    }

    #[allow(dead_code)]
    /// Get the number of symlinks processed (lock-free atomic read)
    #[must_use]
    pub fn symlinks_processed(&self) -> u64 {
        self.symlinks_processed.load(Ordering::Relaxed)
    }

    #[allow(dead_code)]
    /// Get the number of errors encountered (lock-free atomic read)
    #[must_use]
    pub fn errors(&self) -> u64 {
        self.errors.load(Ordering::Relaxed)
    }

    /// Increment the number of files copied (lock-free atomic operation)
    pub fn increment_files_copied(&self) {
        self.files_copied.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment the number of directories created (lock-free atomic operation)
    pub fn increment_directories_created(&self) {
        self.directories_created.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment the number of bytes copied by a given amount (lock-free atomic operation)
    ///
    /// # Arguments
    ///
    /// * `bytes` - The number of bytes to add to the counter
    pub fn increment_bytes_copied(&self, bytes: u64) {
        self.bytes_copied.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Increment the number of symlinks processed (lock-free atomic operation)
    pub fn increment_symlinks_processed(&self) {
        self.symlinks_processed.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment the error counter (lock-free atomic operation)
    pub fn increment_errors(&self) {
        self.errors.fetch_add(1, Ordering::Relaxed);
    }

    /// Convert atomic statistics back to `DirectoryStats`
    ///
    /// This consumes the `SharedStats` and returns a `DirectoryStats` with the final values.
    /// All atomic loads use `Ordering::Relaxed` since we're reading the final values.
    #[must_use]
    pub fn into_inner(self) -> DirectoryStats {
        DirectoryStats {
            files_copied: self.files_copied.load(Ordering::Relaxed),
            directories_created: self.directories_created.load(Ordering::Relaxed),
            bytes_copied: self.bytes_copied.load(Ordering::Relaxed),
            symlinks_processed: self.symlinks_processed.load(Ordering::Relaxed),
            errors: self.errors.load(Ordering::Relaxed),
        }
    }
}
