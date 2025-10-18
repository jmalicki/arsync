#!/bin/bash
# fio baseline benchmarks for RAID array capability
# Establishes theoretical limits for comparison with arsync/rsync

set -euo pipefail

# Check for fio dependency
command -v fio >/dev/null 2>&1 || { 
    echo "Error: fio is required but not installed"
    echo "Install with: apt-get install fio (or yum install fio)"
    exit 1
}

TEST_DIR="${1:-/mnt/benchmark/fio-test}"
RESULTS_FILE="${2:-fio-results.txt}"

mkdir -p "$TEST_DIR"

echo "=== fio RAID Array Baseline Benchmarks ===" | tee "$RESULTS_FILE"
echo "Test directory: $TEST_DIR" | tee -a "$RESULTS_FILE"
echo "Date: $(date)" | tee -a "$RESULTS_FILE"
echo "" | tee -a "$RESULTS_FILE"

# Test 1: Sequential Write (like copy destination)
echo "=== Test 1: Sequential Write (4 jobs, 4GB each = 16GB) ===" | tee -a "$RESULTS_FILE"
fio --name=seq-write \
    --directory="$TEST_DIR" \
    --rw=write \
    --bs=2M \
    --size=4G \
    --numjobs=4 \
    --ioengine=io_uring \
    --iodepth=32 \
    --direct=1 \
    --group_reporting \
    --time_based=0 \
    --output-format=normal \
    | tee -a "$RESULTS_FILE"

echo "" | tee -a "$RESULTS_FILE"

# Test 2: Sequential Read (like copy source)
echo "=== Test 2: Sequential Read (4 jobs, 4GB each = 16GB) ===" | tee -a "$RESULTS_FILE"
fio --name=seq-read \
    --directory="$TEST_DIR" \
    --rw=read \
    --bs=2M \
    --size=4G \
    --numjobs=4 \
    --ioengine=io_uring \
    --iodepth=32 \
    --direct=1 \
    --group_reporting \
    --time_based=0 \
    --output-format=normal \
    | tee -a "$RESULTS_FILE"

echo "" | tee -a "$RESULTS_FILE"

# Test 3: Mixed Read/Write (like same-array copy)
echo "=== Test 3: Mixed Read 50% / Write 50% (simulates copy on same array) ===" | tee -a "$RESULTS_FILE"
fio --name=mixed-rw \
    --directory="$TEST_DIR" \
    --rw=randrw \
    --rwmixread=50 \
    --bs=2M \
    --size=4G \
    --numjobs=4 \
    --ioengine=io_uring \
    --iodepth=32 \
    --direct=1 \
    --group_reporting \
    --time_based=0 \
    --output-format=normal \
    | tee -a "$RESULTS_FILE"

echo "" | tee -a "$RESULTS_FILE"

# Test 4: High queue depth test (saturate the array)
echo "=== Test 4: High Concurrency (8 jobs, QD=128) ===" | tee -a "$RESULTS_FILE"
fio --name=high-concurrency \
    --directory="$TEST_DIR" \
    --rw=write \
    --bs=2M \
    --size=2G \
    --numjobs=8 \
    --ioengine=io_uring \
    --iodepth=128 \
    --direct=1 \
    --group_reporting \
    --time_based=0 \
    --output-format=normal \
    | tee -a "$RESULTS_FILE"

echo "" | tee -a "$RESULTS_FILE"

# Cleanup
[ -n "$TEST_DIR" ] && rm -rf "${TEST_DIR:?}"/*

echo "=== Summary ===" | tee -a "$RESULTS_FILE"
echo "Results saved to: $RESULTS_FILE" | tee -a "$RESULTS_FILE"
echo "" | tee -a "$RESULTS_FILE"
echo "Compare these numbers with arsync/rsync:" | tee -a "$RESULTS_FILE"
echo "- If fio shows >10 GB/s but arsync shows ~1 GB/s, we have room to improve" | tee -a "$RESULTS_FILE"
echo "- If fio also shows ~1 GB/s, the array is the bottleneck" | tee -a "$RESULTS_FILE"

