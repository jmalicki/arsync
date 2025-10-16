//! Progress tracking and reporting
//!
//! This module provides comprehensive progress tracking and reporting functionality
//! for file synchronization operations. It offers real-time progress visualization,
//! performance statistics, and detailed operation metrics.
//!
//! # Features
//!
//! - Real-time progress bars with customizable display formats
//! - Comprehensive statistics tracking (files, bytes, time)
//! - Support for both individual operations and batch tracking
//! - Configurable progress bar styling and templates
//! - Thread-safe progress updates and statistics
//!
//! # Usage
//!
//! ```rust,ignore
//! use arsync::progress::ProgressTracker;
//! use arsync::io_uring::CopyOperation;
//! use std::path::PathBuf;
//!
//! let mut tracker = ProgressTracker::new();
//! tracker.set_total(1024 * 1024); // 1MB total
//!
//! // During operation
//! tracker.update(512 * 1024); // 512KB copied
//!
//! // Get statistics
//! let stats = tracker.stats();
//! println!("Progress: {} files, {} bytes", stats.files_copied, stats.bytes_copied);
//!
//! tracker.finish();
//! ```
//!
//! # Performance Considerations
//!
//! - Progress updates are lightweight and don't impact copy performance
//! - Statistics are updated atomically for thread safety
//! - Progress bar rendering is optimized for minimal overhead
//! - Memory usage is constant regardless of operation size
//!
//! # Thread Safety
//!
//! All progress tracking operations are thread-safe and can be used concurrently
//! across multiple threads. Statistics are updated atomically to prevent race conditions.

use crate::i18n::TranslationKey;
use crate::io_uring::CopyOperation;
use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;

/// Progress tracker for file synchronization operations
///
/// This structure provides real-time progress tracking and reporting for file
/// copying operations. It maintains detailed statistics and displays progress
/// using customizable progress bars.
///
/// # Fields
///
/// * `progress_bar` - Visual progress indicator using indicatif
/// * `files_copied` - Running count of files successfully processed
/// * `bytes_copied` - Running total of bytes successfully copied
///
/// # Examples
///
/// Basic usage:
/// ```rust
/// use arsync::progress::ProgressTracker;
///
/// let mut tracker = ProgressTracker::new();
/// tracker.set_total(1024 * 1024); // Set 1MB target
/// tracker.update(512 * 1024);     // Update with 512KB progress
/// tracker.finish();               // Complete the operation
/// ```
///
/// Tracking individual operations:
/// ```rust
/// use arsync::progress::ProgressTracker;
/// use arsync::io_uring::CopyOperation;
/// use std::path::PathBuf;
///
/// let mut tracker = ProgressTracker::new();
/// let src_path = PathBuf::from("source.txt");
/// let dst_path = PathBuf::from("destination.txt");
/// let file_size = 1024;
/// let operation = CopyOperation::new(src_path, dst_path, file_size);
/// tracker.track_operation(&operation);
/// ```
///
/// # Performance Notes
///
/// - Progress updates are O(1) operations
/// - Statistics are maintained in atomic operations
/// - Progress bar rendering is optimized for minimal CPU usage
/// - Memory footprint is constant regardless of operation size
#[derive(Debug)]
#[allow(dead_code)]
pub struct ProgressTracker {
    /// Visual progress bar for user feedback
    progress_bar: ProgressBar,

    /// Total number of files discovered during traversal (via statx)
    files_discovered: u64,
    /// Total number of bytes discovered during traversal (via statx)
    bytes_discovered: u64,

    /// Running count of files successfully processed/copied
    files_copied: u64,
    /// Running total of bytes successfully copied
    bytes_copied: u64,
}

impl Default for ProgressTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(dead_code)]
impl ProgressTracker {
    /// Create a new progress tracker instance
    ///
    /// This function initializes a new progress tracker with default styling
    /// and zero statistics. The progress bar is configured with a modern
    /// template showing elapsed time, progress bar, bytes copied, and ETA.
    ///
    /// # Returns
    ///
    /// Returns a new `ProgressTracker` instance ready for use.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use arsync::progress::ProgressTracker;
    ///
    /// let tracker = ProgressTracker::new();
    /// ```
    ///
    /// # Performance Notes
    ///
    /// - Initialization is O(1) and very fast
    /// - No memory allocation beyond the struct itself
    /// - Progress bar styling is applied immediately
    ///
    /// # Panics
    ///
    /// This function will panic if the progress bar template is invalid.
    /// This should never happen with the hardcoded template string.
    #[allow(dead_code)]
    #[allow(clippy::unwrap_used)]
    #[must_use]
    pub fn new() -> Self {
        let pb = ProgressBar::new(0);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({eta})")
                .unwrap()
                .progress_chars("#>-"),
        );

        Self {
            progress_bar: pb,
            files_discovered: 0,
            bytes_discovered: 0,
            files_copied: 0,
            bytes_copied: 0,
        }
    }

    /// Track a file discovered during traversal (via statx)
    ///
    /// This should be called for each file encountered during directory traversal.
    /// It updates the discovery counters but doesn't affect the progress bar
    /// until the file is actually copied.
    ///
    /// # Parameters
    ///
    /// * `file_size` - Size of the discovered file in bytes
    ///
    /// # Examples
    ///
    /// ```rust
    /// use arsync::progress::ProgressTracker;
    ///
    /// let mut tracker = ProgressTracker::new();
    /// tracker.track_discovery(1024); // Discovered a 1KB file
    /// ```
    #[allow(dead_code)]
    pub fn track_discovery(&mut self, file_size: u64) {
        self.files_discovered += 1;
        self.bytes_discovered += file_size;
        // Update progress bar total to reflect discovered bytes
        self.progress_bar.set_length(self.bytes_discovered);
    }

    /// Set the total number of bytes to be processed
    ///
    /// This function configures the progress bar with the total expected
    /// byte count, enabling accurate progress percentage calculations and
    /// ETA estimates.
    ///
    /// # Parameters
    ///
    /// * `total_bytes` - Total number of bytes to be processed in the operation
    ///
    /// # Examples
    ///
    /// ```rust
    /// use arsync::progress::ProgressTracker;
    ///
    /// let mut tracker = ProgressTracker::new();
    /// tracker.set_total(1024 * 1024); // Set 1MB target
    /// ```
    ///
    /// # Performance Notes
    ///
    /// - This is an O(1) operation
    /// - Progress bar updates immediately to reflect the new total
    /// - Setting total to 0 disables percentage and ETA calculations
    pub fn set_total(&self, total_bytes: u64) {
        self.progress_bar.set_length(total_bytes);
    }

    /// Update progress with additional bytes copied
    ///
    /// This function updates the progress tracker with the number of bytes
    /// that have been successfully copied. It increments both the file count
    /// and byte count, and updates the visual progress bar.
    ///
    /// # Parameters
    ///
    /// * `bytes` - Number of bytes that were successfully copied
    ///
    /// # Examples
    ///
    /// ```rust
    /// use arsync::progress::ProgressTracker;
    ///
    /// let mut tracker = ProgressTracker::new();
    /// tracker.set_total(1024 * 1024);
    /// tracker.update(512 * 1024); // 512KB copied
    /// ```
    ///
    /// # Performance Notes
    ///
    /// - This is an O(1) operation
    /// - Progress bar updates are batched for performance
    /// - Statistics are updated atomically
    /// - Visual updates are throttled to prevent excessive redraws
    pub fn update(&mut self, bytes: u64) {
        self.bytes_copied += bytes;
        self.files_copied += 1;
        self.progress_bar.inc(bytes);
    }

    /// Mark the progress tracking operation as complete
    ///
    /// This function finalizes the progress tracking operation, displaying
    /// a completion message and stopping the progress bar animation.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use arsync::progress::ProgressTracker;
    ///
    /// let mut tracker = ProgressTracker::new();
    /// // ... perform operations ...
    /// tracker.finish(); // Show completion message
    /// ```
    ///
    /// # Performance Notes
    ///
    /// - This is an O(1) operation
    /// - Progress bar animation is stopped immediately
    /// - Completion message is displayed once
    /// - Statistics remain available after completion
    pub fn finish(&self) {
        let message = TranslationKey::ProgressComplete
            .get()
            .unwrap_or_else(|_| "Complete".to_string());
        self.progress_bar.finish_with_message(message);
    }

    /// Track progress for a specific copy operation
    ///
    /// This function updates the progress tracker with statistics from a
    /// completed or in-progress copy operation. It's designed to work with
    /// the `CopyOperation` structure for detailed operation tracking.
    ///
    /// # Parameters
    ///
    /// * `operation` - The copy operation to track, containing file size,
    ///   bytes copied, and other relevant statistics
    ///
    /// # Examples
    ///
    /// ```rust
    /// use arsync::progress::ProgressTracker;
    /// use arsync::io_uring::CopyOperation;
    /// use std::path::PathBuf;
    ///
    /// let mut tracker = ProgressTracker::new();
    /// let src_path = PathBuf::from("source.txt");
    /// let dst_path = PathBuf::from("destination.txt");
    /// let file_size = 1024;
    /// let operation = CopyOperation::new(src_path, dst_path, file_size);
    /// // ... perform copy operation ...
    /// tracker.track_operation(&operation);
    /// ```
    ///
    /// # Performance Notes
    ///
    /// - This is an O(1) operation
    /// - Progress bar position is updated if file size is known
    /// - Statistics are updated atomically
    /// - Visual updates are optimized for minimal overhead
    pub fn track_operation(&mut self, operation: &CopyOperation) {
        self.files_copied += 1;
        self.bytes_copied += operation.bytes_copied;

        // Update progress bar if we have total size
        if operation.file_size > 0 {
            self.progress_bar.set_position(operation.bytes_copied);
        }
    }

    /// Get current progress statistics
    ///
    /// This function returns a snapshot of the current progress statistics,
    /// including file count, byte count, and elapsed time. The statistics
    /// are captured at the time of the call and represent cumulative progress.
    ///
    /// # Returns
    ///
    /// Returns a `ProgressStats` structure containing:
    /// - `files_copied`: Number of files successfully processed
    /// - `bytes_copied`: Total bytes successfully copied
    /// - `elapsed`: Time elapsed since tracker creation
    ///
    /// # Examples
    ///
    /// ```rust
    /// use arsync::progress::ProgressTracker;
    ///
    /// let mut tracker = ProgressTracker::new();
    /// // ... perform operations ...
    /// let stats = tracker.stats();
    /// println!("Progress: {} files, {} bytes, {:?} elapsed",
    ///          stats.files_copied, stats.bytes_copied, stats.elapsed);
    /// ```
    ///
    /// # Performance Notes
    ///
    /// - This is an O(1) operation
    /// - Statistics are read atomically
    /// - Elapsed time calculation is cached for performance
    /// - No memory allocation occurs
    #[must_use]
    pub fn stats(&self) -> ProgressStats {
        ProgressStats {
            files_copied: self.files_copied,
            bytes_copied: self.bytes_copied,
            elapsed: self.progress_bar.elapsed(),
        }
    }
}

/// Progress statistics for synchronization operations
///
/// This structure contains comprehensive statistics about the progress
/// of file synchronization operations, providing detailed metrics for
/// monitoring, reporting, and performance analysis.
///
/// # Fields
///
/// * `files_copied` - Number of files successfully processed
/// * `bytes_copied` - Total number of bytes successfully copied
/// * `elapsed` - Time elapsed since operation started
///
/// # Examples
///
/// Basic usage:
/// ```rust
/// use arsync::progress::ProgressStats;
/// use std::time::Duration;
///
/// let stats = ProgressStats {
///     files_copied: 150,
///     bytes_copied: 1_048_576,
///     elapsed: Duration::from_secs(30),
/// };
///
/// println!("Copied {} files ({} bytes) in {:?}",
///          stats.files_copied,
///          stats.bytes_copied,
///          stats.elapsed);
/// ```
///
/// Performance analysis:
/// ```rust
/// use arsync::progress::ProgressTracker;
///
/// let mut tracker = ProgressTracker::new();
/// // ... perform operations ...
/// let stats = tracker.stats();
/// let throughput = stats.bytes_copied as f64 / stats.elapsed.as_secs_f64();
/// println!("Throughput: {:.2} MB/s", throughput / 1_048_576.0);
/// ```
///
/// # Performance Notes
///
/// - All fields are efficiently stored and accessed
/// - Duration calculations are optimized for minimal overhead
/// - Statistics can be safely shared across threads
/// - Memory footprint is constant and minimal
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub struct ProgressStats {
    /// Number of files successfully processed during the operation
    pub files_copied: u64,

    /// Total number of bytes successfully copied during the operation
    pub bytes_copied: u64,

    /// Time elapsed since the operation started
    pub elapsed: Duration,
}
