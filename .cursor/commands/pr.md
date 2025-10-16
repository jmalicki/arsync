# /pr

Create or update a GitHub Pull Request from the current branch using gh, with a templated body, and enforce PR best practices.

Note: Use the GitHub CLI (`gh`) for all PR/CI operations.

- title (string, required): PR title (Conventional Commits)
- body (string, optional): PR body (markdown). If omitted, generate from recent commits + diff
- base (string, optional): Base branch (default: main)
- draft (boolean, optional): Open as draft (default: false)

```bash
/pr "feat(sync): add adaptive concurrency control for io_uring operations" "See template below" main true
```

## Best practices
- Conventional Commits for PR titles: feat|fix|docs|chore|refactor|perf|test|style|ci|build|revert(optional scope)
  - Examples: `feat(copy): add zero-copy optimization`, `docs(benchmark): add performance comparison [no ci]`
- Single concern PRs: split independent fixes/features into separate PRs
- Explain the why: focus on motivation and impact, not just diffs
- Tests are mandatory: add/update unit/integration tests; don't bypass pre-commit
- Docs and config: update docs and workflows when behavior changes
- Security: call out security implications, unsafe code usage, or privilege changes
- CI etiquette: docs-only PRs use `[no ci]` in the title; watch checks before merging

## Body template
```md
## Summary
- <1–3 bullets explaining the change and its user impact>

## Motivation
- <Why this is needed; link issues/contexts>

## Changes
- <High-level list of significant changes>

## Test plan
- Unit tests: <commands / scope>
  - `cargo test <module_or_test_name>`
- Integration tests: <commands / scope>
  - `cargo test --test <integration_test>`
- Manual testing: <specific scenarios tested>
- Benchmarks (if applicable): <performance measurements>

## Risks & Rollback
- Risks: <perf, security, compatibility, breaking changes>
- Rollback plan: <how to revert safely>

## Performance Impact
- Benchmark results: <before/after metrics if applicable>
- Memory usage: <changes in memory footprint>
- I/O patterns: <changes in I/O behavior>

## Links
- Closes #<id>
- Related: <links>
```

## Helper commands
- Create/ensure PR and view CI: `/pr-ready "feat(...): ..."`
- Watch PR checks: `/pr-checks`
- Show latest CI runs: `/ci-latest`

