# Documentation Index

This directory contains all project documentation organized by purpose.

## Directory Structure

### 📐 `designs/`
**Design documents and architectural decisions**

Technical design documents explaining how and why features are built:
- `semaphore-design.md` - Semaphore-based concurrency control
- `compio-fs-extended-plan.md` - compio-fs-extended architecture
- `fadvise-vs-o-direct.md` - I/O optimization approach
- `metadata-preservation.md` - Metadata handling design
- `research.md` - Research notes and investigations

Use `/design` to create new design documents from conversations.

### 📋 `implementation-plans/`
**Phased implementation plans with checkboxes**

Execution plans for features and projects:
- `main-implementation-plan.md` - Original arsync implementation
- `benchmarking-plan.md` - Benchmarking infrastructure
- `testing-strategy.md` - Testing approach
- `testing-priorities.md` - Testing priorities
- `phase-3-1-summary.md` - Phase 3.1 completion summary
- `phase-3-2-status.md` - Phase 3.2 current status

Use `/plan` to create new implementation plans.
Use `/implement` to execute plans step-by-step.

### 🔍 `analysis/`
**Post-mortem analyses and investigations**

Bug investigations, performance analyses, and technical deep-dives:
- `compio-metadata-bug-analysis.md` - Bug investigation
- `compio-metadata-bug-verdict.md` - Conclusion and resolution
- `github-actions-improvements.md` - CI/CD analysis

Use `/debug` when investigating issues - results may become analysis docs.

### 📦 `projects/`
**Project-specific documentation**

Subdirectories for specific projects and initiatives:
- `ci/` - CI/CD improvements
- `cli-refactor/` - CLI refactoring analysis

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
   # Creates: docs/designs/feature-name.md
   ```

2. **Plan implementation**:
   ```bash
   /plan @docs/designs/feature-name.md
   # Creates: docs/implementation-plans/feature-name.md
   ```

3. **Execute the plan**:
   ```bash
   /implement @docs/implementation-plans/feature-name.md
   ```

4. **If issues arise**:
   ```bash
   /debug "issue description"
   # May result in: docs/analysis/issue-name.md
   ```

### Document Lifecycle

```
Idea/Discussion
    ↓
docs/designs/feature.md         ← /design
    ↓
docs/implementation-plans/      ← /plan
feature.md
    ↓
Implementation                   ← /implement
    ↓
(if issues)
docs/analysis/investigation.md  ← /debug
```

## Naming Conventions

- **Designs**: `feature-name.md` (kebab-case)
- **Plans**: `feature-name.md` (matches design doc name)
- **Analysis**: `issue-name.md` or `feature-analysis.md`
- **User docs**: Clear, descriptive names (any case)

## See Also

- `.cursor/commands/README.md` - All available slash commands
- `DEVELOPER.md` - Development workflow guide
- `DOCUMENTATION_STANDARDS.md` - Writing standards

