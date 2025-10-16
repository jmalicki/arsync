# /design

Create a comprehensive design document based on current conversation and context.

- feature_name (string, optional): Override the inferred feature name for the design doc filename

```bash
/design                           # Infer everything from context
/design "sparse-file-support"     # Override feature name
```

## Context Inference

The command automatically analyzes the current context to extract:

1. **Current conversation** 
   - What problem or idea is being discussed?
   - What solutions have been proposed?
   - What trade-offs have been considered?
   - What constraints exist?

2. **Open files**
   - What code is being reviewed or modified?
   - What's the cursor position (relevant context)?
   - What modules are involved?

3. **Recent changes**
   - Any uncommitted work (git diff)?
   - What has been modified?
   - What scope is being worked on?

4. **Feature name**
   - **If provided**: Use the specified name
   - **If not**: Derive from conversation key phrases
     - "add sparse file support" → `sparse-file-support`
     - "refactor buffer management" → `buffer-management-refactor`
     - "fix race condition in metadata" → `metadata-race-fix`

5. **Design content**
   - Extract problem statement from discussion
   - Capture proposed solutions
   - Note alternatives considered
   - Identify complexity indicators
   - List open questions

**Output**: `docs/designs/FEATURE_NAME.md`

If context is unclear, the agent will:
- Ask clarifying questions about the feature
- Request more details about the problem/solution
- Suggest discussing the design approach first

## Design Document Structure

Generate a comprehensive design document with the following sections:

```markdown
# Design: [Feature Name]

**Status**: Draft | In Review | Approved | Implemented
**Author**: [Inferred from conversation or git user]
**Created**: [Date]
**Last Updated**: [Date]

## Overview

[1-2 paragraph summary of what this design addresses and why it matters]

## Problem Statement

### Current Situation
[Describe the current state and limitations]

### Challenges
- [Challenge 1]
- [Challenge 2]
- [Challenge 3]

### Goals
- [Goal 1: What we want to achieve]
- [Goal 2: Success criteria]
- [Goal 3: Performance/quality targets]

### Non-Goals
- [Explicitly out of scope items]

## Proposed Solution

### High-Level Approach
[Describe the overall approach and strategy]

### Architecture

[Describe the architecture, components, and their relationships]
[Include ASCII diagrams if helpful]

```
┌─────────────┐     ┌─────────────┐
│  Component  │────▶│  Component  │
│      A      │     │      B      │
└─────────────┘     └─────────────┘
```

### Key Components

#### Component 1: [Name]
- **Purpose**: [What it does]
- **Location**: `src/module.rs`
- **Key Types**: `StructName`, `EnumName`
- **Responsibilities**: [What it's responsible for]

#### Component 2: [Name]
[Same structure]

### Data Structures

```rust
// Key data structures with documentation
pub struct NewStructure {
    field1: Type,
    field2: Type,
}
```

### Algorithms

[Describe key algorithms, their complexity, and trade-offs]

## API Design

### Public API

```rust
// New public functions/methods
pub fn new_function(param: Type) -> Result<ReturnType, Error> {
    // Purpose and behavior
}
```

### Internal API

[Internal functions and their contracts]

### CLI Changes (if applicable)

```bash
# New CLI options or changes
arsync --new-option value SOURCE DEST
```

## Implementation Details

### File Changes

| File | Changes | Complexity |
|------|---------|------------|
| `src/module1.rs` | Add new struct, implement trait | Medium |
| `src/module2.rs` | Refactor existing function | Low |
| `tests/test_module.rs` | Add new test cases | Medium |

### Dependencies

- New crates required: `crate-name = "version"`
- Modified dependencies: [List any version bumps]
- Why: [Justification for new dependencies]

### Complexity Assessment

**Overall Complexity**: Simple | Medium | Complex

**Breakdown**:
- **Scope**: [How many files/modules affected]
- **Dependencies**: [Number of external dependencies]
- **Testing**: [Test complexity]
- **Risk**: [Technical risk level]

**Estimated Phases**: [1-2 | 3-4 | 4-6]

## Testing Strategy

### Unit Tests
- Test [specific functionality]
- Test error cases: [list error cases]
- Test edge cases: [boundary conditions]
- Coverage target: [percentage]

### Integration Tests
- Test [end-to-end scenarios]
- Test [integration points]

### Performance Tests
- Benchmark [operations]
- Compare with [baseline]
- Targets: [specific metrics]

### Test Files
- `tests/test_feature.rs` - Main test file
- `tests/integration_feature.rs` - Integration tests

## Performance Considerations

### Expected Impact
- CPU: [expected impact]
- Memory: [expected impact]
- I/O: [expected impact]
- Latency: [expected impact]

### Optimizations
- [Optimization 1 and trade-offs]
- [Optimization 2 and trade-offs]

### Benchmarks
- [What will be benchmarked]
- [Performance targets]

## Security Considerations

### Threat Model
- [Potential security concerns]
- [Attack vectors]

### Mitigations
- [Security measure 1]
- [Security measure 2]

### Unsafe Code
- [Any unsafe blocks needed and why]
- [Safety invariants]

## Error Handling

### New Error Types
```rust
pub enum NewError {
    Case1(String),
    Case2(io::Error),
}
```

### Error Propagation
- [How errors propagate through the system]
- [User-facing error messages]

## Migration & Compatibility

### Breaking Changes
- [List any breaking changes]
- [Migration path for users]

### Backward Compatibility
- [How to maintain compatibility]
- [Deprecation strategy if needed]

### Configuration Changes
- [New config options]
- [Default values]

## Rollout Plan

1. **Phase 1**: [Initial implementation]
2. **Phase 2**: [Feature completion]
3. **Phase 3**: [Testing and refinement]
4. **Phase 4**: [Release]

## Alternatives Considered

### Alternative 1: [Name]
- **Approach**: [Description]
- **Pros**: [Benefits]
- **Cons**: [Drawbacks]
- **Why not chosen**: [Reason]

### Alternative 2: [Name]
[Same structure]

## Open Questions

- [ ] Question 1?
- [ ] Question 2?
- [ ] Question 3?

## References

- [Related designs or docs]
- [External documentation]
- [RFCs or standards]
- [Research papers if applicable]

## Acceptance Criteria

- [ ] All unit tests pass
- [ ] Integration tests cover key scenarios
- [ ] Performance benchmarks meet targets
- [ ] Documentation updated
- [ ] No clippy warnings
- [ ] Code review approved
- [ ] [Feature-specific criteria]

## Future Work

- [Potential enhancements]
- [Follow-up features]
- [Technical debt to address later]

---

**Next Steps**:
1. Review this design with team
2. Address open questions
3. Update based on feedback
4. Create implementation plan: `/plan @docs/designs/THIS_FILE.md`
5. Execute the plan: `/implement @docs/implementation-plans/THIS_FILE.md`
```

## Output Process

When invoked, the agent should:

1. **Analyze all available context**
   - **Conversation**: What problem/idea/solution is being discussed?
   - **Open files**: What modules are relevant?
   - **Git diff**: Any work in progress?
   - **Cursor position**: Specific code being looked at?
   - Extract key concepts, constraints, trade-offs

2. **Determine feature name**
   - **If specified explicitly**: Use provided name
   - **If not specified**: Derive from conversation
     - Look for key phrases: "add X", "fix Y", "refactor Z"
     - Extract feature name from problem statement
     - Convert to kebab-case: "Sparse File Support" → `sparse-file-support`
   - **If ambiguous**: Ask for clarification or suggest a name

3. **Extract design information**
   - **Problem**: What needs to be solved?
   - **Solution**: What approach was discussed?
   - **Alternatives**: What other approaches were considered?
   - **Constraints**: Performance, compatibility, complexity
   - **Open questions**: What's still unclear?
   - **Acceptance criteria**: What defines success?

4. **Create design document**
   - Fill in all sections based on extracted information
   - Be specific where details were discussed
   - Note uncertainties and open questions
   - Include code examples if discussed
   - Auto-determine complexity
   - Mark status as "Draft"

5. **Save to file**
   - Path: `docs/projects/PROJECT_NAME/design.md`
   - Create `docs/projects/PROJECT_NAME/` if needed
   - Use kebab-case for project directory name
   - Display path to user

6. **Suggest next steps**
   - Mention creating implementation plan: `/plan` (auto-finds design in project folder)
   - Note any open questions needing discussion
   - Suggest code review if relevant
   - Recommend who to discuss with if complex

## Integration with /plan

After creating a design doc:

```bash
# Create the design
/design "sparse-file-support"
# Output: docs/projects/sparse-file-support/design.md

# Create implementation plan (auto-finds design in project folder)
/plan
# Output: docs/projects/sparse-file-support/plan.md
# Or explicitly: /plan @docs/projects/sparse-file-support/design.md
```

## Example Usage Scenarios

### Scenario 1: From conversation (auto-infer everything)
```bash
# User: "I want to add support for sparse files to optimize disk usage"
# [Discussion about approach, trade-offs, etc.]
# Agent has enough context to extract problem and solution
/design
# Infers: Project name "sparse-file-support"
# Creates: docs/projects/sparse-file-support/design.md
```

### Scenario 2: From conversation with explicit name
```bash
# User: "We should refactor the buffer management to be adaptive"
# [Discussion about current issues and proposed solution]
/design "adaptive-buffer-management"
# Creates: docs/projects/adaptive-buffer-management/design.md
```

### Scenario 3: Bug fix discussion
```bash
# User: "There's a race condition in metadata handling that causes corruption"
# [Analysis of root cause and solution approach]
/design
# Infers: Project "metadata-race-fix"
# Creates: docs/projects/metadata-race-fix/design.md
```

### Scenario 4: Open files provide context
```bash
# User has src/copy.rs open
# Discussion about improving performance with zero-copy
/design
# Infers: Project from discussion and open file context
# Creates: docs/projects/zero-copy-optimization/design.md
```

### Scenario 5: Idea without implementation discussion
```bash
# User: "I have an idea to add compression support"
# Agent: "Tell me more about the use case and approach"
# [Brief discussion]
/design
# Creates: docs/projects/compression-support/design.md
# May have more open questions, less implementation detail
```

## Quality Checks

The design doc should be:

- **Complete**: All sections filled with relevant information
- **Specific**: Concrete details, not vague descriptions
- **Realistic**: Honest about complexity and trade-offs
- **Actionable**: Clear enough to implement from
- **Reviewed**: Note any areas needing more discussion

## Notes

- Design docs are living documents - they can be updated as understanding evolves
- Start with "Draft" status, move to "In Review" when ready
- Mark "Approved" after consensus, "Implemented" when done
- Include yourself in open questions if uncertain
- Link to related designs if building on previous work
- Be honest about complexity and risks

