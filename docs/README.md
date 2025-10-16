# Documentation Index

This directory contains all project documentation organized **project-first** for better integration with Cursor slash commands.

## Directory Structure

### 📦 `projects/` - **All project documentation**

Each project contains its related design docs, plans, and analyses bundled together:

```
docs/projects/PROJECT_NAME/
  ├── design.md           # Design document (created with /design)
  ├── plan.md             # Implementation plan (created with /plan)
  ├── analysis-*.md       # Analysis documents
  ├── design-*.md         # Additional designs
  └── README.md           # Project overview
```

**Current Projects:**
- `main-arsync/` - Main project implementation and testing
- `benchmarking/` - Benchmarking infrastructure
- `cli-refactor/` - CLI architecture refactoring
- `ci/` - CI/CD improvements
- `compio-fs-extended/` - Advanced async filesystem library
- `compio-metadata-bug/` - Metadata bug investigation
- `semaphore/` - Semaphore concurrency control

See `projects/README.md` for detailed project listing.

### 🏴‍☠️ `pirate/`
**Pirate-themed documentation**

Fun pirate translations of user-facing docs.

## Root-Level Documents

### User-Facing
- `BENCHMARK_QUICK_START.md` - How to run benchmarks
- `CHANGELOG.md` - Version history and changes
- `RSYNC_COMPARISON.md` - Comparison with rsync

### Developer Reference
- `DEVELOPER.md` - Development guide and workflow
- `DOCUMENTATION_STANDARDS.md` - Doc writing standards
- `INDUSTRY_STANDARDS.md` - Industry best practices reference

### Technical Reference
- `NVME_ARCHITECTURE.md` - NVMe architecture reference
- `POWER_MEASUREMENT.md` - Power measurement guide
- `LINUX_KERNEL_CONTRIBUTIONS.md` - Kernel contribution guide

### Specialized
- `PIRATE_ART_PROMPTS.md` - AI art generation prompts
- `PIRATE_TRANSLATION.md` - Pirate translation guide

## Workflow Integration

### Creating New Documentation

1. **Design a feature**:
   ```bash
   /design "feature-name"
   # Creates: docs/projects/feature-name/design.md
   ```

2. **Plan implementation**:
   ```bash
   /plan
   # Auto-finds design in project folder
   # Creates: docs/projects/feature-name/plan.md
   ```

3. **Execute the plan**:
   ```bash
   /implement
   # Auto-finds plan in project folder
   # Executes: docs/projects/feature-name/plan.md
   ```

4. **If issues arise**:
   ```bash
   /debug "issue description"
   # May create: docs/projects/feature-name/analysis-issue.md
   ```

### Document Lifecycle

```
Idea/Discussion
    ↓
docs/projects/feature-name/design.md    ← /design
    ↓
docs/projects/feature-name/plan.md      ← /plan (auto-finds design.md)
    ↓
Implementation                           ← /implement (auto-finds plan.md)
    ↓
(if issues)
docs/projects/feature-name/analysis.md  ← /debug
```

## Naming Conventions

- **Projects**: `project-name/` directory (kebab-case)
- **Within projects**:
  - `design.md` - Main design document
  - `plan.md` - Main implementation plan
  - `analysis.md` or `analysis-TOPIC.md` - Analysis documents
  - `design-ASPECT.md` - Additional designs
  - `plan-ASPECT.md` - Additional plans
- **User docs**: Clear, descriptive names (any case)

## See Also

- `.cursor/commands/README.md` - All available slash commands
- `DEVELOPER.md` - Development workflow guide
- `DOCUMENTATION_STANDARDS.md` - Writing standards

