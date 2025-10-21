# Trait-Based Filesystem Abstraction - Implementation Plan

## Overview

Incremental plan with **early and continuous integration**. Each phase adds a trait AND integrates it with existing code. No "big bang" at the end.

**See [design.md](./design.md)** for architecture and rationale.

## Principles

1. **Add trait + integrate immediately** - validate as we go
2. **Each PR must compile and pass tests**
3. **Stacked PRs** - each builds on previous
4. **Early validation** - find design issues early
5. **Reference design docs** - don't deviate

---

## Phase 0: Design Documentation ✅

**Status**: COMPLETE (current branch)

**Branch**: `design/trait-based-filesystem-abstraction`

**What**: Complete design documentation

**Files**: `docs/projects/trait-filesystem-abstraction/*.md` (7 files)

**Success Criteria**:
- [ ] Design reviewed and approved
- [ ] Team buy-in
- [ ] Merged to main

---

## Phase 1: AsyncMetadata + Integration

**Base Branch**: `main` (after Phase 0)
**Branch**: `feat/async-metadata-trait-integrated`

**What**: Add AsyncMetadata trait AND implement for existing types

**Design Reference**: [design.md](./design.md#phase-1-asyncmetadata)

**Tasks**:
- [ ] Create `src/traits/mod.rs` and `src/traits/metadata.rs`
- [ ] Define `AsyncMetadata` trait
- [ ] **Implement for existing `Metadata`** in `src/metadata.rs` ← INTEGRATION!
- [ ] **Add helper methods that use the trait** ← INTEGRATION!
- [ ] Tests for trait + integration

**Integration Point**: Existing `Metadata` type implements trait immediately

**Success Criteria**:
- [ ] Trait defined
- [ ] `Metadata` implements `AsyncMetadata`
- [ ] Existing code can use trait methods
- [ ] All tests pass

**Estimated Time**: 6-8 hours

---

## Phase 2: AsyncFile + Wrapper Integration

**Base Branch**: `feat/async-metadata-trait-integrated`
**Branch**: `feat/async-file-trait-integrated`

**What**: Add AsyncFile trait AND create wrapper for compio::fs::File

**Design Reference**: [design.md](./design.md#phase-2-asyncfile)

**Tasks**:
- [ ] Create `src/traits/file.rs` with `AsyncFile` trait
- [ ] **Create `src/file_wrapper.rs`** with `AsyncFileWrapper(compio::fs::File)` ← INTEGRATION!
- [ ] **Implement `AsyncFile` for `AsyncFileWrapper`** ← INTEGRATION!
- [ ] **Add convenience function in `src/copy.rs`** that uses trait ← INTEGRATION!
- [ ] Tests with real file operations

**Integration Point**: 
- Wrapper around compio::fs::File implements trait
- Small helper function in copy.rs uses it

**Example Integration**:
```rust
// src/copy.rs - add alongside existing functions
pub async fn copy_file_with_trait(
    src: &Path,
    dst: &Path,
) -> Result<u64> {
    use crate::file_wrapper::AsyncFileWrapper;
    use crate::traits::AsyncFile;
    
    let src_file = AsyncFileWrapper(compio::fs::File::open(src).await?);
    let dst_file = AsyncFileWrapper(compio::fs::File::create(dst).await?);
    
    // Use trait methods
    let metadata = src_file.metadata().await?;
    let size = metadata.size();
    // ... copy logic using trait
}
```

**Success Criteria**:
- [ ] Trait defined and works with real files
- [ ] Wrapper validated with actual I/O
- [ ] Copy helper function works
- [ ] All tests pass

**Estimated Time**: 6-8 hours

---

## Phase 3: AsyncDirectory + DirectoryEntry Integration

**Base Branch**: `feat/async-file-trait-integrated`
**Branch**: `feat/async-directory-trait-integrated`

**What**: Add AsyncDirectory traits AND implement for existing DirectoryEntry

**Design Reference**: [design.md](./design.md#phase-3-asyncdirectory)

**Tasks**:
- [ ] Create `src/traits/directory.rs`
- [ ] Define `AsyncDirectoryEntry` and `AsyncDirectory` traits
- [ ] **Implement traits for existing `DirectoryEntry`** in `src/directory/types.rs` ← INTEGRATION!
- [ ] **Add helper in `src/directory/mod.rs`** that uses traits ← INTEGRATION!
- [ ] Tests with real directory operations

**Integration Point**:
- Existing `DirectoryEntry` implements `AsyncDirectoryEntry`
- Directory walking helper uses trait

**Success Criteria**:
- [ ] Traits defined
- [ ] Existing types implement traits
- [ ] Directory operations work via traits
- [ ] All tests pass

**Estimated Time**: 6-8 hours

---

## Phase 4: Shared Operations + Incremental Migration

**Base Branch**: `feat/async-directory-trait-integrated`
**Branch**: `feat/shared-ops-with-migration`

**What**: Extract shared operations AND migrate first use case

**Design Reference**: [layer-integration.md](./layer-integration.md#shared-filesystem-operations-layer-2)

**⚠️ CRITICAL**: See [implementation-requirements.md](./implementation-requirements.md)

**Tasks**:
- [ ] Create `src/filesystem/` module with shared operations:
  - `walker.rs` - SecureTreeWalker with DirectoryFd
  - `read.rs` - read_file_content with openat
  - `write.rs` - write_file_content with openat
  - `metadata.rs` - preserve_metadata with *at syscalls
- [ ] **Pick ONE existing function to migrate** (e.g., `copy_file`) ← INTEGRATION!
- [ ] **Create `copy_file_v2`** using shared operations ← INTEGRATION!
- [ ] **Benchmark: old vs new** ← VALIDATION!
- [ ] Tests comparing behavior

**Integration Point**:
- Shared operations used by new copy_file_v2
- Side-by-side comparison validates design

**Success Criteria**:
- [ ] Shared operations work correctly
- [ ] Uses DirectoryFd throughout
- [ ] TOCTOU-safe, stat once per file
- [ ] New version matches old behavior
- [ ] Performance equivalent or better
- [ ] All tests pass

**Estimated Time**: 10-12 hours

---

## Phase 5: AsyncFileSystem Trait + LocalFileSystem

**Base Branch**: `feat/shared-ops-with-migration`
**Branch**: `feat/local-filesystem-integrated`

**What**: Add AsyncFileSystem trait AND implement LocalFileSystem

**Design Reference**: [design.md](./design.md#phase-4-asyncfilesystem) and [layer-integration.md](./layer-integration.md#local-backend-direct-use)

**⚠️ CRITICAL**: See [implementation-requirements.md](./implementation-requirements.md)

**Before Implementation**:
- [ ] Read `src/directory/mod.rs`
- [ ] Read `crates/compio-fs-extended/src/directory.rs`
- [ ] Review [implementation-requirements.md](./implementation-requirements.md)

**Tasks**:
- [ ] Create `src/traits/filesystem.rs` with `AsyncFileSystem`
- [ ] Create `src/backends/local.rs` with full implementation:
  - LocalFileSystem, LocalFile, LocalDirectory, LocalDirectoryEntry
  - Uses shared operations from Phase 4
  - Uses DirectoryFd throughout
- [ ] **Migrate second function** (e.g., `sync_directory`) ← INTEGRATION!
- [ ] **Create `sync_directory_v2`** using LocalFileSystem ← INTEGRATION!
- [ ] **Benchmark and compare** ← VALIDATION!
- [ ] Comprehensive integration tests

**Integration Point**:
- LocalFileSystem used in real sync operation
- Validates full filesystem abstraction

**Success Criteria**:
- [ ] AsyncFileSystem trait complete
- [ ] LocalFileSystem fully implemented
- [ ] **DirectoryFd used everywhere**
- [ ] **No std::fs usage**
- [ ] **TOCTOU-safe**
- [ ] **stat() called once per file**
- [ ] sync_directory_v2 works correctly
- [ ] Performance validated
- [ ] Security audit passed

**Estimated Time**: 12-16 hours

---

## Phase 6: SyncProtocol + Rsync Backend Integration

**Base Branch**: `feat/local-filesystem-integrated`
**Branch**: `feat/sync-protocol-integrated`

**What**: Add SyncProtocol trait AND implement for rsync

**Design Reference**: [rsync-protocol-analysis.md](./rsync-protocol-analysis.md#proposed-design-three-layer-architecture)

**Tasks**:
- [ ] Create `src/traits/sync_protocol.rs`
- [ ] **Update `src/protocol/rsync_compat.rs`** to use shared operations ← INTEGRATION!
- [ ] **Refactor to implement `SyncProtocol`** ← INTEGRATION!
- [ ] Use DirectoryFd and shared operations for file I/O
- [ ] **Test with real rsync protocol transfers** ← VALIDATION!

**Integration Point**:
- Existing rsync code refactored to use traits
- Validates protocol abstraction
- Shows shared operations work for both local and remote

**Success Criteria**:
- [ ] SyncProtocol trait defined
- [ ] Rsync backend uses shared operations
- [ ] No duplicate file I/O code
- [ ] Real protocol transfers work
- [ ] All tests pass

**Estimated Time**: 10-12 hours

---

## Phase 7: High-Level Sync API + Migration

**Base Branch**: `feat/sync-protocol-integrated`
**Branch**: `feat/unified-sync-api`

**What**: Unified sync API that works with both backends + migrate main paths

**Design Reference**: [streaming-vs-batch.md](./streaming-vs-batch.md#the-unified-api-backend-decides)

**Tasks**:
- [ ] Create `src/sync/engine.rs` with `SyncEngine`
- [ ] **Update `src/sync.rs`** to add unified API ← INTEGRATION!
- [ ] **Create `sync_unified()`** that dispatches to backends ← INTEGRATION!
- [ ] **Migrate CLI commands** to use unified API ← INTEGRATION!
- [ ] Progress reporting, error handling
- [ ] End-to-end tests with both backends

**Integration Point**:
- Main CLI paths use unified API
- Both local and remote use same high-level code
- Validates entire architecture

**Success Criteria**:
- [ ] Unified API works for both local and remote
- [ ] CLI commands migrated
- [ ] Streaming for local, batching for remote
- [ ] All tests pass
- [ ] User-facing functionality unchanged

**Estimated Time**: 10-12 hours

---

## Phase 8: Complete Migration + Cleanup

**Base Branch**: `feat/unified-sync-api`
**Branch**: `refactor/complete-trait-migration`

**What**: Migrate remaining code and clean up

**Tasks**:
- [ ] Migrate all remaining functions to use traits
- [ ] **Deprecate old implementations** with `#[deprecated]` ← INTEGRATION!
- [ ] Update all call sites
- [ ] Remove duplicated code
- [ ] Update documentation
- [ ] Final performance validation

**Integration Point**: 
- All code uses trait-based abstractions
- Old code deprecated but still available

**Success Criteria**:
- [ ] All major paths use traits
- [ ] Old code marked deprecated
- [ ] Performance validated
- [ ] Documentation updated
- [ ] All tests pass

**Estimated Time**: 8-10 hours

---

## Summary

**Total**: 1 design PR + 8 implementation PRs (each with integration)

**Key Difference**: Each phase integrates with existing code immediately

**Timeline**: ~6-8 weeks (same, but lower risk!)

**Integration Points**:
- Phase 1: Existing Metadata implements trait
- Phase 2: Wrapper + copy helper
- Phase 3: DirectoryEntry implements trait
- Phase 4: First function migrated (copy_file_v2)
- Phase 5: Second function migrated (sync_directory_v2)
- Phase 6: Rsync backend refactored
- Phase 7: CLI commands migrated
- Phase 8: All code migrated

**Benefits**:
- ✅ Early validation of design
- ✅ Continuous integration (not big bang)
- ✅ Find issues early
- ✅ Each PR delivers value
- ✅ Can stop/adjust anytime
- ✅ Performance validated continuously

**Git Strategy**:
```
main
 └─> design/... (PR #0)
      └─> feat/async-metadata-trait-integrated (PR #1) ← Metadata implements trait
           └─> feat/async-file-trait-integrated (PR #2) ← Wrapper + helper
                └─> feat/async-directory-trait-integrated (PR #3) ← DirectoryEntry
                     └─> feat/shared-ops-with-migration (PR #4) ← copy_file_v2
                          └─> ... (continue)
```

---

## Review Checklist

### For Each Implementation PR:
- [ ] Trait works with real code (not just mocks)
- [ ] Integration point validated
- [ ] Performance compared to existing
- [ ] Behavior matches existing (if replacing)
- [ ] All tests pass
- [ ] Follows [implementation-requirements.md](./implementation-requirements.md)
