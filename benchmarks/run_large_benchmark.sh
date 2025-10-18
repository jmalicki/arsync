#!/bin/bash
# Large-scale benchmark: 1TB dataset (4× RAM) to measure true I/O performance
# This exceeds page cache capacity to ensure we measure disk-to-disk, not RAM performance

set -euo pipefail

SOURCE="${1:-/mnt/benchmark/source-1tb}"
RESULTS_DIR="${2:-/tmp/benchmark-results-$(date +%Y%m%d-%H%M%S)}"
ARSYNC_BIN="${3:-./target/release/arsync}"

# Validate inputs
if [ ! -d "$SOURCE" ]; then
    echo "Error: Source directory not found: $SOURCE"
    exit 1
fi

if [ ! -f "$ARSYNC_BIN" ]; then
    echo "Error: arsync binary not found: $ARSYNC_BIN"
    exit 1
fi

mkdir -p "$RESULTS_DIR"
RESULTS_FILE="$RESULTS_DIR/benchmark-results.txt"
CONFIG_FILE="$RESULTS_DIR/test-config.txt"

echo "=== Large-Scale Benchmark: 1TB Dataset (4× RAM) ===" | tee "$RESULTS_FILE"
echo "Date: $(date)" | tee -a "$RESULTS_FILE"
echo "Source: $SOURCE" | tee -a "$RESULTS_FILE"
echo "Results: $RESULTS_DIR" | tee -a "$RESULTS_FILE"
echo "" | tee -a "$RESULTS_FILE"

# Log system configuration
{
    echo "=== System Configuration ==="
    echo "RAM: $(free -h | grep Mem: | awk '{print $2}')"
    echo "Available: $(free -h | grep Mem: | awk '{print $7}')"
    echo "Cache: $(free -h | grep Mem: | awk '{print $6}')"
    echo "CPU cores: $(nproc)"
    echo "Dataset size: $(du -sh $SOURCE | cut -f1)"
    echo "Files: $(ls -1 $SOURCE | wc -l)"
    echo "RAID array: $(df -h /mnt/benchmark | grep /dev/md)"
    echo "Kernel: $(uname -r)"
    echo "arsync version: $($ARSYNC_BIN --version 2>/dev/null || echo 'unknown')"
    echo ""
    echo "=== Metadata Preservation Settings ==="
    echo "Using -a (archive mode):"
    echo "  ✓ Permissions (-p)"
    echo "  ✓ Ownership (-o)"
    echo "  ✓ Timestamps (-t)"
    echo "  ✓ Recursive (-r)"
    echo "  ✓ Preserve links (-l)"
    echo "  ✓ Preserve device files (-D)"
    echo "  ✓ Preserve group (-g)"
    echo ""
    echo "This matches 'rsync -a' behavior for fair comparison"
    echo ""
} | tee "$CONFIG_FILE"

# Helper to drop caches if running as root
drop_caches() {
    echo "Syncing filesystems..."
    sync
    
    if [ "$EUID" -eq 0 ]; then
        echo "Dropping page cache..."
        echo 3 > /proc/sys/vm/drop_caches
        sleep 2
        echo "✓ Caches dropped"
    else
        echo "⚠ Not root - cannot drop caches, waiting 30s for natural eviction..."
        sleep 30
    fi
    
    # Show current cache state
    free -h | grep "Mem:" | awk '{print "Available: " $7 ", Cache: " $6}'
    echo ""
}

# Helper to show cache growth
show_cache_stats() {
    local label="$1"
    echo "$label cache stats:" | tee -a "$RESULTS_FILE"
    free -h | grep "Mem:" | tee -a "$RESULTS_FILE"
    echo "" | tee -a "$RESULTS_FILE"
}

# Test 1: rsync baseline
echo "=== Test 1/5: rsync baseline ===" | tee -a "$RESULTS_FILE"
drop_caches
show_cache_stats "Before rsync:"

DEST="/mnt/benchmark/dest-rsync-1tb"
rm -rf "$DEST"

echo "Running: rsync -a $SOURCE/ $DEST/" | tee -a "$RESULTS_FILE"
{ time rsync -a "$SOURCE/" "$DEST/" ; } 2>&1 | tee -a "$RESULTS_FILE"

show_cache_stats "After rsync:"
echo "" | tee -a "$RESULTS_FILE"

# Test 2: arsync sequential (no parallel copy)
echo "=== Test 2/5: arsync sequential ===" | tee -a "$RESULTS_FILE"
drop_caches
show_cache_stats "Before sequential:"

DEST="/mnt/benchmark/dest-seq-1tb"
rm -rf "$DEST"

echo "Running: arsync -a $SOURCE/ $DEST/" | tee -a "$RESULTS_FILE"
{ time "$ARSYNC_BIN" -a "$SOURCE/" "$DEST/" ; } 2>&1 | tee -a "$RESULTS_FILE"

show_cache_stats "After sequential:"
echo "" | tee -a "$RESULTS_FILE"

# Test 3: arsync parallel depth 2 (4 tasks per file)
echo "=== Test 3/5: arsync parallel depth 2 ===" | tee -a "$RESULTS_FILE"
drop_caches
show_cache_stats "Before parallel depth 2:"

DEST="/mnt/benchmark/dest-par2-1tb"
rm -rf "$DEST"

echo "Running: arsync -a --parallel-max-depth 2 --parallel-min-size-mb 512 $SOURCE/ $DEST/" | tee -a "$RESULTS_FILE"
{ time "$ARSYNC_BIN" -a "$SOURCE/" "$DEST/" \
    --parallel-max-depth 2 \
    --parallel-min-size-mb 512 ; } 2>&1 | tee -a "$RESULTS_FILE"

show_cache_stats "After parallel depth 2:"
echo "" | tee -a "$RESULTS_FILE"

# Test 4: arsync parallel depth 3 (8 tasks per file)
echo "=== Test 4/5: arsync parallel depth 3 ===" | tee -a "$RESULTS_FILE"
drop_caches
show_cache_stats "Before parallel depth 3:"

DEST="/mnt/benchmark/dest-par3-1tb"
rm -rf "$DEST"

echo "Running: arsync -a --parallel-max-depth 3 --parallel-min-size-mb 512 $SOURCE/ $DEST/" | tee -a "$RESULTS_FILE"
{ time "$ARSYNC_BIN" -a "$SOURCE/" "$DEST/" \
    --parallel-max-depth 3 \
    --parallel-min-size-mb 512 ; } 2>&1 | tee -a "$RESULTS_FILE"

show_cache_stats "After parallel depth 3:"
echo "" | tee -a "$RESULTS_FILE"

# Test 5: arsync parallel depth 4 (16 tasks per file)
echo "=== Test 5/5: arsync parallel depth 4 ===" | tee -a "$RESULTS_FILE"
drop_caches
show_cache_stats "Before parallel depth 4:"

DEST="/mnt/benchmark/dest-par4-1tb"
rm -rf "$DEST"

echo "Running: arsync -a --parallel-max-depth 4 --parallel-min-size-mb 1024 $SOURCE/ $DEST/" | tee -a "$RESULTS_FILE"
{ time "$ARSYNC_BIN" -a "$SOURCE/" "$DEST/" \
    --parallel-max-depth 4 \
    --parallel-min-size-mb 1024 ; } 2>&1 | tee -a "$RESULTS_FILE"

show_cache_stats "After parallel depth 4:"
echo "" | tee -a "$RESULTS_FILE"

# Data integrity check (sample 5 random files)
echo "=== Data Integrity Check (5 random files) ===" | tee -a "$RESULTS_FILE"
for i in $(shuf -i 1-250 -n 5); do
    file=$(printf "file_%03d.bin" $i)
    echo "Checking $file..." | tee -a "$RESULTS_FILE"
    sha256sum "$SOURCE/$file" /mnt/benchmark/dest-*-1tb/$file 2>&1 | tee -a "$RESULTS_FILE"
done | tee -a "$RESULTS_FILE"

echo "" | tee -a "$RESULTS_FILE"
echo "=== Benchmark Complete ===" | tee -a "$RESULTS_FILE"
echo "Results saved to: $RESULTS_FILE" | tee -a "$RESULTS_FILE"
echo "Configuration: $CONFIG_FILE" | tee -a "$RESULTS_FILE"

# Cleanup old 400GB attempt if it exists
rm -rf /mnt/benchmark/source-400gb 2>/dev/null || true

echo ""
echo "Summary will be in: $RESULTS_DIR/"
ls -lh "$RESULTS_DIR/"

