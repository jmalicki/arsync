use arsync::cli::ParallelCopyConfig;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub mod container_helpers;
pub mod copy_helpers;
pub mod test_args;

/// Helper to create a disabled parallel copy config for tests
#[allow(dead_code)]
pub fn disabled_parallel_config() -> ParallelCopyConfig {
    ParallelCopyConfig {
        max_depth: 0, // 0 = disabled
        min_file_size_mb: 128,
        chunk_size_mb: 2,
    }
}

#[allow(dead_code)]
pub struct TestTimeoutGuard {
    cancelled: Arc<AtomicBool>,
}

impl Drop for TestTimeoutGuard {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }
}

#[allow(dead_code)]
pub fn test_timeout_guard(duration: Duration) -> TestTimeoutGuard {
    let cancelled = Arc::new(AtomicBool::new(false));
    let cancelled_clone = Arc::clone(&cancelled);
    std::thread::spawn(move || {
        std::thread::sleep(duration);
        if !cancelled_clone.load(Ordering::SeqCst) {
            eprintln!("Test timeout exceeded ({}s). Aborting.", duration.as_secs());
            std::process::abort();
        }
    });
    TestTimeoutGuard { cancelled }
}
