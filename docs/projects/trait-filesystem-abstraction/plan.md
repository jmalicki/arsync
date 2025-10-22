# Trait-Based Filesystem Abstraction - Implementation Plan

## Overview

Incremental plan with **fine-grained steps**. Each PR is small, focused, and integrates with existing code.

**See [design.md](./design.md)** for architecture and rationale.

## Principles

1. **Small PRs** - each does one thing
2. **Early integration** - validate as we go
3. **Each PR compiles and passes tests**
4. **Create PR immediately** - push branch and create PR after completing tasks
5. **Stacked** - each builds on previous
6. **Can merge independently** - deliver value early

---

## Phase 0: Design Documentation ✅

**Branch**: `design/trait-based-filesystem-abstraction` (current)

**What**: Complete design documentation (7 docs)

**Status**: Ready for review and merge

**Completed**:
- [x] Create README.md - Overview and navigation
- [x] Create design.md - Full architecture and rationale
- [x] Create plan.md - Fine-grained implementation plan (25 PRs)
- [x] Create rsync-protocol-analysis.md - Protocol compatibility analysis
- [x] Create streaming-vs-batch.md - Execution strategy comparison
- [x] Create layer-integration.md - How layers work together
- [x] Create implementation-requirements.md - Security/performance requirements

**Next**: 
- [x] Push branch to origin
- [x] Create PR #0: `gh pr create --base main --title "design: Add trait-based filesystem abstraction design" --draft`
- [ ] Get design reviewed and approved
- [ ] Merge to main

**PR**: https://github.com/jmalicki/arsync/pull/105 ✅

**Success**: [ ] Merged to main

---

## Phase 1: AsyncMetadata Trait

### PR #1: Define AsyncMetadata trait

**Branch**: `feat/trait-async-metadata`
**Base**: `main` (after Phase 0)

**What**: Just the trait definition, no implementation yet

**Tasks**:
- [ ] Create `src/traits/mod.rs`
- [ ] Create `src/traits/metadata.rs` with trait
- [ ] Add `pub mod traits;` to `src/lib.rs`
- [ ] Add basic tests (compile checks)

**Files**: `src/lib.rs`, `src/traits/*.rs` (2 new files)

**Time**: 2-3 hours

**Success**:
- [ ] Trait compiles
- [ ] Tests pass

---

### PR #2: Implement AsyncMetadata for Metadata

**Branch**: `feat/metadata-impl-trait`
**Base**: `feat/trait-async-metadata`

**What**: Make existing `Metadata` implement the trait

**Tasks**:
- [x] Add `impl AsyncMetadata for Metadata` in `src/metadata.rs`
- [x] Add integration tests
- [x] Verify all trait methods work

**Files**: `src/metadata.rs`, `src/main.rs`, `tests/trait_metadata_integration.rs` (new)

**Time**: 2-3 hours

**Integration**: Existing type now implements trait ✓

**Success**:
- [x] `FileMetadata` implements `AsyncMetadata`
- [x] All trait methods work
- [x] Tests pass (3 integration tests)

---

### PR #2.1 (Future): Richer special-file typing

**Branch**: `feat/metadata-special-file-types` (optional enhancement)
**Base**: `feat/metadata-impl-trait`

**What**: Enhance `AsyncMetadata` to expose Unix special file types

**Context**: CodeRabbit review suggestion from PR #107 - currently `file_type()` returns generic "unknown" for special files. Could expose richer typing using Unix mode bits.

**Tasks**:
- [ ] Add methods to detect specific special file types:
  - `is_block_device()` - block devices
  - `is_char_device()` - character devices  
  - `is_fifo()` - named pipes/FIFOs
  - `is_socket()` - Unix domain sockets
- [ ] Update `file_type()` to return specific types: "block", "char", "fifo", "socket"
- [ ] Update `file_type_description()` for consistency
- [ ] Add tests for each special file type
- [ ] Requires exposing Unix mode bits (`S_IFBLK`, `S_IFCHR`, `S_IFIFO`, `S_IFSOCK`)

**Files**: `src/traits/metadata.rs`, tests

**Time**: 2-3 hours

**Priority**: Low - Nice to have for completeness, but not needed for core functionality

**Success**:
- [ ] Can detect and describe all Unix file types
- [ ] `file_type()` returns specific strings for special files
- [ ] Tests cover all file types
- [ ] Documentation explains when/why to use these methods

**Note**: This enhancement can be deferred indefinitely. The current implementation with "unknown" for special files is sufficient for most use cases.

---

## Phase 2: AsyncFile Trait

### PR #3: Define AsyncFile trait

**Branch**: `feat/trait-async-file`
**Base**: `feat/metadata-impl-trait`

**What**: Just the trait definition

**Tasks**:
- [x] Create `src/traits/file.rs`
- [x] Define `AsyncFile` trait with provided methods
- [x] Add tests (compile checks, provided methods)

**Files**: `src/traits/mod.rs`, `src/traits/file.rs` (new)

**Time**: 2-3 hours

**Success**:
- [x] Trait compiles
- [x] Provided methods work in tests

**PR**: https://github.com/jmalicki/arsync/pull/108 ✅

---

### PR #4: Create AsyncFileWrapper

**Branch**: `feat/file-wrapper`
**Base**: `feat/trait-async-file`

**What**: Wrapper around compio::fs::File that implements AsyncFile

**Tasks**:
- [ ] Create `src/file_wrapper.rs`
- [ ] Implement `AsyncFile` for wrapper
- [ ] Add tests with real files

**Files**: `src/lib.rs`, `src/file_wrapper.rs` (new)

**Time**: 3-4 hours

**Integration**: Can wrap and use compio files via trait ✓

**Success**:
- [ ] Wrapper works with real files
- [ ] All trait methods tested
- [ ] Performance acceptable

**PR**: https://github.com/jmalicki/arsync/pull/109 ✅

---

### PR #5: Add copy helper using AsyncFile

**Branch**: `feat/copy-with-file-trait`
**Base**: `feat/file-wrapper`

**What**: Small helper function that uses AsyncFile trait

**Tasks**:
- [x] Add `copy_file_with_trait()` in new `src/copy_trait.rs`
- [x] Use AsyncFileWrapper and trait methods
- [x] Add tests (4 tests: small, empty, large, exact match)
- [ ] Benchmark

**Files**: `src/lib.rs`, `src/main.rs`, `src/copy_trait.rs` (new)

**Time**: 2-3 hours

**Integration**: Real copy operation uses traits ✓

**Success**:
- [x] Helper works correctly
- [x] All 4 tests pass
- [ ] Performance comparison (deferred to benchmarking PR)
- [x] Tests pass

**PR**: https://github.com/jmalicki/arsync/pull/110 ✅

---

## Phase 3: AsyncDirectory Trait

### PR #6: Define AsyncDirectory traits

**Branch**: `feat/trait-async-directory`
**Base**: `feat/copy-with-file-trait`

**What**: Trait definitions for directory and entry

**Tasks**:
- [x] Create `src/traits/directory.rs`
- [x] Define `AsyncDirectoryEntry` trait (3 required + 3 provided methods)
- [x] Define `AsyncDirectory` trait (2 required methods)
- [x] Add tests (4 tests with mock implementations)

**Files**: `src/traits/mod.rs`, `src/traits/directory.rs` (new)

**Time**: 3-4 hours

**Success**:
- [x] Traits compile
- [x] All 4 tests pass

**PR**: https://github.com/jmalicki/arsync/pull/111 ✅

---

### PR #7: Implement traits for DirectoryEntry

**Branch**: `feat/directory-entry-trait-impl`
**Base**: `feat/trait-async-directory`

**What**: Make existing DirectoryEntry implement trait

**Tasks**:
- [ ] Add `impl AsyncDirectoryEntry` in `src/directory/types.rs`
- [ ] Add integration tests
- [ ] Verify trait methods work

**Files**: `src/directory/types.rs`

**Time**: 2-3 hours

**Integration**: Existing DirectoryEntry uses trait ✓

**Success**:
- [ ] DirectoryEntry implements trait
- [ ] All methods work
- [ ] Tests pass

---

## Phase 4: Shared Operations (Incremental)

### PR #8: Add SecureTreeWalker

**Branch**: `feat/secure-tree-walker`
**Base**: `feat/directory-entry-trait-impl`

**What**: Shared directory walker using DirectoryFd

**Design**: [layer-integration.md](./layer-integration.md#1-secure-directory-walking)
**Requirements**: [implementation-requirements.md](./implementation-requirements.md)

**Before**:
- [ ] Read `src/directory/mod.rs`
- [ ] Read `crates/compio-fs-extended/src/directory.rs`

**Tasks**:
- [ ] Create `src/filesystem/mod.rs`
- [ ] Create `src/filesystem/walker.rs` with `SecureTreeWalker`
- [ ] Uses DirectoryFd throughout
- [ ] Add comprehensive tests

**Files**: `src/lib.rs`, `src/filesystem/*.rs` (2 new)

**Time**: 6-8 hours

**Success**:
- [ ] Uses DirectoryFd + *at syscalls
- [ ] TOCTOU-safe
- [ ] stat() once per file
- [ ] Tests pass

---

### PR #9: Add read_file_content helper

**Branch**: `feat/read-file-helper`
**Base**: `feat/secure-tree-walker`

**What**: Shared file reading with DirectoryFd

**Design**: [layer-integration.md](./layer-integration.md#2-secure-file-reading)
**Requirements**: [implementation-requirements.md](./implementation-requirements.md)

**Tasks**:
- [ ] Create `src/filesystem/read.rs`
- [ ] Implement `read_file_content()` using openat
- [ ] Add tests

**Files**: `src/filesystem/mod.rs`, `src/filesystem/read.rs` (new)

**Time**: 2-3 hours

**Success**:
- [ ] Uses DirectoryFd::open_file_at()
- [ ] Uses compio, not std::fs
- [ ] Tests pass

---

### PR #10: Add write_file_content helper

**Branch**: `feat/write-file-helper`
**Base**: `feat/read-file-helper`

**What**: Shared file writing with DirectoryFd

**Design**: [layer-integration.md](./layer-integration.md#3-secure-file-writing)
**Requirements**: [implementation-requirements.md](./implementation-requirements.md)

**Tasks**:
- [ ] Create `src/filesystem/write.rs`
- [ ] Implement `write_file_content()` using openat
- [ ] Add tests

**Files**: `src/filesystem/mod.rs`, `src/filesystem/write.rs` (new)

**Time**: 2-3 hours

**Success**:
- [ ] Uses DirectoryFd::open_file_at()
- [ ] O_NOFOLLOW for security
- [ ] Tests pass

---

### PR #11: Add preserve_metadata helper

**Branch**: `feat/preserve-metadata-helper`
**Base**: `feat/write-file-helper`

**What**: Shared metadata preservation with *at syscalls

**Design**: [layer-integration.md](./layer-integration.md#4-metadata-preservation)
**Requirements**: [implementation-requirements.md](./implementation-requirements.md)

**Tasks**:
- [ ] Create `src/filesystem/metadata.rs`
- [ ] Implement `preserve_metadata()` using DirectoryFd
- [ ] Uses lutimensat, lfchmodat, lfchownat
- [ ] Add tests

**Files**: `src/filesystem/mod.rs`, `src/filesystem/metadata.rs` (new)

**Time**: 3-4 hours

**Success**:
- [ ] Uses DirectoryFd *at syscalls
- [ ] TOCTOU-safe
- [ ] Tests pass

---

### PR #12: Migrate copy_file to use shared operations

**Branch**: `feat/migrate-copy-file`
**Base**: `feat/preserve-metadata-helper`

**What**: First real migration - create copy_file_v2 using shared ops

**Tasks**:
- [ ] Add `copy_file_v2()` in `src/copy.rs`
- [ ] Uses `SecureTreeWalker`, `read_file_content`, `write_file_content`, `preserve_metadata`
- [ ] Side-by-side comparison tests
- [ ] Performance benchmarks

**Files**: `src/copy.rs`

**Time**: 4-5 hours

**Integration**: Real copy operation uses all shared components ✓

**Success**:
- [ ] copy_file_v2 works correctly
- [ ] Behavior matches copy_file
- [ ] Performance equivalent or better
- [ ] Uses DirectoryFd throughout
- [ ] Tests pass

---

## Phase 5: AsyncFileSystem

### PR #13: Define AsyncFileSystem trait

**Branch**: `feat/trait-async-filesystem`
**Base**: `feat/migrate-copy-file`

**What**: Top-level filesystem trait definition

**Design**: [design.md](./design.md#phase-4-asyncfilesystem)

**Tasks**:
- [ ] Create `src/traits/filesystem.rs`
- [ ] Define `AsyncFileSystem` trait
- [ ] Add tests (compile checks)

**Files**: `src/traits/mod.rs`, `src/traits/filesystem.rs` (new)

**Time**: 2-3 hours

**Success**:
- [ ] Trait compiles
- [ ] Associated types work

---

### PR #14: Implement LocalFile

**Branch**: `feat/local-file-impl`
**Base**: `feat/trait-async-filesystem`

**What**: LocalFile implementation only

**Requirements**: [implementation-requirements.md](./implementation-requirements.md)

**Tasks**:
- [ ] Create `src/backends/mod.rs`
- [ ] Create `src/backends/local.rs` (LocalFile only)
- [ ] Implement AsyncFile for LocalFile
- [ ] Add tests

**Files**: `src/lib.rs`, `src/backends/*.rs` (2 new)

**Time**: 3-4 hours

**Success**:
- [ ] LocalFile works
- [ ] Uses compio::fs::File internally
- [ ] Tests pass

---

### PR #15: Implement LocalDirectory

**Branch**: `feat/local-directory-impl`
**Base**: `feat/local-file-impl`

**What**: LocalDirectory implementation only

**Requirements**: [implementation-requirements.md](./implementation-requirements.md)

**Tasks**:
- [ ] Add LocalDirectory and LocalDirectoryEntry to `src/backends/local.rs`
- [ ] Implement AsyncDirectory traits
- [ ] Uses DirectoryFd internally
- [ ] Add tests

**Files**: `src/backends/local.rs`

**Time**: 4-5 hours

**Success**:
- [ ] Uses DirectoryFd
- [ ] read_dir works
- [ ] Tests pass

---

### PR #16: Implement LocalFileSystem

**Branch**: `feat/local-filesystem-impl`
**Base**: `feat/local-directory-impl`

**What**: Complete LocalFileSystem implementation

**Requirements**: [implementation-requirements.md](./implementation-requirements.md)

**Before**:
- [ ] Read `src/directory/mod.rs`
- [ ] Review all requirements

**Tasks**:
- [ ] Add LocalFileSystem to `src/backends/local.rs`
- [ ] Implement all AsyncFileSystem methods
- [ ] Comprehensive integration tests

**Files**: `src/backends/local.rs`

**Time**: 5-6 hours

**Success**:
- [ ] Full AsyncFileSystem implementation
- [ ] Uses shared operations
- [ ] DirectoryFd throughout
- [ ] Tests pass

---

### PR #17: Migrate sync_directory using LocalFileSystem

**Branch**: `feat/migrate-sync-directory`
**Base**: `feat/local-filesystem-impl`

**What**: Create sync_directory_v2 using LocalFileSystem

**Tasks**:
- [ ] Add `sync_directory_v2()` in `src/sync.rs`
- [ ] Uses LocalFileSystem + shared operations
- [ ] Streaming (not batching)
- [ ] Side-by-side tests
- [ ] Benchmarks

**Files**: `src/sync.rs`

**Time**: 5-6 hours

**Integration**: Real sync uses full filesystem abstraction ✓

**Success**:
- [ ] Streams (walk→copy immediately)
- [ ] Behavior matches original
- [ ] Performance equivalent
- [ ] Tests pass

---

## Phase 6: SyncProtocol

### PR #18: Define SyncProtocol trait

**Branch**: `feat/trait-sync-protocol`
**Base**: `feat/migrate-sync-directory`

**What**: Protocol abstraction trait

**Design**: [rsync-protocol-analysis.md](./rsync-protocol-analysis.md)

**Tasks**:
- [ ] Create `src/traits/sync_protocol.rs`
- [ ] Define `SyncProtocol` trait
- [ ] Add stub tests

**Files**: `src/traits/mod.rs`, `src/traits/sync_protocol.rs` (new)

**Time**: 2-3 hours

**Success**:
- [ ] Trait compiles
- [ ] Design validated

---

### PR #19: Refactor rsync to use shared walker

**Branch**: `feat/rsync-use-walker`
**Base**: `feat/trait-sync-protocol`

**What**: Update rsync to use SecureTreeWalker

**Tasks**:
- [ ] Update `src/protocol/rsync_compat.rs`
- [ ] Replace walkdir with SecureTreeWalker
- [ ] Uses DirectoryFd for file access
- [ ] Tests with real protocol

**Files**: `src/protocol/rsync_compat.rs`

**Time**: 4-5 hours

**Integration**: rsync uses shared component ✓

**Success**:
- [ ] Uses SecureTreeWalker
- [ ] Protocol still works
- [ ] Tests pass

---

### PR #20: Refactor rsync to use shared file I/O

**Branch**: `feat/rsync-use-file-ops`
**Base**: `feat/rsync-use-walker`

**What**: Update rsync to use shared read/write helpers

**Tasks**:
- [ ] Replace `fs::read()` with `read_file_content()`
- [ ] Replace `fs::write()` with `write_file_content()`
- [ ] Uses DirectoryFd throughout
- [ ] Tests

**Files**: `src/protocol/rsync_compat.rs`, `src/protocol/rsync.rs`

**Time**: 4-5 hours

**Integration**: rsync uses shared I/O ✓

**Success**:
- [ ] Uses shared operations
- [ ] No std::fs in protocol code
- [ ] Tests pass

---

### PR #21: Implement SyncProtocol for rsync

**Branch**: `feat/rsync-sync-protocol-impl`
**Base**: `feat/rsync-use-file-ops`

**What**: Formal SyncProtocol trait implementation

**Tasks**:
- [ ] Create `src/backends/protocol.rs`
- [ ] Implement `SyncProtocol` for `RsyncProtocol<T>`
- [ ] Refactor existing rsync code to use trait
- [ ] Tests

**Files**: `src/backends/mod.rs`, `src/backends/protocol.rs` (new)

**Time**: 5-6 hours

**Integration**: rsync backend uses trait ✓

**Success**:
- [ ] Implements SyncProtocol
- [ ] Protocol transfers work
- [ ] Tests pass

---

## Phase 7: Unified Sync API

### PR #22: Add SyncBackend enum

**Branch**: `feat/sync-backend-enum`
**Base**: `feat/rsync-sync-protocol-impl`

**What**: Enum to represent local or remote backend

**Tasks**:
- [ ] Create `src/sync/backend.rs`
- [ ] Define `SyncBackend` enum (Local or Remote)
- [ ] Add dispatch methods
- [ ] Tests

**Files**: `src/sync/mod.rs`, `src/sync/backend.rs` (new)

**Time**: 2-3 hours

**Success**:
- [ ] Enum compiles
- [ ] Dispatch works

---

### PR #23: Add sync_unified() API

**Branch**: `feat/sync-unified-api`
**Base**: `feat/sync-backend-enum`

**What**: High-level API that works with both backends

**Design**: [streaming-vs-batch.md](./streaming-vs-batch.md)

**Tasks**:
- [ ] Add `sync_unified()` in `src/sync.rs`
- [ ] Dispatches to local or remote
- [ ] Streaming for local, batching for remote
- [ ] Progress reporting
- [ ] Tests with both backends

**Files**: `src/sync.rs`

**Time**: 5-6 hours

**Integration**: Single API for both ✓

**Success**:
- [ ] Works with local backend
- [ ] Works with remote backend
- [ ] Streaming vs batching correct
- [ ] Tests pass

---

### PR #24: Migrate CLI to use sync_unified()

**Branch**: `feat/cli-use-unified-sync`
**Base**: `feat/sync-unified-api`

**What**: Update CLI commands to use unified API

**Tasks**:
- [ ] Update `src/main.rs` or `src/cli.rs`
- [ ] Replace old sync calls with `sync_unified()`
- [ ] Keep old code with `#[deprecated]`
- [ ] End-to-end tests

**Files**: `src/main.rs` or `src/cli.rs`

**Time**: 3-4 hours

**Integration**: User-facing commands use traits ✓

**Success**:
- [ ] CLI works with new API
- [ ] Behavior unchanged
- [ ] Tests pass

---

## Phase 8: Cleanup

### PR #25: Remove deprecated code

**Branch**: `refactor/remove-old-impl`
**Base**: `feat/cli-use-unified-sync`

**What**: Remove old implementations

**Tasks**:
- [ ] Remove deprecated functions
- [ ] Update all remaining call sites
- [ ] Clean up imports
- [ ] Final tests

**Files**: Multiple

**Time**: 3-4 hours

**Success**:
- [ ] Old code removed
- [ ] All tests pass
- [ ] Clean codebase

---

## Summary

**Total**: 1 design + 25 small PRs

**Average PR size**: 2-6 hours each

**Benefits**:
- ✅ Each PR is reviewable in < 30 min
- ✅ Can merge and deploy incrementally
- ✅ Integration every few PRs
- ✅ Find issues early
- ✅ Can pause/adjust anytime
- ✅ Each PR delivers value

**Timeline**: ~6-8 weeks (more PRs, but safer)

**Integration Points**:
- PR #2: Metadata implements trait
- PR #4: File wrapper works
- PR #5: Copy helper works
- PR #7: DirectoryEntry implements trait
- PR #12: copy_file_v2 works
- PR #17: sync_directory_v2 works
- PR #19-20: rsync uses shared ops
- PR #23: Unified API works
- PR #24: CLI uses unified API

**Git Strategy**:
```
main → PR #0 (design)
        ↓
       PR #1 (trait def)
        ↓
       PR #2 (impl for Metadata) ← First integration
        ↓
       PR #3 (trait def)
        ↓
       PR #4 (wrapper) ← Integration
        ↓
       PR #5 (copy helper) ← Integration
        ↓
       ... (continue with frequent integration points)
```

---

## Review Checklist

**For each PR**:
- [ ] Does ONE thing
- [ ] < 500 lines changed
- [ ] Integrates with existing code (if applicable)
- [ ] Tests pass
- [ ] Follows [implementation-requirements.md](./implementation-requirements.md)
- [ ] Can merge independently
