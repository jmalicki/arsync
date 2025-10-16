# Implementation Plan: rsync-Compatible CLI Integration

**Status**: Planning  
**Complexity**: Medium  
**Estimated Duration**: 7-10 days  
**Created On Branch**: feature/rsync-wire-protocol  
**Implementation Branch**: feature/rsync-wire-protocol (continue in same branch)  
**Related Design**: [Design Document](design.md)

---

## Context

This plan implements the integration layer to connect the **complete rsync wire protocol** 
(106 tests passing, all phases 1-7 complete) to the CLI for production-ready remote sync.

**What Exists**:
- ✅ Complete protocol implementation (`src/protocol/rsync_compat.rs`)
- ✅ `rsync_send_via_pipe()` / `rsync_receive_via_pipe()` - Working
- ✅ CLI infrastructure (`RemoteConfig`, `Location::parse()`)
- ✅ Main routing structure

**What's Missing**: Integration glue (~200 lines total)
- Wire rsync_compat to SSH transport
- Implement --server mode
- Integration testing

---

## Overview

Enable users to run rsync-compatible remote sync commands:
```bash
arsync user@host:/remote /local    # Pull from remote
arsync /local user@host:/remote    # Push to remote
```

This is **primarily integration work**, not new protocol development. The protocol
is complete and tested - we're wiring it to SSH and the CLI.

---

## Design References

**Design Document**: [`design.md`](design.md)

**Key Design Decisions**:
1. **Build on rsync-wire**: Complete protocol (106 tests) - just add integration
2. **SshTransport wrapper**: Adapt SSH connection to Transport trait
3. **Server mode via stdin/stdout**: Use PipeTransport for --server mode
4. **Opt-out feature flag**: Enable remote-sync by default, opt-out with --no-default-features

**Architecture**: All code uses io_uring via compio (no tokio, no blocking I/O)

**Components from Design**:
- Component 1: Wire `rsync.rs` integration functions (call rsync_compat)
- Component 2: `SshTransport` wrapper (adapt SSH to Transport trait)
- Component 3: Server mode implementation
- Component 4: Feature flag (enable by default)

---

## Prerequisites

- [x] Review `src/protocol/rsync_compat.rs` - Complete protocol implementation
- [x] Review `src/protocol/ssh.rs` - SSH connection via compio::process
- [x] Review `src/protocol/transport.rs` - Transport trait definition
- [x] Review `src/main.rs` - Routing logic already structured
- [x] Read design doc: `docs/projects/rsync-cli/design.md`
- [ ] Verify SSH works on development machine
- [ ] Test localhost SSH key authentication

---

## Implementation Phases

### Phase 1: Wire Up Integration Functions

**Objective**: Connect rsync_compat protocol to SSH transport layer

**Estimated Time**: 1-2 days

#### 1.1: Create SshTransport Wrapper

**File**: `src/protocol/ssh.rs`

- [ ] Add `SshTransport` struct:
  ```rust
  pub struct SshTransport<'a> {
      connection: &'a mut SshConnection,
  }
  ```
- [ ] Implement `SshTransport::new(connection: &mut SshConnection)`
- [ ] Implement `AsyncRead` for `SshTransport<'_>`:
  - [ ] Delegate to `connection.stdout.read()`
  - [ ] Handle async properly with compio
- [ ] Implement `AsyncWrite` for `SshTransport<'_>`:
  - [ ] Delegate to `connection.stdin.write()`
  - [ ] Handle async properly with compio
- [ ] Implement `Transport` marker trait for `SshTransport<'_>`
- [ ] Add doc comments explaining the wrapper pattern

**Alternative Considered**: Make `SshConnection` directly implement `Transport`
- Start with wrapper (less invasive)
- Can refactor later if needed

#### 1.2: Implement push_via_rsync_protocol

**File**: `src/protocol/rsync.rs` (line ~39)

**Current**:
```rust
pub async fn push_via_rsync_protocol(...) -> Result<SyncStats> {
    // TODO: Implement rsync protocol
    anyhow::bail!("rsync protocol implementation in progress")
}
```

**Implementation**:
- [ ] Remove TODO and bail
- [ ] Create `SshTransport` wrapper: `let transport = SshTransport::new(connection);`
- [ ] Call rsync_compat: `rsync_compat::rsync_send_via_pipe(args, local_path, transport).await`
- [ ] Add error context for SSH failures
- [ ] Add tracing::info for start/completion
- [ ] Update doc comment to reflect actual implementation

#### 1.3: Implement pull_via_rsync_protocol

**File**: `src/protocol/rsync.rs` (line ~56)

**Current**:
```rust
pub async fn pull_via_rsync_protocol(...) -> Result<SyncStats> {
    // TODO: Implement rsync protocol (receiver side)
    anyhow::bail!("rsync protocol implementation in progress")
}
```

**Implementation**:
- [ ] Remove TODO and bail
- [ ] Create `SshTransport` wrapper: `let transport = SshTransport::new(connection);`
- [ ] Call rsync_compat: `rsync_compat::rsync_receive_via_pipe(args, transport, local_path).await`
- [ ] Add error context for SSH failures
- [ ] Add tracing::info for start/completion
- [ ] Update doc comment to reflect actual implementation

#### 1.4: Quality Checks - Phase 1

- [ ] `/fmt false true` - Format new code
- [ ] `/clippy false false` - Fix any warnings
- [ ] `/build "debug" "all" false` - Verify compiles with --features remote-sync
- [ ] Code review: Verify wrapper properly delegates to SSH connection
- [ ] Commit: `git commit -m "feat(protocol): wire rsync_compat to SSH transport"`

**Acceptance Criteria**:
- [ ] `push_via_rsync_protocol()` implemented (calls rsync_send_via_pipe)
- [ ] `pull_via_rsync_protocol()` implemented (calls rsync_receive_via_pipe)
- [ ] `SshTransport` wrapper complete and tested for compilation
- [ ] No clippy warnings
- [ ] Code formatted
- [ ] Compiles with `--features remote-sync`

---

### Phase 2: Implement Server Mode

**Objective**: Handle being invoked as `arsync --server` by remote SSH

**Estimated Time**: 1 day

#### 2.1: Add server_mode() Function

**File**: `src/protocol/mod.rs`

- [ ] Add new function after `pipe_receiver()`:
  ```rust
  #[cfg(feature = "remote-sync")]
  pub async fn server_mode(args: &Args) -> Result<SyncStats>
  ```
- [ ] Create `PipeTransport::from_stdio()` for stdin/stdout
- [ ] Get destination path from args
- [ ] Validate destination is Local (not Remote)
- [ ] Call `rsync_compat::rsync_receive_via_pipe(args, transport, &dest_path)`
- [ ] Add comprehensive doc comment explaining server mode
- [ ] Add tracing (to stderr!) for server mode start/complete
- [ ] Handle errors gracefully

**Design Note**: Server mode is always receiver in rsync protocol.
Remote SSH client connects and sends files to our stdin.

#### 2.2: Update Main Routing

**File**: `src/main.rs` (before line ~123, before pipe mode check)

- [ ] Add server mode check FIRST (before pipe, before remote):
  ```rust
  let result = if args.remote.server {
      // SERVER MODE (invoked by remote SSH)
      #[cfg(feature = "remote-sync")]
      {
          info!("Running in server mode");
          protocol::server_mode(&args).await
      }
      #[cfg(not(feature = "remote-sync"))]
      {
          anyhow::bail!("Server mode requires remote-sync feature")
      }
  } else if args.remote.pipe {
      // ... existing pipe mode code
  ```
- [ ] Ensure logging goes to stderr in server mode (already done for pipe mode)
- [ ] Test compilation

**Order Matters**: Check `--server` before checking remote paths
(server mode doesn't use Location, just gets destination from args)

#### 2.3: Manual Testing - Server Mode

- [ ] Build: `cargo build --features remote-sync`
- [ ] Test --server flag parsing: `./target/debug/arsync --server /tmp/dest` (should wait for stdin)
- [ ] Test with echo: `echo "test" | ./target/debug/arsync --server /tmp/dest` (should fail gracefully)
- [ ] Verify logs go to stderr (not stdout)
- [ ] Verify clean exit

#### 2.4: Quality Checks - Phase 2

- [ ] `/fmt false true` - Format new code
- [ ] `/clippy false false` - Fix warnings
- [ ] `/build "debug" "all" false` - Verify compiles
- [ ] Manual test: --server mode responds correctly
- [ ] Commit: `git commit -m "feat(protocol): implement server mode for remote invocation"`

**Acceptance Criteria**:
- [ ] `server_mode()` function implemented
- [ ] `main.rs` routes `--server` flag correctly
- [ ] Logs go to stderr (not stdout)
- [ ] Compiles cleanly
- [ ] Manual testing passes (responds to stdin)
- [ ] No clippy warnings

---

### Phase 3: End-to-End Integration Testing

**Objective**: Validate actual remote sync works via SSH

**Estimated Time**: 2-3 days

#### 3.1: Create Integration Test File

**File**: `tests/remote_sync_integration_test.rs` (new)

- [ ] Create new test file with module-level docs
- [ ] Add feature gate: `#![cfg(feature = "remote-sync")]`
- [ ] Import necessary modules
- [ ] Add helper function to check SSH availability:
  ```rust
  fn ssh_available() -> bool {
      Command::new("ssh").arg("-V").output().is_ok()
  }
  ```

#### 3.2: Test - Localhost Push via SSH

**Test**: `test_push_to_localhost_via_ssh`

- [ ] Skip if SSH not available: `if !ssh_available() { return; }`
- [ ] Create temp source directory with test files
- [ ] Create temp destination on "remote" (localhost)
- [ ] Build path: `localhost:/tmp/arsync_test_dest`
- [ ] Call integration (not via CLI, via protocol functions):
  ```rust
  let source = Location::Local(src_path);
  let dest = Location::parse("localhost:/tmp/test").unwrap();
  protocol::remote_sync(&args, &source, &dest).await?;
  ```
- [ ] Verify all files transferred
- [ ] Verify metadata preserved (permissions, times)
- [ ] Verify file contents match
- [ ] Clean up temp directories

**Note**: Requires SSH key for localhost configured (or will prompt for password)

#### 3.3: Test - Localhost Pull via SSH

**Test**: `test_pull_from_localhost_via_ssh`

- [ ] Skip if SSH not available
- [ ] Create temp source directory on "remote" (localhost)
- [ ] Create temp destination locally
- [ ] Build path: `localhost:/tmp/arsync_test_source`
- [ ] Call integration:
  ```rust
  let source = Location::parse("localhost:/tmp/test").unwrap();
  let dest = Location::Local(dest_path);
  protocol::remote_sync(&args, &source, &dest).await?;
  ```
- [ ] Verify all files transferred
- [ ] Verify metadata preserved
- [ ] Verify file contents match
- [ ] Clean up

#### 3.4: Test - Server Mode Direct

**Test**: `test_server_mode_via_stdio`

- [ ] Create bidirectional pipes (like existing pipe tests)
- [ ] Spawn client task: Send files via rsync_send_via_pipe
- [ ] Spawn server task: Call server_mode() with piped stdin/stdout
- [ ] Run concurrently with futures::join!
- [ ] Verify files transferred
- [ ] Verify server mode completes successfully

#### 3.5: Test - Error Conditions

**Test**: `test_remote_sync_errors`

- [ ] Test SSH connection failure (invalid host)
- [ ] Test remote arsync not found
- [ ] Test permission denied on remote
- [ ] Verify error messages are clear
- [ ] Verify no panics or hangs

#### 3.6: Debug Integration Issues

**Expected**: Some issues will arise during testing

- [ ] If tests fail: Use `/debug` to diagnose
- [ ] Add observability (tracing statements)
- [ ] Identify root causes
- [ ] Fix issues
- [ ] Document in plan as notes

#### 3.7: Quality Checks - Phase 3

- [ ] `/test "remote_sync_integration_test"` - All integration tests pass
- [ ] `/test "all" --features remote-sync` - No regressions
- [ ] `/fmt false true` - Format test code
- [ ] `/clippy false false` - Clean
- [ ] Commit: `git commit -m "test(protocol): add remote sync integration tests"`

**Acceptance Criteria**:
- [ ] Integration test file created
- [ ] Push to localhost via SSH works
- [ ] Pull from localhost via SSH works
- [ ] Server mode tested
- [ ] Error conditions handled
- [ ] All tests passing
- [ ] No regressions in existing tests (106 tests still pass)

---

### Phase 4: Feature Flag & Build Configuration

**Objective**: Enable remote-sync by default, ensure both builds work

**Estimated Time**: 1 day

#### 4.1: Update Cargo.toml

**File**: `Cargo.toml` (root)

- [ ] Find `[features]` section
- [ ] Update to enable by default:
  ```toml
  [features]
  default = ["remote-sync"]
  remote-sync = []
  ```
- [ ] Verify no other changes needed in dependencies

#### 4.2: Test Both Build Configurations

- [ ] Test default build: `cargo build --release`
- [ ] Verify remote sync works: Try parsing remote path
- [ ] Test minimal build: `cargo build --release --no-default-features`
- [ ] Verify compiles successfully (local-only)
- [ ] Verify remote paths give clear error in minimal build
- [ ] Measure binary sizes:
  - [ ] Default build: `ls -lh target/release/arsync`
  - [ ] Minimal build: Compare sizes
  - [ ] Document difference (~50-100KB expected)

#### 4.3: Update CI Configuration

**File**: `.github/workflows/ci.yml`

- [ ] Add job or step for minimal build:
  ```yaml
  - name: Test minimal build (no remote-sync)
    run: cargo build --no-default-features
  ```
- [ ] Ensure default build tests include remote-sync feature
- [ ] Verify both configurations tested on each PR

#### 4.4: Quality Checks - Phase 4

- [ ] `/build "release" "all" false` - Default build succeeds
- [ ] Build minimal: `cargo build --release --no-default-features`
- [ ] `/test "all"` - All tests pass with default features
- [ ] Verify minimal build compiles (tests may skip)
- [ ] Commit: `git commit -m "feat: enable remote-sync feature by default"`

**Acceptance Criteria**:
- [ ] `Cargo.toml` has `default = ["remote-sync"]`
- [ ] Default build includes remote sync
- [ ] Minimal build (--no-default-features) compiles and works (local-only)
- [ ] Binary size difference documented
- [ ] CI tests both configurations
- [ ] Clear error message when remote path used in minimal build

---

### Phase 5: Error Handling & Polish

**Objective**: Production-ready error messages and edge case handling

**Estimated Time**: 1-2 days

#### 5.1: Improve Error Messages

**Files**: `src/protocol/mod.rs`, `src/protocol/ssh.rs`

- [ ] SSH connection failures:
  - [ ] Clear error: "Failed to connect to {host}"
  - [ ] Suggest checking SSH keys
  - [ ] Suggest verifying host is reachable
- [ ] Remote arsync not found:
  - [ ] Error: "arsync not found on remote host {host}"
  - [ ] Suggest: "Install arsync on remote host or add to PATH"
- [ ] Permission denied:
  - [ ] Clear error with file path
  - [ ] Suggest checking remote permissions
- [ ] Version mismatch:
  - [ ] Show local and remote versions
  - [ ] Suggest upgrading older version

#### 5.2: Add Progress Reporting

**File**: `src/protocol/rsync_compat.rs` or wrapper

- [ ] Investigate: Can we show progress for remote transfers?
- [ ] Option 1: Extend rsync_compat to accept progress callback
- [ ] Option 2: Wrap in progress tracker at higher level
- [ ] If feasible: Add progress bar for remote sync
- [ ] If not: Document as future enhancement

#### 5.3: Handle Network Interruptions

- [ ] Test: Disconnect SSH during transfer
- [ ] Verify: Clean error message (not panic)
- [ ] Verify: Partial files cleaned up or marked incomplete
- [ ] Document behavior in code comments

#### 5.4: Validate --dry-run for Remote

**File**: `src/protocol/mod.rs`

- [ ] Check if `args.output.dry_run` is set
- [ ] For dry run with remote:
  - [ ] Connect to remote
  - [ ] Receive file list
  - [ ] Display what WOULD be transferred
  - [ ] Don't actually transfer files
  - [ ] Disconnect cleanly

#### 5.5: Quality Checks - Phase 5

- [ ] `/test "all"` - All tests pass
- [ ] Manual test: All error scenarios
- [ ] `/fmt false true` - Format code
- [ ] `/clippy false false` - Clean
- [ ] Commit: `git commit -m "feat(protocol): improve error handling and user experience"`

**Acceptance Criteria**:
- [ ] All error messages clear and actionable
- [ ] Network failures handled gracefully
- [ ] Dry-run works for remote paths (shows file list)
- [ ] Progress reporting explored (implemented or documented for future)
- [ ] No panics on error conditions

---

### Phase 6: Documentation & User Guide

**Objective**: Users know how to use remote sync

**Estimated Time**: 1 day

#### 6.1: Update README.md

**File**: `README.md` (root)

- [ ] Add "Remote Sync" section
- [ ] Add examples:
  ```bash
  # Pull from remote server
  arsync user@host:/var/www/html /local/backup
  
  # Push to remote server
  arsync -a /local/website user@host:/var/www/html
  ```
- [ ] Document SSH key requirements
- [ ] Explain both build modes (default vs minimal)
- [ ] Add troubleshooting section

#### 6.2: Create User Guide

**File**: `docs/REMOTE_SYNC_GUIDE.md` (new)

- [ ] Create comprehensive guide covering:
  - [ ] SSH key setup (ssh-keygen, ssh-copy-id)
  - [ ] First remote sync example
  - [ ] Common use cases
  - [ ] Metadata preservation over remote
  - [ ] Performance tips
  - [ ] Troubleshooting common issues

#### 6.3: Update CHANGELOG.md

**File**: `CHANGELOG.md`

- [ ] Add entry for remote sync feature:
  ```markdown
  ## [Unreleased]
  
  ### Added
  - **Remote sync support via SSH** - rsync-compatible remote file synchronization
    - Pull from remote: `arsync user@host:/remote /local`
    - Push to remote: `arsync /local user@host:/remote`
    - Full rsync wire protocol compatibility
    - Complete metadata preservation across network
    - Uses io_uring throughout for maximum performance
  ```

#### 6.4: Update Inline Documentation

**Files**: Protocol module files

- [ ] Update `src/protocol/mod.rs` module doc
- [ ] Update `src/protocol/rsync.rs` module doc  
- [ ] Update `src/protocol/ssh.rs` module doc
- [ ] Add examples showing remote sync usage
- [ ] Document feature flag requirement

#### 6.5: Quality Checks - Phase 6

- [ ] `/docs true false` - Verify docs build (if applicable)
- [ ] `/fmt false true` - Format all docs
- [ ] Review: All examples tested manually
- [ ] Commit: `git commit -m "docs: add remote sync user guide and examples"`

**Acceptance Criteria**:
- [ ] README has remote sync section with examples
- [ ] User guide exists and is comprehensive
- [ ] CHANGELOG updated
- [ ] SSH setup documented
- [ ] Troubleshooting guide complete
- [ ] All examples verified

---

## Final Phase: PR Preparation

### Pre-PR Verification

- [ ] `/fmt true true` - Verify formatting (check mode)
- [ ] `/clippy false false` - Verify no warnings
- [ ] `/test "all" --features remote-sync` - All tests pass with feature
- [ ] Test minimal build: `cargo build --no-default-features`
- [ ] `/build "release" "all" false` - Release build succeeds
- [ ] `/review` - Final review of all changes

### Testing Checklist

- [ ] All 106 existing protocol tests still pass
- [ ] New integration tests pass (3-5 tests expected)
- [ ] Manual testing complete:
  - [ ] Push to localhost via SSH
  - [ ] Pull from localhost via SSH
  - [ ] Error messages tested
  - [ ] Server mode tested
- [ ] Both build modes tested (default + minimal)

### PR Creation

- [ ] Commit all remaining changes
- [ ] Push branch: `git push origin feature/rsync-wire-protocol`
- [ ] Create PR or update existing PR
- [ ] Fill out PR template:
  - [ ] **Summary**: "Adds production-ready remote sync via SSH using complete rsync wire protocol"
  - [ ] **What changed**: Integration glue (~200 lines), server mode, docs
  - [ ] **Testing**: All 106+ tests passing, manual SSH testing complete
  - [ ] **Feature flag**: Enabled by default, opt-out with --no-default-features
  - [ ] **Dependencies**: Requires SSH client on system, arsync on remote
- [ ] `/pr-checks` - Monitor CI

### PR Body Checklist

- [ ] Summary of changes (wire up protocol → CLI)
- [ ] Examples showing remote sync usage
- [ ] Testing approach described
- [ ] Feature flag behavior explained (default enabled)
- [ ] SSH requirements documented
- [ ] Binary size impact noted (~50-100KB)
- [ ] Link to design doc

---

## Testing Strategy

### Unit Tests (Existing)

**No new unit tests needed** - Protocol layer has 106 tests

- Handshake: 14 unit + 5 pipe + 3 rsync integration
- File list: 7 varint + 14 format + 5 integration
- Checksum: 7 unit + 5 integration
- Delta: 8 tests
- End-to-end: 2 tests

### Integration Tests (New)

**File**: `tests/remote_sync_integration_test.rs`

Expected tests:
1. `test_push_to_localhost_via_ssh` - Push files to localhost
2. `test_pull_from_localhost_via_ssh` - Pull files from localhost
3. `test_server_mode_via_stdio` - Server mode with pipes
4. `test_remote_sync_errors` - Error conditions
5. (Optional) `test_metadata_preservation_remote` - Full metadata check

**Total new tests**: 3-5

### Manual Testing Checklist

- [ ] Push to real remote server (not localhost)
- [ ] Pull from real remote server
- [ ] Test with SSH keys
- [ ] Test with large files (>1GB)
- [ ] Test with many small files (>1000)
- [ ] Test with various metadata (xattrs, ACLs, etc.)
- [ ] Test error scenarios (connection lost, permission denied, etc.)
- [ ] Test --dry-run with remote paths
- [ ] Test --verbose with remote paths
- [ ] Test --progress with remote transfers

---

## Success Criteria

### Functional Requirements

- [ ] User can pull: `arsync user@host:/remote /local`
- [ ] User can push: `arsync /local user@host:/remote`
- [ ] All metadata preserved (permissions, times, ownership, xattrs)
- [ ] Works with SSH key authentication
- [ ] Works with custom SSH commands (`-e`, `--rsh`)
- [ ] Server mode works when invoked remotely
- [ ] Error messages clear and actionable

### Quality Requirements

- [ ] All existing tests still pass (106 tests)
- [ ] New integration tests pass (3-5 tests)
- [ ] No clippy warnings
- [ ] Code formatted
- [ ] Documentation complete

### Build Requirements

- [ ] Default build includes remote sync
- [ ] `--no-default-features` build works (local-only)
- [ ] Both tested in CI
- [ ] Binary size impact documented

### Performance (Not Blocking)

- [ ] Throughput reasonable (within 2x of rsync acceptable)
- [ ] No memory leaks
- [ ] Handles large files without issues
- [ ] Can defer optimization to future work

---

## Timeline Summary

| Phase | Task | Duration | Deliverable |
|-------|------|----------|-------------|
| 1 | Wire Integration | 1-2 days | rsync_compat → SSH working |
| 2 | Server Mode | 1 day | --server flag working |
| 3 | Integration Testing | 2-3 days | Remote sync validated |
| 4 | Feature Flag & CI | 1 day | Default enabled, CI updated |
| 5 | Error Handling | 1-2 days | Polish and UX |
| 6 | Documentation | 1 day | User guide complete |

**Total**: 7-10 days

---

## Risk Mitigation

### If SSH Issues Arise

- Protocol is complete and working (106 tests)
- Issue is likely in SSH wrapper or connection
- Use `/debug` to isolate SSH vs protocol issues
- Can test protocol with --pipe mode (already working)

### If Performance Issues

- Initial implementation focused on correctness
- Performance optimization can follow
- io_uring should provide good baseline performance
- Can profile and optimize in follow-up work

### If Compatibility Issues with Real rsync

- We've tested with rsync 3.4.1 in handshake integration test
- File list, checksum, delta all tested with rsync wire format
- End-to-end byte-for-byte verification passing
- Edge cases well-covered (106 tests)

---

## Notes

- This builds on complete protocol implementation - low risk
- Most work is integration glue, not new protocol code
- Comprehensive testing already exists (106 tests)
- Can iterate quickly with manual SSH testing
- Feature flag allows gradual rollout

---

**Ready to implement**: All protocol components complete, design approved, plan created.

