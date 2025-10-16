# /implement

Execute the next steps in an implementation plan, tracking progress via checkboxes.

- plan (string, optional): Path to implementation plan or design doc name

```bash
/implement                                      # Infer from context/recent work
/implement @docs/implementation-plans/feature.md
/implement "feature-name"                       # Find matching plan
```

## How It Works

The command automatically:

1. **Locates the plan**
   - If path provided: Use that plan
   - If name provided: Look for `docs/projects/NAME/plan.md`
   - If nothing provided: Infer from:
     - Current project context (most recent project discussed)
     - Open files matching a project folder
     - Most recent plan.md accessed
     - Current conversation context

2. **Reads current progress**
   - Parse all checkboxes in the plan
   - Identify completed items: `[x]` or `[X]`
   - Identify pending items: `[ ]`
   - Find current phase and next unchecked item

3. **Executes next steps**
   - Implement the next unchecked item(s)
   - Follow the instructions in the plan
   - Run quality checks as specified
   - Update checkboxes as work is completed

4. **Handles problems**
   - If issues arise during implementation
   - **Use `/debug` for systematic diagnosis**
   - Add notes to plan under relevant section
   - Do NOT modify the original step text
   - Format: `**Note**: [explanation of issue/resolution]`
   - Continue or ask for guidance

## Plan Progress Tracking

### Reading Checkboxes

```markdown
## Phase 1: Setup
- [x] Review existing code         ← Completed
- [x] Identify extension points    ← Completed  
- [ ] Create new struct            ← NEXT (this is where we continue)
- [ ] Add tests                    ← Pending
```

### Adding Notes (Without Editing Steps)

```markdown
## Phase 2: Implementation
- [x] Implement basic functionality
  **Note**: Initially used approach A, but switched to approach B for better performance.
  See commit abc123.

- [ ] Add error handling
  **Note**: Blocked pending resolution of issue #123. Continuing with other items.
```

## Execution Strategy

### Single Step Mode (Default)
- Execute the next unchecked item
- Run any associated quality checks
- Check the box when complete
- Report status and next step

### Multi-Step Mode
- Continue through multiple steps if they're related
- Stop at phase boundaries
- Stop if quality checks fail
- Stop if manual intervention needed

### Quality Check Integration

When plan specifies checks, run them:
```markdown
- [ ] Implement feature X
- [ ] /fmt false true
- [ ] /clippy false false  
- [ ] /test "module"
```

Execute each in order, checking boxes as they pass.

## Implementation Guidelines

### DO:
- ✅ Follow the plan steps in order
- ✅ Run specified quality checks (`/fmt`, `/clippy`, `/test`, etc.)
- ✅ Check boxes as items complete
- ✅ Add notes for context, issues, or decisions
- ✅ Update plan file to track progress
- ✅ Commit work at logical checkpoints
- ✅ Ask for input if ambiguous

### DON'T:
- ❌ Modify the original step text
- ❌ Skip steps without good reason
- ❌ Ignore quality check failures
- ❌ Remove steps from the plan
- ❌ Proceed if fundamentally blocked
- ❌ Edit phase structure

## Adding Notes Format

### Good Notes:
```markdown
- [x] Implement caching layer
  **Note**: Used LRU cache instead of TTL-based as discussed. Performance improved 40%.
  Benchmarks in commit def456.

- [ ] Add integration tests
  **Note**: Requires test fixtures from PR #789. Proceeding with unit tests first.
  
- [x] Update documentation
  **Note**: Added examples for common use cases. Addressed feedback from code review.
```

### Bad Notes (Don't Do This):
```markdown
- [x] Implement caching layer ← Changed to "Implement LRU cache"  ❌ Don't edit step text
  
- [ ] Add integration tests (BLOCKED: waiting on #789)  ❌ Don't modify step inline

## Phase 2: Testing ← SKIPPED, did Phase 3 first  ❌ Don't skip/reorder phases
```

## Progress Reporting

After each execution, report:

```markdown
## Implementation Progress: [Feature Name]

**Current Phase**: Phase 2 of 4 - Core Implementation
**Progress**: 8/24 items completed (33%)

### Just Completed:
- [x] Implement buffer allocation strategy
  **Note**: Used arena allocator for better locality. 15% perf improvement.

### Next Up:
- [ ] Add error handling for OOM scenarios
  Location: src/buffer.rs:145
  Estimated: 30 minutes

### Blockers:
- Integration tests waiting on PR #789
- Performance benchmark needs production dataset

### Quality Status:
- ✅ Tests passing (12/12)
- ✅ Clippy clean
- ✅ Formatted
- ⏸️ Benchmarks pending
```

## Example Workflows

### Scenario 1: Continue from where we left off
```bash
# Previously created plan: docs/implementation-plans/sparse-file-support.md
# Some items are checked, some aren't

/implement
# Reads plan, finds next unchecked item
# Executes it, updates checkbox
# Runs quality checks
# Reports progress
```

### Scenario 2: Start implementing a plan
```bash
# Just created implementation plan
/plan @docs/projects/feature/design.md
# Creates: docs/projects/feature/plan.md

# Start implementing
/implement @docs/projects/feature/plan.md
# Starts from first unchecked item (probably Phase 1, first step)

# After completing Phase 1
/commit "feat(module): implement phase 1 - scaffolding"
/pr-ready "feat(module): add feature name"
# Creates draft PR for early feedback
```

### Scenario 3: Resume after a break
```bash
# Coming back to a partially completed feature
# Plan is at Phase 2, items 5/12 done

/implement "feature-name"
# Finds docs/projects/feature-name/plan.md
# Resumes at item 6

# Continue working...
# After completing more items
/commit "feat(module): complete phase 2 items 6-8"
/pr-ready "feat(module): add feature name"
# Updates existing PR with new commits
```

### Scenario 4: Handle a blocker
```bash
/implement
# Executing step: "Add integration with service X"
# Agent discovers: Service X API changed, blocking progress
# Agent adds note to plan:
#   **Note**: Service X API changed in v2.0. Needs design update.
#   See issue #456 for discussion. Continuing with other items.
# Agent skips to next non-blocked item
```

### Scenario 5: Implementation fails, debug, fix, continue
```bash
/implement
# Executing step: "Implement buffer management"
# Implementation attempt results in test failure
# Error: "test_buffer_allocation fails with assertion error"

# Switch to debugging
/debug "test_buffer_allocation fails"
# Systematic debugging:
# 1. Add observability (committed)
# 2. Identify root cause: off-by-one in size calculation
# 3. Fix and verify
# 4. Add note to plan

# Plan updated with note:
#   **Note**: Initial implementation had off-by-one error.
#   Used /debug to diagnose. Added unit tests for edge cases (committed).
#   Fixed calculation. All tests passing.
#   See commits: abc123 (tests), def456 (fix).

# Continue implementing
/implement
# Resumes with next unchecked item
```

## Integration with Other Commands

The implementation command uses other slash commands:

- `/debug` - **Use when implementation fails or tests break**
- `/fmt false true` - Format code as specified in plan
- `/clippy false false` - Run linter as specified
- `/test "module"` - Run tests as specified
- `/smoke` - Run smoke tests when phase completes
- `/build "release" "all" false` - Build as needed
- `/commit "message"` - Commit at checkpoints
- `/pr-ready "title"` - **Create or update PR after phase completion**
- `/pr-checks` - Watch CI checks
- `/review` - Review changes before final PR

### When to Use `/debug` During Implementation

**Use `/debug` when:**
- ✅ Tests fail after implementing a step
- ✅ Compilation errors that aren't immediately clear
- ✅ Implementation works but performance is wrong
- ✅ Intermittent test failures
- ✅ Unexpected behavior in implemented feature
- ✅ Edge cases causing issues

**Process:**
1. Attempt implementation step
2. If it fails: `/debug` with specific issue
3. Follow debugging process (add observability first!)
4. Add note to plan documenting the issue and solution
5. Commit fixes (including observability improvements)
6. Continue with `/implement`

**Example flow:**
```bash
/implement                          # Try to implement next step
# Fails with error

/debug "specific error description" # Debug systematically
# Add observability (commit)
# Identify root cause
# Fix issue (commit)

/implement                          # Continue with plan
# Plan now has note about the issue
# Observability improvements are committed
```

## Plan Updates

The command updates the plan file directly:

**Before**:
```markdown
## Phase 2: Implementation
- [x] Create base structure
- [ ] Add validation logic
- [ ] Implement error handling
```

**After `/implement`**:
```markdown
## Phase 2: Implementation
- [x] Create base structure
- [x] Add validation logic
  **Note**: Added custom validator for edge case handling. See src/validator.rs.
- [ ] Implement error handling
```

## State Management

### Plan Metadata

The command may add metadata at top of plan:

```markdown
# Implementation Plan: Feature Name

**Status**: ~~Planning~~ → In Progress
**Progress**: 8/24 (33%)
**Current Phase**: Phase 2 - Core Implementation
**Last Updated**: 2025-10-15
**Last Worked**: 2025-10-15 21:30

...
```

### Checkpoint Commits

At logical boundaries (especially after completing phases), commit and update PR:
```bash
# After completing a phase
/commit "feat(module): implement phase 2 - core functionality

- Added buffer management
- Implemented caching layer  
- Added unit tests

Part of implementation plan: docs/projects/feature/plan.md
Phase 2 of 4 complete."

# Push to existing PR or create new one if none exists
/pr-ready "feat(module): feature name"
# This command:
# - Pushes the branch
# - Creates PR if none exists
# - Updates existing PR if one is already open
# - Shows PR URL and CI status

# Optionally watch CI
/pr-checks
```

**Best practice:** Create/update PR after each major phase completion. This:
- Allows early feedback on approach
- Shows progress incrementally
- Makes code review easier (smaller chunks)
- Catches integration issues early
- Enables parallel work on different phases

## Error Handling

### When Implementation Fails

**If something doesn't work on first attempt, use `/debug`:**

```bash
# Implementation attempt fails
/implement
# Error: "test_feature fails with assertion error"

# Switch to debugging mode
/debug "test_feature fails after implementing step X"
# Follow systematic debugging process
# Add observability, form hypothesis, test fix
# Document findings in implementation plan

# Return to implementation
/implement
# Continue with next steps
```

### Compilation Errors
```markdown
- [ ] Implement feature X
  **Note**: Compilation error in initial approach. Error was: [error details].
  Used `/debug` to diagnose. Issue: [root cause].
  Fixed by [explanation]. See commit abc123.
```

### Test Failures
```markdown
- [ ] Add integration tests
  **Note**: Tests failing with intermittent errors.
  Used `/debug` to identify race condition in async code.
  Added tracing instrumentation (committed).
  Fixed synchronization. Tests now passing consistently.
  See commits: debug instrumentation (abc123), fix (def456).
```

### Blocked Items
```markdown
- [ ] Deploy to staging
  **Note**: BLOCKED - Requires staging environment setup. See issue #789.
  Continuing with other Phase 3 items. Will return to this.
```

### Complex Issues Requiring Investigation
```markdown
- [ ] Implement caching layer
  **Note**: Performance not meeting targets after initial implementation.
  Used `/debug @src/cache.rs "cache slower than expected"` to profile.
  Added benchmarks and instrumentation (committed).
  Identified: HashMap contention in multi-threaded access.
  Fixed: Switched to DashMap. Performance now 3x target.
  See commits: instrumentation (abc123), benchmark (def456), fix (ghi789).
```

## Completion

When all items are checked:

```markdown
## Implementation Complete! 🎉

**Feature**: [Feature Name]
**Plan**: docs/implementation-plans/feature-name.md  
**Design**: docs/designs/feature-name.md
**Progress**: 24/24 (100%)

### Summary:
- All phases completed
- All tests passing
- All quality checks pass
- No open blockers

### Next Steps:
1. Final review: /review
2. Create PR: /pr "feat(area): feature name"
3. Push and monitor: /pr-ready "feat(area): feature name"
4. Watch CI: /pr-checks

### Stats:
- Commits: 8
- Files changed: 12
- Tests added: 24
- Duration: 3 days
```

## Best Practices

1. **Work incrementally** - One step at a time, verify, move on
2. **Run checks frequently** - Don't accumulate problems
3. **Use `/debug` when stuck** - Don't guess, diagnose systematically
4. **Add observability first** - Make problems visible before fixing
5. **Commit often** - Checkpoint at logical boundaries  
6. **Add useful notes** - Document issues and solutions
7. **Don't skip steps** - Follow the plan, adjust if needed
8. **Update blockers** - Document what's blocking and why
9. **Keep improvements** - Debug instrumentation and tests stay in code
10. **Ask for help** - If truly stuck, involve human
11. **Stay focused** - Complete phases before jumping ahead

## Notes

- The command modifies the plan file to track progress
- Original step text is never changed, only checkboxes and notes
- Plan file should be committed as progress is made
- If plan needs structural changes, discuss with user first
- The command is iterative - run it multiple times to complete plan
- Can pause and resume anytime
- Works across multiple sessions

