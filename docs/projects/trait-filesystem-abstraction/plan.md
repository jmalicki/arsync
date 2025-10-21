# Trait-Based Filesystem Abstraction - Implementation Plan

## Overview

Incremental plan to add trait-based filesystem abstractions to arsync. Each phase is a single PR that compiles and passes tests.

**See [design.md](./design.md)** for architecture, rationale, and design decisions.

## Principles

1. **One trait per PR** (mostly)
2. **Each PR must compile and pass tests**
3. **Stacked PRs** - each builds on the previous
4. **Design first** - PR #0 (docs) merged before implementation
5. **Reference design docs** - don't deviate from approved design

## Phase 0: Design Documentation ✅

**Status**: COMPLETE (current branch)

**Goal**: Get design reviewed and approved before any implementation

**Branch**: `design/trait-based-filesystem-abstraction`

**PR Description**:
- Complete design for trait-based filesystem abstraction
- Three-layer architecture (see [design.md](./design.md))
- Analysis of rsync protocol compatibility (see [rsync-protocol-analysis.md](./rsync-protocol-analysis.md))
- Streaming vs batch strategies (see [streaming-vs-batch.md](./streaming-vs-batch.md))
- Layer integration with DirectoryFd (see [layer-integration.md](./layer-integration.md))

**Files**:
- `docs/projects/trait-filesystem-abstraction/*.md` (6 files)

**Success Criteria**:
- [ ] Design reviewed
- [ ] Architecture approved
- [ ] Team buy-in
- [ ] Merged to main

**Next**: Start Phase 1 after merge

---

## Phase 1: AsyncMetadata Trait

**Status**: Waiting for Phase 0 merge

**Goal**: Add simplest trait with no dependencies

**Base Branch**: `main` (after Phase 0 merged)
**Branch**: `feat/async-metadata-trait`

**Design Reference**: See [design.md](./design.md#phase-1-asyncmetadata)

**Implementation Tasks**:
- [ ] Create `src/traits/mod.rs`
- [ ] Create `src/traits/metadata.rs` with `AsyncMetadata` trait
- [ ] Add `pub mod traits;` to `src/lib.rs`
- [ ] Implement `AsyncMetadata` for existing `Metadata` type in `src/metadata.rs`
- [ ] Add tests for trait default methods
- [ ] Add integration test verifying `Metadata` implements trait

**Files Changed**:
- `src/lib.rs`
- `src/traits/mod.rs` (new)
- `src/traits/metadata.rs` (new)
- `src/metadata.rs`

**Estimated Time**: 4-6 hours

**Success Criteria**:
- [ ] Compiles without errors
- [ ] All existing tests pass
- [ ] New trait tests pass
- [ ] `Metadata` implements `AsyncMetadata` correctly

---

## Phase 2: AsyncFile Trait

**Status**: Waiting for Phase 1

**Goal**: Add file I/O trait that uses AsyncMetadata

**Base Branch**: `feat/async-metadata-trait`
**Branch**: `feat/async-file-trait`

**Design Reference**: See [design.md](./design.md#phase-2-asyncfile)

**Implementation Tasks**:
- [ ] Create `src/traits/file.rs` with `AsyncFile` trait
- [ ] Add `pub mod file;` to `src/traits/mod.rs`
- [ ] Add tests with mock implementation
- [ ] Verify trait bounds compile correctly
- [ ] Test provided methods (read_all, write_all_at)

**Files Changed**:
- `src/traits/mod.rs`
- `src/traits/file.rs` (new)

**Estimated Time**: 4-6 hours

**Success Criteria**:
- [ ] Compiles without errors
- [ ] All tests pass
- [ ] Mock implementation works
- [ ] Trait definition is clean

---

## Phase 3: AsyncDirectory Trait

**Status**: Waiting for Phase 2

**Goal**: Add directory trait with entry iteration

**Base Branch**: `feat/async-file-trait`
**Branch**: `feat/async-directory-trait`

**Design Reference**: See [design.md](./design.md#phase-3-asyncdirectory)

**Implementation Tasks**:
- [ ] Create `src/traits/directory.rs` with traits:
  - `AsyncDirectoryEntry`
  - `AsyncDirectory`
- [ ] Add `pub mod directory;` to `src/traits/mod.rs`
- [ ] Add tests with mock implementations
- [ ] Verify trait bounds compile

**Files Changed**:
- `src/traits/mod.rs`
- `src/traits/directory.rs` (new)

**Estimated Time**: 4-6 hours

**Success Criteria**:
- [ ] Compiles without errors
- [ ] All tests pass
- [ ] Mock implementations work

---

## Phase 4: AsyncFileSystem Trait

**Status**: Waiting for Phase 3

**Goal**: Add top-level filesystem trait

**Base Branch**: `feat/async-directory-trait`
**Branch**: `feat/async-filesystem-trait`

**Design Reference**: See [design.md](./design.md#phase-4-asyncfilesystem)

**Implementation Tasks**:
- [ ] Create `src/traits/filesystem.rs` with `AsyncFileSystem` trait
- [ ] Add `pub mod filesystem;` to `src/traits/mod.rs`
- [ ] Add tests with mock implementation
- [ ] Verify associated types work correctly

**Files Changed**:
- `src/traits/mod.rs`
- `src/traits/filesystem.rs` (new)

**Estimated Time**: 4-6 hours

**Success Criteria**:
- [ ] Compiles without errors
- [ ] All tests pass
- [ ] Associated types work

---

## Phase 5a: Shared Filesystem Operations

**Status**: Waiting for Phase 4

**Goal**: Extract shared operations (tree walking, metadata preservation, etc.)

**Base Branch**: `feat/async-filesystem-trait`
**Branch**: `feat/shared-filesystem-ops`

**Design Reference**: See [layer-integration.md](./layer-integration.md#shared-filesystem-operations-layer-2)

**Implementation Tasks**:
- [ ] Create `src/filesystem/` module
- [ ] Create `src/filesystem/walker.rs` - `SecureTreeWalker` using DirectoryFd
- [ ] Create `src/filesystem/read.rs` - `read_file_content()` with openat
- [ ] Create `src/filesystem/write.rs` - `write_file_content()` with openat
- [ ] Create `src/filesystem/metadata.rs` - `preserve_metadata()` with *at syscalls
- [ ] Add comprehensive tests

**Files Changed**:
- `src/lib.rs`
- `src/filesystem/mod.rs` (new)
- `src/filesystem/walker.rs` (new)
- `src/filesystem/read.rs` (new)
- `src/filesystem/write.rs` (new)
- `src/filesystem/metadata.rs` (new)

**Key Pattern**: All operations use DirectoryFd and secure *at syscalls

**Estimated Time**: 8-12 hours

**Success Criteria**:
- [ ] All operations use DirectoryFd
- [ ] TOCTOU-safe
- [ ] Comprehensive tests
- [ ] Used by both local and protocol backends

---

## Phase 5b: Local Backend Implementation

**Status**: Waiting for Phase 5a

**Goal**: Implement AsyncFileSystem traits for local filesystem

**Base Branch**: `feat/shared-filesystem-ops`
**Branch**: `feat/local-filesystem-backend`

**Design Reference**: See [design.md](./design.md#5a-local-filesystem-backend) and [layer-integration.md](./layer-integration.md#local-backend-direct-use)

**Implementation Tasks**:
- [ ] Create `src/backends/mod.rs`
- [ ] Create `src/backends/local.rs`:
  - `LocalFileSystem`
  - `LocalFile`
  - `LocalDirectory`
  - `LocalDirectoryEntry`
- [ ] Implement all trait methods
- [ ] **Use shared operations from Phase 5a**
- [ ] Add comprehensive integration tests
- [ ] Performance comparison with direct compio

**Files Changed**:
- `src/lib.rs`
- `src/backends/mod.rs` (new)
- `src/backends/local.rs` (new)

**Key Pattern**: Reuse shared operations, don't duplicate

**Estimated Time**: 8-12 hours

**Success Criteria**:
- [ ] All trait methods implemented
- [ ] Uses shared operations from `src/filesystem/`
- [ ] Uses DirectoryFd throughout
- [ ] Performance comparable to direct compio
- [ ] All tests pass

---

## Phase 6: SyncProtocol Trait + Rsync Backend

**Status**: Waiting for Phase 5b

**Goal**: Add protocol abstraction and rsync implementation

**Base Branch**: `feat/local-filesystem-backend`
**Branch**: `feat/rsync-protocol-backend`

**Design Reference**: See [rsync-protocol-analysis.md](./rsync-protocol-analysis.md#proposed-design-three-layer-architecture)

**Implementation Tasks**:
- [ ] Create `src/traits/sync_protocol.rs` with `SyncProtocol` trait
- [ ] Update `src/backends/protocol.rs`:
  - Implement `SyncProtocol` for rsync wire protocol
  - **Use shared operations from Phase 5a** for file I/O
  - Don't duplicate file operations
- [ ] Add tests with existing Transport implementations

**Files Changed**:
- `src/traits/mod.rs`
- `src/traits/sync_protocol.rs` (new)
- `src/backends/protocol.rs` (update)

**Key Pattern**: Protocol uses shared operations for file I/O, only adds protocol-specific logic

**Estimated Time**: 8-12 hours

**Success Criteria**:
- [ ] `SyncProtocol` trait defined
- [ ] Rsync backend uses shared operations
- [ ] No duplicate file I/O code
- [ ] Works with existing Transport
- [ ] Tests pass

---

## Phase 7: High-Level Sync Operations

**Status**: Waiting for Phase 6

**Goal**: Add unified sync operations that work with any backend

**Base Branch**: `feat/rsync-protocol-backend`
**Branch**: `feat/high-level-sync-ops`

**Design Reference**: See [streaming-vs-batch.md](./streaming-vs-batch.md#the-unified-api-backend-decides)

**Implementation Tasks**:
- [ ] Create `src/sync/engine.rs`:
  - `SyncEngine` that orchestrates backends
  - Common logic: tree walking, metadata comparison
  - Backend dispatch: streaming for local, batching for protocol
- [ ] Add progress reporting
- [ ] Add comprehensive tests with both backends

**Files Changed**:
- `src/sync/engine.rs` (new)
- `src/sync/mod.rs` (update)

**Key Pattern**: Shared logic with backend-specific execution

**Estimated Time**: 8-12 hours

**Success Criteria**:
- [ ] Works with both local and protocol backends
- [ ] Streaming for local (no forced batching)
- [ ] Batching for protocol (where needed)
- [ ] Common logic reused
- [ ] Tests pass with both backends

---

## Phase 8: Integration with Existing Code

**Status**: Waiting for Phase 7

**Goal**: Gradually migrate existing code to use new traits

**Base Branch**: `feat/high-level-sync-ops`
**Branch**: `refactor/migrate-to-traits`

**Design Reference**: See [design.md](./design.md#phase-7-integration-with-existing-code)

**Implementation Tasks**:
- [ ] Add trait-based alternative functions alongside existing
- [ ] Update tests to verify equivalent behavior
- [ ] Performance benchmarks
- [ ] Document migration path

**Files Changed**:
- `src/copy.rs` (add trait-based alternatives)
- `src/sync.rs` (add trait-based alternatives)

**Strategy**: Keep existing code, add alternatives, migrate gradually

**Estimated Time**: 6-8 hours

**Success Criteria**:
- [ ] Trait-based versions work correctly
- [ ] Performance is equivalent
- [ ] Both versions coexist
- [ ] Tests pass

---

## Future Work (Not in Initial PRs)

### Phase 9: Complete Protocol Backend
- Implement full protocol filesystem operations
- Add remote file operations
- Integrate with SSH transport

### Phase 10: Full Migration
- Deprecate old code paths
- Remove duplicated implementations
- Update all call sites

### Phase 11: Optimizations
- Add caching where beneficial
- Optimize hot paths
- Advanced features

---

## Summary

**Total**: 1 design PR + 8 implementation PRs

**Timeline**:
- **Phase 0**: ✅ COMPLETE (design docs)
- Phase 1-4: ~2-3 weeks (trait definitions)
- Phase 5-7: ~2-3 weeks (implementations)
- Phase 8: ~1 week (integration)
- **Total**: ~5-7 weeks after design approval

**Dependencies**: Linear - each PR builds on previous

**Git Strategy**:
```
main
 └─> design/... (PR #0) ← CURRENT
      └─> feat/async-metadata-trait (PR #1)
           └─> feat/async-file-trait (PR #2)
                └─> ... (continue stacking)
```

**Key Reference Docs**:
- [design.md](./design.md) - Architecture and detailed design
- [rsync-protocol-analysis.md](./rsync-protocol-analysis.md) - Protocol compatibility
- [streaming-vs-batch.md](./streaming-vs-batch.md) - Execution strategies
- [layer-integration.md](./layer-integration.md) - How layers work together

---

## Review Checklist

### For PR #0 (Design):
- [ ] Architecture makes sense
- [ ] Streaming vs batching approach is sound
- [ ] Layer integration (DirectoryFd reuse) is correct
- [ ] Plan is achievable
- [ ] No major design flaws

### For Each Implementation PR:
- [ ] Compiles without warnings
- [ ] All tests pass
- [ ] Follows design documents
- [ ] No deviation from approved architecture
- [ ] Documentation complete
- [ ] Performance is acceptable
