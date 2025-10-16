# /bench

Run benchmarks for the io_uring_sync project.

- quick (boolean, optional): Run quick benchmark suite (default: false)
- full (boolean, optional): Run full comprehensive benchmark suite (default: false)

```bash
/bench true false
/bench false true
```

Benchmark options:
- Quick benchmarks: `./benchmarks/run_benchmarks_quick.sh`
  - Faster, smaller test data
  - Good for iterative development
  - Located in `benchmarks/run_benchmarks_quick.sh`

- Full benchmarks: `./benchmarks/run_benchmarks.sh`
  - Comprehensive, larger test data
  - Full performance evaluation
  - Takes longer to run
  - Located in `benchmarks/run_benchmarks.sh`

- Generate test data:
  - Quick: `./benchmarks/generate_testdata_quick.sh`
  - Full: `./benchmarks/generate_testdata.sh`

Before benchmarking:
- Build release binary: `cargo build --release`
- Ensure clean test environment
- Review benchmark configuration in `benchmarks/TEST_MATRIX.md`

Results location:
- Results stored in `benchmark-results-*/` directories
- Analysis: `python benchmarks/analyze_results.py`
- See `benchmarks/README.md` for details

Notes:
- Benchmarks compare against rsync
- Monitor system resources during benchmarking
- Run on dedicated/idle system for consistent results

