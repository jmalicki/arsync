# Complete Workflow Guide: From Branch to Merged PR

This guide shows how to use `/branch`, `/restack`, and `/pr-checks` together for efficient PR management.

## Scenario 1: Simple Feature Branch

**Use case:** Adding a focused new feature

```bash
# 1. Create branch directly from remote main
/branch "sync/feat-progress-reporting"

# 2. Make your changes
# ... edit files ...

# 3. Commit with conventional commits
git add .
git commit -m "feat(sync): add real-time progress reporting

- Display transfer rate and ETA
- Update progress bar every 100ms
- Add tests for progress calculations"

# 4. Push (already set up by /branch)
git push

# 5. Create PR
/pr

# 6. Watch and auto-fix CI
/pr-checks
# AI automatically:
# - Watches CI status
# - Detects failures immediately
# - Fixes issues (format, lint, tests)
# - Pushes fixes
# - Re-checks until all green

# Result: ✅ PR ready for review
```

**Timeline:** 10-30 minutes depending on complexity

---

## Scenario 2: Large PR Needs Breaking Down

**Use case:** You have a 100-commit, 200-file PR that's too large

### Step 1: Start Restacking

```bash
# On branch: feature/massive-sync-refactor
/restack

# AI: === Phase 1: Preparation ===
#     Creating feature/massive-sync-refactor-restack-prep
#     Squashing 100 commits into 1...
#     
#     Summary of changes (generated):
#     feat(sync): comprehensive sync engine refactor
#     
#     Major changes:
#     - Refactored sync engine architecture
#     - Added adaptive concurrency control  
#     - Improved error handling and recovery
#     - Enhanced progress reporting with ETA
#     - Updated documentation and examples
#     - Added comprehensive test suite
#     
#     Fetching origin/main...
#     Rebasing on origin/main...
#     ✓ Prep complete!
```

### Step 2: AI Suggests Groupings

```bash
# AI: === Phase 2: Identifying Logical Groups ===
#     
#     Analyzing 200 files, 15,000 lines changed...
#     
#     Suggested PR stack (5 PRs):
#     
#     1. docs/sync-architecture (8 files, ~500 lines)
#        - README.md updates
#        - Architecture documentation  
#        - API examples
#        Why first: Sets context for reviewers
#     
#     2. test/sync-framework (15 files, ~2000 lines)
#        - New test infrastructure
#        - Test utilities and helpers
#        - Mock implementations
#        Why second: Shows expected behavior
#     
#     3. sync/refactor-core (45 files, ~6000 lines)
#        - Core sync engine refactor
#        - New architecture implementation
#        - Basic error handling
#        Why third: Foundation for features
#     
#     4. sync/feat-adaptive-concurrency (28 files, ~4000 lines)
#        - Adaptive concurrency control
#        - Performance optimizations
#        - Enhanced progress tracking
#        Why fourth: Builds on refactored core
#     
#     5. sync/feat-advanced-recovery (15 files, ~2500 lines)
#        - Advanced error recovery
#        - Retry logic improvements  
#        - Edge case handling
#        Why fifth: Polish and robustness
#     
#     Proceed with this plan? (y/n/modify)
```

### Step 3: Iterative PR Creation

#### Iteration 1: Documentation

```bash
# You: y

# AI: === Stack Creation - Iteration 1/5 ===
#     Creating: docs/sync-architecture
#     Base: origin/main
#     
#     Files to include:
#       README.md
#       docs/ARCHITECTURE.md
#       docs/SYNC_ENGINE.md
#       docs/examples/...
#     
#     Creating branch from main...
#     
#     Using /branch internally:
#     /branch "docs/sync-architecture" main origin true

# AI: Applying documentation changes...
#     [selectively applies just doc changes]
#     
#     Committing:
#     docs: update sync engine architecture documentation
#     
#     - Document new sync engine design
#     - Add adaptive concurrency explanation
#     - Update README with new features
#     - Add usage examples
#     
#     Pushing to origin...

# AI: Creating PR #125...
#     /pr
#     
#     Title: docs: update sync engine architecture documentation
#     
#     Description:
#     ## Overview
#     Updates documentation for the refactored sync engine.
#     
#     ## Changes
#     - New architecture documentation
#     - Adaptive concurrency explanation
#     - Updated README and examples
#     
#     ## Dependencies
#     None (documentation only)
#     
#     ## Testing
#     N/A (docs only)

# AI: Running CI checks...
#     /pr-checks
#     
#     Watching CI... 
#     - rustfmt: ✅ PASS
#     - clippy: ✅ PASS
#     - tests: ✅ PASS (no code changes)
#     - docs: ✅ PASS
#     
#     ✅ All checks passing!

# AI: Rebasing prep branch...
#     git checkout feature/massive-sync-refactor-restack-prep
#     git rebase docs/sync-architecture
#     
#     ✓ Iteration 1 complete!
#     
#     Progress: 1/5 PRs created
#     ✅ PR #125: docs/sync-architecture (merged)
#     
#     Next base: docs/sync-architecture
```

#### Iteration 2: Test Infrastructure

```bash
# AI: === Stack Creation - Iteration 2/5 ===
#     Creating: test/sync-framework
#     Base: docs/sync-architecture
#     
#     Files to include:
#       tests/sync_framework/
#       tests/utils/mock_*.rs
#       tests/common/test_helpers.rs
#       ...
#     
#     Using /branch internally:
#     /branch "test/sync-framework" "docs/sync-architecture" origin true

# AI: Applying test infrastructure changes...
#     [selectively applies test framework code]
#     
#     Running tests locally...
#     cargo test --test sync_framework
#     ✅ All tests pass
#     
#     Pushing to origin...

# AI: Creating PR #126...
#     
#     Title: test: add comprehensive sync test framework
#     
#     Description:
#     ## Overview
#     Adds test infrastructure for the refactored sync engine.
#     
#     ## Changes
#     - New test framework with utilities
#     - Mock implementations for testing
#     - Helper functions for common test scenarios
#     - Integration test structure
#     
#     ## Dependencies
#     - Depends on #125 (documentation provides context)
#     
#     ## Testing
#     All new tests pass:
#     ```
#     cargo test --test sync_framework
#     ```

# AI: Running CI checks...
#     /pr-checks
#     
#     Watching CI...
#     - rustfmt: ✅ PASS
#     - clippy: ❌ FAIL (at 0:45)
#     
#     → IMMEDIATELY stop watching, debug clippy failure

# AI: Debugging clippy failure...
#     /debug "clippy warnings in test/sync-framework"
#     
#     Found: Unused import in tests/common/test_helpers.rs
#     Fixing: Removing unused import
#     
#     git add tests/common/test_helpers.rs
#     git commit -m "fix(test): remove unused import"
#     git push

# AI: Restarting CI checks...
#     /pr-checks
#     
#     Watching CI...
#     - rustfmt: ✅ PASS
#     - clippy: ✅ PASS
#     - test: ✅ PASS
#     - integration: ✅ PASS
#     
#     ✅ All checks passing!

# AI: Rebasing prep branch...
#     ✓ Iteration 2 complete!
#     
#     Progress: 2/5 PRs created
#     ✅ PR #125: docs/sync-architecture (merged)
#     ✅ PR #126: test/sync-framework (CI passing, in review)
#     
#     Next base: test/sync-framework
```

#### Iteration 3-5: Continue Pattern

```bash
# [Similar process for remaining PRs]

# AI: === Stack Creation - Iteration 3/5 ===
#     Creating: sync/refactor-core
#     Base: test/sync-framework
#     [... creates PR #127 ...]

# AI: === Stack Creation - Iteration 4/5 ===
#     Creating: sync/feat-adaptive-concurrency
#     Base: sync/refactor-core
#     [... creates PR #128 ...]

# AI: === Stack Creation - Iteration 5/5 ===
#     Creating: sync/feat-advanced-recovery
#     Base: sync/feat-adaptive-concurrency
#     [... creates PR #129 ...]
```

### Step 4: Final Summary

```bash
# AI: ✓ Restacking complete!
#     
#     === Summary ===
#     
#     Original PR: feature/massive-sync-refactor
#     - 100 commits, 200 files
#     - Status: Too large for review
#     
#     Restacked into 5 PRs:
#     
#     ├─ main
#     │  └─ PR #125: docs/sync-architecture ✅ merged (2 files, 500 lines)
#     │     └─ PR #126: test/sync-framework ✅ approved (15 files, 2000 lines)
#     │        └─ PR #127: sync/refactor-core 🔍 in review (45 files, 6000 lines)
#     │           └─ PR #128: sync/feat-adaptive ⏳ pending (28 files, 4000 lines)
#     │              └─ PR #129: sync/feat-recovery ⏳ pending (15 files, 2500 lines)
#     
#     Merge order: #125 → #126 → #127 → #128 → #129
#     
#     Benefits achieved:
#     ✅ Each PR is reviewable (< 50 files)
#     ✅ Logical progression (docs → tests → core → features)
#     ✅ Independent CI validation
#     ✅ Parallel review possible
#     ✅ Can merge incrementally
#     
#     CI fix iterations:
#     - PR #125: 0 iterations (clean)
#     - PR #126: 1 iteration (clippy fix)
#     - PR #127: 2 iterations (test + clippy fixes)
#     - PR #128: 0 iterations (clean)
#     - PR #129: 1 iteration (format fix)
#     
#     Total time: ~3 hours
#     - Prep: 30 min
#     - Planning: 15 min
#     - PR creation: ~30 min each × 5
#     - CI fixes: ~15 min total
#     
#     Original branch preserved: feature/massive-sync-refactor
#     Prep branch kept: feature/massive-sync-refactor-restack-prep
```

---

## Scenario 3: Mid-Development Correction

**Use case:** Realize your branch is getting too large mid-development

```bash
# You're on: feature/growing-feature (40 commits, 85 files)
# Realize: This is getting too big!

# Option 1: Restack now before it's worse
/restack

# Option 2: Split into planned pieces from here
# Create separate branch for docs you've written
/branch "docs/feature-documentation" main
# Cherry-pick just doc commits
git cherry-pick <doc-commits>
git push

# Create PR for docs
/pr

# Continue working on feature branch
git checkout feature/growing-feature

# Rebase to remove doc commits (they're in separate PR now)
git rebase -i main
# Remove doc commits interactively
```

---

## Best Practices Summary

### When to Use Each Command

| Command | Use When |
|---------|----------|
| `/branch` | Starting any new work |
| `/pr` | Creating a pull request |
| `/pr-checks` | After creating PR, monitors CI |
| `/restack` | PR is >20 commits or >50 files |

### Workflow Decision Tree

```
Starting new work?
├─ Small feature (< 20 commits expected)
│  └─ Use: /branch → code → /pr → /pr-checks
│
└─ Large feature (> 20 commits expected)
   ├─ Option A: Plan upfront
   │  └─ Use: Create smaller PRs with /branch for each piece
   │
   └─ Option B: Restack later
      └─ Use: /branch → code → realize it's big → /restack

Already have large PR?
└─ Use: /restack immediately
```

### Command Interactions

```mermaid
/branch
  ↓
Make changes
  ↓
/pr ────────────→ /pr-checks
  ↓                    ↓
  └──→ /restack ──→ Creates multiple branches
          ↓             ↓
          └─────────→ /pr for each
                        ↓
                    /pr-checks for each
```

### Time Estimates

| Workflow | Time |
|----------|------|
| Simple feature | 30 min - 2 hours |
| Medium feature (2-3 PRs) | 2-4 hours |
| Large restack (5+ PRs) | 4-8 hours |
| Includes: coding, testing, CI fixes, PR creation |

### Tips for Success

1. **Start with `/branch`** - Always create branches from remote
2. **Use conventional commits** - Makes restacking easier
3. **Run `/pr-checks`** - Let AI handle CI issues automatically
4. **Restack early** - Don't wait until 200 files
5. **Keep PRs focused** - One concern per PR
6. **Document dependencies** - Link related PRs
7. **Merge promptly** - Don't let stack get stale

---

## Complete Example Timeline

**Day 1: Start feature**
```
09:00 - /branch "sync/feat-cool-feature"
09:05 - Start coding
12:00 - 30 commits, 60 files... getting big
12:30 - Decide to restack
13:00 - /restack (start)
```

**Day 1: Restack**
```
13:00 - AI preps branch, suggests 3 PRs
13:15 - Create PR #1 (docs)
13:30 - /pr-checks passes
13:45 - Create PR #2 (core)
14:00 - /pr-checks finds lint issue
14:05 - AI auto-fixes, re-checks
14:10 - All green
14:15 - Create PR #3 (features)
14:30 - /pr-checks passes
14:45 - Done! 3 PRs ready
```

**Day 2: Reviews and merges**
```
09:00 - PR #1 reviewed, approved, merged
10:00 - PR #2 reviewed, requested changes
11:00 - Make changes, push
11:15 - /pr-checks auto-validates
12:00 - PR #2 approved, merged
14:00 - PR #3 reviewed, approved, merged
```

**Result:** Feature merged in 2 days vs weeks for large PR

---

## Resources

- **Branch creation:** [branch.md](./branch.md)
- **Restacking:** [restack.md](./restack.md)
- **Quick reference:** [../RESTACK_QUICKREF.md](../RESTACK_QUICKREF.md)
- **CI automation:** [pr-checks.md](./pr-checks.md)
- **All commands:** [../README_COMMANDS.md](../README_COMMANDS.md)

---

**Remember:** These commands work together to make PR management effortless. Start simple with `/branch`, use `/restack` when needed, and let `/pr-checks` handle CI automatically.

