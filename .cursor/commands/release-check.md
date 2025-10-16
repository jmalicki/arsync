# /release-check

Pre-release checklist to verify project is ready for release.

```bash
/release-check
```

This command should verify:

## Code Quality
- [ ] All tests pass: `cargo test --all-features`
- [ ] Clippy clean: `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] Formatted: `cargo fmt --all -- --check`
- [ ] Docs build: `cargo doc --all-features`
- [ ] No warnings: `cargo build --all-features --release 2>&1 | grep -i warning`

## Functionality
- [ ] Smoke tests pass: `./benchmarks/smoke_test.sh`
- [ ] Benchmarks run: `./benchmarks/run_benchmarks_quick.sh`
- [ ] Integration tests pass: `cargo test --test '*'`

## Documentation
- [ ] CHANGELOG.md updated with changes
- [ ] README.md accurate and up-to-date
- [ ] Version numbers updated in `Cargo.toml`
- [ ] API docs reviewed: `cargo doc --open`
- [ ] Examples work and are documented

## CI/CD
- [ ] GitHub Actions passing on main
- [ ] All PRs merged
- [ ] No pending security issues: `cargo audit`
- [ ] Dependencies up to date: `cargo outdated`

## Version & Git
- [ ] Version bumped appropriately (semver)
- [ ] Git tag created: `git tag -a v<version> -m "Release v<version>"`
- [ ] All changes committed
- [ ] Clean working directory: `git status`

## Release artifacts
- [ ] Release build tested: `cargo build --release`
- [ ] Binary works: `./target/release/arsync --version`
- [ ] Cross-compilation verified (if applicable)

Quick commands:
```bash
# Run all checks
cargo test --all-features && \
cargo clippy --all-targets --all-features -- -D warnings && \
cargo fmt --all -- --check && \
cargo doc --all-features && \
./benchmarks/smoke_test.sh

# Check for issues
cargo audit
cargo outdated

# Create release
git tag -a v1.0.0 -m "Release v1.0.0"
git push origin v1.0.0
```

After release:
- Monitor GitHub Actions release workflow
- Verify release artifacts
- Update documentation sites
- Announce release

