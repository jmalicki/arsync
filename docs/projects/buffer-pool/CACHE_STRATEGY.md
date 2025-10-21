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

### 5. Clock/Second-Chance ⭐

```rust
struct ClockCache {
    /// Circular array of (buffer_ptr, io_uring_index, referenced_bit)
    entries: Vec<CacheEntry>,
    /// Clock hand position
    hand: usize,
    /// Map for O(1) lookup: buffer_ptr → array index
    map: HashMap<usize, usize>,
}

struct CacheEntry {
    buffer_ptr: usize,
    ioring_index: u16,
    referenced: bool,  // Set on access, cleared on scan
}
```

**How it works:**
1. On access: Set `referenced = true`
2. On eviction needed: Scan from clock hand
   - If `referenced = true`: Clear it, advance hand (second chance!)
   - If `referenced = false`: Evict this one
3. O(1) amortized (usually find victim quickly)

**Pros:**
- ✅ Approximates LRU with much less overhead
- ✅ O(1) amortized eviction
- ✅ Simple reference bit (no timestamps)
- ✅ Handles any pool size gracefully
- ✅ Good for variable access patterns

**Cons:**
- ⚠️ Slightly more complex than no-eviction
- ⚠️ Needs circular buffer + HashMap

**Verdict**: ⭐⭐⭐⭐⭐ **BEST CHOICE!** 

**Why it's better than no-eviction:**
- Handles case where pool > cache limit
- Adapts to varying worker counts (4, 8, 16 threads)
- Still simple (~80 lines vs 150 for LRU)

---

## Decision Matrix

| Policy | Complexity | Lookup | Eviction | Handles Growth | Our Workload Fit |
|--------|------------|--------|----------|----------------|------------------|
| **LRU** | High | O(1) | O(n) | ✅ Yes | ⭐⭐⭐ Good, but complex |
| **FIFO** | Medium | O(1) | O(1) | ✅ Yes | ⭐⭐⭐ Evicts wrong buffers |
| **Random** | Low | O(1) | O(1) | ✅ Yes | ⭐⭐ Too unpredictable |
| **No Eviction** | Minimal | O(1) | N/A | ❌ Breaks if pool grows | ⭐⭐⭐⭐ Simple but fragile |
| **Clock** | **Medium** | **O(1)** | **O(1) amortized** | **✅ Yes** | **⭐⭐⭐⭐⭐ BEST!** |

---

## Recommended: Clock/Second-Chance Algorithm

### Why Clock is Optimal for Us

**Advantages over no-eviction:**
- ✅ Handles varying worker counts (4, 8, 16+ threads)
- ✅ Adapts if pool size grows beyond cache
- ✅ Robust to configuration changes

**Advantages over LRU:**
- ✅ Simpler: No timestamp tracking
- ✅ Faster: O(1) amortized eviction (vs O(n) scan)
- ✅ Less memory: One bit per entry vs counter

**How it works for our workload:**

**Sequential copy** (same buffer reused):
```
Use buffer A → referenced = true
Use buffer A → referenced = true (already set)
Use buffer A → referenced = true (no overhead)
→ Buffer stays hot, never evicted
```

**Many files** (buffers cycle):
```
File 1: buffer A → register, referenced = true
File 2: buffer B → register, referenced = true
...
File 128: buffer A again → referenced = true (was false, now true)
→ Clock hand sweeps past A, sees referenced=true, gives second chance
→ Hot buffers stay registered
```

**Migration** (task moves to new thread):
```
Thread A: has buffer X registered
Task migrates to Thread B
Thread B: doesn't have buffer X
→ Registers it (or evicts cold buffer if full)
→ Graceful handling
```

### Implementation: Clock Algorithm

```rust
thread_local! {
    static REGISTERED_IO_BUFFERS: RefCell<ClockBufferCache> = 
        RefCell::new(ClockBufferCache::new());
}

/// Clock/Second-Chance cache for registered buffers
struct ClockBufferCache {
    /// Circular array of cache entries
    entries: Vec<CacheEntry>,
    
    /// Clock hand position (next eviction candidate)
    hand: usize,
    
    /// Fast lookup: buffer_ptr → array index
    map: HashMap<usize, usize>,
    
    /// io_uring ring for this thread
    ring: *mut io_uring,
    
    /// Statistics
    stats: CacheStats,
}

struct CacheEntry {
    buffer_ptr: usize,      // Buffer address (or 0 if empty)
    ioring_index: u16,      // Registered index in io_uring
    referenced: bool,       // Reference bit for second-chance
}

impl ClockBufferCache {
    fn new(max_size: usize) -> Self {
        Self {
            entries: vec![CacheEntry::empty(); max_size],
            hand: 0,
            map: HashMap::with_capacity(max_size),
            ring: get_current_thread_ring(),
            stats: CacheStats::default(),
        }
    }
    
    fn get_or_register(&mut self, buffer: &[u8]) -> Option<u16> {
        let ptr = buffer.as_ptr() as usize;
        
        // Fast path: Already registered?
        if let Some(&slot) = self.map.get(&ptr) {
            self.entries[slot].referenced = true;  // Mark as used
            self.stats.hits += 1;
            return Some(self.entries[slot].ioring_index);
        }
        
        self.stats.misses += 1;
        
        // Find empty slot or evict using clock algorithm
        let slot = self.find_slot_or_evict()?;
        
        // Register buffer with io_uring
        let ioring_index = unsafe {
            io_uring_register_buffer_at_index(self.ring, buffer, slot as u16)?
        };
        
        // Install in cache
        self.entries[slot] = CacheEntry {
            buffer_ptr: ptr,
            ioring_index,
            referenced: true,
        };
        self.map.insert(ptr, slot);
        self.stats.registrations += 1;
        
        Some(ioring_index)
    }
    
    fn find_slot_or_evict(&mut self) -> Option<usize> {
        // Make one full sweep of the clock
        for _ in 0..self.entries.len() {
            let entry = &mut self.entries[self.hand];
            
            // Empty slot? Use it!
            if entry.buffer_ptr == 0 {
                let slot = self.hand;
                self.hand = (self.hand + 1) % self.entries.len();
                return Some(slot);
            }
            
            // Referenced? Give second chance
            if entry.referenced {
                entry.referenced = false;
                self.hand = (self.hand + 1) % self.entries.len();
                continue;
            }
            
            // Not referenced - evict this one
            let slot = self.hand;
            self.evict_entry(slot);
            self.hand = (self.hand + 1) % self.entries.len();
            self.stats.evictions += 1;
            return Some(slot);
        }
        
        // All entries referenced - evict at current hand anyway
        let slot = self.hand;
        self.evict_entry(slot);
        self.hand = (self.hand + 1) % self.entries.len();
        self.stats.forced_evictions += 1;
        Some(slot)
    }
    
    fn evict_entry(&mut self, slot: usize) {
        let entry = &self.entries[slot];
        self.map.remove(&entry.buffer_ptr);
        unsafe {
            io_uring_unregister_buffer_at_index(self.ring, entry.ioring_index);
        }
    }
}
```

**Complexity**: ~80 lines vs ~150 for LRU vs ~50 for no-eviction

**Performance**:
- Lookup: O(1) via HashMap
- Eviction: O(1) amortized (usually finds victim quickly)
- Reference bit update: O(1) (just set bool)
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

## Decision: Clock/Second-Chance Algorithm

### Why Clock is Best

**Compared to no-eviction:**
- ✅ Handles pool growth (if we increase concurrency)
- ✅ Adapts to varying worker counts (4, 8, 16 threads)
- ✅ Robust to configuration changes

**Compared to LRU:**
- ✅ Much simpler (no timestamps, no scanning)
- ✅ Same performance characteristics
- ✅ O(1) amortized eviction
- ✅ Approximates LRU well enough

**Compared to FIFO:**
- ✅ Keeps hot buffers registered (FIFO doesn't)
- ✅ Second-chance prevents premature eviction
- ✅ Better hit rates in practice

### Implementation Complexity

**Clock**: ~80 lines
```rust
- Circular array: 20 lines
- HashMap for lookup: 10 lines
- Clock sweep logic: 30 lines
- Eviction handling: 20 lines
```

**LRU**: ~150 lines
```rust
- Timestamp tracking: 30 lines
- Find-minimum logic: 40 lines
- Update tracking: 30 lines
- Eviction: 30 lines
- etc.
```

**No-eviction**: ~50 lines
```rust
- HashMap only: 30 lines
- Simple insert: 20 lines
- No eviction: 0 lines
```

**Verdict**: Clock is sweet spot - robust enough, not too complex

---

## Final Recommendation for PR #94

**Use Clock/Second-Chance algorithm:**

```rust
struct ClockBufferCache {
    entries: Vec<CacheEntry>,        // Fixed size array
    hand: usize,                     // Clock hand
    map: HashMap<usize, usize>,      // ptr → slot
    max_size: usize,                 // e.g., 1024/num_workers
}

// On buffer use: mark referenced
cache.get_or_register(buffer);  // Sets referenced = true

// On eviction: sweep from hand
// - Skip referenced (second chance)
// - Evict unreferenced
```

**Benefits:**
- Handles all worker counts (4, 8, 16+ threads)
- Adapts if pool size changes
- Approximates LRU well
- Simple enough to implement correctly
- Well-tested algorithm (used in OS page replacement)

