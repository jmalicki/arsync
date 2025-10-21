#!/bin/bash
# Benchmark zero-copy I/O performance
#
# Compares:
# 1. Baseline (no buffer pool)
# 2. read_managed only (PR #94)
# 3. read_managed + write_managed (PR #95)
#
# Usage: ./zero_copy_bench.sh [output_dir]

set -euo pipefail

OUTPUT_DIR="${1:-benchmark-results-zerocopy-$(date +%Y%m%d_%H%M%S)}"
mkdir -p "$OUTPUT_DIR"

echo "=== Zero-Copy I/O Benchmark Suite ===" | tee "$OUTPUT_DIR/README.md"
echo "" | tee -a "$OUTPUT_DIR/README.md"
echo "Started: $(date)" | tee -a "$OUTPUT_DIR/README.md"
echo "Host: $(hostname)" | tee -a "$OUTPUT_DIR/README.md"
echo "Kernel: $(uname -r)" | tee -a "$OUTPUT_DIR/README.md"
echo "" | tee -a "$OUTPUT_DIR/README.md"

# Build arsync in release mode
echo "Building arsync (release mode)..." | tee -a "$OUTPUT_DIR/README.md"
cargo build --release

# Create test data directory
TEST_DATA_DIR="/tmp/arsync-zero-copy-bench"
mkdir -p "$TEST_DATA_DIR"

echo "" | tee -a "$OUTPUT_DIR/README.md"
echo "## Test Files" | tee -a "$OUTPUT_DIR/README.md"

# Generate test files of various sizes
declare -A TEST_FILES
TEST_FILES=(
    ["small_1mb"]="1M"
    ["medium_10mb"]="10M"
    ["medium_100mb"]="100M"
    ["large_1gb"]="1G"
    ["large_10gb"]="10G"
)

for name in "${!TEST_FILES[@]}"; do
    size="${TEST_FILES[$name]}"
    file="$TEST_DATA_DIR/${name}.dat"
    
    if [ ! -f "$file" ]; then
        echo "Creating $name ($size)..." | tee -a "$OUTPUT_DIR/README.md"
        dd if=/dev/urandom of="$file" bs=1M count=$(echo "$size" | sed 's/[MG]//') iflag=fullblock 2>&1 | grep -v records
    else
        echo "Using existing $name ($size)" | tee -a "$OUTPUT_DIR/README.md"
    fi
    
    echo "- $name: $size ($(stat -c%s "$file") bytes)" | tee -a "$OUTPUT_DIR/README.md"
done

echo "" | tee -a "$OUTPUT_DIR/README.md"

# Benchmark function
run_bench() {
    local name="$1"
    local src="$2"
    local desc="$3"
    local flags="$4"
    
    local dst="/tmp/arsync-bench-dst-$name"
    rm -rf "$dst"
    
    echo "## Benchmark: $desc" | tee -a "$OUTPUT_DIR/README.md"
    echo "File: $(basename "$src")" | tee -a "$OUTPUT_DIR/README.md"
    echo "Flags: $flags" | tee -a "$OUTPUT_DIR/README.md"
    echo "" | tee -a "$OUTPUT_DIR/README.md"
    
    # Drop caches
    sudo sh -c 'echo 3 > /proc/sys/vm/drop_caches' 2>/dev/null || true
    sleep 1
    
    # Run 3 times and take average
    local total_time=0
    for run in 1 2 3; do
        rm -rf "$dst"
        
        # Time the copy
        local start=$(date +%s.%N)
        ./target/release/arsync $flags "$src" "$dst" > /dev/null 2>&1
        local end=$(date +%s.%N)
        
        local elapsed=$(echo "$end - $start" | bc)
        total_time=$(echo "$total_time + $elapsed" | bc)
        
        echo "  Run $run: ${elapsed}s" | tee -a "$OUTPUT_DIR/README.md"
    done
    
    local avg_time=$(echo "scale=3; $total_time / 3" | bc)
    local file_size=$(stat -c%s "$src")
    local throughput=$(echo "scale=2; $file_size / $avg_time / 1024 / 1024" | bc)
    
    echo "  Average: ${avg_time}s" | tee -a "$OUTPUT_DIR/README.md"
    echo "  Throughput: ${throughput} MB/s" | tee -a "$OUTPUT_DIR/README.md"
    echo "" | tee -a "$OUTPUT_DIR/README.md"
    
    # Save results
    echo "$avg_time" > "$OUTPUT_DIR/${name}_time.txt"
    echo "$throughput" > "$OUTPUT_DIR/${name}_throughput.txt"
}

# Run benchmarks
echo "" | tee -a "$OUTPUT_DIR/README.md"
echo "# Benchmark Results" | tee -a "$OUTPUT_DIR/README.md"
echo "" | tee -a "$OUTPUT_DIR/README.md"

# For each file size
for name in small_1mb medium_10mb medium_100mb large_1gb; do
    src="$TEST_DATA_DIR/${name}.dat"
    
    if [ ! -f "$src" ]; then
        echo "Skipping $name (file not found)" | tee -a "$OUTPUT_DIR/README.md"
        continue
    fi
    
    # TODO: Add flag to disable buffer pool for baseline comparison
    # For now, buffer pool is always used if available
    
    run_bench "${name}" "$src" "Zero-copy (buffer pool enabled)" ""
done

echo "" | tee -a "$OUTPUT_DIR/README.md"
echo "## Summary" | tee -a "$OUTPUT_DIR/README.md"
echo "" | tee -a "$OUTPUT_DIR/README.md"
echo "Results saved to: $OUTPUT_DIR" | tee -a "$OUTPUT_DIR/README.md"
echo "" | tee -a "$OUTPUT_DIR/README.md"
echo "To analyze:" | tee -a "$OUTPUT_DIR/README.md"
echo "  cat $OUTPUT_DIR/*_throughput.txt" | tee -a "$OUTPUT_DIR/README.md"
echo "" | tee -a "$OUTPUT_DIR/README.md"

# Performance counters (if available)
if command -v perf &> /dev/null; then
    echo "## Performance Counters" | tee -a "$OUTPUT_DIR/README.md"
    echo "" | tee -a "$OUTPUT_DIR/README.md"
    echo "Run with perf:" | tee -a "$OUTPUT_DIR/README.md"
    echo "  perf stat -e cycles,instructions,cache-references,cache-misses,mem_load_retired.l3_miss ./target/release/arsync src dst" | tee -a "$OUTPUT_DIR/README.md"
    echo "" | tee -a "$OUTPUT_DIR/README.md"
fi

echo "Finished: $(date)" | tee -a "$OUTPUT_DIR/README.md"

