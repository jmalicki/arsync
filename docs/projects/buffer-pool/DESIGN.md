# Compio Buffer Pool Design

**Branch**: feat/compio-buffer-pool  
**Stacked on**: PR #91 (TraversalContext refactor)  
**Goal**: Zero-copy I/O with io_uring registered buffers  
**Date**: October 21, 2025

---

## Problem Statement

### Current State

**Buffer allocation patterns:**
```rust
// In copy_read_write: ONE buffer per file (good!)
let mut buffer = vec![0u8; BUFFER_SIZE];  // User-configured size, reused per loop

// In copy_region_sequential: NEW buffer EVERY iteration (bad!)
let buffer = vec![0u8; to_read];  // Allocated ~16,000 times for 1GB file

// In statx, symlink reads, xattr: ad-hoc small allocations
let mut buffer = vec![0u8; 4096];  // Fixed sizes, scattered throughout
```

**Issues:**
1. ❌ **Parallel copy allocates constantly** - New buffer every chunk (~16k allocations per 1GB file)
2. ❌ **No io_uring buffer registration** - Missing zero-copy potential
3. ❌ **No buffer reuse across files** - Each file gets fresh allocations

### Target State

**Two buffer types with io_uring registration:**
```rust
// Pool of user-configured size buffers (for read/write)
let buffer = pool.acquire_io_buffer().await;  // e.g., 64KB, 128KB, etc.
src.read_at(buffer, offset).await;  // io_uring can use registered buffer
pool.release(buffer);

// Pool of fixed-size metadata buffers
let buffer = pool.acquire_metadata_buffer().await;  // Always 4KB
statx_at(dirfd, &buffer).await;
pool.release(buffer);
```

**Benefits:**
1. ✅ **Zero allocations in hot paths** - All buffers pre-allocated and reused
2. ✅ **Zero-copy I/O** - io_uring uses registered buffers (IORING_OP_READ_FIXED)
3. ✅ **Cross-file reuse** - Buffers shared across entire sync operation
4. ✅ **Simple design** - Just two buffer types, not complex size classes

---

## io_uring Registered Buffers

### How It Works

**Traditional I/O:**
```
User buffer → Kernel copies to kernel buffer → Device
                 ↑ Copy overhead
```

**Registered buffers:**
```
Pre-registered buffer → Kernel uses directly → Device
                         ↑ Zero-copy!
```

### io_uring API

```c
// Setup: Register buffers with kernel once
struct iovec iovecs[NUM_BUFFERS];
// ... fill iovecs with buffer addresses ...
io_uring_register_buffers(&ring, iovecs, NUM_BUFFERS);

// Use: Reference by buffer ID instead of passing pointer
struct io_uring_sqe *sqe = io_uring_get_sqe(&ring);
io_uring_prep_read_fixed(sqe, fd, buf, len, offset, buf_index);
//                       ↑ uses buf_index, not buf pointer!
```

### compio Support

**Question: Does compio expose buffer registration?**

Looking at the code, compio uses `IoBuf`/`IoBufMut` traits but doesn't currently expose:
- `io_uring_register_buffers()` - Register buffer array
- `io_uring_prep_read_fixed()` - Use registered buffer by index
- `io_uring_prep_write_fixed()` - Write with registered buffer

**Investigation needed:**
1. Check if compio has buffer registration support (probably not yet)
2. Check if we need to add it to compio-fs-extended
3. Determine if we can use raw io_uring_sys for this feature

---

## Design: Buffer Pool Architecture

### Two Buffer Types (Not Size Classes!)

**Simplified approach** - buffers are determined by use case, not arbitrary sizes:

```rust
pub struct BufferPool {
    /// I/O buffers for read/write operations
    /// Size comes from user configuration (CLI --buffer-size)
    io_buffers: BufferSubPool,
    
    /// Fixed-size metadata buffers (4KB)
    /// For statx, readlink, xattr operations
    metadata_buffers: BufferSubPool,
}
```

**Two pools:**
1. **I/O buffer pool** - User-configured size (default 64KB)
   - Used for: file read/write operations
   - Size: From `ParallelCopyConfig::buffer_size` CLI arg
   - Count: Based on concurrency (e.g., 2× max concurrent files)

2. **Metadata buffer pool** - Fixed 4KB
   - Used for: statx, readlink, xattr
   - Size: Always 4096 bytes (kernel limits)
   - Count: Based on concurrency (e.g., 4× max concurrent operations)

**Rationale:**
- ✅ Simple: Just two pools, clear purpose for each
- ✅ User control: I/O buffer size is configurable
- ✅ Efficient: Metadata operations don't waste memory with large buffers
- ✅ No magic numbers: Buffer sizes have clear provenance

### Pool Structure (Per-Thread!)

**Critical insight**: Buffers must be per-thread because:
1. Each thread has its own io_uring ring
2. Registered buffers are per-ring (not shareable across rings)
3. compio dispatcher schedules tasks on arbitrary threads

```rust
/// Thread-local buffer pool (one per worker thread)
thread_local! {
    static THREAD_BUFFER_POOL: RefCell<ThreadBufferPool> = RefCell::new(
        ThreadBufferPool::new_for_current_thread()
    );
}

/// Buffer pool for a single thread's io_uring ring
struct ThreadBufferPool {
    /// I/O buffers (user-configured size)
    io_buffers: VecDeque<Vec<u8>>,
    io_buffer_size: usize,
    
    /// Metadata buffers (fixed 4KB)
    metadata_buffers: VecDeque<Vec<u8>>,
    
    /// io_uring registration for THIS thread's ring
    registration: Option<RegisteredBuffers>,
    
    /// Statistics for this thread
    stats: PoolStats,
}

struct PoolStats {
    io_buffers_allocated: usize,
    io_buffers_in_use: usize,
    metadata_buffers_allocated: usize,
    metadata_buffers_in_use: usize,
    total_acquisitions: usize,
}

/// RAII guard for pooled buffer
pub struct PooledBuffer {
    /// The actual buffer data (None when taken for compio operations)
    data: Option<Vec<u8>>,
    
    /// io_uring buffer index (if registered with THIS thread's ring)
    ioring_index: Option<u16>,
    
    /// Buffer type (for return to correct pool)
    buffer_type: BufferType,
}

enum BufferType {
    Io,
    Metadata,
}
```

### RAII Pattern

```rust
impl Drop for PooledBuffer {
    fn drop(&mut self) {
        // Automatically return buffer to pool when dropped
        self.pool.release(self);
    }
}

// Usage - automatic cleanup!
{
    let buffer = pool.acquire(BufferSize::Medium).await;
    src.read_at(buffer, offset).await;
    // buffer automatically returned to pool on drop
}
```

### API Design

```rust
impl BufferPool {
    /// Create pool with user-configured I/O buffer size
    pub fn new(io_buffer_size: usize) -> Self {
        Self {
            io_pool: Arc::new(BufferSubPool::new(io_buffer_size)),
            metadata_pool: Arc::new(BufferSubPool::new(4096)),
            registration: None,
        }
    }
    
    /// Acquire I/O buffer (user-configured size for read/write)
    pub fn acquire_io_buffer(&self) -> PooledBuffer {
        self.io_pool.acquire()
    }
    
    /// Acquire metadata buffer (fixed 4KB for statx/readlink)
    pub fn acquire_metadata_buffer(&self) -> PooledBuffer {
        self.metadata_pool.acquire()
    }
    
    /// Get pool statistics
    pub fn stats(&self) -> BufferPoolStats;
}

impl BufferSubPool {
    fn acquire(&self) -> PooledBuffer {
        // Try to get existing buffer from pool
        if let Some(data) = self.available.lock().unwrap().pop_front() {
            self.stats.current_in_use.fetch_add(1, Ordering::Relaxed);
            self.stats.total_acquisitions.fetch_add(1, Ordering::Relaxed);
            return PooledBuffer {
                data: Some(data),
                ioring_index: None,  // TODO: Phase 3
                pool: Arc::clone(&self),
            };
        }
        
        // Pool empty - allocate new buffer
        let data = vec![0u8; self.buffer_size];
        self.stats.total_allocated.fetch_add(1, Ordering::Relaxed);
        self.stats.current_in_use.fetch_add(1, Ordering::Relaxed);
        self.update_peak();
        
        PooledBuffer {
            data: Some(data),
            ioring_index: None,
            pool: Arc::clone(&self),
        }
    }
    
    fn release(&self, data: Vec<u8>) {
        self.stats.current_in_use.fetch_sub(1, Ordering::Relaxed);
        self.available.lock().unwrap().push_back(data);
    }
}
```

---

## io_uring Buffer Registration

### Option 1: Wait for compio support

**Pros:**
- Clean API integration
- Maintained by compio team
- Works with compio's buffer traits

**Cons:**
- Uncertain timeline
- May never be implemented
- Blocks our optimization

### Option 2: Add to compio-fs-extended

**Pros:**
- We control the implementation
- Can upstream later
- Immediate benefit

**Cons:**
- Need to use io_uring_sys directly
- More unsafe code
- Need to track registration state

### Option 3: Hybrid approach (RECOMMENDED)

**Phase 1 (immediate)**: Buffer pool WITHOUT io_uring registration
- Still gets benefit of allocation reuse
- No unsafe code needed
- Works with current compio

**Phase 2 (later)**: Add io_uring registration
- Implement in compio-fs-extended
- Use io_uring_sys for registration
- Optional feature: only if io_uring available

---

## Implementation Plan

### Phase 1: Buffer Pool (No io_uring registration yet)

#### 1.1 Create buffer pool infrastructure

**New file**: `src/buffer_pool.rs`

```rust
//! Buffer pool for efficient memory reuse across I/O operations

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Buffer size classes for different I/O operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BufferSize {
    /// 128 bytes - Tiny metadata buffers
    Tiny = 128,
    /// 4KB - Small buffers for statx, symlinks, xattr
    Small = 4096,
    /// 64KB - Standard I/O operations
    Medium = 65536,
    /// 2MB - Large parallel copy chunks (huge page aligned)
    Large = 2097152,
}

/// Thread-safe buffer pool
pub struct BufferPool {
    pools: HashMap<BufferSize, Arc<SizeClassPool>>,
}

struct SizeClassPool {
    size: usize,
    available: Mutex<VecDeque<Vec<u8>>>,
    stats: PoolStats,
}

struct PoolStats {
    total_allocated: AtomicUsize,
    current_in_use: AtomicUsize,
    peak_usage: AtomicUsize,
    total_acquisitions: AtomicUsize,
}

/// RAII guard for pooled buffer
pub struct PooledBuffer {
    data: Option<Vec<u8>>,
    pool: Arc<SizeClassPool>,
}

impl BufferPool {
    /// Create buffer pool with user-configured I/O buffer size
    ///
    /// # Parameters
    /// - `io_buffer_size`: Size for read/write buffers (from CLI)
    /// - `concurrency`: Max concurrent operations (for pool sizing)
    pub fn new(io_buffer_size: usize, concurrency: usize) -> Self {
        let io_pool_size = 2 * concurrency;  // 2× for pipelining
        let metadata_pool_size = concurrency;  // 1× for metadata ops
        
        Self {
            io_pool: Arc::new(BufferSubPool::new(io_buffer_size, io_pool_size)),
            metadata_pool: Arc::new(BufferSubPool::new(4096, metadata_pool_size)),
            registration: None,
        }
    }
    
    /// Acquire I/O buffer (user-configured size for read/write)
    pub fn acquire_io_buffer(&self) -> PooledBuffer;
    
    /// Acquire metadata buffer (fixed 4KB for statx/readlink/xattr)
    pub fn acquire_metadata_buffer(&self) -> PooledBuffer;
    
    /// Get pool statistics
    pub fn stats(&self) -> BufferPoolStats;
}

impl PooledBuffer {
    /// Take ownership of the inner Vec (for compio operations)
    ///
    /// This transfers ownership to compio's read_at/write_at.
    /// After the operation, use `restore()` to return it to the pool.
    pub fn take(&mut self) -> Vec<u8> {
        self.data.take().expect("Buffer already taken")
    }
    
    /// Restore buffer after compio operation returns it
    ///
    /// compio operations return (Result, Buffer) - this puts the buffer back.
    pub fn restore(&mut self, data: Vec<u8>) {
        assert!(self.data.is_none(), "Buffer already present");
        self.data = Some(data);
    }
}

impl Drop for PooledBuffer {
    fn drop(&mut self) {
        // Return buffer to pool automatically
        if let Some(data) = self.data.take() {
            self.pool.release(data);
        }
    }
}
```

#### 1.2 Integration points

**In `src/copy.rs` (copy_read_write):**
```rust
// Before:
let mut buffer = vec![0u8; BUFFER_SIZE];  // BUFFER_SIZE is user-configured

// After:
let mut pooled = ctx.buffer_pool.acquire_io_buffer();  // Same size from CLI
let mut buffer = pooled.take();  // Take Vec for compio
while total_copied < file_size {
    let read_result = src_file.read_at(buffer, offset).await;
    buffer = read_result.1;  // Get buffer back
    // ... process ...
    pooled.restore(buffer);  // Put back in guard for next iteration
}
// pooled drops here, returns to pool automatically
```

**In parallel copy (`copy_region_sequential`):**
```rust
// Before: NEW allocation every iteration (!!!!)
while offset < end {
    let buffer = vec![0u8; to_read];  // 🔥 ALLOCATES EVERY TIME
    let read_result = src.read_at(buffer, offset).await;
    // ...
}

// After: Acquire ONCE, reuse for entire region
let mut pooled = ctx.buffer_pool.acquire_io_buffer();
let mut buffer = pooled.take();
while offset < end {
    // Resize buffer if needed (reuses capacity)
    buffer.resize(to_read, 0);
    let read_result = src.read_at(buffer, offset).await;
    buffer = read_result.1;
    // ...
}
// pooled drops here, returns to pool
```

**In metadata operations (statx, readlink):**
```rust
// For readlink in compio-fs-extended:
let pooled = buffer_pool.acquire_metadata_buffer();  // 4KB
let buffer = pooled.take();
let (bytes_read, buffer) = readlinkat(dirfd, path, buffer).await;
// pooled drops, buffer returns to pool

// For statx - currently uses stack allocation, might not need pool
// (statx buffer is tiny ~256 bytes, stack is fine)
```

#### 1.3 Testing

**New test file**: `tests/buffer_pool_tests.rs`

```rust
#[test]
fn test_io_buffer_reuse() {
    let pool = BufferPool::new(65536, 64);  // 64KB, concurrency=64
    
    // Acquire and release
    let mut buf1 = pool.acquire_io_buffer();
    let data1 = buf1.take();
    let ptr1 = data1.as_ptr();
    buf1.restore(data1);
    drop(buf1);  // Returns to pool
    
    // Should get same buffer back
    let mut buf2 = pool.acquire_io_buffer();
    let data2 = buf2.take();
    let ptr2 = data2.as_ptr();
    assert_eq!(ptr1, ptr2, "Buffer should be reused");
}

#[test]
fn test_metadata_buffer_size() {
    let pool = BufferPool::new(65536, 64);
    
    let mut buf = pool.acquire_metadata_buffer();
    let data = buf.take();
    
    assert_eq!(data.len(), 4096, "Metadata buffers are always 4KB");
    assert_eq!(data.capacity(), 4096);
}

#[test]
fn test_buffer_pool_concurrent() {
    let pool = Arc::new(BufferPool::new(65536, 64));
    
    // Spawn 100 tasks, each acquiring/releasing I/O buffers
    let handles: Vec<_> = (0..100).map(|_| {
        let pool = Arc::clone(&pool);
        std::thread::spawn(move || {
            for _ in 0..1000 {
                let _buf = pool.acquire_io_buffer();
                // Automatic return on drop
            }
        })
    }).collect();
    
    for h in handles { h.join().unwrap(); }
    
    let stats = pool.stats();
    println!("I/O buffers - peak in use: {}", stats.io_pool_peak);
    println!("Metadata buffers - peak in use: {}", stats.metadata_pool_peak);
}
```

### Phase 2: io_uring Buffer Registration (Future)

#### 2.1 Add registration support to compio-fs-extended

**New file**: `crates/compio-fs-extended/src/registered_buffers.rs`

```rust
//! io_uring registered buffer support for zero-copy I/O

use io_uring_sys::io_uring;
use std::sync::Arc;

/// Registered buffer pool for io_uring zero-copy operations
pub struct RegisteredBuffers {
    /// Raw io_uring instance
    ring: Arc<io_uring>,
    
    /// Buffer metadata
    buffers: Vec<BufferInfo>,
    
    /// Available buffer indices
    available: Mutex<Vec<u16>>,
}

struct BufferInfo {
    /// Buffer index in io_uring
    index: u16,
    
    /// Buffer address (pinned in memory)
    address: *mut u8,
    
    /// Buffer size
    size: usize,
}

impl RegisteredBuffers {
    /// Register buffers with io_uring
    ///
    /// SAFETY: Buffers must remain valid and pinned for lifetime of registration
    pub unsafe fn register(
        ring: &io_uring,
        buffers: &[Vec<u8>],
    ) -> Result<Self>;
    
    /// Acquire a registered buffer by index
    pub fn acquire_index(&self) -> Option<u16>;
    
    /// Release buffer index back to pool
    pub fn release_index(&self, index: u16);
}
```

#### 2.2 Modify DirectoryFd operations

```rust
// In DirectoryFd::read_at_fixed
pub async fn read_at_fixed(
    &self,
    buf_index: u16,  // io_uring buffer index
    offset: u64,
) -> Result<usize> {
    // Use io_uring_prep_read_fixed instead of read_at
    // Kernel uses registered buffer directly (zero-copy!)
}
```

#### 2.3 Update buffer pool to use registration

```rust
impl BufferPool {
    /// Create pool with io_uring buffer registration
    pub fn with_registration(ring: &io_uring) -> Result<Self> {
        let mut buffers = Vec::new();
        
        // Pre-allocate buffers for each size class
        for size in [BufferSize::Tiny, Small, Medium, Large] {
            for _ in 0..pool_size(size) {
                buffers.push(vec![0u8; size as usize]);
            }
        }
        
        // Register all buffers with io_uring
        let registration = unsafe {
            RegisteredBuffers::register(&ring, &buffers)?
        };
        
        Ok(Self { buffers, registration: Some(registration) })
    }
}
```

---

## Buffer Sizing Strategy

### I/O Buffer Size (User-Configured)

**Source**: CLI argument `--buffer-size` (in `ParallelCopyConfig`)

```rust
// From command line (already exists):
#[arg(long, default_value = "65536", value_parser = parse_buffer_size)]
pub buffer_size: usize,  // Default: 64KB
```

**Usage:**
- File read/write operations
- Parallel copy chunks
- All data I/O

**Pool count calculation:**
```rust
// Conservative: 2× max concurrent operations
let io_pool_size = 2 * max_concurrent_files;

// e.g., if concurrency = 64, pool has 128 buffers
// Memory: 128 × 64KB = 8MB (with default buffer size)
```

### Metadata Buffer Size (Fixed 4KB)

**Source**: Kernel limits

```rust
const METADATA_BUFFER_SIZE: usize = 4096;
```

**Why 4KB?**
- `statx`: struct is ~256 bytes, but we need PATH_MAX for paths
- `readlink`: PATH_MAX is 4096 bytes
- `xattr`: Most values fit in 4KB (we already handle larger separately)

**Pool count:**
```rust
// Conservative: Equal to concurrency limit
let metadata_pool_size = max_concurrent_files;

// e.g., if concurrency = 64, pool has 64 buffers
// Memory: 64 × 4KB = 256KB
```

### Total Memory Budget

**Example** (concurrency = 64, buffer_size = 64KB):
```
I/O pool:       128 buffers × 64KB  = 8MB
Metadata pool:   64 buffers × 4KB   = 256KB
                              Total = ~8.3MB
```

**Scaling** (concurrency = 256, buffer_size = 128KB):
```
I/O pool:       512 buffers × 128KB = 64MB
Metadata pool:  256 buffers × 4KB   = 1MB
                              Total = ~65MB
```

**Key insight**: Memory is predictable and bounded!

**io_uring registration limit**: 1024 buffers per ring (kernel limit) - we're well under this

---

## Implementation Phases

### Phase 1: Global Two-Pool System (Week 1)

**Deliverables:**
1. ✅ `src/buffer_pool.rs` - Global two-pool implementation
2. ✅ Integration in `src/copy.rs` - Replace allocations in sequential and parallel copy
3. ✅ Pass buffer pool config (io_buffer_size from CLI)
4. ✅ Tests - Verify reuse, concurrency, thread safety
5. ✅ Benchmarks - Measure allocation reduction

**Success metrics:**
- Zero allocations in `copy_region_sequential` hot path
- 50-70% reduction in total allocations per sync
- 25-40% performance improvement on large files
- Works correctly with compio task migration
- No performance regression

**Explicitly NOT doing:**
- ❌ io_uring buffer registration (incompatible with task migration)

### Phase 2: Measure and Decide (Week 2)

**Investigation:**
1. Profile compio task migration frequency
2. Measure actual thread affinity in practice
3. Research compio thread-pinning possibilities
4. Benchmark global pool performance

**Decision point:**
- If tasks rarely migrate: Consider thread-local pools + registration
- If tasks migrate often: Keep global pool, skip registration
- Document findings and trade-offs

**Success metrics:**
- Clear data on task migration patterns
- Informed decision on registration feasibility
- Documented rationale for approach chosen

---

## Integration with compio

### Current compio Buffer Model

compio uses **ownership-based buffers**:

```rust
// compio takes ownership, returns buffer
let (result, buffer) = file.read_at(buffer, offset).await;
//                     ↑ Tuple: (Result<usize>, Buffer)
```

**This is perfect for pooling!**

The pool can:
1. Give out a `Vec<u8>` to compio
2. Get it back from the result tuple
3. Return it to pool for next use

### IoBuf/IoBufMut Traits

```rust
pub trait IoBuf: 'static {
    fn as_buf_ptr(&self) -> *const u8;
    fn buf_len(&self) -> usize;
    fn buf_capacity(&self) -> usize;
}

pub trait IoBufMut: IoBuf {
    fn as_buf_mut_ptr(&mut self) -> *mut u8;
}
```

**Our pool buffers satisfy these traits** because `Vec<u8>` implements them!

### Pool Integration Pattern

```rust
// Wrapper for pool integration
impl IoBuf for PooledBuffer {
    fn as_buf_ptr(&self) -> *const u8 {
        self.data.as_ref().unwrap().as_ptr()
    }
    
    fn buf_len(&self) -> usize {
        self.data.as_ref().unwrap().len()
    }
    
    fn buf_capacity(&self) -> usize {
        self.data.as_ref().unwrap().capacity()
    }
}

impl IoBufMut for PooledBuffer {
    fn as_buf_mut_ptr(&mut self) -> *mut u8 {
        self.data.as_mut().unwrap().as_mut_ptr()
    }
}
```

---

## Performance Expectations

### Current Performance

**1GB file copy (parallel, 8 threads):**
- Allocations: ~16,000+ (new buffer per chunk)
- Memory pressure: High
- Allocator contention: Moderate

### With Buffer Pool

**1GB file copy (parallel, 8 threads):**
- Allocations: ~16 (one-time pool init)
- Memory pressure: Low
- Allocator contention: None

**Expected improvements:**
- **Large files**: 25-40% faster (less allocator overhead)
- **High concurrency**: 10-15% faster (no allocator contention)
- **Memory**: Bounded usage (34-137MB vs unbounded)

### With io_uring Registration (Phase 3)

**Additional improvements:**
- **CPU usage**: 5-10% reduction (kernel doesn't copy)
- **Throughput**: 10-20% improvement (zero-copy)
- **Cache efficiency**: Better (fewer buffer copies)

---

## Risks and Mitigations

### Risk 1: Pool Exhaustion

**Risk**: All buffers in use, acquire() blocks

**Mitigation**:
1. Grow pool dynamically (up to max)
2. Log warnings when pool is exhausted
3. Tune pool sizes based on concurrency limits

### Risk 2: Memory Pinning (io_uring registration)

**Risk**: Registered buffers must stay pinned, can't be moved by allocator

**Mitigation**:
1. Use `Box<[u8]>` instead of `Vec<u8>` (stable address)
2. Pin buffers explicitly
3. Never resize registered buffers

### Risk 3: compio API Changes

**Risk**: Future compio versions change buffer handling

**Mitigation**:
1. Keep pool interface separate from compio
2. Use adapter pattern for compio integration
3. Version pin compio until stable

### Risk 4: Fragmentation

**Risk**: Pool holds memory even when idle

**Mitigation**:
1. Implement shrink-on-idle
2. Release buffers after timeout
3. Configurable min/max pool sizes

---

## Critical Design Challenge: Thread Migration vs Registered Buffers

### The Problem

**io_uring registered buffers are per-ring, rings are per-thread:**

```
Thread A: io_uring ring A → registered buffers [0-127]
Thread B: io_uring ring B → registered buffers [0-127]
Thread C: io_uring ring C → registered buffers [0-127]
```

**compio dispatcher can migrate tasks between threads:**

```
Task starts on Thread A → acquires buffer from Thread A's pool
Task resumes on Thread B → buffer is from wrong ring!
                          → io_uring_prep_read_fixed() with Thread A's buffer index
                          → Thread B's ring doesn't know about that buffer
                          → FAILURE or fall back to non-registered I/O
```

### Use Case Analysis

#### Case 1: Sequential Copy (copy_read_write)
```rust
// Runs in one function, likely one thread
let mut buffer = vec![0u8; size];
while copying {
    read_at(buffer, offset).await;  // Might migrate here
    write_at(buffer, offset).await; // Or here
}
```

**Migration risk**: MEDIUM
- Multiple await points in loop
- compio could reschedule between reads
- But probably stays on same thread (work-stealing is lazy)

#### Case 2: Parallel Copy (copy_region_sequential)
```rust
// Each region dispatched independently
dispatcher.dispatch(move || {
    copy_region_sequential(start, end);  // Whole region on one thread?
});
```

**Migration risk**: LOW per region, HIGH across regions
- Each region probably stays on one thread
- But different regions definitely on different threads
- Need buffer per region, not per thread

#### Case 3: Directory Traversal
```rust
// Metadata fetch
let metadata = statx_at(dirfd, filename).await;
// ... reschedule possible ...
// File copy (different thread possible!)
copy_file_internal(...).await;
```

**Migration risk**: HIGH
- Many await points
- Long-running operation
- Lots of opportunity for work-stealing

### Solution Options

#### Option A: ❌ Thread-Local Pools Only

```rust
thread_local! {
    static POOL: RefCell<BufferPool> = ...;
}
```

**Pros:**
- ✅ Perfect for io_uring registration (one pool per ring)
- ✅ No locking (thread-local is fast)

**Cons:**
- ❌ **BREAKS on task migration** - buffer from Thread A used on Thread B
- ❌ Can't register buffers safely (task might migrate)
- ❌ Defeats compio's work-stealing efficiency

**Verdict**: Doesn't work with compio's architecture

#### Option B: ❌ Global Pool with Registration

```rust
static POOL: BufferPool = ...;  // Shared across threads
```

**Pros:**
- ✅ Works with task migration

**Cons:**
- ❌ **Can't register with io_uring** - buffers not tied to specific ring
- ❌ Locking overhead (Mutex on every acquire)

**Verdict**: No zero-copy benefit, just allocation reuse

#### Option C: ⚠️ Global Pool + Thread-Local Registration

```rust
static GLOBAL_POOL: BufferPool = ...;  // Allocations shared

thread_local! {
    static RING_BUFFERS: RegisteredBufferSet = ...;  // Per-ring indices
}

// Acquire from global, try to use thread-local registration
let buffer = GLOBAL_POOL.acquire_io_buffer();
if let Some(index) = try_register_locally(buffer) {
    // Use registered I/O (fast path)
    read_fixed(fd, index, len);
} else {
    // Fall back to normal I/O
    read_at(fd, buffer, offset);
}
```

**Pros:**
- ✅ Works with migration (buffers portable)
- ✅ Opportunistic zero-copy when task stays on same thread

**Cons:**
- ⚠️ Complex: tracking registration state
- ⚠️ Unpredictable performance (depends on migration)
- ⚠️ Registration overhead (need to register/unregister dynamically)

**Verdict**: Complicated, unpredictable benefit

#### Option D: ✅ Global Pool WITHOUT Registration (Phase 1 Only)

```rust
static BUFFER_POOL: BufferPool = ...;  // Just allocation reuse

// Use normal (non-registered) I/O
let buffer = BUFFER_POOL.acquire_io_buffer();
src.read_at(buffer, offset).await;  // Works on any thread
```

**Pros:**
- ✅ Simple: Just allocation reuse
- ✅ Works with task migration
- ✅ Still get 25-40% improvement (allocation reduction)
- ✅ compio-compatible

**Cons:**
- ❌ No zero-copy (can't use registered buffers safely)
- ❌ Miss potential 10-20% additional gain

**Verdict**: Safe, significant benefit, simpler

#### Option E: ⚠️ Pin Tasks to Threads

```rust
// Somehow ensure tasks don't migrate
dispatcher.dispatch_pinned(move || { ... });
```

**Pros:**
- ✅ Could use thread-local pools
- ✅ Could register buffers

**Cons:**
- ❌ compio doesn't expose thread-pinning API
- ❌ Defeats work-stealing benefits
- ❌ Reduces concurrency efficiency
- ❌ Complex to implement correctly

**Verdict**: Breaks compio's architecture

### Recommendation: Hybrid Approach

**Phase 1: Global pool (no registration)**
- Simple, safe, works with migration
- Gets 25-40% from allocation reuse
- Good ROI for effort

**Phase 2: Investigate compio's thread model**
- Check if tasks actually migrate frequently
- Profile to see if registration would help
- Consider upstreaming task-pinning to compio
- Or accept that zero-copy isn't compatible with work-stealing

**Phase 3: Decide on registration**
- If tasks rarely migrate: Add thread-local registration
- If tasks migrate often: Skip registration, keep simple pool
- Measure actual benefit vs complexity

---

## Alternative Approaches Considered

### 1. ❌ Size-classed pools (Tiny/Small/Medium/Large)

**Rejected**: Overcomplicated! We only need two buffer types:
- I/O buffers (user-configured from CLI)
- Metadata buffers (fixed 4KB)

Adding more size classes adds complexity without benefit.

### 2. ✅ Global pool without registration (CHOSEN for Phase 1)

**Selected:**
- ✅ Simple: One global pool with two buffer types
- ✅ Safe: Works with compio's task migration
- ✅ Effective: 25-40% improvement from allocation reuse alone
- ✅ Foundation: Can add registration later if task-pinning becomes available

---

## Open Questions

1. **Does compio support io_uring buffer registration?**
   - Need to check compio source/docs
   - Likely need to add to compio-fs-extended

2. **Pool sizing: 2× concurrency enough for I/O pool?**
   - Profile to verify
   - May need 3× or 4× for high pipelining

3. **Should buffer pool be in TraversalContext or global?**
   - TraversalContext: Easy to pass, matches existing pattern
   - Global static: Simpler initialization
   - **Recommendation**: TraversalContext (consistent with dispatcher)

4. **Alignment requirements for io_uring registration?**
   - Investigate kernel requirements
   - May need page-aligned allocation (4KB minimum)

5. **How to handle oversized xattr values (> 4KB)?**
   - Fallback to heap allocation for large xattr
   - Log and track outliers
   - Most xattr fit in 4KB, so this is rare

---

## Success Criteria

### Phase 1 (Global Two-Pool System):
- ✅ Zero buffer allocations in parallel copy hot path
- ✅ Pool hit rate > 95% (simple two-pool design)
- ✅ 25-40% performance improvement on large files
- ✅ Memory usage bounded: ~8MB (concurrency=64, buffer_size=64KB)
- ✅ Thread-safe: Works correctly with compio task migration
- ✅ All tests passing
- ✅ Works with any user-configured buffer size

### Phase 2 (Investigation):
- ✅ Documented task migration frequency data
- ✅ Clear decision on registration feasibility
- ✅ If registration viable: prototype + benchmark
- ✅ If not viable: document why and move on

**Note**: io_uring registration may not be compatible with compio's work-stealing.  
We get the major benefit (25-40%) from allocation reuse alone.

---

## Next Steps

1. **Research**: Check compio for buffer registration support
2. **Prototype**: Implement basic BufferPool without registration
3. **Benchmark**: Measure allocation reduction
4. **Integrate**: Replace allocations in copy.rs
5. **Expand**: Add to parallel copy
6. **Test**: Comprehensive buffer pool tests
7. **Document**: Update performance documentation

---

## References

- io_uring registered buffers: https://kernel.dk/io_uring.pdf (Section 5.4)
- compio buffer traits: Check compio-buf crate
- Rust buffer pooling patterns: Check tokio::io::util::BufReader
- io_uring_sys: https://docs.rs/io-uring/latest/io_uring/

---

**Author**: AI Assistant  
**Reviewer**: TBD  
**Status**: Design phase

