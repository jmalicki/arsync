# /architect

Create a high-level architecture design for complex features that need to be broken down into multiple sub-projects.

**Use this for**: Large, complex features that span multiple components or require phased delivery of independent sub-features.

- feature_name (string, optional): Override the inferred feature name

```bash
/architect                              # Infer from conversation
/architect "distributed-sync-protocol"  # Explicit name
```

## When to Use /architect vs /design

**Use `/architect` when:**
- Feature affects multiple modules/components
- Needs to be broken into independent sub-projects
- Each sub-project could be a separate PR
- Requires coordinated multi-phase delivery
- Complexity is very high (6+ implementation phases if done as single project)

**Use `/design` when:**
- Feature is self-contained
- Single component/module
- Can be implemented in one coherent effort
- 1-6 phases sufficient

**Example: Use `/architect` for:**
- Distributed sync protocol (networking + storage + coordination + security)
- Plugin system (API + loader + sandboxing + registry)
- Complete refactor of core system

**Example: Use `/design` for:**
- Sparse file support (single feature)
- Adaptive buffer management (one component)
- Bug fixes or incremental features

## Architecture Document Structure

```markdown
# Architecture: [Feature Name]

**Status**: Draft | In Review | Approved | In Progress | Complete
**Author**: [Inferred from git user]
**Created**: [Date]
**Last Updated**: [Date]
**Branch**: [Current git branch]
**Complexity**: Very High

## Overview

[High-level description of the complex feature and why it needs architectural breakdown]

## Problem Statement

### Current Situation
[What exists today]

### Why This Needs Architecture-Level Planning
- Spans multiple components/modules
- Requires coordinated delivery
- Independent sub-features can be delivered separately
- High complexity requires decomposition

### Goals
[High-level goals for the overall feature]

### Non-Goals
[What's explicitly out of scope]

## High-Level Architecture

[System-level architecture showing major components and their interactions]

```
┌─────────────────┐      ┌─────────────────┐
│   Component A   │─────▶│   Component B   │
│  (sub-project)  │      │  (sub-project)  │
└─────────────────┘      └─────────────────┘
         │                        │
         ▼                        ▼
┌─────────────────┐      ┌─────────────────┐
│   Component C   │◀─────│   Component D   │
│  (sub-project)  │      │  (sub-project)  │
└─────────────────┘      └─────────────────┘
```

## Sub-Projects

This feature breaks down into the following independent sub-projects:

### Sub-Project 1: [Component Name]

**Purpose**: [What this component does]
**Dependencies**: [What it depends on]
**Priority**: Critical | High | Medium | Low
**Estimated Complexity**: Simple | Medium | Complex
**Directory**: `docs/projects/MAIN_PROJECT/projects/sub-project-1/`

**Scope**:
- [Key responsibility 1]
- [Key responsibility 2]

**Deliverables**:
- Design document
- Implementation plan
- Working implementation
- Tests

### Sub-Project 2: [Component Name]

[Same structure]

### Sub-Project 3: [Component Name]

[Same structure]

## Dependencies Between Sub-Projects

**Dependency Graph**:
```
Sub-Project 1 (foundation)
    ↓
Sub-Project 2 (builds on 1)
    ↓
Sub-Project 3 (integrates 1 & 2)
    ↓
Sub-Project 4 (final integration)
```

**Execution Order**:
1. Sub-Project 1 must complete first (foundation)
2. Sub-Projects 2 & 3 can be parallel (independent)
3. Sub-Project 4 depends on all (integration)

## Integration Strategy

[How the sub-projects come together]

## Implementation Approach

This architecture will be implemented using nested project structure:

```
docs/projects/FEATURE_NAME/
  ├── architecture.md          # This document
  ├── plan.md                  # High-level orchestration plan
  └── projects/
      ├── sub-project-1/
      │   ├── design.md        # Created with /design
      │   ├── plan.md          # Created with /plan
      │   └── ...
      ├── sub-project-2/
      │   ├── design.md
      │   ├── plan.md
      │   └── ...
      └── sub-project-3/
          ├── design.md
          └── ...
```

## Acceptance Criteria

- [ ] All sub-projects have design documents
- [ ] All sub-projects have implementation plans
- [ ] All sub-projects implemented and tested
- [ ] Integration tests pass
- [ ] Performance targets met
- [ ] Documentation complete
- [ ] Security review passed

## Risks

[High-level risks and mitigation strategies]

---

**Next Steps**:
1. Review this architecture design
2. Get feedback from stakeholders
3. Create orchestration plan: `/plan`
4. Execute sub-projects one by one
```

## High-Level Orchestration Plan

When `/plan` is run after `/architect`, it should create a special orchestration plan:

```markdown
# Implementation Plan: [Architecture Name] - Orchestration

**Type**: Architecture Orchestration
**Sub-Projects**: [Number]
**Overall Complexity**: Very High

## Phase 1: Setup & Foundation

### Steps
- [ ] Review architecture document
- [ ] Create project directory structure
- [ ] Set up integration test framework
- [ ] Define cross-component contracts/interfaces

### Quality Checks
- [ ] Architecture design approved
- [ ] All stakeholders aligned

## Phase 2: Sub-Project 1 - [Component Name]

### Steps
- [ ] Create sub-project directory: `docs/projects/MAIN/projects/sub-1/`
- [ ] Create design: `/design "sub-project-1"` in context of sub-1
- [ ] Review design for sub-project-1
- [ ] Create plan: `/plan` for sub-project-1
- [ ] Create implementation branch for sub-1
- [ ] Implement: `/implement` for sub-project-1
- [ ] Test sub-project-1: `/test "module"`
- [ ] Create PR for sub-project-1
- [ ] Merge sub-project-1

### Quality Checks
- [ ] Sub-project-1 design approved
- [ ] Sub-project-1 tests pass
- [ ] Sub-project-1 merged to main

## Phase 3: Sub-Project 2 - [Component Name]

[Same structure as Phase 2]

## Phase 4: Sub-Project 3 - [Component Name]

[Same structure]

## Phase 5: Integration

### Steps
- [ ] Integrate all sub-projects
- [ ] Write integration tests
- [ ] Performance testing across all components
- [ ] End-to-end testing
- [ ] Documentation for complete feature

### Quality Checks
- [ ] /test "all"
- [ ] /bench true false
- [ ] All integration tests pass
- [ ] Performance targets met

## Phase 6: Final PR

[Standard PR preparation steps]
```

## Output Process

When invoked, the agent should:

1. **Analyze complexity**
   - Determine if this truly needs architecture-level planning
   - If not complex enough, suggest `/design` instead
   - Identify major components/sub-systems

2. **Identify sub-projects**
   - Break down into logical, independent components
   - Each sub-project should be deliverable independently
   - Identify dependencies between sub-projects

3. **Create architecture document**
   - Path: `docs/projects/FEATURE_NAME/architecture.md`
   - High-level system design
   - Sub-project breakdown
   - Dependency graph
   - Integration strategy

4. **Suggest orchestration**
   - Recommend running `/plan` to create orchestration plan
   - Plan will guide creation of each sub-project's design/plan
   - Each sub-project gets its own directory under `projects/`

## Example Usage

### Scenario 1: Distributed sync protocol
```bash
# Complex multi-component feature
/branch "distributed-sync/architecture" main origin true

/architect "distributed-sync-protocol"
# Creates: docs/projects/distributed-sync-protocol/architecture.md
# Identifies sub-projects:
#   - networking-protocol
#   - state-synchronization
#   - conflict-resolution
#   - security-layer

/commit "docs(distributed-sync): add architecture design"

/plan
# Creates: docs/projects/distributed-sync-protocol/plan.md
# Plan includes steps to create design/plan for each sub-project

/commit "docs(distributed-sync): add orchestration plan"

/pr-ready "docs: distributed sync architecture and plan"
```

### Scenario 2: Plugin system
```bash
/branch "plugins/architecture" main origin true

/architect "plugin-system"
# Creates: docs/projects/plugin-system/architecture.md
# Sub-projects:
#   - plugin-api
#   - plugin-loader
#   - plugin-sandbox
#   - plugin-registry

/plan
# Orchestration plan with steps for each sub-project

/implement
# Execute orchestration plan:
# - Creates sub-project-1 design
# - Creates sub-project-1 plan
# - Implements sub-project-1
# - Moves to sub-project-2, etc.
```

## Integration with /implement

When executing an orchestration plan:

```bash
/implement
# Executing: Create design for networking-protocol sub-project

# Agent should:
# 1. Create: docs/projects/distributed-sync/projects/networking-protocol/
# 2. Create design in that sub-project
# 3. Check box in orchestration plan
# 4. Move to next step
```

## Directory Structure Created

```
docs/projects/complex-feature/
  ├── architecture.md          # Created by /architect
  ├── plan.md                  # Orchestration plan (created by /plan)
  └── projects/                # Sub-projects
      ├── sub-project-1/
      │   ├── design.md        # Created during /implement of orchestration
      │   ├── plan.md
      │   └── implementation files...
      ├── sub-project-2/
      │   ├── design.md
      │   └── ...
      └── sub-project-3/
          └── ...
```

## Best Practices

1. **Use sparingly** - Most features don't need this level of planning
2. **Clear boundaries** - Sub-projects should be truly independent
3. **Deliverable units** - Each sub-project should be shippable on its own
4. **Dependency order** - Plan execution order based on dependencies
5. **Integration points** - Clearly define how sub-projects integrate
6. **Incremental delivery** - Ship sub-projects as they complete

## Notes

- `/architect` is for very complex features only
- Creates nested project structure
- Orchestration plan guides creation of sub-project designs/plans
- Each sub-project follows normal `/design` → `/plan` → `/implement` workflow
- Integration phase brings everything together

