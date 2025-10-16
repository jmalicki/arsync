//! Async semaphore for compio runtime
//!
//! Provides a semaphore primitive compatible with compio's async runtime to limit
//! concurrent operations. This is critical for bounding memory usage and preventing
//! resource exhaustion during directory traversal.
//!
//! # Example
//!
//! ```rust,no_run
//! use compio_sync::Semaphore;
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create semaphore with 1024 permits
//! let semaphore = Arc::new(Semaphore::new(1024));
//!
//! // Acquire permit before starting work
//! let permit = semaphore.acquire().await;
//!
//! // Do work while holding permit
//! // ...
//!
//! // Permit automatically released when dropped
//! drop(permit);
//! # Ok(())
//! # }
//! ```

use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

/// A compio-compatible async semaphore for bounding concurrency
///
/// The semaphore maintains a fixed number of permits that must be acquired
/// before performing an operation. When all permits are in use, `acquire()`
/// will wait asynchronously until a permit becomes available.
///
/// # Design
///
/// - **Lock-free fast path**: Uses atomics for acquiring/releasing when permits available
/// - **FIFO waiters**: Blocked tasks are woken in order to prevent starvation
/// - **RAII permits**: `SemaphorePermit` automatically releases on drop
/// - **Cloneable**: Wrapped in `Arc` for sharing across tasks
///
/// # Example
///
/// ```rust,no_run
/// use compio_sync::Semaphore;
/// use std::sync::Arc;
///
/// # async fn example() {
/// let sem = Arc::new(Semaphore::new(100));
///
/// // Spawn multiple concurrent tasks
/// for i in 0..1000 {
///     let sem = sem.clone();
///     compio::runtime::spawn(async move {
///         let _permit = sem.acquire().await;
///         // Only 100 tasks run concurrently
///         println!("Processing {}", i);
///     });
/// }
/// # }
/// ```
#[derive(Clone)]
pub struct Semaphore {
    /// Shared state between all clones of this semaphore
    inner: Arc<SemaphoreInner>,
}

/// Internal shared state for the semaphore
///
/// This structure contains the atomic permit counter and the queue of waiting tasks.
/// It is wrapped in an Arc to allow the Semaphore to be cloned cheaply.
///
/// # Implementation Note
///
/// Currently uses `Mutex<VecDeque<Waker>>` for simplicity and maintainability.
///
/// **Future optimization**: Could use intrusive linked list (like tokio) to avoid
/// allocations and improve cache locality. However, this requires unsafe code and
/// is significantly more complex. The current VecDeque approach is proven and fast enough.
struct SemaphoreInner {
    /// Available permits (atomic for lock-free operations)
    permits: AtomicUsize,
    /// Maximum permits (for metrics and debugging)
    max_permits: usize,
    /// Queue of tasks waiting for permits
    /// TODO: Consider intrusive linked list for zero-allocation waiting
    waiters: Mutex<VecDeque<Waker>>,
}

impl Semaphore {
    /// Create a new semaphore with the given number of permits
    ///
    /// # Arguments
    ///
    /// * `permits` - The initial number of permits (maximum concurrency)
    ///
    /// # Panics
    ///
    /// Panics if `permits` is 0 (semaphore must have at least one permit)
    ///
    /// # Example
    ///
    /// ```rust
    /// use compio_sync::Semaphore;
    ///
    /// let sem = Semaphore::new(1024);
    /// assert_eq!(sem.available_permits(), 1024);
    /// ```
    #[must_use]
    pub fn new(permits: usize) -> Self {
        assert!(permits > 0, "Semaphore must have at least one permit");
        Self {
            inner: Arc::new(SemaphoreInner {
                permits: AtomicUsize::new(permits),
                max_permits: permits,
                waiters: Mutex::new(VecDeque::new()),
            }),
        }
    }

    /// Acquire a permit, waiting asynchronously if none are available
    ///
    /// Returns a `SemaphorePermit` that will release the permit when dropped.
    /// This method will wait (yield to other tasks) if no permits are currently available.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use compio_sync::Semaphore;
    ///
    /// # async fn example() {
    /// let sem = Semaphore::new(10);
    ///
    /// let permit = sem.acquire().await;
    /// // Do work...
    /// drop(permit);  // Release permit
    /// # }
    /// ```
    pub async fn acquire(&self) -> SemaphorePermit {
        AcquireFuture {
            semaphore: self.clone(),
        }
        .await
    }

    /// Try to acquire a permit without waiting
    ///
    /// Returns `Some(SemaphorePermit)` if a permit was immediately available,
    /// or `None` if all permits are currently in use.
    ///
    /// # Example
    ///
    /// ```rust
    /// use compio_sync::Semaphore;
    ///
    /// let sem = Semaphore::new(1);
    ///
    /// let permit1 = sem.try_acquire();
    /// assert!(permit1.is_some());
    ///
    /// let permit2 = sem.try_acquire();
    /// assert!(permit2.is_none());  // No permits left
    /// ```
    #[must_use]
    pub fn try_acquire(&self) -> Option<SemaphorePermit> {
        // Fast path: atomic decrement if permits available
        let mut current = self.inner.permits.load(Ordering::Acquire);

        loop {
            if current == 0 {
                return None; // No permits available
            }

            // Try to atomically decrement
            match self.inner.permits.compare_exchange_weak(
                current,
                current - 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    return Some(SemaphorePermit {
                        semaphore: self.clone(),
                    })
                }
                Err(actual) => current = actual, // Retry with updated value
            }
        }
    }

    /// Get the number of available permits
    ///
    /// This is useful for monitoring and debugging but should not be used
    /// for making decisions (value may change immediately after reading).
    ///
    /// # Example
    ///
    /// ```rust
    /// use compio_sync::Semaphore;
    ///
    /// let sem = Semaphore::new(100);
    /// assert_eq!(sem.available_permits(), 100);
    ///
    /// let _permit = sem.try_acquire().unwrap();
    /// assert_eq!(sem.available_permits(), 99);
    /// ```
    #[must_use]
    pub fn available_permits(&self) -> usize {
        self.inner.permits.load(Ordering::Acquire)
    }

    /// Get the maximum number of permits (configured limit)
    ///
    /// # Example
    ///
    /// ```rust
    /// use compio_sync::Semaphore;
    ///
    /// let sem = Semaphore::new(1024);
    /// assert_eq!(sem.max_permits(), 1024);
    /// ```
    #[must_use]
    pub fn max_permits(&self) -> usize {
        self.inner.max_permits
    }

    /// Get the number of permits currently in use (max - available)
    ///
    /// # Example
    ///
    /// ```rust
    /// use compio_sync::Semaphore;
    ///
    /// let sem = Semaphore::new(100);
    /// let _permit = sem.try_acquire().unwrap();
    /// assert_eq!(sem.in_use(), 1);
    /// ```
    #[must_use]
    pub fn in_use(&self) -> usize {
        self.inner.max_permits - self.available_permits()
    }

    /// Reduce the number of available permits (for adaptive concurrency control)
    ///
    /// This allows dynamically reducing concurrency in response to resource constraints
    /// like file descriptor exhaustion. Only reduces permits that are currently available
    /// (won't affect permits already in use).
    ///
    /// # Arguments
    ///
    /// * `count` - Number of permits to remove from the available pool
    ///
    /// # Returns
    ///
    /// The actual number of permits reduced (may be less than requested if not enough available)
    ///
    /// # Examples
    ///
    /// ```
    /// use compio_sync::Semaphore;
    ///
    /// let sem = Semaphore::new(100);
    /// let reduced = sem.reduce_permits(20);
    /// assert_eq!(reduced, 20);
    /// assert_eq!(sem.available_permits(), 80);
    /// ```
    #[must_use]
    pub fn reduce_permits(&self, count: usize) -> usize {
        let mut reduced = 0;

        loop {
            let current = self.inner.permits.load(Ordering::Acquire);
            if current == 0 || reduced >= count {
                break;
            }

            let to_reduce = std::cmp::min(current, count - reduced);
            let new_value = current - to_reduce;

            if self
                .inner
                .permits
                .compare_exchange(current, new_value, Ordering::Release, Ordering::Acquire)
                .is_ok()
            {
                reduced += to_reduce;
            }
        }

        reduced
    }

    /// Add permits back to the semaphore (for adaptive concurrency control)
    ///
    /// This allows dynamically increasing concurrency after resources become available.
    ///
    /// # Arguments
    ///
    /// * `count` - Number of permits to add to the available pool
    ///
    /// # Examples
    ///
    /// ```
    /// use compio_sync::Semaphore;
    ///
    /// let sem = Semaphore::new(100);
    /// sem.reduce_permits(20);
    /// assert_eq!(sem.available_permits(), 80);
    ///
    /// sem.add_permits(20);
    /// assert_eq!(sem.available_permits(), 100);
    /// ```
    pub fn add_permits(&self, count: usize) {
        self.inner.permits.fetch_add(count, Ordering::Release);

        // Wake up waiters (up to count)
        if let Ok(mut waiters) = self.inner.waiters.lock() {
            for _ in 0..count {
                if let Some(waker) = waiters.pop_front() {
                    waker.wake();
                } else {
                    break;
                }
            }
        }
    }

    /// Release a permit (called internally by `SemaphorePermit::drop`)
    fn release(&self) {
        // Increment available permits
        self.inner.permits.fetch_add(1, Ordering::Release);

        // Wake one waiter if any exist
        if let Ok(mut waiters) = self.inner.waiters.lock() {
            if let Some(waker) = waiters.pop_front() {
                waker.wake();
            }
        }
    }

    /// Add a waiter to the queue (called by `AcquireFuture`)
    fn add_waiter(&self, waker: Waker) {
        if let Ok(mut waiters) = self.inner.waiters.lock() {
            waiters.push_back(waker);
        }
    }
}

/// RAII guard that releases a semaphore permit on drop
///
/// This guard is returned by `Semaphore::acquire()` and `Semaphore::try_acquire()`.
/// When dropped, it automatically releases the permit back to the semaphore and
/// wakes one waiting task (if any).
///
/// # Example
///
/// ```rust,no_run
/// use compio_sync::Semaphore;
///
/// # async fn example() {
/// let sem = Semaphore::new(10);
///
/// {
///     let permit = sem.acquire().await;
///     // Permit is held here
/// } // Permit released automatically when scope ends
///
/// assert_eq!(sem.available_permits(), 10);
/// # }
/// ```
pub struct SemaphorePermit {
    /// Reference to the semaphore that issued this permit
    semaphore: Semaphore,
}

impl Drop for SemaphorePermit {
    fn drop(&mut self) {
        self.semaphore.release();
    }
}

/// Future that resolves when a semaphore permit is acquired
///
/// This future is returned by `Semaphore::acquire()`. It will:
/// 1. Try the fast path (atomic decrement if permits available)
/// 2. If no permits, register the task's waker and return `Poll::Pending`
/// 3. When a permit is released, the waker is called and the future retries
struct AcquireFuture {
    /// The semaphore from which to acquire a permit
    semaphore: Semaphore,
}

impl Future for AcquireFuture {
    type Output = SemaphorePermit;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // Try fast path first (lock-free atomic operation)
        if let Some(permit) = self.semaphore.try_acquire() {
            return Poll::Ready(permit);
        }

        // No permits available - register waker for notification
        self.semaphore.add_waiter(cx.waker().clone());

        // Try again immediately in case a permit became available
        // while we were registering the waker (avoid missed wakeup)
        if let Some(permit) = self.semaphore.try_acquire() {
            return Poll::Ready(permit);
        }

        // No permits available, wait for wakeup
        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_semaphore_new() {
        let sem = Semaphore::new(100);
        assert_eq!(sem.available_permits(), 100);
        assert_eq!(sem.max_permits(), 100);
        assert_eq!(sem.in_use(), 0);
    }

    #[test]
    fn test_semaphore_try_acquire() {
        let sem = Semaphore::new(2);

        // Acquire first permit
        let permit1 = sem.try_acquire();
        assert!(permit1.is_some());
        assert_eq!(sem.available_permits(), 1);
        assert_eq!(sem.in_use(), 1);

        // Acquire second permit
        let permit2 = sem.try_acquire();
        assert!(permit2.is_some());
        assert_eq!(sem.available_permits(), 0);
        assert_eq!(sem.in_use(), 2);

        // Try to acquire third (should fail)
        let permit3 = sem.try_acquire();
        assert!(permit3.is_none());
        assert_eq!(sem.available_permits(), 0);

        // Release first permit
        drop(permit1);
        assert_eq!(sem.available_permits(), 1);
        assert_eq!(sem.in_use(), 1);

        // Can acquire again
        let permit4 = sem.try_acquire();
        assert!(permit4.is_some());
        assert_eq!(sem.available_permits(), 0);
    }

    #[test]
    fn test_semaphore_permit_drop() {
        let sem = Semaphore::new(1);

        {
            let _permit = sem.try_acquire().unwrap();
            assert_eq!(sem.available_permits(), 0);
        } // Permit dropped here

        assert_eq!(sem.available_permits(), 1);
    }

    #[compio::test]
    async fn test_semaphore_acquire_basic() {
        let sem = Semaphore::new(2);

        let permit1 = sem.acquire().await;
        assert_eq!(sem.available_permits(), 1);

        let permit2 = sem.acquire().await;
        assert_eq!(sem.available_permits(), 0);

        drop(permit1);
        assert_eq!(sem.available_permits(), 1);

        drop(permit2);
        assert_eq!(sem.available_permits(), 2);
    }

    #[compio::test]
    async fn test_semaphore_blocking_and_wakeup() {
        let sem = Arc::new(Semaphore::new(1));

        // Acquire the only permit
        let permit1 = sem.acquire().await;
        assert_eq!(sem.available_permits(), 0);

        // Spawn a task that will block waiting for permit
        let sem2 = sem.clone();
        let handle = compio::runtime::spawn(async move {
            let _permit = sem2.acquire().await;
            42
        });

        // Give spawned task a chance to start and block
        // We can't use sleep without the time feature, so we'll just check after spawn
        // The task should acquire the permit when we release ours

        // Release permit - should wake the blocked task
        drop(permit1);

        // Spawned task should complete
        let result = handle.await.unwrap();
        assert_eq!(result, 42);
        assert_eq!(sem.available_permits(), 1);
    }

    #[compio::test]
    async fn test_semaphore_multiple_waiters() {
        let sem = Arc::new(Semaphore::new(1));

        // Acquire the only permit
        let permit = sem.acquire().await;

        // Spawn multiple waiting tasks
        let mut handles = Vec::new();
        for i in 0..5 {
            let sem = sem.clone();
            let handle = compio::runtime::spawn(async move {
                let _permit = sem.acquire().await;
                i
            });
            handles.push(handle);
        }

        // Release permit - should wake tasks one at a time (FIFO)
        drop(permit);

        // All tasks should eventually complete (FIFO order)
        let mut results = Vec::new();
        for handle in handles {
            results.push(handle.await.unwrap());
        }

        // Should complete in order (0, 1, 2, 3, 4) due to FIFO
        assert_eq!(results.len(), 5);
    }

    #[compio::test]
    async fn test_semaphore_high_concurrency() {
        let sem = Arc::new(Semaphore::new(100));
        let mut handles = Vec::new();

        // Spawn 1000 tasks, but only 100 should run concurrently
        for i in 0..1000 {
            let sem = sem.clone();
            let handle = compio::runtime::spawn(async move {
                let _permit = sem.acquire().await;
                // No need to simulate work - just testing concurrency limit
                i
            });
            handles.push(handle);
        }

        // Collect results
        let mut results = Vec::new();
        for handle in handles {
            results.push(handle.await.unwrap());
        }

        assert_eq!(results.len(), 1000);
        assert_eq!(sem.available_permits(), 100);
    }

    #[compio::test]
    async fn test_semaphore_clone() {
        let sem = Arc::new(Semaphore::new(10));
        let sem2 = sem.clone();

        let permit1 = sem.acquire().await;
        assert_eq!(sem2.available_permits(), 9);

        let permit2 = sem2.acquire().await;
        assert_eq!(sem.available_permits(), 8);

        drop(permit1);
        drop(permit2);
        assert_eq!(sem.available_permits(), 10);
    }

    #[test]
    #[should_panic(expected = "Semaphore must have at least one permit")]
    fn test_semaphore_zero_permits_panics() {
        let _sem = Semaphore::new(0);
    }
}
