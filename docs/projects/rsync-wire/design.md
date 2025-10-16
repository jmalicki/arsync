# Detailed Design: rsync Wire Protocol Compatibility

**Status**: Design Document  
**Date**: October 9, 2025  
**Purpose**: Complete specification for implementing rsync wire protocol compatibility in arsync

---

## Table of Contents

1. [Executive Summary](#executive-summary)
2. [Part 1: Handshake Protocol](#part-1-handshake-protocol)
3. [Part 2: compio/io_uring Integration](#part-2-compioio_uring-integration)
4. [Part 3: Checksum Exchange Abstraction](#part-3-checksum-exchange-abstraction)
5. [Part 4: Delta Token Handling](#part-4-delta-token-handling)
6. [Implementation Timeline](#implementation-timeline)
7. [Risk Assessment](#risk-assessment)

---

## Executive Summary

This document provides a complete design for implementing rsync wire protocol compatibility in arsync. The design is split into four major components:

1. **Handshake Protocol**: Complete rsync protocol negotiation sequence
2. **compio Integration**: Replace tokio with compio for io_uring-based async I/O
3. **Checksum Abstraction**: Unified interface for both rsync and arsync checksums
4. **Delta Tokens**: Handle rsync's token format vs arsync's native format

**Goal**: Enable `arsync` to act as a drop-in replacement for `rsync` in client/server scenarios while maintaining native performance for arsync-to-arsync transfers.

**Current Status**: Phase 3 of 6 (file list format complete, handshake incomplete)

---

# Part 1: Handshake Protocol

## Overview

The rsync handshake is a multi-phase protocol that establishes:
1. Protocol version compatibility
2. Capability negotiation (what features both sides support)
3. Random seed exchange (for checksum algorithms)
4. Exclusion/inclusion patterns
5. Multiplexing mode activation

## 1.1 arsync's Existing Metadata Capabilities

**Important Note**: arsync ALREADY has full local support for:
- ✅ **Extended attributes** (`-X/--xattrs`) - See `tests/file_xattr_tests.rs`, `tests/directory_xattr_tests.rs`
- ✅ **POSIX ACLs** (`-A/--acls`) - Full implementation in `src/copy.rs` and `src/directory.rs`
- ✅ **Hard links** (`-H/--hard-links`) - Tracked and preserved
- ✅ **Device files** (`-D/--devices`) - Special files supported
- ✅ **Symlinks** (`-l/--links`) - Copy as symlinks
- ✅ **Permissions** (`-p/--perms`) - Full mode preservation
- ✅ **Timestamps** (`-t/--times`) - Modification times
- ✅ **Access times** (`-U/--atimes`) - Use times
- ✅ **Creation times** (`--crtimes`) - Birth times (when supported)
- ✅ **Owner/Group** (`-o/-g`) - UID/GID preservation

**What this means for rsync protocol**: We already know HOW to preserve all this metadata locally. The wire protocol implementation just needs to:
1. **Transmit** these attributes in rsync's file list format
2. **Receive** these attributes from rsync servers  
3. **Apply** them using our existing local functions

This is MUCH easier than if we had to implement metadata preservation from scratch!

## 1.1 Protocol Sequence

### Phase 1.1: Initial Version Exchange

**Direction**: Bidirectional (simultaneous)

```
Client → Server: [u8 protocol_version]  (e.g., 31)
Server → Client: [u8 protocol_version]  (e.g., 31)
```

**Details**:
- NOT multiplexed - raw byte on the wire
- Both sides send their version simultaneously
- Effective version = min(client_version, server_version)
- Common versions: 27-31 (rsync 3.x)

**Implementation Location**: `src/protocol/rsync_compat.rs::handshake_phase1()`

**State Machine**:
```rust
enum HandshakeState {
    Initial,
    VersionSent,
    VersionReceived { remote_version: u8 },
    VersionNegotiated { protocol_version: u8 },
    // ... more states
}
```

### Phase 1.2: Compatibility Flags Exchange

**Direction**: Bidirectional

```
Client → Server: [u32 flags] (varint encoded)
Server → Client: [u32 flags] (varint encoded)
```

**Flag Bits** (from rsync protocol.h):
```rust
const XMIT_CHECKSUMS: u32    = 1 << 0;  // Checksum algorithm support
const XMIT_HARDLINKS: u32    = 1 << 1;  // Hardlink support
const XMIT_SYMLINKS: u32     = 1 << 2;  // Symlink support
const XMIT_DEVICES: u32      = 1 << 3;  // Device file support
const XMIT_XATTRS: u32       = 1 << 4;  // Extended attributes
const XMIT_ACLS: u32         = 1 << 5;  // ACLs
const XMIT_SPARSE: u32       = 1 << 6;  // Sparse file optimization
const XMIT_CHECKSUM_SEED: u32 = 1 << 7; // Use checksum seed
const XMIT_PROTECTION: u32   = 1 << 8;  // Preserve permissions
const XMIT_TIMES: u32        = 1 << 9;  // Preserve times
```

**Implementation Strategy**:
```rust
struct ProtocolCapabilities {
    version: u8,
    flags: u32,
    checksum_seed: Option<u32>,
    compression: Option<CompressionType>,
}

impl ProtocolCapabilities {
    fn negotiate(client: Self, server: Self) -> Self {
        Self {
            version: client.version.min(server.version),
            flags: client.flags & server.flags, // Intersection
            // ... negotiate other fields
        }
    }
}
```

### Phase 1.3: Seed Exchange (Optional)

**Condition**: If `XMIT_CHECKSUM_SEED` flag is set

```
Sender → Receiver: [u32 checksum_seed] (4 bytes, little-endian)
```

**Purpose**: 
- Randomizes checksums to prevent birthday attacks
- Used as initial state for rolling checksum algorithm
- Critical for security in untrusted environments

**Implementation**:
```rust
struct ChecksumSeed {
    seed: u32,
}

impl ChecksumSeed {
    fn generate() -> Self {
        use rand::Rng;
        Self {
            seed: rand::thread_rng().gen(),
        }
    }
    
    fn apply_to_checksum(&self, data: &[u8]) -> u32 {
        // Mix seed into checksum calculation
        rolling_checksum_with_seed(data, self.seed)
    }
}
```

### Phase 1.4: Filter Rules Exchange (Optional)

**Condition**: If client sends exclude/include patterns

```
Client → Server: [list of filter rules]
Format per rule:
  - [u8 rule_type]  ('+' = include, '-' = exclude, etc.)
  - [varint len]
  - [len bytes pattern]
  - [u8 terminator] (0x00)
End: [u8 0x00] (empty rule)
```

**Implementation**:
```rust
enum FilterRule {
    Include(String),  // '+ pattern'
    Exclude(String),  // '- pattern'
    Clear,            // '!'
    ClearIncludes,    // '!'
}

struct FilterList {
    rules: Vec<FilterRule>,
}

impl FilterList {
    async fn send<T: Transport>(&self, transport: &mut T) -> Result<()> {
        for rule in &self.rules {
            match rule {
                FilterRule::Include(pat) => {
                    transport.write(&[b'+']).await?;
                    encode_varint_into(pat.len() as u64, &mut buf);
                    transport.write(&buf).await?;
                    transport.write(pat.as_bytes()).await?;
                    transport.write(&[0]).await?;
                }
                // ... other types
            }
        }
        transport.write(&[0]).await?; // End marker
        Ok(())
    }
}
```

### Phase 1.5: Multiplexing Activation

**Condition**: After version >= 27

From this point forward, ALL messages are multiplexed with tags:
```
[u8 tag][u8 len_low][u8 len_mid][u8 len_high][data...]
```

**State Transition**:
```rust
struct ProtocolConnection {
    state: HandshakeState,
    capabilities: ProtocolCapabilities,
    transport: Box<dyn Transport>,
    multiplexing: bool,
}

impl ProtocolConnection {
    async fn activate_multiplexing(&mut self) -> Result<()> {
        // After this point, use MultiplexReader/MultiplexWriter
        self.multiplexing = true;
        Ok(())
    }
}
```

## 1.2 Complete Handshake State Machine

```rust
pub enum HandshakeState {
    /// Initial state
    Initial,
    
    /// Sent our protocol version
    VersionSent { our_version: u8 },
    
    /// Received remote version
    VersionReceived { 
        our_version: u8,
        remote_version: u8,
    },
    
    /// Negotiated effective version
    VersionNegotiated {
        protocol_version: u8,
    },
    
    /// Sent capability flags
    FlagsSent {
        protocol_version: u8,
        our_flags: u32,
    },
    
    /// Received remote flags
    FlagsReceived {
        protocol_version: u8,
        our_flags: u32,
        remote_flags: u32,
    },
    
    /// Negotiated capabilities
    CapabilitiesNegotiated {
        capabilities: ProtocolCapabilities,
    },
    
    /// Exchanging checksum seed
    SeedExchange {
        capabilities: ProtocolCapabilities,
    },
    
    /// Exchanging filter rules
    FilterExchange {
        capabilities: ProtocolCapabilities,
        seed: Option<ChecksumSeed>,
    },
    
    /// Handshake complete, ready for file list
    Complete {
        capabilities: ProtocolCapabilities,
        seed: Option<ChecksumSeed>,
        filters: Option<FilterList>,
    },
}

impl HandshakeState {
    pub async fn advance<T: Transport>(
        self,
        transport: &mut T,
        role: Role,
    ) -> Result<Self> {
        match self {
            Self::Initial => {
                // Send our version
                let our_version = PROTOCOL_VERSION;
                transport.write(&[our_version]).await?;
                Ok(Self::VersionSent { our_version })
            }
            
            Self::VersionSent { our_version } => {
                // Read remote version
                let mut buf = [0u8; 1];
                transport.read_exact(&mut buf).await?;
                let remote_version = buf[0];
                
                // Validate version
                if remote_version < MIN_PROTOCOL_VERSION {
                    anyhow::bail!(
                        "Unsupported protocol version: {} (min: {})",
                        remote_version,
                        MIN_PROTOCOL_VERSION
                    );
                }
                
                Ok(Self::VersionReceived { our_version, remote_version })
            }
            
            Self::VersionReceived { our_version, remote_version } => {
                // Negotiate effective version
                let protocol_version = our_version.min(remote_version);
                info!("Protocol version negotiated: {}", protocol_version);
                
                Ok(Self::VersionNegotiated { protocol_version })
            }
            
            Self::VersionNegotiated { protocol_version } => {
                // Send capability flags
                let our_flags = get_our_capabilities();
                let mut buf = Vec::new();
                encode_varint_into(our_flags as u64, &mut buf);
                transport.write(&buf).await?;
                
                Ok(Self::FlagsSent { protocol_version, our_flags })
            }
            
            Self::FlagsSent { protocol_version, our_flags } => {
                // Receive remote flags
                let remote_flags = decode_varint(transport).await? as u32;
                
                Ok(Self::FlagsReceived {
                    protocol_version,
                    our_flags,
                    remote_flags,
                })
            }
            
            Self::FlagsReceived {
                protocol_version,
                our_flags,
                remote_flags,
            } => {
                // Negotiate capabilities
                let capabilities = ProtocolCapabilities {
                    version: protocol_version,
                    flags: our_flags & remote_flags, // Intersection
                    checksum_seed: None,
                    compression: None,
                };
                
                info!("Capabilities negotiated: {:?}", capabilities);
                
                Ok(Self::CapabilitiesNegotiated { capabilities })
            }
            
            Self::CapabilitiesNegotiated { mut capabilities } => {
                // Exchange checksum seed if enabled
                if capabilities.flags & XMIT_CHECKSUM_SEED != 0 {
                    let seed = match role {
                        Role::Sender => {
                            // Send our seed
                            let seed = ChecksumSeed::generate();
                            let bytes = seed.seed.to_le_bytes();
                            transport.write(&bytes).await?;
                            Some(seed)
                        }
                        Role::Receiver => {
                            // Receive seed
                            let mut bytes = [0u8; 4];
                            transport.read_exact(&mut bytes).await?;
                            let seed = u32::from_le_bytes(bytes);
                            Some(ChecksumSeed { seed })
                        }
                    };
                    
                    capabilities.checksum_seed = seed.as_ref().map(|s| s.seed);
                    
                    Ok(Self::SeedExchange { capabilities })
                } else {
                    Ok(Self::FilterExchange {
                        capabilities,
                        seed: None,
                    })
                }
            }
            
            Self::SeedExchange { capabilities } => {
                // Move to filter exchange
                Ok(Self::FilterExchange {
                    capabilities,
                    seed: capabilities.checksum_seed.map(|s| ChecksumSeed { seed: s }),
                })
            }
            
            Self::FilterExchange { capabilities, seed } => {
                // TODO: Handle filter rules if needed
                // For now, we skip filter rules
                
                // Activate multiplexing (protocol >= 27)
                if capabilities.version >= 27 {
                    info!("Activating multiplexed I/O mode");
                }
                
                Ok(Self::Complete {
                    capabilities,
                    seed,
                    filters: None,
                })
            }
            
            Self::Complete { .. } => {
                anyhow::bail!("Handshake already complete")
            }
        }
    }
}
```

## 1.3 Helper Functions

```rust
pub enum Role {
    Sender,
    Receiver,
}

fn get_our_capabilities() -> u32 {
    let mut flags = 0u32;
    
    // What we support (arsync has LOCAL support for all of these!)
    flags |= XMIT_CHECKSUMS;      // ✅ Checksums
    flags |= XMIT_SYMLINKS;       // ✅ Symlinks (-l/--links)
    flags |= XMIT_HARDLINKS;      // ✅ Hard links (-H/--hard-links)
    flags |= XMIT_DEVICES;        // ✅ Device files (-D/--devices)
    flags |= XMIT_XATTRS;         // ✅ Extended attributes (-X/--xattrs)
    flags |= XMIT_ACLS;           // ✅ POSIX ACLs (-A/--acls)
    flags |= XMIT_CHECKSUM_SEED;  // ✅ Checksum seed
    flags |= XMIT_PROTECTION;     // ✅ Permissions (-p/--perms)
    flags |= XMIT_TIMES;          // ✅ Timestamps (-t/--times)
    // Note: We also support -U/--atimes and --crtimes locally!
    
    // What we don't support in wire protocol yet
    // (but WILL need to implement for rsync compatibility)
    // flags |= XMIT_SPARSE;      // Sparse file optimization (rsync-specific)
    
    flags
}
```

## 1.4 Testing Strategy

### Unit Tests

```rust
#[tokio::test]
async fn test_handshake_version_negotiation() {
    let (client_tx, server_rx) = create_pipe_pair();
    let (server_tx, client_rx) = create_pipe_pair();
    
    let client_transport = PipeTransport::from_fds(client_rx, client_tx);
    let server_transport = PipeTransport::from_fds(server_rx, server_tx);
    
    let client_task = tokio::spawn(async move {
        let mut state = HandshakeState::Initial;
        while !matches!(state, HandshakeState::Complete { .. }) {
            state = state.advance(&mut client_transport, Role::Sender).await?;
        }
        Ok::<_, anyhow::Error>(state)
    });
    
    let server_task = tokio::spawn(async move {
        let mut state = HandshakeState::Initial;
        while !matches!(state, HandshakeState::Complete { .. }) {
            state = state.advance(&mut server_transport, Role::Receiver).await?;
        }
        Ok::<_, anyhow::Error>(state)
    });
    
    let (client_result, server_result) = tokio::try_join!(client_task, server_task).unwrap();
    
    let client_final = client_result.unwrap();
    let server_final = server_result.unwrap();
    
    // Both should reach Complete state
    assert!(matches!(client_final, HandshakeState::Complete { .. }));
    assert!(matches!(server_final, HandshakeState::Complete { .. }));
}
```

### Integration Tests

```rust
#[test]
fn test_handshake_with_real_rsync() {
    // Spawn real rsync --server
    let mut rsync = Command::new("rsync")
        .arg("--server")
        .arg("--sender")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    
    // Run our handshake
    let mut client = RsyncClient::new(
        rsync.stdin.take().unwrap(),
        rsync.stdout.take().unwrap(),
    );
    
    let result = client.handshake().await;
    assert!(result.is_ok());
    
    let capabilities = result.unwrap();
    assert!(capabilities.version >= 27);
}
```

---

# Part 2: compio/io_uring Integration

## Overview

Currently, the protocol code uses **tokio** and **blocking I/O** (`std::io`). To align with arsync's core architecture, we need to migrate to **compio** and **io_uring**-based async I/O.

## 2.1 Current Architecture

```
┌─────────────────┐
│   main.rs       │
│  (compio::main) │
└────────┬────────┘
         │
    ┌────┴─────────────────────┐
    │                          │
┌───▼──────────┐    ┌─────────▼─────────┐
│  sync.rs     │    │  protocol/mod.rs  │
│  (compio)    │    │  (tokio/std::io)  │ ← Problem!
│  (io_uring)  │    │  (syscalls)       │
└──────────────┘    └───────────────────┘
```

**Issue**: We have two async runtimes fighting for control!

## 2.2 Target Architecture

```
┌─────────────────┐
│   main.rs       │
│  (compio::main) │
└────────┬────────┘
         │
    ┌────┴─────────────────────┐
    │                          │
┌───▼──────────┐    ┌─────────▼─────────┐
│  sync.rs     │    │  protocol/mod.rs  │
│  (compio)    │    │  (compio)         │ ← Aligned!
│  (io_uring)  │    │  (io_uring)       │
└──────────────┘    └───────────────────┘
```

## 2.3 Required Changes

### 2.3.1 Transport Trait Redesign

**Current** (`src/protocol/transport.rs`):
```rust
#[async_trait::async_trait]
pub trait Transport: Send {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize>;
    async fn write(&mut self, buf: &[u8]) -> Result<usize>;
    async fn flush(&mut self) -> Result<()>;
}
```

**Problem**: `async_trait` uses boxed futures (allocations), not compatible with compio's poll-based model.

**Target**:
```rust
// No async_trait! Use compio's AsyncRead/AsyncWrite
pub trait Transport: compio::io::AsyncRead + compio::io::AsyncWrite + Send {
    fn name(&self) -> &str { "unknown" }
    fn supports_multiplexing(&self) -> bool { false }
}
```

**Migration**:
- Remove `async_trait` dependency
- Use `compio::io::{AsyncRead, AsyncWrite}` traits directly
- Implement using compio's poll-based async

### 2.3.2 PipeTransport Conversion

**Current** (`src/protocol/pipe.rs`):
```rust
pub struct PipeTransport {
    reader: Box<dyn Read + Send>,      // std::io::Read
    writer: Box<dyn Write + Send>,     // std::io::Write
    name: String,
}

#[async_trait]
impl Transport for PipeTransport {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        // BLOCKING call inside async!
        Ok(self.reader.read(buf)?)
    }
}
```

**Target**:
```rust
pub struct PipeTransport {
    reader: compio::fs::File,  // io_uring backed
    writer: compio::fs::File,  // io_uring backed
    name: String,
}

impl compio::io::AsyncRead for PipeTransport {
    async fn read(&mut self, buf: &mut [u8]) -> compio::io::Result<usize> {
        // Uses io_uring under the hood!
        self.reader.read(buf).await
    }
}

impl compio::io::AsyncWrite for PipeTransport {
    async fn write(&mut self, buf: &[u8]) -> compio::io::Result<usize> {
        self.writer.write(buf).await
    }
    
    async fn flush(&mut self) -> compio::io::Result<()> {
        self.writer.flush().await
    }
}
```

**Implementation**:
```rust
impl PipeTransport {
    pub fn from_stdio() -> Result<Self> {
        use std::os::unix::io::{AsRawFd, FromRawFd};
        
        // Convert stdin/stdout to compio Files
        let stdin_fd = std::io::stdin().as_raw_fd();
        let stdout_fd = std::io::stdout().as_raw_fd();
        
        // SAFETY: We own these file descriptors
        let reader = unsafe {
            compio::fs::File::from_raw_fd(stdin_fd)
        };
        let writer = unsafe {
            compio::fs::File::from_raw_fd(stdout_fd)
        };
        
        Ok(Self {
            reader,
            writer,
            name: "stdio".to_string(),
        })
    }
}
```

### 2.3.3 SSH Connection Conversion

**Current** (`src/protocol/ssh.rs`):
```rust
use tokio::process::{Child, ChildStdin, ChildStdout};

pub struct SshConnection {
    process: Child,              // tokio process
    stdin: ChildStdin,           // tokio async
    stdout: ChildStdout,         // tokio async
}
```

**Target**:
```rust
use compio::process::{Child, ChildStdin, ChildStdout};

pub struct SshConnection {
    process: Child,              // compio process
    stdin: ChildStdin,           // compio async
    stdout: ChildStdout,         // compio async
}
```

**⚠️ MISSING FUNCTIONALITY ALERT**:

**compio does NOT have process support yet!**

**Options**:

#### Option A: Wait for compio process support
- **Pro**: Clean architecture, no hacks
- **Con**: Blocks implementation
- **Timeline**: Unknown (need to check compio roadmap)

#### Option B: Use compio-driver directly
```rust
// Use compio's io_uring driver for file descriptors
pub struct SshConnection {
    process: std::process::Child,  // stdlib process
    stdin_fd: compio::driver::OwnedFd,
    stdout_fd: compio::driver::OwnedFd,
}

impl SshConnection {
    pub async fn connect(host: &str, user: &str, shell: &str) -> Result<Self> {
        use std::process::{Command, Stdio};
        use std::os::unix::io::{AsRawFd, FromRawFd};
        
        // Spawn with stdlib (synchronous)
        let mut child = Command::new(shell)
            .arg(format!("{}@{}", user, host))
            .arg("--server")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;
        
        // Extract raw FDs
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        
        let stdin_fd = stdin.as_raw_fd();
        let stdout_fd = stdout.as_raw_fd();
        
        // Forget stdlib handles (we own the FDs now)
        std::mem::forget(stdin);
        std::mem::forget(stdout);
        
        // Wrap in compio's io_uring driver
        let stdin_fd = unsafe {
            compio::driver::OwnedFd::from_raw_fd(stdin_fd)
        };
        let stdout_fd = unsafe {
            compio::driver::OwnedFd::from_raw_fd(stdout_fd)
        };
        
        Ok(Self {
            process: child,
            stdin_fd,
            stdout_fd,
        })
    }
}

impl compio::io::AsyncRead for SshConnection {
    async fn read(&mut self, buf: &mut [u8]) -> compio::io::Result<usize> {
        // Use compio's io_uring op directly
        compio::driver::op::Read::new(
            self.stdout_fd.as_raw_fd(),
            buf,
        ).await
    }
}
```

#### Option C: Hybrid approach (recommended)
```rust
// Use compio for pipe transport (testing)
// Use stdlib + thread pool for SSH (production)

pub enum TransportImpl {
    Pipe(PipeTransport),      // compio, io_uring
    Ssh(SshConnection),       // stdlib + thread pool
}

impl Transport for TransportImpl {
    // Dispatch based on variant
}
```

**Recommendation**: **Option C** - Keep pipes on io_uring, use thread pool for SSH until compio adds process support.

### 2.3.4 Multiplexed I/O Conversion

**Current**:
```rust
pub struct MultiplexReader<T: Transport> {
    transport: T,
    buffer: Vec<u8>,
}

impl<T: Transport> MultiplexReader<T> {
    pub async fn read_message(&mut self) -> Result<(MessageTag, Vec<u8>)> {
        // Uses Transport::read (async_trait)
    }
}
```

**Target**:
```rust
pub struct MultiplexReader<T: compio::io::AsyncRead> {
    transport: T,
    buffer: Vec<u8>,
}

impl<T: compio::io::AsyncRead + Unpin> MultiplexReader<T> {
    pub async fn read_message(&mut self) -> Result<(MessageTag, Vec<u8>)> {
        use compio::io::AsyncReadExt;
        
        // Read tag
        let mut tag_buf = [0u8; 1];
        self.transport.read_exact(&mut tag_buf).await?;
        let tag = MessageTag::from_u8(tag_buf[0])?;
        
        // Read length (3 bytes)
        let mut len_buf = [0u8; 3];
        self.transport.read_exact(&mut len_buf).await?;
        let length = u32::from_le_bytes([len_buf[0], len_buf[1], len_buf[2], 0]);
        
        // Read data
        let mut data = vec![0u8; length as usize];
        self.transport.read_exact(&mut data).await?;
        
        Ok((tag, data))
    }
}
```

## 2.4 Missing compio Functionality Assessment

### ✅ Available in compio
- ✅ `AsyncRead` / `AsyncWrite` traits
- ✅ File operations (`compio::fs::File`)
- ✅ Raw FD support (`FromRawFd`)
- ✅ Buffered I/O (`BufReader`, `BufWriter`)
- ✅ io_uring backend

### ❌ Missing in compio
- ❌ **Process spawning** (`compio::process`) - **CRITICAL**
- ❌ **TCP sockets** (`compio::net::TcpStream`) - May exist, need to verify
- ❌ **Signals** - For child process management
- ❌ **Async pipe creation** - Need to use `pipe2()` syscall

### 🔍 Need to Investigate
- **compio-net**: Does it exist? What does it provide?
- **compio-process**: Roadmap? Timeline?
- **Custom io_uring ops**: Can we write our own for missing features?

## 2.5 Migration Plan

### Phase 2.1: Audit compio capabilities
```bash
# Check what's available
grep -r "pub mod" ~/.cargo/registry/src/*/compio-*/src/

# Check for process support
rg "process" ~/.cargo/registry/src/*/compio-*/

# Check for network support
rg "TcpStream" ~/.cargo/registry/src/*/compio-*/
```

### Phase 2.2: Transport trait migration
1. Remove `async_trait` dependency
2. Define new Transport based on compio traits
3. Update all implementors

### Phase 2.3: PipeTransport migration
1. Convert to `compio::fs::File`
2. Update tests
3. Verify io_uring usage with `strace`

### Phase 2.4: SSH connection strategy
1. If compio process exists: use it
2. Else: Implement hybrid approach (Option C)
3. Document limitations

### Phase 2.5: Integration
1. Update all protocol functions to use new Transport
2. Remove tokio dependency
3. Run full test suite

## 2.6 Performance Expectations

### Before (tokio + syscalls)
```
read() syscall → context switch → kernel → context switch → userspace
write() syscall → context switch → kernel → context switch → userspace
```
**Cost**: 2 context switches per I/O operation

### After (compio + io_uring)
```
io_uring_prep_read() → SQE queue
io_uring_prep_write() → SQE queue
io_uring_submit() → kernel batches operations
io_uring_wait_cqe() → CQE queue
```
**Cost**: 0-1 context switches for entire batch

**Expected Improvement**: 30-50% reduction in I/O latency for small operations

---

# Part 3: Checksum Exchange Abstraction

## Overview

We need a unified abstraction for checksums that supports:
1. **rsync's checksum format**: Rolling + MD5/MD4
2. **arsync native format**: Rolling + MD5 (already implemented)
3. **Future formats**: BLAKE3, SHA-256, etc.

## 3.1 Problem Statement

### rsync Checksum Format
```
Sender → Receiver: For each block:
  [u32 rolling_checksum]  (weak, Adler-32 style)
  [16 bytes strong]       (MD4 or MD5)
```

### arsync Native Format
```rust
pub struct BlockChecksum {
    pub weak: u32,        // Rolling checksum
    pub strong: [u8; 16], // MD5
    pub offset: u64,      // Block position
    pub block_index: u32, // Block number
}
```

**Differences**:
1. rsync doesn't send `offset` (implicit from block_index)
2. rsync doesn't send `block_index` (implicit from order)
3. rsync may use MD4 instead of MD5 (protocol version dependent)

## 3.2 Abstraction Design

### 3.2.1 Checksum Algorithm Trait

```rust
/// Trait for strong checksum algorithms
pub trait StrongChecksumAlgorithm: Send + Sync {
    /// Digest size in bytes
    fn digest_size(&self) -> usize;
    
    /// Compute digest
    fn compute(&self, data: &[u8]) -> Vec<u8>;
    
    /// Algorithm name (for logging)
    fn name(&self) -> &'static str;
}

/// MD5 checksum (arsync native, rsync protocol >= 27)
pub struct Md5Checksum;

impl StrongChecksumAlgorithm for Md5Checksum {
    fn digest_size(&self) -> usize { 16 }
    
    fn compute(&self, data: &[u8]) -> Vec<u8> {
        md5::compute(data).to_vec()
    }
    
    fn name(&self) -> &'static str { "MD5" }
}

/// MD4 checksum (rsync protocol < 27, legacy)
pub struct Md4Checksum;

impl StrongChecksumAlgorithm for Md4Checksum {
    fn digest_size(&self) -> usize { 16 }
    
    fn compute(&self, data: &[u8]) -> Vec<u8> {
        // Use md-4 crate
        md4::Md4::digest(data).to_vec()
    }
    
    fn name(&self) -> &'static str { "MD4" }
}

/// BLAKE3 checksum (future: high performance)
pub struct Blake3Checksum;

impl StrongChecksumAlgorithm for Blake3Checksum {
    fn digest_size(&self) -> usize { 32 }
    
    fn compute(&self, data: &[u8]) -> Vec<u8> {
        blake3::hash(data).as_bytes().to_vec()
    }
    
    fn name(&self) -> &'static str { "BLAKE3" }
}
```

### 3.2.2 Rolling Checksum Abstraction

```rust
/// Rolling checksum with seed support
pub struct RollingChecksum {
    seed: u32,
}

impl RollingChecksum {
    pub fn new(seed: u32) -> Self {
        Self { seed }
    }
    
    pub fn compute(&self, data: &[u8]) -> u32 {
        if self.seed == 0 {
            // No seed, standard Adler-32
            rolling_checksum(data)
        } else {
            // With seed, mix it in
            rolling_checksum_with_seed(data, self.seed)
        }
    }
    
    pub fn update(&self, old_checksum: u32, old_byte: u8, new_byte: u8, window: usize) -> u32 {
        rolling_checksum_update(old_checksum, old_byte, new_byte, window)
    }
}

fn rolling_checksum_with_seed(data: &[u8], seed: u32) -> u32 {
    // Mix seed into initial state
    let mut a = seed & 0xFFFF;
    let mut b = (seed >> 16) & 0xFFFF;
    
    for &byte in data {
        a = (a + u32::from(byte)) % MODULUS;
        b = (b + a) % MODULUS;
    }
    
    (b << 16) | a
}
```

### 3.2.3 Block Checksum Abstraction

```rust
/// Universal block checksum representation
#[derive(Debug, Clone)]
pub struct BlockChecksum {
    /// Rolling (weak) checksum
    pub rolling: u32,
    
    /// Strong checksum (variable length)
    pub strong: Vec<u8>,
    
    /// Block offset in file (optional - rsync doesn't use)
    pub offset: Option<u64>,
    
    /// Block index (optional - may be implicit)
    pub index: Option<u32>,
}

impl BlockChecksum {
    /// Convert to arsync native format
    pub fn to_native(&self) -> crate::protocol::rsync::BlockChecksum {
        crate::protocol::rsync::BlockChecksum {
            weak: self.rolling,
            strong: self.strong[..16].try_into().unwrap(), // Assume MD5
            offset: self.offset.unwrap_or(0),
            block_index: self.index.unwrap_or(0),
        }
    }
    
    /// Create from arsync native format
    pub fn from_native(native: &crate::protocol::rsync::BlockChecksum) -> Self {
        Self {
            rolling: native.weak,
            strong: native.strong.to_vec(),
            offset: Some(native.offset),
            index: Some(native.block_index),
        }
    }
}
```

### 3.2.4 Checksum Generator

```rust
/// Generate checksums for a file
pub struct ChecksumGenerator {
    block_size: usize,
    rolling: RollingChecksum,
    strong: Box<dyn StrongChecksumAlgorithm>,
}

impl ChecksumGenerator {
    pub fn new(
        block_size: usize,
        seed: u32,
        strong: Box<dyn StrongChecksumAlgorithm>,
    ) -> Self {
        Self {
            block_size,
            rolling: RollingChecksum::new(seed),
            strong,
        }
    }
    
    /// Generate checksums for entire file
    pub fn generate(&self, data: &[u8]) -> Vec<BlockChecksum> {
        let mut checksums = Vec::new();
        
        for (index, chunk) in data.chunks(self.block_size).enumerate() {
            let rolling = self.rolling.compute(chunk);
            let strong = self.strong.compute(chunk);
            
            checksums.push(BlockChecksum {
                rolling,
                strong,
                offset: Some((index * self.block_size) as u64),
                index: Some(index as u32),
            });
        }
        
        checksums
    }
}
```

## 3.3 Wire Format Abstraction

### 3.3.1 Checksum Protocol Trait

```rust
#[async_trait]
pub trait ChecksumProtocol: Send {
    /// Send checksums for a file
    async fn send_checksums<T: Transport>(
        &self,
        transport: &mut T,
        checksums: &[BlockChecksum],
    ) -> Result<()>;
    
    /// Receive checksums for a file
    async fn receive_checksums<T: Transport>(
        &self,
        transport: &mut T,
    ) -> Result<Vec<BlockChecksum>>;
}
```

### 3.3.2 rsync Format Implementation

```rust
pub struct RsyncChecksumProtocol {
    protocol_version: u8,
}

#[async_trait]
impl ChecksumProtocol for RsyncChecksumProtocol {
    async fn send_checksums<T: Transport>(
        &self,
        transport: &mut T,
        checksums: &[BlockChecksum],
    ) -> Result<()> {
        // rsync format: [u32 count][checksums...]
        let count = checksums.len() as u32;
        transport.write(&count.to_le_bytes()).await?;
        
        for checksum in checksums {
            // Rolling checksum (4 bytes)
            transport.write(&checksum.rolling.to_le_bytes()).await?;
            
            // Strong checksum (16 bytes for MD5/MD4)
            let strong = if checksum.strong.len() == 16 {
                &checksum.strong[..]
            } else {
                // Truncate or pad
                &checksum.strong[..16.min(checksum.strong.len())]
            };
            transport.write(strong).await?;
        }
        
        transport.flush().await?;
        Ok(())
    }
    
    async fn receive_checksums<T: Transport>(
        &self,
        transport: &mut T,
    ) -> Result<Vec<BlockChecksum>> {
        // Read count
        let mut count_buf = [0u8; 4];
        transport.read_exact(&mut count_buf).await?;
        let count = u32::from_le_bytes(count_buf);
        
        let mut checksums = Vec::with_capacity(count as usize);
        
        for index in 0..count {
            // Read rolling checksum
            let mut rolling_buf = [0u8; 4];
            transport.read_exact(&mut rolling_buf).await?;
            let rolling = u32::from_le_bytes(rolling_buf);
            
            // Read strong checksum
            let mut strong = vec![0u8; 16];
            transport.read_exact(&mut strong).await?;
            
            checksums.push(BlockChecksum {
                rolling,
                strong,
                offset: None,
                index: Some(index),
            });
        }
        
        Ok(checksums)
    }
}
```

### 3.3.3 arsync Native Format Implementation

```rust
pub struct ArsyncChecksumProtocol;

#[async_trait]
impl ChecksumProtocol for ArsyncChecksumProtocol {
    async fn send_checksums<T: Transport>(
        &self,
        transport: &mut T,
        checksums: &[BlockChecksum],
    ) -> Result<()> {
        // arsync format: [varint count][checksums with metadata...]
        let mut buf = Vec::new();
        encode_varint_into(checksums.len() as u64, &mut buf);
        transport.write(&buf).await?;
        
        for checksum in checksums {
            buf.clear();
            
            // Rolling (4 bytes)
            buf.extend(&checksum.rolling.to_le_bytes());
            
            // Strong (16 bytes)
            buf.extend(&checksum.strong[..16]);
            
            // Offset (varint)
            encode_varint_into(checksum.offset.unwrap_or(0), &mut buf);
            
            // Index (varint)
            encode_varint_into(checksum.index.unwrap_or(0) as u64, &mut buf);
            
            transport.write(&buf).await?;
        }
        
        transport.flush().await?;
        Ok(())
    }
    
    async fn receive_checksums<T: Transport>(
        &self,
        transport: &mut T,
    ) -> Result<Vec<BlockChecksum>> {
        // Similar to send, but reading
        let count = decode_varint(transport).await? as usize;
        
        let mut checksums = Vec::with_capacity(count);
        
        for _ in 0..count {
            let mut rolling_buf = [0u8; 4];
            transport.read_exact(&mut rolling_buf).await?;
            let rolling = u32::from_le_bytes(rolling_buf);
            
            let mut strong = vec![0u8; 16];
            transport.read_exact(&mut strong).await?;
            
            let offset = decode_varint(transport).await?;
            let index = decode_varint(transport).await? as u32;
            
            checksums.push(BlockChecksum {
                rolling,
                strong,
                offset: Some(offset),
                index: Some(index),
            });
        }
        
        Ok(checksums)
    }
}
```

## 3.4 Protocol Selection

```rust
pub fn create_checksum_protocol(
    capabilities: &ProtocolCapabilities,
    compat_mode: bool,
) -> Box<dyn ChecksumProtocol> {
    if compat_mode || capabilities.version < 100 {
        // rsync compatibility mode
        Box::new(RsyncChecksumProtocol {
            protocol_version: capabilities.version,
        })
    } else {
        // arsync native mode
        Box::new(ArsyncChecksumProtocol)
    }
}
```

## 3.5 Testing Strategy

```rust
#[test]
fn test_checksum_roundtrip() {
    let data = b"Hello, World!";
    
    // Generate checksums
    let generator = ChecksumGenerator::new(
        4096,
        0,
        Box::new(Md5Checksum),
    );
    let checksums = generator.generate(data);
    
    // Test both protocols
    for protocol in [
        Box::new(RsyncChecksumProtocol { protocol_version: 31 }),
        Box::new(ArsyncChecksumProtocol),
    ] {
        let (tx, rx) = create_pipe_pair();
        
        // Send
        protocol.send_checksums(&mut tx, &checksums).await.unwrap();
        
        // Receive
        let received = protocol.receive_checksums(&mut rx).await.unwrap();
        
        // Compare
        assert_eq!(checksums.len(), received.len());
        for (sent, recv) in checksums.iter().zip(received.iter()) {
            assert_eq!(sent.rolling, recv.rolling);
            assert_eq!(sent.strong, recv.strong);
        }
    }
}
```

---

# Part 4: Delta Token Handling

## Overview

Delta tokens represent instructions for reconstructing a file from a basis file and new data. rsync and arsync use different formats.

## 4.1 Problem Statement

### arsync Native Delta Format

```rust
pub enum DeltaInstruction {
    /// Insert literal data
    Literal(Vec<u8>),
    
    /// Copy from basis file
    BlockMatch {
        block_index: u32,
        length: u32,
    },
}
```

**Wire format** (arsync):
```
Literal:    [u8 type=0][varint len][len bytes data]
BlockMatch: [u8 type=1][varint block_index][varint length]
```

### rsync Delta Token Format

From rsync technical report and source:

```
Token types:
  0           : End of delta stream
  1-127       : Literal byte (value = byte)
  128-65535   : Short token
                  If >= 128+block_count: literal run of (token - 128 - block_count) bytes
                  Else: block match at index (token - 128)
  65536+      : Long format
                  High 16 bits = flags/type
                  Low 16 bits = length or index
```

**Detailed rsync format**:
```
# Token format (from rsync source)
if token == 0:
    # End marker
    break
elif token < 128:
    # Single literal byte
    literal_byte = token
elif token < 128 + block_count:
    # Block match
    block_index = token - 128
    # Length is implicit (block_size)
else:
    # Literal run
    run_length = token - 128 - block_count
    read(run_length bytes)
```

**Long token format** (for large files):
```
if token >= 0x10000:
    type = (token >> 16) & 0xFF
    value = token & 0xFFFF
    
    if type == LONG_LITERAL:
        length = value + 65536
        read(length bytes)
    elif type == LONG_BLOCK:
        block_index = value + 65536
```

## 4.2 Delta Token Abstraction

### 4.2.1 Universal Delta Instruction

```rust
/// Universal delta instruction
#[derive(Debug, Clone)]
pub enum DeltaOp {
    /// Insert literal data
    Literal {
        data: Vec<u8>,
    },
    
    /// Copy from basis file
    Copy {
        /// Block index in basis file (for rsync)
        /// Or offset in basis file (for future formats)
        source: CopySource,
        
        /// Length to copy
        length: u32,
    },
}

#[derive(Debug, Clone)]
pub enum CopySource {
    /// Block index (rsync style)
    BlockIndex(u32),
    
    /// Byte offset (future style)
    ByteOffset(u64),
}

impl DeltaOp {
    /// Convert to arsync native format
    pub fn to_native(&self) -> DeltaInstruction {
        match self {
            Self::Literal { data } => DeltaInstruction::Literal(data.clone()),
            Self::Copy { source, length } => {
                let block_index = match source {
                    CopySource::BlockIndex(idx) => *idx,
                    CopySource::ByteOffset(off) => {
                        // Assume default block size for conversion
                        (*off / DEFAULT_BLOCK_SIZE as u64) as u32
                    }
                };
                DeltaInstruction::BlockMatch {
                    block_index,
                    length: *length,
                }
            }
        }
    }
    
    /// Create from arsync native format
    pub fn from_native(native: &DeltaInstruction) -> Self {
        match native {
            DeltaInstruction::Literal(data) => Self::Literal { data: data.clone() },
            DeltaInstruction::BlockMatch { block_index, length } => Self::Copy {
                source: CopySource::BlockIndex(*block_index),
                length: *length,
            },
        }
    }
}
```

### 4.2.2 Delta Protocol Trait

```rust
#[async_trait]
pub trait DeltaProtocol: Send {
    /// Send delta stream
    async fn send_delta<T: Transport>(
        &self,
        transport: &mut T,
        delta: &[DeltaOp],
        block_size: u32,
    ) -> Result<()>;
    
    /// Receive delta stream
    async fn receive_delta<T: Transport>(
        &self,
        transport: &mut T,
        block_count: u32,
        block_size: u32,
    ) -> Result<Vec<DeltaOp>>;
}
```

### 4.2.3 rsync Delta Protocol Implementation

```rust
pub struct RsyncDeltaProtocol {
    protocol_version: u8,
}

impl RsyncDeltaProtocol {
    fn encode_token(op: &DeltaOp, block_count: u32, block_size: u32) -> Vec<u32> {
        match op {
            DeltaOp::Literal { data } => {
                let mut tokens = Vec::new();
                
                if data.len() == 1 {
                    // Single byte literal: token = byte value (1-127)
                    tokens.push(data[0] as u32);
                } else if data.len() < 65536 {
                    // Short literal run
                    let token = 128 + block_count + data.len() as u32;
                    tokens.push(token);
                } else {
                    // Long literal run
                    let type_flag = 0x01u32 << 16; // LONG_LITERAL
                    let length = (data.len() - 65536) as u32;
                    let token = type_flag | length;
                    tokens.push(token);
                }
                
                tokens
            }
            
            DeltaOp::Copy { source, length } => {
                let block_index = match source {
                    CopySource::BlockIndex(idx) => *idx,
                    CopySource::ByteOffset(off) => {
                        (*off / block_size as u64) as u32
                    }
                };
                
                if block_index < 65536 {
                    // Short block match
                    vec![128 + block_index]
                } else {
                    // Long block match
                    let type_flag = 0x02u32 << 16; // LONG_BLOCK
                    let index = (block_index - 65536) as u32;
                    vec![type_flag | index]
                }
            }
        }
    }
    
    fn decode_token(token: u32, block_count: u32) -> TokenType {
        if token == 0 {
            TokenType::End
        } else if token < 128 {
            TokenType::LiteralByte(token as u8)
        } else if token < 128 + block_count {
            TokenType::BlockMatch {
                index: token - 128,
            }
        } else if token < 0x10000 {
            TokenType::LiteralRun {
                length: token - 128 - block_count,
            }
        } else {
            let type_byte = (token >> 16) & 0xFF;
            let value = token & 0xFFFF;
            
            match type_byte {
                0x01 => TokenType::LongLiteralRun {
                    length: value + 65536,
                },
                0x02 => TokenType::LongBlockMatch {
                    index: value + 65536,
                },
                _ => TokenType::Invalid,
            }
        }
    }
}

enum TokenType {
    End,
    LiteralByte(u8),
    LiteralRun { length: u32 },
    LongLiteralRun { length: u32 },
    BlockMatch { index: u32 },
    LongBlockMatch { index: u32 },
    Invalid,
}

#[async_trait]
impl DeltaProtocol for RsyncDeltaProtocol {
    async fn send_delta<T: Transport>(
        &self,
        transport: &mut T,
        delta: &[DeltaOp],
        block_size: u32,
    ) -> Result<()> {
        let block_count = calculate_block_count_from_delta(delta);
        
        for op in delta {
            let tokens = Self::encode_token(op, block_count, block_size);
            
            for token in tokens {
                // Send token (4 bytes, little-endian)
                transport.write(&token.to_le_bytes()).await?;
                
                // Send literal data if applicable
                if let DeltaOp::Literal { data } = op {
                    transport.write(data).await?;
                }
            }
        }
        
        // Send end marker
        transport.write(&[0u8; 4]).await?;
        transport.flush().await?;
        
        Ok(())
    }
    
    async fn receive_delta<T: Transport>(
        &self,
        transport: &mut T,
        block_count: u32,
        block_size: u32,
    ) -> Result<Vec<DeltaOp>> {
        let mut delta = Vec::new();
        
        loop {
            // Read token
            let mut token_buf = [0u8; 4];
            transport.read_exact(&mut token_buf).await?;
            let token = u32::from_le_bytes(token_buf);
            
            match Self::decode_token(token, block_count) {
                TokenType::End => break,
                
                TokenType::LiteralByte(byte) => {
                    delta.push(DeltaOp::Literal {
                        data: vec![byte],
                    });
                }
                
                TokenType::LiteralRun { length } => {
                    let mut data = vec![0u8; length as usize];
                    transport.read_exact(&mut data).await?;
                    delta.push(DeltaOp::Literal { data });
                }
                
                TokenType::LongLiteralRun { length } => {
                    let mut data = vec![0u8; length as usize];
                    transport.read_exact(&mut data).await?;
                    delta.push(DeltaOp::Literal { data });
                }
                
                TokenType::BlockMatch { index } => {
                    delta.push(DeltaOp::Copy {
                        source: CopySource::BlockIndex(index),
                        length: block_size,
                    });
                }
                
                TokenType::LongBlockMatch { index } => {
                    delta.push(DeltaOp::Copy {
                        source: CopySource::BlockIndex(index),
                        length: block_size,
                    });
                }
                
                TokenType::Invalid => {
                    anyhow::bail!("Invalid delta token: 0x{:08X}", token);
                }
            }
        }
        
        Ok(delta)
    }
}
```

### 4.2.4 arsync Native Delta Protocol

```rust
pub struct ArsyncDeltaProtocol;

#[async_trait]
impl DeltaProtocol for ArsyncDeltaProtocol {
    async fn send_delta<T: Transport>(
        &self,
        transport: &mut T,
        delta: &[DeltaOp],
        _block_size: u32,
    ) -> Result<()> {
        // Send count
        let mut buf = Vec::new();
        encode_varint_into(delta.len() as u64, &mut buf);
        transport.write(&buf).await?;
        
        for op in delta {
            buf.clear();
            
            match op {
                DeltaOp::Literal { data } => {
                    // Type tag
                    buf.push(0u8);
                    
                    // Length
                    encode_varint_into(data.len() as u64, &mut buf);
                    
                    // Write header
                    transport.write(&buf).await?;
                    
                    // Write data
                    transport.write(data).await?;
                }
                
                DeltaOp::Copy { source, length } => {
                    // Type tag
                    buf.push(1u8);
                    
                    // Block index
                    let block_index = match source {
                        CopySource::BlockIndex(idx) => *idx,
                        CopySource::ByteOffset(off) => {
                            (*off / DEFAULT_BLOCK_SIZE as u64) as u32
                        }
                    };
                    encode_varint_into(block_index as u64, &mut buf);
                    
                    // Length
                    encode_varint_into(*length as u64, &mut buf);
                    
                    transport.write(&buf).await?;
                }
            }
        }
        
        transport.flush().await?;
        Ok(())
    }
    
    async fn receive_delta<T: Transport>(
        &self,
        transport: &mut T,
        _block_count: u32,
        _block_size: u32,
    ) -> Result<Vec<DeltaOp>> {
        let count = decode_varint(transport).await?;
        let mut delta = Vec::with_capacity(count as usize);
        
        for _ in 0..count {
            let mut type_buf = [0u8; 1];
            transport.read_exact(&mut type_buf).await?;
            
            match type_buf[0] {
                0 => {
                    // Literal
                    let len = decode_varint(transport).await?;
                    let mut data = vec![0u8; len as usize];
                    transport.read_exact(&mut data).await?;
                    delta.push(DeltaOp::Literal { data });
                }
                
                1 => {
                    // Copy
                    let block_index = decode_varint(transport).await? as u32;
                    let length = decode_varint(transport).await? as u32;
                    delta.push(DeltaOp::Copy {
                        source: CopySource::BlockIndex(block_index),
                        length,
                    });
                }
                
                _ => anyhow::bail!("Invalid delta operation type: {}", type_buf[0]),
            }
        }
        
        Ok(delta)
    }
}
```

## 4.3 Delta Generation (Unified)

```rust
/// Generate delta from new file and basis checksums
pub fn generate_delta(
    new_data: &[u8],
    basis_checksums: &[BlockChecksum],
    block_size: usize,
) -> Vec<DeltaOp> {
    let mut delta = Vec::new();
    let mut pos = 0;
    
    // Build checksum lookup table
    let mut checksum_map: HashMap<u32, Vec<&BlockChecksum>> = HashMap::new();
    for checksum in basis_checksums {
        checksum_map
            .entry(checksum.rolling)
            .or_default()
            .push(checksum);
    }
    
    let mut literal_buffer = Vec::new();
    
    while pos < new_data.len() {
        let window_end = (pos + block_size).min(new_data.len());
        let window = &new_data[pos..window_end];
        
        // Compute rolling checksum for current window
        let rolling = rolling_checksum(window);
        
        // Check for match
        if let Some(candidates) = checksum_map.get(&rolling) {
            // Verify with strong checksum
            let strong = md5::compute(window);
            
            if let Some(matching) = candidates.iter().find(|c| c.strong == strong.0) {
                // Match found!
                
                // Flush literal buffer
                if !literal_buffer.is_empty() {
                    delta.push(DeltaOp::Literal {
                        data: literal_buffer.clone(),
                    });
                    literal_buffer.clear();
                }
                
                // Add copy instruction
                delta.push(DeltaOp::Copy {
                    source: CopySource::BlockIndex(
                        matching.index.unwrap_or(0)
                    ),
                    length: window.len() as u32,
                });
                
                pos += window.len();
                continue;
            }
        }
        
        // No match, add byte to literal buffer
        literal_buffer.push(new_data[pos]);
        pos += 1;
    }
    
    // Flush remaining literal buffer
    if !literal_buffer.is_empty() {
        delta.push(DeltaOp::Literal {
            data: literal_buffer,
        });
    }
    
    delta
}
```

## 4.4 Delta Application (Unified)

```rust
/// Apply delta to reconstruct file
pub fn apply_delta(
    basis_data: &[u8],
    delta: &[DeltaOp],
    block_size: usize,
) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    
    for op in delta {
        match op {
            DeltaOp::Literal { data } => {
                // Copy literal data
                output.extend_from_slice(data);
            }
            
            DeltaOp::Copy { source, length } => {
                // Copy from basis file
                let offset = match source {
                    CopySource::BlockIndex(idx) => {
                        (*idx as usize) * block_size
                    }
                    CopySource::ByteOffset(off) => *off as usize,
                };
                
                let end = (offset + *length as usize).min(basis_data.len());
                
                if offset >= basis_data.len() {
                    anyhow::bail!(
                        "Copy source out of bounds: offset={}, basis_len={}",
                        offset,
                        basis_data.len()
                    );
                }
                
                output.extend_from_slice(&basis_data[offset..end]);
            }
        }
    }
    
    Ok(output)
}
```

## 4.5 Protocol Selection

```rust
pub fn create_delta_protocol(
    capabilities: &ProtocolCapabilities,
    compat_mode: bool,
) -> Box<dyn DeltaProtocol> {
    if compat_mode || capabilities.version < 100 {
        // rsync compatibility
        Box::new(RsyncDeltaProtocol {
            protocol_version: capabilities.version,
        })
    } else {
        // arsync native
        Box::new(ArsyncDeltaProtocol)
    }
}
```

## 4.6 Testing Strategy

```rust
#[test]
fn test_delta_roundtrip() {
    let basis = b"Hello, World! This is a test.";
    let modified = b"Hello, Rust! This is a test.";
    
    // Generate checksums for basis
    let generator = ChecksumGenerator::new(8, 0, Box::new(Md5Checksum));
    let checksums = generator.generate(basis);
    
    // Generate delta
    let delta = generate_delta(modified, &checksums, 8);
    
    // Test both protocols
    for protocol in [
        Box::new(RsyncDeltaProtocol { protocol_version: 31 }),
        Box::new(ArsyncDeltaProtocol),
    ] {
        let (tx, rx) = create_pipe_pair();
        
        // Send delta
        protocol.send_delta(&mut tx, &delta, 8).await.unwrap();
        
        // Receive delta
        let received_delta = protocol
            .receive_delta(&mut rx, checksums.len() as u32, 8)
            .await
            .unwrap();
        
        // Apply delta
        let reconstructed = apply_delta(basis, &received_delta, 8).unwrap();
        
        // Verify
        assert_eq!(reconstructed, modified);
    }
}

#[test]
fn test_delta_with_real_rsync() {
    // Create basis file
    let basis_path = "/tmp/basis.txt";
    fs::write(basis_path, b"Hello, World!").unwrap();
    
    // Create modified file
    let modified_path = "/tmp/modified.txt";
    fs::write(modified_path, b"Hello, Rust!").unwrap();
    
    // Use real rsync to generate delta
    let output = Command::new("rsync")
        .arg("--only-write-batch=/tmp/delta")
        .arg(modified_path)
        .arg(basis_path)
        .output()
        .unwrap();
    
    assert!(output.status.success());
    
    // Parse delta file (rsync batch format)
    let delta_data = fs::read("/tmp/delta").unwrap();
    
    // Verify we can parse it
    // (This tests our understanding of rsync's format)
    let protocol = RsyncDeltaProtocol { protocol_version: 31 };
    // ... parse and verify
}
```

---

# Implementation Timeline

## Phase 1: Handshake Protocol (1-2 weeks)
- [ ] Implement HandshakeState machine
- [ ] Add capability negotiation
- [ ] Add seed exchange
- [ ] Test with real rsync
- **Deliverable**: Can complete handshake with rsync server

## Phase 2: compio Integration (2-3 weeks)
- [ ] Audit compio capabilities
- [ ] Redesign Transport trait
- [ ] Convert PipeTransport to io_uring
- [ ] Implement SSH connection strategy
- [ ] Update all protocol code
- **Deliverable**: Protocol code uses io_uring

## Phase 3: Checksum Abstraction (1 week)
- [ ] Design checksum traits
- [ ] Implement rsync checksum protocol
- [ ] Implement arsync native protocol
- [ ] Add protocol selection
- [ ] Test roundtrip
- **Deliverable**: Both checksum formats working

## Phase 4: Delta Token Handling (2 weeks)
- [ ] Implement DeltaOp abstraction
- [ ] Implement rsync token encoding/decoding
- [ ] Implement arsync native encoding/decoding
- [ ] Test delta generation/application
- [ ] Test with real rsync
- **Deliverable**: Can exchange deltas with rsync

## Phase 5: Integration & Testing (1-2 weeks)
- [ ] End-to-end rsync compatibility test
- [ ] Performance benchmarking
- [ ] Documentation
- [ ] Bug fixes
- **Deliverable**: Full rsync compatibility

**Total Estimated Time**: 7-10 weeks

---

# Risk Assessment

## High Risk

### 1. compio Process Support Missing
- **Impact**: Cannot spawn SSH easily
- **Mitigation**: Hybrid approach (stdlib process + io_uring FDs)
- **Fallback**: Use thread pool for SSH transport

### 2. rsync Protocol Complexity
- **Impact**: Many edge cases and versions
- **Mitigation**: Extensive testing with real rsync
- **Fallback**: Support only rsync 3.x (protocol >= 27)

## Medium Risk

### 3. Performance Regression
- **Impact**: Slower than native arsync
- **Mitigation**: Benchmark at each phase
- **Fallback**: Keep native protocol as default

### 4. Checksum Algorithm Compatibility
- **Impact**: MD4 vs MD5 confusion
- **Mitigation**: Detect from protocol version
- **Fallback**: Always use MD5 (modern rsync)

## Low Risk

### 5. Testing Coverage
- **Impact**: Bugs in production
- **Mitigation**: Multi-level testing strategy already in place
- **Fallback**: More integration tests

---

# Conclusion

This design provides a complete roadmap for implementing rsync wire protocol compatibility in arsync. Key innovations:

1. **State machine for handshake**: Clean, testable protocol negotiation
2. **compio integration**: Align with arsync's io_uring architecture
3. **Abstraction layers**: Support both rsync and arsync native formats
4. **Extensibility**: Easy to add new checksums and delta algorithms

**Next Steps**: 
1. Review this design document
2. Get feedback on approach
3. Begin Phase 1 implementation (handshake)

**Estimated Total Effort**: 7-10 weeks for full implementation

