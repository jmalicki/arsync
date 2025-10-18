#!/bin/bash
# Clean restart of benchmark with updated scripts

set -euo pipefail

echo "=== Cleaning Up Old Test Data and Results ==="
sudo rm -rf /mnt/newhome/benchmark-data-quick
sudo rm -rf /mnt/newhome/benchmark-output-quick
rm -rf benchmark-results-quick-*
echo "✓ Cleaned up"
echo ""

echo "=== Building Fresh Release Binary ==="
cargo build --release
echo "✓ Binary ready"
echo ""

echo "=== Generating NEW Test Data with Multiple Large Files ==="
echo "This will create 5× 5GB files (more realistic than 1× 10GB)"
sudo ./benchmarks/generate_testdata_quick.sh /mnt/newhome
echo "✓ Test data ready"
echo ""

echo "=== Running Quick Benchmark (~30 min) ==="
echo "Will show cache dropping before EVERY test"
echo ""
sudo ./benchmarks/run_benchmarks_quick.sh \
    /mnt/newhome/benchmark-data-quick \
    /mnt/newhome/benchmark-output-quick \
    ./benchmark-results-quick-$(date +%Y%m%d_%H%M%S)

echo ""
echo "=== Analyzing Results ==="
RESULTS=$(ls -dt benchmark-results-quick-* | head -1)
python3 ./benchmarks/analyze_results.py "$RESULTS"

echo ""
echo "========================================="
echo "BENCHMARK COMPLETE!"
echo "========================================="
echo ""
echo "Review results:"
echo "  cat $RESULTS/final_report.txt"
echo "  cat $RESULTS/hardware_detailed.txt"
echo ""
