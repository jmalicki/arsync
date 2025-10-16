# /plan

Create a comprehensive, phase-based implementation plan based on current context or a given design document.

- design_doc (string, optional): Path to design document (.md file) to inform the plan
  - Can use file path: `"docs/design.md"`
  - Can use @-mention: `@docs/design.md`
  - Cursor expands @-mentions automatically

```bash
/plan
/plan "docs/designs/adaptive-buffers.md"
/plan @docs/designs/sparse-files.md
```

## Context Inference

The command will automatically analyze:

1. **Design document** (if provided) - Parse for:
   - Complexity indicators (scope, dependencies, risk)
   - Requirements and acceptance criteria
   - Technical approach and architecture
   - Testing and performance requirements
   
2. **Current conversation** - What feature/task has been discussed

3. **Open files** - What code is being reviewed/modified

4. **Recent changes** - Git diff to understand scope

5. **Related code** - Identify affected modules and dependencies

**Complexity is automatically determined from:**
- Design doc scope and requirements
- Number of files/modules affected
- Dependencies and integrations required
- Testing complexity
- Performance/security considerations
- Breaking changes or migrations

If context is unclear, the agent will:
- Ask clarifying questions about the feature scope
- Request a brief description or design doc
- Suggest reviewing relevant code first

## Plan Structure

Generate a detailed implementation plan with the following structure:

### Header
```markdown
# Implementation Plan: [Feature Name - inferred from context]

**Status**: Planning
**Complexity**: [simple/medium/complex - automatically inferred]
**Estimated Duration**: [time estimate based on complexity and scope]
**Branch**: [suggested branch name following <area>/<verb-noun> convention]
**Related Design**: [Link to design doc if provided: `../designs/FEATURE_NAME.md`]

## Context
[Brief summary of what was inferred from:
- Current conversation and user intent
- Open files and cursor position
- Recent git changes
- Design document (if provided)]

## Overview
[1-2 paragraph description of what will be implemented and why]

## Design References
[If design_doc provided:
- **Design Document**: [`docs/designs/FEATURE_NAME.md`](../designs/FEATURE_NAME.md)
- Key design decisions
- Architecture choices
- API contracts
- Performance requirements
- Acceptance criteria from design]

## Prerequisites
- [ ] Review related code in [relevant modules - identified from context]
- [ ] Understand current implementation of [related features]
- [ ] Check for existing tests in [test files - identified from codebase]
- [ ] Read design doc: [path] (if provided)
```

### Phase Breakdown

For **simple** tasks (1-2 phases):
- Phase 1: Implementation
- Phase 2: Testing & PR

For **medium** tasks (3-4 phases):
- Phase 1: Design & Setup
- Phase 2: Core Implementation
- Phase 3: Testing & Validation
- Phase 4: Documentation & PR

For **complex** tasks (4-6 phases):
- Phase 1: Research & Design
- Phase 2: Infrastructure/Scaffolding
- Phase 3: Core Implementation
- Phase 4: Integration & Edge Cases
- Phase 5: Comprehensive Testing
- Phase 6: Documentation, Benchmarks & PR

### Each Phase Must Include:

1. **Clear objective** - What this phase accomplishes
2. **Detailed steps** - Individual checkbox items for each action
3. **Quality checks** - Using our slash commands:
   - `[ ] /fmt false true` - Format code
   - `[ ] /clippy false false` - Run clippy
   - `[ ] /test "module_name"` - Run relevant tests
   - `[ ] /smoke` - Run smoke tests (when appropriate)
4. **Files to modify** - List specific files with line numbers if known
5. **Tests to write** - Specific test cases with locations

### Example Phase Format:

```markdown
## Phase 1: Research & Design

**Objective**: Understand current implementation and design the solution

### Steps
- [ ] Read and understand `src/module.rs` (lines 100-200)
- [ ] Identify extension points in `StructName`
- [ ] Review similar implementations in [related code]
- [ ] Document design decisions in `docs/` or inline comments
- [ ] Sketch out new types/functions needed

### Quality Checks
- [ ] /review - Review current changes
- [ ] Document design approach (inline or in docs/)

### Files to Review
- `src/module.rs` - Current implementation
- `tests/module_tests.rs` - Existing test patterns
- `docs/DESIGN.md` - Design documentation

### Next Phase Prerequisites
- Design documented
- No blockers identified
```

## Testing Requirements

Every plan must include dedicated testing items:

### Unit Tests
- [ ] Test happy path for [functionality]
- [ ] Test error cases: [specific errors]
- [ ] Test edge cases: [empty input, max values, etc.]
- [ ] Test boundary conditions
- [ ] `/test "test_name"` - Verify tests pass

### Integration Tests
- [ ] Test integration with [related systems]
- [ ] Test end-to-end workflow
- [ ] `/test "integration_test_name"` - Run integration tests

### Smoke Tests
- [ ] `/smoke` - Verify core functionality still works

### Performance (if applicable)
- [ ] Add benchmark for [operation]
- [ ] `/bench true false` - Run quick benchmarks
- [ ] Compare before/after results

## Quality Gates

Every plan must include these quality checkpoints:

### Code Quality
- [ ] `/fmt false true` - Format all code
- [ ] `/clippy false false` - Fix all clippy warnings
- [ ] `/build "release" "all" false` - Verify release build

### Testing
- [ ] `/test "all"` - All tests pass
- [ ] `/smoke` - Smoke tests pass
- [ ] Test coverage for new code > 80%

### Documentation
- [ ] Update inline documentation for public APIs
- [ ] Update relevant docs/ files
- [ ] Add examples if public API changed
- [ ] `/docs true false` - Verify docs build

### Final Checks
- [ ] `/review` - Review all changes
- [ ] Update CHANGELOG.md with changes
- [ ] Verify no TODOs or FIXME comments remain
- [ ] Check for debug prints or commented code

## PR Checklist

The final phase must always include PR preparation:

```markdown
## Final Phase: Create Pull Request

### Pre-PR Verification
- [ ] `/fmt true true` - Verify formatting (check mode)
- [ ] `/clippy false false` - Verify no warnings
- [ ] `/test "all"` - All tests pass
- [ ] `/smoke` - Smoke tests pass
- [ ] `/build "release" "all" false` - Release build succeeds
- [ ] `/docs true false` - Documentation builds
- [ ] `/review` - Final review of changes

### Benchmarks (if performance-related)
- [ ] `/bench true false` - Quick benchmark
- [ ] Compare results with baseline
- [ ] Document performance impact

### PR Creation
- [ ] `/commit "feat(area): description"` - Commit all changes
- [ ] `/pr "feat(area): description" "See template" main false`
- [ ] Fill out PR template completely
- [ ] `/pr-ready "feat(area): description"` - Push and create PR
- [ ] `/pr-checks` - Monitor CI checks

### PR Body Checklist
- [ ] Summary of changes (1-3 bullets)
- [ ] Motivation and context
- [ ] Test plan described
- [ ] Performance impact noted (if applicable)
- [ ] Breaking changes called out (if any)
- [ ] Screenshots/examples (if UI/CLI changed)
```

## Output Format

The plan should be output as a complete markdown document and saved to:

**Location**: `docs/implementation-plans/FEATURE_NAME.md`

**Naming conventions**:
- If design doc provided: Match the design doc name
  - Design: `docs/designs/sparse-file-support.md`
  - Plan: `docs/implementation-plans/sparse-file-support.md`
- If no design doc: Derive from feature name in context
  - Feature: "Add adaptive buffer sizing"
  - Plan: `docs/implementation-plans/adaptive-buffer-sizing.md`

**Reference to design doc**:
- If design doc exists, include link at the top:
  ```markdown
  **Related Design**: [Design Document](../designs/FEATURE_NAME.md)
  ```

The plan can be:
1. Saved to `docs/implementation-plans/FEATURE_NAME.md`
2. Used as a checklist during implementation
3. Referenced in PR descriptions
4. Tracked using `/todo` commands

## Best Practices

1. **Be specific** - No vague items like "implement feature"
2. **Atomic tasks** - Each checkbox should be completable in < 1 hour
3. **Ordered logically** - Dependencies should come before dependents
4. **Include file paths** - Always specify which files to modify
5. **Test-driven** - Write tests before or alongside implementation
6. **Quality first** - Run checks frequently, not just at the end
7. **Document as you go** - Don't leave docs for the end

## Example Usage Scenarios

### Scenario 1: Context from conversation
```bash
# User has been discussing adaptive buffer sizing
# Agent infers feature from conversation
/plan
```

### Scenario 2: With design document (file path)
```bash
# User has a design doc: docs/designs/sparse-file-support.md
/plan "docs/designs/sparse-file-support.md"
# Creates: docs/implementation-plans/sparse-file-support.md
# Links to: ../designs/sparse-file-support.md
```

### Scenario 2b: With design document (@-mention)
```bash
# Using Cursor's @-mention syntax (recommended)
/plan @docs/designs/sparse-file-support.md
# Creates: docs/implementation-plans/sparse-file-support.md
# Includes reference to design doc at top
```

### Scenario 3: Open files provide context
```bash
# User has src/copy.rs open with cursor on buffer allocation
# Agent infers this is about buffer management and determines complexity
/plan
```

### Scenario 4: From git diff
```bash
# User has WIP changes in sync.rs
# Agent analyzes diff to understand intent
/plan
```

### Scenario 5: Complex feature with design doc
```bash
# Design doc: docs/designs/distributed-sync.md
# Agent parses doc and determines appropriate complexity
/plan @docs/designs/distributed-sync.md
# Creates: docs/implementation-plans/distributed-sync.md
# Header includes: **Related Design**: [../designs/distributed-sync.md](../designs/distributed-sync.md)
```

## Integration with Other Commands

The plan should reference and integrate these commands at appropriate points:

- `/branch` - Create feature branch
- `/fmt` - Code formatting
- `/clippy` - Linting
- `/test` - Run tests
- `/smoke` - Quick validation
- `/bench` - Benchmarking
- `/build` - Build verification
- `/docs` - Documentation
- `/review` - Change review
- `/commit` - Create commits
- `/pr` - Create pull request
- `/pr-ready` - Push and verify
- `/pr-checks` - Monitor CI

## Context Analysis Process

When invoked, the agent should:

1. **Analyze conversation history**
   - What has been discussed in recent messages?
   - What problems are being solved?
   - What features are being designed?

2. **Review open/recent files**
   - Which files are currently open?
   - Where is the cursor positioned?
   - What code is selected or highlighted?

3. **Check git status**
   - Any uncommitted changes?
   - What files have been modified?
   - What does the diff show?

4. **Parse design document** (if provided)
   - Works with both file paths and @-mentions
   - Extract key requirements
   - Identify technical approach
   - Note performance/security concerns
   - List acceptance criteria

5. **Determine complexity automatically**
   - Analyze design doc (if provided):
     - Stated complexity or scope sections
     - Number of requirements/acceptance criteria
     - Performance/security requirements
     - Migration or breaking change indicators
   - Analyze context:
     - Scope of changes (files affected)
     - Dependencies involved
     - New feature vs. modification vs. bug fix
     - Testing requirements
     - Integration complexity

6. **Generate plan**
   - Determine feature name (from design doc or context)
   - Create output path: `docs/implementation-plans/FEATURE_NAME.md`
   - Create phases appropriate to complexity
   - Reference specific files and line numbers
   - Include all quality checks
   - Integrate design doc requirements
   - Link to design doc if provided

## Notes

- Plans are living documents - update as you learn
- Not every checkbox needs to be followed exactly
- Adjust based on what you discover during implementation
- Focus on delivering value, not checking boxes
- Skip irrelevant steps (e.g., benchmarks for doc changes)
- If context is ambiguous, ask for clarification before generating plan

