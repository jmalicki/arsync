# rsync Pipe Protocol and Local Testing Infrastructure

**Purpose**: Document rsync's local pipe-based communication for testing  
**Date**: October 9, 2025  
**Status**: Design proposal

---

## Table of Contents

1. [rsync's Local Mode Architecture](#rsyncs-local-mode-architecture)
2. [Pipe-Based Communication Protocol](#pipe-based-communication-protocol)
3. [Testing Strategy with Pipes](#testing-strategy-with-pipes)
4. [Proposed Extension: --pipe Mode](#proposed-extension---pipe-mode)
5. [Implementation Design](#implementation-design)
6. [Testing Benefits](#testing-benefits)

---

## rsync's Local Mode Architecture

### How rsync Works Locally

When you run rsync for a local copy:
```bash
rsync -av /source/ /dest/
```

**What happens internally**:

```
rsync (main process)
  |
  ├─ Fork → Sender process
  |          ↓
  |       (reads /source/, generates file list and deltas)
  |          ↓
  └─ Fork → Receiver process
             ↓
          (receives file list, applies deltas to /dest/)

Communication: Sender ←→ Receiver via pipes
```

**Process Model**:
```c
// Simplified from rsync source (main.c)
int main(int argc, char *argv[]) {
    int f_in, f_out;  // File descriptors for pipes
    
    // Create pipes for bidirectional communication
    int to_child_pipe[2];
    int from_child_pipe[2];
    pipe(to_child_pipe);
    pipe(from_child_pipe);
    
    pid_t pid = fork();
    if (pid == 0) {
        // Receiver process
        close(to_child_pipe[1]);    // Close write end
        close(from_child_pipe[0]);   // Close read end
        f_in = to_child_pipe[0];
        f_out = from_child_pipe[1];
        receiver_process(f_in, f_out);
    } else {
        // Sender process
        close(to_child_pipe[0]);    // Close read end
        close(from_child_pipe[1]);   // Close write end
        f_in = from_child_pipe[0];
        f_out = to_child_pipe[1];
        sender_process(f_in, f_out);
    }
}
```

**Key Insight**: The **same wire protocol** used over SSH is used over pipes!

---

## Pipe-Based Communication Protocol

### Protocol is Transport-Agnostic

rsync's wire protocol works over **any bidirectional byte stream**:
- TCP socket (remote rsync daemon)
- SSH stdin/stdout (remote shell)
- **Unix pipes** (local rsync)
- Unix domain sockets

### Wire Format (Same Regardless of Transport)

```
Phase 1: Handshake
  Sender → Receiver: Protocol version (1 byte)
  Receiver → Sender: Protocol version (1 byte)
  
Phase 2: File List
  Sender → Receiver: File count (varint)
  For each file:
    Sender → Receiver: Path, size, mtime, mode, uid, gid
  Sender → Receiver: EOL marker
  
Phase 3: Per-File Sync
  For each file:
    Receiver → Sender: Block checksum list
    Sender → Receiver: Delta (copy blocks + literal data)
    
Phase 4: Metadata Update
  Receiver confirms completion
```

**This means**: We can test arsync's protocol implementation **without network, without SSH, just with pipes!**

---

## Testing Strategy with Pipes

### Existing Testing Approach (Process-Based)

```bash
# Run rsync as separate process
rsync -av /source/ /dest-rsync/

# Run arsync as separate process
arsync -av /source/ /dest-arsync/

# Compare outputs
diff -r /dest-rsync/ /dest-arsync/
```

**Limitations**:
- Can't inspect wire protocol
- Can't inject faults
- Can't test protocol edge cases
- Requires full file I/O

### Proposed Testing Approach (Pipe-Based)

```rust
// Create pipe pair
let (sender_read, receiver_write) = pipe();
let (receiver_read, sender_write) = pipe();

// Spawn sender task
tokio::spawn(async move {
    let mut sender = RsyncSender::new(sender_read, sender_write);
    sender.send_directory("/source/").await
});

// Spawn receiver task
tokio::spawn(async move {
    let mut receiver = RsyncReceiver::new(receiver_read, receiver_write);
    receiver.receive_to_directory("/dest/").await
});

// Can observe protocol messages!
// Can inject errors!
// Can test edge cases!
```

---

## Proposed Extension: --pipe Mode

### Design: `--pipe` Flag for Testing

**Purpose**: Run arsync in pipe mode for protocol testing

**Usage**:
```bash
# Sender mode (reads from pipe FD 0, writes to pipe FD 1)
arsync --pipe --sender /source/

# Receiver mode (reads from pipe FD 0, writes to pipe FD 1)
arsync --pipe --receiver /dest/
```

**Combined for Testing**:
```bash
# Create named pipes
mkfifo /tmp/to_receiver /tmp/from_receiver

# Run receiver in background
arsync --pipe --receiver /dest/ \
    < /tmp/to_receiver \
    > /tmp/from_receiver &

# Run sender
arsync --pipe --sender /source/ \
    > /tmp/to_receiver \
    < /tmp/from_receiver

# Or use process substitution
arsync --pipe --sender /source/ \
    | arsync --pipe --receiver /dest/
```

### Compatibility Testing

**Test arsync sender ↔ arsync receiver**:
```bash
arsync --pipe --sender /source/ | arsync --pipe --receiver /dest/
```

**Test arsync sender ↔ rsync receiver**:
```bash
arsync --pipe --sender /source/ | rsync --server --sender . /dest/
```

**Test rsync sender ↔ arsync receiver**:
```bash
rsync --server --sender . /source/ | arsync --pipe --receiver /dest/
```

### Non-Standard Extension: --pipe-debug Mode

**Purpose**: Inject debugging, logging, fault injection into protocol

```bash
# With protocol logging
arsync --pipe --sender --pipe-debug=log /source/ \
    | tee protocol.log \
    | arsync --pipe --receiver /dest/

# With fault injection
arsync --pipe --sender /source/ \
    | chaos-pipe --drop-rate=0.01 \  # Drop 1% of bytes
    | arsync --pipe --receiver /dest/
```

**Debug Features**:
- `--pipe-debug=log`: Log all protocol messages
- `--pipe-debug=hexdump`: Hex dump of wire protocol
- `--pipe-debug=checkpoint`: Save protocol state at intervals
- `--pipe-inject-error=<type>`: Inject specific error scenarios

---

## Implementation Design

### 1. Pipe Mode Infrastructure

```rust
// src/protocol/pipe.rs

use tokio::io::{AsyncRead, AsyncWrite};
use std::os::unix::io::{AsRawFd, FromRawFd};

pub struct PipeTransport {
    reader: Box<dyn AsyncRead + Unpin + Send>,
    writer: Box<dyn AsyncWrite + Unpin + Send>,
}

impl PipeTransport {
    /// Create from stdin/stdout (for --pipe mode)
    pub fn from_stdio() -> Result<Self> {
        let stdin = unsafe { std::fs::File::from_raw_fd(0) };
        let stdout = unsafe { std::fs::File::from_raw_fd(1) };
        
        Ok(Self {
            reader: Box::new(tokio::fs::File::from_std(stdin)),
            writer: Box::new(tokio::fs::File::from_std(stdout)),
        })
    }
    
    /// Create from pipe file descriptors (for testing)
    pub fn from_fds(read_fd: i32, write_fd: i32) -> Result<Self> {
        let reader = unsafe { std::fs::File::from_raw_fd(read_fd) };
        let writer = unsafe { std::fs::File::from_raw_fd(write_fd) };
        
        Ok(Self {
            reader: Box::new(tokio::fs::File::from_std(reader)),
            writer: Box::new(tokio::fs::File::from_std(writer)),
        })
    }
    
    /// Create from in-memory pipes (for unit testing)
    pub fn from_memory_pipe() -> (Self, Self) {
        let (a_read, b_write) = tokio::io::duplex(64 * 1024);
        let (b_read, a_write) = tokio::io::duplex(64 * 1024);
        
        let transport_a = Self {
            reader: Box::new(a_read),
            writer: Box::new(a_write),
        };
        
        let transport_b = Self {
            reader: Box::new(b_read),
            writer: Box::new(b_write),
        };
        
        (transport_a, transport_b)
    }
}

impl AsyncRead for PipeTransport {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.reader).poll_read(cx, buf)
    }
}

impl AsyncWrite for PipeTransport {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.writer).poll_write(cx, buf)
    }
    
    fn poll_flush(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.writer).poll_flush(cx)
    }
    
    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.writer).poll_shutdown(cx)
    }
}
```

### 2. Generic Transport Abstraction

```rust
// src/protocol/transport.rs

pub trait Transport: AsyncRead + AsyncWrite + Unpin + Send {}

impl Transport for PipeTransport {}
impl Transport for SshConnection {}
#[cfg(feature = "quic")]
impl Transport for QuicStream {}

// Now protocol code works with any transport!
pub async fn rsync_send<T: Transport>(
    transport: &mut T,
    source: &Path,
    args: &Args,
) -> Result<()> {
    // Send over any transport (pipe, SSH, QUIC)
    send_handshake(transport).await?;
    send_file_list(transport, source, args).await?;
    // ...
}
```

### 3. CLI Integration

```rust
// src/cli.rs additions

#[derive(Parser)]
pub struct Args {
    // ... existing fields ...
    
    /// Run in pipe mode (for protocol testing)
    #[arg(long, hide = true)]
    pub pipe: bool,
    
    /// Pipe role: sender or receiver
    #[arg(long, requires = "pipe", value_enum)]
    pub pipe_role: Option<PipeRole>,
    
    /// Debug pipe protocol
    #[arg(long, requires = "pipe")]
    pub pipe_debug: Option<PipeDebugMode>,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum PipeRole {
    Sender,
    Receiver,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum PipeDebugMode {
    Log,        // Log all messages
    Hexdump,    // Hex dump wire format
    Checkpoint, // Save state at intervals
}
```

### 4. Main Entry Point

```rust
// src/main.rs

async fn main() -> Result<()> {
    let args = Args::parse();
    
    if args.pipe {
        // Pipe mode: stdin/stdout communication
        let transport = PipeTransport::from_stdio()?;
        
        match args.pipe_role {
            Some(PipeRole::Sender) => {
                protocol::rsync_send(transport, &source, &args).await
            }
            Some(PipeRole::Receiver) => {
                protocol::rsync_receive(transport, &destination, &args).await
            }
            None => {
                bail!("--pipe requires --pipe-role")
            }
        }
    } else if source.is_remote() || destination.is_remote() {
        // Remote mode: SSH
        protocol::remote_sync(&args, &source, &destination).await
    } else {
        // Local mode: Direct file operations
        sync::sync_files(&args).await
    }
}
```

---

## Testing Benefits

### Unit Testing Protocol Logic

**Before** (requires full SSH setup):
```rust
#[test]
fn test_rsync_protocol() {
    // Need SSH server running
    // Need authentication setup
    // Slow, complex, brittle
}
```

**After** (pure in-memory):
```rust
#[tokio::test]
async fn test_rsync_protocol() {
    // Create in-memory pipe pair
    let (sender_transport, receiver_transport) = PipeTransport::from_memory_pipe();
    
    // Spawn sender
    let sender_task = tokio::spawn(async move {
        rsync_send(sender_transport, &source, &args).await
    });
    
    // Spawn receiver
    let receiver_task = tokio::spawn(async move {
        rsync_receive(receiver_transport, &dest, &args).await
    });
    
    // Both run concurrently via in-memory pipes!
    let (send_result, recv_result) = tokio::join!(sender_task, receiver_task);
    
    assert!(send_result.is_ok());
    assert!(recv_result.is_ok());
}
```

### Protocol Fuzzing

```rust
#[tokio::test]
async fn test_corrupted_file_list() {
    let (mut sender_transport, mut receiver_transport) = PipeTransport::from_memory_pipe();
    
    // Inject corruption
    let corruption_layer = CorruptionLayer::new(receiver_transport, |byte| {
        if rand::random::<f32>() < 0.01 {
            byte ^ 0xFF  // Flip all bits 1% of the time
        } else {
            byte
        }
    });
    
    let receiver_task = tokio::spawn(async move {
        rsync_receive(corruption_layer, &dest, &args).await
    });
    
    // Should detect corruption and fail gracefully
    assert!(receiver_task.await.is_err());
}
```

### Interoperability Testing

```bash
# Test arsync sender with real rsync receiver
arsync --pipe --sender /source/ | rsync --server --sender . /dest/

# Test real rsync sender with arsync receiver
rsync --server --sender . /source/ | arsync --pipe --receiver /dest/
```

### Performance Testing

```bash
# Benchmark wire protocol overhead
pv < /dev/zero | arsync --pipe --sender --dry-run | pv > /dev/null
# Shows: Protocol encoding/decoding throughput

# Compare protocols
arsync --pipe --sender /data/ | arsync --pipe --receiver /dest/  # Our protocol
rsync /data/ /dest/  # rsync's implementation
```

---

## Proposed Extension: Enhanced Testing Modes

### 1. Protocol Capture Mode

```bash
# Capture protocol for analysis
arsync --pipe --sender --capture=protocol.bin /source/ \
    | arsync --pipe --receiver /dest/

# Replay captured protocol
cat protocol.bin | arsync --pipe --receiver --replay /dest/
```

### 2. Protocol Comparison Mode

```bash
# Generate protocol trace from arsync
arsync --pipe --sender --trace=/tmp/arsync.trace /source/ \
    | arsync --pipe --receiver /dest/

# Generate protocol trace from rsync
rsync /source/ /dest/ --trace=/tmp/rsync.trace

# Compare traces
arsync-protocol-diff /tmp/arsync.trace /tmp/rsync.trace
```

### 3. Fault Injection Mode

```bash
# Inject network-like conditions
arsync --pipe --sender /source/ \
    | arsync --pipe-proxy \
        --latency=50ms \
        --packet-loss=0.01 \
        --bandwidth=100mbps \
    | arsync --pipe --receiver /dest/
```

### 4. Merkle Tree Comparison Mode

**Non-standard extension for testing**:
```bash
# Generate merkle tree via pipe protocol
arsync --pipe --sender --merkle-only /source/ \
    > /tmp/source.merkle

arsync --pipe --sender --merkle-only /dest/ \
    > /tmp/dest.merkle

# Compare merkle trees
diff /tmp/source.merkle /tmp/dest.merkle
```

---

## Implementation Design

### Module Structure

```
src/protocol/
├── mod.rs           # Entry point
├── transport.rs     # Transport trait (NEW)
├── pipe.rs          # Pipe transport (NEW)
├── ssh.rs           # SSH transport
├── rsync/
│   ├── mod.rs       # rsync protocol core
│   ├── sender.rs    # Sender implementation
│   ├── receiver.rs  # Receiver implementation
│   ├── handshake.rs # Protocol negotiation
│   ├── filelist.rs  # File list encoding/decoding
│   ├── checksum.rs  # Block checksums
│   └── delta.rs     # Delta generation/application
└── debug/
    ├── capture.rs   # Protocol capture
    ├── replay.rs    # Protocol replay
    └── inject.rs    # Fault injection
```

### Transport Trait

```rust
// src/protocol/transport.rs

use tokio::io::{AsyncRead, AsyncWrite};

/// Generic transport for rsync protocol
pub trait Transport: AsyncRead + AsyncWrite + Unpin + Send {
    /// Get transport name (for debugging)
    fn name(&self) -> &str {
        "unknown"
    }
    
    /// Check if transport supports parallel streams
    fn supports_multiplexing(&self) -> bool {
        false
    }
}
```

### Sender/Receiver Abstraction

```rust
// src/protocol/rsync/sender.rs

pub struct RsyncSender<T: Transport> {
    transport: T,
    protocol_version: u8,
    capabilities: Vec<Capability>,
}

impl<T: Transport> RsyncSender<T> {
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            protocol_version: 31,
            capabilities: vec![],
        }
    }
    
    pub async fn send_directory(&mut self, source: &Path, args: &Args) -> Result<()> {
        // 1. Handshake
        self.handshake().await?;
        
        // 2. Send file list
        let files = generate_file_list(source, args).await?;
        self.send_file_list(&files).await?;
        
        // 3. For each file, send deltas
        for file in files {
            let checksums = self.receive_checksums().await?;
            let delta = generate_delta(&file, &checksums).await?;
            self.send_delta(&delta).await?;
        }
        
        // 4. Finalize
        self.finalize().await?;
        
        Ok(())
    }
}

// src/protocol/rsync/receiver.rs

pub struct RsyncReceiver<T: Transport> {
    transport: T,
    protocol_version: u8,
}

impl<T: Transport> RsyncReceiver<T> {
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            protocol_version: 31,
        }
    }
    
    pub async fn receive_to_directory(&mut self, dest: &Path, args: &Args) -> Result<()> {
        // 1. Handshake
        self.handshake().await?;
        
        // 2. Receive file list
        let files = self.receive_file_list().await?;
        
        // 3. For each file, send checksums and apply delta
        for file in files {
            let checksums = generate_local_checksums(dest, &file).await?;
            self.send_checksums(&checksums).await?;
            
            let delta = self.receive_delta().await?;
            apply_delta(dest, &file, &delta).await?;
        }
        
        // 4. Finalize
        self.finalize().await?;
        
        Ok(())
    }
}
```

### Testing Infrastructure

```rust
// tests/protocol/pipe_tests.rs

#[tokio::test]
async fn test_sender_receiver_via_memory_pipe() {
    // Create in-memory pipes
    let (sender_transport, receiver_transport) = PipeTransport::from_memory_pipe();
    
    // Create test data
    let temp = TempDir::new().unwrap();
    let source = temp.path().join("source");
    let dest = temp.path().join("dest");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("file.txt"), "Hello, World!").unwrap();
    fs::create_dir(&dest).unwrap();
    
    let args = Args::test_default(source.clone(), dest.clone());
    
    // Spawn sender
    let sender_task = tokio::spawn(async move {
        let mut sender = RsyncSender::new(sender_transport);
        sender.send_directory(&source, &args).await
    });
    
    // Spawn receiver
    let receiver_task = tokio::spawn(async move {
        let mut receiver = RsyncReceiver::new(receiver_transport);
        receiver.receive_to_directory(&dest, &args).await
    });
    
    // Wait for both
    let (send_result, recv_result) = tokio::join!(sender_task, receiver_task);
    
    assert!(send_result.unwrap().is_ok());
    assert!(recv_result.unwrap().is_ok());
    
    // Verify file was transferred
    let content = fs::read(dest.join("file.txt")).unwrap();
    assert_eq!(content, b"Hello, World!");
}

#[tokio::test]
async fn test_interop_with_rsync() {
    use std::process::Stdio;
    use tokio::process::Command;
    
    let temp = TempDir::new().unwrap();
    let source = temp.path().join("source");
    let dest = temp.path().join("dest");
    
    // Create test data
    fs::create_dir(&source).unwrap();
    fs::write(source.join("file.txt"), "Test").unwrap();
    fs::create_dir(&dest).unwrap();
    
    // Start arsync sender | rsync receiver
    let arsync_sender = Command::new("target/debug/arsync")
        .arg("--pipe")
        .arg("--sender")
        .arg(&source)
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    
    let rsync_receiver = Command::new("rsync")
        .arg("--server")
        .arg("--sender")
        .arg(".")
        .arg(&dest)
        .stdin(arsync_sender.stdout.unwrap())
        .output()
        .await
        .unwrap();
    
    assert!(rsync_receiver.status.success());
    assert!(dest.join("file.txt").exists());
}
```

---

## Benefits of Pipe Mode

### 1. Fast Protocol Testing

**No network overhead**:
- In-memory pipes: ~40 GB/s
- Unix pipes: ~10 GB/s
- SSH: ~2 GB/s (encryption overhead)

**Result**: Protocol tests run 5-20x faster

### 2. Deterministic Testing

**No network variability**:
- No packet loss
- No reordering
- No latency jitter
- No bandwidth limits

**Result**: Tests are reproducible

### 3. Protocol Debugging

**Can capture exact wire format**:
```bash
arsync --pipe --sender /source/ | tee protocol.bin | hexdump -C
```

**Can replay captured protocols**:
```bash
cat protocol.bin | arsync --pipe --receiver /dest/
```

### 4. Interoperability Validation

**Test against real rsync**:
- arsync sender ↔ rsync receiver (push compatibility)
- rsync sender ↔ arsync receiver (pull compatibility)
- No network setup needed!

### 5. Fault Injection

**Simulate network conditions**:
```bash
arsync --pipe --sender /source/ \
    | tc-pipe --delay=100ms --loss=1% \
    | arsync --pipe --receiver /dest/
```

---

## Comparison with SSH Mode

| Aspect | Pipe Mode | SSH Mode | QUIC Mode |
|--------|-----------|----------|-----------|
| **Setup** | None | SSH keys | Certificate/PSK |
| **Overhead** | ~0 | ~10-20% (encryption) | ~5% (QUIC) |
| **Throughput** | 10-40 GB/s | 2-5 GB/s | 5-10 GB/s |
| **Testing** | ✅ Excellent | ⚠️ Complex | ⚠️ Complex |
| **Debugging** | ✅ Easy (tee, hexdump) | ❌ Encrypted | ❌ Encrypted |
| **Production** | ❌ Local only | ✅ Yes | ✅ Yes |

---

## Implementation Phases

### Phase 1: Basic Pipe Support (Week 1)

- [ ] Create `PipeTransport` struct
- [ ] Implement `Transport` trait
- [ ] Add `--pipe` CLI flag
- [ ] Test with simple file transfer

### Phase 2: Sender/Receiver Split (Week 2)

- [ ] Implement `RsyncSender`
- [ ] Implement `RsyncReceiver`
- [ ] Test sender | receiver via pipes

### Phase 3: rsync Interoperability (Week 3)

- [ ] Test arsync sender | rsync receiver
- [ ] Test rsync sender | arsync receiver
- [ ] Validate wire format compatibility

### Phase 4: Debug Infrastructure (Week 4)

- [ ] Protocol capture (`--capture`)
- [ ] Protocol replay (`--replay`)
- [ ] Hex dump mode (`--pipe-debug=hexdump`)

### Phase 5: Testing Extensions (Week 5)

- [ ] In-memory pipe testing
- [ ] Fault injection framework
- [ ] Performance benchmarks

---

## Example Test Cases

### Test 1: Basic File Transfer

```rust
#[tokio::test]
async fn test_basic_file_transfer_via_pipe() {
    let (sender_pipe, receiver_pipe) = PipeTransport::from_memory_pipe();
    
    let temp = TempDir::new().unwrap();
    let source = temp.path().join("source");
    let dest = temp.path().join("dest");
    
    fs::write(&source, "Hello, World!").unwrap();
    
    let args = Args::test_default(source.clone(), dest.clone());
    
    let send_task = tokio::spawn(async move {
        RsyncSender::new(sender_pipe)
            .send_file(&source, &args)
            .await
    });
    
    let recv_task = tokio::spawn(async move {
        RsyncReceiver::new(receiver_pipe)
            .receive_file(&dest, &args)
            .await
    });
    
    tokio::try_join!(send_task, recv_task).unwrap();
    
    assert_eq!(fs::read(&dest).unwrap(), b"Hello, World!");
}
```

### Test 2: Metadata Preservation

```rust
#[tokio::test]
async fn test_metadata_over_pipe() {
    let (sender_pipe, receiver_pipe) = PipeTransport::from_memory_pipe();
    
    // Create file with specific metadata
    let temp = TempDir::new().unwrap();
    let source = temp.path().join("source");
    fs::write(&source, "content").unwrap();
    fs::set_permissions(&source, fs::Permissions::from_mode(0o600)).unwrap();
    
    let mut args = Args::test_default(source.clone(), dest.clone());
    args.perms = true;  // Preserve permissions
    
    // Transfer via pipes
    // ... (sender and receiver tasks)
    
    // Verify metadata preserved
    let dest_meta = fs::metadata(&dest).unwrap();
    assert_eq!(dest_meta.permissions().mode() & 0o777, 0o600);
}
```

### Test 3: Delta Algorithm

```rust
#[tokio::test]
async fn test_delta_generation() {
    // Receiver has old version of file
    let old_content = b"The quick brown fox jumps over the lazy dog";
    
    // Sender has new version (one word changed)
    let new_content = b"The quick brown cat jumps over the lazy dog";
    
    let (sender_pipe, receiver_pipe) = PipeTransport::from_memory_pipe();
    
    // ... setup and transfer ...
    
    // Verify: Delta should be small (only changed block sent)
    let delta_size = capture_delta_size(&sender_pipe).await.unwrap();
    assert!(delta_size < new_content.len() / 2, "Delta should be smaller than sending whole file");
}
```

---

## rsync --server Mode

### How rsync Uses --server

When you run `rsync` over SSH, the remote rsync is invoked with `--server`:

```bash
# Client side
rsync -av /source/ user@host:/dest/

# What runs on server (invoked by SSH)
rsync --server --sender -vlogDtpr . /dest/
```

**The `--server` flag means**: "Read protocol from stdin, write to stdout"

### arsync --server Mode

We should implement the same:

```bash
# SSH invokes on remote side
ssh user@host "arsync --server --sender /dest/"

# arsync receives protocol on stdin, sends on stdout
```

**Implementation**:
```rust
if args.server {
    // Server mode: use stdin/stdout
    let transport = PipeTransport::from_stdio()?;
    
    if args.sender {
        // We're the sender (confusing naming from rsync)
        rsync_send(transport, &path, args).await?;
    } else {
        // We're the receiver
        rsync_receive(transport, &path, args).await?;
    }
}
```

---

## References

### rsync Source Code
- [main.c](https://github.com/RsyncProject/rsync/blob/master/main.c) - Process forking and pipe setup
- [io.c](https://github.com/RsyncProject/rsync/blob/master/io.c) - I/O over file descriptors
- [sender.c](https://github.com/RsyncProject/rsync/blob/master/sender.c) - Sender process logic
- [receiver.c](https://github.com/RsyncProject/rsync/blob/master/receiver.c) - Receiver process logic

### Testing Frameworks
- [tokio::io::duplex](https://docs.rs/tokio/latest/tokio/io/fn.duplex.html) - In-memory pipes for testing
- [assert_cmd](https://docs.rs/assert_cmd/) - Command testing
- [proptest](https://docs.rs/proptest/) - Property-based testing for protocols

### Similar Approaches
- [librsync](https://librsync.github.io/) - rsync algorithm as library (pipe-based API)
- [bup](https://github.com/bup/bup) - Uses pipe-based protocol testing

---

## Conclusion

**rsync's local mode already uses pipes** - we're just exposing this explicitly for testing!

**Benefits**:
1. ✅ **Fast testing**: No network overhead
2. ✅ **Protocol debugging**: Can capture/replay wire format
3. ✅ **Interoperability**: Test against real rsync via pipes
4. ✅ **Fault injection**: Simulate network conditions
5. ✅ **Unit testable**: In-memory pipes for fast tests

**Recommendation**:
- Implement `--pipe` mode early (Week 2-3)
- Use for all protocol development
- Keeps tests fast and deterministic
- Enables TDD (test-driven development) of protocol

**This is how we'll validate rsync wire protocol compatibility!**

---

**Document Version**: 1.0  
**Last Updated**: October 9, 2025  
**Status**: Design proposal - Ready for implementation

