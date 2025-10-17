# /restack

Break down a large PR into a stack of smaller, independent PRs that build on each other. This command orchestrates the entire restacking workflow from prep to completion.

## Overview

This command helps you transform a large, monolithic PR (branchA) into a series of smaller, reviewable PRs (branchC, branchD, branchE...) that:
- Are individually reviewable and testable
- Build on each other in a logical dependency chain
- Pass CI independently
- Can be merged sequentially

## Workflow Phases

### Phase 1: Preparation

Starting point: You're on `branchA` (your large PR)

1. **Create prep branch**: `${branchA}-restack-prep` (we'll call this `branchB`)
2. **Squash commits**: Combine all commits since `main` into one with a comprehensive message
3. **Rebase on latest**: Fetch `origin/main` and rebase `branchB` off it, resolving conflicts
4. **Review changes**: Identify logical groupings that can stand independently

### Phase 2: Stack Creation (Loop)

For each logical group of changes:

1. **Create independent branch**: `branchC` (with meaningful name) based on current `base`
   - First iteration: base = `main`
   - Subsequent iterations: base = previous PR branch
2. **Apply changes**: Cherry-pick or apply the specific changes for this PR
3. **Push and PR**: Create PR with `/pr`
4. **Test**: Run tests for the new changes
5. **CI validation**: Use `/pr-checks` to ensure CI passes
6. **Update prep branch**: Go back to `branchB`, rebase off `branchC` (removes those changes)
7. **Repeat**: Until all changes from `branchB` are in separate PRs

## Parameters

- **branch_name** (string, optional): The branch to restack (default: current branch)
- **base** (string, optional): The base branch to restack onto (default: `main`)
- **remote** (string, optional): Remote name (default: `origin`)

## Example Usage

```bash
# Start restacking the current branch
/restack

# Restack a specific branch
/restack "feature/large-refactor" main origin
```

## Detailed Implementation

### Phase 1: Preparation Commands

```bash
#!/bin/bash
set -euo pipefail

BRANCH_A="${1:-$(git rev-parse --abbrev-ref HEAD)}"
BASE="${2:-main}"
REMOTE="${3:-origin}"
BRANCH_B="${BRANCH_A}-restack-prep"

echo "=== Phase 1: Preparation ==="
echo "Large PR branch: ${BRANCH_A}"
echo "Prep branch: ${BRANCH_B}"
echo "Base: ${REMOTE}/${BASE}"

# Ensure we're on branchA
git checkout "${BRANCH_A}"

# Step 1: Create prep branch (branchB)
echo "Creating prep branch: ${BRANCH_B}..."
git checkout -b "${BRANCH_B}"

# Step 2: Squash all commits since main
echo "Squashing commits since ${BASE}..."
COMMIT_COUNT=$(git rev-list --count "${REMOTE}/${BASE}..HEAD")
echo "Commits to squash: ${COMMIT_COUNT}"

if [ "${COMMIT_COUNT}" -gt 1 ]; then
    git reset --soft "${REMOTE}/${BASE}"
    
    # Generate conventional commit message
    echo "# Creating squashed commit message..."
    echo "# Summarize the major achievements and changes"
    echo "# Format: <type>(<scope>): <summary>"
    echo "#"
    echo "# Body: List major accomplishments (keep to ~1 page)"
    
    # Let AI or user provide comprehensive commit message
    git commit
else
    echo "Only one commit since ${BASE}, no squashing needed"
fi

# Step 3: Fetch and rebase on latest main
echo "Fetching latest ${REMOTE}/${BASE}..."
git fetch "${REMOTE}" "${BASE}"

echo "Rebasing on ${REMOTE}/${BASE}..."
git rebase "${REMOTE}/${BASE}"

echo "✓ Prep complete! Branch ${BRANCH_B} ready for restacking"
echo ""
echo "=== Phase 2: Review Changes ==="
echo "Review the changes and identify logical groupings:"
git log --oneline "${REMOTE}/${BASE}..HEAD"
echo ""
echo "Files changed:"
git diff --name-status "${REMOTE}/${BASE}..HEAD"
```

### Phase 2: Stack Creation Loop

The AI will guide you through creating each PR in the stack:

```bash
#!/bin/bash
set -euo pipefail

BRANCH_B="$1"
ITERATION="$2"
BASE_BRANCH="$3"  # main for first iteration, then previous PR branch
REMOTE="${4:-origin}"

echo "=== Stack Creation - Iteration ${ITERATION} ==="
echo "Current base: ${BASE_BRANCH}"
echo "Prep branch: ${BRANCH_B}"

# Prompt user/AI to define this PR's scope
echo "What logical group should this PR contain?"
echo "Suggestions:"
echo "  - Documentation updates"
echo "  - Test infrastructure"
echo "  - Core feature (minimal)"
echo "  - Bug fixes"
echo "  - Performance optimizations"
read -p "Enter PR scope: " PR_SCOPE

# Generate branch name
read -p "Enter branch name (e.g., 'sync/feat-core-structure'): " BRANCH_C

# Step 1: Create independent branch from current base
echo "Creating branch ${BRANCH_C} from ${REMOTE}/${BASE_BRANCH}..."
git fetch "${REMOTE}" "${BASE_BRANCH}"
git checkout -b "${BRANCH_C}" "${REMOTE}/${BASE_BRANCH}"

# Step 2: Apply specific changes
echo "Apply changes for: ${PR_SCOPE}"
echo ""
echo "Options:"
echo "  a) Cherry-pick specific commits from ${BRANCH_B}"
echo "  b) Manually apply file changes"
echo "  c) Interactive rebase"
read -p "Choose method (a/b/c): " METHOD

case "${METHOD}" in
    a)
        echo "Available commits from ${BRANCH_B}:"
        git log --oneline "${REMOTE}/${BASE_BRANCH}..${BRANCH_B}"
        read -p "Enter commit range or SHAs: " COMMITS
        git cherry-pick ${COMMITS}
        ;;
    b)
        echo "Checkout specific files from ${BRANCH_B}..."
        git checkout "${BRANCH_B}" -- <files>
        git commit -m "<conventional commit message>"
        ;;
    c)
        git rebase -i "${REMOTE}/${BASE_BRANCH}"
        ;;
esac

# Step 3: Push and create PR
echo "Pushing ${BRANCH_C}..."
git push -u "${REMOTE}" "${BRANCH_C}"

echo "Creating PR..."
# AI should invoke: /pr

# Step 4: Test the changes
echo "Running tests for new changes..."
cargo test

# Step 5: CI validation
echo "Validating CI..."
# AI should invoke: /pr-checks
# This will automatically fix any CI issues and loop until green

# Step 6: Update prep branch
echo "Updating ${BRANCH_B} to remove merged changes..."
git checkout "${BRANCH_B}"

# Rebase branchB off branchC (this removes the changes we just PR'd)
git rebase "${BRANCH_C}"

echo "✓ Iteration ${ITERATION} complete!"
echo "  PR created: ${BRANCH_C}"
echo "  Base updated: ${BRANCH_B} rebased on ${BRANCH_C}"
echo ""

# Check if more work remains
REMAINING_COMMITS=$(git rev-list --count "${REMOTE}/${BASE}..HEAD")
echo "Remaining commits in ${BRANCH_B}: ${REMAINING_COMMITS}"

if [ "${REMAINING_COMMITS}" -gt 0 ]; then
    echo "More changes to restack. Continue? (y/n)"
    read -p "> " CONTINUE
    if [ "${CONTINUE}" = "y" ]; then
        # Next iteration uses branchC as the new base
        return "${BRANCH_C}"  # Next base
    fi
fi

echo "✓ Restacking complete! All changes are in separate PRs."
```

## AI Agent Behavior

When you invoke `/restack`, the AI agent should:

### Phase 1: Automated Prep
1. Execute all prep steps automatically
2. Generate a comprehensive squashed commit message
3. Handle rebase conflicts interactively if needed
4. Present a summary of changes for review

### Phase 2: Interactive Stack Creation
For each iteration:

1. **Analyze remaining changes** in branchB
2. **Suggest logical groupings**:
   ```
   Suggested PR stack:
   1. docs/fix-readme - Documentation updates (5 files)
   2. test/add-integration - Test infrastructure (8 files) 
   3. sync/feat-core - Core sync feature (12 files)
   4. sync/perf-optimize - Performance improvements (6 files)
   ```
3. **Ask for confirmation** or let user modify the plan
4. **For each PR**:
   - Create branch with appropriate name
   - Apply relevant changes (guide user on best method)
   - Run tests locally
   - Push and invoke `/pr` with good description
   - Invoke `/pr-checks` (which handles CI loop automatically)
   - Update branchB
5. **Track progress**:
   ```
   Restack Progress: 2/4 complete
   ✅ docs/fix-readme - PR #123 (merged)
   ✅ test/add-integration - PR #124 (CI passing, in review)
   🔄 sync/feat-core - Creating now...
   ⏳ sync/perf-optimize - Pending
   ```
6. **Handle blockers**:
   - If CI fails, `/pr-checks` handles it automatically
   - If tests fail locally, stop and debug
   - If conflicts arise, guide resolution

## Example Workflow

```bash
# User on branch: feature/massive-refactor (100 commits, 200 files)
/restack

# AI responds:
# === Phase 1: Preparation ===
# Creating prep branch: feature/massive-refactor-restack-prep
# Squashing 100 commits...
# 
# Generating commit message. Summary of changes:
# - Refactored sync engine for better performance
# - Added adaptive concurrency control
# - Improved error handling and recovery
# - Enhanced progress reporting
# - Updated documentation
# - Added comprehensive tests
#
# [AI generates proper conventional commit]
# Rebasing on origin/main...
# ✓ Prep complete!
#
# === Phase 2: Identifying Groupings ===
# Analyzing changes...
# 
# Suggested PR stack (5 PRs):
# 1. docs/update-architecture (8 files, docs only)
# 2. test/add-framework (12 files, test infrastructure)
# 3. sync/refactor-core (35 files, core refactor - depends on test)
# 4. sync/feat-adaptive (45 files, new feature - depends on core)
# 5. sync/perf-optimize (15 files, optimizations - depends on feature)
#
# Proceed with this plan? (y/n/modify)

y

# === Stack Creation - Iteration 1/5 ===
# Creating: docs/update-architecture
# Base: origin/main
#
# Files to include:
#   README.md
#   ARCHITECTURE.md
#   docs/DESIGN.md
#   ...
#
# [AI creates branch, applies changes, commits]
# Pushing to origin...
# Creating PR #125: "docs: update architecture documentation"
#
# [AI invokes /pr-checks]
# Watching CI...
# ✅ All checks passing!
#
# Rebasing prep branch...
# ✓ Iteration 1 complete!
#
# === Stack Creation - Iteration 2/5 ===
# Creating: test/add-framework
# Base: docs/update-architecture
# ...
# [continues until all 5 PRs are created]
#
# ✓ Restacking complete!
# 
# Summary:
# ✅ 5 PRs created:
#   - PR #125: docs/update-architecture (merged)
#   - PR #126: test/add-framework (approved, pending merge)
#   - PR #127: sync/refactor-core (in review)
#   - PR #128: sync/feat-adaptive (in review)
#   - PR #129: sync/perf-optimize (in review)
#
# Merge order: 125 → 126 → 127 → 128 → 129
# Original branch: feature/massive-refactor (preserved)
# Prep branch: feature/massive-refactor-restack-prep (can delete)
```

## Best Practices

### Grouping Strategy
1. **Documentation first** - Easiest to review, sets context
2. **Tests second** - Shows what behavior is expected
3. **Core changes** - Minimal working version
4. **Features** - Build on core incrementally
5. **Optimizations last** - After functionality is established

### Branch Naming
Use the standard convention:
- `docs/update-readme`
- `test/add-integration-suite`
- `sync/refactor-engine`
- `sync/feat-adaptive-concurrency`
- `perf/optimize-io-paths`

### Commit Messages
Each PR should have clean conventional commits:
```
docs: update architecture documentation

- Add io_uring integration overview
- Document adaptive concurrency design
- Update README with new features
```

### Dependencies
Make dependencies explicit in PR descriptions:
```markdown
## Dependencies
- Depends on #125 (docs)
- Depends on #126 (test framework)

## Changes
- Core sync engine refactor
- Improved error handling
- Better state management
```

### Testing
Each PR should:
- Include relevant tests
- Pass CI independently
- Be testable in isolation

## Common Scenarios

### Scenario 1: Simple Linear Stack
```
main → docs → tests → feature → optimization
```
Each PR builds on the previous one linearly.

### Scenario 2: Parallel Branches
```
main → shared-infrastructure
     ├→ feature-A (depends on infrastructure)
     └→ feature-B (depends on infrastructure)
```
Create infrastructure PR first, then parallel feature PRs.

### Scenario 3: Refactor + Feature
```
main → refactor-core → feature-using-new-core
```
Refactor first (easier review), then feature using new structure.

## Troubleshooting

### "Too many conflicts during rebase"
- Make smaller groupings
- Consider squashing less aggressively
- Create more intermediate PRs

### "CI fails on small PR but passed on large branch"
- Missing dependencies between changes
- Regroup to include necessary context
- Add more tests to the earlier PR

### "Reviewer confused about PR dependencies"
- Add clear dependency notes in PR description
- Link related PRs
- Consider adding a tracking issue

### "Later PRs keep getting conflicts"
- Rebase prep branch regularly
- Merge approved PRs promptly
- Consider restacking remaining PRs if main moved significantly

## Notes

- **Keep original branch**: `branchA` is preserved unchanged
- **Prep branch is temporary**: Can delete after all PRs merged
- **CI runs on each**: Every small PR validates independently
- **Review parallelization**: Multiple PRs can be reviewed concurrently
- **Easier rollback**: Can revert individual PRs if needed
- **Better history**: Clear progression of changes
- **Faster reviews**: Reviewers can approve incrementally

## Related Commands

- `/branch` - Create individual branches in the stack
- `/pr` - Create PRs for each piece
- `/pr-checks` - Validate CI (automatic fix loop)
- `/commit` - Conventional commits for clean history

## When to Use

Use `/restack` when:
- PR has >20 commits or >50 files changed
- Review is stalled due to size
- Multiple concerns mixed together
- CI failures are hard to diagnose
- You want to merge parts while working on others

## When NOT to Use

Don't restack if:
- PR is already focused and small
- Changes are tightly coupled (can't separate)
- Already in final review stages
- Time pressure (restacking takes effort)

