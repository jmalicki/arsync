# Streaming vs Batch Operations

## The Key Insight

**Local filesystem operations should stream, not batch.**

### Anti-Pattern: Forced Batching
```rust
// BAD: Forces local filesystem to accumulate unnecessarily
async fn sync(src, dst) {
    // 1. Walk entire tree first (OK)
    let files = walk_tree(src).await?;
    
    // 2. Build sync plan (UNNECESSARY for local!)
    let plan = compute_plan(&files, &dst_files);
    
    // 3. Execute plan (WASTE - could have copied while walking!)
    execute_plan(plan).await?;
}
```

### Correct Pattern: Backend-Specific Behavior
```rust
// GOOD: Backend decides whether to stream or batch
async fn sync(src, dst) {
    match (src, dst) {
        (Local, Local) => sync_streaming(src, dst).await,
        (Local, Remote) => sync_with_protocol(src, dst).await,
    }
}

// Streaming for local-to-local
async fn sync_streaming(src, dst) {
    // Walk and copy simultaneously
    walk_and_process(src, |entry| async {
        if needs_copy(entry, dst).await? {
            copy_file(entry, dst).await?;  // Immediate!
        }
    }).await
}

// Batching for protocol
async fn sync_with_protocol(src, dst) {
    // 1. Accumulate file list
    let files = walk_tree(src).await?;
    
    // 2. Send entire list to protocol
    dst.send_file_list(&files).await?;
    
    // 3. Transfer files via protocol
    dst.transfer_files(&files).await?;
}
```

## Backend Implementation Strategies

### Local Backend: Zero Accumulation

```rust
impl SyncBackend for LocalFileSystem {
    async fn sync_directory(&self, src: &Path, dst: &Path) -> Result<SyncStats> {
        let mut stats = SyncStats::default();
        
        // Stream: Walk and process each entry immediately
        for entry in walk_dir_recursive(src).await? {
            let entry = entry?;
            let dst_path = dst.join(entry.relative_path());
            
            // Check if copy needed
            let needs_copy = match get_metadata(&dst_path).await {
                Ok(dst_meta) => {
                    entry.metadata().mtime != dst_meta.mtime ||
                    entry.metadata().size != dst_meta.size
                }
                Err(_) => true, // Doesn't exist
            };
            
            // Copy immediately if needed
            if needs_copy {
                self.copy_file(entry.path(), &dst_path).await?;
                stats.files_copied += 1;
            }
        }
        
        Ok(stats)
    }
}
```

### Protocol Backend: Accumulate Then Send

```rust
impl SyncBackend for RsyncProtocol<T> {
    async fn sync_directory(&self, src: &Path, dst: &Path) -> Result<SyncStats> {
        // Phase 1: Accumulate (required by protocol)
        let mut files = Vec::new();
        for entry in walk_dir_recursive(src).await? {
            files.push(FileEntry {
                path: entry.relative_path(),
                size: entry.metadata().size,
                mtime: entry.metadata().mtime,
                mode: entry.metadata().mode,
                // ... other metadata
            });
        }
        
        // Phase 2: Send file list (protocol requirement)
        self.send_file_list(&files).await?;
        
        // Phase 3: Transfer files (protocol handles delta, etc.)
        self.transfer_files(src, &files).await?;
        
        Ok(SyncStats {
            files_copied: files.len() as u64,
            // ...
        })
    }
}
```

## The Unified API: Backend Decides

```rust
pub trait SyncBackend {
    /// Sync a directory tree from src to dst
    /// 
    /// Implementation is backend-specific:
    /// - Local: Streams (walk and copy immediately)
    /// - Protocol: Batches (accumulate, send list, transfer)
    async fn sync_directory(&self, src: &Path, dst: &Path) -> Result<SyncStats>;
}

// High-level API that dispatches to appropriate backend
pub async fn sync(src: &Path, dst: Destination) -> Result<SyncStats> {
    match dst {
        Destination::Local(path) => {
            let backend = LocalFileSystem;
            backend.sync_directory(src, path).await  // Streams!
        }
        Destination::Remote(conn) => {
            let backend = RsyncProtocol::new(conn);
            backend.sync_directory(src, remote_path).await  // Batches!
        }
    }
}
```

## Common Operations: Reusable Components

Even with different execution strategies, we share **components**:

### 1. Tree Walking (Shared)
```rust
// Async iterator over directory tree
pub async fn walk_tree(path: &Path) -> impl Stream<Item = Result<DirEntry>> {
    // Implementation using walkdir or compio-fs-extended
    // Returns entries one at a time
}
```

### 2. Metadata Comparison (Shared)
```rust
// Decide if file needs syncing
pub fn needs_sync(src_meta: &Metadata, dst_meta: Option<&Metadata>) -> bool {
    match dst_meta {
        None => true,
        Some(dst) => {
            src_meta.mtime != dst.mtime ||
            src_meta.size != dst.size
        }
    }
}
```

### 3. Metadata Preservation (Shared)
```rust
// Preserve metadata after copy
pub async fn preserve_metadata(
    src_meta: &Metadata,
    dst_path: &Path,
    backend: &impl Backend,
) -> Result<()> {
    backend.set_times(dst_path, src_meta.mtime, src_meta.atime).await?;
    backend.set_permissions(dst_path, src_meta.mode).await?;
    backend.set_ownership(dst_path, src_meta.uid, src_meta.gid).await?;
    Ok(())
}
```

### 4. Progress Reporting (Shared)
```rust
// Progress callback during sync
pub trait ProgressReporter {
    fn on_file_start(&mut self, path: &Path, size: u64);
    fn on_file_complete(&mut self, path: &Path, bytes: u64);
    fn on_error(&mut self, path: &Path, error: &Error);
}
```

## Example: Streaming Local Sync with Shared Components

```rust
impl SyncBackend for LocalFileSystem {
    async fn sync_directory(&self, src: &Path, dst: &Path) -> Result<SyncStats> {
        let mut stats = SyncStats::default();
        
        // Use shared tree walker
        let mut entries = walk_tree(src).await;
        
        while let Some(entry) = entries.next().await {
            let entry = entry?;
            let dst_path = dst.join(entry.relative_path());
            
            // Get destination metadata if exists
            let dst_meta = self.metadata(&dst_path).await.ok();
            
            // Use shared comparison logic
            if needs_sync(entry.metadata(), dst_meta.as_ref()) {
                // Copy file
                self.copy_file(entry.path(), &dst_path).await?;
                
                // Use shared metadata preservation
                preserve_metadata(entry.metadata(), &dst_path, self).await?;
                
                stats.files_copied += 1;
                stats.bytes_copied += entry.metadata().size;
            }
        }
        
        Ok(stats)
    }
}
```

## Example: Batching Protocol Sync with Shared Components

```rust
impl SyncBackend for RsyncProtocol<T> {
    async fn sync_directory(&self, src: &Path, dst: &Path) -> Result<SyncStats> {
        // Phase 1: Accumulate using shared tree walker
        let mut files = Vec::new();
        let mut entries = walk_tree(src).await;
        
        while let Some(entry) = entries.next().await {
            let entry = entry?;
            files.push(FileEntry {
                path: entry.relative_path(),
                // ... convert metadata
            });
        }
        
        // Phase 2: Protocol-specific batching
        self.send_file_list(&files).await?;
        
        // Phase 3: Transfer with protocol-specific logic
        for file in &files {
            let content = read_file(src.join(&file.path)).await?;
            self.transfer_file(file, &content).await?;
            
            // Still use shared metadata preservation on receive side!
        }
        
        Ok(SyncStats {
            files_copied: files.len() as u64,
            // ...
        })
    }
}
```

## No-Op Plan/Execute for Local

Your suggestion for the local backend:

```rust
impl SyncBackend for LocalFileSystem {
    // Option 1: No separate plan phase
    async fn sync_directory(&self, src: &Path, dst: &Path) -> Result<SyncStats> {
        // Walk and execute immediately - no plan object
        self.stream_sync(src, dst).await
    }
    
    // Or Option 2: Plan is a no-op iterator that executes immediately
    async fn compute_plan(&self, src: &Path, dst: &Path) -> impl Iterator<Item = SyncOp> {
        // Returns iterator that computes operations on-demand
        // No accumulation - just a streaming adapter
        StreamingPlanIterator::new(src, dst)
    }
    
    async fn execute_plan(&self, plan: impl Iterator<Item = SyncOp>) -> Result<SyncStats> {
        // For local backend, this just consumes the iterator
        // which triggers the actual copies
        let mut stats = SyncStats::default();
        for op in plan {
            match op {
                SyncOp::Copy { src, dst } => {
                    self.copy_file(&src, &dst).await?;
                    stats.files_copied += 1;
                }
                // ...
            }
        }
        Ok(stats)
    }
}
```

## Performance Comparison

### Local Streaming (Good!)
```
Time: t
Disk I/O: Walk tree, copy files
Memory: O(1) - process one entry at a time
Latency: Low - start copying immediately
```

### Local with Batching (Bad!)
```
Time: t + overhead
Disk I/O: Walk tree, copy files + extra scan
Memory: O(n) - store entire plan
Latency: High - wait for full tree walk before first copy
```

### Protocol Batching (Required!)
```
Time: t
Network I/O: Send file list, transfer files
Memory: O(n) - required by protocol
Latency: High - protocol requirement, unavoidable
```

## Key Takeaways

1. **Local Backend**: Stream everything
   - Walk → Check → Copy → Next (immediate)
   - No accumulation of sync plan
   - Minimal memory usage
   - Start copying ASAP

2. **Protocol Backend**: Batch required operations
   - Walk → Accumulate → Send list → Transfer
   - Accumulation required by protocol
   - Higher memory (but unavoidable)
   - Latency from protocol handshaking

3. **Shared Components**: Reusable logic
   - Tree walking iterator
   - Metadata comparison
   - Metadata preservation
   - Progress reporting
   - Error handling

4. **No Forced Pattern**: Backend decides
   - Trait doesn't mandate batch-then-execute
   - Implementation can stream or batch
   - High-level API just calls `sync_directory()`

5. **Plan/Execute can be No-Op**:
   - Local: Plan is streaming iterator, execute just consumes it
   - Protocol: Plan is Vec, execute sends via protocol
   - Same trait, different strategies

## Updated Trait Design

```rust
pub trait SyncBackend {
    /// Sync directory from src to dst
    /// 
    /// Implementation decides execution strategy:
    /// - Streaming (local)
    /// - Batching (protocol)
    async fn sync_directory(
        &self,
        src: &Path,
        dst: &Path,
        options: &SyncOptions,
    ) -> Result<SyncStats>;
}

// Shared components available to all backends
pub mod components {
    pub async fn walk_tree(path: &Path) -> impl Stream<Item = Result<Entry>>;
    pub fn needs_sync(src: &Metadata, dst: Option<&Metadata>) -> bool;
    pub async fn preserve_metadata(meta: &Metadata, path: &Path) -> Result<()>;
}
```

This gives us:
- ✅ Streaming for local (no forced batching)
- ✅ Batching for protocol (where needed)
- ✅ Shared components (maximum reuse)
- ✅ Clean abstraction (backend decides strategy)

