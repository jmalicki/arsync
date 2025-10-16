# /fmt

Format Rust code using rustfmt.

- check (boolean, optional): Only check formatting without modifying files (default: false)
- all (boolean, optional): Format all packages including workspace members (default: true)

```bash
/fmt false true  # Format all code
/fmt true true   # Check formatting without changes
```

Common patterns:
- Format all: `cargo fmt --all`
- Check only: `cargo fmt --all -- --check`
- Single package: `cargo fmt -p <package-name>`
- Format and show diff: `cargo fmt --all -- --check --verbose`

Before committing:
1. Format code: `/fmt false true`
2. Verify: `/fmt true true`
3. Or use both: `cargo fmt --all && cargo fmt --all -- --check`

CI Integration:
- GitHub Actions runs `cargo fmt -- --check`
- Pre-commit hooks may auto-format
- Always format before pushing

Tips:
- Configure rustfmt with `rustfmt.toml` or `.rustfmt.toml`
- Use `#[rustfmt::skip]` to skip specific items
- Format on save in your editor for best workflow

