# Trait-Based Filesystem Abstraction

A comprehensive design for adding a unified trait-based interface to arsync that works with both local filesystem operations and remote protocol operations.

## Quick Links

- **[Design Document](./design.md)** - Full design rationale, architecture, and decisions
- **[Implementation Plan](./plan.md)** - Step-by-step PR breakdown with code examples
- **Previous Branch**: `cursor/integrate-protocol-mod-with-compio-using-traits-4653` (reference, has compilation errors)

## Overview

This project adds a trait abstraction layer to unify local and remote filesystem operations in arsync. The goal is to:

1. **Enable code reuse** between local (compio-fs-extended) and remote (Transport) backends
2. **Provide clean abstractions** that are easy to use and extend
3. **Maintain performance** equivalent to direct compio usage
4. **Support incremental adoption** without breaking existing code

## Problem

Currently, arsync has separate code paths for:
- **Local operations**: Direct use of `compio::fs` and `compio_fs_extended`
- **Remote operations**: Protocol-based operations via `Transport` trait

This leads to:
- Code duplication
- Difficult to add new backends
- Hard to test in isolation
- No unified API for tools that work with both

## Solution

Add a hierarchy of traits:

```
AsyncMetadata        ← Foundation (no dependencies)
    ↓
AsyncFile           ← File I/O operations
    ↓
AsyncDirectory      ← Directory operations
    ↓
AsyncFileSystem     ← Full filesystem abstraction
    ↓
FileOperations      ← High-level operations
```

Each trait can be implemented for different backends:
- `LocalFileSystem` - uses compio-fs-extended
- `ProtocolFileSystem<T: Transport>` - uses Transport trait for remote ops

## Approach

**Incremental, Bottom-Up**:
1. Start with simplest trait (AsyncMetadata)
2. Add one trait at a time
3. Ensure each addition compiles and tests pass
4. Build up to complete system
5. Create stacked PRs for easy review

**Not**: Big-bang approach that adds everything at once (this failed before)

## Timeline

**Short term** (5-7 weeks):
- Phase 1-4: Add all traits (~2-3 weeks)
- Phase 5-7: Add backend implementations (~2-3 weeks)
- Phase 8: Initial integration (~1 week)

**Long term** (future):
- Phase 9-11: Complete migration and optimization

## Getting Started

### For Reviewers

1. Read the [Design Document](./design.md) for full context
2. Check the [Implementation Plan](./plan.md) for PR breakdown
3. Review PRs in order (each builds on previous)

### For Implementers

1. Start with Phase 1 (AsyncMetadata trait)
2. Follow the plan exactly - don't skip ahead
3. Ensure each PR compiles and passes tests before moving on
4. Reference the previous branch for trait implementations (with fixes)

## Key Design Decisions

1. **Two Complementary Abstractions**: AsyncFileSystem for local (random access), SyncProtocol for remote (batch)
2. **No Forced Batching**: Local streams (walk→copy), protocol batches (accumulate→send)
3. **Shared Components**: Tree walking, metadata comparison, preservation used by both
4. **DirectoryFd Everywhere**: Both backends use secure *at syscalls via DirectoryFd
5. **Protocol Reuses Local**: rsync uses local filesystem abstractions, doesn't duplicate I/O
6. **Associated Types** over generic parameters for cleaner API
7. **Buffer ownership** pattern matching compio (zero-copy)

## Files

- **`README.md`** - This file (overview and quick links)
- **`design.md`** - Full design document with architecture and rationale
- **`plan.md`** - Step-by-step implementation plan with 8 sequential PRs (checklist format)
- **`rsync-protocol-analysis.md`** - Why protocol ≠ filesystem, two abstractions needed
- **`streaming-vs-batch.md`** - How to support both patterns without forcing either
- **`layer-integration.md`** - How rsync uses local filesystem abstractions (DirectoryFd, etc.)
- **`implementation-requirements.md`** - ⚠️ **CRITICAL**: Security and performance requirements (must read before implementing)

## Status

**Current Phase**: Phase 0 (Design documentation)

**Current Branch**: `design/trait-based-filesystem-abstraction`

**Status**: ✅ Design complete, ready for review as PR #0

**Next Steps**:
1. **Push this branch** and create PR #0 (design docs only)
2. **Get design reviewed** and merged into main
3. **Then start Phase 1** (implementation begins)

## PR Sequence

```
PR #0: Design docs (THIS PR!)
  ↓
PR #1: AsyncMetadata trait
  ↓
PR #2: AsyncFile trait
  ↓
... (continue stacking)
```

## Questions?

See the [Design Document](./design.md) for detailed explanations of:
- Why each trait is structured the way it is
- How to handle common issues (lifetimes, Sized constraints, etc.)
- Migration strategy
- Testing approach
- Performance considerations

## Related Work

- **Previous attempt**: `cursor/integrate-protocol-mod-with-compio-using-traits-4653`
  - Added all traits at once
  - Had 54+ compilation errors
  - This design learns from those mistakes

- **Existing code**:
  - `src/metadata.rs` - metadata handling
  - `src/copy.rs` - file copying
  - `src/sync.rs` - directory syncing
  - These will be gradually migrated to use traits

## Success Criteria

✅ All traits compile without errors
✅ All tests pass (including new trait tests)
✅ Local filesystem backend works for all operations
✅ Performance equivalent to direct compio usage
✅ Code is cleaner and more maintainable
✅ Path to remote operations is clear
✅ Documentation is complete

## License

Same as arsync project

