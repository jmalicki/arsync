# Design: GitHub Actions CI Trigger Semantics for Stacked PRs

**Status**: Draft  
**Author**: Jake Malicki (inferred from git)  
**Created**: 2025-10-17  
**Last Updated**: 2025-10-17  
**Branch**: `design/ci-trigger-semantics`  
**Implementation Branch**: `fix/ci-trigger-semantics`

## Overview

This design addresses confusion around GitHub Actions CI trigger semantics, specifically how `push` and `pull_request` events interact in a stacked PR workflow. The goal is to configure CI triggers such that:

1. Any push directly to `main` triggers CI (including merged PRs)
2. Any pull request triggers CI, regardless of the base branch
3. No duplicate CI runs occur for the same commit on the same branch

This document explains how GitHub Actions triggers work, why duplicates occur, and provides the correct configuration.

## Problem Statement

### Current Situation

The current CI configuration in `.github/workflows/ci.yml` has:

```yaml
on:
  push:
    branches: ['**']  # Run on all branches
  pull_request:
    branches: [ main, develop ]
```

This configuration has several issues in a stacked PR workflow:

1. **Duplicate runs**: When a PR exists, both `push` and `pull_request` events fire for the same commit
2. **Limited PR triggers**: Only PRs targeting `main` or `develop` get CI
3. **Stacked PR gaps**: Feature branches targeting other feature branches don't get CI

### Challenges

- **Understanding event semantics**: The difference between `push` and `pull_request` events is subtle
- **Stacked PRs**: When PR B targets PR A's branch, we need CI on both
- **Merge commits**: When PRs merge to `main`, we need CI on the merge commit
- **Resource waste**: Duplicate CI runs consume GitHub Actions minutes and slow feedback

### Goals

1. **Clear trigger semantics**: Easy to understand when and why CI runs
2. **Complete coverage**: CI runs for all relevant changes
3. **No duplicates**: Each commit gets CI exactly once per branch
4. **Stacked PR support**: CI works correctly when PRs target non-main branches

### Non-Goals

- Optimizing CI performance (separate concern)
- Changing the CI job structure
- Modifying workflow concurrency settings

## Proposed Solution

### High-Level Approach

The solution is to understand and leverage the semantic difference between GitHub Actions events:

1. **`push` event**: Fires when commits are pushed directly to a branch in your repository
2. **`pull_request` event**: Fires when PR activity occurs (open, sync, etc.), testing the merge result

The key insight: **These events serve different purposes and should not overlap in configuration**.

### The Correct Configuration

```yaml
on:
  push:
    branches:
      - main  # Only run on pushes to main (merges, hotfixes)
  pull_request:
    # No branches specified = all PRs, regardless of base branch
  schedule:
    - cron: '0 0 * * 1'  # Weekly scheduled run
```

### How GitHub Actions Events Work

#### Event Flow Diagram

```
Developer Workflow                  GitHub Actions Events

git push origin feature-A
         │
         ▼
   feature-A branch updated
         │
         ├──────────────────────▶ push: feature-A (ONLY if configured)
         │
Create PR: feature-A → main
         │
         ├──────────────────────▶ pull_request: opened
         │
git push origin feature-A
         │
         ▼
   feature-A branch updated
         │
         ├──────────────────────▶ push: feature-A (ONLY if configured)
         ├──────────────────────▶ pull_request: synchronized
         │
Merge PR to main
         │
         ▼
   main branch updated
         │
         └──────────────────────▶ push: main
```

#### Event Details

**`push` Event:**
- Fires when: Commits are pushed to a branch
- Context: The exact commit that was pushed
- Runs on: The repository's copy of the branch
- Use for: Testing commits landing on protected/release branches

**`pull_request` Event:**
- Fires when: PR opened, synchronized, reopened, etc.
- Context: A merge commit between the PR branch and base branch
- Runs on: `refs/pull/:number/merge` (virtual merge commit)
- Use for: Testing what the code would look like after merging

#### Why Duplicates Happen

With the current config (`push: ['**']` + `pull_request: [main, develop]`):

```
Scenario: Push to branch with an open PR

git push origin feature-A (PR exists: feature-A → main)
         │
         ├──────▶ push: feature-A      ← Triggered by branches: ['**']
         └──────▶ pull_request: synced  ← Triggered by PR existence
                  
Result: TWO CI runs for the same change!
```

This happens because:
1. `push` matches `**` (all branches), so it triggers
2. The PR exists and targets `main`, so `pull_request` also triggers

#### Why The Proposed Solution Works

With `push: [main]` + `pull_request:` (no branch filter):

```
Scenario 1: Push to feature branch with PR

git push origin feature-A (PR exists: feature-A → main)
         │
         ├──────▶ push: feature-A      ← NOT triggered (only main configured)
         └──────▶ pull_request: synced  ← Triggered (no branch filter)
                  
Result: ONE CI run via pull_request

Scenario 2: Push to feature branch without PR

git push origin feature-A (no PR)
         │
         └──────▶ push: feature-A      ← NOT triggered (only main configured)
                  
Result: NO CI run (intentional - feature branch without PR)

Scenario 3: Merge PR to main

Merge button clicked
         │
         ▼
   Merge commit to main
         │
         └──────▶ push: main            ← Triggered (main configured)
                  
Result: ONE CI run on main after merge

Scenario 4: Stacked PR (feature-B → feature-A)

Create PR: feature-B → feature-A
         │
         └──────▶ pull_request: opened  ← Triggered (no branch filter!)
                  
Result: ONE CI run, testing merge into feature-A
```

### Key Components

#### Component 1: Workflow Trigger Configuration
- **Purpose**: Define when CI runs
- **Location**: `.github/workflows/ci.yml` (lines 3-9)
- **Key Elements**: `on.push.branches`, `on.pull_request`
- **Responsibilities**: Ensure CI runs exactly when needed, no more, no less

#### Component 2: Concurrency Control
- **Purpose**: Prevent wasteful parallel runs
- **Location**: `.github/workflows/ci.yml` (lines 11-13)
- **Key Elements**: `concurrency.group`, `cancel-in-progress`
- **Responsibilities**: Cancel outdated runs when new commits arrive

### Data Structures

The GitHub Actions context provides different data for each event:

```yaml
# push event context
github.ref: refs/heads/main
github.sha: abc123...
github.event_name: push
github.head_ref: ''

# pull_request event context
github.ref: refs/pull/42/merge  # virtual merge commit
github.sha: def456...            # merge commit SHA
github.event_name: pull_request
github.head_ref: feature-branch  # PR source branch
github.base_ref: main            # PR target branch
```

## API Design

### Workflow Trigger Specification

```yaml
name: CI

on:
  # Push events: Only for main branch
  # This catches:
  # - PR merges to main
  # - Direct pushes to main (hotfixes, admin pushes)
  # - Force pushes to main (if allowed)
  push:
    branches:
      - main
  
  # Pull request events: All PRs, any base branch
  # This catches:
  # - PRs targeting main
  # - PRs targeting feature branches (stacked PRs)
  # - PRs targeting release branches
  # - PRs targeting develop or any other branch
  pull_request:
    # Implicit: all branches
    # Explicit types (default if not specified):
    # types: [opened, synchronize, reopened]
  
  # Scheduled runs for periodic checks
  schedule:
    - cron: '0 0 * * 1'  # Monday at midnight UTC

# Prevent duplicate runs on the same ref
concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: true
```

### Event Type Breakdown

**For `pull_request`, these activity types trigger by default:**
- `opened`: New PR created
- `synchronize`: New commits pushed to PR branch
- `reopened`: Closed PR is reopened

**Other available types (must be explicitly specified):**
- `edited`: PR title or body changed (don't need CI for this)
- `closed`: PR closed (don't need CI for this)
- `assigned`, `labeled`, etc.: Metadata changes (don't need CI)

## Implementation Details

### File Changes

| File | Changes | Complexity |
|------|---------|------------|
| `.github/workflows/ci.yml` | Update `on:` section lines 3-9 | Low |
| `docs/projects/ci-trigger-semantics/design.md` | Create design doc | Low |

### Current Configuration

```yaml
on:
  push:
    branches: ['**']  # ← PROBLEM: Too broad
  pull_request:
    branches: [ main, develop ]  # ← PROBLEM: Too restrictive
```

### Proposed Configuration

```yaml
on:
  push:
    branches:
      - main  # Only main branch
  pull_request:
    # No branch filter = all PRs
  schedule:
    - cron: '0 0 * * 1'
```

### Complexity Assessment

**Overall Complexity**: Simple

**Breakdown**:
- **Scope**: One file, 7 lines changed
- **Dependencies**: None
- **Testing**: Manual verification with stacked PRs
- **Risk**: Low (easy to revert, well-documented GitHub feature)

**Estimated Phases**: 1 (single change)

## Testing Strategy

### Manual Testing

1. **Test Case 1: Push to main**
   - Action: Merge a PR or push directly to main
   - Expected: ONE CI run via `push` event
   - Verify: Check Actions tab shows `push` event

2. **Test Case 2: PR targeting main**
   - Action: Create PR from feature branch to main
   - Expected: ONE CI run via `pull_request` event
   - Verify: PR shows CI check, Actions shows `pull_request` event

3. **Test Case 3: Stacked PR (feature-B → feature-A)**
   - Action: Create PR from feature-B to feature-A
   - Expected: ONE CI run via `pull_request` event
   - Verify: PR shows CI check, confirms merge into feature-A works

4. **Test Case 4: Push to PR branch**
   - Action: Push new commit to branch with open PR
   - Expected: ONE CI run via `pull_request` (synchronized)
   - Verify: No duplicate `push` event run

5. **Test Case 5: Feature branch without PR**
   - Action: Push to feature branch without PR
   - Expected: NO CI run
   - Verify: Actions tab shows no new runs

### Integration Verification

After deployment, monitor:
- No duplicate runs for the same commit
- All PRs get CI regardless of base branch
- Main branch always gets CI on push
- GitHub Actions usage doesn't spike

## Performance Considerations

### Expected Impact

- **GitHub Actions Minutes**: Reduced by ~50% (eliminate duplicate runs)
- **Feedback Speed**: Faster (no wasted CI resources)
- **Queue Time**: Potentially reduced (fewer total jobs)

### Benefits

1. **Resource efficiency**: Half the CI runs for PR branches
2. **Clearer feedback**: One CI result per change, not two
3. **Simpler debugging**: Only one run to investigate on failure

## Error Handling

### Potential Issues

**Issue**: No CI on feature branches without PRs
- **Impact**: Low (expected behavior)
- **Mitigation**: Document that CI requires a PR
- **Alternative**: Add manual workflow dispatch if needed

**Issue**: Main branch protection requires status checks
- **Impact**: None (main still gets CI via push)
- **Mitigation**: N/A

**Issue**: Stacked PR confusion**
- **Impact**: Medium (developers might not realize CI tests merge into base)
- **Mitigation**: Document in PR template that CI tests merge result

## Migration & Compatibility

### Breaking Changes

None. This is purely a trigger configuration change.

### Workflow Changes

**Before**: CI runs on every push to every branch  
**After**: CI runs on pushes to main, and on all pull requests

**Impact on developers:**
- Feature branches: Only get CI when PR exists (desirable)
- Main branch: Still gets CI on every push
- Stacked PRs: Now properly supported

### Communication

Update the developer documentation to explain:
1. Feature branches need a PR to get CI
2. Stacked PRs are fully supported
3. Main always gets CI on push

## Rollout Plan

1. **Phase 1**: Update `.github/workflows/ci.yml` trigger section
2. **Phase 2**: Commit and push to the feature branch
3. **Phase 3**: Create PR targeting main
4. **Phase 4**: Verify CI runs correctly on the PR
5. **Phase 5**: Merge and verify CI runs on main
6. **Phase 6**: Test with a stacked PR scenario

## Alternatives Considered

### Alternative 1: Keep `push: ['**']`, remove `pull_request`

```yaml
on:
  push:
    branches: ['**']
```

- **Pros**: 
  - Simple configuration
  - CI on every branch
- **Cons**: 
  - No testing of merge commits
  - Doesn't test PR merge result
  - CI runs even without PRs
- **Why not chosen**: `pull_request` tests the merge result, which is what actually matters

### Alternative 2: Use `paths` filters to reduce runs

```yaml
on:
  push:
    branches: ['**']
    paths-ignore:
      - 'docs/**'
      - '**.md'
```

- **Pros**: Fewer CI runs for doc changes
- **Cons**: Doesn't solve the duplicate run problem
- **Why not chosen**: Orthogonal concern, can be added separately

### Alternative 3: Use workflow_run to chain workflows

```yaml
on:
  workflow_run:
    workflows: ["CI"]
    types: [completed]
```

- **Pros**: Complex dependency management
- **Cons**: Much more complex, harder to debug
- **Why not chosen**: Overkill for this problem

### Alternative 4: Use `pull_request_target` instead of `pull_request`

- **Pros**: Runs in the context of the base branch
- **Cons**: Security risk (runs PR code with write permissions)
- **Why not chosen**: Security issue, not needed for this use case

## Open Questions

- [x] Should scheduled runs continue on main only?
  - **Answer**: No, keep as-is. Schedule runs on the default branch (main).

- [ ] Should we add `workflow_dispatch` for manual CI runs?
  - **Consider**: Useful for testing specific branches without creating PRs
  - **Decision**: Can be added later if needed

- [ ] Should documentation builds continue to deploy only from main?
  - **Answer**: Yes (already handled in the docs job with: `if: github.ref == 'refs/heads/main' && github.event_name == 'push'`)

## References

- [GitHub Actions: Events that trigger workflows](https://docs.github.com/en/actions/using-workflows/events-that-trigger-workflows)
- [GitHub Actions: `push` event](https://docs.github.com/en/actions/using-workflows/events-that-trigger-workflows#push)
- [GitHub Actions: `pull_request` event](https://docs.github.com/en/actions/using-workflows/events-that-trigger-workflows#pull_request)
- [GitHub Actions: Workflow syntax](https://docs.github.com/en/actions/using-workflows/workflow-syntax-for-github-actions)
- [Avoiding duplicate workflows](https://docs.github.com/en/actions/using-workflows/events-that-trigger-workflows#avoiding-duplicate-workflows)

## Acceptance Criteria

- [x] Design document created and reviewed
- [ ] `.github/workflows/ci.yml` updated with new trigger configuration
- [ ] CI runs exactly once on PR branches (via `pull_request` event)
- [ ] CI runs on pushes to main (via `push` event)
- [ ] Stacked PRs (e.g., feature-B → feature-A) trigger CI
- [ ] No duplicate CI runs observed
- [ ] Developer documentation updated
- [ ] Manual testing completed for all scenarios

## Future Work

- Add `workflow_dispatch` for manual CI triggers
- Consider `paths` filters to skip CI for doc-only changes
- Monitor GitHub Actions usage to quantify cost savings
- Document best practices for stacked PR workflows

---

## Deep Dive: Why `pull_request` Tests the Merge Commit

One of the most important and least understood aspects of `pull_request` events:

### The Virtual Merge Commit

When a `pull_request` event triggers:

1. GitHub creates a **virtual merge commit** between:
   - The HEAD of the PR branch (what you pushed)
   - The HEAD of the base branch (what you're merging into)

2. This virtual commit is at `refs/pull/:number/merge`

3. CI runs against **this merge commit**, not your branch

### Why This Matters

```
Scenario: PR from feature-A → main

feature-A:  A---B---C
                      \
main:        X---Y---Z---M (virtual merge commit)

pull_request event runs CI on commit M, which includes:
- All changes from A, B, C
- Merged with current state of main (X, Y, Z)
- Resolves any merge conflicts
```

This is **exactly what you want to test**: "Will my PR break things when it merges?"

### What About Merge Conflicts?

If your PR has merge conflicts:
- The virtual merge commit cannot be created
- The `pull_request` event still fires
- But the test will fail early with a merge conflict error
- This is good! It prevents merging broken code.

### Contrast with `push` Event

```
push event to feature-A tests:
- Commit C in isolation
- Against whatever main was when you branched
- May be outdated if main has moved forward
- Doesn't test the merge result
```

---

## Quick Reference: Event Decision Tree

```
Question: Should I use push or pull_request?

Is this a protected branch (main, release)?
├─ YES → Use push
│         (Test what actually lands on the branch)
│
└─ NO → Is there a PR?
         ├─ YES → Use pull_request
         │         (Test the merge result)
         │
         └─ NO → No CI needed
                  (Wait for PR to be created)
```

---

## Appendix: Full Example Workflow

### Complete `.github/workflows/ci.yml` Configuration

```yaml
name: CI (Optimized)

on:
  # Push events: Only main branch
  push:
    branches:
      - main
  
  # Pull request events: All PRs, any base
  pull_request:
    # types: [opened, synchronize, reopened]  # Default, can omit
  
  # Scheduled runs
  schedule:
    - cron: '0 0 * * 1'  # Mondays at midnight UTC

# Prevent duplicate runs
concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: true

env:
  CARGO_TERM_COLOR: always
  RUST_BACKTRACE: 1

permissions:
  contents: read
  checks: write
  actions: read

jobs:
  # ... existing jobs ...
```

### Real-World Scenario Examples

**Scenario: Simple Feature PR**

```bash
# 1. Create feature branch and PR
git switch -c feature/new-thing --track origin/main
git push -u origin feature/new-thing
gh pr create --base main

# Result: pull_request event fires → CI runs
# No push event (main not involved yet)
```

**Scenario: Stacked PRs**

```bash
# 1. Create feature-A (base changes)
git switch -c feature/a --track origin/main
# ... make changes ...
git push -u origin feature/a
gh pr create --base main  # PR #100

# Result: pull_request event fires → CI tests merge into main

# 2. Create feature-B (builds on feature-A)
git switch -c feature/b --track origin/main
git merge feature/a
# ... make additional changes ...
git push -u origin feature/b
gh pr create --base feature/a  # PR #101

# Result: pull_request event fires → CI tests merge into feature/a!
# This is the key: base branch doesn't matter, PR still gets CI
```

**Scenario: PR Merge to Main**

```bash
# 1. PR is approved and merged
gh pr merge 100

# Result: push event fires on main → CI runs
# Verifies the merge commit on main branch
```

---

**Next Steps**:

1. Review this design document
2. Validate understanding of GitHub Actions event semantics
3. Create implementation plan: `/plan`
4. Implement the trigger changes
5. Test with real PRs (simple and stacked)
6. Update developer documentation

