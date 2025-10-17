# Custom Cursor Commands

This directory contains custom slash commands for the io_uring_sync4 project.

## Available Commands

### `/branch` - Smart Branch Creation

Create a new local branch directly from a remote base branch without checking out the base locally.

**Quick Start:**
```bash
/branch "feature/my-new-feature"
```

**Full Syntax:**
```bash
/branch <name> [base] [remote] [push]
```

**Examples:**
```bash
# Simple feature branch from main
/branch "sync/feat-adaptive-io"

# Branch from a release branch
/branch "hotfix/critical-bug" "release/v1.0"

# Create without pushing
/branch "experimental/test-idea" main origin false

# Full specification
/branch "metadata/fix-xattr-bug" main origin true
```

**See:** [branch.md](./branch.md) for full documentation

### `/restack` - Break Down Large PRs

Transform a large, monolithic PR into a stack of smaller, reviewable PRs that build on each other.

**Quick Start:**
```bash
# Start restacking the current branch
/restack
```

**Full Syntax:**
```bash
/restack [branch_name] [base] [remote]
```

**Workflow:**
1. **Prep Phase**: Creates a prep branch, squashes commits, rebases on latest main
2. **Stack Phase**: Iteratively creates small, focused PRs from the large change
3. **Each iteration**: Create branch → Apply changes → Test → CI → Rebase prep branch
4. **Result**: Multiple small PRs that build on each other

**Example:**
```bash
# Restack current large PR
/restack

# AI guides you through:
# - Creates feature/large-refactor-restack-prep
# - Suggests: 5 logical PRs (docs, tests, core, feature, optimization)
# - Creates docs/update-readme (PR #125)
# - Creates test/add-framework (PR #126, depends on #125)
# - Creates sync/refactor-core (PR #127, depends on #126)
# - etc.
```

**When to use:**
- PR has >20 commits or >50 files changed
- Review is stalled due to size
- Multiple concerns mixed together
- You want to merge parts while working on others

**See:** [restack.md](./restack.md) for full documentation

## Naming Conventions

Our project uses the following branch naming pattern:

```
<area>/<verb>-<noun>
```

**Areas:**
- `sync/` - Core sync functionality
- `copy/` - File copy operations
- `metadata/` - Metadata handling
- `io/` - I/O operations and io_uring
- `bench/` - Benchmarking and performance
- `ci/` - CI/CD improvements
- `docs/` - Documentation updates
- `test/` - Testing improvements
- `fix/` - Bug fixes (when not area-specific)
- `feat/` - New features (when not area-specific)

**Verbs:**
- `feat` - New feature
- `fix` - Bug fix
- `refactor` - Code refactoring
- `perf` - Performance improvement
- `test` - Test additions/improvements
- `docs` - Documentation
- `chore` - Maintenance tasks

**Examples:**
- `sync/feat-progress-reporting`
- `metadata/fix-xattr-permission`
- `io/perf-adaptive-concurrency`
- `bench/feat-power-monitoring`
- `ci/refactor-workflow-stages`

## Workflow

### Standard Single-Feature Workflow

1. **Create a branch:** `/branch "area/verb-description"`
2. **Make changes:** Work on your feature
3. **Open PR:** Use `/pr` or `/pr-ready` when ready
4. **Monitor CI:** Use `/pr-checks` to watch and auto-fix CI issues

### Large PR Restacking Workflow

When you have a large PR that needs to be broken down:

1. **Start restacking:** `/restack` (on your large PR branch)
2. **AI creates prep branch:** Squashes and rebases changes
3. **AI suggests groupings:** Reviews changes and proposes logical PRs
4. **Iterative PR creation:**
   - Creates small branch from current base (starts with `main`)
   - Applies specific changes for that concern
   - Runs tests
   - Pushes and creates PR
   - Automatically runs `/pr-checks` (fixes CI issues)
   - Rebases prep branch to remove merged changes
   - Next PR uses previous PR as base
5. **Result:** Stack of dependent PRs, each reviewable independently

**Example flow:**
```
Large PR: feature/massive-refactor (100 commits, 200 files)

After /restack:
├─ main
│  └─ docs/update-architecture (PR #125) ✅ merged
│     └─ test/add-framework (PR #126) ✅ CI passing
│        └─ sync/refactor-core (PR #127) 🔄 in review
│           └─ sync/feat-adaptive (PR #128) ⏳ pending
│              └─ perf/optimize-io (PR #129) ⏳ pending
```

## Tips

### General
- Keep branches single-purpose and focused
- Branch directly from remote to avoid local `main` pollution
- Use descriptive names that explain the change
- Push immediately (default) to enable early CI feedback
- For hotfixes, specify the release branch as base

### For Restacking
- **Start early**: Restack before PR gets too stale
- **Group logically**: Docs → Tests → Core → Features → Optimizations
- **Make PRs standalone**: Each should pass CI independently
- **Document dependencies**: Link related PRs in descriptions
- **Merge promptly**: Merge earlier PRs to unblock later ones
- **Keep prep branch**: Don't delete until all PRs are merged
- **Original branch preserved**: Your large PR branch stays untouched

## Adding New Commands

To add a new custom command:

1. Create a new `.md` file in this directory with your command name
2. Follow the structure of existing commands
3. Document parameters, behavior, and examples
4. The command will be available as `/command-name`

## Notes

- Commands are project-specific and won't affect other workspaces
- Cursor will automatically discover commands in this directory
- Update this README when adding new commands
