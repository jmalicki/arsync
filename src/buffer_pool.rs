//! Buffer pool for efficient memory reuse across I/O operations
//!
//! This module provides a thread-safe buffer pool that eliminates allocations in hot paths
//! by reusing buffers across file copy operations. It maintains two separate pools:
//! - I/O buffers (user-configured size for read/write operations)
//! - Metadata buffers (fixed 4KB for statx, readlink, xattr)
//!
//! # Architecture
//!
//! The pool uses a simple design:
//! - Pre-allocate buffers at startup based on concurrency
//! - RAII guards (`PooledBuffer`) automatically return buffers on drop
//! - Thread-safe via `Mutex` (acceptable overhead - acquire is rare)
//!
//! # Performance
//!
//! - Eliminates ~16,000 allocations per 1GB file in parallel copy
//! - Expected 25-40% performance improvement on large files
//! - Bounded memory usage (e.g., ~8MB for concurrency=64, `buffer_size=64KB`)

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// Thread-safe buffer pool with two buffer types
///
/// Maintains separate pools for I/O buffers (user-configured size) and
/// metadata buffers (fixed 4KB). Buffers are automatically returned via RAII.
pub struct BufferPool {
    /// Pool for I/O buffers (read/write operations)
    io_pool: Arc<BufferSubPool>,

    /// Pool for metadata buffers (statx, readlink, xattr)
    metadata_pool: Arc<BufferSubPool>,
}

/// Sub-pool for a specific buffer type
struct BufferSubPool {
    /// Size of buffers in this pool
    buffer_size: usize,

    /// Available buffers (protected by mutex)
    available: Mutex<VecDeque<Vec<u8>>>,

    /// Statistics (lock-free)
    stats: PoolStats,

    /// Weak self-reference for `PooledBuffer` (set after Arc creation)
    self_ref: Mutex<Option<std::sync::Weak<Self>>>,
}

/// Pool statistics (atomic for lock-free updates)
#[derive(Default)]
struct PoolStats {
    /// Total buffers allocated (grows as needed)
    total_allocated: AtomicUsize,

    /// Currently in use (not in available queue)
    current_in_use: AtomicUsize,

    /// Peak simultaneous usage
    peak_usage: AtomicUsize,

    /// Total acquire calls
    total_acquisitions: AtomicUsize,
}

/// RAII guard for a pooled buffer
///
/// Automatically returns the buffer to the pool when dropped.
/// Uses interior mutability to allow taking/restoring the Vec for compio operations.
pub struct PooledBuffer {
    /// The buffer data (None when taken for compio operations)
    data: Option<Vec<u8>>,

    /// Pool to return to on drop
    pool: Arc<BufferSubPool>,
}

impl BufferPool {
    /// Create a new buffer pool
    ///
    /// # Parameters
    ///
    /// * `io_buffer_size` - Size for I/O buffers (from CLI --buffer-size)
    /// * `concurrency` - Maximum concurrent operations (for initial pool sizing)
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let pool = BufferPool::new(65536, 64);  // 64KB buffers, concurrency=64
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if mutex initialization fails (extremely rare, indicates system-level issues).
    #[must_use]
    #[allow(clippy::expect_used)] // Mutex poisoning is unrecoverable
    pub fn new(io_buffer_size: usize, concurrency: usize) -> Arc<Self> {
        let io_pool_size = 2 * concurrency; // 2× for read/write pipelining
        let metadata_pool_size = concurrency; // 1× for metadata operations

        let io_pool = Arc::new(BufferSubPool::new(io_buffer_size, io_pool_size));
        let metadata_pool = Arc::new(BufferSubPool::new(4096, metadata_pool_size));

        // Set weak self-references
        *io_pool.self_ref.lock().expect("Mutex poisoned") = Some(Arc::downgrade(&io_pool));
        *metadata_pool.self_ref.lock().expect("Mutex poisoned") =
            Some(Arc::downgrade(&metadata_pool));

        Arc::new(Self {
            io_pool,
            metadata_pool,
        })
    }

    /// Acquire an I/O buffer (user-configured size)
    ///
    /// Returns a buffer from the pool, or allocates a new one if pool is empty.
    /// Buffer is automatically returned when the `PooledBuffer` guard is dropped.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let mut pooled = pool.acquire_io_buffer();
    /// let buffer = pooled.take();  // Take Vec for compio
    /// let (result, buffer) = file.read_at(buffer, offset).await;
    /// pooled.restore(buffer);  // Put back in guard
    /// // pooled drops here, buffer returns to pool automatically
    /// ```
    #[must_use]
    pub fn acquire_io_buffer(&self) -> PooledBuffer {
        self.io_pool.acquire()
    }

    /// Acquire a metadata buffer (fixed 4KB)
    ///
    /// Used for statx, readlink, and xattr operations which have fixed size limits.
    #[must_use]
    pub fn acquire_metadata_buffer(&self) -> PooledBuffer {
        self.metadata_pool.acquire()
    }

    /// Get pool statistics
    ///
    /// Returns statistics for both I/O and metadata pools.
    #[must_use]
    pub fn stats(&self) -> BufferPoolStats {
        BufferPoolStats {
            io_pool: self.io_pool.stats.snapshot(),
            metadata_pool: self.metadata_pool.stats.snapshot(),
        }
    }
}

impl BufferSubPool {
    /// Create a new buffer sub-pool with pre-allocated buffers
    fn new(buffer_size: usize, initial_count: usize) -> Self {
        let mut buffers = VecDeque::with_capacity(initial_count);

        // Pre-allocate initial buffers
        for _ in 0..initial_count {
            buffers.push_back(vec![0u8; buffer_size]);
        }

        Self {
            buffer_size,
            available: Mutex::new(buffers),
            stats: PoolStats {
                total_allocated: AtomicUsize::new(initial_count),
                current_in_use: AtomicUsize::new(0),
                peak_usage: AtomicUsize::new(0),
                total_acquisitions: AtomicUsize::new(0),
            },
            self_ref: Mutex::new(None), // Set by BufferPool::new
        }
    }

    /// Acquire a buffer from this pool
    #[allow(clippy::expect_used)] // Mutex poisoning is unrecoverable
    fn acquire(&self) -> PooledBuffer {
        self.stats
            .total_acquisitions
            .fetch_add(1, Ordering::Relaxed);

        // Try to get buffer from pool (fast path)
        let data = {
            let mut available = self.available.lock().expect("Pool mutex poisoned");
            available.pop_front()
        };

        let data = data.map_or_else(
            || {
                // Pool empty - allocate new buffer (rare after warmup)
                self.stats.total_allocated.fetch_add(1, Ordering::Relaxed);
                vec![0u8; self.buffer_size]
            },
            |buffer| buffer,
        );

        // Update usage stats
        let in_use = self.stats.current_in_use.fetch_add(1, Ordering::Relaxed) + 1;

        // Update peak (compare-and-swap loop)
        let mut peak = self.stats.peak_usage.load(Ordering::Relaxed);
        while in_use > peak {
            match self.stats.peak_usage.compare_exchange_weak(
                peak,
                in_use,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(current) => peak = current,
            }
        }

        // These panics are programming errors (pool setup) or unrecoverable states (poisoned mutex)
        #[allow(clippy::expect_used)]
        let pool_ref = self
            .self_ref
            .lock()
            .expect("Mutex poisoned")
            .as_ref()
            .expect("Self-reference not set")
            .upgrade()
            .expect("Pool was dropped");

        PooledBuffer {
            data: Some(data),
            pool: pool_ref,
        }
    }

    /// Release a buffer back to the pool
    #[allow(clippy::expect_used)] // Mutex poisoning is unrecoverable
    fn release(&self, mut data: Vec<u8>) {
        // Ensure buffer is at full size (resize reuses capacity if available)
        // Note: This should not reallocate if capacity >= buffer_size
        if data.len() != self.buffer_size {
            data.resize(self.buffer_size, 0);
        }

        // Sanity check - buffer should have correct capacity
        debug_assert!(
            data.capacity() >= self.buffer_size,
            "Buffer capacity {} < expected {}",
            data.capacity(),
            self.buffer_size
        );

        self.stats.current_in_use.fetch_sub(1, Ordering::Relaxed);

        let mut available = self.available.lock().expect("Pool mutex poisoned");
        available.push_back(data);
    }
}

impl PooledBuffer {
    /// Take ownership of the inner Vec for compio operations
    ///
    /// This transfers the buffer to compio's `read_at/write_at` operations.
    /// After the operation, use `restore()` to put the buffer back.
    ///
    /// # Panics
    ///
    /// Panics if the buffer has already been taken.
    #[must_use]
    #[allow(clippy::expect_used)] // Double-take is a programming error
    #[allow(clippy::missing_const_for_fn)] // Option::take() is not const
    pub fn take(&mut self) -> Vec<u8> {
        self.data.take().expect("Buffer already taken")
    }

    /// Restore the buffer after a compio operation
    ///
    /// compio operations return (Result, Buffer) tuples. This method
    /// puts the buffer back into the guard for automatic return to pool.
    ///
    /// # Panics
    ///
    /// Panics if the buffer is already present (double restore).
    #[allow(clippy::panic)] // Double-restore is a programming error
    pub fn restore(&mut self, data: Vec<u8>) {
        assert!(self.data.is_none(), "Buffer already restored");
        self.data = Some(data);
    }
}

impl Drop for PooledBuffer {
    fn drop(&mut self) {
        // Return buffer to pool if present
        if let Some(data) = self.data.take() {
            self.pool.release(data);
        }
    }
}

/// Statistics for a pool
#[derive(Debug, Clone)]
pub struct PoolStatsSnapshot {
    /// Total buffers ever allocated
    pub total_allocated: usize,

    /// Currently in use
    pub current_in_use: usize,

    /// Peak simultaneous usage
    pub peak_usage: usize,

    /// Total acquisitions
    pub total_acquisitions: usize,
}

impl PoolStats {
    fn snapshot(&self) -> PoolStatsSnapshot {
        PoolStatsSnapshot {
            total_allocated: self.total_allocated.load(Ordering::Relaxed),
            current_in_use: self.current_in_use.load(Ordering::Relaxed),
            peak_usage: self.peak_usage.load(Ordering::Relaxed),
            total_acquisitions: self.total_acquisitions.load(Ordering::Relaxed),
        }
    }
}

/// Aggregate pool statistics
#[derive(Debug, Clone)]
pub struct BufferPoolStats {
    /// I/O pool statistics
    pub io_pool: PoolStatsSnapshot,

    /// Metadata pool statistics
    pub metadata_pool: PoolStatsSnapshot,
}

impl BufferPoolStats {
    /// Calculate pool hit rate (% of acquisitions that didn't allocate)
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // Loss acceptable for statistics
    pub fn hit_rate(&self) -> f64 {
        let total_acq = self.io_pool.total_acquisitions + self.metadata_pool.total_acquisitions;
        let total_alloc = self.io_pool.total_allocated + self.metadata_pool.total_allocated;

        if total_acq == 0 {
            return 0.0;
        }

        let hits = total_acq.saturating_sub(total_alloc);
        (hits as f64 / total_acq as f64) * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_io_buffer_basic() {
        let pool = BufferPool::new(65536, 64);

        let mut pooled = pool.acquire_io_buffer();
        let buffer = pooled.take();

        assert_eq!(buffer.len(), 65536);
        assert_eq!(buffer.capacity(), 65536);
    }

    #[test]
    fn test_metadata_buffer_size() {
        let pool = BufferPool::new(65536, 64);

        let mut pooled = pool.acquire_metadata_buffer();
        let buffer = pooled.take();

        assert_eq!(buffer.len(), 4096, "Metadata buffers are always 4KB");
    }

    #[test]
    fn test_buffer_reuse() {
        // Use concurrency=0 so only 0 buffers pre-allocated (forces allocation then reuse)
        let pool = BufferPool::new(1024, 0);

        // First acquire - will allocate
        let mut buf1 = pool.acquire_io_buffer();
        let data1 = buf1.take();
        let ptr1 = data1.as_ptr();
        buf1.restore(data1);
        drop(buf1); // Return to pool

        // Second acquire - should reuse (pool now has 1 buffer)
        let mut buf2 = pool.acquire_io_buffer();
        let data2 = buf2.take();
        let ptr2 = data2.as_ptr();

        assert_eq!(ptr1, ptr2, "Buffer should be reused (same pointer)");

        // Verify stats
        let stats = pool.stats();
        assert_eq!(
            stats.io_pool.total_allocated, 1,
            "Should only allocate once"
        );
        assert_eq!(stats.io_pool.total_acquisitions, 2, "Two acquisitions");
    }

    #[test]
    fn test_pool_stats() {
        let pool = BufferPool::new(1024, 2);

        let _buf1 = pool.acquire_io_buffer();
        let _buf2 = pool.acquire_io_buffer();
        let _buf3 = pool.acquire_io_buffer(); // Should allocate (pool only has 2×2=4 pre-allocated)

        let stats = pool.stats();
        assert_eq!(stats.io_pool.total_acquisitions, 3);
        assert_eq!(stats.io_pool.current_in_use, 3);
        // 2×2=4 pre-allocated initially, no additional needed for 3 acquisitions
        assert_eq!(
            stats.io_pool.total_allocated, 4,
            "Should have 4 pre-allocated"
        );
    }

    #[test]
    fn test_concurrent_access() {
        use std::thread;

        let pool = BufferPool::new(1024, 10);
        let pool_clone = Arc::clone(&pool);

        // Spawn threads that acquire/release buffers
        let handles: Vec<_> = (0..10)
            .map(|_| {
                let pool = Arc::clone(&pool_clone);
                thread::spawn(move || {
                    for _ in 0..100 {
                        let _buf = pool.acquire_io_buffer();
                        // Auto-release on drop
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().expect("Thread panicked");
        }

        let stats = pool.stats();
        assert_eq!(stats.io_pool.total_acquisitions, 1000);
        assert_eq!(stats.io_pool.current_in_use, 0, "All buffers returned");

        // Should have good reuse (not 1000 allocations!)
        assert!(
            stats.io_pool.total_allocated < 50,
            "Should reuse buffers, got {} allocations",
            stats.io_pool.total_allocated
        );
    }

    #[test]
    fn test_auto_return_on_drop() {
        let pool = BufferPool::new(1024, 2);

        {
            let _buf = pool.acquire_io_buffer();
            let stats = pool.stats();
            assert_eq!(stats.io_pool.current_in_use, 1);
        } // buf drops here

        let stats = pool.stats();
        assert_eq!(stats.io_pool.current_in_use, 0, "Buffer should be returned");
    }

    #[test]
    fn test_pool_growth_and_reuse() {
        let pool = BufferPool::new(1024, 1); // Small pool: 2 pre-allocated

        // Acquire more than available (force growth)
        let buf1 = pool.acquire_io_buffer();
        let buf2 = pool.acquire_io_buffer();
        let buf3 = pool.acquire_io_buffer(); // Should allocate new

        let stats = pool.stats();
        assert!(
            stats.io_pool.total_allocated >= 3,
            "Should have allocated at least 3 buffers"
        );

        drop(buf1);
        drop(buf2);
        drop(buf3);

        // Now acquire again - should reuse
        let _buf4 = pool.acquire_io_buffer();

        let stats_after = pool.stats();
        assert_eq!(
            stats_after.io_pool.total_allocated, stats.io_pool.total_allocated,
            "Should not allocate more (reusing)"
        );
    }
}
