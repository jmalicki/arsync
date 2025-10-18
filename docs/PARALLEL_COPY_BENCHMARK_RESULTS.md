# Parallel Copy Benchmark Results

**Date:** October 18, 2025,  
**Test Files:** 500MB and 1GB random data  
**System:** 64-core system (details in benchmark results)  
**Note:** Tests run without cache dropping (ALLOW_NO_ROOT=1) - actual performance may vary

## Quick Performance Test

### Configuration
- Source: `/tmp/bench-test-source/test-500mb.bin` (500MB)
- Sequential: `--parallel-max-depth 0` (default)
- Parallel depth 2: `--parallel-max-depth 2` (4 tasks)
- Parallel depth 3: `--parallel-max-depth 3` (8 tasks)
- Min size threshold: 100MB
- Chunk size: 2MB

### Results

| Tool / Configuration | Time (avg) | Throughput | Speedup vs arsync-seq | Speedup vs rsync |
|---------------------|-----------|------------|----------------------|------------------|
| **rsync** (baseline) | 0.609s | 821 MB/s | 1.00x | 1.00x |
| arsync Sequential | 0.611s | 818 MB/s | 1.00x | 1.00x (same) |
| arsync Parallel depth 2 (4 tasks) | 0.391s | 1279 MB/s | **1.56x** | **1.56x** |
| arsync Parallel depth 3 (8 tasks) | 0.388s | 1289 MB/s | **1.57x** | **1.57x** |

### Data Integrity

✅ **All checksums match perfectly**
```
470baec2840a852adaa65887b3e990fc62e30ab8578b0c5898cd79f5aa6fb58b (source)
470baec2840a852adaa65887b3e990fc62e30ab8578b0c5898cd79f5aa6fb58b (rsync copy)
470baec2840a852adaa65887b3e990fc62e30ab8578b0c5898cd79f5aa6fb58b (arsync sequential)
470baec2840a852adaa65887b3e990fc62e30ab8578b0c5898cd79f5aa6fb58b (arsync parallel depth 2)
470baec2840a852adaa65887b3e990fc62e30ab8578b0c5898cd79f5aa6fb58b (arsync parallel depth 3)
```

## 1GB File Test

| Tool / Configuration | Time | Throughput | Speedup vs rsync |
|---------------------|------|-----------|------------------|
| **rsync** (baseline) | 1.111s | 900 MB/s | 1.00x |
| arsync Parallel depth 2 (4 tasks) | 0.698s | 1433 MB/s | **1.59x** |

✅ **Checksums verified identical**

## Key Findings

### 🎯 **1.56-1.59x faster than rsync with parallel copy!**

**500MB file:**
- rsync: 0.609s (821 MB/s)
- arsync sequential: 0.611s (818 MB/s) - same as rsync
- arsync parallel depth 2: 0.391s (1279 MB/s) - **1.56x faster**

**1GB file:**
- rsync: 1.111s (900 MB/s)
- arsync parallel depth 2: 0.698s (1433 MB/s) - **1.59x faster**

### Other Observations

1. **Perfect data integrity** - All copies verified with SHA256 checksums
2. **Diminishing returns** - Depth 3 (8 tasks) only marginally better than depth 2 (4 tasks)
   - Likely hitting storage bandwidth limits
   - 500MB file may be too small to saturate more tasks
3. **Consistent performance** - Low variance across runs (~0.01s)
4. **Sequential parity** - arsync sequential matches rsync performance exactly
5. **No overhead** - Parallel code path adds no penalty when disabled

## Next Steps

1. Run full benchmark with proper cache control
2. Test with larger files (2GB+)
3. Test with different storage types
4. Validate on actual NVMe hardware

## Test Command Log

```bash
# Sequential
./target/release/arsync -a /tmp/bench-test-source/test-500mb.bin /tmp/bench-test-dest/test-seq.bin

# Parallel (4 tasks)  
./target/release/arsync --parallel-max-depth 2 --parallel-min-size-mb 100 -a \
    /tmp/bench-test-source/test-500mb.bin /tmp/bench-test-dest/test-parallel.bin
```

