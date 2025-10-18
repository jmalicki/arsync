#!/bin/bash
# Create large benchmark dataset that exceeds RAM
# 100 files × 4GB = 400GB (exceeds 256GB RAM)

set -euo pipefail

DEST_DIR="${1:-/mnt/benchmark/source-400gb}"
NUM_FILES="${2:-100}"
FILE_SIZE_GB="${3:-4}"

echo "=== Creating Large Benchmark Dataset ==="
echo "Destination: $DEST_DIR"
echo "Files: $NUM_FILES × ${FILE_SIZE_GB}GB = $((NUM_FILES * FILE_SIZE_GB))GB total"
echo "Started: $(date)"
echo ""

mkdir -p "$DEST_DIR"

# Calculate file size in MB for dd
FILE_SIZE_MB=$((FILE_SIZE_GB * 1024))

START_TIME=$(date +%s)

for i in $(seq -f "%03g" 1 $NUM_FILES); do
    FILE_PATH="$DEST_DIR/file_${i}.bin"
    
    # Skip if file already exists and is correct size
    if [ -f "$FILE_PATH" ]; then
        ACTUAL_SIZE=$(stat -f "%z" "$FILE_PATH" 2>/dev/null || stat -c "%s" "$FILE_PATH" 2>/dev/null)
        EXPECTED_SIZE=$((FILE_SIZE_GB * 1024 * 1024 * 1024))
        if [ "$ACTUAL_SIZE" -eq "$EXPECTED_SIZE" ]; then
            echo "[$i/$NUM_FILES] ✓ file_${i}.bin already exists (${FILE_SIZE_GB}GB)"
            continue
        fi
    fi
    
    echo "[$i/$NUM_FILES] Creating file_${i}.bin (${FILE_SIZE_GB}GB)..."
    
    # Use /dev/urandom for random data (verifiable, not compressible)
    # oflag=direct bypasses page cache during creation
    dd if=/dev/urandom \
       of="$FILE_PATH" \
       bs=1M \
       count=$FILE_SIZE_MB \
       oflag=direct \
       status=progress 2>&1 | tail -2
    
    # Show progress
    CURRENT_TIME=$(date +%s)
    ELAPSED=$((CURRENT_TIME - START_TIME))
    RATE=$(echo "scale=1; $i * $FILE_SIZE_GB / $ELAPSED" | bc 2>/dev/null || echo "N/A")
    echo "  Progress: $i/$NUM_FILES files, Elapsed: ${ELAPSED}s, Rate: ${RATE} GB/s"
    echo ""
done

END_TIME=$(date +%s)
TOTAL_TIME=$((END_TIME - START_TIME))
TOTAL_GB=$((NUM_FILES * FILE_SIZE_GB))
AVG_RATE=$(echo "scale=2; $TOTAL_GB / $TOTAL_TIME" | bc 2>/dev/null || echo "N/A")

echo "=== Dataset Creation Complete ==="
echo "Total time: ${TOTAL_TIME}s ($(($TOTAL_TIME / 60)) minutes)"
echo "Average rate: ${AVG_RATE} GB/s"
echo "Total size: ${TOTAL_GB}GB"
du -sh "$DEST_DIR"
ls -lh "$DEST_DIR" | head -10
echo ""
echo "Ready for benchmarking!"

