# RAID Array Saturation Test Plan

**Date:** October 18, 2025,  
**Array:** md127 RAID10 with 4× Samsung 990 PRO 4TB  
**Goal:** Approach theoretical bandwidth with combined directory + per-file parallelism

## Hardware Specs

**Array Configuration:**
- 4× Samsung 990 PRO 4TB NVMe drives
- RAID10 (2 offset copies)
- 512K chunk size
- Total capacity: 6.9TB

**Single Drive Specs:**
- Sequential Read: 7,450 MB/s
- Sequential Write: 6,900 MB/s
- Interface: PCIe 4.0 x4

**Theoretical Array Bandwidth:**
- **Read**: 14-30 GB/s (2-4x single drive)
- **Write**: 7-14 GB/s (1-2x single drive, RAID10 overhead)

## Test Dataset

**50 files × 4GB = 200GB total**

Why this size:
- Large enough to stress the array
- Each file benefits from parallel copy (>128MB threshold)
- Enough concurrency: 50 files × 4-8 tasks = 200-400 I/O operations
- Realistic for real-world workloads

## Test Matrix

### Test 1: rsync baseline
```bash
time rsync -a /mnt/benchmark/parallel-test-src/ /mnt/benchmark/parallel-test-dest-rsync/
```
Expected: ~2-3 GB/s (limited by single-threaded design)

### Test 2: arsync sequential (no parallel copy)
```bash
time arsync -a /mnt/benchmark/parallel-test-src/ /mnt/benchmark/parallel-test-dest-seq/
```
Expected: ~3-5 GB/s (io_uring + directory concurrency)

### Test 3: arsync with parallel copy (depth 2)
```bash
time arsync --parallel-max-depth 2 --parallel-min-size-mb 100 -a \
    /mnt/benchmark/parallel-test-src/ /mnt/benchmark/parallel-test-dest-par2/
```
Expected: ~5-10 GB/s (directory concurrency + per-file parallelism)

### Test 4: arsync aggressive (depth 3)
```bash
time arsync --parallel-max-depth 3 --parallel-min-size-mb 100 -a \
    /mnt/benchmark/parallel-test-src/ /mnt/benchmark/parallel-test-dest-par3/
```
Expected: ~8-15 GB/s (maximum parallelism)

## Metrics to Collect

1. **Aggregate throughput** - 200GB / total_time
2. **Per-file throughput** - Should be consistent
3. **Data integrity** - SHA256 verify sample files
4. **System stats**:
   - io_uring queue depth utilization
   - CPU usage
   - Per-drive IOPS (iostat)

## Success Criteria

✅ **Good**: >5 GB/s aggregate with parallel copy  
✅ **Great**: >10 GB/s aggregate  
✅ **Excellent**: >15 GB/s aggregate  

## Current Status

- File creation: 4/50 files complete (~14GB/200GB)
- ETA: ~6-7 minutes remaining
- Will run tests when complete

## Notes

- Running without sudo (no cache dropping) for initial test
- Can run with sudo later for more accurate results
- May need to tune max_files_in_flight for optimal concurrency

