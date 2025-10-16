# Design: rsync-Compatible CLI Integration

**Status**: Design Phase  
**Date**: October 16, 2025  
**Purpose**: Wire up the complete rsync wire protocol implementation to provide rsync-compatible remote sync via CLI  
**Depends On**: `docs/projects/rsync-wire/` (protocol implementation - COMPLETE ✅)

---

## Table of Contents

1. [Executive Summary](#executive-summary)
2. [Current State Analysis](#current-state-analysis)
3. [Gap Analysis](#gap-analysis)
4. [Design](#design)
5. [CLI User Experience](#cli-user-experience)
6. [Implementation Approach](#implementation-approach)
7. [Testing Strategy](#testing-strategy)
8. [Risk Assessment](#risk-assessment)

---

## Executive Summary

### What We Have

The rsync wire protocol implementation (`docs/projects/rsync-wire/`) is **COMPLETE**:
- ✅ Handshake protocol (9-state FSM, capability negotiation, seed exchange)
- ✅ File list exchange (varint encoding, rsync format)
- ✅ Checksum algorithm (seeded rolling checksums, rsync wire format)
- ✅ Delta token handling (token stream encoding 0, 1-96, 97-255)
- ✅ End-to-end protocol flow (byte-for-byte verification)
- ✅ 106/106 tests passing
- ✅ 100% io_uring architecture

**Working Functions**:
- `protocol::rsync_compat::rsync_send_via_pipe()` - Complete sender implementation
- `protocol::rsync_compat::rsync_receive_via_pipe()` - Complete receiver implementation
- `protocol::handshake::handshake()` - Protocol negotiation
- `protocol::Location::parse()` - Parses `user@host:/path` syntax

**CLI Infrastructure**:
- ✅ `RemoteConfig` with `--pipe`, `--server`, `--daemon`, `--rsh` flags
- ✅ `Location` enum supporting `user@host:/path` parsing
- ✅ `main.rs` routing logic for remote vs local vs pipe modes

### What's Missing

The **integration glue** between protocol implementation and production usage:

1. **SSH Transport Integration** ❌
   - `protocol::rsync::push_via_rsync_protocol()` - Stub with TODO
   - `protocol::rsync::pull_via_rsync_protocol()` - Stub with TODO
   - Need to wire rsync_compat functions through SSH

2. **Server Mode** ❌
   - `--server` flag exists but not implemented
   - Need to handle being invoked by remote SSH

3. **Feature Flag** ⚠️
   - All protocol code is `#[cfg(feature = "remote-sync")]`
   - Not compiled by default
   - Need to enable for production builds

### Goal

Enable users to run:

```bash
# Pull from remote
arsync user@host:/remote/path /local/path

# Push to remote  
arsync /local/path user@host:/remote/path

# Works exactly like rsync!
```

**Internally**:
1. Parse remote path with `Location::parse()`
2. SSH to remote host via `SshConnection::connect()`
3. Start remote `arsync --server` process
4. Use rsync wire protocol over SSH (via `rsync_send_via_pipe`/`rsync_receive_via_pipe`)
5. Transfer files with full metadata preservation

---

## Current State Analysis

### What Works (Verified by Tests)

**Protocol Layer** (`src/protocol/rsync_compat.rs`):
- ✅ `rsync_send_via_pipe(args, path, transport)` - Complete
- ✅ `rsync_receive_via_pipe(args, transport, path)` - Complete
- ✅ Full protocol flow: handshake → file list → checksums → delta → reconstruction
- ✅ Tested with bidirectional pipes
- ✅ Byte-for-byte verification passing

**Transport Layer** (`src/protocol/`):
- ✅ `PipeTransport` - compio::fs::AsyncFd based
- ✅ `SshConnection` - compio::process based
- ✅ `Transport` trait - Generic async interface

**CLI Layer** (`src/cli.rs`, `src/main.rs`):
- ✅ `Location::parse()` - Handles `user@host:/path`
- ✅ `RemoteConfig` - All flags defined
- ✅ Routing in `main.rs` - Checks for remote, routes to `protocol::remote_sync()`

### What's Stubbed

**Integration Functions** (`src/protocol/rsync.rs`):
```rust
pub async fn push_via_rsync_protocol(...) -> Result<SyncStats> {
    // TODO: Implement rsync protocol
    anyhow::bail!("rsync protocol implementation in progress")
}

pub async fn pull_via_rsync_protocol(...) -> Result<SyncStats> {
    // TODO: Implement rsync protocol (receiver side)
    anyhow::bail!("rsync protocol implementation in progress")
}
```

**High-Level Entry Points** (`src/protocol/mod.rs`):
```rust
pub async fn remote_sync(args, source, destination) -> Result<SyncStats> {
    match (source, destination) {
        (Local(src), Remote{..}) => push_to_remote(...).await,  // Calls push_via_rsync_protocol
        (Remote{..}, Local(dest)) => pull_from_remote(...).await, // Calls pull_via_rsync_protocol
        ...
    }
}
```

**Server Mode**:
- Flag exists: `args.remote.server`
- Not checked in `main.rs` - no routing for server mode
- Server needs to:
  - Run as receiver when invoked with `--server --sender`  
  - Run as sender when invoked with `--server` (no --sender)
  - Communicate via stdin/stdout (SSH will connect these)

---

## Gap Analysis

### Missing Components

1. **Wire up rsync_compat → rsync.rs** 
   - `push_via_rsync_protocol()` needs to call `rsync_send_via_pipe()`
   - `pull_via_rsync_protocol()` needs to call `rsync_receive_via_pipe()`
   - Adapt SSH connection to look like PipeTransport

2. **Implement --server mode**
   - Detect `--server` flag in main.rs
   - Determine role (sender vs receiver) from arguments
   - Create PipeTransport from stdin/stdout
   - Call appropriate rsync_compat function
   - Exit when complete

3. **SSH Connection as Transport**
   - `SshConnection` has `stdin: ChildStdin` and `stdout: ChildStdout`
   - These implement AsyncRead/AsyncWrite
   - Need Transport impl or wrapper

4. **Feature Flag Strategy**
   - Enable `remote-sync` by default (opt-out pattern)
   - Users can disable with `--no-default-features` for minimal builds
   - CI tests both configurations (with and without feature)

5. **Error Handling**
   - SSH connection failures
   - Remote arsync not found
   - Version incompatibility
   - Network errors during transfer

### Already Complete (Don't Need to Implement)

- ✅ Protocol implementation (all phases 1-7 complete)
- ✅ Location parsing (`user@host:/path`)
- ✅ CLI flag definitions
- ✅ Main routing logic structure
- ✅ Comprehensive test suite (106 tests)

---

## Design

### Architecture Overview

```
User runs: arsync /local user@host:/remote

main.rs
  ↓ Parse args
  ↓ Detect: destination.is_remote()
  ↓
protocol::remote_sync(&args, &source, &destination)
  ↓ Match on (Local, Remote)
  ↓
protocol::push_to_remote(args, local_path, user, host, remote_path)
  ↓ 1. SSH connect
  ↓ 2. Start remote "arsync --server"
  ↓ 3. Wrap SSH stdio as Transport
  ↓
protocol::rsync::push_via_rsync_protocol(args, local_path, connection)
  ↓ Wrap SshConnection stdin/stdout
  ↓
protocol::rsync_compat::rsync_send_via_pipe(args, local_path, transport)
  ↓ Execute complete protocol:
  ✅ Handshake
  ✅ Send file list
  ✅ Receive checksums
  ✅ Generate delta
  ✅ Send delta
  ✅ Complete!
```

### Server Mode Flow

```
Remote SSH invokes: arsync --server /remote/path

main.rs
  ↓ Parse args
  ↓ Detect: args.remote.server == true
  ↓ Determine role from flags
  ↓
protocol::server_mode(&args, &destination)
  ↓ Create PipeTransport from stdin/stdout
  ↓ Detect role: server mode is receiver by default
  ↓
protocol::rsync_compat::rsync_receive_via_pipe(args, transport, dest_path)
  ✅ Handshake
  ✅ Receive file list
  ✅ Send checksums
  ✅ Receive delta
  ✅ Reconstruct files
  ✅ Complete!
```

---

## CLI User Experience

### Syntax (rsync-compatible)

```bash
# Pull from remote (download)
arsync user@host:/remote/source /local/dest
arsync host:/remote/source /local/dest           # User defaults to current

# Push to remote (upload)
arsync /local/source user@host:/remote/dest
arsync /local/source host:/remote/dest

# Local copy (existing behavior, io_uring direct)
arsync /local/source /local/dest

# With metadata preservation (rsync-compatible flags already exist)
arsync -a user@host:/remote /local              # Archive mode
arsync -avz user@host:/remote /local            # Archive + verbose (+ compress TBD)
arsync --perms --times user@host:/remote /local # Explicit flags
```

### Advanced Options

```bash
# Custom SSH command
arsync -e "ssh -p 2222" user@host:/remote /local

# Different remote shell
arsync --rsh="ssh -i ~/.ssh/custom_key" user@host:/remote /local

# Dry run (see what would be transferred)
arsync -n user@host:/remote /local

# Verbose progress
arsync -av --progress user@host:/remote /local
```

### Server Mode (Invoked by SSH)

```bash
# User never types this - SSH invokes it automatically
arsync --server /remote/path

# Internal flags (hidden from help)
--server         # Server mode (invoked by remote SSH)
--daemon         # Daemon mode (future: rsyncd compatibility)
```

### Testing Mode (Already Working)

```bash
# Pipe mode for protocol testing (hidden flag, already works)
arsync --pipe --pipe-role=sender /source | \
    arsync --pipe --pipe-role=receiver /dest

# With rsync compatibility
arsync --pipe --pipe-role=sender --rsync-compat /source | \
    rsync --server /dest
```

---

## Implementation Approach

### Component 1: Wire rsync.rs Integration Functions

**File**: `src/protocol/rsync.rs`

**Current**:
```rust
pub async fn push_via_rsync_protocol(
    _args: &Args,
    _local_path: &Path,
    _connection: &mut SshConnection,
) -> Result<SyncStats> {
    // TODO: Implement rsync protocol
    anyhow::bail!("rsync protocol implementation in progress")
}
```

**Implementation**:
```rust
pub async fn push_via_rsync_protocol(
    args: &Args,
    local_path: &Path,
    connection: &mut SshConnection,
) -> Result<SyncStats> {
    // Create transport wrapper for SSH connection
    let transport = SshTransport::new(connection);
    
    // Use complete rsync_compat implementation
    rsync_compat::rsync_send_via_pipe(args, local_path, transport).await
}
```

**Key Design Decision**: Create `SshTransport` wrapper that implements `Transport` trait.

**Alternative**: Make `SshConnection` directly implement `Transport` (cleaner but requires refactoring).

### Component 2: SshTransport Wrapper

**File**: `src/protocol/ssh.rs` (new struct)

**Purpose**: Adapt `SshConnection` (has stdin/stdout) to `Transport` trait.

**Design**:
```rust
pub struct SshTransport<'a> {
    connection: &'a mut SshConnection,
}

impl<'a> SshTransport<'a> {
    pub fn new(connection: &'a mut SshConnection) -> Self {
        Self { connection }
    }
}

impl AsyncRead for SshTransport<'_> {
    // Delegate to connection.stdout
}

impl AsyncWrite for SshTransport<'_> {
    // Delegate to connection.stdin  
}

impl Transport for SshTransport<'_> {}
```

**Alternative Design**: Modify `SshConnection` to directly implement `Transport`
- Pros: Cleaner, no wrapper
- Cons: More invasive change
- **Recommendation**: Start with wrapper, refactor later if needed

### Component 3: Server Mode Implementation

**File**: `src/main.rs`

**Current Flow**:
```rust
let result = if args.remote.pipe {
    // Pipe mode (testing)
} else if source.is_remote() || destination.is_remote() {
    // Remote sync
} else {
    // Local sync
};
```

**Add Server Mode**:
```rust
let result = if args.remote.server {
    // ============================================================
    // SERVER MODE (invoked by remote SSH)
    // ============================================================
    protocol::server_mode(&args).await
    
} else if args.remote.pipe {
    // Pipe mode (testing)
} else if source.is_remote() || destination.is_remote() {
    // Remote sync
} else {
    // Local sync
};
```

**Server Mode Function** (`src/protocol/mod.rs`):
```rust
#[cfg(feature = "remote-sync")]
pub async fn server_mode(args: &Args) -> Result<SyncStats> {
    // Create PipeTransport from stdin/stdout
    let transport = pipe::PipeTransport::from_stdio()?;
    
    // Server mode is always receiver in rsync protocol
    // (Remote client initiates and sends)
    let dest_path = args.get_destination()?;
    let Location::Local(dest) = dest_path else {
        anyhow::bail!("Server mode requires local destination path");
    };
    
    // Use rsync_compat receiver
    rsync_compat::rsync_receive_via_pipe(args, transport, &dest).await
}
```

### Component 4: Feature Flag Strategy

**Decision**: Enable `remote-sync` by **default** (opt-out pattern)

```toml
# Cargo.toml
[features]
default = ["remote-sync"]  # Enabled by default
remote-sync = []
```

**Rationale**:
- **User expectations**: CLI accepts `user@host:/path`, so it should work
- **rsync compatibility**: rsync has remote sync by default
- **Minimal cost**: ~50-100KB is negligible for the functionality
- **Better UX**: Most users will want remote sync eventually
- **Matches syntax**: If we parse remote paths, we should support them

**User Experience**:
```bash
# Default build (includes remote sync)
cargo build --release

# Minimal build (local-only, opt-out)
cargo build --release --no-default-features

# Install default (with remote sync)
cargo install --path .

# Install minimal
cargo install --path . --no-default-features
```

**Error Handling**:
When user tries remote path in minimal build:
```
Error: Remote sync not supported in this build
Hint: This binary was compiled without remote sync support.
      Recompile with default features or add --features remote-sync
      cargo build --release
```

**CI Strategy**:
- Test both configurations:
  - Default build (with remote-sync) ← Primary test path
  - Minimal build (--no-default-features) ← Verify local-only works
- Ensures both paths remain working

**For Library Users**:
Projects depending on arsync can opt-out in their Cargo.toml:
```toml
[dependencies]
arsync = { version = "0.4", default-features = false }
```

---

## CLI User Experience

### Use Cases

#### Use Case 1: Pull from Remote Server

**User runs**:
```bash
arsync user@webserver:/var/www/html /local/backup
```

**What happens**:
1. Parse: `user@webserver:/var/www/html` → `Location::Remote`
2. Detect: `destination.is_remote() == false`, `source.is_remote() == true`
3. Route to: `protocol::pull_from_remote()`
4. SSH connect: `ssh user@webserver arsync --server`
5. Remote server runs: `arsync --server /var/www/html`
6. Protocol: Client (local) ← Server (remote) sends files
7. Files written to `/local/backup/`

**Expected output**:
```
Starting arsync v0.4.0
Source: user@webserver:/var/www/html (remote)
Destination: /local/backup (local)
Connecting to webserver...
Connected. Starting remote arsync...
Handshake complete (protocol v31)
Receiving file list... 142 files
Generating checksums...
Receiving deltas...
Reconstructing files... 142/142
Complete!
Files completed: 142
Bytes completed: 15728640
Duration: 2.3s
```

#### Use Case 2: Push to Remote Server

**User runs**:
```bash
arsync -a /local/source user@webserver:/var/www/html
```

**What happens**:
1. Parse: `user@webserver:/var/www/html` → `Location::Remote`
2. Detect: `destination.is_remote() == true`, `source.is_remote() == false`
3. Route to: `protocol::push_to_remote()`
4. SSH connect: `ssh user@webserver arsync --server`
5. Remote server runs: `arsync --server /var/www/html`
6. Protocol: Client (local) → Server (remote) receives files
7. Files written to remote `/var/www/html/`

#### Use Case 3: Local Copy (Existing Behavior)

**User runs**:
```bash
arsync -a /source /dest
```

**What happens**:
1. Parse: Both paths are `Location::Local`
2. Route to: `sync::sync_files()` (existing code)
3. Direct io_uring copy (no protocol overhead)
4. **Fastest path** - no SSH, no protocol encoding

---

## Implementation Approach

### Phase 1: Wire Up Integration Functions

**Goal**: Connect rsync_compat to SSH transport

**Tasks**:
1. Implement `SshTransport` wrapper in `src/protocol/ssh.rs`
2. Update `push_via_rsync_protocol()` in `src/protocol/rsync.rs`
3. Update `pull_via_rsync_protocol()` in `src/protocol/rsync.rs`
4. Add error handling for SSH failures

**Acceptance Criteria**:
- [ ] `push_via_rsync_protocol()` calls `rsync_send_via_pipe()` with SSH transport
- [ ] `pull_via_rsync_protocol()` calls `rsync_receive_via_pipe()` with SSH transport
- [ ] Compiles with no errors
- [ ] No functional tests yet (testing in Phase 3)

### Phase 2: Implement Server Mode

**Goal**: Handle being invoked as `--server` by remote SSH

**Tasks**:
1. Add `server_mode()` function to `src/protocol/mod.rs`
2. Update routing in `src/main.rs` to check for `args.remote.server`
3. Create PipeTransport from stdin/stdout in server mode
4. Call `rsync_receive_via_pipe()` (server is always receiver)
5. Handle logging to stderr (not stdout - protocol uses stdout!)

**Acceptance Criteria**:
- [ ] `--server` mode routes to `protocol::server_mode()`
- [ ] Creates PipeTransport from stdio
- [ ] Calls rsync_receive_via_pipe
- [ ] Logs go to stderr only
- [ ] Compiles and runs (functional testing in Phase 3)

### Phase 3: End-to-End Testing

**Goal**: Validate actual remote sync works

**Test 1: Manual SSH Test**
```bash
# Terminal 1 (server):
$ arsync --server /tmp/dest

# Terminal 2 (client):  
$ arsync --pipe --pipe-role=sender --rsync-compat /tmp/source | \
  ssh localhost 'arsync --server /tmp/dest'
```

**Test 2: Full Remote Sync (Localhost)**
```bash
# Should work via SSH to localhost
$ arsync /tmp/source localhost:/tmp/dest

# Should invoke:
# ssh localhost arsync --server /tmp/dest
# Then transfer files via protocol
```

**Test 3: Automated Integration Test**
```rust
// tests/remote_sync_integration_test.rs
#[compio::test]
async fn test_remote_sync_via_localhost() {
    // Spawn local arsync --server via SSH
    // Transfer test files
    // Verify all files transferred correctly
    // Verify metadata preserved
}
```

**Acceptance Criteria**:
- [ ] Can transfer files to localhost via SSH
- [ ] All files transferred correctly
- [ ] Metadata preserved (permissions, times, etc.)
- [ ] No hangs or deadlocks
- [ ] Error handling works (connection failures, etc.)

### Phase 4: Feature Flag & Build Configuration

**Goal**: Enable remote-sync by default, support minimal builds via opt-out

**Tasks**:
1. Update `Cargo.toml` to include `remote-sync` in default features
2. Update CI to test both configurations:
   - Default build (with remote-sync) ← Primary
   - Minimal build (--no-default-features, local-only)
3. Add clear error messages when remote path used in minimal build
4. Document how to build minimal version in README
5. Measure binary size difference

**Acceptance Criteria**:
- [ ] Default `cargo build` includes remote sync
- [ ] `cargo build --no-default-features` produces minimal local-only binary
- [ ] Both configurations tested in CI
- [ ] Clear error when remote path used in minimal build
- [ ] Binary size difference documented (~50-100KB)
- [ ] README explains both build modes
- [ ] Default behavior matches user expectations (remote paths work)

### Phase 5: Error Handling & Polish

**Goal**: Production-ready error messages and edge cases

**Tasks**:
1. Handle "arsync not found on remote" gracefully
2. Handle "SSH connection refused"
3. Handle "permission denied" on remote
4. Handle network interruptions
5. Add progress reporting for remote transfers
6. Add `--dry-run` support for remote (show what would transfer)

**Acceptance Criteria**:
- [ ] Clear error messages for common failures
- [ ] Graceful degradation (connection lost, etc.)
- [ ] Progress bar works for remote transfers
- [ ] Dry run shows remote file list
- [ ] Help message updated with remote examples

### Phase 6: Documentation & User Guide

**Goal**: Users know how to use remote sync

**Tasks**:
1. Update README.md with remote sync examples
2. Add "Remote Sync Guide" to docs/
3. Document SSH key setup requirements
4. Add troubleshooting section
5. Document performance characteristics vs rsync

**Acceptance Criteria**:
- [ ] README has remote sync examples
- [ ] User guide covers common scenarios
- [ ] SSH setup documented
- [ ] Troubleshooting guide exists
- [ ] Performance benchmarks vs rsync

---

## Testing Strategy

### Unit Tests

**File**: `tests/server_mode_test.rs` (new)
- [ ] Test server mode detection
- [ ] Test stdin/stdout → PipeTransport conversion
- [ ] Test server mode routing logic
- [ ] Test error handling (invalid args, etc.)

### Integration Tests

**File**: `tests/remote_sync_integration_test.rs` (new)
- [ ] Test push to localhost via SSH
- [ ] Test pull from localhost via SSH
- [ ] Test metadata preservation across SSH
- [ ] Test error conditions (connection failures, etc.)
- [ ] Test with actual SSH (requires SSH server running)

**File**: `tests/rsync_cli_compat_test.rs` (new)
- [ ] Test CLI parsing of remote paths
- [ ] Test Location::parse edge cases
- [ ] Test --server flag handling
- [ ] Test --rsh flag (custom SSH command)

### Docker-Based Integration Testing

**Approach**: Use Docker containers to create isolated SSH environments for automated testing

#### Test Infrastructure Design

**Container Setup**:
```dockerfile
# tests/docker/Dockerfile
FROM rust:latest

# Install SSH server
RUN apt-get update && apt-get install -y openssh-server

# Create test user
RUN useradd -m -s /bin/bash testuser && \
    echo "testuser:testpass" | chpasswd

# Configure SSH
RUN mkdir /var/run/sshd && \
    ssh-keygen -A && \
    sed -i 's/#PasswordAuthentication yes/PasswordAuthentication yes/' /etc/ssh/sshd_config

# Copy arsync binary into container
COPY target/debug/arsync /usr/local/bin/arsync
RUN chmod +x /usr/local/bin/arsync

# Setup test directories
RUN mkdir -p /home/testuser/source /home/testuser/dest && \
    chown -R testuser:testuser /home/testuser

EXPOSE 22
CMD ["/usr/sbin/sshd", "-D"]
```

**Test Helper** (`tests/docker/mod.rs`):
```rust
use std::process::Command;
use anyhow::Result;

pub struct DockerContainer {
    container_id: String,
    ssh_port: u16,
}

impl DockerContainer {
    pub async fn start() -> Result<Self> {
        // Build Docker image
        Command::new("docker")
            .args(&["build", "-t", "arsync-test", "tests/docker"])
            .status()?;
        
        // Run container with SSH exposed
        let output = Command::new("docker")
            .args(&["run", "-d", "-P", "arsync-test"])
            .output()?;
        
        let container_id = String::from_utf8(output.stdout)?.trim().to_string();
        
        // Get mapped SSH port
        let port_output = Command::new("docker")
            .args(&["port", &container_id, "22"])
            .output()?;
        
        let port_str = String::from_utf8(port_output.stdout)?;
        let ssh_port = port_str
            .split(':')
            .last()
            .and_then(|p| p.trim().parse().ok())
            .ok_or_else(|| anyhow::anyhow!("Failed to parse SSH port"))?;
        
        Ok(Self { container_id, ssh_port })
    }
    
    pub fn ssh_connection_string(&self) -> String {
        format!("localhost:{}", self.ssh_port)
    }
}

impl Drop for DockerContainer {
    fn drop(&mut self) {
        let _ = Command::new("docker")
            .args(&["rm", "-f", &self.container_id])
            .status();
    }
}
```

**Integration Test** (`tests/docker_remote_sync_test.rs`):
```rust
#![cfg(feature = "remote-sync")]

use arsync::protocol::Location;
use std::fs;
use std::path::Path;

mod docker;

#[compio::test]
async fn test_push_via_docker_ssh() -> anyhow::Result<()> {
    // Start Docker container with SSH
    let container = docker::DockerContainer::start().await?;
    
    // Wait for SSH to be ready
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    // Create test data locally
    let temp_dir = tempfile::tempdir()?;
    let source = temp_dir.path().join("source");
    fs::create_dir(&source)?;
    fs::write(source.join("test.txt"), "Hello, Docker!")?;
    
    // Push to container via SSH
    let remote_path = format!("testuser@localhost:{}/dest", container.ssh_port);
    let source_loc = Location::Local(source.clone());
    let dest_loc = Location::parse(&remote_path)?;
    
    // TODO: Call arsync with SSH
    // This would test the full remote sync flow
    
    Ok(())
}
```

**Benefits**:
- ✅ **Isolated environment**: No pollution of developer's system
- ✅ **Reproducible**: Same container every time
- ✅ **CI-friendly**: Can run in GitHub Actions
- ✅ **Fast**: Docker startup ~2-3 seconds
- ✅ **Realistic**: Uses actual SSH protocol
- ✅ **Automated**: No manual setup required

**Challenges**:
- ⚠️ Requires Docker installed (document in test README)
- ⚠️ May be slow in CI (can use Docker layer caching)
- ⚠️ Need to build arsync binary first

**Alternative: No Docker** (Simpler but less isolated):
```rust
// Just test with localhost SSH (requires SSH server running locally)
#[compio::test]
#[ignore] // Only run when SSH is available
async fn test_push_to_localhost() {
    if !ssh_available() {
        println!("Skipping: SSH not available");
        return;
    }
    
    // Test with localhost SSH
    // Requires user has SSH keys configured for localhost
}
```

**Recommendation**: 
- Start with **localhost SSH tests** (simpler, no Docker dep)
- Add **Docker tests** later for CI/CD isolation
- Mark Docker tests as optional or feature-gated

### Manual Testing

**Scenarios**:
1. Transfer to remote server (real SSH)
2. Transfer from remote server
3. Handle SSH key authentication
4. Handle password authentication (if prompted)
5. Handle connection timeouts
6. Handle remote arsync not installed
7. Transfer large files (>1GB)
8. Transfer many small files (>10k)

---

## Implementation Timeline

### Phase 1: Wire Up Integration (1-2 days)
- Implement SshTransport wrapper
- Wire rsync_compat to rsync.rs functions
- Compiles cleanly

### Phase 2: Server Mode (1 day)
- Implement server_mode() function
- Update main.rs routing
- Manual testing with --server flag

### Phase 3: End-to-End Testing (2-3 days)
- Create integration tests
- Test localhost SSH transfers
- Validate metadata preservation
- Debug any issues found

### Phase 4: Build Configuration & CI (1 day)
- Enable remote-sync by default in Cargo.toml
- Update CI to test both configurations (default + minimal)
- Document opt-out for minimal builds

### Phase 5: Error Handling (1-2 days)
- Add comprehensive error handling
- Test failure scenarios
- Polish error messages

### Phase 6: Documentation (1 day)
- Update README
- Create user guide
- Add examples

**Total Estimated Time**: 7-10 days

---

## Risk Assessment

### High Risk

#### 1. SSH Connection Stability
- **Risk**: Network errors, connection drops during transfer
- **Mitigation**: Proper error handling, connection state validation
- **Fallback**: Clear error messages, user can retry

#### 2. Remote arsync Not Installed
- **Risk**: SSH connects but `arsync --server` not found
- **Mitigation**: Check remote PATH, provide clear error message
- **Fallback**: Documentation explains installation requirement

### Medium Risk

#### 3. Protocol Version Mismatch
- **Risk**: Local arsync v0.4, remote arsync v0.3
- **Mitigation**: Handshake negotiates compatible version (already implemented!)
- **Fallback**: Protocol supports version negotiation

#### 4. Metadata Preservation Across Systems
- **Risk**: Different file systems, permission models
- **Mitigation**: Use existing metadata preservation code (already tested locally)
- **Fallback**: Document limitations (e.g., no ACLs on systems without ACL support)

### Low Risk

#### 5. Performance vs rsync
- **Risk**: Slower than rsync
- **Mitigation**: Use io_uring throughout, same delta algorithm
- **Expected**: Similar or better performance (io_uring is faster)

#### 6. SSH Authentication Issues
- **Risk**: SSH keys not configured, password prompts
- **Mitigation**: SSH handles this (we just spawn ssh command)
- **Fallback**: Standard SSH documentation applies

---

## Success Criteria

### Functional Requirements

- [ ] User can pull files from remote: `arsync user@host:/remote /local`
- [ ] User can push files to remote: `arsync /local user@host:/remote`
- [ ] All metadata preserved (permissions, times, ownership, xattrs)
- [ ] Works with SSH key authentication
- [ ] Works with custom SSH commands (`-e`, `--rsh`)
- [ ] Server mode works when invoked remotely
- [ ] Error messages are clear and actionable
- [ ] Progress reporting works for remote transfers

### Performance Requirements

- [ ] Throughput within 20% of rsync (ideally better with io_uring)
- [ ] Handles large files (>10GB) without memory issues
- [ ] Handles many small files (>100k) efficiently
- [ ] No memory leaks or resource exhaustion

### Quality Requirements

- [ ] 90%+ test coverage for new code
- [ ] All integration tests passing
- [ ] No clippy warnings
- [ ] No new security vulnerabilities
- [ ] Documentation complete and accurate

---

## Open Questions

1. **Default Compression**: Should we support `-z` compression like rsync?
   - Protocol has infrastructure (can add later)
   - May impact performance
   - **Decision**: Defer to future enhancement

2. **Bandwidth Limiting**: Support `--bwlimit` flag?
   - rsync has this for WAN transfers
   - io_uring makes this harder
   - **Decision**: Defer to future enhancement

3. **Partial Transfers**: Support `--partial` (resume interrupted transfers)?
   - Requires state management
   - Adds complexity
   - **Decision**: Defer to future enhancement

4. **rsync Daemon Mode**: Support rsyncd protocol (daemon mode)?
   - Different from SSH mode
   - More complex authentication
   - **Decision**: Defer, focus on SSH first

---

## Dependencies

### Internal (arsync codebase)

- ✅ `src/protocol/rsync_compat.rs` - Complete implementation (Phase 1-7)
- ✅ `src/protocol/handshake.rs` - Complete (106 tests passing)
- ✅ `src/protocol/ssh.rs` - SSH connection via compio
- ✅ `src/protocol/transport.rs` - Transport trait
- ⏳ `src/protocol/rsync.rs` - Needs integration functions (this project)

### External Dependencies

- ✅ `compio` - For io_uring async runtime
- ✅ `compio::process` - For SSH process spawning
- ✅ `whoami` - For default username (already in Cargo.toml)
- ✅ `anyhow` - For error handling
- ⚠️ SSH client - Required on system (document this)

---

## Out of Scope (Future Enhancements)

These are explicitly **NOT** part of this project:

- ❌ Compression (`-z` flag)
- ❌ Bandwidth limiting (`--bwlimit`)
- ❌ Partial transfers (`--partial`)
- ❌ Daemon mode (`rsyncd` protocol)
- ❌ QUIC transport (already stubbed for future)
- ❌ Incremental file lists (protocol supports, but not needed yet)
- ❌ Batch mode optimizations
- ❌ ACL translation between different file systems
- ❌ Extended attribute translation
- ❌ Hardlink optimization across network

These can be added later without architectural changes.

---

## Conclusion

This design wires up the **complete** rsync wire protocol implementation to provide
production-ready remote sync via CLI. The protocol implementation (106 tests, all
passing) is ready to use - we just need the integration glue.

**Key Advantages**:
1. **Protocol complete** - All hard work done, just need integration
2. **io_uring throughout** - High performance maintained
3. **Minimal new code** - ~200 lines of integration glue
4. **Low risk** - Protocol thoroughly tested
5. **rsync-compatible** - Drop-in replacement for basic use cases

**Next Steps**:
1. Review this design
2. Create implementation plan (`/plan`)
3. Begin Phase 1 implementation
4. Complete in ~1-2 weeks

**Estimated Effort**: 7-10 days for full production-ready remote sync.

