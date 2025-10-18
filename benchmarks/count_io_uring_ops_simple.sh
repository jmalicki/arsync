#!/bin/bash
# Simple io_uring operation counter (no sudo required)
#
# This script infers io_uring operations from file characteristics
# and known code patterns, providing an estimate without kernel tracing.

set -euo pipefail

SRC_DIR="$1"
FILE_SIZE_MB="${2:-10}"
ARSYNC="${3:-./target/release/arsync}"

if [ ! -d "$SRC_DIR" ]; then
    echo "Error: Source directory $SRC_DIR doesn't exist"
    exit 1
fi

NUM_FILES=$(find "$SRC_DIR" -type f | wc -l)
TOTAL_MB=$((NUM_FILES * FILE_SIZE_MB))

echo "=== io_uring Operation Estimation ==="
echo "Files: $NUM_FILES × ${FILE_SIZE_MB}MB"
echo ""

# Calculate expected operations based on code patterns
CHUNK_SIZE=4096                    # Default read/write buffer
OPS_PER_FILE=$((FILE_SIZE_MB * 1024 * 1024 / CHUNK_SIZE))
READS_PER_FILE=$OPS_PER_FILE
WRITES_PER_FILE=$OPS_PER_FILE
FALLOCATE_PER_FILE=1               # One fallocate per file
STATX_PER_FILE=1                   # Phase 2: one statx per file via dirfd
FADVISE_PER_FILE=2                 # Source SEQUENTIAL + NOREUSE, Dest NOREUSE

echo "Estimated io_uring operations per file:"
echo "  READ:       $READS_PER_FILE (one per ${CHUNK_SIZE}-byte chunk)"
echo "  WRITE:      $WRITES_PER_FILE"
echo "  FALLOCATE:  $FALLOCATE_PER_FILE"
echo "  STATX:      $STATX_PER_FILE (Phase 2: via DirectoryFd)"
echo "  FADVISE:    $FADVISE_PER_FILE (if --parallel-use-dup)"
echo ""

TOTAL_OPS_PER_FILE=$((READS_PER_FILE + WRITES_PER_FILE + FALLOCATE_PER_FILE + STATX_PER_FILE))
TOTAL_OPS=$((TOTAL_OPS_PER_FILE * NUM_FILES))

echo "Total estimated io_uring operations:"
echo "  Per file:   $TOTAL_OPS_PER_FILE ops"
echo "  All files:  $TOTAL_OPS ops"
echo ""

echo "io_uring batching (from io_uring_enter):"
echo "  If batch=1:  $TOTAL_OPS io_uring_enter calls"
echo "  If batch=2:  $((TOTAL_OPS / 2)) io_uring_enter calls"
echo "  If batch=4:  $((TOTAL_OPS / 4)) io_uring_enter calls"
echo ""

echo "Operation breakdown estimate:"
echo "+---------------+----------+--------+"
echo "| Operation     | Per File |  Total |"
echo "+---------------+----------+--------+"
printf "| %-13s | %8d | %6d |\n" "READ" "$READS_PER_FILE" "$((READS_PER_FILE * NUM_FILES))"
printf "| %-13s | %8d | %6d |\n" "WRITE" "$WRITES_PER_FILE" "$((WRITES_PER_FILE * NUM_FILES))"
printf "| %-13s | %8d | %6d |\n" "FALLOCATE" "$FALLOCATE_PER_FILE" "$((FALLOCATE_PER_FILE * NUM_FILES))"
printf "| %-13s | %8d | %6d |\n" "STATX" "$STATX_PER_FILE" "$((STATX_PER_FILE * NUM_FILES))"
printf "| %-13s | %8d | %6d |\n" "FSYNC" "1" "$NUM_FILES"
echo "+---------------+----------+--------+"
printf "| %-13s | %8d | %6d |\n" "TOTAL" "$TOTAL_OPS_PER_FILE" "$TOTAL_OPS"
echo "+---------------+----------+--------+"
echo ""

echo "Note: This is an estimate. For exact counts, use:"
echo "  sudo bpftrace benchmarks/trace_io_uring_ops.bt -c '$ARSYNC /src /dst -a'"

