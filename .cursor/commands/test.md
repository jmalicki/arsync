# /test

Run project tests with sensible defaults and common patterns.

- scope (string, optional): Test scope - "all", "unit", "integration", module name, or specific test name
- package (string, optional): Specific package/crate to test (e.g., "compio-fs-extended")

```bash
/test "all"
/test "copy"
/test "test_metadata_preservation"
/test "all" "compio-fs-extended"
```

Common test patterns:
- All tests: `cargo test --all-features`
- Single module: `cargo test <module_name>`
- Specific test: `cargo test <test_name>`
- Integration tests: `cargo test --test '*'`
- Specific integration test: `cargo test --test integration_tests`
- With output: `cargo test -- --nocapture`
- Specific crate: `cd crates/<name> && cargo test`

Before running tests:
- Ensure code compiles: `cargo build --all-features`
- Format code: `cargo fmt --all`
- Run clippy: `cargo clippy --all-targets --all-features`

Notes:
- Use `--nocapture` to see println! output during tests
- Use `RUST_BACKTRACE=1` for detailed error traces
- Some integration tests may require specific test data or permissions

