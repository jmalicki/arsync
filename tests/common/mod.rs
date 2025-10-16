pub mod test_helpers;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub mod test_args;

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
