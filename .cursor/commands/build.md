# /build

Build the project with common configurations.

- profile (string, optional): Build profile - "debug" or "release" (default: "debug")
- features (string, optional): Feature flags to enable - "all" or specific features
- check_only (boolean, optional): Only check compilation without producing binary (default: false)

```bash
/build "release" "all" false
/build "debug" "" true
```

Common build patterns:
- Debug build: `cargo build`
- Release build: `cargo build --release`
- All features: `cargo build --all-features`
- Check only: `cargo check --all-features`
- Specific crate: `cargo build -p <crate-name>`
- Clean build: `cargo clean && cargo build --release`

Build profiles:
- **Debug**: Fast compile, includes debug symbols, no optimization
- **Release**: Optimized, slower compile, production-ready

Before building:
- Format check: `cargo fmt --all -- --check`
- Clippy check: `cargo clippy --all-targets --all-features -- -D warnings`

Output locations:
- Debug: `target/debug/arsync`
- Release: `target/release/arsync`

Notes:
- Release builds are significantly faster at runtime
- Use release builds for benchmarking
- Check builds are faster for catching compilation errors

