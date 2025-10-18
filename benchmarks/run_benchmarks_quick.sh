#!/bin/bash
# Quick 30-minute benchmark suite with power monitoring
# Usage: sudo ./run_benchmarks_quick.sh [source] [dest] [results]
#    or: ALLOW_NO_ROOT=1 ./run_benchmarks_quick.sh [source] [dest] [results] (testing only)

set -euo pipefail

# Configuration
SOURCE_DIR="${1:-/mnt/source-nvme/benchmark-data-quick}"
DEST_DIR="${2:-/mnt/dest-nvme/benchmark-output-quick}"
RESULTS_DIR="${3:-./benchmark-results-quick-$(date +%Y%m%d_%H%M%S)}"
# Convert to absolute path to avoid issues when changing directories
RESULTS_DIR="$(readlink -f "$RESULTS_DIR" 2>/dev/null || (mkdir -p "$RESULTS_DIR" && cd "$RESULTS_DIR" && pwd))"
NUM_RUNS=3  # Quick benchmark: only 3 runs instead of 5
CPUS=$(nproc)
ENABLE_POWER_MONITORING="${ENABLE_POWER_MONITORING:-yes}"  # ENABLED BY DEFAULT for quick test!
ARSYNC_EXTRA_FLAGS="${ARSYNC_EXTRA_FLAGS:-}"  # Extra flags to pass to arsync (e.g., --parallel-max-depth 2)

# Paths to binaries (use absolute paths!)
RSYNC_BIN=$(which rsync)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
ARSYNC_BIN="${ARSYNC_BIN:-$PROJECT_ROOT/target/release/arsync}"

# Check root (can be bypassed with ALLOW_NO_ROOT=1 for testing)
if [ "$EUID" -ne 0 ]; then
    if [ "${ALLOW_NO_ROOT}" != "1" ]; then
        echo "ERROR: This script requires root (for dropping caches)"
        echo "Usage: sudo $0 [source] [dest] [results]"
        echo ""
        echo "For testing only (results won't be accurate):"
        echo "  ALLOW_NO_ROOT=1 $0 [source] [dest] [results]"
        exit 1
    else
        echo "========================================"
        echo "⚠️  WARNING: RUNNING WITHOUT ROOT"
        echo "========================================"
        echo ""
        echo "Cache dropping DISABLED - results will NOT be accurate!"
        echo "This mode is for TESTING ONLY, not real benchmarks."
        echo ""
        echo "For accurate results, run with sudo."
        echo ""
        sleep 3
    fi
fi

# Validate
if [ ! -d "$SOURCE_DIR" ]; then
    echo "ERROR: Source directory not found: $SOURCE_DIR"
    echo "Run ./benchmarks/generate_testdata_quick.sh first"
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

echo "=== Quick Benchmark Configuration ===" | tee test_config.txt
echo "Date: $(date)" | tee -a test_config.txt
echo "Source: $SOURCE_DIR" | tee -a test_config.txt
echo "Destination: $DEST_DIR" | tee -a test_config.txt
echo "Results: $RESULTS_DIR" | tee -a test_config.txt
echo "CPUs: $CPUS" | tee -a test_config.txt
echo "Runs per test: $NUM_RUNS" | tee -a test_config.txt
echo "Power monitoring: $ENABLE_POWER_MONITORING" | tee -a test_config.txt
echo "rsync: $RSYNC_BIN ($($RSYNC_BIN --version | head -1))" | tee -a test_config.txt
echo "arsync: $ARSYNC_BIN" | tee -a test_config.txt
if [ -n "$ARSYNC_EXTRA_FLAGS" ]; then
    echo "arsync extra flags: $ARSYNC_EXTRA_FLAGS" | tee -a test_config.txt
else
    echo "arsync extra flags: none (sequential copy)" | tee -a test_config.txt
fi
echo "" | tee -a test_config.txt

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
fi
echo ""

# Check if RAPL is available for power monitoring
if [ "$ENABLE_POWER_MONITORING" = "yes" ]; then
    if [ -d /sys/class/powercap/intel-rapl ]; then
        echo "✓ RAPL available - power monitoring ENABLED"
    else
        echo "✗ RAPL not available - power monitoring DISABLED (Intel/AMD CPU required)"
        ENABLE_POWER_MONITORING="no"
    fi
fi

# Drop caches and prepare destination
prepare_test() {
    echo "  Preparing test environment..."
    
    # Clear destination
    rm -rf "${DEST_DIR:?}"/*
    sync
    
    # Drop caches (CRITICAL for fair comparison)
    if [ "$EUID" -eq 0 ]; then
        echo "  → Dropping caches (echo 3 > /proc/sys/vm/drop_caches)..."
        echo 3 > /proc/sys/vm/drop_caches
    else
        echo "  → ⚠️  SKIPPING cache drop (not root - results INVALID)"
    fi
    echo "  → Caches dropped - testing COLD performance"
    
    # Wait for I/O to quiesce
    sleep 2
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
    
    # Optional: Start power monitoring
    if [ -f "$(dirname "$0")/power_monitoring.sh" ] && [ "$ENABLE_POWER_MONITORING" = "yes" ]; then
        "$(dirname "$0")/power_monitoring.sh" "${output_prefix}_power.csv" &
        local power_pid=$!
    else
        local power_pid=""
    fi
    
    # Run benchmark with time measurement AND TIMEOUT (hanging is never acceptable)
    local start_time=$(date +%s.%N)
    
    # Timeout: 5 minutes for quick tests (should complete in <2 min normally)
    timeout 300 /usr/bin/time -v bash -c "$command" \
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
    if [ $exit_code -eq 124 ]; then
        echo ""
        echo "    ❌ TIMEOUT: Process hung for >5 minutes (CRITICAL BUG!)"
        echo "    Command: $command"
        echo ""
        echo "    This is a DEADLOCK - arsync should NEVER hang!"
        echo "    Check logs for 'Too many open files' or other errors before hang:"
        tail -50 "${output_prefix}_stdout.log" | grep -E "WARN|ERROR|error|failed" | tail -10 | sed 's/^/      /'
        echo ""
        echo "    Full logs:"
        echo "      stdout: ${output_prefix}_stdout.log"
        echo "      stderr: ${output_prefix}_time.log"
        echo ""
        return 1
    elif [ $exit_code -ne 0 ]; then
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
    
    # Count files and calculate throughput
    local file_count=$(find "$dest" -type f 2>/dev/null | wc -l)
    echo "$file_count" > "${output_prefix}_filecount.txt"
    
    local total_size=$(du -sb "$dest" 2>/dev/null | cut -f1)
    echo "$total_size" > "${output_prefix}_bytes.txt"
    
    if [ "$elapsed" != "0" ]; then
        local throughput=$(echo "scale=2; $total_size / $elapsed / 1024 / 1024 / 1024" | bc)
        echo "${throughput} GB/s" > "${output_prefix}_throughput.txt"
        echo "    Throughput: ${throughput} GB/s ($file_count files, ${elapsed}s)"
    fi
    
    return 0
}

# Run test suite for a scenario
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
    
    # Quick statistics
    echo "  Analyzing results..."
    python3 << 'EOF'
import glob
import sys

# Load elapsed times (skip first run as warm-up if we have 3+ runs)
times = []
for f in sorted(glob.glob('*_elapsed.txt')):
    with open(f) as fp:
        times.append(float(fp.read().strip()))

if len(times) >= 3:
    # Skip first run
    times = times[1:]

if times:
    mean = sum(times) / len(times)
    print(f"  Mean: {mean:.3f}s")
    
    with open('summary.txt', 'w') as f:
        f.write(f"mean={mean:.3f}\n")
        f.write(f"runs={len(times)}\n")
EOF
    
    cd "$RESULTS_DIR"
}

# Set CPU to performance mode
echo "=== Setting CPU to performance mode ==="
if [ "$EUID" -eq 0 ]; then
    for cpu in /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor; do
        if [ -f "$cpu" ]; then
            echo performance > "$cpu"
        fi
    done
    echo "✓ CPU governor set to performance"
else
    echo "⚠️  SKIPPING CPU governor change (not root)"
fi

echo ""
echo "========================================"
echo "===  QUICK BENCHMARK SUITE (30 min) ==="
echo "========================================"
echo ""
echo "This will run 6 key scenarios with 3 runs each"
echo "Focus: Get preliminary numbers quickly"
echo ""

# Test 1: Multiple large files (5× 5GB = 25GB total)
echo "=== TEST 1: Large Files (5× 5GB = 25GB) ==="
# Run arsync first to catch bugs early
run_test_suite "01_arsync_large_files" \
    "$SOURCE_DIR/large-files/" \
    "$ARSYNC_BIN $ARSYNC_EXTRA_FLAGS -a '$SOURCE_DIR/large-files/' '$DEST_DIR/'"

run_test_suite "02_rsync_large_files" \
    "$SOURCE_DIR/large-files/" \
    "$RSYNC_BIN -a '$SOURCE_DIR/large-files/' '$DEST_DIR/'"

# Test 2: Many small files (1000 × 10KB)
echo ""
echo "=== TEST 2: Small Files (1000 × 10KB) ==="
run_test_suite "03_arsync_1k_small" \
    "$SOURCE_DIR/small-files-1k/" \
    "$ARSYNC_BIN $ARSYNC_EXTRA_FLAGS -a '$SOURCE_DIR/small-files-1k/' '$DEST_DIR/'"

run_test_suite "04_rsync_1k_small" \
    "$SOURCE_DIR/small-files-1k/" \
    "$RSYNC_BIN -a '$SOURCE_DIR/small-files-1k/' '$DEST_DIR/'"

# Test 3: Tiny files (5000 × 1KB) - extreme syscall overhead
echo ""
echo "=== TEST 3: Tiny Files (5000 × 1KB) ==="
run_test_suite "05_arsync_5k_tiny" \
    "$SOURCE_DIR/tiny-files-5k/" \
    "$ARSYNC_BIN $ARSYNC_EXTRA_FLAGS -a '$SOURCE_DIR/tiny-files-5k/' '$DEST_DIR/'"

run_test_suite "06_rsync_5k_tiny" \
    "$SOURCE_DIR/tiny-files-5k/" \
    "$RSYNC_BIN -a '$SOURCE_DIR/tiny-files-5k/' '$DEST_DIR/'"

# Test 4: Medium files (500 × 1MB)
echo ""
echo "=== TEST 4: Medium Files (500 × 1MB) ==="
run_test_suite "07_arsync_500_medium" \
    "$SOURCE_DIR/medium-files-500/" \
    "$ARSYNC_BIN $ARSYNC_EXTRA_FLAGS -a '$SOURCE_DIR/medium-files-500/' '$DEST_DIR/'"

run_test_suite "08_rsync_500_medium" \
    "$SOURCE_DIR/medium-files-500/" \
    "$RSYNC_BIN -a '$SOURCE_DIR/medium-files-500/' '$DEST_DIR/'"

# Test 5: Mixed workload (photos)
echo ""
echo "=== TEST 5: Mixed Workload (Photos) ==="
run_test_suite "09_arsync_photos" \
    "$SOURCE_DIR/mixed-photos/" \
    "$ARSYNC_BIN $ARSYNC_EXTRA_FLAGS -a '$SOURCE_DIR/mixed-photos/' '$DEST_DIR/'"

run_test_suite "10_rsync_photos" \
    "$SOURCE_DIR/mixed-photos/" \
    "$RSYNC_BIN -a '$SOURCE_DIR/mixed-photos/' '$DEST_DIR/'"

# Test 6: Directory tree
echo ""
echo "=== TEST 6: Directory Tree ==="
run_test_suite "11_arsync_dirtree" \
    "$SOURCE_DIR/dir-tree/" \
    "$ARSYNC_BIN $ARSYNC_EXTRA_FLAGS -a '$SOURCE_DIR/dir-tree/' '$DEST_DIR/'"

run_test_suite "12_rsync_dirtree" \
    "$SOURCE_DIR/dir-tree/" \
    "$RSYNC_BIN -a '$SOURCE_DIR/dir-tree/' '$DEST_DIR/'"

echo ""
echo "========================================"
echo "===  QUICK BENCHMARK COMPLETE       ==="
echo "========================================"
echo ""
echo "Results saved to: $RESULTS_DIR"
echo ""
echo "Next steps:"
echo "1. Run: python3 ./benchmarks/analyze_results.py $RESULTS_DIR"
echo "2. Review: cat $RESULTS_DIR/final_report.txt"
echo ""
if [ "$ENABLE_POWER_MONITORING" = "yes" ]; then
    echo "✓ Power measurements included in results!"
    echo "  Look for *_power.csv files in each test directory"
    echo ""
fi
echo "Total runtime: ~30 minutes"
echo "These are PRELIMINARY results - run full benchmark for publication quality!"

