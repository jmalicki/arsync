# Buffer Pool Stacked PR Plan

**Base**: PR #91 (TraversalContext)  
**Goal**: Eliminate allocations + zero-copy for large I/O  
**Date**: October 21, 2025

---

## PR Stack Overview

```
PR #91 (TraversalContext)
  └─ PR #92: Design doc (current)
       └─ PR #93: Global buffer pool (allocation reuse)
            └─ PR #94: Thread-local LRU cache for registered I/O buffers
```

---

## PR #93: Global Buffer Pool (Allocation Reuse)

**Branch**: `feat/buffer-pool-global`  
**Stacked on**: PR #92 (design)

### Scope

**Implement two global pools:**

1. **I/O Buffer Pool** - User-configured size
   - Size: From CLI `--buffer-size` (default 64KB)
   - For: File read/write operations
   - Pre-allocate: 2× concurrency

2. **Metadata Buffer Pool** - Fixed 4KB
   - For: statx, readlink, xattr
   - Pre-allocate: 1× concurrency

**No io_uring registration** - just allocation reuse!

### Implementation

**Files:**
```
src/buffer_pool.rs          - NEW: BufferPool implementation
src/directory.rs            - Add buffer_pool to TraversalContext
src/copy.rs                 - Use pool in copy_read_write
src/copy.rs                 - Use pool in copy_region_sequential (FIX!)
tests/buffer_pool_tests.rs  - NEW: Pool tests
```

**Key changes:**

```rust
// src/buffer_pool.rs (new file)
pub struct BufferPool {
    io_pool: Arc<BufferSubPool>,
    metadata_pool: Arc<BufferSubPool>,
}

impl BufferPool {
    pub fn new(io_buffer_size: usize, concurrency: usize) -> Arc<Self>;
    pub fn acquire_io_buffer(&self) -> PooledBuffer;
    pub fn acquire_metadata_buffer(&self) -> PooledBuffer;
}

// src/directory.rs
pub struct TraversalContext {
    // ... existing fields ...
    pub buffer_pool: Arc<BufferPool>,  // NEW
}

// src/copy.rs - copy_region_sequential
// BEFORE: let buffer = vec![0u8; to_read];  // 🔥 ALLOCATES EVERY ITERATION
// AFTER:
let mut pooled = ctx.buffer_pool.acquire_io_buffer();
let mut buffer = pooled.take();
while offset < end {
    buffer.resize(to_read, 0);  // Reuse capacity
    let (bytes_read, buf) = src.read_at(buffer, offset).await;
    buffer = buf;
    // ...
}
pooled.restore(buffer);
// Auto-return on drop
```

### Testing

```bash
# Verify reuse
cargo test test_buffer_pool_reuse

# Verify thread safety
cargo test test_buffer_pool_concurrent

# Verify no allocations in hot path
cargo test --release -- --nocapture | grep "allocated"
```

### Success Metrics

- ✅ Zero allocations in `copy_region_sequential` hot path
- ✅ 50-70% reduction in total allocations
- ✅ 25-40% performance improvement on large files
- ✅ Thread-safe (passes concurrent tests)
- ✅ All existing tests still pass

### Estimated Impact

**Before** (1GB file, 8 threads, 64KB buffer):
- Allocations: ~16,384 (new buffer per chunk)
- Memory: Unbounded spikes

**After** (global pool):
- Allocations: ~16 (initial pool only)
- Memory: Bounded ~8.3MB
- Speed: **+25-40%** faster

---

## PR #94: Thread-Local LRU Cache for Registered I/O Buffers

**Branch**: `feat/buffer-pool-registered-io`  
**Stacked on**: PR #93 (global pool)

### Scope

**Add zero-copy ONLY for large I/O operations**

- ✅ Register I/O buffers (user-configured size, typically 64KB+)
- ❌ Don't register metadata buffers (too small, not worth it)

**Key insight:** Zero-copy only matters for large buffers!
- Copying 4KB metadata: ~few nanoseconds (negligible)
- Copying 64KB+ I/O data: microseconds per operation (worth optimizing)

### Architecture

**Two-tier system:**

```rust
// Global pool (from PR #93) - allocation reuse
static BUFFER_POOL: Arc<BufferPool> = ...;

// Per-thread Clock cache - registered buffer indices
thread_local! {
    static REGISTERED_IO_CACHE: RefCell<ClockBufferCache> = ...;
}

struct ClockBufferCache {
    /// Circular array of cache entries
    entries: Vec<CacheEntry>,
    
    /// Clock hand position
    hand: usize,
    
    /// Fast lookup: buffer_ptr → slot index
    map: HashMap<usize, usize>,
    
    /// io_uring ring for THIS thread
    ring: *mut io_uring,
}

struct CacheEntry {
    buffer_ptr: usize,   // 0 = empty
    ioring_index: u16,
    referenced: bool,    // Second-chance bit
}
```

### How It Works

```rust
// Acquire buffer from global pool
let mut pooled = BUFFER_POOL.acquire_io_buffer();
let buffer = pooled.take();

// Try to use registered I/O on current thread
REGISTERED_IO_BUFFERS.with(|reg| {
    let mut reg = reg.borrow_mut();
    if let Some(buf_index) = reg.get_or_register(&buffer) {
        // HIT: Buffer is registered with this thread's ring
        // Use zero-copy I/O
        file.read_at_fixed(buf_index, len, offset).await
    } else {
        // MISS: Cache full (unlikely) or first time on this thread
        // Fall back to normal I/O (still reuses allocation from pool!)
        file.read_at(buffer, offset).await
    }
});
```

### Registration Logic (Clock Algorithm)

```rust
impl ClockBufferCache {
    fn get_or_register(&mut self, buffer: &[u8]) -> Option<u16> {
        let ptr = buffer.as_ptr() as usize;
        
        // Already registered?
        if let Some(&slot) = self.map.get(&ptr) {
            self.entries[slot].referenced = true;  // Mark as used
            return Some(self.entries[slot].ioring_index);
        }
        
        // Need to register - find slot (empty or evict)
        let slot = self.find_slot_or_evict()?;
        
        // Register with io_uring
        let ioring_index = unsafe {
            io_uring_register_buffer_at_index(self.ring, buffer, slot as u16)?
        };
        
        // Install in cache
        self.entries[slot] = CacheEntry {
            buffer_ptr: ptr,
            ioring_index,
            referenced: true,  // Just used
        };
        self.map.insert(ptr, slot);
        
        Some(ioring_index)
    }
    
    fn find_slot_or_evict(&mut self) -> Option<usize> {
        // Sweep clock looking for victim
        for _ in 0..self.entries.len() {
            let entry = &mut self.entries[self.hand];
            
            // Empty slot? Use it
            if entry.buffer_ptr == 0 {
                let slot = self.hand;
                self.advance_hand();
                return Some(slot);
            }
            
            // Referenced? Give second chance
            if entry.referenced {
                entry.referenced = false;
                self.advance_hand();
                continue;
            }
            
            // Not referenced - evict!
            let slot = self.hand;
            self.evict_at(slot);
            self.advance_hand();
            return Some(slot);
        }
        
        // All referenced - evict current anyway (rare)
        let slot = self.hand;
        self.evict_at(slot);
        self.advance_hand();
        Some(slot)
    }
    
    fn advance_hand(&mut self) {
        self.hand = (self.hand + 1) % self.entries.len();
    }
    
    fn evict_at(&mut self, slot: usize) {
        let entry = &self.entries[slot];
        self.map.remove(&entry.buffer_ptr);
        unsafe {
            io_uring_unregister_buffer(self.ring, entry.ioring_index);
        }
        self.entries[slot].buffer_ptr = 0;  // Mark empty
    }
}
```

### Key Benefits of Clock Algorithm

✅ **Robust:**
- Handles any pool size / cache size ratio
- Adapts to 4, 8, 16+ worker threads
- Graceful eviction when needed

✅ **Efficient:**
- O(1) lookup via HashMap
- O(1) amortized eviction (usually finds victim on first sweep)
- Just one bool per entry (not timestamps)

✅ **Approximates LRU:**
- Hot buffers stay registered (referenced bit protects them)
- Cold buffers get evicted
- Second-chance prevents thrashing

✅ **Handles migration:**
- Task moves to new thread
- New thread's cache doesn't have buffer
- Registers it (or evicts something cold)
- No errors, graceful adaptation

### Performance Expectations

**Sequential copy** (one buffer reused):
- First chunk: Register buffer → use _fixed (zero-copy)
- Rest of file: Cache hit → use _fixed (zero-copy)
- **Benefit**: ✅ Full zero-copy!

**Parallel copy** (multiple threads):
- Each thread registers its buffers
- High cache hit rate within each thread
- Some misses on migration (fallback OK)
- **Benefit**: ✅ Most operations zero-copy

**Small files** (< 1MB):
- Might not benefit much (overhead of registration)
- Metadata operations: Never registered (too small)
- **Benefit**: ⚠️ Minimal, but no harm

### What We DON'T Register

❌ **Metadata buffers (4KB)**
- Too small for zero-copy to matter
- Kernel copy overhead: ~few nanoseconds
- Not worth the registration complexity

❌ **Tiny allocations**
- Stack buffers, temp strings, etc.
- Keep these simple

✅ **ONLY register I/O buffers**
- Large: 64KB, 128KB, 256KB, etc.
- Where kernel copy overhead actually matters

### Implementation Details

**In compio-fs-extended:**

```rust
// New file: src/registered_buffers.rs
use io_uring_sys::io_uring;
use lru::LruCache;

pub struct RegisteredBufferCache {
    cache: LruCache<usize, u16>,
    ring: *mut io_uring,
    registered_buffers: Vec<(*const u8, usize)>,  // Track what's registered
}

impl RegisteredBufferCache {
    pub fn new(ring: *mut io_uring, max_size: usize) -> Self;
    
    pub fn get_or_register(&mut self, buffer: &[u8]) -> Option<u16>;
    
    unsafe fn register_buffer(&mut self, buffer: &[u8]) -> io::Result<u16>;
    
    unsafe fn unregister_buffer(&mut self, index: u16);
}
```

**In src/copy.rs:**

```rust
// Helper to try registered I/O, fallback to normal
async fn read_at_maybe_registered(
    file: &File,
    buffer: Vec<u8>,
    offset: u64,
) -> BufResult<usize, Vec<u8>> {
    thread_local! {
        static REG_CACHE: RefCell<Option<RegisteredBufferCache>> = RefCell::new(None);
    }
    
    REG_CACHE.with(|cache| {
        if let Some(cache) = cache.borrow_mut().as_mut() {
            if let Some(index) = cache.get_or_register(&buffer) {
                // Use registered I/O
                return file.read_at_fixed(index, offset).await;
            }
        }
        // Fallback to normal I/O
        file.read_at(buffer, offset).await
    })
}
```

### Success Metrics for PR #94

- ✅ Zero-copy I/O for sequential copy (cache hit rate ~100%)
- ✅ High cache hit rate for parallel copy (>80%)
- ✅ Graceful fallback on cache miss (no errors)
- ✅ Additional 10-20% throughput improvement
- ✅ CPU reduction measurable (5-10%)
- ✅ Works correctly even with task migration

### Risks

**Risk 1: Cache thrashing**
- If too many buffers, constant eviction
- **Mitigation**: Size cache appropriately (256 entries per thread)

**Risk 2: Stale registrations**
- Buffer returned to pool, then freed
- Cache still has stale pointer
- **Mitigation**: Use buffer address + generation counter

**Risk 3: Migration overhead**
- Task migrates → cache miss → register again
- **Mitigation**: Acceptable - still falls back to normal I/O (which we already have)

---

## Complete PR Timeline

### PR #92: Design (CURRENT)
- ✅ Design document
- ✅ Analysis of thread migration
- ✅ Justification for approach

### PR #93: Global Pool (~1 week)
**Focus**: Allocation reuse

**Deliverables:**
1. Implement `src/buffer_pool.rs`
2. Two pools: I/O (user-size) + metadata (4KB)
3. Thread-safe with Mutex
4. Integrate into copy operations
5. Tests + benchmarks

**Expected**: 25-40% improvement

### PR #94: LRU Registered Buffer Cache (~1 week)
**Focus**: Zero-copy for large I/O only

**Deliverables:**
1. Thread-local LRU cache (per io_uring ring)
2. Register ONLY I/O buffers (skip metadata)
3. Automatic fallback on cache miss
4. Handle task migration gracefully
5. Benchmarks showing zero-copy benefit

**Expected**: Additional 10-20% improvement

---

## Decision Matrix: When to Use Each Optimization

| Operation | Buffer Type | Pool | Registration | Rationale |
|-----------|-------------|------|--------------|-----------|
| File read/write | I/O (64KB+) | ✅ Global | ✅ LRU cache | Large buffers, zero-copy matters |
| Parallel copy chunks | I/O (64KB+) | ✅ Global | ✅ LRU cache | Large buffers, high volume |
| statx | Metadata (4KB) | ✅ Global | ❌ Skip | Too small, copy overhead negligible |
| readlink | Metadata (4KB) | ✅ Global | ❌ Skip | Too small, rare operation |
| xattr | Metadata (4KB) | ✅ Global | ❌ Skip | Too small, rare operation |

**Key principle:** Only optimize what matters!
- Registration overhead only worth it for large buffers
- Small buffers: just reuse allocations, skip zero-copy

---

## Summary

**Simple, focused approach:**

1. **PR #93**: Global pool for everything → 25-40% faster
2. **PR #94**: Register only large I/O buffers → Additional 10-20% faster

**Total expected improvement:** 35-60% on large files with parallel copy

**Complexity:** Moderate
- Global pool is simple (just Mutex + VecDeque)
- LRU cache is well-understood pattern
- Graceful degradation on migration

