//! Hardlink tracking for race-free hardlink synchronization
//!
//! This module provides `FilesystemTracker` which uses condition variables to ensure
//! that hardlinked files are handled correctly in concurrent scenarios:
//! - The first task to encounter a hardlinked inode becomes the "copier"
//! - Subsequent tasks become "linkers" that wait for the copier to finish
//! - The copier signals completion, allowing linkers to create hardlinks

use dashmap::DashMap;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tracing::debug;

/// Inode information used as a key for hardlink tracking
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct InodeInfo {
    /// Device ID
    pub dev: u64,
    /// Inode number
    pub ino: u64,
}

/// Hardlink tracking information with concurrent synchronization
///
/// Uses `Condvar` to ensure race-free hardlink creation:
/// - First task to register becomes the "copier" (stores `dst_path`, returns immediately)
/// - Subsequent tasks are "linkers" (wait on condvar inside `register_file`, then return)
/// - Copier signals condvar when copy completes (success or failure)
/// - Linkers wake up and naturally succeed/fail based on whether dst file exists
pub struct HardlinkInfo {
    /// Original file path (immutable after creation)
    #[allow(dead_code)]
    pub original_path: std::path::PathBuf,
    /// Inode number (immutable after creation)
    pub inode_number: u64,
    /// Number of hardlinks found (incremented atomically)
    pub link_count: AtomicU64,
    /// Destination path (set at registration time by first copier)
    pub dst_path: std::path::PathBuf,
    /// Condition variable signaled when copy completes
    /// Linker tasks wait on this (inside `register_file`) before returning
    /// Public for testing synchronization behavior
    pub copy_complete: Arc<compio_sync::Condvar>,
}

impl std::fmt::Debug for HardlinkInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HardlinkInfo")
            .field("original_path", &self.original_path)
            .field("inode_number", &self.inode_number)
            .field("link_count", &self.link_count.load(Ordering::Relaxed))
            .field("dst_path", &self.dst_path)
            .finish_non_exhaustive() // Omitting copy_complete condvar
    }
}

/// Filesystem boundary and hardlink tracker
#[derive(Debug, Default)]
pub struct FilesystemTracker {
    /// Tracked hardlinks by inode
    pub hardlinks: DashMap<InodeInfo, HardlinkInfo>,
    /// Source filesystem device ID (set during scan to avoid cross-filesystem hardlinks)
    source_filesystem: std::sync::RwLock<Option<u64>>,
}

/// Statistics about hardlink tracking
#[derive(Debug, Clone, Default)]
pub struct FilesystemStats {
    /// Total number of tracked inodes
    pub total_files: usize,
    /// Number of inodes with multiple hardlinks
    pub hardlink_groups: usize,
    /// Total number of hardlink references
    pub total_hardlinks: u64,
}

#[allow(dead_code)]
impl FilesystemTracker {
    /// Create a new filesystem tracker
    #[must_use]
    pub fn new() -> Self {
        Self {
            hardlinks: DashMap::new(),
            source_filesystem: std::sync::RwLock::new(None),
        }
    }

    /// Create a tracker with a known source filesystem
    #[must_use]
    pub fn with_source_filesystem(dev: u64) -> Self {
        Self {
            hardlinks: DashMap::new(),
            source_filesystem: std::sync::RwLock::new(Some(dev)),
        }
    }

    /// Set the source filesystem device ID
    ///
    /// This should be called once at the beginning of a copy operation
    /// to establish the source filesystem boundary.
    pub fn set_source_filesystem(&self, dev: u64) {
        if let Ok(mut source) = self.source_filesystem.write() {
            *source = Some(dev);
            debug!("Set source filesystem device ID: {}", dev);
        }
    }

    /// Check if a device is on the source filesystem
    ///
    /// Returns `true` if the device matches the source filesystem or if no source is set.
    #[must_use]
    pub fn is_on_source_filesystem(&self, dev: u64) -> bool {
        self.source_filesystem
            .read()
            .ok()
            .is_none_or(|source_dev| source_dev.is_none_or(|source| source == dev))
    }

    /// Check if a device is on the same filesystem as the source
    ///
    /// Alias for `is_on_source_filesystem` for compatibility.
    #[must_use]
    pub fn is_same_filesystem(&self, dev: u64) -> bool {
        self.is_on_source_filesystem(dev)
    }

    /// Check if a path is on the source filesystem
    pub fn is_path_on_source_filesystem(&self, path: &Path) -> bool {
        std::fs::metadata(path)
            .ok()
            .is_some_and(|m| self.is_on_source_filesystem(m.dev()))
    }

    /// Check if inode is on source filesystem using a predicate
    ///
    /// This is a more flexible version that allows custom device extraction.
    pub fn check_source_filesystem<F>(&self, dev: u64, predicate: F) -> bool
    where
        F: FnOnce(Option<u64>) -> bool,
    {
        self.source_filesystem
            .read()
            .ok()
            .is_none_or(|source_dev| match *source_dev {
                Some(source) if source != dev => false,
                _ => predicate(*source_dev),
            })
    }

    /// Register a file for hardlink tracking (async - waits if linker)
    ///
    /// This atomically determines if this task is the "copier" or a "linker":
    /// - **Copier**: First to register, returns `true` immediately
    /// - **Linker**: Already registered, WAITS on condvar until copier signals, returns `false`
    /// - **Regular file**: `link_count == 1`, returns `false` immediately
    ///
    /// The waiting is encapsulated inside this method, so caller doesn't need to manage condvars.
    ///
    /// # Arguments
    /// * `src_path` - Source file path
    /// * `dst_path` - Destination path (stored in `HardlinkInfo` for linkers to use)
    /// * `dev` - Device ID  
    /// * `ino` - Inode number
    /// * `link_count` - Number of hardlinks (from stat)
    ///
    /// # Returns
    /// * `true` - Caller is copier, should copy file then call `signal_copy_complete()`
    /// * `false` - Caller is linker (already waited) or regular file, should create hardlink/copy
    #[allow(clippy::too_many_arguments)]
    pub async fn register_file(
        &self,
        src_path: &Path,
        dst_path: &Path,
        dev: u64,
        ino: u64,
        link_count: u64,
    ) -> bool {
        // Skip files with link count of 1 - they're not hardlinks
        if link_count == 1 {
            return false; // Not a hardlink, caller should copy normally
        }

        let inode_info = InodeInfo { dev, ino };

        // ATOMIC PATTERN: Try to insert, get back whether we won the race
        match self.hardlinks.entry(inode_info) {
            dashmap::mapref::entry::Entry::Occupied(entry) => {
                // This inode is already registered - we're a linker
                let hardlink_info = entry.get();

                // Increment link count
                let count = hardlink_info.link_count.fetch_add(1, Ordering::Relaxed);

                // Get condvar before releasing DashMap ref
                let condvar = Arc::clone(&hardlink_info.copy_complete);

                // Release DashMap ref
                let _ = hardlink_info;
                drop(entry);

                debug!(
                    "Found hardlink #{} for inode ({}, {}): {} (linker - waiting for copy)",
                    count + 1,
                    dev,
                    ino,
                    src_path.display()
                );

                // WAIT for copier to signal completion
                condvar.wait().await;

                debug!(
                    "Linker woke up for inode ({}, {}): {}",
                    dev,
                    ino,
                    src_path.display()
                );

                false // We're a linker, waiting is done
            }
            dashmap::mapref::entry::Entry::Vacant(entry) => {
                // We're the first - we're the copier
                entry.insert(HardlinkInfo {
                    original_path: src_path.to_path_buf(),
                    inode_number: ino,
                    link_count: AtomicU64::new(1),
                    dst_path: dst_path.to_path_buf(),
                    copy_complete: Arc::new(compio_sync::Condvar::new()),
                });
                debug!(
                    "Registered new hardlink inode ({}, {}): {} → {} (copier)",
                    dev,
                    ino,
                    src_path.display(),
                    dst_path.display()
                );
                true // We're the copier
            }
        }
    }

    /// Get all hardlink groups that have multiple links
    ///
    /// Returns a vector of (`inode_number`, `link_count`) tuples for inodes with multiple hardlinks.
    #[must_use]
    pub fn get_hardlink_groups(&self) -> Vec<(u64, u64)> {
        self.hardlinks
            .iter()
            .filter_map(|entry| {
                let val = entry.value();
                let count = val.link_count.load(Ordering::Relaxed);
                if count > 1 {
                    Some((val.inode_number, count))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Signal that an inode's copy is complete
    ///
    /// This should be called by the copier task after attempting to copy (success or failure).
    /// It wakes all waiting linker tasks. Linkers will naturally succeed/fail based on
    /// whether the destination file exists.
    ///
    /// # Arguments
    /// * `ino` - Inode number that was copied
    pub fn signal_copy_complete(&self, ino: u64) {
        // Find the condvar and signal it (move temporary above if-let to avoid significant drop)
        let found = self
            .hardlinks
            .iter()
            .find(|e| e.value().inode_number == ino);
        if let Some(entry) = found {
            let condvar = Arc::clone(&entry.value().copy_complete);
            drop(entry); // Release DashMap ref before signaling

            condvar.notify_all();
            debug!("Signaled copy complete for inode {}", ino);
        }
    }

    /// Get the destination path for an inode
    ///
    /// Returns the destination path that was set during registration.
    /// For linkers, this should only be called after `register_file()` returns (waiting is done).
    #[must_use]
    pub fn get_dst_path_for_inode(&self, ino: u64) -> Option<PathBuf> {
        self.hardlinks
            .iter()
            .find(|entry| entry.value().inode_number == ino)
            .map(|entry| entry.value().dst_path.clone())
    }

    /// Get statistics about the filesystem tracking
    #[must_use]
    pub fn get_stats(&self) -> FilesystemStats {
        let total_files = self.hardlinks.len();
        let mut hardlink_groups = 0;
        let mut total_hardlinks = 0;

        for entry in &self.hardlinks {
            let count = entry.value().link_count.load(Ordering::Relaxed);
            if count > 1 {
                hardlink_groups += 1;
            }
            total_hardlinks += count;
        }

        FilesystemStats {
            total_files,
            hardlink_groups,
            total_hardlinks,
        }
    }

    /// Check if an inode has been copied
    #[must_use]
    pub fn is_inode_copied(&self, dev: u64, ino: u64) -> bool {
        self.hardlinks.get(&InodeInfo { dev, ino }).is_some()
    }

    /// Get the original path for an inode if it's been registered
    #[must_use]
    pub fn get_original_path_for_inode(&self, dev: u64, ino: u64) -> Option<PathBuf> {
        self.hardlinks
            .get(&InodeInfo { dev, ino })
            .map(|entry| entry.original_path.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Test basic filesystem tracker functionality
    #[compio::test]
    async fn test_filesystem_tracker_basic() {
        let tracker = FilesystemTracker::new();
        let stats = tracker.get_stats();
        assert_eq!(stats.total_files, 0);
        assert_eq!(stats.hardlink_groups, 0);
        assert_eq!(stats.total_hardlinks, 0);
    }

    /// Test hardlink registration and tracking
    #[compio::test]
    async fn test_filesystem_tracker_hardlinks() {
        let tracker = FilesystemTracker::new();
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let file1 = temp_dir.path().join("file1.txt");
        let file2 = temp_dir.path().join("file2.txt");

        // Create a file
        std::fs::write(&file1, "content").expect("Failed to write file");

        // Create hardlink
        std::fs::hard_link(&file1, &file2).expect("Failed to create hardlink");

        // Register first file (should be copier)
        let dst1 = temp_dir.path().join("dst1.txt");
        let is_copier = tracker.register_file(&file1, &dst1, 1, 100, 2).await;
        assert!(is_copier); // Should be copier (first registration)

        // For linker test, we'd need to spawn async task to avoid blocking this test
        // The synchronization test covers that - this test just verifies tracker state

        // Check stats
        let stats = tracker.get_stats();
        assert_eq!(stats.total_files, 1);
        assert_eq!(stats.hardlink_groups, 0); // Only 1 link registered so far
        assert_eq!(stats.total_hardlinks, 1);
    }

    /// SYNCHRONIZATION: Test that linker waits for copier using condvar coordination
    #[compio::test]
    async fn test_linker_waits_for_copier_signal() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let file1 = temp_dir.path().join("file1.txt");
        let file2 = temp_dir.path().join("file2.txt");
        let dst_dir = temp_dir.path().join("dst");
        std::fs::create_dir(&dst_dir).expect("Failed to create dst dir");

        std::fs::write(&file1, "content").expect("Failed to write file");
        std::fs::hard_link(&file1, &file2).expect("Failed to create hardlink");

        let meta = std::fs::metadata(&file1).expect("Failed to get metadata");
        let tracker = Arc::new(FilesystemTracker::new());

        let dst_file1 = dst_dir.join("file1.txt");
        let dst_file2 = dst_dir.join("file2.txt");

        // Register copier (returns immediately)
        let is_copier = tracker
            .register_file(&file1, &dst_file1, 1, meta.ino(), 2)
            .await;
        assert!(is_copier, "First registration should be copier");

        // Get inode and condvar before spawning task
        let inode = meta.ino();
        let hardlink_info = tracker
            .hardlinks
            .get(&InodeInfo { dev: 1, ino: inode })
            .expect("Should have entry");
        let copier_cv = Arc::clone(&hardlink_info.copy_complete);
        drop(hardlink_info);

        let tracker_clone = Arc::clone(&tracker);
        let file2_clone = file2.clone();
        let dst_file2_clone = dst_file2.clone();

        // Use a coordination condvar to ensure linker has time to start waiting
        let coord_cv = Arc::new(compio_sync::Condvar::new());
        let coord_cv_clone = Arc::clone(&coord_cv);

        // Spawn a coordinator task that gives the linker time to execute
        let coord_handle = compio::runtime::spawn(async move {
            // Yield control to allow linker to start
            for _ in 0..10 {
                compio::runtime::spawn(async {}).await.ok();
            }
            // Signal that we've given time for linker to enter wait queue
            coord_cv_clone.notify_all();
        });

        // Spawn linker task
        let linker_handle = compio::runtime::spawn(async move {
            // This will block inside register_file on condvar.wait()
            let is_linker = tracker_clone
                .register_file(&file2_clone, &dst_file2_clone, 1, inode, 2)
                .await;
            assert!(!is_linker, "Should be linker");
            "linker woke up"
        });

        // Wait for coordinator to signal (gives linker time to enter queue)
        coord_cv.wait().await;
        coord_handle.await.ok();

        // CRITICAL: Verify linker is in the waiter queue
        assert_eq!(
            copier_cv.as_ref().waiter_count(),
            1,
            "Linker should be in waiter queue before copier signals"
        );

        // Signal copier completion
        tracker.signal_copy_complete(inode);

        // Verify waiter was removed from queue
        std::thread::sleep(std::time::Duration::from_millis(1));
        assert_eq!(
            copier_cv.as_ref().waiter_count(),
            0,
            "Waiter should be removed after notify"
        );

        // Linker should wake up and complete
        let result = linker_handle.await.expect("Linker task should complete");
        assert_eq!(result, "linker woke up", "Linker should wake after signal");
    }

    /// SYNCHRONIZATION: Test that dst_path is set at registration time
    #[compio::test]
    async fn test_dst_path_set_at_registration() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let src_file = temp_dir.path().join("src.txt");
        let dst_file = temp_dir.path().join("dst.txt");

        std::fs::write(&src_file, "content").expect("Failed to write file");

        let meta = std::fs::metadata(&src_file).expect("Failed to get metadata");
        let tracker = Arc::new(FilesystemTracker::new());

        // Register file with dst_path
        let is_copier = tracker
            .register_file(&src_file, &dst_file, meta.dev(), meta.ino(), 2)
            .await;

        assert!(is_copier, "First registration should be copier");

        // CRITICAL: dst_path should be available IMMEDIATELY after registration
        // Should NOT require waiting for copy to complete or signal_copy_complete
        let retrieved_dst = tracker.get_dst_path_for_inode(meta.ino());
        assert_eq!(
            retrieved_dst,
            Some(dst_file.clone()),
            "dst_path should be set at registration, not after copy"
        );
    }
}
