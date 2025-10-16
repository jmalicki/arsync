#!/bin/bash
#
# Quick smoke test for arsync - catches bugs/crashes without full benchmark
# 
# This runs MINIMAL test data (10-50 files) through all test scenarios
# to ensure arsync doesn't crash, hang, or have obvious bugs.
#
# Expected time: < 2 minutes
# Focus: Correctness, not performance
#
# ⚠️  NO ROOT REQUIRED - runs in /tmp
# (Unlike run_benchmarks_quick.sh which needs sudo for cache dropping)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Default paths (use /tmp for quick tests)
SOURCE_DIR="${1:-/tmp/arsync-smoke-data}"
DEST_DIR="${2:-/tmp/arsync-smoke-output}"
ARSYNC_BIN="${PROJECT_ROOT}/target/release/arsync"

echo "========================================"
echo "===  ARSYNC SMOKE TEST               ==="
echo "========================================"
echo ""
echo "Quick validation - NOT a benchmark!"
echo "Testing for crashes, hangs, and obvious bugs"
echo ""
echo "Source: $SOURCE_DIR"
echo "Dest:   $DEST_DIR"
echo "Binary: $ARSYNC_BIN"
echo ""

# Check if binary exists
if [ ! -f "$ARSYNC_BIN" ]; then
    echo "ERROR: arsync binary not found at $ARSYNC_BIN"
    echo "Run: cargo build --release"
    exit 1
fi

# Create test data directory
mkdir -p "$SOURCE_DIR"
rm -rf "$DEST_DIR"

echo "========================================"
echo "Generating minimal test data..."
echo "========================================"

# Test 1: A few large files (3 × 500MB = 1.5GB)
echo "  Creating 3 large files (500MB each)..."
mkdir -p "$SOURCE_DIR/large"
for i in {1..3}; do
    dd if=/dev/urandom of="$SOURCE_DIR/large/file_${i}.dat" bs=1M count=500 status=none 2>/dev/null
done

# Test 2: Some small files (50 × 100KB)
echo "  Creating 50 small files (100KB each)..."
mkdir -p "$SOURCE_DIR/small"
for i in {1..50}; do
    dd if=/dev/urandom of="$SOURCE_DIR/small/file_${i}.dat" bs=1K count=100 status=none 2>/dev/null
done

# Test 3: Tiny files (100 × 1KB)
echo "  Creating 100 tiny files (1KB each)..."
mkdir -p "$SOURCE_DIR/tiny"
for i in {1..100}; do
    dd if=/dev/urandom of="$SOURCE_DIR/tiny/file_${i}.dat" bs=1K count=1 status=none 2>/dev/null
done

# Test 4: Directory tree (3 levels, 10 files)
echo "  Creating nested directory tree..."
mkdir -p "$SOURCE_DIR/tree/level1/level2/level3"
for i in {1..10}; do
    echo "test content $i" > "$SOURCE_DIR/tree/level1/level2/level3/file_${i}.txt"
done

# Test 5: Symlinks
echo "  Creating symlinks..."
mkdir -p "$SOURCE_DIR/links"
echo "target content" > "$SOURCE_DIR/links/target.txt"
ln -s target.txt "$SOURCE_DIR/links/symlink.txt"

# Test 6: Mixed permissions
echo "  Creating files with various permissions..."
mkdir -p "$SOURCE_DIR/perms"
echo "read-write" > "$SOURCE_DIR/perms/rw.txt"
chmod 644 "$SOURCE_DIR/perms/rw.txt"
echo "read-only" > "$SOURCE_DIR/perms/ro.txt"
chmod 444 "$SOURCE_DIR/perms/ro.txt"
echo "executable" > "$SOURCE_DIR/perms/exec.sh"
chmod 755 "$SOURCE_DIR/perms/exec.sh"

echo ""
echo "Test data created: $(du -sh $SOURCE_DIR | cut -f1)"
echo ""

# Function to run a test with timeout
run_test() {
    local name=$1
    local source=$2
    local timeout_sec=${3:-60}
    
    echo "========================================"
    echo "TEST: $name"
    echo "========================================"
    echo "  Source: $source"
    echo "  Timeout: ${timeout_sec}s"
    echo ""
    
    local dest="${DEST_DIR}/${name}"
    mkdir -p "$dest"
    
    local start=$(date +%s.%N)
    
    # Run with timeout protection
    set +e
    timeout ${timeout_sec}s "$ARSYNC_BIN" -a "$source" "$dest" 2>&1
    local exit_code=$?
    set -e
    
    local end=$(date +%s.%N)
    local elapsed=$(echo "$end - $start" | bc)
    
    if [ $exit_code -eq 124 ]; then
        echo ""
        echo "  ❌ TIMEOUT - Process hung for >${timeout_sec}s"
        echo "  THIS IS A BUG - arsync should never hang!"
        echo ""
        return 1
    elif [ $exit_code -ne 0 ]; then
        echo ""
        echo "  ❌ FAILED with exit code $exit_code"
        echo ""
        return 1
    fi
    
    # Verify file count matches
    local src_count=$(find "$source" -type f | wc -l)
    local dst_count=$(find "$dest" -type f | wc -l)
    
    if [ "$src_count" -ne "$dst_count" ]; then
        echo ""
        echo "  ❌ FILE COUNT MISMATCH"
        echo "     Source: $src_count files"
        echo "     Dest:   $dst_count files"
        echo ""
        return 1
    fi
    
    echo "  ✅ PASSED (${elapsed}s, $dst_count files)"
    echo ""
    return 0
}

# Run all tests
FAILED_TESTS=0

run_test "01_large_files" "$SOURCE_DIR/large" 90 || ((FAILED_TESTS++))
run_test "02_small_files" "$SOURCE_DIR/small" 60 || ((FAILED_TESTS++))
run_test "03_tiny_files" "$SOURCE_DIR/tiny" 60 || ((FAILED_TESTS++))
run_test "04_dir_tree" "$SOURCE_DIR/tree" 30 || ((FAILED_TESTS++))
run_test "05_symlinks" "$SOURCE_DIR/links" 30 || ((FAILED_TESTS++))
run_test "06_permissions" "$SOURCE_DIR/perms" 30 || ((FAILED_TESTS++))

# Summary
echo "========================================"
echo "===  SMOKE TEST RESULTS              ==="
echo "========================================"
echo ""

if [ $FAILED_TESTS -eq 0 ]; then
    echo "  ✅ ALL TESTS PASSED"
    echo ""
    echo "  arsync appears to be working correctly!"
    echo "  Ready for full benchmarks."
    echo ""
    exit 0
else
    echo "  ❌ $FAILED_TESTS TEST(S) FAILED"
    echo ""
    echo "  DO NOT run full benchmarks until bugs are fixed!"
    echo ""
    exit 1
fi

