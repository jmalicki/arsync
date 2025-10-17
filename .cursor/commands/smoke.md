# /smoke

Run smoke tests to verify basic functionality.

```bash
/smoke
```

This runs the smoke test suite located at `benchmarks/smoke_test.sh` which performs quick sanity checks on core functionality.

What smoke tests typically verify:
- Basic file copying works
- Metadata preservation functions
- CLI argument parsing
- Core sync operations
- Error handling for common cases

Run this before:
- Opening a PR
- After major refactoring
- Before running full test suite or benchmarks

Quick validation:
```bash
./benchmarks/smoke_test.sh
```

If smoke tests pass, proceed with:
- Full tests: `/test "all"`
- Benchmarks: `/bench true false`

