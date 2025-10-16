//! Async condition variable for compio runtime
//!
//! Provides a condition variable primitive for async task notification, similar to
//! `tokio::sync::Notify` but built specifically for the compio runtime.
//!
//! # Example
//!
//! ```rust,no_run
//! use compio_sync::Condvar;
//! use std::sync::Arc;
//!
//! #[compio::main]
//! async fn main() {
//!     let cv = Arc::new(CondVar::new());
//!     let cv_clone = cv.clone();
//!     
//!     // Spawn a task that waits for notification
//!     compio::runtime::spawn(async move {
//!         cv_clone.wait().await;
//!         println!("Notified!");
//!     });
//!     
//!     // Signal the waiting task
//!     cv.notify_all();
//! }
//! ```

use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

/// A compio-compatible async condition variable for task notification
///
/// `Condvar` allows one or more tasks to wait for a notification from another task.
/// This is useful for coordinating async operations where one task needs to signal
/// completion to others.
///
/// # Design
///
/// Implemented using:
/// - `AtomicBool` for lock-free notification check
/// - `Mutex<VecDeque<Waker>>` for waiter queue (FIFO)
/// - Manual `Future` implementation for async waiting
///
/// # Thread Safety
///
/// All methods are thread-safe and can be called from multiple tasks concurrently.
///
/// # Usage Pattern
///
/// ```rust,no_run
/// use compio_sync::CondVar;
/// use std::sync::Arc;
///
/// # async fn example() {
/// let cv = Arc::new(CondVar::new());
///
/// // Spawn waiters
/// let mut handles = Vec::new();
/// for i in 0..5 {
///     let cv = cv.clone();
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
#[derive(Clone)]
pub struct Condvar {
    /// Shared state between all clones
    inner: Arc<CondvarInner>,
}

/// Internal shared state for the condition variable
///
/// # Implementation Note
///
/// Currently uses `Mutex<VecDeque<Waker>>` for simplicity and maintainability.
/// This is correct and performant for our use case.
///
/// **Future optimization**: Could use intrusive linked list (like tokio does) to:
/// - Avoid VecDeque allocations
/// - Avoid Waker cloning
/// - Better cache locality
/// - Improved cancellation handling
///
/// However, intrusive lists require unsafe code and are significantly more complex.
/// The current VecDeque approach is proven, simple, and fast enough.
struct CondvarInner {
    /// Whether notification has been signaled (one-shot flag)
    notified: AtomicBool,
    /// Queue of tasks waiting for notification (FIFO)
    /// TODO: Consider intrusive linked list for zero-allocation waiting
    waiters: Mutex<VecDeque<Waker>>,
}

impl Condvar {
    /// Create a new condition variable in "not notified" state
    ///
    /// Tasks calling `wait()` will block until `notify_one()` or `notify_all()` is called.
    ///
    /// # Example
    ///
    /// ```rust
    /// use compio_sync::CondVar;
    ///
    /// let cv = CondVar::new();
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(CondvarInner {
                notified: AtomicBool::new(false),
                waiters: Mutex::new(VecDeque::new()),
            }),
        }
    }

    /// Wait for notification
    ///
    /// This method will block the current task until another task calls
    /// `notify_one()` or `notify_all()`. If the condition variable has already
    /// been notified, this returns immediately.
    ///
    /// # Cancellation Safety
    ///
    /// This method is cancellation-safe. If the future is dropped before completing,
    /// the task's waker may remain in the queue but this is harmless.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use compio_sync::CondVar;
    /// use std::sync::Arc;
    ///
    /// # async fn example() {
    /// let cv = Arc::new(CondVar::new());
    /// let cv_clone = cv.clone();
    ///
    /// compio::runtime::spawn(async move {
    ///     cv_clone.wait().await;
    ///     println!("Notified!");
    /// });
    ///
    /// cv.notify_all();
    /// # }
    /// ```
    pub async fn wait(&self) {
        WaitFuture {
            condvar: self.clone(),
        }
        .await
    }

    /// Notify one waiting task
    ///
    /// Wakes up one task waiting on `wait()`. If no tasks are waiting,
    /// the notification sets a flag so the next `wait()` returns immediately.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use compio_sync::CondVar;
    ///
    /// # async fn example() {
    /// let cv = CondVar::new();
    /// cv.notify_one();
    /// # }
    /// ```
    pub fn notify_one(&self) {
        // Set notified flag
        self.inner.notified.store(true, Ordering::Release);

        // Wake one waiter if any exist
        if let Ok(mut waiters) = self.inner.waiters.lock() {
            if let Some(waker) = waiters.pop_front() {
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
    /// use compio_sync::CondVar;
    ///
    /// # async fn example() {
    /// let cv = CondVar::new();
    /// cv.notify_all();
    /// # }
    /// ```
    pub fn notify_all(&self) {
        // Set notified flag (one-shot)
        self.inner.notified.store(true, Ordering::Release);

        // Wake all waiters
        if let Ok(mut waiters) = self.inner.waiters.lock() {
            for waker in waiters.drain(..) {
                waker.wake();
            }
        }
    }
}

impl Default for Condvar {
    fn default() -> Self {
        Self::new()
    }
}

/// Future that resolves when the condition variable is notified
///
/// This future is returned by `CondVar::wait()`. It will:
/// 1. Check if already notified (fast path)
/// 2. If not notified, register the task's waker and return `Poll::Pending`
/// 3. When notified, the waker is called and the future returns `Poll::Ready`
struct WaitFuture {
    /// The condition variable to wait on
    condvar: Condvar,
}

impl Future for WaitFuture {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // Fast path: Check if already notified (lock-free)
        if self.condvar.inner.notified.load(Ordering::Acquire) {
            return Poll::Ready(());
        }

        // Not notified yet - register our waker
        if let Ok(mut waiters) = self.condvar.inner.waiters.lock() {
            waiters.push_back(cx.waker().clone());
        }

        // Check again after registering (avoid missed notification race)
        // This is critical: notification could have happened between first check
        // and registering the waker
        if self.condvar.inner.notified.load(Ordering::Acquire) {
            return Poll::Ready(());
        }

        // Not notified, wait for wakeup
        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[compio::test]
    async fn test_condvar_basic() {
        let cv = Arc::new(Condvar::new());
        let cv_clone = cv.clone();

        // Spawn a waiter
        let handle = compio::runtime::spawn(async move {
            cv_clone.wait().await;
            42
        });

        // Notify
        cv.notify_all();

        // Waiter completes
        assert_eq!(handle.await.unwrap(), 42);
    }

    #[compio::test]
    async fn test_condvar_multiple_waiters() {
        let cv = Arc::new(Condvar::new());
        let mut handles = Vec::new();

        // Spawn 10 waiters
        for i in 0..10 {
            let cv = cv.clone();
            let handle = compio::runtime::spawn(async move {
                cv.wait().await;
                i
            });
            handles.push(handle);
        }

        // Notify all
        cv.notify_all();

        // All complete
        for handle in handles {
            handle.await.unwrap();
        }
    }

    #[compio::test]
    async fn test_condvar_notify_one() {
        let cv = Arc::new(Condvar::new());
        let mut handles = Vec::new();

        // Spawn 3 waiters
        for i in 0..3 {
            let cv = cv.clone();
            let handle = compio::runtime::spawn(async move {
                cv.wait().await;
                i
            });
            handles.push(handle);
        }

        // Notify one at a time
        for _ in 0..3 {
            cv.notify_one();
            // Give time for task to wake
            compio::runtime::spawn(async {}).await.ok();
        }

        // All should complete
        for handle in handles {
            handle.await.unwrap();
        }
    }

    #[compio::test]
    async fn test_condvar_no_waiters() {
        let cv = Condvar::new();

        // Notify with no waiters - should not panic
        cv.notify_all();
        cv.notify_one();
    }

    #[compio::test]
    async fn test_condvar_already_notified() {
        let cv = Arc::new(Condvar::new());

        // Notify before any waiters
        cv.notify_all();

        // Wait should return immediately (already notified)
        cv.wait().await;

        // Multiple waits after notification should all return immediately
        cv.wait().await;
        cv.wait().await;
    }
}
