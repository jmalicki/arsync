# Design Documents

This directory contains design documents for features and significant changes to the project.

## Creating Design Documents

Use the `/design-doc` command to generate a design document from your current conversation:

```bash
/design-doc                    # Infer feature name from conversation
/design-doc "feature-name"     # Specify feature name explicitly
```

Design documents are saved as: `FEATURE_NAME.md` (kebab-case)

## Design Document Lifecycle

1. **Draft** - Initial design being written
2. **In Review** - Ready for feedback and discussion
3. **Approved** - Consensus reached, ready to implement
4. **Implemented** - Feature has been implemented

## Creating Implementation Plans

After creating a design doc, generate an implementation plan:

```bash
/implementation-plan @docs/designs/FEATURE_NAME.md
```

This creates: `docs/implementation-plans/FEATURE_NAME.md`

## Design Document Template

Each design doc includes:

- **Overview** - Problem and solution summary
- **Problem Statement** - Current situation, challenges, goals
- **Proposed Solution** - Architecture, components, algorithms
- **API Design** - Public/internal APIs, CLI changes
- **Implementation Details** - File changes, dependencies, complexity
- **Testing Strategy** - Unit, integration, performance tests
- **Performance Considerations** - Expected impact and optimizations
- **Security Considerations** - Threats and mitigations
- **Migration & Compatibility** - Breaking changes, backward compatibility
- **Alternatives Considered** - Other approaches and why not chosen
- **Acceptance Criteria** - Definition of done
- **Future Work** - Follow-up enhancements

## Best Practices

- Write designs before significant implementation work
- Include code examples and diagrams
- Be honest about complexity and trade-offs
- Document alternatives considered
- List open questions
- Update as understanding evolves
- Link to related designs

