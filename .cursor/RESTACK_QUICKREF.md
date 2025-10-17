# 🔄 Restack Quick Reference

## One-Line Summary
Break a large PR into reviewable pieces that build on each other.

## When to Use
- ✅ PR has >20 commits or >50 files
- ✅ Review is stalled due to size
- ✅ Multiple concerns mixed together
- ✅ Want to merge parts incrementally

## Quick Command
```bash
/restack
```

## The Process

### Phase 1: Prep (Automatic)
```
You: Currently on feature/large-pr
AI:  Creating feature/large-pr-restack-prep
     Squashing 87 commits → 1 commit
     Rebasing on origin/main
     ✅ Ready for restacking
```

### Phase 2: Stack Creation (Interactive Loop)

Each iteration:
```
AI:  Analyzing remaining changes...
     Suggested PR: docs/update-readme (5 files)
     
     Creating branch from main...
     Applying documentation changes...
     Running tests... ✅ Pass
     Pushing to origin...
     Creating PR #125
     Running /pr-checks... ✅ All green
     
     Rebasing prep branch off docs/update-readme
     
     ✅ Iteration 1 complete
     Next base: docs/update-readme
     
[Repeat until prep branch is empty]
```

## Recommended Grouping Order

1. **Documentation** - Context for reviewers
2. **Test Infrastructure** - Shows expected behavior
3. **Core Changes** - Minimal working version
4. **Features** - Build on core incrementally  
5. **Optimizations** - Performance improvements

## Example Transformation

**Before:**
```
feature/massive-refactor
├─ 100 commits
├─ 200 files changed
└─ Review: "Too large to review" 😓
```

**After:**
```
main
├─ docs/architecture        (PR #125) ✅ merged
│  └─ test/framework        (PR #126) ✅ approved
│     └─ sync/core-refactor (PR #127) 🔍 reviewing
│        └─ sync/adaptive   (PR #128) 🕐 waiting
│           └─ perf/optimize (PR #129) 🕐 waiting

Result: 5 focused PRs, easier review, parallel work
```

## Key Benefits

- ✅ **Faster reviews** - Each PR is small and focused
- ✅ **Parallel work** - Multiple reviewers can work simultaneously  
- ✅ **Incremental merge** - Land parts while refining others
- ✅ **Easier rollback** - Can revert individual PRs
- ✅ **Better CI** - Smaller changes = clearer failures
- ✅ **Clear history** - Logical progression preserved

## The Stack Dependency Chain

```
PR #125 (docs)          ← Base: main
  ↓ depends on
PR #126 (tests)         ← Base: PR #125
  ↓ depends on
PR #127 (core)          ← Base: PR #126
  ↓ depends on
PR #128 (feature)       ← Base: PR #127
  ↓ depends on
PR #129 (perf)          ← Base: PR #128
```

**Merge Order:** Must merge in sequence (125 → 126 → 127 → 128 → 129)

## What the AI Does Automatically

✅ Creates prep branch  
✅ Squashes commits  
✅ Rebases on latest main  
✅ Analyzes changes  
✅ Suggests logical groupings  
✅ Creates branches  
✅ Applies changes  
✅ Runs tests  
✅ Pushes to remote  
✅ Creates PRs with good descriptions  
✅ Runs `/pr-checks` (auto-fixes CI)  
✅ Rebases prep branch  
✅ Tracks progress  

## What You Control

🤔 Approve suggested groupings (or modify)  
🤔 Choose which changes go in each PR  
🤔 Review PR descriptions before creating  
🤔 Decide when to stop and continue later  

## Common Patterns

### Pattern 1: Linear Dependency
```
main → A → B → C → D
```
Each PR builds on the previous one.

### Pattern 2: Shared Foundation
```
main → foundation
     ├→ feature-x
     └→ feature-y
```
Create foundation first, then parallel features.

### Pattern 3: Refactor Then Build
```
main → refactor → new-feature-using-refactor
```
Modernize code first, add features second.

## Integration with Other Commands

```bash
# 1. Create branch for each piece
/branch "docs/update-readme" main

# 2. Make PR
/pr

# 3. Auto-fix CI issues
/pr-checks

# 4. Use /restack to orchestrate all of the above
/restack
```

## Troubleshooting

| Issue | Solution |
|-------|----------|
| Too many conflicts | Make smaller groupings |
| CI fails on small PR | Include more dependencies |
| Later PRs have conflicts | Rebase prep branch regularly |
| Reviewer confused | Add dependency notes to PR description |

## Quick Checklist

Before starting:
- [ ] Large PR is pushed to remote
- [ ] Working directory is clean
- [ ] On the large PR branch

During restacking:
- [ ] Review suggested groupings
- [ ] Each PR has tests
- [ ] Each PR passes CI independently
- [ ] PRs have clear dependency notes
- [ ] Good conventional commit messages

After completion:
- [ ] All changes are in separate PRs
- [ ] Dependencies documented
- [ ] Original branch preserved
- [ ] Prep branch kept until all merge

## Time Estimate

- Small PR (20-50 files): ~30 min
- Medium PR (50-100 files): ~1-2 hours
- Large PR (100+ files): ~2-4 hours

*Includes prep, planning, and CI validation*

## Remember

🎯 **Goal**: Reviewable PRs, not perfect grouping  
🎯 **Each PR**: Should stand alone and pass CI  
🎯 **Stack order**: Logical dependencies, merge in sequence  
🎯 **Original branch**: Preserved, safe to restart  

## Resources

- Full docs: [commands/restack.md](./commands/restack.md)
- Branch creation: [commands/branch.md](./commands/branch.md)
- CI automation: [commands/pr-checks.md](./commands/pr-checks.md)
- All commands: [README_COMMANDS.md](./README_COMMANDS.md)

---

**Pro Tip:** Run `/restack` as soon as you realize your PR is getting large. Earlier is easier!

