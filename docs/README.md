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

## 🛡️ Safety & Security

**✅ VERIFIED SAFE**: Compio handles cancellation correctly (verified via source code review)

Response to [Tonbo.io "Async Rust is Not Safe with io_uring"](https://tonbo.io/blog/async-rust-is-not-safe-with-io-uring):

- **⭐ [Safety Analysis](safety/)** - Complete io_uring safety verification
  - [README.md](safety/README.md) - Complete answer (start here!)
  - [compio-verification.md](safety/compio-verification.md) - Source code proof
  - [diagrams.md](safety/diagrams.md) - Visual diagrams (Mermaid)
  - [quick-reference.md](safety/quick-reference.md) - Developer patterns

**TL;DR**: The criticism is valid for naive implementations. Compio is safe via heap allocation + manual reference counting (verified by source review). Our code uses safe patterns. Production-ready.

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
