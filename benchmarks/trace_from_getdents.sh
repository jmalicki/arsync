#!/bin/bash
# Trace syscalls starting from actual directory traversal work

if [ $# -lt 2 ]; then
    echo "Usage: $0 <src_dir> <dst_dir> [arsync_flags...]"
    exit 1
fi

SRC_DIR="$1"
DST_DIR="$2"
shift 2

TRACE_FILE="/tmp/work-only-trace.txt"
FULL_TRACE="/tmp/full-trace-temp.txt"

# Run with full trace
strace -f -tt -T -o "$FULL_TRACE" ./target/release/arsync "$SRC_DIR" "$DST_DIR" "$@" 2>&1

# Find first getdents64 (start of directory traversal)
MARKER_LINE=$(grep -n "getdents64" "$FULL_TRACE" | head -1 | cut -d: -f1)

if [ -n "$MARKER_LINE" ]; then
    # Extract just the work phase
    tail -n +$MARKER_LINE "$FULL_TRACE" > "$TRACE_FILE"
    
    TOTAL_LINES=$(wc -l < "$FULL_TRACE")
    WORK_LINES=$(wc -l < "$TRACE_FILE")
    INIT_LINES=$((TOTAL_LINES - WORK_LINES))
    
    echo "✅ Filtered trace written to: $TRACE_FILE"
    echo ""
    echo "📊 Statistics:"
    echo "  - Initialization syscalls: $INIT_LINES (skipped)"
    echo "  - Work syscalls: $WORK_LINES (captured)"
    echo "  - Reduction: $((INIT_LINES * 100 / TOTAL_LINES))% of noise removed"
    echo ""
    echo "First 50 lines of actual work:"
    head -50 "$TRACE_FILE"
else
    echo "❌ Could not find getdents64 marker"
    echo "Showing all syscalls:"
    head -50 "$FULL_TRACE"
fi
