# Cursor Commands

This directory contains custom slash commands for Cursor AI to streamline common development workflows for the io_uring_sync project.

## Available Commands

### Git & GitHub Workflow

- **`/branch`** - Create a new branch from remote without checking out base locally
  - Example: `/branch "sync/feat-adaptive-io" main origin true`
  
- **`/commit`** - Create a Conventional Commit with pre-commit checks
  - Example: `/commit "feat(sync): add adaptive concurrency control"`
  
- **`/pr`** - Create or update a GitHub Pull Request with structured template
  - Example: `/pr "feat(sync): adaptive concurrency" "Description..." main false`
  
- **`/pr-ready`** - Push branch and ensure PR exists, show CI status
  - Example: `/pr-ready "feat(sync): adaptive concurrency"`
  
- **`/pr-checks`** - Watch PR CI checks with live updates
  - Example: `/pr-checks`
  
- **`/ci-latest`** - Show latest GitHub Actions runs for current branch
  - Example: `/ci-latest`
  
- **`/review`** - Summarize diff and highlight risks, test gaps, CI considerations
  - Example: `/review`

### Planning & Design

- **`/design`** - Create comprehensive design document from context or conversation
  - Example: `/design` (auto-infer) or `/design "feature-name"`

- **`/plan`** - Create detailed phase-based implementation plan from context
  - Example: `/plan` or `/plan @docs/projects/feature/design.md`

- **`/implement`** - Execute implementation plan step-by-step, tracking progress
  - Example: `/implement` or `/implement @docs/projects/feature/plan.md`

- **`/debug`** - Systematic debugging with disciplined iterative approach
  - Example: `/debug` or `/debug @src/module.rs "issue description"`

### Build & Test

- **`/build`** - Build project with specified profile and features
  - Example: `/build "release" "all" false`
  
- **`/test`** - Run tests with common patterns
  - Example: `/test "all"` or `/test "copy"` or `/test "all" "compio-fs-extended"`
  
- **`/bench`** - Run benchmark suites
  - Example: `/bench true false` (quick) or `/bench false true` (full)

- **`/smoke`** - Run quick smoke tests for basic functionality
  - Example: `/smoke`

### Code Quality

- **`/fmt`** - Format code with rustfmt
  - Example: `/fmt false true` (format) or `/fmt true true` (check only)
  
- **`/clippy`** - Run Clippy linter
  - Example: `/clippy false false` (check) or `/clippy true false` (auto-fix)
  
- **`/clean`** - Clean build artifacts and test data
  - Example: `/clean false true` (target only) or `/clean true false` (full)
  
- **`/docs`** - Build and open documentation
  - Example: `/docs true false` (public) or `/docs true true` (include private)

### CI/CD

- **`/workflow-audit`** - Audit GitHub Actions workflows for best practices
  - Example: `/workflow-audit`

- **`/release-check`** - Pre-release checklist and verification
  - Example: `/release-check`

## Command Conventions

### Conventional Commits

All commits should follow the [Conventional Commits](https://www.conventionalcommits.org/) format:

```
<type>(<scope>): <description>

[optional body]

[optional footer(s)]
```

**Types:**
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation only
- `style`: Code style (formatting, etc.)
- `refactor`: Code refactoring
- `perf`: Performance improvement
- `test`: Adding or updating tests
- `build`: Build system changes
- `ci`: CI configuration changes
- `chore`: Other changes

**Common Scopes for this project:**
- `sync`: Synchronization logic
- `copy`: File copying operations
- `metadata`: Metadata preservation
- `io_uring`: io_uring operations
- `cli`: Command-line interface
- `progress`: Progress reporting
- `bench`: Benchmarking
- `test`: Test infrastructure

### Branch Naming

Format: `<area>/<verb-noun>`

Examples:
- `sync/feat-adaptive-concurrency`
- `metadata/fix-xattr-preservation`
- `copy/perf-zero-copy`
- `docs/update-benchmarks`

## Pre-Commit Checklist

Before committing or opening a PR:

1. **Format**: `/fmt false true` or `cargo fmt --all`
2. **Lint**: `/clippy false false` or `cargo clippy --all-targets --all-features -- -D warnings`
3. **Test**: `/test "all"` or `cargo test`
4. **Smoke test**: `/smoke` or `./benchmarks/smoke_test.sh`
5. **Build**: `/build "release" "all" false` or `cargo build --all-features`

## PR Best Practices

1. **Single concern** - One PR per feature/fix
2. **Tests required** - Add unit and integration tests
3. **Documentation** - Update docs and comments
4. **Benchmarks** - Run benchmarks for performance-sensitive changes
5. **Conventional titles** - Use conventional commit format
6. **CI passes** - Ensure all checks pass before requesting review

## Quick Workflows

### Design and implement a new feature:
```bash
# 1. Create design document from conversation
/design "new-feature"
# Creates: docs/projects/new-feature/design.md

# 2. Create implementation plan (auto-finds design in project folder)
/plan
# Creates: docs/projects/new-feature/plan.md

# 3. Create feature branch
/branch "sync/feat-new-feature" main origin true

# 4. Execute the plan step-by-step
/implement @docs/projects/new-feature/plan.md
# Works through checkboxes, runs quality checks
# Add notes if issues arise, commits at checkpoints

# 5. Continue implementing (run multiple times)
/implement
# Resumes from last checkpoint, continues

# 6. When complete, create PR
/commit "feat(sync): add new feature"
/pr-ready "feat(sync): add new feature"
/pr-checks
```

### Fix a bug:
```bash
/branch "copy/fix-bug-name" main origin true
# Fix the bug...
/fmt false true
/test "copy"
/smoke
/commit "fix(copy): resolve bug description"
/pr-ready "fix(copy): resolve bug description"
```

### Run benchmarks:
```bash
/build "release" "all" false
/bench true false  # Quick benchmark
# Review results
/commit "perf(sync): optimize operation X"
```

### Prepare for release:
```bash
/release-check
# Fix any issues found
/docs true false
# Review and tag
git tag -a v1.0.0 -m "Release v1.0.0"
```

## Integration

These commands were adapted from the [EconGraph project](https://github.com/EconGraph/econ-graph/tree/main/.cursor/commands) and customized for Rust development workflows specific to io_uring_sync.

Commands removed from the original:
- All Kubernetes commands (k8s-*)
- Database migration commands
- Docker-specific commands
- TypeScript/frontend commands
- Backend debugging commands specific to their architecture

Commands added:
- `/test` - Rust testing patterns
- `/bench` - Benchmark execution
- `/build` - Cargo build configurations
- `/smoke` - Quick smoke tests
- `/fmt` - Code formatting
- `/clippy` - Linting
- `/clean` - Clean artifacts
- `/docs` - Documentation building
- `/release-check` - Release verification
- `/design` - Generate design documents from conversation
- `/plan` - Generate structured implementation plans
- `/implement` - Execute implementation plans step-by-step
- `/debug` - Systematic debugging with iterative approach

