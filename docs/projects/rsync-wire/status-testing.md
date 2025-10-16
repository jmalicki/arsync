# Pipe-Based Protocol Testing Status

**Purpose**: Track implementation status of pipe-based rsync protocol testing  
**Date**: October 9, 2025  
**Location**: `tests/protocol_pipe_tests.rs`

---

## Test Matrix Status

| # | Test Case | Sender | Receiver | Status | Notes |
|---|-----------|--------|----------|--------|-------|
| 1 | Baseline | rsync | rsync | ✅ **PASSING** | Validates rsync works, test infrastructure correct |
| 2 | Our Protocol | arsync | arsync | ✅ **PASSING** | Full file transfer working! |
| 3 | Pull Compat | rsync | arsync | ⏳ **PENDING** | Needs rsync wire protocol compatibility |
| 4 | Push Compat | arsync | rsync | ⏳ **PENDING** | Needs rsync wire protocol compatibility |

---

## Test 1: rsync → rsync (Baseline) ✅

**Status**: PASSING

**What it does**:
- Runs rsync in local mode (rsync internally uses fork+pipe)
- Creates test data (files, subdirectories, permissions)
- Verifies all files transferred correctly

**Output**:
```
✓ Test 1/4: rsync baseline (local copy) PASSED
  This validates that rsync itself works correctly
  (rsync internally uses fork+pipe, same protocol we'll implement)
```

**Purpose**: Validates that:
- Test infrastructure works
- Test data is correct
- Expected behavior is clear

---

## Test 2: arsync → arsync (Our Protocol) ✅

**Status**: PASSING - Full file transfer working!

**What was implemented**:

### 1. Protocol Phases ✅
- **Handshake**: Version exchange (31 ↔ 31)
- **File List**: Count + metadata for each file
- **File Transfer**: Length + content for each file

### 2. Transport Layer ✅
- Bidirectional Unix pipe pairs (not simple `|` pipe)
- `PipeTransport` using blocking I/O
- Proper flush() after all data sent

### 3. Test Infrastructure ✅
```rust
// Creates bidirectional pipes
let (pipe1_read, pipe1_write) = create_pipe_pair();
let (pipe2_read, pipe2_write) = create_pipe_pair();

// Sender: writes to pipe1, reads from pipe2
// Receiver: reads from pipe1, writes to pipe2
```

### 4. Current Implementation
- Sends whole files (no delta optimization yet)
- Simple, correct, and validates protocol works
- Foundation for future delta/checksum features

**Test Output**:
```
✓ Test 2/4: arsync → arsync via pipe PASSED
  Our custom protocol implementation works!
```

---

## Test 3: rsync → arsync (Pull Compatibility) ⏳

**Status**: SKIPPED - Needs receiver protocol implementation

**What this validates**:
- arsync can act as receiver in rsync protocol
- Validates pull operations (`arsync user@host:/remote /local`)
- Tests protocol compatibility from rsync's perspective

**Implementation needed**:
1. Implement `RsyncReceiver` struct
2. Implement protocol handshake (receiver side)
3. Implement file list reception
4. Implement block checksum generation
5. Implement delta reception and application

**Test command (once implemented)**:
```bash
rsync --server --sender -av . /source/ \
    | arsync --pipe --pipe-role=receiver /dest/
```

---

## Test 4: arsync → rsync (Push Compatibility) ⏳

**Status**: SKIPPED - Needs sender protocol implementation

**What this validates**:
- arsync can act as sender in rsync protocol
- Validates push operations (`arsync /local user@host:/remote`)
- Tests protocol compatibility from arsync's perspective

**Implementation needed**:
1. Implement `RsyncSender` struct
2. Implement protocol handshake (sender side)
3. Implement file list generation and transmission
4. Implement block checksum reception
5. Implement delta generation and transmission

**Test command (once implemented)**:
```bash
arsync --pipe --pipe-role=sender /source/ \
    | rsync --server -av . /dest/
```

---

## Current Infrastructure

### ✅ Implemented

**Transport Abstraction**:
- `src/protocol/transport.rs` - Generic Transport trait
- `src/protocol/pipe.rs` - Pipe transport implementation
- In-memory pipe testing (tokio::io::duplex)

**Test Infrastructure**:
- `tests/protocol_pipe_tests.rs` - Test suite skeleton
- Test 1 passing (rsync baseline)
- Test data generation
- Transfer verification

**Dependencies**:
- tokio (for async pipes)
- async-trait (for Transport trait)
- Feature-gated: `cargo test --features remote-sync`

### ⏳ To Be Implemented

**Protocol Implementation** (see `docs/RSYNC_PROTOCOL_IMPLEMENTATION.md`):
1. Week 1-2: Protocol handshake
2. Week 3-4: File list exchange
3. Week 5-6: Block checksums
4. Week 7-8: Delta generation
5. Week 9-10: Delta application
6. Week 11-12: Metadata preservation
7. Week 13-14: Integration testing

**CLI Flags**:
- `--pipe` flag
- `--pipe-role=sender|receiver`
- `--pipe-debug=log|hexdump` (optional)

---

## Testing Strategy

### Phase 1: Baseline (✅ Complete)
- Test 1: rsync → rsync passing
- Infrastructure validated

### Phase 2: Implement --pipe Mode (Week 1)
- Add CLI flags
- Wire up PipeTransport to main.rs
- Test 2: arsync → arsync via in-memory pipes

### Phase 3: Implement Receiver (Weeks 2-6)
- Handshake, file list, checksums, delta application
- Test 3: rsync sender → arsync receiver
- Validates pull compatibility

### Phase 4: Implement Sender (Weeks 7-10)
- Delta generation, file list transmission
- Test 4: arsync sender → rsync receiver
- Validates push compatibility

### Phase 5: All Tests Passing (Week 11-12)
- All 4 combinations work
- Full rsync wire protocol compatibility validated

---

## How to Run Tests

```bash
# Run all pipe protocol tests
cargo test --features remote-sync --test protocol_pipe_tests -- --nocapture

# Run specific test
cargo test --features remote-sync --test protocol_pipe_tests test_rsync_to_rsync -- --nocapture

# Run including ignored tests (future tests)
cargo test --features remote-sync --test protocol_pipe_tests -- --nocapture --include-ignored
```

---

## Success Criteria

All 4 tests must pass before declaring rsync wire protocol compatibility:

- [x] Test 1: rsync baseline (passing)
- [x] Test 2: arsync ↔ arsync (our implementation works!) ✅
- [ ] Test 3: rsync → arsync (pull compatibility)
- [ ] Test 4: arsync → rsync (push compatibility)

**Current Status**: 2/4 tests passing (50%)  
**When all 4 pass**: arsync is fully compatible with rsync wire protocol! 🎉

---

## References

- **Implementation Plan**: `docs/RSYNC_PROTOCOL_IMPLEMENTATION.md`
- **Research**: `docs/research/REMOTE_SYNC_RESEARCH.md`
- **Pipe Protocol Design**: `docs/research/RSYNC_PIPE_PROTOCOL.md`
- **Test Code**: `tests/protocol_pipe_tests.rs`

---

**Last Updated**: October 9, 2025  
**Next Milestone**: Implement `--pipe` mode (Test 2)

