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

use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::task::{Context, Poll, Waker};

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

/// Internal state protected by mutex
///
/// CRITICAL RACE PREVENTION:
/// The `notified` flag MUST be checked while holding the `waiters` mutex
/// to prevent this race:
///
/// WITHOUT mutex protection:
/// 1. Waiter: check notified → false (no lock)
/// 2. Notifier: set notified → true
/// 3. Notifier: drain waiters
/// 4. Waiter: add to waiters → LOST WAKEUP!
///
/// WITH mutex protection:
/// 1. Waiter: lock, check notified → false, add to waiters, unlock
/// 2. Notifier: lock, set notified → true, drain waiters, unlock
///
/// The mutex ensures these operations are atomic.
struct CondvarInner {
    /// Notification flag (true = notified, wake immediately)
    /// This is atomic for lock-free fast path, but MUST also be checked under mutex
    notified: AtomicBool,

    /// Queue of waiting tasks
    /// Protected by mutex to ensure waiters can't be added during drain
    waiters: Mutex<VecDeque<Waker>>,
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
                waiters: Mutex::new(VecDeque::new()),
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
        // CRITICAL ORDERING: Must hold mutex while setting notified AND draining
        // to prevent race with waiters checking notified
        if let Ok(mut waiters) = self.inner.waiters.lock() {
            // Set notified flag INSIDE mutex critical section
            // This ensures waiters can't check-then-add between our set and drain
            self.inner.notified.store(true, Ordering::Release);

            // Remove waiter from queue while holding lock
            let waker = waiters.pop_front();

            // Release lock before waking to avoid holding lock during callback
            drop(waiters);

            // Wake the task (lock is released, safe to call arbitrary code)
            if let Some(waker) = waker {
                waker.wake();
            }
        }
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
        // CRITICAL ORDERING: Must hold mutex while setting notified AND draining
        if let Ok(mut waiters) = self.inner.waiters.lock() {
            // Set notified flag INSIDE mutex critical section
            self.inner.notified.store(true, Ordering::Release);

            // Swap with empty VecDeque (zero-copy, faster than drain+collect)
            // This is O(1) - just swaps internal pointers, no allocation
            let wakers = std::mem::take(&mut *waiters);

            // Release lock before waking to avoid holding lock during callbacks
            drop(waiters);

            // Wake all tasks (lock is released, safe to call arbitrary code)
            for waker in wakers {
                waker.wake();
            }
        }
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
        // RACE-FREE PATTERN: Check notified ONLY under mutex lock
        //
        // Why fast-path check is UNSAFE:
        // 1. Thread A (waiter): load notified → false (NO LOCK)
        // 2. Thread B (notifier): lock, store notified → true, drain, unlock
        // 3. Thread A: lock, (thinks notified=false due to stale cache), push, unlock → LOST WAKEUP!
        //
        // The problem is CPU memory ordering:
        // - Notifier's Release store might not be visible to waiter's Acquire load
        // - Even with proper ordering, there's a TOCTOU race between check and push
        //
        // Solution: Check notified INSIDE the mutex critical section
        // This ensures atomic check-and-push operation

        if let Ok(mut waiters) = self.condvar.inner.waiters.lock() {
            // CRITICAL: Check notified WHILE HOLDING LOCK
            // This prevents notifier from setting notified and draining
            // between our check and our push to the queue
            if self.condvar.inner.notified.load(Ordering::Acquire) {
                // Already notified - return immediately without adding to queue
                return Poll::Ready(());
            }

            // Safe to add to waiters:
            // - We hold the lock (notifier can't drain while we add)
            // - notified is false (checked atomically inside critical section)
            // - No TOCTOU race possible
            waiters.push_back(cx.waker().clone());
            // Lock released here when waiters drops
        }

        // Not notified, wait for wakeup
        // Waker will be called by notify_one() or notify_all()
        Poll::Pending
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
