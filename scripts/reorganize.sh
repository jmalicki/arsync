#!/bin/bash
#
# Codebase Reorganization Script
# 
# This script reorganizes the io_uring_sync repository to make README.md more prominent
# by moving documentation and scripts to appropriate subdirectories.
#
# Safety: Uses 'git mv' to preserve file history. Can be rolled back with 'git reset --hard'
#

set -e  # Exit on error

echo "=========================================="
echo "Codebase Reorganization Script"
echo "=========================================="
echo ""
echo "This script will:"
echo "  • Create new documentation subdirectories"
echo "  • Move 9 documentation files to docs/"
echo "  • Move 4 PR archive files to docs/pr-archive/"
echo "  • Move 3 scripts to scripts/ (with renaming)"
echo "  • Remove build artifacts"
echo "  • Update documentation links"
echo ""
echo "All file moves use 'git mv' to preserve history."
echo ""
read -p "Continue? (y/n) " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "Aborted."
    exit 1
fi

echo ""
echo "================================================"
echo "Phase 1: Creating directory structure"
echo "================================================"

mkdir -p docs/development
mkdir -p docs/features
mkdir -p docs/implementation
mkdir -p docs/pr-archive

echo "✓ Created docs/development/"
echo "✓ Created docs/features/"
echo "✓ Created docs/implementation/"
echo "✓ Created docs/pr-archive/"

echo ""
echo "================================================"
echo "Phase 2: Moving documentation files"
echo "================================================"

# Development documentation
echo "Moving development documentation..."
git mv CODEBASE_ANALYSIS.md docs/development/
git mv KNOWN_BUGS.md docs/development/
git mv REFACTORING_SUMMARY.md docs/development/
git mv REMAINING_IMPROVEMENTS.md docs/development/
echo "✓ Moved 4 files to docs/development/"

# Feature documentation
echo "Moving feature documentation..."
git mv PIRATE_FEATURE_SUMMARY.md docs/features/
echo "✓ Moved 1 file to docs/features/"

# Implementation documentation
echo "Moving implementation documentation..."
git mv ADAPTIVE_CONCURRENCY_IMPLEMENTATION.md docs/implementation/
git mv BENCHMARK_IMPLEMENTATION_PLAN.md docs/implementation/
git mv CI_OPTIMIZATION_ANALYSIS.md docs/implementation/
git mv IMPLEMENTATION_PLAN.md docs/implementation/
echo "✓ Moved 4 files to docs/implementation/"

# PR archive
echo "Moving PR archive files..."
git mv PR_DESCRIPTION.md docs/pr-archive/
git mv PR_SUMMARY.md docs/pr-archive/
git mv pr_data.json docs/pr-archive/
git mv pr_url.txt docs/pr-archive/
echo "✓ Moved 4 files to docs/pr-archive/"

echo ""
echo "================================================"
echo "Phase 3: Moving scripts"
echo "================================================"

git mv optimize-ci.sh scripts/optimize-ci.sh
echo "✓ Moved optimize-ci.sh to scripts/"

git mv RESTART_BENCHMARK_CLEAN.sh scripts/restart-benchmark-clean.sh
echo "✓ Moved and renamed RESTART_BENCHMARK_CLEAN.sh → restart-benchmark-clean.sh"

git mv RUN_BENCHMARK_NOW.sh scripts/run-benchmark-now.sh
echo "✓ Moved and renamed RUN_BENCHMARK_NOW.sh → run-benchmark-now.sh"

echo ""
echo "================================================"
echo "Phase 4: Removing build artifacts"
echo "================================================"

# Check if files exist before trying to remove
if [ -f benchmark_run.log ]; then
    rm -f benchmark_run.log
    echo "✓ Removed benchmark_run.log"
else
    echo "⊘ benchmark_run.log not found (already removed)"
fi

if [ -f smoke_test.log ]; then
    rm -f smoke_test.log
    echo "✓ Removed smoke_test.log"
else
    echo "⊘ smoke_test.log not found (already removed)"
fi

if [ -d benchmark-results-quick-20251008_172738 ]; then
    rm -rf benchmark-results-quick-20251008_172738
    echo "✓ Removed benchmark-results-quick-20251008_172738/"
else
    echo "⊘ benchmark-results-quick-20251008_172738/ not found (already removed)"
fi

echo ""
echo "================================================"
echo "Phase 5: Updating documentation links"
echo "================================================"

# Fix README.md broken link
echo "Updating README.md..."
sed -i 's|docs/IMPLEMENTATION_PLAN\.md|docs/implementation/IMPLEMENTATION_PLAN.md|g' README.md
# Remove broken TESTING_STRATEGY.md link
sed -i 's| • \[Testing Strategy\](docs/TESTING_STRATEGY\.md)||g' README.md
echo "✓ Fixed README.md links"

# Fix pirate README
echo "Updating docs/pirate/README.pirate.md..."
sed -i 's|docs/IMPLEMENTATION_PLAN\.md|docs/implementation/IMPLEMENTATION_PLAN.md|g' docs/pirate/README.pirate.md
sed -i 's| • \[Testing Strategy\](docs/TESTING_STRATEGY\.md)||g' docs/pirate/README.pirate.md
echo "✓ Fixed pirate README links"

# Update project documentation
echo "Updating project documentation..."
sed -i 's|docs/IMPLEMENTATION_PLAN\.md|docs/implementation/IMPLEMENTATION_PLAN.md|g' docs/projects/main-arsync/phase-3-2-status.md
sed -i 's|IMPLEMENTATION_PLAN\.md|docs/implementation/IMPLEMENTATION_PLAN.md|g' docs/projects/main-arsync/phase-3-1-summary.md
sed -i 's|../IMPLEMENTATION_PLAN\.md|../implementation/IMPLEMENTATION_PLAN.md|g' docs/projects/semaphore/design.md
echo "✓ Updated project documentation links"

echo ""
echo "================================================"
echo "Phase 6: Updating .gitignore"
echo "================================================"

# Check if patterns are already in .gitignore
if ! grep -q "^/benchmark-results-\*/$" .gitignore; then
    echo "" >> .gitignore
    echo "# Timestamped benchmark results" >> .gitignore
    echo "/benchmark-results-*/" >> .gitignore
    echo "✓ Added /benchmark-results-*/ to .gitignore"
else
    echo "⊘ .gitignore already contains benchmark-results pattern"
fi

echo ""
echo "================================================"
echo "Phase 7: Creating documentation index"
echo "================================================"

cat > docs/README.md << 'EOF'
# Documentation Index

Welcome to the arsync documentation! This guide helps you find the right documentation for your needs.

## 📖 Getting Started

- [Main README](../README.md) - Project overview, feature comparison, and quick start
- [Developer Guide](DEVELOPER.md) - Development setup, coding standards, and contribution guidelines
- [Benchmark Quick Start](BENCHMARK_QUICK_START.md) - How to run performance benchmarks
- [Changelog](CHANGELOG.md) - Version history and release notes

## 🔧 Development

Documentation for contributors and maintainers:

- [Codebase Analysis](development/CODEBASE_ANALYSIS.md) - Architecture overview and code structure
- [Known Bugs](development/KNOWN_BUGS.md) - Current issues and workarounds
- [Refactoring Summary](development/REFACTORING_SUMMARY.md) - History of major refactorings
- [Remaining Improvements](development/REMAINING_IMPROVEMENTS.md) - Future work and roadmap

## 📋 Implementation Details

Technical implementation plans and designs:

- [Implementation Plan](implementation/IMPLEMENTATION_PLAN.md) - Overall project implementation roadmap
- [Adaptive Concurrency](implementation/ADAPTIVE_CONCURRENCY_IMPLEMENTATION.md) - Dynamic concurrency tuning design
- [Benchmark Implementation](implementation/BENCHMARK_IMPLEMENTATION_PLAN.md) - Benchmark suite design
- [CI Optimization](implementation/CI_OPTIMIZATION_ANALYSIS.md) - Continuous integration optimization

## ✨ Features

Feature-specific documentation:

- [Pirate Translation](features/PIRATE_FEATURE_SUMMARY.md) - Internationalization and pirate mode
- [Pirate Translation Guide](PIRATE_TRANSLATION.md) - How the pirate translation works
- [Pirate Art Prompts](PIRATE_ART_PROMPTS.md) - Prompts used to generate pirate artwork

## 🏴‍☠️ Pirate Edition

Arrr! Documentation in pirate speak:

- [Pirate README](pirate/) - Full documentation translated to pirate speak

## 🔬 Technical Deep Dives

In-depth technical documentation:

- [NVMe Architecture](NVME_ARCHITECTURE.md) - Why NVMe needs io_uring
- [rsync Comparison](RSYNC_COMPARISON.md) - Detailed feature comparison
- [Industry Standards](INDUSTRY_STANDARDS.md) - Standards and best practices
- [Linux Kernel Contributions](LINUX_KERNEL_CONTRIBUTIONS.md) - Upstream contribution guidelines
- [Power Measurement](POWER_MEASUREMENT.md) - Energy efficiency benchmarking
- [Documentation Standards](DOCUMENTATION_STANDARDS.md) - How we write documentation

## 📊 Historical Records

Past pull requests and development history:

- [PR Archive](pr-archive/) - Historical pull request documentation and metadata

## 🤝 Contributing

Want to contribute? Start here:

1. Read the [Developer Guide](DEVELOPER.md)
2. Check [Known Bugs](development/KNOWN_BUGS.md) and [Remaining Improvements](development/REMAINING_IMPROVEMENTS.md)
3. Review the [Implementation Plan](implementation/IMPLEMENTATION_PLAN.md)
4. Follow our [Documentation Standards](DOCUMENTATION_STANDARDS.md)

## 📚 Projects

Detailed project-specific documentation:

- [projects/](projects/) - Per-project design documents and status updates

---

**Note:** This documentation is organized to help you find information quickly. If you're lost, start with the [Main README](../README.md) or [Developer Guide](DEVELOPER.md).
EOF

echo "✓ Created docs/README.md index"

echo ""
echo "================================================"
echo "✨ Reorganization Complete! ✨"
echo "================================================"
echo ""
echo "Summary:"
echo "  • 9 documentation files moved to docs/"
echo "  • 4 PR archive files moved to docs/pr-archive/"
echo "  • 3 scripts moved to scripts/"
echo "  • Build artifacts removed"
echo "  • Documentation links updated"
echo "  • Created docs/README.md index"
echo ""
echo "Root directory reduced from 24 files to 8 files! 🎉"
echo ""
echo "Next steps:"
echo "  1. Review changes: git status"
echo "  2. Test links: Open README.md and click links"
echo "  3. Run tests: cargo test"
echo "  4. Commit: git commit -m 'refactor: reorganize codebase for cleaner root directory'"
echo ""
echo "To rollback: git reset --hard HEAD"
echo ""

