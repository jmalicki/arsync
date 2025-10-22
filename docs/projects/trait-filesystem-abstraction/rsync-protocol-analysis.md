# rsync Protocol Compatibility Analysis

## The Critical Question

**Can the AsyncFileSystem traits work with the rsync protocol?**

## Understanding the rsync Protocol

After analyzing the code, the rsync protocol is **fundamentally different** from a filesystem API:

### rsync Protocol Operations (Batch-Oriented)

```
1. Handshake
   - Exchange protocol version
   - Negotiate capabilities

2. Send File List (ALL files at once)
   - Walk source tree
   - Send complete file list with metadata
   - Receiver gets entire list before processing

3. For Each File (in order):
   - Receiver: Send block checksums (if basis file exists)
   - Sender: Compute delta using checksums
   - Sender: Send delta instructions
   - Receiver: Apply delta to reconstruct file

4. Finalize
   - Flush buffers
   - Send completion message
```

### AsyncFileSystem Operations (Random Access)

```
- open_file(path) -> open any file at any time
- read_at(offset) -> read from any position
- write_at(offset) -> write to any position
- open_directory(path) -> open any directory
```

## The Fundamental Mismatch

| Aspect | rsync Protocol | AsyncFileSystem |
|--------|---------------|-----------------|
| **Access Pattern** | Sequential, batch | Random access |
| **File List** | Must send all at once | Walk individually |
| **Operations** | Delta-based sync | Direct read/write |
| **State** | Stateful stream | Stateless handles |
| **Multiplexing** | All I/O multiplexed | Independent operations |

## The Key Insight: Two Different Abstractions

The user's insight is crucial:

> "we shouldn't force batching on the local filesystem, we want a high level api that makes sense for both"

This suggests **TWO levels of abstraction**:

### Level 1: Low-Level Filesystem Operations
```rust
// Works for: Local filesystem, theoretical remote filesystem (NFS, FUSE, etc.)
AsyncFileSystem {
    open_file()
    create_file()
    open_directory()
    // Random access, individual operations
}
```

### Level 2: High-Level Sync Operations
```rust
// Works for: Both local and remote (handles batching internally)
SyncOperations {
    async fn sync_tree(src: &Path, dst: &Path) {
        // 1. Walk source tree
        let files = walk_tree(src).await?;
        
        // 2. Backend decides how to process:
        // - Local: Process files individually
        // - Protocol: Batch and send via rsync protocol
        
        // 3. Common logic: metadata comparison, what to copy, etc.
    }
}
```

## Proposed Design: Three-Layer Architecture

```
┌─────────────────────────────────────────┐
│   High-Level Sync API                   │
│   - sync_tree()                          │
│   - compare_metadata()                   │
│   - decide_what_to_copy()               │
│   Common logic for both local & remote  │
└─────────────┬───────────────────────────┘
              │
       ┌──────┴──────┐
       │             │
┌──────▼──────┐ ┌───▼──────────┐
│ Local Path  │ │ Remote Path  │
│             │ │              │
│ Uses:       │ │ Uses:        │
│ AsyncFS     │ │ SyncProtocol │
│ traits      │ │ (rsync, etc) │
└─────────────┘ └──────────────┘
```

### Layer 1: Backend Traits

**For Local Operations:**
```rust
trait AsyncFileSystem {
    // Random access filesystem operations
}
```

**For Remote Sync:**
```rust
trait SyncProtocol {
    // Batch-oriented sync operations
    async fn send_file_list(&mut self, files: Vec<FileEntry>) -> Result<()>;
    async fn receive_file_list(&mut self) -> Result<Vec<FileEntry>>;
    async fn transfer_file(&mut self, file: &FileEntry, basis: Option<&[u8]>) -> Result<Vec<u8>>;
}
```

### Layer 2: High-Level Operations

```rust
struct SyncEngine {
    source: SyncBackend,
    destination: SyncBackend,
}

enum SyncBackend {
    Local(LocalFileSystem),
    Remote(Box<dyn SyncProtocol>),
}

impl SyncEngine {
    async fn sync_directory(&self, src: &Path, dst: &Path) -> Result<SyncStats> {
        // 1. Walk source tree (common for both)
        let files = self.walk_tree(src).await?;
        
        // 2. Get destination state (common for both)
        let dst_files = self.get_existing_files(dst).await?;
        
        // 3. Compare and decide what to copy (common logic)
        let to_copy = self.compute_delta(&files, &dst_files);
        
        // 4. Transfer files (backend-specific)
        match (&self.source, &self.destination) {
            (Local(src_fs), Local(dst_fs)) => {
                // Local-to-local: Individual file copies
                for file in to_copy {
                    self.copy_file_local(src_fs, dst_fs, &file).await?;
                }
            }
            (Local(src_fs), Remote(protocol)) => {
                // Local-to-remote: Batch via protocol
                protocol.send_file_list(&to_copy).await?;
                for file in to_copy {
                    let content = src_fs.read_file(&file.path).await?;
                    protocol.transfer_file(&file, None).await?;
                }
            }
            (Remote(protocol), Local(dst_fs)) => {
                // Remote-to-local: Receive via protocol
                let files = protocol.receive_file_list().await?;
                for file in files {
                    let content = protocol.transfer_file(&file, None).await?;
                    dst_fs.write_file(&file.path, &content).await?;
                }
            }
        }
    }
    
    // Common operations used by all backends
    async fn walk_tree(&self, path: &Path) -> Result<Vec<FileEntry>> {
        // Same implementation for local and remote
    }
    
    fn compute_delta(&self, src: &[FileEntry], dst: &[FileEntry]) -> Vec<FileEntry> {
        // Same logic for all backends
    }
}
```

## What This Means for the Design

### The Good News

The **AsyncFileSystem traits are still valuable** for:
1. Local filesystem operations
2. Testing with mock filesystems
3. Potential future: Remote filesystem access (not sync protocol)
4. Building blocks for the high-level sync API

### The Adjustment Needed

Add a **parallel abstraction** for sync protocols:

```rust
// traits/sync_protocol.rs
trait SyncProtocol {
    async fn handshake(&mut self) -> Result<Capabilities>;
    async fn send_file_list(&mut self, files: Vec<FileEntry>) -> Result<()>;
    async fn receive_file_list(&mut self) -> Result<Vec<FileEntry>>;
    async fn send_file_content(&mut self, path: &Path, basis: Option<&[u8]>) -> Result<()>;
    async fn receive_file_content(&mut self, path: &Path) -> Result<Vec<u8>>;
}

// backends/rsync_protocol.rs
struct RsyncProtocol<T: Transport> {
    transport: T,
}

impl<T: Transport> SyncProtocol for RsyncProtocol<T> {
    // Implements the rsync wire protocol
}
```

### Common High-Level Logic

The **real value** comes from the high-level operations that work with **either** backend:

```rust
// High-level API that abstracts local vs remote
async fn sync_directory(
    src: impl AsRef<Path>,
    dst: SyncDestination,
    options: SyncOptions,
) -> Result<SyncStats> {
    match dst {
        SyncDestination::Local(path) => {
            // Use AsyncFileSystem traits
            sync_local_to_local(src, path, options).await
        }
        SyncDestination::Remote { protocol, path } => {
            // Use SyncProtocol trait
            sync_local_to_remote(src, protocol, path, options).await
        }
    }
}
```

## The Common Operations (Your Key Point)

You're absolutely right that these should be **shared** regardless of backend:

### 1. File Tree Walking
```rust
// Same for both local and remote source
async fn walk_tree(path: &Path, backend: &impl Backend) -> Result<Vec<FileEntry>> {
    // Recursively walk
    // Collect file metadata
    // Return complete list
}
```

### 2. Metadata Comparison
```rust
// Same logic for all backends
fn files_need_sync(src: &FileEntry, dst: Option<&FileEntry>) -> bool {
    match dst {
        None => true, // Doesn't exist
        Some(d) => {
            src.mtime != d.mtime ||
            src.size != d.size ||
            src.mode != d.mode
        }
    }
}
```

### 3. Copy Decision Logic
```rust
// Same logic for all backends
fn compute_sync_plan(
    src_files: &[FileEntry],
    dst_files: &[FileEntry],
    options: &SyncOptions,
) -> SyncPlan {
    // Determine what needs to be copied
    // Determine what needs metadata updates
    // Determine what should be deleted
}
```

### 4. Metadata Preservation
```rust
// Same logic, different backend implementation
async fn preserve_metadata(
    file: &FileEntry,
    dst_path: &Path,
    backend: &impl Backend,
) -> Result<()> {
    backend.set_mtime(dst_path, file.mtime).await?;
    backend.set_permissions(dst_path, file.mode).await?;
    backend.set_ownership(dst_path, file.uid, file.gid).await?;
}
```

## Revised Implementation Plan

### Phase 1-4: AsyncFileSystem Traits (As Planned)
- These are building blocks for local operations
- Used by high-level sync for local-to-local

### Phase 5: SyncProtocol Trait (New)
```rust
trait SyncProtocol {
    async fn send_file_list(&mut self, files: Vec<FileEntry>) -> Result<()>;
    async fn receive_file_list(&mut self) -> Result<Vec<FileEntry>>;
    async fn transfer_files(&mut self, files: &[FileEntry]) -> Result<SyncStats>;
}
```

### Phase 6: RsyncProtocol Backend
- Implement SyncProtocol for rsync wire protocol
- Uses existing Transport trait
- Handles multiplexing, deltas, etc.

### Phase 7: High-Level Sync Operations (The Key!)
```rust
struct SyncEngine {
    // Common operations
}

impl SyncEngine {
    // Tree walking (common)
    async fn walk_source_tree(&self, path: &Path) -> Result<Vec<FileEntry>>;
    
    // Metadata comparison (common)
    fn compute_changes(&self, src: &[FileEntry], dst: &[FileEntry]) -> ChangeSet;
    
    // Backend dispatch (handles batching internally)
    async fn apply_changes(&self, changes: ChangeSet, backend: Backend) -> Result<SyncStats>;
}
```

### Phase 8: Unified API
```rust
// User-facing API
pub async fn sync(src: &Path, dst: Destination, opts: SyncOptions) -> Result<SyncStats> {
    let engine = SyncEngine::new();
    
    // Walk source (always the same)
    let src_files = engine.walk_source_tree(src).await?;
    
    // Get destination state (backend-dependent)
    let dst_files = match &dst {
        Destination::Local(path) => engine.walk_local_tree(path).await?,
        Destination::Remote(conn) => conn.get_file_list().await?,
    };
    
    // Compare (always the same)
    let changes = engine.compute_changes(&src_files, &dst_files);
    
    // Apply (backend handles batching)
    engine.apply_changes(changes, dst).await
}
```

## Key Takeaways

1. **Two Abstractions Needed**:
   - `AsyncFileSystem` for random-access filesystem operations (local)
   - `SyncProtocol` for batch-oriented sync operations (remote)

2. **Common High-Level Logic** (Your Key Insight!):
   - File tree walking
   - Metadata comparison
   - Sync decision making
   - Metadata preservation
   
   These are **backend-agnostic** and shared by all implementations.

3. **Backend Handles Batching**:
   - Local backend: Doesn't need batching
   - Protocol backend: Batches internally
   - High-level API doesn't force either approach

4. **The Traits Are Building Blocks**:
   - Not end-user API
   - Used by high-level sync operations
   - Enable testing and flexibility

## Updated Design Document

The main design document should be updated to:
1. Clarify that AsyncFileSystem traits are for LOCAL operations
2. Add SyncProtocol trait for remote sync
3. Emphasize the HIGH-LEVEL operations that abstract both
4. Show how common logic (tree walking, metadata, etc.) is shared

This gives us the best of both worlds:
- ✅ Clean abstractions for local filesystem
- ✅ Proper protocol support for remote sync
- ✅ Shared logic for common operations
- ✅ No forced batching on local filesystem
- ✅ High-level API that works for both

