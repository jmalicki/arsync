#!/bin/bash
# DEPRECATED: This shell script is no longer used.
# 
# The Rust binary at benchmarks/target/release/syscall-analyzer
# now does everything this script did (and more):
# - Creates test dataset
# - Runs strace on arsync
# - Parses results
# - Generates markdown reports
#
# Usage:
#   cd benchmarks && cargo build --release
#   ./target/release/syscall-analyzer --arsync-bin ../target/release/arsync
#
# See benchmarks/README.md for full documentation.

echo "⚠️  This shell script is deprecated."
echo "Use: ./benchmarks/target/release/syscall-analyzer instead"
echo "See: ./benchmarks/README.md for documentation"
exit 1
