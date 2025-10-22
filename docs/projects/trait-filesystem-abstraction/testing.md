# Testing Strategy for Trait-Based Filesystem Abstraction

## Overview

Comprehensive testing strategy for validating the trait-based filesystem abstraction at all levels.

## Testing Pyramid

```
             ┌──────────────┐
             │ E2E Tests    │ ← CLI integration
             └──────┬───────┘
                    │
         ┌──────────▼──────────┐
         │ Integration Tests   │ ← Real filesystem ops
         └──────────┬──────────┘
                    │
         ┌──────────▼──────────┐
         │ Component Tests     │ ← Individual traits
         └──────────┬──────────┘
                    │
              ┌─────▼─────┐
              │ Unit Tests│ ← Provided methods
              └───────────┘
```

## Generic Test Helper Pattern

**Key Innovation**: Reusable test functions that work with ANY trait implementation.

### Structure

1. **Generic Test Helpers** - Functions without `#[test]` attribute
   - Generic over trait types
   - Test specific behavior (e.g., "handles short reads correctly")
   - Reusable by any implementation

2. **Concrete Test Instantiations** - Normal test functions with `#[compio::test]`
   - Set up mocks/fixtures
   - Call generic helpers
   - Get comprehensive coverage

### Example Implementation

**Location**: `src/traits/file.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    // =========================================================================
    // Generic Test Helpers - Reusable for ANY AsyncFile implementation
    // =========================================================================
    
    /// Test write_all_at() with partial writes
    /// 
    /// Requires: File that simulates partial writes (returns < requested bytes)
    pub async fn test_write_all_at_handles_partial_writes<F, M>(
        file: F,
        test_data: &[u8],
        start_offset: u64,
        verify_written: impl FnOnce() -> Vec<(u64, Vec<u8>)>,
    )
    where
        F: AsyncFile<Metadata = M>,
        M: AsyncMetadata,
    {
        file.write_all_at(test_data, start_offset).await.unwrap();
        let writes = verify_written();
        
        // Verify multiple writes occurred
        assert!(writes.len() > 1);
        
        // Verify data integrity
        let mut reconstructed = Vec::new();
        for (_offset, data) in writes.iter() {
            reconstructed.extend_from_slice(data);
        }
        assert_eq!(reconstructed, test_data);
    }
    
    /// Test streaming read pattern with short reads
    pub async fn test_streaming_pattern_with_short_reads<F, M>(
        file: F,
        expected_data: &[u8],
        buffer_size: usize,
    )
    where
        F: AsyncFile<Metadata = M>,
        M: AsyncMetadata,
    {
        let mut result = Vec::new();
        let mut offset = 0;
        let mut buffer = vec![0u8; buffer_size];

        loop {
            let (n, buf) = file.read_at(buffer, offset).await.unwrap();
            if n == 0 { break; }
            result.extend_from_slice(&buf[..n]);
            buffer = buf;
            offset += n as u64;
        }

        assert_eq!(result, expected_data);
    }
    
    // =========================================================================
    // Concrete Test Instantiations - Call helpers with mocks
    // =========================================================================
    
    struct ShortReadMockFile { /* mock that returns < requested bytes */ }
    impl AsyncFile for ShortReadMockFile { /* ... */ }
    
    #[compio::test]
    async fn test_streaming_read_with_short_reads() {
        let content: Vec<u8> = (0..100).collect();
        let file = ShortReadMockFile { content: content.clone() };
        
        // Call generic helper - verifies behavior with this mock
        test_streaming_pattern_with_short_reads(file, &content, 64).await;
    }
    
    struct PartialWriteMockFile { /* mock that writes < requested bytes */ }
    impl AsyncFile for PartialWriteMockFile { /* ... */ }
    
    #[compio::test]
    async fn test_write_all_at_with_partial_writes() {
        let file = PartialWriteMockFile::new();
        let test_data = b"Hello, World!";
        
        // Call generic helper - verifies behavior with this mock
        test_write_all_at_handles_partial_writes(
            file,
            test_data,
            0,
            || get_written_data()
        ).await;
    }
}
```

### Benefits

✅ **Reusable** - Any implementation can use these helpers  
✅ **No macros** - Simple manual instantiation  
✅ **Works with compio::test** - No attribute conflicts  
✅ **Type-safe** - Generic over any AsyncFile  
✅ **Clear intent** - Explicitly shows what's being tested  
✅ **Easy to extend** - Add new helpers as needed

### Usage for New Implementations

When implementing a new backend (e.g., `LocalFile`, `RemoteFile`):

```rust
// In tests/local_file_tests.rs

use arsync::traits::file::tests::*; // Import generic helpers

#[compio::test]
async fn test_local_file_short_reads() {
    let temp = TempDir::new().unwrap();
    let file = LocalFile::open_with_short_reads(&temp.path().join("test.txt")).await.unwrap();
    
    // Reuse generic helper - instant comprehensive coverage!
    test_streaming_pattern_with_short_reads(file, &expected_data, 64).await;
}

#[compio::test]
async fn test_local_file_partial_writes() {
    let file = LocalFile::new_with_partial_writes().unwrap();
    
    // Reuse generic helper
    test_write_all_at_handles_partial_writes(
        file,
        b"test data",
        0,
        || get_local_file_writes()
    ).await;
}
```

### Critical Properties Tested

Generic helpers verify essential correctness properties:

1. **Data Integrity**
   - `test_streaming_pattern_with_short_reads` - Data correct despite short reads
   - `test_write_all_at_handles_partial_writes` - Data correct despite partial writes
   - `test_copy_loop_with_short_reads` - End-to-end copy maintains data integrity

2. **Completeness**
   - All data read/written
   - No data lost
   - Offsets advance correctly

3. **Error Handling**
   - `test_write_all_at_zero_write_error` - Zero-byte writes detected
   - Appropriate error types returned

4. **Buffer Management**
   - Buffers reused correctly (compio pattern)
   - No buffer leaks
   - Correct ownership transfer

### Test Organization

```
src/traits/
├── file.rs
│   └── tests/
│       ├── Generic helpers (pub async fn)
│       ├── Mock implementations
│       └── Concrete test instantiations (#[compio::test])
│
├── metadata.rs
│   └── tests/
│       └── (same pattern)
│
└── directory.rs
    └── tests/
        └── (same pattern)

tests/
├── local_file_tests.rs      # Calls generic helpers with LocalFile
├── remote_file_tests.rs     # Calls generic helpers with RemoteFile
└── mock_backend_tests.rs    # Calls generic helpers with MockFile
```

### Why Not Use Test Macros?

We chose **manual instantiation** over macros (rstest, test-case) because:

1. **compio::test compatibility** - Macros don't support custom async test attributes
2. **Simplicity** - No macro debugging, just function calls
3. **Type safety** - Full IDE support and type checking
4. **Flexibility** - Easy to customize per-implementation
5. **Clarity** - Explicit what each test does

## Unit Tests

### Trait Provided Methods

Test default implementations on traits:

**Location**: `src/traits/metadata.rs`, `src/traits/file.rs`, etc.

**Pattern**:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    struct MockMetadata { /* fields */ }
    
    impl AsyncMetadata for MockMetadata {
        // Implement required methods
    }
    
    #[test]
    fn test_is_empty() {
        let meta = MockMetadata { size: 0, ... };
        assert!(meta.is_empty());
    }
    
    #[test]
    fn test_file_type_description() {
        let meta = MockMetadata { is_file: true, size: 1024, ... };
        assert_eq!(meta.file_type_description(), "file (1024 bytes)");
    }
}
```

**Coverage**: Every provided method with default implementation

## Component Tests

### Mock Backend Implementation

**Location**: `src/backends/mock.rs` (add in Phase 5)

**Purpose**: Testing filesystem operations without real I/O

```rust
pub struct MockFileSystem {
    files: HashMap<PathBuf, Vec<u8>>,
    metadata: HashMap<PathBuf, FileMetadata>,
}

impl AsyncFileSystem for MockFileSystem {
    type File = MockFile;
    type Directory = MockDirectory;
    type Metadata = FileMetadata;
    
    async fn open_file(&self, path: &Path) -> Result<Self::File> {
        if let Some(content) = self.files.get(path) {
            Ok(MockFile {
                path: path.to_path_buf(),
                content: content.clone(),
            })
        } else {
            Err(SyncError::FileNotFound)
        }
    }
    
    // ... other methods
}
```

**Tests**:
```rust
#[compio::test]
async fn test_copy_file_with_mock_filesystem() {
    let mut fs = MockFileSystem::new();
    fs.add_file("/src/test.txt", b"Hello, World!");
    
    let ops = GenericFileOperations::new(fs, 64 * 1024);
    ops.copy_file("/src/test.txt", "/dst/test.txt").await?;
    
    assert_eq!(fs.read_file("/dst/test.txt")?, b"Hello, World!");
}
```

### Shared Operations Tests

**Location**: `src/filesystem/walker.rs`, `src/filesystem/read.rs`, etc.

**Pattern**:
```rust
#[compio::test]
async fn test_secure_tree_walker() {
    let temp_dir = TempDir::new()?;
    // Create test structure
    create_test_tree(&temp_dir).await?;
    
    // Test walker
    let dir_fd = DirectoryFd::open(temp_dir.path()).await?;
    let walker = SecureTreeWalker::new(&dir_fd).await?;
    
    let mut entries = Vec::new();
    for entry in walker.walk().await {
        entries.push(entry?);
    }
    
    // Verify all files found
    assert_eq!(entries.len(), expected_count);
    
    // Verify DirectoryFd usage (no path-based ops)
    // Verify TOCTOU-safe
}
```

## Integration Tests

### Real Filesystem Operations

**Location**: `tests/trait_*_integration.rs`

**Coverage**:
1. **Metadata operations** (PR #2)
   - File metadata via trait
   - Directory metadata
   - Hardlink detection
   - Symlink metadata

2. **File operations** (PR #4-5)
   - Read/write via trait
   - Buffer ownership pattern
   - Sync operations
   - Large files (> buffer size)

3. **Directory operations** (PR #7)
   - Directory listing
   - Recursive traversal
   - Entry metadata caching

4. **Complete filesystem** (PR #12, #17)
   - Full copy operations
   - Metadata preservation
   - Progress tracking
   - Error handling

### Pattern

```rust
#[compio::test]
async fn test_copy_with_metadata_preservation() {
    let temp_src = TempDir::new()?;
    let temp_dst = TempDir::new()?;
    
    // Create source file with specific metadata
    let src_file = temp_src.path().join("test.txt");
    std::fs::write(&src_file, b"test content")?;
    std::fs::set_permissions(&src_file, Permissions::from_mode(0o644))?;
    
    // Copy using trait-based API
    let fs = LocalFileSystem;
    fs.copy_file(&src_file, &temp_dst.path().join("test.txt")).await?;
    
    // Verify content matches
    assert_eq!(
        std::fs::read(&temp_dst.path().join("test.txt"))?,
        b"test content"
    );
    
    // Verify metadata preserved
    let dst_meta = std::fs::metadata(&temp_dst.path().join("test.txt"))?;
    assert_eq!(dst_meta.permissions().mode() & 0o777, 0o644);
}
```

## Security Tests

### TOCTOU Vulnerability Tests

**Purpose**: Verify operations are TOCTOU-safe

```rust
#[compio::test]
async fn test_toctou_safety_symlink_attack() {
    // Create test directory structure
    let temp = TempDir::new()?;
    let dir = temp.path().join("dir");
    std::fs::create_dir(&dir)?;
    
    // Open DirectoryFd
    let dir_fd = DirectoryFd::open(&dir).await?;
    
    // Create a regular file
    std::fs::write(dir.join("target"), b"original")?;
    
    // Simulate attack: Replace file with symlink to /etc/passwd
    std::fs::remove_file(dir.join("target"))?;
    std::os::unix::fs::symlink("/etc/passwd", dir.join("target"))?;
    
    // Try to open with DirectoryFd (should fail due to O_NOFOLLOW)
    let result = dir_fd.open_file_at(
        std::ffi::OsStr::new("target"),
        true, false, false, false
    ).await;
    
    // Should fail - O_NOFOLLOW prevents following symlink
    assert!(result.is_err());
}
```

### Symlink Attack Prevention

```rust
#[compio::test]
async fn test_symlink_attack_prevention() {
    // Verify all operations use O_NOFOLLOW
    // Verify DirectoryFd prevents path traversal
    // Verify no path-based operations after initial open
}
```

## Performance Tests

### Syscall Counting

**Purpose**: Verify stat() called once per file

```rust
#[compio::test]
async fn test_single_stat_per_file() {
    let temp_dir = TempDir::new()?;
    create_test_tree(&temp_dir, 100)?; // 100 files
    
    // Wrap in syscall counter (using strace or dtrace)
    let syscall_counter = SyscallCounter::new();
    
    let walker = SecureTreeWalker::new(temp_dir.path()).await?;
    let entries = walker.walk().await.collect::<Vec<_>>().await;
    
    let stats = syscall_counter.stop();
    
    // Should have ~100 statx calls (one per file)
    // NOT 200+ (which would indicate redundant stats)
    assert!(stats.statx_count <= 110); // Some overhead OK
    assert!(entries.len() == 100);
}
```

### Performance Benchmarks

**Location**: `benches/trait_performance.rs`

```rust
#[bench]
fn bench_copy_file_direct_vs_trait(b: &mut Bencher) {
    // Benchmark: Direct compio vs trait-based
    // Ensure < 5% overhead
}

#[bench]
fn bench_directory_walk_direct_vs_trait(b: &mut Bencher) {
    // Benchmark: Direct walk vs SecureTreeWalker
    // Ensure equivalent performance
}
```

## Regression Tests

### Comparison with Existing Code

**Purpose**: Ensure new implementation matches old behavior

```rust
#[compio::test]
async fn test_copy_file_v2_matches_copy_file() {
    let temp = TempDir::new()?;
    let src = temp.path().join("src.txt");
    let dst1 = temp.path().join("dst1.txt");
    let dst2 = temp.path().join("dst2.txt");
    
    std::fs::write(&src, b"test data")?;
    
    // Old implementation
    copy_file(&src, &dst1, &config).await?;
    
    // New implementation  
    copy_file_v2(&src, &dst2, &config).await?;
    
    // Should be identical
    assert_eq!(
        std::fs::read(&dst1)?,
        std::fs::read(&dst2)?
    );
    
    // Metadata should match
    assert_metadata_equal(&dst1, &dst2)?;
}
```

## Protocol Tests

### rsync Protocol Integration

**Location**: `tests/protocol_with_traits.rs` (add in Phase 6)

```rust
#[compio::test]
async fn test_rsync_protocol_uses_shared_operations() {
    // Verify protocol backend uses SecureTreeWalker
    // Verify protocol uses read_file_content (not fs::read)
    // Verify protocol uses DirectoryFd
    // Verify no duplicate file I/O code
}
```

## Test Data Generation

### Helper Functions

```rust
pub async fn create_test_tree(base: &Path, num_files: usize) -> Result<()> {
    for i in 0..num_files {
        let path = base.join(format!("file_{}.txt", i));
        std::fs::write(&path, format!("Content {}", i).as_bytes())?;
    }
    Ok(())
}

pub fn assert_metadata_equal(path1: &Path, path2: &Path) -> Result<()> {
    let meta1 = std::fs::metadata(path1)?;
    let meta2 = std::fs::metadata(path2)?;
    
    assert_eq!(meta1.len(), meta2.len());
    assert_eq!(meta1.permissions(), meta2.permissions());
    // ... other fields
    Ok(())
}
```

## Continuous Integration

### GitHub Actions Workflow

Run all test levels on each PR:

```yaml
- name: Unit Tests
  run: cargo test --lib
  
- name: Integration Tests
  run: cargo test --test '*'
  
- name: Doc Tests
  run: cargo test --doc
  
- name: Benchmarks (comparison only)
  run: cargo bench --no-run
```

## Test Organization

```
tests/
├── trait_metadata_integration.rs      # PR #2
├── trait_file_integration.rs          # PR #4  
├── trait_directory_integration.rs     # PR #7
├── shared_operations_test.rs          # PR #8-11
├── local_backend_test.rs              # PR #16
├── protocol_backend_test.rs           # PR #21
├── unified_api_test.rs                # PR #23
└── regression_tests.rs                # PR #12, #17, #24

src/
├── traits/
│   ├── metadata.rs  # Unit tests for provided methods
│   ├── file.rs      # Unit tests for provided methods
│   └── directory.rs # Unit tests for provided methods
│
├── filesystem/
│   ├── walker.rs    # Component tests for SecureTreeWalker
│   ├── read.rs      # Component tests for read_file_content
│   └── write.rs     # Component tests for write_file_content
│
└── backends/
    ├── mock.rs      # Mock backend for testing
    ├── local.rs     # Integration tests
    └── protocol.rs  # Protocol-specific tests
```

## Quality Gates

Each PR must pass:

1. **Unit Tests**: All trait provided methods
2. **Integration Tests**: Works with real filesystem
3. **Security Tests**: TOCTOU-safe, no symlink attacks
4. **Performance Tests**: No regression vs baseline
5. **Regression Tests**: Matches existing behavior (where replacing)

## Test Execution Order

For each PR:

```bash
# 1. Unit tests (fast)
cargo test --lib

# 2. Integration tests (slower)
cargo test --test '*'

# 3. Security validation (manual or automated)
./scripts/security-audit.sh

# 4. Performance comparison
cargo bench --bench trait_performance

# 5. Full test suite
cargo test --all
```

## Mock Backend Example

Complete mock implementation for testing:

```rust
pub struct MockFileSystem {
    files: HashMap<PathBuf, MockFile>,
}

impl MockFileSystem {
    pub fn new() -> Self {
        Self { files: HashMap::new() }
    }
    
    pub fn add_file(&mut self, path: impl Into<PathBuf>, content: Vec<u8>) {
        self.files.insert(path.into(), MockFile { content });
    }
}

impl AsyncFileSystem for MockFileSystem {
    type File = MockFile;
    type Directory = MockDirectory;
    type Metadata = MockMetadata;
    
    async fn open_file(&self, path: &Path) -> Result<Self::File> {
        self.files.get(path)
            .cloned()
            .ok_or(SyncError::FileNotFound)
    }
    
    // ... other methods
}
```

## Security Test Cases

### Test Suite

1. **TOCTOU Tests**
   - Verify DirectoryFd usage
   - Verify no path resolution after open
   - Verify *at syscalls used

2. **Symlink Attack Tests**
   - O_NOFOLLOW verification
   - Symlink replacement detection
   - Path traversal prevention

3. **Race Condition Tests**
   - Concurrent file modifications
   - Directory changes during traversal
   - Hardlink race conditions

4. **Permission Tests**
   - Proper error handling for permission denied
   - Elevation requirements for ownership changes
   - ACL preservation

## Performance Benchmarks

### Baseline Comparisons

```rust
// Benchmark: Current implementation vs trait-based
criterion_group!(
    benches,
    bench_copy_file_baseline,
    bench_copy_file_trait_based,
    bench_walk_dir_baseline,
    bench_walk_dir_trait_based,
);

criterion_main!(benches);
```

**Acceptance Criteria**:
- Trait-based <= 5% overhead
- Syscall count equivalent
- I/O operations equivalent

## Integration Test Examples

### Full Copy Operation

```rust
#[compio::test]
async fn test_copy_directory_tree() {
    let src = TempDir::new()?;
    let dst = TempDir::new()?;
    
    // Create complex tree
    create_nested_structure(&src, 3 /* levels */, 10 /* files per level */).await?;
    
    // Copy using LocalFileSystem
    let fs = LocalFileSystem;
    let ops = GenericFileOperations::new(fs, 64 * 1024);
    let stats = ops.copy_directory(src.path(), dst.path()).await?;
    
    // Verify all files copied
    assert_eq!(stats.files_copied, 30); // 3 levels * 10 files
    
    // Verify metadata preserved
    verify_tree_metadata_matches(src.path(), dst.path())?;
}
```

### Hardlink Handling

```rust
#[compio::test]
async fn test_hardlink_detection() {
    let src = TempDir::new()?;
    let dst = TempDir::new()?;
    
    // Create file with hardlinks
    let file1 = src.path().join("file1.txt");
    let file2 = src.path().join("file2.txt");
    std::fs::write(&file1, b"content")?;
    std::fs::hard_link(&file1, &file2)?;
    
    // Copy
    let fs = LocalFileSystem;
    fs.copy_directory(src.path(), dst.path()).await?;
    
    // Verify destination has hardlink
    let dst1 = dst.path().join("file1.txt");
    let dst2 = dst.path().join("file2.txt");
    
    let meta1 = std::fs::metadata(&dst1)?;
    let meta2 = std::fs::metadata(&dst2)?;
    
    assert_eq!(meta1.ino(), meta2.ino()); // Same inode
    assert_eq!(meta1.nlink(), 2); // Two links
}
```

## Test Coverage Goals

- **Unit tests**: 100% of provided methods
- **Integration tests**: All major code paths
- **Security tests**: All attack vectors covered
- **Performance tests**: Baseline comparisons
- **Regression tests**: Existing functionality preserved

## Summary

Testing strategy provides:
- ✅ Confidence in correctness
- ✅ Security validation
- ✅ Performance verification
- ✅ Regression prevention
- ✅ Easy debugging when issues arise

Each PR includes appropriate tests at all levels.

