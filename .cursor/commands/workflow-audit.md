# /workflow-audit

Audit `.github/workflows` for correctness, hygiene, and CI efficiency, and propose concrete fixes.

What this should do:
- Parse every workflow (`.yml/.yaml`) and produce a findings list with file, issue, severity, and suggested fix
- Propose minimal diffs for each fix (ready to commit), and group them into a single PR
- Use GitHub CLI for visibility (`gh workflow list/view`, `gh run list/view`)

Checks and suggested remediations:
- Triggers (on:)
  - Missing or empty triggers → add `pull_request`, `push`, and `workflow_dispatch` as applicable
  - Missing path filters for heavy workflows → add `paths`/`paths-ignore` to limit noise
  - Example:
    ```yaml
    on:
      pull_request:
        types: [opened, synchronize, reopened]
        paths:
          - "src/**"
          - "Cargo.toml"
          - "Cargo.lock"
      push:
        branches: [main]
      workflow_dispatch:
    ```
- Names
  - Missing `name:` at workflow or job level → add descriptive names
- Jobs
  - Missing `runs-on` → set appropriate runner (e.g., `ubuntu-latest`)
  - Missing or empty `steps` → add at least one step; verify each step has either `uses:` or `run:`
  - Missing `timeout-minutes` → add sensible caps (e.g., 30 for build/test, 60 for benchmarks)
- Permissions (principle of least privilege)
  - Missing `permissions:` → add minimal permissions at workflow root
    ```yaml
    permissions:
      contents: read
    ```
- Concurrency (avoid duplicate runs)
  - Missing `concurrency:` → add safe group with cancel-in-progress
    ```yaml
    concurrency:
      group: ${{ github.workflow }}-${{ github.ref }}
      cancel-in-progress: true
    ```
- Caching (speed)
  - Rust projects should cache cargo registry and build artifacts
    ```yaml
    - uses: actions/cache@v4
      with:
        path: |
          ~/.cargo/registry
          ~/.cargo/git
          target
        key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}
    ```
- Action pinning
  - Prefer major version pins (or SHAs if policy requires)
  - Example: `uses: actions/checkout@v4`, `uses: actions-rs/toolchain@v1`
- Docs-only optimization
  - For heavy workflows, ignore docs changes
    ```yaml
    on:
      pull_request:
        paths-ignore: ["**/*.md", "docs/**"]
    ```

Example usage (single line):
```bash
/workflow-audit && /pr "ci: workflow hygiene fixes" "Apply triggers/permissions/concurrency/timeouts; reduce CI noise" && /pr-ready "ci: workflow hygiene fixes" && /pr-checks
```

Expected output:
- A concise table of findings with per-file issues and severities
- Inline suggested diffs for each affected workflow
- A ready PR plan title like `ci: workflow hygiene fixes` with a summary of changes

Tip: To just inspect CI runs without streaming checks, append `&& /ci-latest` to the one-liner above.

