# /pr-checks

Watch PR checks for the current branch. **Immediately enters debug/fix loop as soon as ANY subjob fails** - does not wait for other jobs to complete.

Recommended approach (uses GitHub CLI):
- `gh pr checks --watch` streams status for the open PR tied to the current branch
- If no PR exists yet, use `/pr-ready` first

```bash
/pr-checks
```

## Immediate Failure Handling

When invoked, the command should:

1. **Watch CI checks** - Stream status from all jobs
2. **On FIRST failure** - Immediately stop watching and enter debug loop
3. **Don't wait for other jobs** - React to first failure instantly
4. **Fix the failure** - Use `/debug` to diagnose and fix
5. **Commit fix** - Use `/commit` for conventional commits with validation
6. **Push and re-check** - Push then recursively call `/pr-checks` again
7. **Repeat** - Until all checks pass or manual intervention needed

**Key behavior: React immediately to ANY failure, don't wait for completion.**

### Example Timeline:
```
00:00 - Start watching CI
00:30 - rustfmt: ✅ PASS
00:45 - clippy: ❌ FAIL   ← IMMEDIATELY jump to debug loop
        (don't wait for test job to finish)
        
        /debug "clippy failure..."
        Fix and push
        
        /pr-checks (restart)
        
01:00 - rustfmt: ✅ PASS
01:15 - clippy: ✅ PASS
01:30 - tests: ❌ FAIL    ← IMMEDIATELY jump to debug loop
        (don't wait for bench job)
        
        /debug "test failure..."
        Fix and push
        
        /pr-checks (restart)
```

## The CI Feedback Loop (Automatic)

The agent should automatically handle CI failures in a loop:

### 1. Watch Checks (React Immediately)
```bash
# Agent runs: gh pr checks --watch
# Streams CI status
# As soon as ANY job fails → STOP watching
# Immediately jump to fix that failure
# Don't wait for other jobs to complete
```

### 2. If Checks Fail - Automatic Fix Attempt

**For obvious fixes** (formatting, simple lints):
```bash
# Agent detects: rustfmt failed
# Agent automatically runs:
/fmt false true          # Fix formatting
/clippy true false       # Auto-fix clippy issues

# Use /commit for conventional commits with validation
/commit "fix(ci): address linting issues"
git push

# Agent automatically continues:
/pr-checks               # Re-check CI (recursive call)
```

**For non-obvious failures** (test failures, complex issues):
```bash
# Agent detects: test_name failed in CI
# Agent automatically invokes:
/debug "CI test failure: test_name fails on GitHub Actions"
# Follow debugging process:
# - Reproduce locally
# - Add observability (commit with /commit!)
# - Identify root cause
# - Fix

# Agent uses /commit for proper conventional commits:
/commit "fix(test): resolve issue found in CI

Root cause: [explanation]
Added: [instrumentation/tests]
Fixed: [solution]"

git push

# Agent automatically continues:
/pr-checks               # Re-check CI (recursive call)
```

### 3. Automatic Iteration Until Green

```bash
# The agent automatically loops, reacting immediately to failures:
/pr-checks
# Watching... rustfmt ✅, clippy ✅, tests running...
# ❌ tests FAIL → IMMEDIATELY stop and fix
# Fix → Push → /pr-checks (restart from beginning)

# Watching... rustfmt ✅, clippy ✅, tests ✅, bench running...
# ✅ All green → Report success and stop

# Agent reports: "✅ All CI checks passing. Ready for review!"
```

**Important:** Don't wait for slow jobs to finish if a fast job already failed. Fix failures as soon as they're detected.

### 4. When Manual Intervention Needed

The agent should stop automatic iteration and ask for help when:
- **Unable to reproduce** - Can't reproduce CI failure locally
- **Complex architectural issues** - Requires design decisions
- **Blocked by external factors** - Requires infrastructure changes
- **Repeated failures** - Same fix attempted 3+ times without success
- **Unclear root cause** - After adding observability, still uncertain

In these cases, the agent should:
1. Report the situation clearly
2. Document what was attempted
3. Add detailed notes to the implementation plan
4. Request user guidance

## Common CI Failure Scenarios

### Formatting Issues
```bash
# CI fails: rustfmt check
/fmt false true
/commit "style: apply rustfmt"  # Conventional commit
git push
/pr-checks
```

### Clippy Warnings
```bash
# CI fails: clippy warnings
/clippy false false
# Fix warnings
/commit "fix(lint): address clippy warnings"  # Conventional commit
git push
/pr-checks
```

### Test Failures
```bash
# CI fails: test_feature fails
/debug "test_feature fails in CI but passes locally"
# Common causes:
# - Environment differences
# - Timing issues (races)
# - Missing test isolation
# Add: better test isolation, timeouts, retries

/commit "test(feature): improve test stability for CI"  # Use /commit
git push
/pr-checks
```

### Platform-Specific Issues
```bash
# CI fails: Linux/macOS differences
/debug "build fails on macOS but works on Linux"
# Add conditional compilation or platform-specific fixes

/commit "fix(platform): handle macOS differences"  # Use /commit
git push
/pr-checks
```

### Dependency Issues
```bash
# CI fails: dependency resolution or version conflicts
# Check Cargo.lock, update dependencies

/commit "fix(deps): resolve dependency conflicts"  # Use /commit
git push
/pr-checks
```

## Viewing Detailed Logs

If you need more information:

```bash
# View specific run logs
gh run list --branch $(git rev-parse --abbrev-ref HEAD) --limit 5
gh run view <run-id> --log

# View specific job
gh run view <run-id> --job <job-id> --log

# Download logs for local analysis
gh run view <run-id> --log > ci-failure.log
```

## Best Practices

1. **Fix CI issues immediately** - Don't accumulate failures
2. **Reproduce locally first** - Easier to debug
3. **Add tests for CI-specific issues** - Prevent regression
4. **Keep CI configuration clean** - Regularly review workflows
5. **Use `/debug` for non-obvious issues** - Don't guess
6. **Commit observability improvements** - Help future debugging
7. **Document platform differences** - Add comments for edge cases

## Complete Example: Immediate Reaction to CI Failures

```bash
# User runs:
/pr-checks

# Agent process (automatic, reacts immediately):
# 1. Watches CI: gh pr checks --watch
#    - rustfmt: ✅ PASS (at 00:30)
#    - clippy: ✅ PASS (at 00:45)
#    - tests: running... (at 00:50)
#    - bench: running... (at 00:55)
# 2. At 01:10 - test_concurrent_operations FAILS
#    → IMMEDIATELY stop watching (don't wait for bench)
# 3. Automatically invokes debug:

/debug "test_concurrent_operations fails in CI"
# Agent reproduces: cargo test --test-threads=1 test_concurrent_operations
# Agent identifies: Race condition, timing-sensitive
# Agent adds: Better synchronization and logging
# Agent commits instrumentation:

/commit "debug(test): add tracing for concurrent operations"
git push

# 4. Agent RESTARTS from beginning:
/pr-checks
#    - rustfmt: ✅ PASS
#    - clippy: ✅ PASS  
#    - tests: running...
#    - At 01:30 - test_concurrent_operations still FAILS
#    → IMMEDIATELY stop watching again

# 5. Agent continues debugging with better visibility:
/debug
# Agent finds: Missing await on async operation
# Agent fixes: Add proper await
# Agent commits:

/commit "fix(sync): await async operation to prevent race

Root cause: Future not being awaited in concurrent context.
Added tracing to diagnose (previous commit).
Fixed by ensuring all async operations are properly awaited."

git push

# 6. Agent RESTARTS from beginning:
/pr-checks
#    - rustfmt: ✅ PASS
#    - clippy: ✅ PASS
#    - tests: ✅ PASS
#    - bench: ✅ PASS
# ✅ All checks complete without failures!

# Agent reports:
# "✅ All CI checks passing. Ready for review!
#  - Fixed 1 test failure (test_concurrent_operations)
#  - Added instrumentation in commit abc123
#  - Fixed race condition in commit def456
#  - 2 commits added to PR
#  - Total iterations: 3"
```

**User experience:** Run `/pr-checks` once, the agent handles the entire loop automatically, reacting immediately to any failure.

## Implementation Guidelines

### Immediate Reaction
- Watch all jobs in parallel
- As soon as ANY job fails → immediately enter debug loop
- Don't wait for other jobs to complete
- After fix, restart checking from the beginning

### Loop Detection
- Track which failures have been seen
- If same failure occurs 3+ times, stop and ask for help
- Report progress after each iteration
- Count total restarts (iterations)

### Automatic vs Manual
- **Automatic**: Formatting, linting, simple test fixes
- **Manual intervention needed**: Architecture changes, external dependencies, unclear issues

### Efficiency Note
By reacting immediately to failures:
- Faster feedback loop (don't wait for slow jobs)
- Fix issues sooner
- Multiple issues may be fixed in sequence rather than parallel
- Trade-off: May restart CI multiple times, but fixes happen faster

### Progress Reporting
After each fix iteration, report:
```markdown
## CI Fix Progress

**Iteration 1**: Fixed rustfmt issues (commit abc123)
**Iteration 2**: Fixed clippy warnings (commit def456)  
**Iteration 3**: Debugging test_feature failure...
  - Added instrumentation (commit ghi789)
  - Identified root cause: race condition
  - Applied fix (commit jkl012)
**Iteration 4**: ✅ All checks passing!

Total: 4 commits added, 3 failures resolved
```

## Notes

- CI failures are a normal part of development
- The agent automatically handles the loop: check → debug → fix → push → check
- Good CI hygiene: fix issues immediately, don't let them pile up
- Observability improvements from debugging help the whole team
- Document unusual CI behaviors in code comments
- The command is recursive: `/pr-checks` calls itself until green or blocked

