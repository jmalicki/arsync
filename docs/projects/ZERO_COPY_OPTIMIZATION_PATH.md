# Zero-Copy Optimization Path

## Current Status

### What We Have (PR #94 + #95)

✅ **API is ready**: `write_managed_at()` exists and works  
⚠️ **Still copies**: Uses `Vec::from()` internally  
✅ **Tests pass**: All 96 tests + 5 zero-copy tests

```rust
// Current implementation
async fn write_managed_at(buf: BorrowedBuffer, pos: u64) -> Result<usize> {
    let data = buf.as_ref();
    let owned = Vec::from(data);  // ← ONE COPY HERE
    let buf_result = self.write_at(owned, pos).await;
    buf_result.0
}
```

**Performance**: Better than before (buffer pool reuse), but not true zero-copy yet.

---

## Why We Can't Optimize Further (Yet)

### The Challenge

To eliminate the `Vec::from()` copy, we need:

1. **Access to io_uring ring FD** - compio abstracts this away
2. **Buffer index in registration** - `BorrowedBuffer` doesn't expose this
3. **Custom OpCode implementation** - Need to submit `IORING_OP_WRITE_FIXED`
4. **Completion handling** - Poll and match buffer to completion

**All of these require compio internals access**.

### What's Needed in compio

```rust
// In compio_driver/src/iour/op.rs
pub struct WriteFixed<S> {
    fd: S,
    buffer_index: u16,  // ← Need from BorrowedBuffer
    offset: u64,
    len: u32,
    _p: PhantomPinned,
}

impl<S: AsFd> OpCode for WriteFixed<S> {
    fn create_entry(self: Pin<&mut Self>) -> OpEntry {
        let fd = self.fd.as_fd().as_raw_fd();
        opcode::WriteFixed::new(
            Fd(fd),
            self.buffer_index,
        )
        .offset(self.offset)
        .build()
        .into()
    }
}
```

**This belongs in compio**, not arsync!

---

## Optimization Roadmap

### Phase 1: Measure Current Gains ← **PR #96**

**Goal**: Quantify performance with current implementation

1. **Benchmark suite**
   - Small files (1KB - 1MB)
   - Medium files (10MB - 100MB)
   - Large files (1GB - 10GB)
   - NVMe vs SSD vs HDD

2. **Metrics**
   - Throughput (GB/s)
   - CPU usage (%)
   - Memory bandwidth (perf counters)
   - io_uring queue depth

3. **Comparisons**
   - Baseline (no buffer pool)
   - read_managed only (PR #94)
   - read_managed + write_managed (PR #95)
   - rsync

4. **Analysis**
   - Where are we spending time?
   - Is Vec::from the bottleneck?
   - What's the theoretical max?

**Deliverable**: Benchmark results showing current performance

### Phase 2: Upstream to compio ← **PR to compio**

**Goal**: Add write_fixed support to compio itself

1. **RFC to compio maintainers**
   - Explain use case (zero-copy file copying)
   - Show performance data from Phase 1
   - Propose API design

2. **Implement in compio**
   - Add `WriteFixed` OpCode
   - Expose buffer index in `BorrowedBuffer`
   - Add `write_fixed_at()` method
   - Tests and documentation

3. **Get it merged**
   - Address review feedback
   - Wait for release

**Deliverable**: `write_fixed` available in compio

### Phase 3: Use compio's write_fixed ← **PR to arsync**

**Goal**: Replace our Vec::from with compio's write_fixed

1. **Update to new compio version**
2. **Replace implementation**:
   ```rust
   async fn write_managed_at(buf: BorrowedBuffer, pos: u64) -> Result<usize> {
       // Use compio's write_fixed (true zero-copy!)
       self.write_fixed_at(buf, pos).await
   }
   ```
3. **Benchmark again** - measure actual zero-copy gains
4. **Enable by default**

**Deliverable**: True zero-copy in production

### Phase 4: Tune and Optimize ← **Future PRs**

**Goal**: Squeeze out every last bit of performance

1. **Buffer pool tuning**
   - Optimal buffer count (128? 256?)
   - Optimal buffer size per workload
   - Dynamic adjustment

2. **Parallel writes**
   - Submit multiple write_fixed ops
   - Pipelined read → write
   - Out-of-order completion

3. **Advanced features**
   - `IORING_SETUP_IOPOLL` for polling mode
   - `IORING_SETUP_SQPOLL` for kernel thread
   - Direct I/O (`O_DIRECT`)

**Deliverable**: Maximum possible throughput

---

## Timeline

| Phase | Duration | Blocker | Owner |
|-------|----------|---------|-------|
| **1. Benchmarks** | 1 week | None | arsync team |
| **2. Upstream** | 4-8 weeks | compio review | compio maintainers |
| **3. Integration** | 3 days | Phase 2 complete | arsync team |
| **4. Tuning** | Ongoing | Phase 3 complete | arsync team |

**Total to true zero-copy**: ~2-3 months (depends on compio review)

---

## Alternative: Direct io_uring (Not Recommended)

We *could* bypass compio and use io_uring directly:

```rust
// Direct io_uring access (BAD IDEA)
use io_uring::{opcode, types, IoUring};

async fn write_fixed_direct(
    uring: &mut IoUring,
    fd: i32,
    buf: &BorrowedBuffer,
    offset: u64,
    buf_index: u16,
) -> Result<usize> {
    let entry = opcode::WriteFixed::new(
        types::Fd(fd),
        buf.as_ptr(),
        buf.len() as u32,
        offset,
    )
    .buf_index(buf_index)
    .build();
    
    unsafe {
        uring.submission().push(&entry)?;
        uring.submit_and_wait(1)?;
        
        let cqe = uring.completion().next().unwrap();
        Ok(cqe.result() as usize)
    }
}
```

**Why this is bad**:

- ❌ Loses compio's async abstraction
- ❌ Requires managing our own io_uring ring
- ❌ No work-stealing runtime integration  
- ❌ Platform-specific (Linux only)
- ❌ High maintenance burden
- ❌ Conflicts with compio's ring

**Don't do this!** Wait for compio to add write_fixed properly.

---

## Recommendation

### For PR #96

**Focus on measurement, not optimization**:

1. ✅ Add comprehensive benchmarks
2. ✅ Measure current performance (read_managed + write_managed with Vec::from)
3. ✅ Document optimization path
4. ✅ Create RFC for compio
5. ❌ Don't hack direct io_uring access

### Why Benchmarks First?

We need data to answer:
- **How much does Vec::from cost?** (is it 10% or 50% of runtime?)
- **What's our current throughput?** (baseline for future comparison)
- **Is buffer pool helping?** (vs. raw Vec allocation)
- **Where's the bottleneck?** (CPU? disk? memory bandwidth?)

**Without data, optimization is guesswork!**

---

## Success Criteria

### Phase 1 (Benchmarks - PR #96)
- [ ] Measure throughput on 1GB, 10GB files
- [ ] Compare: no pool vs read_managed vs read+write_managed
- [ ] Identify Vec::from overhead with perf counters
- [ ] Document results in benchmark report

### Phase 2 (Upstream)
- [ ] RFC accepted by compio maintainers
- [ ] write_fixed implemented in compio
- [ ] Released in stable compio version

### Phase 3 (Integration)
- [ ] arsync uses compio's write_fixed
- [ ] Measured 2-5× throughput improvement
- [ ] Zero memory copies confirmed (perf counters)

---

## References

### io_uring Documentation
- [Write Fixed Buffers](https://unixism.net/loti/tutorial/fixed_buffers.html)
- [IORING_OP_WRITE_FIXED](https://kernel.dk/io_uring.pdf) - Section 5.7

### compio
- [Contributing Guide](https://github.com/compio-rs/compio/blob/master/CONTRIBUTING.md)
- [OpCode trait](https://docs.rs/compio-driver/latest/compio_driver/trait.OpCode.html)

### Performance Tools
- `perf stat` for memory bandwidth counters
- `perf record` for hotspot analysis
- `bpftrace` for io_uring tracing

---

**Current status**: API ready, tests passing, waiting for benchmarks  
**Next**: Create benchmarking PR (#96)  
**Blocker**: None - benchmarks can run on current implementation  
**Timeline**: 1 week for comprehensive benchmark suite

