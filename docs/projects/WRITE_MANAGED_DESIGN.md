# write_managed Design: Full Zero-Copy I/O

## Executive Summary

Design for `write_managed()` API to enable **true zero-copy writes** from registered io_uring buffers, completing the zero-copy story for file copying.

**Current state**: Zero-copy reads ✓, copy on write ✗  
**Goal**: Zero-copy reads ✓, zero-copy writes ✓

**Expected performance**: 2-5× speedup on large files vs. current implementation

---

## The Problem

### Current Limitation

```rust
// ✅ Zero-copy read (kernel DMA to registered buffer)
let borrowed_buf = file.read_managed_at(&pool, len, offset).await?;

// ❌ Write requires copy (BorrowedBuffer → Vec → kernel)
dst.write_at(Vec::from(borrowed_buf.as_ref()), offset).await?;
```

### Why This Happens

1. **Ownership semantics**: `write_at()` requires `IoBuf` trait with ownership
2. **Borrowed buffer**: `BorrowedBuffer` is already borrowed from pool (can't move)
3. **Async lifetime**: Buffer must outlive the async operation
4. **No transfer**: Can't transfer ownership of borrowed data

### Cost Analysis

For a 1GB file with 64KB chunks:
- **Chunks**: 16,384 operations
- **Copy overhead**: 1GB of memcpy operations
- **Cache pollution**: Evicts useful data from CPU cache
- **Memory bandwidth**: Wastes precious DRAM bandwidth

**Net**: We save 50% (read zero-copy), but lose 50% (write still copies)

---

## The Solution: write_managed()

### API Design

```rust
/// Write from a borrowed buffer without copying
/// 
/// The buffer is kept alive until the write completes, then returned to pool.
/// This enables zero-copy writes from registered io_uring buffers.
pub trait AsyncWriteManagedAt {
    /// Write data from a borrowed buffer at a specific offset
    ///
    /// # Parameters
    /// - `buf`: Borrowed buffer from BufferPool (will be held until write completes)
    /// - `pos`: File offset to write at
    ///
    /// # Returns
    /// Number of bytes written
    ///
    /// # Lifetime Management
    /// The borrowed buffer is held during the entire async operation and
    /// automatically returned to the pool after write completion.
    async fn write_managed_at<'pool>(
        &mut self,
        buf: BorrowedBuffer<'pool>,
        pos: u64,
    ) -> std::io::Result<usize>;
}
```

### Implementation Strategy

#### Option 1: Hold Buffer During Write (Recommended)

**Approach**: Keep `BorrowedBuffer` alive during the write operation

```rust
impl AsyncWriteManagedAt for File {
    async fn write_managed_at<'pool>(
        &mut self,
        buf: BorrowedBuffer<'pool>,
        pos: u64,
    ) -> std::io::Result<usize> {
        // Get the buffer slice (no copy!)
        let data = buf.as_ref();
        
        // Submit io_uring write op with buffer pointer
        // The buffer stays alive in this scope
        let (result, _buf) = unsafe {
            // SAFETY: buf stays alive until write completes
            // io_uring gets raw pointer to registered buffer
            self.write_at_raw(data.as_ptr(), data.len(), pos).await
        }?;
        
        // buf drops here → returns to pool after write completes
        Ok(result)
    }
}
```

**Key insight**: We can use the buffer's raw pointer in io_uring while keeping the `BorrowedBuffer` alive!

#### Option 2: Buffer Pinning

**Approach**: Pin the buffer for the duration of the operation

```rust
use std::pin::Pin;

impl AsyncWriteManagedAt for File {
    async fn write_managed_at<'pool>(
        &mut self,
        buf: BorrowedBuffer<'pool>,
        pos: u64,
    ) -> std::io::Result<usize> {
        // Pin the buffer to prevent moves
        let pinned = Pin::new(&buf);
        
        // Submit write with pinned buffer
        let result = self.write_at_pinned(pinned, pos).await?;
        
        // Unpin happens automatically on drop
        Ok(result)
    }
}
```

#### Option 3: BufferPool Cooperation

**Approach**: BufferPool "lends" buffer for write, gets it back on completion

```rust
impl BufferPool {
    /// Lend a buffer for writing (marks as in-use)
    fn lend_for_write(&self, buf: BorrowedBuffer) -> LentBuffer {
        // Mark buffer as "in write operation"
        // Can't be reallocated until write completes
        LentBuffer { inner: buf, pool: self }
    }
}

struct LentBuffer<'pool> {
    inner: BorrowedBuffer<'pool>,
    pool: &'pool BufferPool,
}

impl Drop for LentBuffer<'_> {
    fn drop(&mut self) {
        // Notify pool that write is complete
        // Buffer can be reused now
    }
}
```

---

## Detailed Design: Option 1 (Recommended)

### Architecture

```
┌─────────────────────────────────────────────────────────────┐
│  Application (arsync copy loop)                             │
└─────────────────────────────────────────────────────────────┘
                          │
                          │ read_managed_at()
                          ▼
┌─────────────────────────────────────────────────────────────┐
│  BufferPool (compio_runtime)                                │
│  - Registered buffers (128 × 64KB)                          │
│  - Returns BorrowedBuffer<'pool>                            │
└─────────────────────────────────────────────────────────────┘
                          │
                          │ BorrowedBuffer holds ref to pool
                          ▼
┌─────────────────────────────────────────────────────────────┐
│  write_managed_at(&mut file, buf, offset)                   │
│  - Keep buf alive during write                              │
│  - Use buf.as_ptr() for io_uring                            │
│  - buf drops → returns to pool                              │
└─────────────────────────────────────────────────────────────┘
                          │
                          │ write_at with raw ptr
                          ▼
┌─────────────────────────────────────────────────────────────┐
│  io_uring (kernel)                                          │
│  - DMA from registered buffer → disk                        │
│  - No copy!                                                 │
└─────────────────────────────────────────────────────────────┘
```

### Usage in arsync

```rust
// Full zero-copy copy loop
if use_buffer_pool {
    let pool = BufferPool::new(128, buffer_size)?;
    
    while total_copied < file_size {
        let to_read = std::cmp::min(buffer_size, (file_size - total_copied) as usize);
        
        // ✅ Zero-copy read (kernel → registered buffer)
        let borrowed_buf = src_file
            .read_managed_at(&pool, to_read, offset)
            .await?;
        
        let bytes_read = borrowed_buf.len();
        if bytes_read == 0 {
            break;
        }
        
        // ✅ Zero-copy write (registered buffer → kernel)
        // NEW: write_managed_at keeps borrowed_buf alive!
        let bytes_written = dst_file
            .write_managed_at(borrowed_buf, offset)  // ← No Vec::from!
            .await?;
        
        // borrowed_buf drops here → returns to pool
        
        total_copied += bytes_written as u64;
        offset += bytes_written as u64;
    }
}
```

**Key difference**: No `Vec::from(borrowed_buf.as_ref())` - buffer goes directly to kernel!

---

## Implementation Details

### In compio-fs-extended

Add new trait and implementation:

```rust
// crates/compio-fs-extended/src/write_managed.rs

use compio::runtime::BorrowedBuffer;
use std::io::Result;

/// Extension trait for zero-copy writes from borrowed buffers
pub trait AsyncWriteManagedAt {
    /// Write from a borrowed buffer without copying
    async fn write_managed_at<'pool>(
        &mut self,
        buf: BorrowedBuffer<'pool>,
        pos: u64,
    ) -> Result<usize>;
}

impl AsyncWriteManagedAt for compio::fs::File {
    async fn write_managed_at<'pool>(
        &mut self,
        buf: BorrowedBuffer<'pool>,
        pos: u64,
    ) -> Result<usize> {
        use compio::io::AsyncWriteAt;
        
        // Get buffer data without copying
        let data = buf.as_ref();
        
        // Create a write operation that holds the buffer
        // This is safe because:
        // 1. buf stays alive for the entire async operation
        // 2. io_uring gets pointer to registered buffer
        // 3. No reallocation can happen (buffer is borrowed)
        let write_op = WriteFromBorrowed {
            file: self,
            buffer: buf,  // Holds BorrowedBuffer<'pool>
            offset: pos,
        };
        
        write_op.await
    }
}

/// Write operation that holds a borrowed buffer
struct WriteFromBorrowed<'a, 'pool> {
    file: &'a mut compio::fs::File,
    buffer: BorrowedBuffer<'pool>,
    offset: u64,
}

impl<'a, 'pool> std::future::Future for WriteFromBorrowed<'a, 'pool> {
    type Output = Result<usize>;
    
    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        // Get raw pointer to buffer data
        let data = self.buffer.as_ref();
        
        // Submit write_at with the buffer data
        // Buffer stays alive because we hold BorrowedBuffer
        // SAFETY: buffer won't move or be freed during operation
        unsafe {
            // Create io_uring write operation with buffer pointer
            // compio will keep the buffer alive until completion
            self.file.write_at_raw(data, self.offset).poll(cx)
        }
    }
}
```

### Safety Considerations

**Critical invariants**:

1. **Buffer lifetime**: `BorrowedBuffer<'pool>` must outlive the write operation
2. **No reallocation**: Buffer pool can't reallocate until buffer is returned
3. **No moves**: Buffer data must not move during async operation
4. **Completion ordering**: Write must complete before buffer is returned

**How we ensure safety**:

```rust
async fn write_managed_at<'pool>(
    &mut self,
    buf: BorrowedBuffer<'pool>,  // ← Holds borrow for 'pool lifetime
    pos: u64,
) -> Result<usize> {
    // buf is alive here
    let data = buf.as_ref();
    
    // Submit write (data pointer is valid)
    let result = self.write_at_raw(data.as_ptr(), data.len(), pos).await?;
    
    // buf still alive here (not dropped yet)
    Ok(result)
    
    // buf drops at end of scope → returns to pool
    // Write is already complete, so this is safe
}
```

**Rust's ownership system guarantees**:
- Buffer can't be moved (behind `&mut` or `Pin`)
- Buffer can't be freed (held by `BorrowedBuffer`)
- Buffer can't be reallocated (pool tracks in-use buffers)

---

## Performance Analysis

### Without write_managed (Current)

```
Read:  Disk → Kernel → Registered Buffer (zero-copy) ✓
       |                     |
       |                     | Vec::from() ← COPY!
       |                     ▼
Write: Registered Buffer → Vec → Kernel → Disk
```

**Copies per 1GB file (64KB chunks)**:
- Reads: 0 copies (zero-copy DMA)
- Writes: 16,384 × 64KB = 1GB of memcpy
- **Total: 1GB copied**

### With write_managed (Proposed)

```
Read:  Disk → Kernel → Registered Buffer (zero-copy) ✓
                            │
                            │ Direct pointer!
                            ▼
Write: Registered Buffer → Kernel → Disk (zero-copy) ✓
```

**Copies per 1GB file (64KB chunks)**:
- Reads: 0 copies (zero-copy DMA)
- Writes: 0 copies (zero-copy DMA)
- **Total: 0 bytes copied** 🎉

### Expected Performance Gain

| Metric | Current (read-only zero-copy) | With write_managed | Improvement |
|--------|-------------------------------|-------------------|-------------|
| Memory copies | 1GB per 1GB file | 0 bytes | **100% reduction** |
| Memory bandwidth | ~8 GB/s consumed | ~1 GB/s (disk only) | **8× less** |
| CPU cache pollution | High | Minimal | **Significant** |
| Large file throughput | ~2 GB/s | ~5 GB/s | **2.5× faster** |
| NVMe saturation | 50% | 95%+ | **Near-maximum** |

**Why such big gains**:
1. **No memcpy overhead**: Saves CPU cycles
2. **No cache pollution**: Data never enters CPU cache
3. **Direct DMA**: Disk → Registered Buffer → Disk
4. **Full NVMe bandwidth**: No CPU bottleneck

---

## Implementation Plan

### Phase 1: Prototype (1 week)

1. **Add write_managed trait** to compio-fs-extended
   - Define `AsyncWriteManagedAt` trait
   - Implement for `compio::fs::File`
   - Add safety documentation

2. **Implement WriteFromBorrowed future**
   - Hold `BorrowedBuffer` during write
   - Submit io_uring write with raw pointer
   - Ensure buffer returns to pool on completion

3. **Basic testing**
   - Unit test: write_managed writes correct data
   - Safety test: buffer returns to pool
   - Integration test: full read/write cycle

### Phase 2: Integration (3 days)

1. **Update copy_read_write()**
   - Detect write_managed availability
   - Use write_managed when buffer pool enabled
   - Keep fallback for non-io_uring platforms

2. **Testing**
   - All existing tests pass
   - New test: verify zero-copy writes
   - Performance regression test

### Phase 3: Benchmarking (1 week)

1. **Micro-benchmarks**
   - 1GB file copy (SSD)
   - 10GB file copy (NVMe)
   - Small files (< 64KB)

2. **Real-world workloads**
   - Large directory copy
   - Mixed file sizes
   - Comparison with rsync

3. **Analysis**
   - Throughput measurements
   - CPU usage
   - Memory bandwidth
   - io_uring queue depth

### Phase 4: Documentation & Upstream (1 week)

1. **Documentation**
   - API documentation
   - Safety guarantees
   - Performance characteristics
   - Usage examples

2. **Upstream to compio**
   - Submit PR to compio project
   - Address review feedback
   - Get approval from maintainers

3. **arsync integration**
   - Update to new compio version
   - Enable by default
   - Add --disable-write-managed flag

---

## API Evolution

### v1: Basic write_managed

```rust
async fn write_managed_at(
    &mut self,
    buf: BorrowedBuffer<'pool>,
    pos: u64,
) -> Result<usize>
```

**Pros**: Simple, direct
**Cons**: Takes ownership of BorrowedBuffer

### v2: Reference-based

```rust
async fn write_managed_at(
    &mut self,
    buf: &BorrowedBuffer<'pool>,
    pos: u64,
) -> Result<usize>
```

**Pros**: Can reuse buffer immediately after
**Cons**: More complex lifetime management

### v3: Pinned

```rust
async fn write_managed_at_pinned(
    &mut self,
    buf: Pin<&BorrowedBuffer<'pool>>,
    pos: u64,
) -> Result<usize>
```

**Pros**: Explicit about no-move guarantee
**Cons**: More verbose usage

**Recommendation**: Start with v1 (ownership), evolve to v2 if needed

---

## Alternative Approaches Considered

### 1. Write via IoBuf wrapper

```rust
struct BorrowedBuf<'pool>(BorrowedBuffer<'pool>);

unsafe impl IoBuf for BorrowedBuf<'_> {
    // ...
}

// Then use normal write_at
dst.write_at(BorrowedBuf(buf), offset).await?;
```

**Pros**: Reuses existing write_at infrastructure
**Cons**: 
- `IoBuf` requires ownership semantics
- Can't implement correctly (violates IoBuf contract)
- Unsafe to return buffer while operation pending

### 2. Direct io_uring syscalls

```rust
// Bypass compio entirely
unsafe {
    let uring = get_thread_uring();
    submit_write_fixed(uring, fd, buf.as_ptr(), len, offset, buf_index);
}
```

**Pros**: Full control, maximum performance
**Cons**:
- Loses compio abstraction
- Must manage io_uring ring manually
- No cross-platform support
- High maintenance burden

### 3. Copy-on-write semantics

```rust
// Make a copy only if needed
dst.write_at_cow(buf, offset).await?;
```

**Pros**: Flexibility
**Cons**:
- Still copies in common case
- Complex API
- Doesn't solve the core problem

**Decision**: Go with dedicated `write_managed` API (clean, explicit, zero-copy guaranteed)

---

## Testing Strategy

### Unit Tests

```rust
#[compio::test]
async fn test_write_managed_basic() {
    let file = File::create("test.dat").await.unwrap();
    let pool = BufferPool::new(1, 4096).unwrap();
    
    // Fill buffer
    let mut buf = pool.acquire().await.unwrap();
    buf.as_mut().copy_from_slice(b"test");
    
    // Write via write_managed
    let written = file.write_managed_at(buf, 0).await.unwrap();
    assert_eq!(written, 4);
    
    // Verify data
    let contents = std::fs::read("test.dat").unwrap();
    assert_eq!(contents, b"test");
}

#[compio::test]
async fn test_buffer_returns_to_pool() {
    let file = File::create("test.dat").await.unwrap();
    let pool = BufferPool::new(1, 4096).unwrap();
    
    {
        let buf = pool.acquire().await.unwrap();
        file.write_managed_at(buf, 0).await.unwrap();
        // buf drops, returns to pool
    }
    
    // Should be able to acquire again
    let buf2 = pool.acquire().await.unwrap();
    assert!(buf2.len() > 0);
}
```

### Integration Tests

```rust
#[compio::test]
async fn test_full_zero_copy_cycle() {
    let src = File::open("large_file.dat").await.unwrap();
    let mut dst = File::create("copy.dat").await.unwrap();
    let pool = BufferPool::new(128, 65536).unwrap();
    
    let mut offset = 0;
    loop {
        // Zero-copy read
        let buf = src.read_managed_at(&pool, 65536, offset).await.unwrap();
        if buf.len() == 0 {
            break;
        }
        
        // Zero-copy write
        let written = dst.write_managed_at(buf, offset).await.unwrap();
        offset += written as u64;
    }
    
    // Verify files identical
    assert_files_equal("large_file.dat", "copy.dat");
}
```

### Performance Tests

```rust
#[test]
fn bench_write_managed_vs_regular() {
    // Compare write_managed vs write_at(Vec::from())
    let results = benchmark_file_copy(
        "10GB.dat",
        Methods::Both,
    );
    
    assert!(
        results.write_managed.throughput > results.regular.throughput * 1.5,
        "write_managed should be >1.5× faster"
    );
}
```

---

## Risks & Mitigation

### Risk 1: Buffer Lifetime Bugs

**Risk**: Buffer freed while write pending → use-after-free
**Mitigation**: 
- Rust's borrow checker prevents this at compile time
- `BorrowedBuffer<'pool>` lifetime tied to pool
- Extensive testing with miri/valgrind

### Risk 2: Performance Regression

**Risk**: Holding buffers longer reduces pool efficiency
**Mitigation**:
- Larger pool (128 buffers = plenty for parallel ops)
- Benchmark before/after
- Adaptive pool sizing

### Risk 3: Platform Compatibility

**Risk**: write_managed might not work on all platforms
**Mitigation**:
- Graceful fallback to regular write_at
- Runtime detection of io_uring support
- Clear error messages

### Risk 4: compio API Changes

**Risk**: Future compio versions break our implementation
**Mitigation**:
- Upstream to compio (make it official)
- Maintain backward compatibility
- Version pinning if needed

---

## Success Metrics

### Performance Goals

- [ ] 2× throughput on 1GB files (NVMe)
- [ ] 3× throughput on 10GB files (NVMe)
- [ ] 50% reduction in CPU usage
- [ ] 80% reduction in memory bandwidth usage

### Quality Goals

- [ ] Zero crashes in 1000-hour stress test
- [ ] All existing tests pass
- [ ] No memory leaks (valgrind clean)
- [ ] Thread-safe (miri clean)

### Adoption Goals

- [ ] Accepted into compio upstream
- [ ] Enabled by default in arsync
- [ ] Documented in compio examples
- [ ] Used by other compio applications

---

## Timeline

| Week | Milestone | Deliverables |
|------|-----------|--------------|
| 1 | Prototype | Working write_managed implementation |
| 2 | Integration | arsync uses write_managed |
| 3 | Testing | All tests pass, no regressions |
| 4 | Benchmarking | Performance data collected |
| 5 | Documentation | API docs, safety analysis |
| 6 | Upstream | PR submitted to compio |
| 7-8 | Review | Address feedback |
| 9 | Release | Ship in arsync |

**Total**: ~2 months from design to production

---

## References

### io_uring

- [io_uring buffer selection](https://kernel.dk/io_uring.pdf) - Section 5.6
- [Efficient IO with io_uring](https://kernel.dk/io_uring.pdf)
- [Fixed buffers](https://unixism.net/loti/tutorial/fixed_buffers.html)

### compio

- [compio BufferPool docs](https://docs.rs/compio-runtime/latest/compio_runtime/struct.BufferPool.html)
- [compio IoBuf trait](https://docs.rs/compio-buf/latest/compio_buf/trait.IoBuf.html)
- [compio architecture](https://github.com/compio-rs/compio)

### Rust Async

- [Pin and Unpin](https://doc.rust-lang.org/std/pin/index.html)
- [Future trait](https://doc.rust-lang.org/std/future/trait.Future.html)
- [Async book](https://rust-lang.github.io/async-book/)

---

## Appendix A: Full Example Code

See implementation in:
- `crates/compio-fs-extended/src/write_managed.rs` (trait definition)
- `src/copy.rs` (usage in arsync)
- `tests/zero_copy_tests.rs` (comprehensive tests)

## Appendix B: Benchmark Results

(To be filled after implementation)

## Appendix C: Safety Proof

(Formal verification of memory safety guarantees)

---

**Status**: Design complete, ready for implementation  
**Next**: Begin Phase 1 (Prototype)  
**Owner**: TBD  
**Target**: Q1 2026

