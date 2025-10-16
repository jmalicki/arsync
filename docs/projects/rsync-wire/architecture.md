# rsync Wire Protocol Implementation Plan

**Status**: Handshake Complete & Validated - Building Toward Full Compatibility  
**Date**: October 9, 2025  
**Last Updated**: Current session
**Architecture**: Pure compio + io_uring throughout ✅

---

## Executive Summary

This document tracks the implementation of rsync wire protocol compatibility in arsync.

**Current Status (Updated)**:
- ✅ **Phase 1**: Handshake Protocol - COMPLETE (4 commits, 21 tests)
- ✅ **Phase 2**: compio/io_uring Migration - COMPLETE (5 commits, full alignment)
- ✅ **Phase 3**: rsync Integration Test - COMPLETE (validated with real rsync!)
- ⏳ **Phase 4**: File List Exchange - NEXT
- ⏳ **Phase 5**: Checksum Algorithm
- ⏳ **Phase 6**: Delta Algorithm
- ⏳ **Phase 7**: Full End-to-End Sync

**Test Results**: 86/86 tests passing ✅

---

## Part 1: Completed Phases ✅

### ✅ Phase 1: Handshake Protocol (COMPLETE)

**Status**: ✅ Fully implemented, tested, validated

**What's Implemented**:
- Protocol version negotiation (supports v27-v40, uses v31)
- 10 capability flags (checksums, symlinks, hardlinks, devices, xattrs, ACLs, etc.)
- Checksum seed generation and exchange
- 9-state handshake state machine
- Role-based API (sender/receiver)

**Files**:
- `src/protocol/handshake.rs` - 1045 lines
- `tests/handshake_unit_tests.rs` - 14 tests ✅
- `tests/handshake_pipe_tests.rs` - 5 tests ✅

**Commits**:
1. `37451a4` - Core data structures
2. `d25e02a` - State machine implementation
3. `e7bc831` - High-level API
4. `0f47e14` - Unit tests
5. `f3cb0d0` - Pipe integration tests

**Test Coverage**: 19/19 handshake tests passing

---

### ✅ Phase 2: compio/io_uring Migration (COMPLETE)

**Status**: ✅ Full architecture alignment achieved

**Why This Phase**:
- Original protocol used blocking I/O (`std::io`) in async context
- Caused deadlocks and test hangs
- Migrated everything to compio for io_uring backend

**What's Migrated**:
- ✅ `PipeTransport`: Now uses `compio::fs::AsyncFd<OwnedFd>`
- ✅ `SshConnection`: Now uses `compio::process::Command`
- ✅ Transport helpers: `read_exact`, `write_all` use compio buffer model
- ✅ All tests: Use `#[compio::test]` runtime

**Files Changed**:
- `src/protocol/transport.rs` - Redesigned for compio traits
- `src/protocol/pipe.rs` - Migrated to AsyncFd
- `src/protocol/ssh.rs` - Migrated to compio::process
- `Cargo.toml` - Added `"process"` feature to compio

**Commits**:
1. `12396c5` - compio audit
2. `9fbf1fb` - Transport trait redesign
3. `4a68f88` - PipeTransport migration
4. `62ea27a` - SshConnection migration
5. Validation commit

**Results**:
- ✅ No more tokio in protocol layer
- ✅ No more blocking I/O in async code
- ✅ All 78 tests passing after migration
- ✅ Pipe tests that previously hung now work perfectly

---

### ✅ Phase 3: rsync Integration Test (COMPLETE)

**Status**: ✅ Handshake validated with real rsync binary

**What's Implemented**:
- Spawn `rsync --server` via `compio::process::Command`
- Create `RsyncTransport` wrapper (implements `Transport` trait)
- Perform bidirectional handshake with real rsync
- Validate protocol version and capabilities

**File**:
- `tests/rsync_handshake_integration_test.rs` - 3 tests ✅

**Commit**:
- `39da443` - rsync handshake integration test

**Results**:
- ✅ Works with rsync 3.4.1 (protocol version 32)
- ✅ Handshake completes successfully
- ✅ Protocol compatibility validated
- ⚠️ rsync error "connection unexpectedly closed" is EXPECTED (we only implement handshake so far)

**Key Achievement**: This proves our handshake implementation is correct before building more!

---

## Part 2: Current Architecture

### Module Structure (Updated)

```
src/protocol/
├── mod.rs              # Entry point, routing logic
├── handshake.rs        # ✅ Handshake state machine (complete)
├── checksum.rs         # ✅ Rolling + MD5 checksums (complete)
├── pipe.rs             # ✅ Pipe transport with compio (complete)
├── ssh.rs              # ✅ SSH via compio::process (complete)
├── transport.rs        # ✅ Transport trait (compio-based, complete)
├── rsync.rs            # ✅ arsync native protocol (complete)
├── rsync_compat.rs     # ⏳ rsync wire protocol (started - multiplex I/O)
├── varint.rs           # ⏳ Variable-length integers (started, needs tests)
└── quic.rs             # ⏳ QUIC transport (stub, feature-gated)

tests/
├── handshake_unit_tests.rs           # ✅ 14 tests
├── handshake_pipe_tests.rs           # ✅ 5 tests  
├── rsync_handshake_integration_test.rs # ✅ 3 tests
├── rsync_format_unit_tests.rs        # ✅ 14 tests
└── rsync_integration_tests.rs        # ⏳ Shell-based (may replace)
```

### Architecture Stack (All on io_uring!)

```
Application Layer
    ↓
Handshake Protocol (src/protocol/handshake.rs)
    ↓
Transport Abstraction (src/protocol/transport.rs)
    ↓
    ├─ PipeTransport (compio::fs::AsyncFd)
    ├─ SshConnection (compio::process)
    └─ QuicConnection (future)
    ↓
compio Runtime (io_uring backend)
    ↓
Linux Kernel (io_uring)
```

---

## Part 3: Remaining Phases

### Phase 4: File List Exchange (NEXT)

**Goal**: Implement rsync file list format encoding/decoding

**Status**: ⏳ Not started

**Tasks**:
- [ ] Implement varint encoding (already stubbed)
- [ ] Add varint unit tests
- [ ] Implement file list encoding (rsync format)
- [ ] Implement file list decoding (rsync format)
- [ ] Handle flags byte correctly
- [ ] Test file list exchange via pipes
- [ ] Test with real rsync

**Files to Modify**:
- `src/protocol/varint.rs` (needs tests)
- `src/protocol/rsync_compat.rs` (add file list functions)

**Estimated Time**: 6-8 hours

**Success Criteria**:
- [ ] File list roundtrips correctly
- [ ] rsync accepts our file list
- [ ] We can parse rsync's file list

---

### Phase 5: Checksum Algorithm

**Goal**: Implement checksum exchange in rsync format

**Status**: ⏳ Not started

**Tasks**:
- [ ] Implement rsync checksum header format
- [ ] Send block checksums (rsync format)
- [ ] Receive block checksums (rsync format)
- [ ] Test checksum exchange
- [ ] Verify rsync accepts checksums

**Files to Modify**:
- `src/protocol/rsync_compat.rs`
- `src/protocol/checksum.rs` (may need rsync-specific wrappers)

**Estimated Time**: 3-4 hours

**Success Criteria**:
- [ ] Checksum exchange works
- [ ] rsync generates delta from our checksums
- [ ] We can parse rsync's checksums

---

### Phase 6: Delta Algorithm

**Goal**: Implement delta token format (rsync's token stream)

**Status**: ⏳ Not started

**Tasks**:
- [ ] Study rsync's token.c
- [ ] Implement token encoding (1-96 = literal, 97-255 = block match)
- [ ] Implement token decoding
- [ ] Convert DeltaInstructions to tokens
- [ ] Test token generation
- [ ] Test token parsing

**Files to Modify**:
- `src/protocol/rsync_compat.rs`

**Estimated Time**: 8-12 hours (most complex!)

**Success Criteria**:
- [ ] Delta tokens roundtrip correctly
- [ ] rsync accepts our delta tokens
- [ ] We can parse rsync's delta tokens
- [ ] Files reconstruct byte-for-byte

---

### Phase 7: Full End-to-End Sync

**Goal**: Wire everything together for complete rsync compatibility

**Status**: ⏳ Not started

**Tasks**:
- [ ] Add `--rsync-compat` CLI flag
- [ ] Wire up complete sender flow
- [ ] Wire up complete receiver flow
- [ ] Test arsync → rsync (push)
- [ ] Test rsync → arsync (pull)
- [ ] Test arsync ↔ arsync (native vs compat)
- [ ] Performance testing
- [ ] Documentation

**Files to Modify**:
- `src/main.rs`
- `src/cli.rs`
- `src/protocol/mod.rs`

**Estimated Time**: 6-8 hours

**Success Criteria**:
- [ ] Full file sync works (arsync ↔ rsync)
- [ ] Metadata preserved correctly
- [ ] Performance is acceptable
- [ ] Error handling is robust

---

## Part 4: Testing Strategy

### Current Test Coverage (86 tests)

```
Lib tests:              50 ✅
Handshake unit:         14 ✅
Format unit:            14 ✅
Pipe integration:        5 ✅
rsync integration:       3 ✅
──────────────────────────
Total:                  86 ✅
```

### Planned Test Additions

**Phase 4 (File List)**:
- [ ] Unit tests for varint encoding (10 tests)
- [ ] File list roundtrip tests (5 tests)
- [ ] rsync compatibility tests (3 tests)

**Phase 5 (Checksums)**:
- [ ] Checksum format tests (5 tests)
- [ ] rsync checksum exchange tests (3 tests)

**Phase 6 (Delta)**:
- [ ] Token encoding tests (10 tests)
- [ ] Delta roundtrip tests (5 tests)
- [ ] rsync delta tests (3 tests)

**Phase 7 (End-to-End)**:
- [ ] Full sync tests (arsync → rsync)
- [ ] Full sync tests (rsync → arsync)
- [ ] Performance benchmarks

**Target**: 130+ tests by Phase 7 completion

---

## Part 5: Timeline Estimate

### Conservative Estimate

| Phase | Task | Status | Hours |
|-------|------|--------|-------|
| 1 | Handshake Protocol | ✅ DONE | ~12 |
| 2 | compio Migration | ✅ DONE | ~8 |
| 3 | rsync Integration Test | ✅ DONE | ~2 |
| 4 | File List Exchange | ⏳ Next | 6-8 |
| 5 | Checksum Algorithm | ⏳ Pending | 3-4 |
| 6 | Delta Algorithm | ⏳ Pending | 8-12 |
| 7 | End-to-End Integration | ⏳ Pending | 6-8 |
| **Completed** | **Phases 1-3** | ✅ | **~22** |
| **Remaining** | **Phases 4-7** | ⏳ | **23-32** |
| **Total** | **Full rsync compatibility** | - | **45-54** |

### Realistic Prediction

Based on our velocity so far:
- **Completed in session**: Phases 2.3-2.5, 1.5b, 3 (4 phases in ~3 hours)
- **Projection**: Remaining phases will take **25-35 hours** realistically
- **Total project**: **~50 hours** from start to finish

---

## Part 6: Key Learnings

### What Went Well ✅

1. **Handshake design**: State machine approach worked perfectly
2. **compio migration**: Solved async/blocking mismatch, no more deadlocks
3. **Early validation**: Testing with real rsync early caught issues
4. **Incremental approach**: Small phases, frequent commits

### What Was Challenging ⚠️

1. **Protocol complexity**: rsync wire format is more complex than expected
2. **Async model**: compio's buffer ownership took time to understand
3. **Testing infrastructure**: Bidirectional pipes were tricky initially

### What We'd Do Differently 🔄

1. ~~Use shell scripts for testing~~ - **FIXED**: Now using compio for real async tests
2. ~~Phase numbering confusion~~ - **FIXED**: Clean renumbered plan
3. Start with integration test earlier - **DONE**: Phase 3 validates foundation

---

## Part 7: Dependencies

### Current Dependencies

```toml
[dependencies]
# Core async runtime (NOW WITH PROCESS SUPPORT!)
compio = { version = "0.16", features = ["macros", "dispatcher", "process"] }

# Checksums
md5 = "0.7"

# Random (for checksum seed)
rand = { version = "0.9", optional = true }

# Metadata
filetime = "0.2"
walkdir = "2.0"

[features]
remote-sync = ["tokio", "async-trait", "rand"]  # Note: tokio still in features but not used in protocol
quic = ["remote-sync", "quinn", "rustls"]
```

### No New Dependencies Needed!

All remaining phases can be implemented with existing dependencies.

---

## Part 8: Success Criteria

### Minimum Success (By Phase 7)

- [ ] arsync can sync with rsync (push)
- [ ] rsync can sync with arsync (pull)
- [ ] Files transfer correctly
- [ ] Metadata preserved
- [ ] No protocol errors

### Full Success (Future)

- [ ] All rsync features working
- [ ] Performance competitive with rsync
- [ ] Multiple protocol versions supported
- [ ] Compression integrated
- [ ] Production-ready

---

## Part 9: References

- [rsync Algorithm Technical Report](https://rsync.samba.org/tech_report/)
- [rsync Source Code](https://github.com/RsyncProject/rsync)
- `RSYNC_WIRE_PROTOCOL_SPEC.md` - Detailed protocol analysis
- `RSYNC_COMPAT_DETAILED_DESIGN.md` - Design document
- `RSYNC_IMPLEMENTATION_CHECKLIST.md` - Detailed task breakdown

---

## Part 10: What's NOT Implemented (Deferred)

### Features to Add Later

- [ ] Compression (zlib/zstd)
- [ ] Incremental recursion
- [ ] `--delete` mode
- [ ] `--partial` mode (resume)
- [ ] `--checksum` mode (whole-file verification)
- [ ] Batch mode
- [ ] Daemon mode (rsyncd)

**Reason**: Focus on core sync functionality first, add features based on need.

---

## Appendix: Commit History

### Phase 1 Commits
- `37451a4` - feat(handshake): implement core data structures
- `d25e02a` - feat(handshake): implement state machine
- `e7bc831` - feat(handshake): add high-level API
- `0f47e14` - test(handshake): add comprehensive unit tests

### Phase 2 Commits
- `12396c5` - docs(compio): audit compio 0.16
- `9fbf1fb` - refactor(transport): redesign for compio
- `4a68f88` - refactor(pipe): migrate to compio/io_uring
- `62ea27a` - refactor(ssh): migrate to compio::process
- Validation commit

### Phase 3 Commits
- `f3cb0d0` - test(handshake): add pipe integration tests
- `39da443` - test(rsync): add handshake integration test

**Total Commits**: 10 (across 3 phases)

---

**Document Version**: 3.0 (Post-Phase 3)  
**Last Updated**: Current session  
**Status**: Phases 1-3 complete, ready for Phase 4  
**Test Count**: 86/86 passing ✅  
**Architecture**: Pure compio + io_uring ✅
