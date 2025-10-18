#!/bin/bash
# Automated syscall analysis for arsync - suitable for CI
# 
# This script runs arsync with strace and analyzes syscall patterns to ensure:
# - Efficient io_uring usage
# - FD-based metadata operations (TOCTOU-safe)
# - Minimal redundant syscalls

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Test configuration
TEST_DIR_SRC="${1:-/tmp/syscall-test-src}"
TEST_DIR_DST="${2:-/tmp/syscall-test-dst}"
NUM_FILES="${3:-5}"
FILE_SIZE_MB="${4:-10}"
ARSYNC_BIN="${5:-./target/release/arsync}"

# Output files
TRACE_RAW="/tmp/syscall-analysis-raw.txt"
TRACE_SUMMARY="/tmp/syscall-analysis-summary.txt"
REPORT="/tmp/syscall-analysis-report.txt"

# Exit codes
EXIT_SUCCESS=0
EXIT_WARNING=1
EXIT_FAILURE=2

exit_code=$EXIT_SUCCESS

echo "============================================"
echo "arsync Syscall Analysis"
echo "============================================"
echo ""

# Create test dataset
echo "Creating test dataset..."
rm -rf "$TEST_DIR_SRC" "$TEST_DIR_DST"
mkdir -p "$TEST_DIR_SRC"

for i in $(seq 1 "$NUM_FILES"); do
    dd if=/dev/urandom of="$TEST_DIR_SRC/file${i}.bin" bs=1M count="$FILE_SIZE_MB" 2>/dev/null
done

echo "✓ Created $NUM_FILES files × ${FILE_SIZE_MB}MB = $((NUM_FILES * FILE_SIZE_MB))MB"
echo ""

## Run arsync with full syscall trace
echo "Running arsync with strace..."
strace -c -f -o "$TRACE_SUMMARY" "$ARSYNC_BIN" "$TEST_DIR_SRC" "$TEST_DIR_DST" -a > /dev/null 2>&1 || true
strace -e trace=all -f -o "$TRACE_RAW" "$ARSYNC_BIN" "$TEST_DIR_SRC" "$TEST_DIR_DST" -a > /dev/null 2>&1 || true

echo "✓ Trace captured"
echo ""

# Extract counts (use wc -l instead of grep -c to avoid multi-line output on failures)
# Note: All these commands should not fail even if pattern not found (wc -l returns 0)
total_syscalls=$(grep -v "^---" "$TRACE_SUMMARY" 2>/dev/null | grep -v "^%" | grep -v "^$" | wc -l || echo 0)
statx_count=$(grep "statx" "$TRACE_RAW" 2>/dev/null | wc -l || echo 0)
openat_count=$(grep "openat" "$TRACE_RAW" 2>/dev/null | wc -l || echo 0)
io_uring_enter_count=$(grep "io_uring_enter" "$TRACE_RAW" 2>/dev/null | wc -l || echo 0)
io_uring_setup_count=$(grep "io_uring_setup" "$TRACE_RAW" 2>/dev/null | wc -l || echo 0)
fallocate_count=$(grep "^[0-9].*fallocate" "$TRACE_RAW" 2>/dev/null | wc -l || echo 0)
fchmod_count=$(grep "fchmod" "$TRACE_RAW" 2>/dev/null | wc -l || echo 0)
fchown_count=$(grep "fchown" "$TRACE_RAW" 2>/dev/null | wc -l || echo 0)
utimensat_count=$(grep "utimensat" "$TRACE_RAW" 2>/dev/null | wc -l || echo 0)

# Count path-based vs FD-based operations
statx_path_based=$(grep "statx(AT_FDCWD" "$TRACE_RAW" 2>/dev/null | grep -v '""' | wc -l || echo 0)
statx_fd_based=$(grep "statx([0-9]" "$TRACE_RAW" 2>/dev/null | wc -l || echo 0)
openat_path_based=$(grep 'openat(AT_FDCWD, "/' "$TRACE_RAW" 2>/dev/null | grep -vE "(/etc|/lib|/proc|/sys)" | wc -l || echo 0)
utimensat_fd_based=$(grep 'utimensat([0-9][0-9]*, NULL' "$TRACE_RAW" 2>/dev/null | wc -l || echo 0)
utimensat_path_based=$(grep 'utimensat(AT_FDCWD, "/' "$TRACE_RAW" 2>/dev/null | wc -l || echo 0)

# Calculate per-file averages (protect against division by zero)
if [ "$NUM_FILES" -gt 0 ]; then
    statx_per_file=$(echo "scale=1; $statx_count / $NUM_FILES" | bc 2>/dev/null || echo "0.0")
    openat_per_file=$(echo "scale=1; $openat_path_based / $NUM_FILES" | bc 2>/dev/null || echo "0.0")
else
    statx_per_file="0.0"
    openat_per_file="0.0"
fi

# Extract io_uring batching info
# io_uring_enter(fd, to_submit, min_complete, ...) - 2nd param is number of ops submitted
io_uring_batch_sizes=$(grep "io_uring_enter" "$TRACE_RAW" 2>/dev/null | sed 's/.*io_uring_enter([0-9]*, \([0-9]*\),.*/\1/' | grep -E '^[0-9]+$' || echo "")
io_uring_batch_1=$(echo "$io_uring_batch_sizes" | grep -c "^1$" 2>/dev/null || echo 0)
io_uring_batch_2plus=$(echo "$io_uring_batch_sizes" | grep -v "^1$" | grep -v "^0$" | wc -l 2>/dev/null || echo 0)
io_uring_batch_max=$(echo "$io_uring_batch_sizes" | sort -n | tail -1 2>/dev/null || echo 0)
io_uring_batch_avg=$(echo "$io_uring_batch_sizes" | awk '{sum+=$1; count++} END {if(count>0) printf "%.1f", sum/count; else print "0"}' 2>/dev/null || echo "0.0")

# Generate report
{
    echo "============================================"
    echo "arsync Syscall Analysis Report"
    echo "============================================"
    echo "Date: $(date)"
    echo "Test: $NUM_FILES files × ${FILE_SIZE_MB}MB"
    echo "Binary: $ARSYNC_BIN"
    echo ""
    
    echo "--- IO_URING USAGE ---"
    echo "io_uring_setup calls: $io_uring_setup_count (one per worker thread + main)"
    echo "io_uring_enter calls: $io_uring_enter_count"
    
    if [ "$io_uring_enter_count" -gt 100 ]; then
        echo "  ✅ PASS: Heavy io_uring usage"
    else
        echo "  ❌ FAIL: Low io_uring usage (expected >100 for ${NUM_FILES} files)"
        exit_code=$EXIT_FAILURE
    fi
    
    echo ""
    echo "io_uring batching efficiency:"
    echo "  Single-op submissions (batch=1): $io_uring_batch_1"
    echo "  Multi-op submissions (batch≥2): $io_uring_batch_2plus"
    echo "  Average batch size: $io_uring_batch_avg ops/submit"
    echo "  Maximum batch size: $io_uring_batch_max ops/submit"
    
    if [ "$io_uring_batch_avg" = "1.0" ] || [ "${io_uring_batch_avg%.*}" -eq 1 ] 2>/dev/null; then
        echo "  ⚠️  WARNING: Poor batching (avg=1.0, mostly single-op submissions)"
        echo "             Better batching could reduce syscall overhead"
    elif [ "${io_uring_batch_avg%.*}" -ge 3 ] 2>/dev/null; then
        echo "  ✅ EXCELLENT: Good batching (avg≥3 ops/submit)"
    else
        echo "  ✅ GOOD: Decent batching (avg>1 ops/submit)"
    fi
    echo ""
    
    echo "--- METADATA OPERATIONS (PER FILE) ---"
    echo "Total statx calls: $statx_count"
    echo "  Path-based (AT_FDCWD + path): $statx_path_based"
    echo "  FD-based (fd + \"\" or fd + NULL): $statx_fd_based"
    echo "  Average per file: $statx_per_file"
    
    # Per-file breakdown of operations
    echo ""
    echo "Per-file syscall breakdown (first 3 files):"
    for i in 1 2 3; do
        if [ -f "$TEST_DIR_SRC/file${i}.bin" ]; then
            file_statx=$(grep "statx.*file${i}\.bin" "$TRACE_RAW" | wc -l)
            file_openat=$(grep "openat.*file${i}\.bin" "$TRACE_RAW" | wc -l)
            file_io_uring=$(grep "file${i}\.bin" "$TRACE_RAW" | wc -l)
            echo "  file${i}.bin:"
            echo "    - statx: $file_statx"
            echo "    - openat: $file_openat"
            echo "    - total mentions: $file_io_uring"
        fi
    done
    echo ""
    
    echo "Per-directory syscall breakdown:"
    dir_statx=$(grep "statx.*\"$TEST_DIR_SRC\"" "$TRACE_RAW" | wc -l)
    dir_openat=$(grep "openat.*\"$TEST_DIR_SRC\".*O_DIRECTORY" "$TRACE_RAW" | wc -l)
    dir_getdents=$(grep "getdents" "$TRACE_RAW" | wc -l)
    dir_fchmod=$(grep "fchmod.*" "$TRACE_RAW" | wc -l)
    dir_fchown=$(grep "fchown.*" "$TRACE_RAW" | wc -l)
    echo "  Source directory ($TEST_DIR_SRC):"
    echo "    - statx: $dir_statx"
    echo "    - openat (O_DIRECTORY): $dir_openat"
    echo "    - getdents64 (directory reads): $dir_getdents"
    echo "  Destination directory ($TEST_DIR_DST):"
    echo "    - fchmod: $dir_fchmod (includes files)"
    echo "    - fchown: $dir_fchown (includes files)"
    
    if [ "$statx_path_based" -gt "$((NUM_FILES * 2))" ]; then
        echo "  ⚠️  WARNING: High path-based statx count (TOCTOU-vulnerable)"
        echo "             Expected: ≤$((NUM_FILES * 2)) (1-2 per file)"
        echo "             Got: $statx_path_based (~$(echo "scale=1; $statx_path_based / $NUM_FILES" | bc) per file)"
        exit_code=$EXIT_WARNING
    elif [ "$statx_path_based" -eq 0 ]; then
        echo "  ✅ PASS: Zero path-based statx (100% TOCTOU-safe)"
    else
        echo "  ⚠️  ACCEPTABLE: Minimal path-based statx"
    fi
    echo ""
    
    echo "--- FILE OPERATIONS ---"
    echo "Total openat calls: $openat_count"
    echo "  User file opens (path-based): $openat_path_based"
    echo "  Average per file: $openat_per_file"
    
    if [ "$openat_path_based" -gt "$((NUM_FILES * 4))" ]; then
        echo "  ⚠️  WARNING: Excessive openat calls"
        echo "             Expected: ≤$((NUM_FILES * 4)) (2 per file: src+dst)"
        echo "             Got: $openat_path_based"
        exit_code=$EXIT_WARNING
    else
        echo "  ✅ PASS: Reasonable openat count"
    fi
    echo ""
    
    echo "Direct fallocate syscalls: $fallocate_count"
    if [ "$fallocate_count" -gt 0 ]; then
        echo "  ⚠️  WARNING: fallocate not using io_uring"
        exit_code=$EXIT_WARNING
    else
        echo "  ✅ PASS: fallocate via io_uring (no direct syscalls)"
    fi
    echo ""
    
    echo "--- METADATA PRESERVATION ---"
    echo "fchmod (FD-based permissions): $fchmod_count"
    echo "fchown (FD-based ownership): $fchown_count"
    echo "utimensat calls: $utimensat_count"
    echo "  FD-based (fd, NULL, ...): $utimensat_fd_based"
    echo "  Path-based (AT_FDCWD, path, ...): $utimensat_path_based"
    
    if [ "$utimensat_path_based" -gt 0 ]; then
        echo "  ❌ FAIL: Path-based utimensat detected (TOCTOU-vulnerable)"
        exit_code=$EXIT_FAILURE
    elif [ "$utimensat_fd_based" -eq "$NUM_FILES" ]; then
        echo "  ✅ PASS: 100% FD-based timestamp preservation"
    else
        echo "  ⚠️  INFO: FD-based timestamps: $utimensat_fd_based (expected: $NUM_FILES)"
    fi
    echo ""
    
    echo "--- SYSCALL EFFICIENCY METRICS ---"
    echo "Total syscalls: $total_syscalls"
    echo "io_uring percentage: $(echo "scale=1; $io_uring_enter_count * 100 / $total_syscalls" | bc)%"
    
    io_uring_pct=$(echo "scale=0; $io_uring_enter_count * 100 / $total_syscalls" | bc)
    if [ "$io_uring_pct" -lt 30 ]; then
        echo "  ⚠️  WARNING: Low io_uring usage (<30%)"
        exit_code=$EXIT_WARNING
    else
        echo "  ✅ PASS: Good io_uring usage (≥30%)"
    fi
    echo ""
    
    echo "--- SECURITY ASSESSMENT ---"
    security_score=100
    
    if [ "$statx_path_based" -gt "$((NUM_FILES * 2))" ]; then
        echo "  ⚠️  Path-based statx: TOCTOU risk"
        security_score=$((security_score - 20))
    fi
    
    if [ "$utimensat_path_based" -gt 0 ]; then
        echo "  ❌ Path-based utimensat: TOCTOU risk"
        security_score=$((security_score - 30))
    fi
    
    if [ "$openat_path_based" -gt "$((NUM_FILES * 2))" ]; then
        echo "  ⚠️  All file opens use AT_FDCWD (not dirfd-relative)"
        security_score=$((security_score - 20))
    fi
    
    if [ $security_score -eq 100 ]; then
        echo "  ✅ Security score: $security_score/100 - Excellent (100% FD-based)"
    elif [ $security_score -ge 70 ]; then
        echo "  ⚠️  Security score: $security_score/100 - Good (mostly FD-based)"
    else
        echo "  ❌ Security score: $security_score/100 - Needs improvement"
    fi
    echo ""
    
    echo "--- RECOMMENDATIONS ---"
    if [ "$statx_path_based" -gt "$NUM_FILES" ]; then
        echo "  • Reduce redundant statx calls (currently ~$(echo "scale=1; $statx_path_based / $NUM_FILES" | bc) per file)"
        echo "    Target: 1 statx per file via DirectoryFd::statx()"
    fi
    
    if [ "$openat_path_based" -gt 0 ]; then
        echo "  • Use dirfd-relative openat() instead of AT_FDCWD + absolute paths"
        echo "    Benefits: TOCTOU-safe, potentially async via io_uring"
    fi
    
    if [ "$utimensat_path_based" -gt 0 ]; then
        echo "  • Use FD-based futimens() instead of path-based utimensat()"
    fi
    
    if [ $security_score -lt 100 ]; then
        echo "  • See docs/DIRFD_IO_URING_ARCHITECTURE.md for implementation plan"
    fi
    echo ""
    
    echo "--- OVERALL RESULT ---"
    if [ $exit_code -eq $EXIT_SUCCESS ]; then
        echo -e "${GREEN}✅ PASS${NC} - All checks passed"
    elif [ $exit_code -eq $EXIT_WARNING ]; then
        echo -e "${YELLOW}⚠️  WARNING${NC} - Some improvements recommended"
    else
        echo -e "${RED}❌ FAIL${NC} - Critical issues detected"
    fi
    echo ""
    
    echo "Full traces available:"
    echo "  - Summary: $TRACE_SUMMARY"
    echo "  - Detailed: $TRACE_RAW"
    
} | tee "$REPORT"

# Show summary table
echo ""
echo "=== Quick Reference Table ==="
echo "+-------------------------+--------+--------+---------+"
echo "| Operation               | Count  | Target | Status  |"
echo "+-------------------------+--------+--------+---------+"
printf "| %-23s | %6d | %6s | " "io_uring_enter" "$io_uring_enter_count" ">100"
[ "$io_uring_enter_count" -gt 100 ] && echo -e "${GREEN}PASS${NC}   |" || echo -e "${RED}FAIL${NC}   |"

printf "| %-23s | %6d | %6s | " "statx (total)" "$statx_count" "<20"
[ "$statx_count" -lt 20 ] && echo -e "${GREEN}PASS${NC}   |" || echo -e "${YELLOW}WARN${NC}   |"

printf "| %-23s | %6d | %6s | " "statx (path-based)" "$statx_path_based" "=0"
[ "$statx_path_based" -eq 0 ] && echo -e "${GREEN}PASS${NC}   |" || echo -e "${YELLOW}WARN${NC}   |"

printf "| %-23s | %6d | %6s | " "openat (user files)" "$openat_path_based" "<20"
[ "$openat_path_based" -lt 20 ] && echo -e "${GREEN}PASS${NC}   |" || echo -e "${YELLOW}WARN${NC}   |"

printf "| %-23s | %6d | %6s | " "fallocate (direct)" "$fallocate_count" "=0"
[ "$fallocate_count" -eq 0 ] && echo -e "${GREEN}PASS${NC}   |" || echo -e "${RED}FAIL${NC}   |"

printf "| %-23s | %6d | %6s | " "utimensat (path-based)" "$utimensat_path_based" "=0"
[ "$utimensat_path_based" -eq 0 ] && echo -e "${GREEN}PASS${NC}   |" || echo -e "${RED}FAIL${NC}   |"

printf "| %-23s | %6d | %6s | " "utimensat (FD-based)" "$utimensat_fd_based" "=$NUM_FILES"
[ "$utimensat_fd_based" -eq "$NUM_FILES" ] && echo -e "${GREEN}PASS${NC}   |" || echo -e "${YELLOW}WARN${NC}   |"

echo "+-------------------------+--------+--------+---------+"
echo ""

echo "Report saved to: $REPORT"
exit $exit_code

