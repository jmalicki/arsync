# /clean

Clean build artifacts, test data, and temporary files.

- full (boolean, optional): Full clean including cargo cache (default: false)
- target_only (boolean, optional): Only clean target directory (default: true)

```bash
/clean false true   # Clean target/ only
/clean true false   # Full clean including all artifacts
```

What gets cleaned:
- `target/` - Compiled artifacts
- `benchmark-results-*/` - Benchmark result directories
- `*.log` - Log files
- Test data directories (if full=true)

Common patterns:
- Standard clean: `cargo clean`
- Clean specific package: `cargo clean -p <package-name>`
- Clean release only: `cargo clean --release`
- Clean doc: `cargo clean --doc`

When to clean:
- Build artifacts are stale or corrupted
- Switching between major dependency versions
- Disk space is low
- Before running fresh benchmarks
- Troubleshooting build issues

Full clean command:
```bash
cargo clean && \
rm -rf benchmark-results-*/ && \
rm -f *.log && \
find . -type d -name "target" -exec rm -rf {} +
```

After cleaning:
- Rebuild: `/build "release" "all" false`
- Re-run tests: `/test "all"`

Warning:
- Full clean requires rebuilding everything (slow)
- Only do full clean when necessary
- Consider `cargo clean -p <package>` for targeted cleaning

