# Cache Strategy Analysis for Registered Buffers

**Question**: Is LRU the best cache design for registered buffers?

---

## Workload Characteristics

### Sequential Copy (One File)
```rust
let buffer = pool.acquire_io_buffer();  // Always same buffer
while copying {
    read_at(buffer, offset).await;
    write_at(buffer, offset).await;
}
```

**Access pattern**: ONE buffer used repeatedly
- First access: MISS (register it)
- All subsequent: HIT (100% hit rate)
- **Any cache policy works perfectly here**

### Parallel Copy (Large File, Multiple Threads)
```rust
// Thread A
copy_region(0MB → 100MB);    // Uses buffer A repeatedly

// Thread B  
copy_region(100MB → 200MB);  // Uses buffer B repeatedly

// Thread C
copy_region(200MB → 300MB);  // Uses buffer C repeatedly
```

**Access pattern**: Each thread uses a small set of buffers repeatedly
- Thread stays on same buffers for entire region
- Within-thread: 100% reuse
- **Any cache policy works well**

### Many Small Files
```rust
for file in 1000s_of_files {
    let buffer = pool.acquire_io_buffer();  // Random buffer from pool
    copy_file(buffer);
    // Release buffer
}
```

**Access pattern**: Buffers cycle through randomly
- Different buffer each time (pool round-robin)
- High turnover
- **Cache policy matters here, but...**
  - If pool size ≤ cache size: Everything registered, no eviction
  - If pool size > cache size: Constant misses anyway

---

## Cache Policy Options

### 1. LRU (Least Recently Used)

```rust
struct LruCache<K, V> {
    map: HashMap<K, (V, usize)>,  // key → (value, timestamp)
    access_counter: usize,
}
```

**Pros:**
- ✅ Good for temporal locality
- ✅ Well-understood, proven algorithm
- ✅ Handles changing access patterns

**Cons:**
- ❌ O(n) eviction (need to find minimum timestamp)
- ❌ Overhead: Track access time on every hit
- ❌ Overkill for our workload

**Verdict**: More complex than needed

---

### 2. FIFO (First In, First Out)

```rust
struct FifoCache {
    map: HashMap<usize, u16>,     // buffer ptr → index
    queue: VecDeque<usize>,       // Insertion order
}
```

**Pros:**
- ✅ O(1) eviction (pop front)
- ✅ Simple implementation
- ✅ No per-access overhead

**Cons:**
- ❌ Evicts oldest, not least-used
- ❌ Might evict hot buffer if it was registered first

**Verdict**: Simple but not optimal

---

### 3. Random Eviction

```rust
struct RandomCache {
    map: HashMap<usize, u16>,
    keys: Vec<usize>,  // For random selection
}
```

**Pros:**
- ✅ O(1) eviction
- ✅ Minimal overhead
- ✅ Surprisingly effective in practice

**Cons:**
- ❌ Unpredictable performance
- ❌ Might evict hot buffer by chance

**Verdict**: Too unpredictable

---

### 4. No Eviction (Register Until Full)

```rust
struct NoEvictionCache {
    map: HashMap<usize, u16>,     // buffer ptr → index
    next_index: u16,              // Next index to allocate
    max_size: usize,              // Stop at kernel limit
}

impl NoEvictionCache {
    fn get_or_register(&mut self, buffer: &[u8]) -> Option<u16> {
        let ptr = buffer.as_ptr() as usize;
        
        // Check if already registered
        if let Some(&index) = self.map.get(&ptr) {
            return Some(index);  // HIT
        }
        
        // Not registered - can we register?
        if self.map.len() < self.max_size {
            let index = self.next_index;
            unsafe { io_uring_register_buffer(self.ring, buffer, index); }
            self.map.insert(ptr, index);
            self.next_index += 1;
            return Some(index);
        }
        
        // Cache full - return None (fallback to normal I/O)
        None
    }
}
```

**Pros:**
- ✅ **Simplest possible**: Just a HashMap
- ✅ O(1) lookup, O(1) registration
- ✅ Zero eviction overhead
- ✅ Once registered, always registered (stable)
- ✅ Perfect for our workload (same buffers reused)

**Cons:**
- ⚠️ Once full, no new registrations
- ⚠️ Doesn't adapt if access pattern changes

**Verdict**: **BEST for our use case!**

**Why this works:**
1. Pool is bounded (e.g., 128 I/O buffers)
2. Kernel limit is 1024 buffers
3. We have plenty of space to register all pool buffers
4. Once registered, they stay hot (constant reuse)

---

### 5. Clock/Second-Chance

```rust
struct ClockCache {
    entries: Vec<(usize, u16, bool)>,  // (ptr, index, ref_bit)
    clock_hand: usize,
}
```

**Pros:**
- ✅ Approximates LRU with less overhead
- ✅ O(1) amortized eviction

**Cons:**
- ❌ More complex than needed
- ❌ Reference bit overhead

**Verdict**: Overkill

---

## Decision Matrix

| Policy | Complexity | Lookup | Eviction | Our Workload Fit |
|--------|------------|--------|----------|------------------|
| **LRU** | High | O(1) | O(n) | ⭐⭐⭐ Good, but overkill |
| **FIFO** | Medium | O(1) | O(1) | ⭐⭐ Suboptimal |
| **Random** | Low | O(1) | O(1) | ⭐ Too unpredictable |
| **No Eviction** | **Minimal** | **O(1)** | **N/A** | **⭐⭐⭐⭐⭐ Perfect!** |
| **Clock** | Medium | O(1) | O(1) | ⭐⭐⭐ Good, unnecessary |

---

## Recommended: No-Eviction Cache

### Why This is Optimal

**Math:**
```
Pool size:         ~128 I/O buffers (2× concurrency=64)
Kernel limit:      1024 registered buffers per ring
Space available:   1024 - 128 = 896 slots remaining
```

We have **plenty of space** to register all pool buffers!

**Access pattern**:
- Same ~128 buffers cycle through the pool
- Once all registered: 100% cache hit rate forever
- No eviction ever needed

**Benefits:**
- ✅ Simplest code: Just HashMap + counter
- ✅ Best performance: No eviction overhead
- ✅ Predictable: Once warm, always fast
- ✅ Stable: Registered buffers don't change

### Implementation

```rust
thread_local! {
    static REGISTERED_IO_BUFFERS: RefCell<RegisteredBufferMap> = 
        RefCell::new(RegisteredBufferMap::new());
}

/// Simple map: buffer address → io_uring index
/// No eviction - just register until we hit a reasonable limit
struct RegisteredBufferMap {
    /// Map buffer pointers to io_uring indices
    map: HashMap<usize, u16>,
    
    /// Next index to allocate
    next_index: u16,
    
    /// Max buffers to register (well under kernel's 1024 limit)
    max_registered: usize,  // e.g., 256
    
    /// Statistics
    hits: usize,
    misses: usize,
    registrations: usize,
}

impl RegisteredBufferMap {
    fn get_or_register(&mut self, buffer: &[u8]) -> Option<u16> {
        let ptr = buffer.as_ptr() as usize;
        
        // Already registered?
        if let Some(&index) = self.map.get(&ptr) {
            self.hits += 1;
            return Some(index);
        }
        
        self.misses += 1;
        
        // Can we register more?
        if self.map.len() < self.max_registered {
            let index = self.next_index;
            
            // Register with io_uring
            if let Ok(()) = unsafe { register_buffer(buffer, index) } {
                self.map.insert(ptr, index);
                self.next_index += 1;
                self.registrations += 1;
                return Some(index);
            }
        }
        
        // Cache full or registration failed - use normal I/O
        None
    }
}
```

### Cache Sizing

**Conservative**: 256 buffers per thread
- For 4 worker threads: 256 × 4 = 1024 total (at kernel limit)
- For 8 worker threads: 256 × 8 = 2048 needed (exceeds limit!)
  - **Solution**: 128 per thread (1024 total, at limit)

**Recommendation**: 
```rust
let max_per_thread = 1024 / num_worker_threads;  // Stay under kernel limit
// e.g., 8 threads → 128 buffers/thread
```

---

## Comparison: LRU vs No-Eviction

### Scenario: 128-buffer pool, 256-buffer cache

**LRU:**
```
Register buffer #1-128: All fit in cache
Register buffer #129: Evict LRU (buffer #1), register #129
Use buffer #1 again: MISS, evict something else, re-register #1
...
Constant eviction/re-registration churn
```

**No-Eviction:**
```
Register buffer #1-128: All fit in cache
Register buffer #129: Cache full (128/256), but we only have 128 buffers!
                      → Never reaches this (pool size < cache size)
All subsequent: 100% hits, zero eviction
```

**Winner**: No-eviction (when pool size ≤ cache size)

---

## When Would LRU Be Better?

**Only if**: Pool size > Cache size

Example:
- Pool: 512 buffers
- Cache limit: 256 buffers per thread
- **Then** LRU helps: Keep hot 256 registered, evict cold ones

**But our case:**
- Pool: ~128 buffers (2× concurrency)
- Cache limit: ~128-256 per thread
- Pool fits entirely in cache!
- **No eviction ever needed**

---

## Alternative: Lazy Full Registration

Even simpler approach:

```rust
thread_local! {
    static REGISTERED: RefCell<HashMap<usize, u16>> = RefCell::new(HashMap::new());
}

// Just register everything as it's seen, until we hit limit
fn try_use_registered(buffer: &[u8]) -> Option<u16> {
    REGISTERED.with(|reg| {
        let mut reg = reg.borrow_mut();
        let ptr = buffer.as_ptr() as usize;
        
        if let Some(&index) = reg.get(&ptr) {
            return Some(index);  // Already registered
        }
        
        if reg.len() < 256 {
            // Register it
            let index = reg.len() as u16;
            register_buffer(buffer, index);
            reg.insert(ptr, index);
            return Some(index);
        }
        
        None  // Cache full, use normal I/O
    })
}
```

**Even simpler**: No eviction logic at all!

---

## Recommendation

### For PR #94: Use Simple HashMap (No Eviction)

**Rationale:**
1. Pool size (128) < Cache limit (256)
2. All pool buffers fit in cache
3. No eviction ever needed
4. Simpler code, better performance

**If pool grows beyond cache:**
- Could add simple FIFO eviction later
- But unlikely: pool sized to concurrency (bounded)

**Alternative if we want to be future-proof:**
- Start with no-eviction
- Log warning if we hit limit
- Add eviction only if we observe it being a problem

---

## Implementation Recommendation

```rust
/// Simple registered buffer cache (no eviction needed)
struct RegisteredBufferCache {
    /// Map: buffer pointer → io_uring index
    registered: HashMap<usize, u16>,
    
    /// Next index to allocate (increments)
    next_index: u16,
    
    /// Max to register (per-thread limit to stay under kernel 1024)
    max_size: usize,
    
    /// Stats
    hits: usize,
    misses: usize,
    fallbacks: usize,  // When cache is full
}

impl RegisteredBufferCache {
    fn get_or_register(&mut self, buffer: &[u8]) -> Option<u16> {
        let ptr = buffer.as_ptr() as usize;
        
        // Check if already registered
        if let Some(&index) = self.registered.get(&ptr) {
            self.hits += 1;
            return Some(index);
        }
        
        self.misses += 1;
        
        // Try to register (if space available)
        if self.registered.len() < self.max_size {
            if let Ok(index) = self.register_with_uring(buffer) {
                self.registered.insert(ptr, index);
                return Some(index);
            }
        }
        
        // Cache full - fallback to normal I/O
        self.fallbacks += 1;
        None
    }
}
```

**Complexity**: O(1) lookup, O(1) registration, **no eviction logic**

**When this breaks**: Only if pool grows beyond cache limit
- Easy to detect: `fallbacks` counter increases
- Easy to fix: Increase cache limit or add FIFO eviction

---

## Comparison

| Approach | Code Lines | Complexity | Our Use Case |
|----------|------------|------------|--------------|
| **LRU** | ~150 | High (timestamp tracking, find-min) | ⭐⭐⭐ Overkill |
| **FIFO** | ~80 | Medium (queue management) | ⭐⭐⭐ Unnecessary |
| **Clock** | ~120 | Medium (ref bits, scanning) | ⭐⭐⭐ Unnecessary |
| **No-Eviction** | **~50** | **Minimal (just HashMap)** | **⭐⭐⭐⭐⭐ Perfect!** |

---

## Decision

### Start with No-Eviction (Simplest)

**Phase 1 (PR #94):**
- Simple HashMap cache
- Register until full (256 per thread)
- No eviction
- Log warnings if fallbacks occur

**Phase 2 (If needed):**
- If `fallbacks` metric shows problems
- Add simple FIFO eviction
- Or just increase cache size

**Rationale:**
- Pool is bounded and small
- Cache can hold entire pool
- No eviction needed in practice
- Simpler code is better code

---

## Final Recommendation

**Replace "LRU cache" with "Registration cache" or just "Buffer registration map"**

The cache doesn't need eviction logic - it's not really an LRU cache, it's just a:
- **Registration tracking map**
- Register buffers as seen (up to limit)
- No eviction needed (pool fits in limit)
- Fallback to normal I/O if somehow we overflow

Much simpler, same or better performance!

