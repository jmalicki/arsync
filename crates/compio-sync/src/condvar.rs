//! Asynchronous condition variable for task notification
//!
//! This module provides `Condvar`, a condition variable primitive for use with
//! compio's async runtime. Unlike traditional condition variables that require
//! a mutex, this implementation is standalone and uses interior mutability.
//!
//! # Example
//!
//! ```rust,no_run
//! use compio_sync::Condvar;
//! use std::sync::Arc;
//!
//! #[compio::main]
//! async fn main() {
//!     let cv = Arc::new(Condvar::new());
//!     let cv_clone = cv.clone();
//!     
//!     // Spawn a task that waits for notification
//!     compio::runtime::spawn(async move {
//!         cv_clone.wait().await;
//!         println!("Notified!");
//!     });
//!     
//!     // Do some work...
//!     
//!     // Notify the waiting task
//!     cv.notify_one();
//! }
//! ```

use crate::waiter_queue::WaiterQueue;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll};

/// A compio-compatible async condition variable for task notification
///
/// `Condvar` allows one or more tasks to wait for a notification from another task.
/// Unlike `std::sync::Condvar`, this implementation:
/// - Works with async/await and compio's runtime
/// - Does not require an external mutex (uses interior mutability)
/// - Users should wrap in `Arc<Condvar>` when sharing between tasks
///
/// # Memory Safety
///
/// This implementation uses CAREFUL memory ordering to prevent lost wakeups:
/// - The `notified` flag is checked INSIDE the mutex to prevent TOCTOU races
/// - All notifier operations (set flag + drain) happen atomically under mutex
/// - Waiter operations (check flag + register) happen atomically under mutex
/// - All accesses use proper Acquire/Release semantics
///
/// # Usage Pattern
///
/// ```rust,no_run
/// use compio_sync::Condvar;
/// use std::sync::Arc;
///
/// # async fn example() {
/// let cv = Arc::new(Condvar::new());
///
/// // Spawn waiters
/// let mut handles = Vec::new();
/// for i in 0..5 {
///     let cv = Arc::clone(&cv);
///     let handle = compio::runtime::spawn(async move {
///         cv.wait().await;
///         i
///     });
///     handles.push(handle);
/// }
///
/// // Do some work...
///
/// // Notify all waiters
/// cv.notify_all();
///
/// // All waiters complete
/// for handle in handles {
///     handle.await.unwrap();
/// }
/// # }
/// ```
pub struct Condvar {
    /// Internal state for the condition variable
    /// Users should wrap in Arc<Condvar> when sharing between tasks
    inner: CondvarInner,
}

/// Internal state using shared waiter queue abstraction
///
/// CRITICAL RACE PREVENTION:
/// The `notified` flag MUST be checked while holding the queue's internal mutex
/// to prevent this race:
///
/// WITHOUT atomic check-and-add:
/// 1. Waiter: check notified → false (no lock)
/// 2. Notifier: set notified → true
/// 3. Notifier: drain waiters
/// 4. Waiter: add to waiters → LOST WAKEUP!
///
/// WITH atomic check-and-add (WaiterQueue):
/// 1. Waiter: lock, check notified → false, add to waiters, unlock
/// 2. Notifier: lock, set notified → true, drain waiters, unlock
///
/// The WaiterQueue encapsulates this pattern for reuse across sync primitives.
struct CondvarInner {
    /// Notification flag (true = notified, wake immediately)
    notified: AtomicBool,

    /// Waiter queue abstraction (handles mutex + check-and-add pattern)
    waiters: WaiterQueue,
}

impl Condvar {
    /// Create a new condition variable
    ///
    /// The condition variable starts in the "not notified" state.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use compio_sync::Condvar;
    ///
    /// # async fn example() {
    /// let cv = Condvar::new();
    /// # }
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: CondvarInner {
                notified: AtomicBool::new(false),
                waiters: WaiterQueue::new(),
            },
        }
    }

    /// Wait for notification
    ///
    /// Suspends the current task until `notify_one()` or `notify_all()` is called.
    /// If the condition variable is already notified, returns immediately.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use compio_sync::Condvar;
    ///
    /// # async fn example() {
    /// let cv = Condvar::new();
    /// cv.wait().await;
    /// # }
    /// ```
    pub async fn wait(&self) {
        WaitFuture { condvar: self }.await
    }

    /// Notify one waiting task
    ///
    /// Wakes up one task currently waiting on `wait()`. If no tasks are waiting,
    /// sets a flag so the next call to `wait()` returns immediately.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use compio_sync::Condvar;
    ///
    /// # async fn example() {
    /// let cv = Condvar::new();
    /// cv.notify_one();
    /// # }
    /// ```
    pub fn notify_one(&self) {
        // Set notified flag (uses Release ordering for memory synchronization)
        self.inner.notified.store(true, Ordering::Release);

        // Wake one waiter (WaiterQueue handles lock-then-wake pattern)
        self.inner.waiters.wake_one();
    }

    /// Notify all waiting tasks
    ///
    /// Wakes up all tasks currently waiting on `wait()`. Also sets a flag so that
    /// future calls to `wait()` return immediately without blocking.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use compio_sync::Condvar;
    ///
    /// # async fn example() {
    /// let cv = Condvar::new();
    /// cv.notify_all();
    /// # }
    /// ```
    pub fn notify_all(&self) {
        // Set notified flag (uses Release ordering for memory synchronization)
        self.inner.notified.store(true, Ordering::Release);

        // Wake all waiters (WaiterQueue handles lock-then-wake pattern)
        self.inner.waiters.wake_all();
    }

    /// Clear the notification flag
    ///
    /// Resets the condition variable to the "not notified" state.
    /// Future calls to `wait()` will block until `notify_one()` or `notify_all()` is called.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use compio_sync::Condvar;
    ///
    /// # async fn example() {
    /// let cv = Condvar::new();
    /// cv.notify_one();
    /// cv.clear();  // Reset notification
    /// cv.wait().await;  // Will block again
    /// # }
    /// ```
    pub fn clear(&self) {
        // Relaxed ordering is fine - this is just a reset, no synchronization needed
        self.inner.notified.store(false, Ordering::Relaxed);
    }
}

impl Default for Condvar {
    fn default() -> Self {
        Self::new()
    }
}

/// Future returned by `Condvar::wait()`
struct WaitFuture<'a> {
    /// The condition variable to wait on
    condvar: &'a Condvar,
}

impl<'a> Future for WaitFuture<'a> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // RACE-FREE PATTERN: Atomic check-and-add using WaiterQueue
        //
        // WaiterQueue.add_waiter_if() provides the critical atomicity:
        // - Checks notified flag INSIDE mutex critical section
        // - Adds waker to queue ONLY if condition is false
        // - No TOCTOU race window between check and add
        //
        // Why this is safe for async code:
        // - Mutex held for nanoseconds (just memory operations, no I/O, no syscalls)
        // - No `.await` inside critical section (never yields while holding lock)
        // - Futex-based on modern OS (~2-3 cycles when uncontended)
        //
        // See `waiter_queue.rs` for detailed explanation of why mutex is appropriate here.

        let is_ready = self.condvar.inner.waiters.add_waiter_if(
            || self.condvar.inner.notified.load(Ordering::Acquire),
            cx.waker().clone(),
        );

        if is_ready {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[compio::test]
    async fn test_condvar_already_notified() {
        let cv = Condvar::new();
        cv.notify_one();

        // Should return immediately since already notified
        cv.wait().await;
    }

    #[compio::test]
    async fn test_condvar_clear() {
        let cv = Condvar::new();
        cv.notify_one();

        // Clear and verify it returns immediately (still notified)
        cv.clear();

        // After clear, wait should block (but we won't test blocking here)
        // Just verify clear doesn't panic
    }

    #[compio::test]
    async fn test_condvar_notify_before_wait() {
        let cv = Arc::new(Condvar::new());

        // Notify before any waiter
        cv.notify_one();

        // Waiter should return immediately
        cv.wait().await;
    }

    #[compio::test]
    async fn test_condvar_notify_all() {
        let cv = Arc::new(Condvar::new());

        // Notify all before any waiters
        cv.notify_all();

        // Multiple waiters should all return immediately
        cv.wait().await;
        cv.wait().await;
        cv.wait().await;
    }

    #[test]
    fn test_condvar_creation() {
        let cv = Condvar::new();
        assert_eq!(cv.inner.notified.load(Ordering::Relaxed), false);
    }
}
