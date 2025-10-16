# /clippy

Run Clippy linter to catch common mistakes and improve code quality.

- fix (boolean, optional): Automatically apply Clippy suggestions (default: false)
- pedantic (boolean, optional): Enable pedantic lints (default: false)

```bash
/clippy false false  # Run clippy
/clippy true false   # Auto-fix issues
```

Common patterns:
- Standard run: `cargo clippy --all-targets --all-features`
- Deny warnings: `cargo clippy --all-targets --all-features -- -D warnings`
- Auto-fix: `cargo clippy --fix --all-targets --all-features`
- Pedantic: `cargo clippy -- -W clippy::pedantic`
- Specific package: `cargo clippy -p <package-name>`

Clippy configuration:
- Configure in `clippy.toml` (you have one)
- Or use attributes: `#[allow(clippy::lint_name)]`

Before committing:
1. Run clippy: `/clippy false false`
2. Fix all warnings
3. Verify CI will pass: `cargo clippy --all-targets --all-features -- -D warnings`

Common categories:
- **Correctness**: Potential bugs (deny by default)
- **Suspicious**: Likely bugs (warn by default)
- **Complexity**: Unnecessarily complex code
- **Perf**: Performance issues
- **Style**: Code style improvements
- **Pedantic**: Nitpicky suggestions (opt-in)

Tips:
- CI fails on clippy warnings (`-D warnings`)
- Fix clippy issues before requesting PR review
- Use `--fix` carefully; review changes before committing

