//! Lock-based waiter queue for async synchronization primitives
//!
//! This module provides a reusable abstraction for managing waiting tasks in async
//! synchronization primitives. It encapsulates the mutex + VecDeque pattern used by
//! both `Condvar` and `Semaphore`.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::task::Waker;

/// A queue of waiting tasks protected by a mutex
///
/// This abstraction separates the concern of managing waiters from the specific
/// synchronization primitive logic. It provides:
/// - Atomic check-then-add operations (prevents TOCTOU races)
/// - Safe waker storage and retrieval
/// - Efficient wakeup patterns (one or all)
///
/// # Why Use Mutex in Async Code?
///
/// This mutex is **safe for async** because:
/// 1. **No `.await` inside lock** - Never yields while holding mutex
/// 2. **Nanosecond duration** - Lock held only for memory operations (no I/O, no syscalls)
/// 3. **Low contention** - Waiters queue, rarely conflict with notifiers
/// 4. **Futex-based** - Modern OS mutexes use fast userspace CAS when uncontended (~2-3 cycles)
///
/// The mutex solves a fundamental problem: **atomically checking state AND modifying the queue**.
/// Lock-free alternatives (intrusive lists) require ~500 lines of unsafe code for the same guarantees.
///
/// # Future Optimization
///
/// Could use intrusive linked list (like `tokio::sync::Notify`) for true lock-free operation:
/// - Each waiter node lives on stack (zero allocation)
/// - Atomic pointer manipulation for queue operations
/// - Requires unsafe code and careful ABA prevention
/// - See design doc for details if profiling shows mutex contention
pub struct WaiterQueue {
    /// Queue of waiting tasks
    ///
    /// Protected by mutex to ensure atomic check-and-add operations.
    /// The mutex is held for nanoseconds (just memory operations), making it
    /// safe for async code.
    waiters: Mutex<VecDeque<Waker>>,
}

impl WaiterQueue {
    /// Create a new empty waiter queue
    #[must_use]
    pub fn new() -> Self {
        Self {
            waiters: Mutex::new(VecDeque::new()),
        }
    }

    /// Add a waiter to the queue if condition is false (atomic check-and-add)
    ///
    /// This provides the critical race-free pattern:
    /// - Acquires mutex
    /// - Checks condition INSIDE critical section
    /// - Adds waiter only if condition is false
    /// - Returns whether waiter was added
    ///
    /// # Arguments
    /// * `condition` - Closure that checks if we should wait (e.g., `|| notified.load()`)
    /// * `waker` - The waker to register if condition is false
    ///
    /// # Returns
    /// * `true` - Condition was true, waiter NOT added (ready immediately)
    /// * `false` - Condition was false, waiter added to queue (pending)
    ///
    /// # Race Prevention
    ///
    /// ```text
    /// RACE-FREE: Check and add happen atomically under mutex
    /// 1. lock.acquire()
    /// 2. if condition() { return true }  ← Checked inside lock
    /// 3. push(waker)                     ← Added inside lock
    /// 4. lock.release()
    ///
    /// Notifier can't interleave between check and push!
    /// ```
    pub fn add_waiter_if<F>(&self, condition: F, waker: Waker) -> bool
    where
        F: FnOnce() -> bool,
    {
        if let Ok(mut waiters) = self.waiters.lock() {
            // CRITICAL: Check condition WHILE HOLDING LOCK
            // This prevents notifier from changing state between check and add
            if condition() {
                // Condition already met - don't add to queue
                return true;
            }

            // Condition not met - add to queue (still holding lock)
            waiters.push_back(waker);
            false // Waiter added, pending
        } else {
            // Mutex poisoned - treat as "ready" to avoid deadlock
            true
        }
    }

    /// Wake one waiting task
    ///
    /// Removes one waiter from the front of the queue and wakes it.
    /// Lock is released before calling `wake()` to avoid holding lock during callback.
    pub fn wake_one(&self) {
        if let Ok(mut waiters) = self.waiters.lock() {
            // Pop one waiter while holding lock
            let waker = waiters.pop_front();

            // Release lock before waking (avoid holding lock during arbitrary callback)
            drop(waiters);

            // Wake outside critical section (safe to call arbitrary code)
            if let Some(waker) = waker {
                waker.wake();
            }
        }
    }

    /// Wake all waiting tasks
    ///
    /// Removes all waiters from the queue and wakes them.
    /// Uses `std::mem::take` for zero-copy queue swap.
    /// Lock is released before calling `wake()` to avoid holding lock during callbacks.
    pub fn wake_all(&self) {
        if let Ok(mut waiters) = self.waiters.lock() {
            // Swap queue with empty VecDeque (O(1), no allocation)
            let wakers = std::mem::take(&mut *waiters);

            // Release lock before waking (avoid holding lock during arbitrary callbacks)
            drop(waiters);

            // Wake all outside critical section (safe to call arbitrary code)
            for waker in wakers {
                waker.wake();
            }
        }
    }

    /// Get the number of waiting tasks (for debugging/stats)
    #[must_use]
    #[allow(dead_code)] // Used in tests and future monitoring
    pub fn waiter_count(&self) -> usize {
        self.waiters.lock().map_or(0, |w| w.len())
    }
}

impl Default for WaiterQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::task::{Wake, Waker};

    // Helper to create a dummy waker for testing
    struct DummyWaker;
    impl Wake for DummyWaker {
        fn wake(self: Arc<Self>) {}
    }

    fn dummy_waker() -> Waker {
        Arc::new(DummyWaker).into()
    }

    #[test]
    fn test_waiter_queue_add_if_condition_true() {
        let queue = WaiterQueue::new();
        let waker = dummy_waker();

        // Condition is true - should NOT add waiter
        let added = queue.add_waiter_if(|| true, waker);
        assert!(added); // Returns true = ready immediately
        assert_eq!(queue.waiter_count(), 0); // No waiter added
    }

    #[test]
    fn test_waiter_queue_add_if_condition_false() {
        let queue = WaiterQueue::new();
        let waker = dummy_waker();

        // Condition is false - should add waiter
        let added = queue.add_waiter_if(|| false, waker);
        assert!(!added); // Returns false = pending
        assert_eq!(queue.waiter_count(), 1); // Waiter added
    }

    #[test]
    fn test_waiter_queue_wake_one() {
        let queue = WaiterQueue::new();
        queue.add_waiter_if(|| false, dummy_waker());
        queue.add_waiter_if(|| false, dummy_waker());

        assert_eq!(queue.waiter_count(), 2);

        queue.wake_one();
        assert_eq!(queue.waiter_count(), 1);

        queue.wake_one();
        assert_eq!(queue.waiter_count(), 0);
    }

    #[test]
    fn test_waiter_queue_wake_all() {
        let queue = WaiterQueue::new();
        queue.add_waiter_if(|| false, dummy_waker());
        queue.add_waiter_if(|| false, dummy_waker());
        queue.add_waiter_if(|| false, dummy_waker());

        assert_eq!(queue.waiter_count(), 3);

        queue.wake_all();
        assert_eq!(queue.waiter_count(), 0);
    }

    #[test]
    fn test_waiter_queue_atomic_check_and_add() {
        let queue = WaiterQueue::new();
        let notified = AtomicBool::new(false);

        // Add waiter while not notified
        let added = queue.add_waiter_if(|| notified.load(Ordering::Acquire), dummy_waker());
        assert!(!added);
        assert_eq!(queue.waiter_count(), 1);

        // Set notified
        notified.store(true, Ordering::Release);

        // Try to add - should not add because condition is true
        let added = queue.add_waiter_if(|| notified.load(Ordering::Acquire), dummy_waker());
        assert!(added);
        assert_eq!(queue.waiter_count(), 1); // Still just one waiter
    }
}
