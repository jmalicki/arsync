#!/bin/bash
# Stress test for small files to find issues before benchmarking
# Tests increasing file counts to find breaking points

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
ARSYNC_BIN="${ARSYNC_BIN:-$PROJECT_ROOT/target/release/arsync}"

TEST_DIR="/tmp/arsync-stress-test-$$"
RESULTS_FILE="stress_test_results.txt"

echo "=== Small File Stress Test ==="
echo "Purpose: Find file descriptor limits and performance degradation points"
echo "Binary: $ARSYNC_BIN"
echo ""

# Check ulimit
echo "Current ulimit -n: $(ulimit -n)"
echo "Attempting to increase to 1000000..."
ulimit -n 1000000 2>/dev/null || echo "Warning: Could not increase ulimit (may hit limits)"
echo "New ulimit -n: $(ulimit -n)"
echo ""

# Verify binary exists
if [ ! -x "$ARSYNC_BIN" ]; then
    echo "ERROR: arsync binary not found at $ARSYNC_BIN"
    echo "Run: cargo build --release"
    exit 1
fi

# Create results file
echo "# Small File Stress Test Results - $(date)" > "$RESULTS_FILE"
echo "# Format: file_count,status,time_seconds,throughput_mbs,error_message" >> "$RESULTS_FILE"
echo "" >> "$RESULTS_FILE"

# Function to generate test files
generate_test_files() {
    local count=$1
    local size=$2
    local dir=$3
    
    echo "Generating $count files of ${size}..."
    mkdir -p "$dir"
    
    for i in $(seq 1 "$count"); do
        dd if=/dev/zero of="${dir}/file_$(printf '%06d' $i).dat" \
           bs="$size" count=1 status=none 2>/dev/null
    done
}

# Function to test arsync with specific file count
test_file_count() {
    local count=$1
    local size=$2
    
    echo ""
    echo "=== Testing $count files of ${size} ==="
    
    local source_dir="$TEST_DIR/source_${count}"
    local dest_dir="$TEST_DIR/dest_${count}"
    
    # Generate files
    generate_test_files "$count" "$size" "$source_dir"
    mkdir -p "$dest_dir"
    
    # Test arsync
    echo "Running arsync..."
    local start_time=$(date +%s.%N)
    
    if timeout 60 "$ARSYNC_BIN" -a \
        "$source_dir/" \
        "$dest_dir/" \
        > "$TEST_DIR/test_${count}_stdout.log" \
        2> "$TEST_DIR/test_${count}_stderr.log"; then
        
        local end_time=$(date +%s.%N)
        local elapsed=$(echo "$end_time - $start_time" | bc)
        
        # Verify all files copied
        local copied=$(find "$dest_dir" -type f | wc -l)
        
        if [ "$copied" -eq "$count" ]; then
            local total_mb=$(du -sm "$source_dir" | cut -f1)
            local throughput=$(echo "scale=2; $total_mb / $elapsed" | bc)
            echo "  ✓ SUCCESS: $copied files in ${elapsed}s (${throughput} MB/s)"
            echo "$count,SUCCESS,$elapsed,$throughput," >> "$RESULTS_FILE"
        else
            echo "  ✗ PARTIAL: Only $copied of $count files copied"
            echo "$count,PARTIAL,$elapsed,0,Missing files: $((count - copied))" >> "$RESULTS_FILE"
        fi
    else
        local exit_code=$?
        echo "  ✗ FAILED: Exit code $exit_code"
        
        # Check for common errors
        if grep -q "Too many open files" "$TEST_DIR/test_${count}_stderr.log" 2>/dev/null; then
            echo "    Error: TOO MANY OPEN FILES (file descriptor limit)"
            echo "$count,FAILED,0,0,Too many open files (ulimit)" >> "$RESULTS_FILE"
        elif [ $exit_code -eq 124 ]; then
            echo "    Error: TIMEOUT (>60 seconds)"
            echo "$count,TIMEOUT,60,0,Timeout after 60s" >> "$RESULTS_FILE"
        else
            local error=$(tail -5 "$TEST_DIR/test_${count}_stderr.log" 2>/dev/null | head -1)
            echo "    Error: $error"
            echo "$count,FAILED,0,0,$error" >> "$RESULTS_FILE"
        fi
    fi
    
    # Cleanup
    rm -rf "$source_dir" "$dest_dir"
}

# Test escalating file counts
echo "Testing with escalating file counts to find limits..."
echo ""

# Small tests first
test_file_count 10 "1K"
test_file_count 50 "1K"
test_file_count 100 "1K"
test_file_count 500 "1K"
test_file_count 1000 "1K"

# Larger tests  
test_file_count 5000 "1K"
test_file_count 10000 "1K"

# If those work, try extreme
if grep -q "SUCCESS.*10000" "$RESULTS_FILE"; then
    echo ""
    echo "=== Extreme Scale Tests ==="
    test_file_count 50000 "1K"
    test_file_count 100000 "1K"
fi

# Cleanup
rm -rf "$TEST_DIR"

echo ""
echo "========================================="
echo "STRESS TEST COMPLETE"
echo "========================================="
echo ""
echo "Results saved to: $RESULTS_FILE"
cat "$RESULTS_FILE"
echo ""

# Summary
success_count=$(grep -c "SUCCESS" "$RESULTS_FILE" || echo 0)
failed_count=$(grep -c "FAILED\|TIMEOUT" "$RESULTS_FILE" || echo 0)

echo "Summary:"
echo "  Successful: $success_count tests"
echo "  Failed: $failed_count tests"
echo ""

if [ $failed_count -gt 0 ]; then
    echo "⚠️  ISSUES FOUND - Fix before benchmarking!"
    echo ""
    echo "Common fixes:"
    echo "  1. Increase ulimit: ulimit -n 1000000"
    echo "  2. Check --max-files-in-flight setting (default may be too high)"
    echo "  3. Review file descriptor leaks in arsync code"
    echo ""
    exit 1
else
    echo "✓ All stress tests passed!"
    echo "  Ready for benchmarking"
    exit 0
fi

