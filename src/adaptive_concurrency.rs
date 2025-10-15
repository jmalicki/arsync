//! Adaptive concurrency control with file descriptor awareness
//!
//! This module provides self-adaptive concurrency control that automatically
//! adjusts the number of concurrent operations based on resource availability,
//! particularly file descriptor exhaustion.
//!
//! # Architecture
//!
//! Each module owns its configuration:
//! - `ConcurrencyOptions` - Configuration for concurrency control (owned by this module)
//! - `AdaptiveConcurrencyController` - Runtime controller that uses the options

use crate::directory::SharedSemaphore;
use crate::error::SyncError;
use std::io::ErrorKind;
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use tracing::warn;

// ============================================================================
// CONFIGURATION
// ============================================================================

/// Concurrency control configuration options
///
/// This struct contains all configuration for the adaptive concurrency controller.
/// It's owned by this module and can be created from CLI args.
///
/// Uses `NonZeroUsize` to guarantee at compile-time that max_files_in_flight >= 1.
#[derive(Debug, Clone)]
pub struct ConcurrencyOptions {
    /// Maximum number of concurrent operations (guaranteed >= 1)
    max_files_in_flight: NonZeroUsize,
    /// Minimum permits to maintain when adapting (never go below this, guaranteed >= 1)
    min_permits: NonZeroUsize,
    /// If true, fail immediately on resource exhaustion; if false, adapt automatically
    fail_on_exhaustion: bool,
}

impl ConcurrencyOptions {
    /// Create new concurrency options with validation
    ///
    /// # Arguments
    ///
    /// * `max_files_in_flight` - Maximum concurrent operations (will be clamped to >= 1)
    /// * `fail_on_exhaustion` - If true, fail on EMFILE; if false, adapt
    ///
    /// # Panics
    ///
    /// This function will panic if `max_files_in_flight` is 0 in debug builds.
    /// In release builds, it will be clamped to 1.
    #[must_use]
    pub fn new(max_files_in_flight: usize, fail_on_exhaustion: bool) -> Self {
        // Ensure max_files_in_flight is at least 1
        let max_files_in_flight = NonZeroUsize::new(max_files_in_flight)
            .unwrap_or_else(|| {
                debug_assert!(false, "max_files_in_flight must be >= 1");
                // SAFETY: 1 is non-zero
                unsafe { NonZeroUsize::new_unchecked(1) }
            });

        // Compute minimum permits: never go below 10 or 10% of max
        let min_value = std::cmp::max(10, max_files_in_flight.get() / 10);
        // SAFETY: min_value is at least 10, which is non-zero
        let min_permits = unsafe { NonZeroUsize::new_unchecked(min_value) };

        Self {
            max_files_in_flight,
            min_permits,
            fail_on_exhaustion,
        }
    }

    /// Get the maximum files in flight
    #[must_use]
    pub const fn max_files_in_flight(&self) -> usize {
        self.max_files_in_flight.get()
    }

    /// Get the minimum permits (floor for adaptive reduction)
    #[must_use]
    pub const fn min_permits(&self) -> usize {
        self.min_permits.get()
    }

    /// Check if should fail on exhaustion
    #[must_use]
    pub const fn fail_on_exhaustion(&self) -> bool {
        self.fail_on_exhaustion
    }
}

// ============================================================================
// CONTROLLER
// ============================================================================

/// Adaptive concurrency controller that responds to resource constraints
///
/// This wraps a semaphore and automatically reduces concurrency when
/// file descriptor exhaustion (EMFILE) is detected, then gradually
/// increases it again when resources are available.
#[derive(Clone)]
pub struct AdaptiveConcurrencyController {
    /// The underlying semaphore
    semaphore: SharedSemaphore,
    /// Counter for EMFILE errors
    emfile_errors: Arc<AtomicUsize>,
    /// Flag indicating if we've already warned about FD exhaustion
    emfile_warned: Arc<AtomicBool>,
    /// Minimum permits to maintain
    min_permits: usize,
    /// Whether to fail hard on exhaustion (true = fail, false = adapt)
    fail_on_exhaustion: bool,
}

impl AdaptiveConcurrencyController {
    /// Create a new adaptive controller from options
    ///
    /// # Arguments
    ///
    /// * `options` - Concurrency configuration options (validated and ready to use)
    #[must_use]
    pub fn new(options: &ConcurrencyOptions) -> Self {
        Self {
            semaphore: SharedSemaphore::new(options.max_files_in_flight()),
            emfile_errors: Arc::new(AtomicUsize::new(0)),
            emfile_warned: Arc::new(AtomicBool::new(false)),
            min_permits: options.min_permits(),
            fail_on_exhaustion: options.fail_on_exhaustion(),
        }
    }

    /// Acquire a permit
    pub async fn acquire(&self) -> compio_sync::SemaphorePermit {
        self.semaphore.acquire().await
    }

    /// Handle an error, checking if it's EMFILE and adapting or failing as configured
    ///
    /// If this is an EMFILE error:
    /// - If `fail_on_exhaustion` is true: Returns error immediately
    /// - If `fail_on_exhaustion` is false: Adapts concurrency and returns Ok
    /// - If not an EMFILE error: Returns Ok
    ///
    /// # Errors
    ///
    /// Returns FdExhaustion error if EMFILE detected and fail_on_exhaustion is true
    pub fn handle_error(&self, error: &SyncError) -> crate::error::Result<()> {
        use crate::error::SyncError;
        
        // Check if this is a file descriptor exhaustion error
        if Self::is_emfile_error(error) {
            let count = self.emfile_errors.fetch_add(1, Ordering::Relaxed) + 1;

            // Check if we should fail hard
            if self.fail_on_exhaustion {
                return Err(SyncError::FdExhaustion(format!(
                    "File descriptor exhaustion detected (--no-adaptive-concurrency is set). \
                     Error: {}. \
                     Either increase ulimit or remove --no-adaptive-concurrency flag.",
                    error
                )));
            }

            // Otherwise, adapt every N errors to avoid over-reaction
            if count % 5 == 1 {
                self.adapt_to_fd_exhaustion();
            }
        }
        Ok(())
    }

    /// Check if an error is EMFILE (too many open files)
    fn is_emfile_error(error: &SyncError) -> bool {
        let error_str = format!("{error:?}");
        error_str.contains("Too many open files")
            || error_str.contains("EMFILE")
            || error_str.contains("os error 24")
    }

    /// Adapt to file descriptor exhaustion by reducing concurrency
    fn adapt_to_fd_exhaustion(&self) {
        let current_available = self.semaphore.available_permits();
        let current_max = self.semaphore.max_permits();

        // Reduce by 25% or minimum 10, but never go below min_permits
        let reduction = std::cmp::max(10, current_max / 4);
        let actual_reduced = self.semaphore.reduce_permits(reduction);

        let new_max = current_max - actual_reduced;

        if actual_reduced > 0 {
            if self.emfile_warned.swap(true, Ordering::Relaxed) {
                // Subsequent reductions - be brief
                warn!(
                    "Reducing concurrency further due to FD exhaustion: {} → {} (-{})",
                    current_max, new_max, actual_reduced
                );
            } else {
                // First time warning - be verbose
                warn!(
                    "⚠️  FILE DESCRIPTOR EXHAUSTION DETECTED (EMFILE)\n\
                     \n\
                     arsync has hit the system file descriptor limit.\n\
                     \n\
                     Self-adaptive response:\n\
                     - Reduced concurrent operations: {} → {} (-{})\n\
                     - Currently available: {}\n\
                     - Minimum limit: {}\n\
                     \n\
                     This may slow down processing but prevents crashes.\n\
                     \n\
                     To avoid this:\n\
                     - Increase ulimit: ulimit -n 100000\n\
                     - Or use --max-files-in-flight to set lower initial concurrency\n\
                     \n\
                     Continuing with reduced concurrency...",
                    current_max, new_max, actual_reduced, current_available, self.min_permits
                );
            }
        }
    }

    /// Get current statistics
    #[must_use]
    #[allow(dead_code)] // Public API for future monitoring/metrics
    pub fn stats(&self) -> ConcurrencyStats {
        ConcurrencyStats {
            max_permits: self.semaphore.max_permits(),
            available_permits: self.semaphore.available_permits(),
            in_use: self.semaphore.max_permits() - self.semaphore.available_permits(),
            emfile_errors: self.emfile_errors.load(Ordering::Relaxed),
        }
    }
}

/// Statistics about concurrency control
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)] // Public API for future monitoring/metrics
pub struct ConcurrencyStats {
    /// Maximum permits configured
    pub max_permits: usize,
    /// Currently available permits
    pub available_permits: usize,
    /// Permits currently in use
    pub in_use: usize,
    /// Number of EMFILE errors encountered
    pub emfile_errors: usize,
}

/// Check system file descriptor limits and warn if too low
///
/// Returns the soft limit for file descriptors
///
/// # Errors
///
/// Returns an error if getrlimit system call fails
pub fn check_fd_limits() -> std::io::Result<u64> {
    use libc::{getrlimit, rlimit, RLIMIT_NOFILE};

    unsafe {
        let mut limit = rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };

        if getrlimit(RLIMIT_NOFILE, &raw mut limit) == 0 {
            let soft_limit = limit.rlim_cur;

            // Warn if limit seems low for high-performance operations
            if soft_limit < 10000 {
                warn!(
                    "⚠️  File descriptor limit is low: {}\n\
                     \n\
                     For optimal performance with arsync, consider:\n\
                     ulimit -n 100000\n\
                     \n\
                     Current limit may cause FD exhaustion on large operations.\n\
                     arsync will adapt automatically if this occurs.",
                    soft_limit
                );
            } else {
                tracing::info!("File descriptor limit: {} (adequate)", soft_limit);
            }

            Ok(soft_limit)
        } else {
            Err(std::io::Error::last_os_error())
        }
    }
}

/// Detect if an I/O error is EMFILE (file descriptor exhaustion)
#[must_use]
#[allow(dead_code)] // Public API for future use in copy.rs
pub fn is_emfile_error(error: &std::io::Error) -> bool {
    error.kind() == ErrorKind::Other && error.raw_os_error() == Some(libc::EMFILE)
}
