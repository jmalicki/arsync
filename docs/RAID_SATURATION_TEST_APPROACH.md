# RAID Array Saturation Test - Detailed Approach

**Date:** October 18, 2025  
**Array:** md127 RAID10 - 4× Samsung 990 PRO 4TB  
**Goal:** Achieve >10 GB/s aggregate throughput by combining directory and file-level parallelism

## Hardware Capabilities

### Single Samsung 990 PRO
- Sequential Read: 7,450 MB/s (7.45 GB/s)
- Sequential Write: 6,900 MB/s (6.9 GB/s)

### RAID10 Array (4 drives, 2 offset copies)
- **Theoretical Read**: 14.9 GB/s (2× single drive, striped across 2 pairs)
- **Theoretical Write**: 13.8 GB/s (2× single drive)
- **Same-array copy**: Lower due to read+write contention

## Why Previous Tests Were Slow

**Single 2GB file test: 1.1 GB/s (only 8% of theoretical)**

Problems:
1. **Single file** - Can't leverage directory concurrency
2. **Same array** - Source and destination compete for same drives
3. **RAID contention** - MD layer may serialize operations
4. **Too small** - 2GB isn't enough to saturate 4 drives

## New Approach: Multi-File Stress Test

### Test Dataset
```
50 files × 4GB = 200GB total
```

**Why this works:**
- **50 files** - Enough for directory-level concurrency
- **4GB each** - Large enough for per-file parallelism to help
- **200GB total** - Substantial workload, ~14 seconds @ 15 GB/s

### Concurrency Breakdown

**arsync sequential** (baseline):
- Directory concurrency: `max_files_in_flight: 1024` (default)
- Per-file: 1 task (sequential copy)
- **Total I/O operations**: up to 1024 concurrent file copies

**arsync parallel depth 2:**
- Directory concurrency: 1024 files
- Per-file: 4 tasks (2^2)
- **Total I/O operations**: up to 1024 × 4 = **4096 concurrent I/O ops**

**arsync parallel depth 3:**
- Directory concurrency: 1024 files
- Per-file: 8 tasks (2^3)
- **Total I/O operations**: up to 1024 × 8 = **8192 concurrent I/O ops**

### Expected Bottleneck

With 4 NVMe drives and io_uring, we should be able to sustain thousands of concurrent operations. The limit will likely be:
1. **Filesystem locks** (XFS/ext4 internal serialization)
2. **MD RAID locks** (RAID10 consistency)
3. **Drive bandwidth** (~7 GB/s write per drive)

## Test Commands

### 1. rsync Baseline
```bash
time rsync -a /mnt/benchmark/parallel-test-src/ /mnt/benchmark/parallel-test-dest-rsync/
# Expected: 2-3 GB/s (200GB in ~70-100 seconds)
```

### 2. arsync Sequential (io_uring, no per-file parallelism)
```bash
time arsync -a /mnt/benchmark/parallel-test-src/ /mnt/benchmark/parallel-test-dest-seq/
# Expected: 4-6 GB/s (200GB in ~33-50 seconds)
# Faster due to io_uring + directory concurrency
```

### 3. arsync Parallel Depth 2 (4 tasks per file)
```bash
time arsync --parallel-max-depth 2 --parallel-min-size-mb 100 -a \
    /mnt/benchmark/parallel-test-src/ /mnt/benchmark/parallel-test-dest-par2/
# Expected: 6-10 GB/s (200GB in ~20-33 seconds)
# Each 4GB file split into 4 regions
# 50 files × 4 tasks = 200 concurrent operations
```

### 4. arsync Parallel Depth 3 (8 tasks per file)
```bash
time arsync --parallel-max-depth 3 --parallel-min-size-mb 100 -a \
    /mnt/benchmark/parallel-test-src/ /mnt/benchmark/parallel-test-dest-par3/
# Expected: 8-15 GB/s (200GB in ~13-25 seconds)
# Each 4GB file split into 8 regions
# 50 files × 8 tasks = 400 concurrent operations
# May hit diminishing returns or contention
```

## Data Verification

After each test:
```bash
# Verify file count matches
ls /mnt/benchmark/parallel-test-src/ | wc -l
ls /mnt/benchmark/parallel-test-dest-XXX/ | wc -l

# Verify total size matches
du -sh /mnt/benchmark/parallel-test-src/
du -sh /mnt/benchmark/parallel-test-dest-XXX/

# Verify sample files (spot check)
sha256sum /mnt/benchmark/parallel-test-src/file_001.bin /mnt/benchmark/parallel-test-dest-XXX/file_001.bin
sha256sum /mnt/benchmark/parallel-test-src/file_025.bin /mnt/benchmark/parallel-test-dest-XXX/file_025.bin
sha256sum /mnt/benchmark/parallel-test-src/file_050.bin /mnt/benchmark/parallel-test-dest-XXX/file_050.bin
```

## Metrics Calculation

```bash
# For each test:
TOTAL_GB=200
TIME_SECONDS=$(extract from 'time' output)
THROUGHPUT_GBPS=$(echo "scale=2; $TOTAL_GB / $TIME_SECONDS" | bc)

# Compare:
SPEEDUP_VS_RSYNC=$(echo "scale=2; $RSYNC_TIME / $ARSYNC_TIME" | bc)
```

## Memory Considerations

**arsync parallel depth 3:**
- Max concurrent files: 1024
- Tasks per file: 8
- Chunk size per task: 2MB
- **Peak memory**: 1024 × 8 × 2MB = 16GB (worst case)
- **Typical**: Much lower as files complete

With 247GB RAM, this is fine.

## Expected Outcomes

### Scenario A: Filesystem/RAID Bottleneck
- All arsync variants perform similarly (~4-6 GB/s)
- Parallel copy doesn't help due to MD/filesystem locks
- **Conclusion:** Single NVMe benefits more than RAID

### Scenario B: Successful Saturation
- arsync sequential: 5-7 GB/s
- arsync parallel depth 2: 8-12 GB/s
- arsync parallel depth 3: 10-15 GB/s
- **Conclusion:** Parallel copy scales well, approaches theoretical bandwidth

### Scenario C: Contention
- arsync parallel worse than sequential
- **Conclusion:** Too much concurrency causes thrashing

## Test Execution Order

1. ✅ Create 50 × 4GB files (in progress - ETA ~4 more minutes)
2. Run rsync baseline
3. Run arsync sequential
4. Run arsync parallel depth 2
5. Run arsync parallel depth 3
6. Verify data integrity (spot checks)
7. Calculate and compare throughput
8. Document findings

## Current Status

- Files created: 11/50 (44GB/200GB)
- Write speed during creation: ~365 MB/s
- ETA for completion: ~4 minutes
- All tests will run on same RAID array (md127)

Ready to execute when file creation completes!

