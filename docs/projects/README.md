# Projects - Internal Development Documentation

This directory contains **internal development documentation only**. Each project folder bundles related design docs, implementation plans, and analyses.

**Important**: User-facing documentation stays in `docs/` root, NOT here.

- ✅ **Goes in projects/**: Design docs, implementation plans, analyses, internal investigations
- ❌ **Stays in docs/ root**: User guides, tutorials, API docs, CHANGELOG, README content

## Structure

Each project follows this pattern:
```
docs/projects/PROJECT_NAME/
  ├── design.md               # Design document (created with /design)
  ├── plan.md                 # Implementation plan (created with /plan)
  ├── analysis-*.md           # Analysis documents
  ├── design-*.md             # Additional designs
  ├── plan-*.md               # Additional plans
  └── README.md               # Project overview
```

## Current Projects

### `benchmarking/`
Benchmarking infrastructure and methodology
- `plan.md` - Benchmarking implementation plan

### `ci/`
Continuous Integration improvements
- `analysis-improvements.md` - GitHub Actions analysis
- `github-pages-setup.md` - GitHub Pages setup guide
- `README.md` - CI project overview

### `cli-refactor/`
CLI architecture refactoring
- `design.md` - Main refactoring design
- `design-examples.md` - Modular examples
- `design-positional-args.md` - Positional args enhancement
- `analysis-summary.md` - Analysis summary
- `analysis-architecture.md` - Architecture analysis
- `analysis-library-comparison.md` - Library comparison
- `plan-summary.md` - Implementation summary
- `README.md` - Project overview

### `compio-fs-extended/`
Advanced async filesystem operations library
- `design.md` - Library design and architecture

### `compio-metadata-bug/`
Investigation of metadata bug in compio
- `analysis.md` - Bug investigation
- `verdict.md` - Conclusion and resolution

### `main-arsync/`
Main arsync project implementation
- `plan.md` - Main implementation plan
- `testing-strategy.md` - Testing approach
- `testing-priorities.md` - Testing priorities  
- `phase-3-1-summary.md` - Phase 3.1 completion
- `phase-3-2-status.md` - Phase 3.2 status

### `semaphore/`
Semaphore-based concurrency control
- `design.md` - Semaphore design and architecture

## Workflow Integration

### Create a new project:
```bash
# Discuss the project idea
/design "project-name"
# Creates: docs/projects/project-name/design.md

# Create implementation plan
/plan @docs/projects/project-name/design.md  
# Creates: docs/projects/project-name/plan.md

# Execute the plan
/implement @docs/projects/project-name/plan.md
```

### Benefits of Project-First Structure:
1. **Auto-discovery** - Slash commands can find related docs in project folder
2. **Bundle related docs** - All docs for a project in one place
3. **Clear boundaries** - Easy to see project scope
4. **Simpler navigation** - Find everything about X in one folder
5. **Works with slash commands** - Commands can default to current project context

## Naming Conventions

- **Projects**: `kebab-case` directory names
- **Files**: Standard names within project
  - `design.md` - Main design
  - `plan.md` - Main implementation plan
  - `analysis.md` or `analysis-TOPIC.md` - Analyses
  - `design-ASPECT.md` - Additional designs
  - `plan-ASPECT.md` - Additional plans
  - `README.md` - Project overview

