# /commit

Create a Conventional Commit and show a concise status summary.

- message (string, required): Use Conventional Commits: `type(scope): summary` (see [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/))
  - Types: `feat, fix, docs, style, refactor, perf, test, build, ci, chore, revert`
  - Examples: `feat(sync): add adaptive concurrency control`, `docs(readme): add benchmark results [no ci]`

```bash
/commit "fix(copy): prevent buffer overflow in large file transfers"
```

Before committing (strongly recommended):
- Rust formatting:
  - `cargo fmt --all`
- Clippy lints:
  - `cargo clippy --all-targets --all-features -- -D warnings`
- Run tests (targeted to changed areas):
  - `cargo test` or `cargo test <module_name>` or `cargo test <test_name>`
- Build check:
  - `cargo build --all-features`
- For crate-specific work:
  - `cd crates/<crate-name> && cargo test`

Pre-commit reminder:
- If you have pre-commit hooks installed: `pre-commit run`
- Run across repo: `pre-commit run --all-files`

Notes:
- Docs-only commits should include `[no ci]` in the subject when appropriate
- If hooks fail, fix the underlying issues; do not bypass with `--no-verify`
- Breaking changes should use `!` after type: `feat(api)!: change CLI argument format`

