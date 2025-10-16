#!/bin/bash
# Rigorous benchmarking script for arsync vs rsync
# Designed for high-performance NVMe RAID arrays (>15 GB/s capable)
#
# Usage: sudo ./run_benchmarks.sh /source/array/benchmark-data /dest/array/benchmark-output /results/dir

set -euo pipefail

# Configuration
SOURCE_DIR="${1:-/mnt/source-nvme/benchmark-data}"
DEST_DIR="${2:-/mnt/dest-nvme/benchmark-output}"
RESULTS_DIR="${3:-./benchmark-results-$(date +%Y%m%d_%H%M%S)}"
# Convert to absolute path to avoid issues when changing directories
RESULTS_DIR="$(readlink -f "$RESULTS_DIR" 2>/dev/null || (mkdir -p "$RESULTS_DIR" && cd "$RESULTS_DIR" && pwd))"
NUM_RUNS=5  # Run each test 5 times, discard first (warm-up)
CPUS=$(nproc)
ENABLE_POWER_MONITORING="${ENABLE_POWER_MONITORING:-no}"  # Set to "yes" to enable power monitoring

# Paths to binaries (use absolute paths!)
RSYNC_BIN=$(which rsync)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
ARSYNC_BIN="${ARSYNC_BIN:-$PROJECT_ROOT/target/release/arsync}"

# Check root
if [ "$EUID" -ne 0 ]; then
    echo "ERROR: This script requires root (for dropping caches)"
    echo "Usage: sudo $0 [source] [dest] [results]"
    exit 1
fi

# Validate
if [ ! -d "$SOURCE_DIR" ]; then
    echo "ERROR: Source directory not found: $SOURCE_DIR"
    echo "Run ./generate_testdata.sh first"
    exit 1
fi

if [ ! -x "$ARSYNC_BIN" ]; then
    echo "ERROR: arsync binary not found: $ARSYNC_BIN"
    echo "Run: cargo build --release"
    exit 1
fi

mkdir -p "$RESULTS_DIR"

# Create documented results structure
if [ -f "$(dirname "$0")/create_results_structure.sh" ]; then
    bash "$(dirname "$0")/create_results_structure.sh" "$RESULTS_DIR" "$SOURCE_DIR" "$DEST_DIR"
fi

cd "$RESULTS_DIR"

echo "=== Benchmark Configuration ==="
echo "Source: $SOURCE_DIR"
echo "Destination: $DEST_DIR"
echo "Results: $RESULTS_DIR"
echo "CPUs: $CPUS"
echo "Runs per test: $NUM_RUNS"
echo "rsync: $RSYNC_BIN ($($RSYNC_BIN --version | head -1))"
echo "arsync: $ARSYNC_BIN"
echo ""

# System info
echo "=== Comprehensive Hardware Inventory ===" | tee system_info.txt
if [ -f "$(dirname "$0")/hardware_inventory.sh" ]; then
    echo "Running hardware detection..." | tee -a system_info.txt
    bash "$(dirname "$0")/hardware_inventory.sh" hardware_detailed.txt
    cat hardware_detailed.txt >> system_info.txt
    echo "✓ Hardware inventory complete (see hardware_detailed.txt)"
else
    echo "Basic system info:" | tee -a system_info.txt
    uname -a | tee -a system_info.txt
    cat /proc/cpuinfo | grep "model name" | head -1 | tee -a system_info.txt
    free -h | tee -a system_info.txt
    lsblk | tee -a system_info.txt
    cat /proc/mdstat | tee -a system_info.txt || echo "No MD RAID found"
fi
echo ""

# Drop caches and prepare destination
prepare_test() {
    echo "  Preparing test environment..."
    
    # Clear destination
    rm -rf "${DEST_DIR:?}"/*
    sync
    
    # Drop caches (CRITICAL for fair comparison)
    echo "  → Dropping caches (echo 3 > /proc/sys/vm/drop_caches)..."
    echo 3 > /proc/sys/vm/drop_caches
    echo "  → Caches dropped - testing COLD performance"
    
    # Wait for I/O to quiesce
    sleep 3
    
    # Verify no other I/O activity
    local io_wait=$(iostat -x 1 2 | tail -1 | awk '{print $NF}')
    echo "  → I/O wait: ${io_wait}%"
}

# Run a single benchmark
run_benchmark() {
    local name=$1
    local source=$2
    local dest=$3
    local command=$4
    local run_num=$5
    
    local output_prefix="${name}_run${run_num}"
    
    echo "    Run $run_num: $name"
    
    prepare_test
    
    # Start background monitoring
    iostat -x 1 > "${output_prefix}_iostat.log" &
    local iostat_pid=$!
    
    # Optional: Start power monitoring (if available)
    if [ -f "$(dirname "$0")/power_monitoring.sh" ] && [ "$ENABLE_POWER_MONITORING" = "yes" ]; then
        "$(dirname "$0")/power_monitoring.sh" "${output_prefix}_power.csv" &
        local power_pid=$!
    else
        local power_pid=""
    fi
    
    # Run benchmark with time measurement
    local start_time=$(date +%s.%N)
    
    /usr/bin/time -v bash -c "$command" \
        > "${output_prefix}_stdout.log" \
        2> "${output_prefix}_time.log"
    
    local exit_code=$?
    local end_time=$(date +%s.%N)
    
    # Stop monitoring
    kill $iostat_pid 2>/dev/null || true
    wait $iostat_pid 2>/dev/null || true
    
    if [ -n "$power_pid" ]; then
        kill $power_pid 2>/dev/null || true
        wait $power_pid 2>/dev/null || true
    fi
    
    # Verify completion and show errors immediately
    if [ $exit_code -ne 0 ]; then
        echo ""
        echo "    ❌ ERROR: Command failed with exit code $exit_code"
        echo "    Command: $command"
        echo ""
        echo "    Error details:"
        tail -20 "${output_prefix}_time.log" | grep -E "error|Error|ERROR|No such|cannot|failed" | sed 's/^/      /'
        echo ""
        echo "    Full logs:"
        echo "      stdout: ${output_prefix}_stdout.log"
        echo "      stderr: ${output_prefix}_time.log"
        echo ""
        return 1
    fi
    
    # Calculate elapsed time
    local elapsed=$(echo "$end_time - $start_time" | bc)
    echo "$elapsed" > "${output_prefix}_elapsed.txt"
    
    # Count files
    local file_count=$(find "$dest" -type f 2>/dev/null | wc -l)
    echo "$file_count" > "${output_prefix}_filecount.txt"
    
    # Calculate total size
    local total_size=$(du -sb "$dest" 2>/dev/null | cut -f1)
    echo "$total_size" > "${output_prefix}_bytes.txt"
    
    # Calculate throughput
    if [ "$elapsed" != "0" ]; then
        local throughput=$(echo "scale=2; $total_size / $elapsed / 1024 / 1024 / 1024" | bc)
        echo "${throughput} GB/s" > "${output_prefix}_throughput.txt"
        echo "    Throughput: ${throughput} GB/s ($file_count files, ${elapsed}s)"
    fi
    
    return 0
}

# Run multiple iterations
run_test_suite() {
    local name=$1
    local source=$2
    local command=$3
    
    echo "  Testing: $name"
    echo "  Source: $source"
    echo "  Command: $command"
    
    local suite_dir="${RESULTS_DIR}/${name}"
    mkdir -p "$suite_dir"
    cd "$suite_dir"
    
    for run in $(seq 1 $NUM_RUNS); do
        run_benchmark "$name" "$source" "$DEST_DIR" "$command" "$run"
    done
    
    # Statistical analysis
    echo "  Analyzing results..."
    python3 << 'EOF'
import glob
import numpy as np

# Load elapsed times (skip first run as warm-up)
times = []
for f in sorted(glob.glob('*_elapsed.txt'))[1:]:  # Skip run 1
    with open(f) as fp:
        times.append(float(fp.read().strip()))

if times:
    print(f"  Times: {times}")
    print(f"  Mean: {np.mean(times):.3f}s")
    print(f"  Median: {np.median(times):.3f}s")
    print(f"  Std Dev: {np.std(times):.3f}s")
    print(f"  Min: {np.min(times):.3f}s")
    print(f"  Max: {np.max(times):.3f}s")
    
    # Save summary
    with open('summary.txt', 'w') as f:
        f.write(f"mean={np.mean(times):.3f}\n")
        f.write(f"median={np.median(times):.3f}\n")
        f.write(f"stddev={np.std(times):.3f}\n")
        f.write(f"min={np.min(times):.3f}\n")
        f.write(f"max={np.max(times):.3f}\n")
EOF
    
    cd "$RESULTS_DIR"
}

# Set CPU to performance mode
echo "=== Setting CPU to performance mode ==="
for cpu in /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor; do
    if [ -f "$cpu" ]; then
        echo performance > "$cpu"
    fi
done

echo ""
echo "========================================"
echo "===  SCENARIO 1: Large Single Files ==="
echo "========================================"
echo ""

# 100GB file
run_test_suite "01_rsync_100gb" \
    "$SOURCE_DIR/single-large-files/100GB.dat" \
    "$RSYNC_BIN -a '$SOURCE_DIR/single-large-files/100GB.dat' '$DEST_DIR/'"

run_test_suite "02_arsync_100gb" \
    "$SOURCE_DIR/single-large-files/100GB.dat" \
    "$ARSYNC_BIN -a '$SOURCE_DIR/single-large-files/100GB.dat' '$DEST_DIR/'"

# 200GB file
run_test_suite "03_rsync_200gb" \
    "$SOURCE_DIR/single-large-files/200GB.dat" \
    "$RSYNC_BIN -a '$SOURCE_DIR/single-large-files/200GB.dat' '$DEST_DIR/'"

run_test_suite "04_arsync_200gb" \
    "$SOURCE_DIR/single-large-files/200GB.dat" \
    "$ARSYNC_BIN -a '$SOURCE_DIR/single-large-files/200GB.dat' '$DEST_DIR/'"

# 500GB file (if available)
if [ -f "$SOURCE_DIR/single-large-files/500GB.dat" ]; then
    run_test_suite "05_rsync_500gb" \
        "$SOURCE_DIR/single-large-files/500GB.dat" \
        "$RSYNC_BIN -a '$SOURCE_DIR/single-large-files/500GB.dat' '$DEST_DIR/'"
    
    run_test_suite "06_arsync_500gb" \
        "$SOURCE_DIR/single-large-files/500GB.dat" \
        "$ARSYNC_BIN -a '$SOURCE_DIR/single-large-files/500GB.dat' '$DEST_DIR/'"
fi

echo ""
echo "========================================="
echo "===  SCENARIO 2: Many Small Files     ==="
echo "========================================="
echo ""

# 10k × 1KB files
run_test_suite "07_rsync_10k_tiny" \
    "$SOURCE_DIR/tiny-files-10k/" \
    "$RSYNC_BIN -a '$SOURCE_DIR/tiny-files-10k/' '$DEST_DIR/'"

run_test_suite "08_arsync_10k_tiny" \
    "$SOURCE_DIR/tiny-files-10k/" \
    "$ARSYNC_BIN -a '$SOURCE_DIR/tiny-files-10k/' '$DEST_DIR/'"

# 100k × 1KB files
run_test_suite "09_rsync_100k_tiny" \
    "$SOURCE_DIR/small-files-100k/" \
    "$RSYNC_BIN -a '$SOURCE_DIR/small-files-100k/' '$DEST_DIR/'"

run_test_suite "10_arsync_100k_tiny" \
    "$SOURCE_DIR/small-files-100k/" \
    "$ARSYNC_BIN -a '$SOURCE_DIR/small-files-100k/' '$DEST_DIR/'"

# 1M × 1KB files (extreme scale test)
if [ -d "$SOURCE_DIR/small-files-1m/" ]; then
    run_test_suite "11_rsync_1m_tiny" \
        "$SOURCE_DIR/small-files-1m/" \
        "$RSYNC_BIN -a '$SOURCE_DIR/small-files-1m/' '$DEST_DIR/'"
    
    run_test_suite "12_arsync_1m_tiny" \
        "$SOURCE_DIR/small-files-1m/" \
        "$ARSYNC_BIN -a '$SOURCE_DIR/small-files-1m/' '$DEST_DIR/'"
fi

# 10k × 10KB files
run_test_suite "13_rsync_10k_small" \
    "$SOURCE_DIR/small-files-10k-each/" \
    "$RSYNC_BIN -a '$SOURCE_DIR/small-files-10k-each/' '$DEST_DIR/'"

run_test_suite "14_arsync_10k_small" \
    "$SOURCE_DIR/small-files-10k-each/" \
    "$ARSYNC_BIN -a '$SOURCE_DIR/small-files-10k-each/' '$DEST_DIR/'"

# 10k × 100KB files (medium)
run_test_suite "15_rsync_10k_medium" \
    "$SOURCE_DIR/medium-files-10k/" \
    "$RSYNC_BIN -a '$SOURCE_DIR/medium-files-10k/' '$DEST_DIR/'"

run_test_suite "16_arsync_10k_medium" \
    "$SOURCE_DIR/medium-files-10k/" \
    "$ARSYNC_BIN -a '$SOURCE_DIR/medium-files-10k/' '$DEST_DIR/'"

echo ""
echo "========================================="
echo "===  SCENARIO 3: Deep Directory Trees ==="
echo "========================================="
echo ""

run_test_suite "17_rsync_deep_d10" \
    "$SOURCE_DIR/deep-tree-d10/" \
    "$RSYNC_BIN -a '$SOURCE_DIR/deep-tree-d10/' '$DEST_DIR/'"

run_test_suite "18_arsync_deep_d10" \
    "$SOURCE_DIR/deep-tree-d10/" \
    "$ARSYNC_BIN -a '$SOURCE_DIR/deep-tree-d10/' '$DEST_DIR/'"

run_test_suite "19_rsync_wide" \
    "$SOURCE_DIR/wide-tree/" \
    "$RSYNC_BIN -a '$SOURCE_DIR/wide-tree/' '$DEST_DIR/'"

run_test_suite "20_arsync_wide" \
    "$SOURCE_DIR/wide-tree/" \
    "$ARSYNC_BIN -a '$SOURCE_DIR/wide-tree/' '$DEST_DIR/'"

echo ""
echo "========================================="
echo "===  SCENARIO 4: Hardlinks            ==="
echo "========================================="
echo ""

run_test_suite "21_rsync_hardlinks_50pct" \
    "$SOURCE_DIR/hardlinks-50pct/" \
    "$RSYNC_BIN -aH '$SOURCE_DIR/hardlinks-50pct/' '$DEST_DIR/'"

run_test_suite "22_arsync_hardlinks_50pct" \
    "$SOURCE_DIR/hardlinks-50pct/" \
    "$ARSYNC_BIN -aH '$SOURCE_DIR/hardlinks-50pct/' '$DEST_DIR/'"

run_test_suite "23_rsync_hardlinks_90pct" \
    "$SOURCE_DIR/hardlinks-90pct/" \
    "$RSYNC_BIN -aH '$SOURCE_DIR/hardlinks-90pct/' '$DEST_DIR/'"

run_test_suite "24_arsync_hardlinks_90pct" \
    "$SOURCE_DIR/hardlinks-90pct/" \
    "$ARSYNC_BIN -aH '$SOURCE_DIR/hardlinks-90pct/' '$DEST_DIR/'"

echo ""
echo "========================================="
echo "===  SCENARIO 5: Mixed Workloads      ==="
echo "========================================="
echo ""

run_test_suite "25_rsync_photo_library" \
    "$SOURCE_DIR/photo-library/" \
    "$RSYNC_BIN -a '$SOURCE_DIR/photo-library/' '$DEST_DIR/'"

run_test_suite "26_arsync_photo_library" \
    "$SOURCE_DIR/photo-library/" \
    "$ARSYNC_BIN -a '$SOURCE_DIR/photo-library/' '$DEST_DIR/'"

if [ -d "$SOURCE_DIR/linux-kernel/" ]; then
    run_test_suite "27_rsync_kernel" \
        "$SOURCE_DIR/linux-kernel/" \
        "$RSYNC_BIN -a '$SOURCE_DIR/linux-kernel/' '$DEST_DIR/'"
    
    run_test_suite "28_arsync_kernel" \
        "$SOURCE_DIR/linux-kernel/" \
        "$ARSYNC_BIN -a '$SOURCE_DIR/linux-kernel/' '$DEST_DIR/'"
fi

echo ""
echo "========================================="
echo "===  SCENARIO 6: Metadata (xattrs)    ==="
echo "========================================="
echo ""

run_test_suite "29_rsync_xattrs" \
    "$SOURCE_DIR/with-xattrs/" \
    "$RSYNC_BIN -aX '$SOURCE_DIR/with-xattrs/' '$DEST_DIR/'"

run_test_suite "30_arsync_xattrs" \
    "$SOURCE_DIR/with-xattrs/" \
    "$ARSYNC_BIN -aX '$SOURCE_DIR/with-xattrs/' '$DEST_DIR/'"

echo ""
echo "========================================="
echo "===  PARALLEL RSYNC COMPARISON        ==="
echo "========================================="
echo ""

# Test rsync with GNU parallel (common optimization)
if command -v parallel &> /dev/null; then
    run_test_suite "31_rsync_parallel_10k" \
        "$SOURCE_DIR/small-files-10k-each/" \
        "find '$SOURCE_DIR/small-files-10k-each/' -type f | parallel -j $CPUS rsync -a {} '$DEST_DIR/'"
fi

echo ""
echo "========================================="
echo "===  BENCHMARKING COMPLETE            ==="
echo "========================================="
echo ""
echo "Results saved to: $RESULTS_DIR"
echo ""
echo "Next steps:"
echo "1. Run: python3 ./analyze_results.py"
echo "2. Review: $RESULTS_DIR/final_report.txt"
echo "3. Update README.md with verified numbers"
echo ""

