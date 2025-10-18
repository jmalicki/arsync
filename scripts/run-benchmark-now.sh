#!/bin/bash
# Quick benchmark run script
set -euo pipefail

cd /home/jmalicki/src/io_uring_sync

echo "=== Step 1: Build Release Binary ==="
cargo build --release
echo "✓ Binary ready: $(ls -lh target/release/arsync | awk '{print $9, $5}')"
echo ""

echo "=== Step 2: Generate Test Data (~5-10 min) ==="
sudo ./benchmarks/generate_testdata_quick.sh /mnt/newhome
echo "✓ Test data ready"
echo ""

echo "=== Step 3: Run Quick Benchmark (~25-30 min) ==="
echo "This will test power monitoring and capture full hardware inventory"
echo ""
sudo ./benchmarks/run_benchmarks_quick.sh \
    /mnt/newhome/benchmark-data-quick \
    /mnt/newhome/benchmark-output-quick \
    ./benchmark-results-quick-$(date +%Y%m%d_%H%M%S)

echo ""
echo "=== Step 4: Analyze Results ==="
RESULTS=$(ls -dt benchmark-results-quick-* | head -1)
python3 ./benchmarks/analyze_results.py "$RESULTS"

echo ""
echo "========================================="
echo "BENCHMARK COMPLETE!"
echo "========================================="
echo ""
echo "Results: $RESULTS"
echo ""
echo "Key files to review:"
echo "  - $RESULTS/final_report.txt         # Performance summary"
echo "  - $RESULTS/hardware_detailed.txt    # Your hardware specs"
echo "  - $RESULTS/*/run2_power.csv         # Power measurements (if RAPL available)"
echo ""
echo "Next steps:"
echo "  cat $RESULTS/final_report.txt"
echo "  cat $RESULTS/hardware_detailed.txt"
echo ""
