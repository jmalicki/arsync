# Zero-Copy Benchmarks

## Purpose

Measure performance gains from compio's BufferPool integration:
1. **Baseline**: Regular Vec allocation
2. **read_managed**: Zero-copy reads (PR #94)
3. **write_managed**: Zero-copy reads + writes (PR #95)

## Usage

```bash
# Run full benchmark suite
./benchmarks/zero_copy_bench.sh

# Results saved to:
benchmark-results-zerocopy-YYYYMMDD_HHMMSS/
```

## Test Matrix

| File Size | Purpose |
|-----------|---------|
| 1 MB | Small file overhead |
| 10 MB | Medium file baseline |
| 100 MB | Warm cache effects |
| 1 GB | Large file performance |
| 10 GB | NVMe saturation |

## Metrics

- **Throughput** (MB/s) - Primary metric
- **Elapsed time** (seconds) - Raw timing
- **CPU usage** (%) - Efficiency
- **Memory bandwidth** (GB/s) - via perf counters

## Expected Results

Based on design analysis:

| Scenario | Throughput | Improvement |
|----------|------------|-------------|
| Baseline (no pool) | 1.5 GB/s | - |
| read_managed (PR #94) | 2-2.5 GB/s | 1.3-1.7× |
| read+write_managed (PR #95) | 2-3 GB/s | 1.3-2× |
| True zero-copy (future) | 4-5 GB/s | 2.7-3.3× |

Note: Current write_managed still copies (Vec::from), so gains are limited.

## Analysis

### Memory Copies per GB

| Implementation | Copies | Overhead |
|----------------|--------|----------|
| Baseline | 2 GB | Read + Write |
| read_managed | 1 GB | Write only |
| write_managed (current) | 1 GB | Vec::from |
| write_managed (optimized) | 0 GB | None! |

### What to Look For

1. **Throughput increase** - Should see 30-50% gain with current implementation
2. **CPU usage decrease** - Less memcpy means less CPU
3. **Memory bandwidth** - Should drop with fewer copies

## Detailed Perf Analysis

```bash
# Memory bandwidth counters
perf stat -e mem_load_retired.l3_miss,mem_inst_retired.all_loads,mem_inst_retired.all_stores \\
    ./target/release/arsync large.dat copy.dat

# CPU cycles breakdown
perf record -g ./target/release/arsync large.dat copy.dat
perf report

# io_uring operations
sudo bpftrace benchmarks/trace_io_uring_ops.bt -c "./target/release/arsync large.dat copy.dat"
```

## Future Optimizations

Once we upstream write_fixed to compio:

1. **Eliminate Vec::from** - Direct write_fixed
2. **Parallel read+write** - Pipeline operations
3. **Registered buffer optimization** - Fixed buffer indices
4. **IOPOLL mode** - Bypass interrupts

**Expected**: 2-5× total improvement over baseline

## Comparison with rsync

```bash
# rsync baseline
time rsync -av --no-perms --no-owner --no-group large.dat copy.dat

# arsync with buffer pool
time ./target/release/arsync large.dat copy.dat
```

Expected: arsync 2-3× faster than rsync on large files

---

**Status**: Benchmarking framework ready  
**Next**: Run benchmarks on NVMe system  
**Timeline**: 1 week for full analysis

