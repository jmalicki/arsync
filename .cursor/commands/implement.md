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
   - If path provided: Use that plan (can be plan.md or plan/phase-X.md)
   - If name provided: Look for `docs/projects/NAME/plan.md`
   - If nothing provided: Infer from:
     - Current project context (most recent project discussed)
     - Open files matching a project folder
     - Most recent plan.md accessed
     - Current conversation context

   **Two-Level Plan Support**:
   - For highly complex projects with `plan.md` + `plan/` directory structure:
     - `/implement` → Works on high-level plan.md (finds next unchecked phase)
     - `/implement plan/phase-X.md` → Works on specific phase's detailed tasks
     - When all tasks in phase-X.md are ✅, checks off that phase in plan.md
   - See "Two-Level Plan Structure" section below for details

2. **Verifies branch**
   - Read "Implementation Branch" from plan.md header
   - Check current git branch
   - If mismatch: Warn and suggest creating/switching to correct branch
   - If correct: Proceed with implementation

3. **Reads current progress**
   - Parse all checkboxes in the plan
   - Identify completed items: `[x]` or `[X]`
   - Identify pending items: `[ ]`
   - Find current phase and next unchecked item

4. **Executes next steps**
   - Implement the next unchecked item(s)
   - Follow the instructions in the plan
   - Run quality checks as specified
   - Update checkboxes as work is completed

5. **Handles problems**
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
- ✅ **Verify you're on the correct branch** (check plan header)
- ✅ Follow the plan steps in order
- ✅ Run specified quality checks (`/fmt`, `/clippy`, `/test`, etc.)
- ✅ Check boxes as items complete
- ✅ Add notes for context, issues, or decisions
- ✅ Update plan file to track progress
- ✅ Commit work at logical checkpoints
- ✅ Ask for input if ambiguous

### DON'T:
- ❌ **Implement on wrong branch** (verify branch matches plan header)
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
# Previously created plan: docs/projects/sparse-file-support/plan.md
# Plan header says: **Implementation Branch**: copy/feat-sparse-files
# Some items are checked, some aren't

# Make sure you're on the right branch first
git checkout copy/feat-sparse-files
# Or: /branch "copy/feat-sparse-files" main origin true

/implement
# Verifies: Current branch matches plan's "Implementation Branch"
# Reads plan, finds next unchecked item
# Executes it, updates checkbox
# Runs quality checks
# Reports progress
```

### Scenario 2: Start implementing a plan
```bash
# Just created implementation plan in design branch
# On branch: feature/design
# Plan created: docs/projects/feature/plan.md
# Plan header says: **Implementation Branch**: area/feat-feature

# Create the implementation branch (from plan header)
/branch "area/feat-feature" main origin true

# Start implementing
/implement @docs/projects/feature/plan.md
# Verifies: On branch area/feat-feature ✓
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

# Check what branch the plan expects
# (Read Implementation Branch from docs/projects/feature-name/plan.md)

# Ensure on correct branch
git checkout area/feat-feature-name
# Or if not created yet:
# /branch "area/feat-feature-name" main origin true

/implement "feature-name"
# Verifies: Current branch matches plan's "Implementation Branch"
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

### Scenario 6: Two-level plan for complex project
```bash
# Working on highly complex project with 7 phases
# Structure:
#   docs/projects/rsync-wire/plan.md
#   docs/projects/rsync-wire/plan/phase-1-handshake.md
#   docs/projects/rsync-wire/plan/phase-2-compio.md
#   ...

# Start with overview
cat docs/projects/rsync-wire/plan.md
# See: Phase 1 is unchecked, Phase 2-7 are unchecked

# Work on Phase 1 details
/implement plan/phase-1-handshake.md
# Executes tasks from phase-1-handshake.md
# Checks boxes in that file
# Commits at milestones

# Continue Phase 1
/implement plan/phase-1-handshake.md
# Resumes from next unchecked task

# After completing all Phase 1 tasks (all ✅ in phase-1-handshake.md)
/implement
# Agent reads plan.md
# Sees Phase 1 details are complete
# Checks off: [x] Phase 1: Handshake Protocol
# Updates stats in plan.md
# Suggests: /implement plan/phase-2-compio.md

# Move to Phase 2
/implement plan/phase-2-compio.md
# Works through Phase 2 detailed tasks
...

# Check overall status anytime
cat docs/projects/rsync-wire/plan.md
# See: [x] Phase 1 ✅, [x] Phase 2 ✅, [ ] Phase 3, ...
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

## Two-Level Plan Structure

For highly complex projects (7+ phases, 1000+ lines of plan), use a two-level structure:

### Structure

```
docs/projects/PROJECT/
├── plan.md                    ← High-level phase checklist
└── plan/                      ← Detailed task directory
    ├── README.md              (index)
    ├── phase-1-name.md        (detailed tasks)
    ├── phase-2-name.md        (detailed tasks)
    ├── phase-3-name.md        (detailed tasks)
    ├── statistics.md          (stats)
    ├── testing.md             (test matrix)
    └── skipped.md             (decisions)
```

### plan.md Format (High-Level)

```markdown
# Implementation Plan: [Project Name]

## Phases
- [ ] Phase 1: Handshake Protocol → [plan/phase-1-handshake.md](plan/phase-1-handshake.md)
  - Core data structures, state machine, API
  - Commits: TBD | Tests: 19 expected
  
- [ ] Phase 2: compio Migration → [plan/phase-2-compio.md](plan/phase-2-compio.md)
  - Transport trait redesign, migration to io_uring
  - Commits: TBD | Tests: Update all existing

- [ ] Phase 3: Integration → [plan/phase-3-integration.md](plan/phase-3-integration.md)
  - Integration testing with external systems
  - Commits: TBD | Tests: 5 expected
...
```

### plan/phase-X.md Format (Detailed)

Each phase file contains detailed task checklists:

```markdown
# Phase 1: Handshake Protocol

**Status**: In Progress
**Commits**: 37451a4, d25e02a, e7bc831

## 1.1: Core Data Structures
- [ ] Create src/protocol/handshake.rs
- [ ] Define HandshakeState enum (9 states)
- [ ] Define ProtocolCapabilities struct
- [ ] Add unit tests (7 expected)
...

## 1.2: State Machine
- [ ] Implement HandshakeState::advance()
- [ ] Handle all 9 state transitions
- [ ] Add error handling
...

## Acceptance Criteria
- [ ] 14 unit tests passing
- [ ] All state transitions tested
- [ ] Code formatted and clippy clean
```

### Workflow

**Starting a new phase:**
```bash
# Check high-level status
cat docs/projects/PROJECT/plan.md
# See: Phase 1 is unchecked

# Work on that phase's details
/implement plan/phase-1-handshake.md
# Works through detailed tasks in phase-1-handshake.md
# Checks boxes as tasks complete

# Continue working on phase
/implement plan/phase-1-handshake.md
# Resumes from next unchecked task
```

**When phase complete:**
```bash
# All tasks in plan/phase-1-handshake.md are ✅

# The agent automatically:
# 1. Marks Phase 1 as complete in plan.md
# 2. Updates stats (commits, tests)
# 3. Suggests next phase

# Or manually:
/implement
# Agent sees Phase 1 is complete
# Moves to Phase 2, suggests: /implement plan/phase-2-compio.md
```

**Working at high level:**
```bash
# From plan.md level
/implement
# Agent checks plan.md
# Finds first unchecked phase (e.g., Phase 2)
# Suggests: "/implement plan/phase-2-compio.md to work on details"
# Or can auto-start if clear
```

### Benefits

**Two-level structure advantages**:
- ✅ Quick overview: `plan.md` shows all phases at a glance
- ✅ Deep dive: `plan/phase-X.md` has 100+ detailed tasks
- ✅ Focused work: Implement one phase file at a time
- ✅ Easy navigation: 7 high-level items → 11 focused files
- ✅ Scalable: Add phases without making plan.md unwieldy
- ✅ Clear progress: Check off phases as major milestones

**Reference Implementation**: `docs/projects/rsync-wire/`
- Successfully used for 7-phase protocol implementation
- Kept 3000-line plan manageable
- Easy to track progress across multiple sessions

### When to Use

Use two-level structure for:
- 7+ implementation phases
- 1000+ lines of detailed tasks
- Multi-week projects
- Complex protocols or systems
- Multiple subsystems/components
- Extensive testing matrices

Use single plan.md for:
- 1-6 phases
- <500 lines of tasks
- Single-week projects
- Straightforward features

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

