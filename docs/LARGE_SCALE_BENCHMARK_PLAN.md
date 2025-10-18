# Large-Scale Benchmark Plan

**Date:** October 18, 2025,  
**System:** 256GB RAM, RAID10 array  
**Goal:** Measure true I/O performance, not page cache

## Problem with Small Benchmarks

**Previous tests: 5GB dataset**
- System RAM: 256GB
- Test data: 5GB
- **Result:** Entire dataset fits in page cache!
- **Measured:** Memory-to-memory copy speed, not disk I/O

## Solution: Exceed RAM Capacity

**New benchmark dataset: 400GB**
- 2× available RAM (256GB)
- Forces kernel to evict pages
- Measures true disk-to-disk performance

### Dataset Structure

**Option A: Many medium files (recommended)**
```text
100 files × 4GB = 400GB total

Why this works:
- Exceeds page cache (400GB > 256GB RAM)
- Realistic workload (mix of directory + file parallelism)
- Tests both arsync concurrency levels:
  * Directory: 100 files in flight
  * Per-file: 2-16 parallel tasks per file
```

**Option B: Few very large files**
```text
10 files × 40GB = 400GB total

Why this works:
- Each file is 40GB >> 256GB / 10 = much larger than per-file cache
- Better tests per-file parallelism
- Less directory concurrency (only 10 files)
```

**Recommendation: Use Option A** for more realistic mixed workload

## Benchmark Matrix

### Test Cases

1. **rsync baseline**
   ```bash
   rsync -a /mnt/benchmark/source-400gb/ /mnt/benchmark/dest-rsync/
   ```

2. **arsync sequential** (no parallel copy)
   ```bash
   arsync -a /mnt/benchmark/source-400gb/ /mnt/benchmark/dest-seq/
   ```

3. **arsync parallel depth 2** (4 tasks per file)
   ```bash
   arsync -a /mnt/benchmark/source-400gb/ /mnt/benchmark/dest-par2/ \
     --parallel-max-depth 2 \
     --parallel-min-size-mb 512
   ```

4. **arsync parallel depth 3** (8 tasks per file)
   ```bash
   arsync -a /mnt/benchmark/source-400gb/ /mnt/benchmark/dest-par3/ \
     --parallel-max-depth 3 \
     --parallel-min-size-mb 512
   ```

5. **arsync parallel depth 4** (16 tasks per file)
   ```bash
   arsync -a /mnt/benchmark/source-400gb/ /mnt/benchmark/dest-par4/ \
     --parallel-max-depth 4 \
     --parallel-min-size-mb 1024
   ```

## Cache Management

### Drop Page Cache Between Tests

**Problem:** Linux caches reads/writes in page cache

**Solution:**
```bash
# After each test, drop caches (requires root)
sudo sync
sudo sh -c 'echo 3 > /proc/sys/vm/drop_caches'

# Or if no root: Use direct I/O flags in fio for comparison
```

**Alternative without root:**
- Wait 5 minutes between tests
- Let kernel naturally evict old pages
- Use `free -h` to monitor cache usage

### Monitoring During Tests

**Watch page cache growth:**
```bash
# In separate terminal
watch -n 5 'free -h && echo "---" && df -h /mnt/benchmark'
```

**Watch I/O rates:**
```bash
iostat -xm 5 /dev/md127
```

## Dataset Creation

### Option A: 100 × 4GB files

```bash
#!/bin/bash
# Create 100 × 4GB = 400GB dataset
# ETA: ~45 minutes on this array

mkdir -p /mnt/benchmark/source-400gb

for i in {001..100}; do
    echo "Creating file $i/100 (4GB)..."
    dd if=/dev/urandom of=/mnt/benchmark/source-400gb/file_${i}.bin \
       bs=1M count=4096 \
       oflag=direct \
       2>&1 | grep -E "(copied|GB)"
done

echo "Dataset creation complete: 400GB"
du -sh /mnt/benchmark/source-400gb
```

**Creation time estimate:**
- 400GB at ~7 GB/s write = 57 seconds theoretical
- With urandom overhead: ~10-15 minutes realistic

### Option B: Use existing data (faster)

```bash
# If you have large files already, copy them
rsync -a /path/to/existing/large/dataset/ /mnt/benchmark/source-400gb/
```

## Expected Results

**Based on fio baseline (5.4-5.7 GB/s mixed workload):**

| Test | Expected Throughput | Expected Time | Notes |
|------|---------------------|---------------|-------|
| rsync | 365 MB/s | ~18 minutes | Baseline |
| arsync sequential | 2.6 GB/s | ~2.5 minutes | 7x faster |
| arsync parallel depth 2 | 4.0 GB/s | ~1.7 minutes | 11x faster |
| arsync parallel depth 3 | 4.5-5.0 GB/s | ~1.3-1.5 minutes | 12-14x faster |
| arsync parallel depth 4 | 5.0-5.5 GB/s | ~1.2-1.3 minutes | 13-15x faster |

**Key question:** Does depth 3 or 4 get us closer to fio's 5.4-5.7 GB/s limit?

## Success Criteria

1. **Throughput > 256GB / time** - Actual measured I/O
2. **Cache validation** - `free -h` shows cache growing to ~256GB then stabilizing
3. **Reproducible** - Multiple runs within 5% variance
4. **Data integrity** - SHA256 checksums match (sample check)
5. **Scaling analysis** - Document how throughput scales with depth

## Validation Steps

### 1. Verify We're Not Cached

```bash
# Before test: Note free cache
free -h | grep "Mem:" | awk '{print "Cache before: " $6}'

# Run test
time arsync -a /mnt/benchmark/source-400gb/ /mnt/benchmark/dest/

# After test: Cache should be near full
free -h | grep "Mem:" | awk '{print "Cache after: " $6}'

# If cache << 256GB, test was too fast or data was already cached
```

### 2. Sample Data Integrity

```bash
# Don't checksum all 400GB (takes forever)
# Sample 5 random files
for i in $(shuf -i 1-100 -n 5); do
    file=$(printf "file_%03d.bin" $i)
    sha256sum /mnt/benchmark/source-400gb/$file \
              /mnt/benchmark/dest*/$file
done | sort -k2 | uniq -c -f1
# Should show 2 of each (source + dest), all matching
```

## Timeline

**Total benchmark time estimate:**
1. Dataset creation: ~15 minutes (one time)
2. Each test run: ~1-20 minutes depending on tool
3. Cache drop between tests: ~30 seconds
4. Total for 5 tests: ~45-90 minutes

**Recommendation:** Run overnight or during lunch

## Script Template

```bash
#!/bin/bash
# Large-scale benchmark runner
set -euo pipefail

SOURCE="/mnt/benchmark/source-400gb"
RESULTS="/tmp/benchmark-results-$(date +%Y%m%d-%H%M%S).txt"

echo "=== Large-Scale Benchmark: 400GB Dataset ===" | tee "$RESULTS"
echo "Date: $(date)" | tee -a "$RESULTS"
echo "Source: $SOURCE" | tee -a "$RESULTS"
echo "" | tee -a "$RESULTS"

# Helper to drop caches (optional, needs root)
drop_caches() {
    if [ "$EUID" -eq 0 ]; then
        sync
        echo 3 > /proc/sys/vm/drop_caches
        echo "✓ Caches dropped"
    else
        echo "⚠ Skipping cache drop (no root)"
        sleep 60  # Wait for some natural eviction
    fi
}

# Test 1: rsync baseline
echo "=== Test 1: rsync baseline ===" | tee -a "$RESULTS"
drop_caches
rm -rf /mnt/benchmark/dest-rsync
time rsync -a $SOURCE/ /mnt/benchmark/dest-rsync/ 2>&1 | tee -a "$RESULTS"
echo "" | tee -a "$RESULTS"

# Test 2: arsync sequential
echo "=== Test 2: arsync sequential ===" | tee -a "$RESULTS"
drop_caches
rm -rf /mnt/benchmark/dest-seq
time ./target/release/arsync -a $SOURCE/ /mnt/benchmark/dest-seq/ 2>&1 | tee -a "$RESULTS"
echo "" | tee -a "$RESULTS"

# Test 3-5: arsync parallel (depths 2, 3, 4)
for depth in 2 3 4; do
    echo "=== Test $(($depth + 2)): arsync parallel depth $depth ===" | tee -a "$RESULTS"
    drop_caches
    rm -rf /mnt/benchmark/dest-par$depth
    time ./target/release/arsync -a $SOURCE/ /mnt/benchmark/dest-par$depth/ \
        --parallel-max-depth $depth \
        --parallel-min-size-mb 512 \
        2>&1 | tee -a "$RESULTS"
    echo "" | tee -a "$RESULTS"
done

echo "=== Benchmark Complete ===" | tee -a "$RESULTS"
echo "Results saved to: $RESULTS"
```

## Next Steps

1. Create 400GB dataset
2. Run benchmark script
3. Analyze results
4. Document findings
5. Create PR with benchmark results

Ready to proceed?

