# arsync vs rsync Benchmarking Plan

## Overview

Rigorous performance comparison between arsync and rsync using scientifically sound methodology to generate reproducible, statistically valid results.

## Hardware Setup

- **Source Array**: NVMe RAID array with test data (read-only during tests)
- **Destination Array**: NVMe RAID array for output (can be wiped between tests)
- **System**: 16-core system (adjust scripts based on actual core count)
- **OS**: Linux kernel 5.15+ (for io_uring support)

## Test Scenarios

### 1. Single Large File Performance
- **1 GB file** - Baseline large file performance
- **10 GB file** - Sustained throughput test
- **100 GB file** - Long-running performance stability

**Metrics**: Throughput (GB/s), CPU usage, memory usage

### 2. Many Small Files
- **10,000 × 1 KB** - Tiny file overhead
- **10,000 × 10 KB** - Small file performance  
- **100,000 × 10 KB** - Scale test for small files
- **1,000,000 × 1 KB** - Extreme scale small files

**Metrics**: Files/second, total throughput (MB/s), syscall overhead

### 3. Mixed Workloads
- **Linux kernel source tree** (~75K files, mixed sizes, deep hierarchy)
- **Node.js node_modules** (~100K+ small files, deep nesting)
- **Photo library** (1000 files, 1-20 MB each, flat structure)
- **Database backup** (Mix of large and small files)

**Metrics**: Overall time, throughput, file type breakdown

### 4. Deep Directory Trees
- **Depth 10, 1000 files** - Directory traversal overhead
- **Depth 20, 10000 files** - Extreme depth test
- **Wide tree** (1000 dirs × 100 files) - Breadth test

**Metrics**: Discovery time, copy time, total time

### 5. Hardlink Scenarios
- **10,000 files, 50% hardlinked** - Moderate hardlink density
- **10,000 files, 90% hardlinked** - Heavy hardlink usage
- **Deduplication scenario** - Similar to backup with many duplicates

**Metrics**: Pre-scan time (rsync), memory usage, total time

### 6. Metadata-Heavy Operations
- **10,000 files with xattrs** - Extended attributes overhead
- **Files with ACLs** - Complex permissions
- **Mixed ownership** - uid/gid preservation (requires root)

**Metrics**: Metadata ops/second, copy time vs metadata time

## rsync Parallelization Strategies

We must test rsync with optimal parallelization to ensure fair comparison:

### 1. **Single-threaded rsync (baseline)**
```bash
rsync -a /source/ /dest/
```

### 2. **GNU Parallel + rsync**
Most common parallelization approach:
```bash
find /source -type f | parallel -j 16 rsync -a {} /dest/{}
```

### 3. **parsyncfp (Parallel rsync)**
Advanced parallel rsync wrapper with chunking:
```bash
parsyncfp --NP=16 -a /source/ /dest/
```

### 4. **Manual directory splitting**
Split by top-level directories, run concurrent rsync:
```bash
for dir in /source/*/; do
  rsync -a "$dir" /dest/ &
done
wait
```

### 5. **rsync with ionice/nice optimization**
```bash
ionice -c 2 -n 0 nice -n -10 rsync -a /source/ /dest/
```

**Note**: We'll test all strategies and report the **best** rsync performance for fair comparison.

## Methodology

### Pre-Test Setup

1. **Verify RAID health**
   ```bash
   cat /proc/mdstat
   # Ensure no rebuild/resync in progress
   ```

2. **Generate test datasets** (one-time setup)
   ```bash
   # Use provided scripts in benchmarks/generate_testdata.sh
   ```

3. **Verify arsync correctness** (one-time validation)
   ```bash
   # Compare arsync output against rsync byte-for-byte
   diff -r /rsync-output/ /arsync-output/
   ```

### Per-Benchmark Procedure

1. **Drop caches** (requires root)
   ```bash
   sync
   echo 3 > /proc/sys/vm/drop_caches
   ```

2. **Clear destination**
   ```bash
   rm -rf /dest/*
   sync
   ```

3. **Wait for I/O quiesce**
   ```bash
   sleep 5
   ```

4. **Run benchmark with measurements**
   ```bash
   /usr/bin/time -v command_here 2>&1 | tee results.txt
   ```

5. **Collect metrics**
   - Wall clock time
   - User CPU time
   - System CPU time  
   - Maximum resident set size (memory)
   - Major page faults
   - File system outputs (via iostat)
   - Context switches

6. **Repeat 5 times** (discard first run as warm-up)

7. **Statistical analysis**
   - Calculate mean, median, std dev
   - Report confidence intervals
   - Identify and remove outliers (if justified)

### Automated Measurement

```bash
# Wrapper script captures all metrics
run_benchmark() {
  local name=$1
  local cmd=$2
  
  # Start iostat in background
  iostat -x 1 > "iostat_${name}.log" &
  local iostat_pid=$!
  
  # Start monitoring memory/CPU
  pidstat -h -r -u 1 > "pidstat_${name}.log" &
  local pidstat_pid=$!
  
  # Run actual benchmark
  /usr/bin/time -v bash -c "$cmd" 2>&1 | tee "time_${name}.log"
  
  # Stop monitoring
  kill $iostat_pid $pidstat_pid
  
  # Verify output
  check_correctness /source /dest
}
```

## Correctness Validation

Before performance numbers matter, verify correctness:

```bash
#!/bin/bash
# Compare outputs
diff -r /rsync-output/ /arsync-output/ || echo "MISMATCH!"

# Verify file count
rsync_count=$(find /rsync-output -type f | wc -l)
arsync_count=$(find /arsync-output -type f | wc -l)
[ "$rsync_count" -eq "$arsync_count" ] || echo "File count mismatch"

# Verify checksums
(cd /rsync-output && find . -type f -exec sha256sum {} \;) | sort > /tmp/rsync.sums
(cd /arsync-output && find . -type f -exec sha256sum {} \;) | sort > /tmp/arsync.sums
diff /tmp/rsync.sums /tmp/arsync.sums || echo "Checksum mismatch"

# Verify metadata (permissions, ownership, timestamps)
stat --format="%n %a %U %G %Y" /rsync-output/* > /tmp/rsync.meta
stat --format="%n %a %U %G %Y" /arsync-output/* > /tmp/arsync.meta
diff /tmp/rsync.meta /tmp/arsync.meta || echo "Metadata mismatch"
```

## Results Reporting

### Per-Scenario Results

```markdown
## Scenario: 10,000 × 10 KB files

| Tool | Mean (s) | Median (s) | Std Dev | Throughput (MB/s) | Memory (MB) |
|------|----------|------------|---------|-------------------|-------------|
| rsync (single) | X.XX | X.XX | X.XX | XXX | XXX |
| rsync (parallel) | X.XX | X.XX | X.XX | XXX | XXX |
| arsync | X.XX | X.XX | X.XX | XXX | XXX |

**Speedup**: arsync is XXx faster than best rsync
```

### Summary Table (for README)

Update README.md with actual measured values:

```markdown
| Workload | rsync | arsync | Speedup |
|----------|-------|--------|---------|
| 1 GB single file | X.XX GB/s | X.XX GB/s | X.XXx |
| 10,000 × 10 KB files | XXX MB/s | XXX MB/s | X.XXx |
| Deep directory tree | XXX MB/s | XXX MB/s | X.XXx |
| Mixed workload | XXX MB/s | XXX MB/s | X.XXx |
```

## Environmental Controls

### Critical Factors to Control

1. **CPU frequency scaling**
   ```bash
   # Set to performance mode
   for cpu in /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor; do
     echo performance > $cpu
   done
   ```

2. **Disable RAID auto-rebuild during tests**
   ```bash
   echo idle > /sys/block/md0/md/sync_action
   ```

3. **Disable background services**
   ```bash
   systemctl stop unattended-upgrades
   systemctl stop packagekit
   ```

4. **Use dedicated test window**
   - No other I/O workloads
   - No network traffic spikes
   - Monitor system load before starting

5. **Temperature throttling**
   - Monitor CPU/SSD temps
   - Pause if approaching thermal limits
   - Allow cooldown between heavy tests

## Statistical Rigor

### Minimum Standards

- **Sample size**: 5 runs per configuration (discard first)
- **Outlier detection**: Grubbs' test or IQR method
- **Confidence intervals**: Report 95% CI
- **Effect size**: Calculate Cohen's d for meaningful differences
- **Reproducibility**: Document exact kernel, tool versions

### Example Analysis

```python
import numpy as np
from scipy import stats

# Collect timing data
rsync_times = [10.2, 10.1, 10.3, 10.2, 10.1]  # seconds
arsync_times = [5.1, 5.2, 5.0, 5.1, 5.2]

# Calculate statistics
rsync_mean = np.mean(rsync_times)
arsync_mean = np.mean(arsync_times)

# T-test for significance
t_stat, p_value = stats.ttest_ind(rsync_times, arsync_times)

# Effect size (Cohen's d)
pooled_std = np.sqrt((np.var(rsync_times) + np.var(arsync_times)) / 2)
cohens_d = (rsync_mean - arsync_mean) / pooled_std

print(f"Speedup: {rsync_mean / arsync_mean:.2f}x")
print(f"p-value: {p_value:.4f}")
print(f"Effect size: {cohens_d:.2f}")
```

## Timeline

1. **Week 1**: Setup, test data generation, correctness validation
2. **Week 2**: Run benchmarks, collect data
3. **Week 3**: Statistical analysis, report writing
4. **Week 4**: Peer review, documentation updates

## Deliverables

1. **Raw data**: CSV files with all measurements
2. **Analysis scripts**: Python/R scripts for statistical analysis  
3. **Graphs**: Performance comparison charts
4. **Documentation**: Updated README.md with verified numbers
5. **Reproducibility package**: Scripts + data for independent verification

## References

- [Linux perf tools](http://www.brendangregg.com/linuxperf.html)
- [Systems Performance](http://www.brendangregg.com/systems-performance-2nd-edition-book.html) - Brendan Gregg
- [Rigorous Benchmarking in Reasonable Time](https://www.cse.unsw.edu.au/~gernot/benchmarking-crimes.html)
- [SPEC CPU Benchmarking Methodology](https://www.spec.org/cpu2017/Docs/overview.html)

