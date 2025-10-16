# /pr-ready

Push current branch and ensure it has an open PR; print PR URL and CI runs.

- title (string, optional): PR title if a PR needs creating

```bash
/pr-ready "feat(sync): adaptive concurrency control"
```

Before running:
- Ensure all tests pass: `cargo test`
- Check formatting: `cargo fmt --all -- --check`
- Run clippy: `cargo clippy --all-targets --all-features -- -D warnings`
- Review your changes: `git diff main...HEAD`

