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

## Solution: Exceed RAM Capacity by 4×

**New benchmark dataset: 1TB (1024GB)**
- **4× available RAM** (247GB × 4 = 988GB ≈ 1TB)
- Guarantees kernel must evict pages continuously
- Measures true sustained disk-to-disk performance
- Leaves 5.3TB for destination copies

### Dataset Structure

**Option A: Many medium files (recommended)**
```text
250 files × 4GB = 1TB total

Why this works:
- Far exceeds page cache (1TB >> 247GB RAM)
- Realistic workload (mix of directory + file parallelism)
- Tests both arsync concurrency levels:
  * Directory: 250 files in flight
  * Per-file: 2-16 parallel tasks per 4GB file
- Sustainable: Each test uses ~1TB source + 1TB dest = 2TB
```

**Option B: Fewer very large files**
```text
100 files × 10GB = 1TB total

Why this works:
- Each file 10GB >> per-file cache allocation
- Better tests per-file parallelism (more regions to split)
- Still good directory concurrency (100 files)
```

**Option C: Maximum stress test**
```text
500 files × 4GB = 2TB total

- 8× RAM capacity
- Maximum stress on array
- Total space per test: 2TB source + 2TB dest = 4TB
- Still fits in 6.3TB available
```

**Recommendation: Start with Option A (1TB), can scale to Option C if needed**

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

### Option A: 250 × 4GB files = 1TB

```bash
#!/bin/bash
# Create 250 × 4GB = 1TB dataset (4× RAM)
# ETA: ~25-30 minutes on this array

mkdir -p /mnt/benchmark/source-1tb

for i in $(seq -f "%03g" 1 250); do
    echo "Creating file $i/250 (4GB)..."
    dd if=/dev/urandom of=/mnt/benchmark/source-1tb/file_${i}.bin \
       bs=1M count=4096 \
       oflag=direct \
       status=progress \
       2>&1 | tail -2
done

echo "Dataset creation complete: 1TB"
du -sh /mnt/benchmark/source-1tb
```

**Creation time estimate:**
- 1TB at ~7 GB/s write = 143 seconds theoretical
- With urandom overhead: ~25-30 minutes realistic

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

