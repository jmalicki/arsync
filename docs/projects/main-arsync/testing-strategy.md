# Testing Strategy

This document outlines the comprehensive testing strategy for arsync, ensuring reliability, performance, and data integrity.

## High-Level End-to-End Testing

### Data Integrity Verification
**Challenge**: How do we ensure files are copied without corruption and all metadata is preserved?

**Testing Approach**:
1. **Checksum Verification**: Use cryptographic hashes (SHA-256) to verify file integrity
2. **Metadata Comparison**: Compare ownership, permissions, timestamps, and extended attributes
3. **Directory Structure Verification**: Ensure complete directory tree replication
4. **Stress Testing**: Large files, many files, deep directory structures

**Implementation Strategy**:
```rust
// Test data generation
pub struct TestDataGenerator {
    // Create files with known checksums
    pub fn create_known_files(&self, dir: &Path) -> HashMap<PathBuf, String>;
    
    // Create files with extended attributes
    pub fn create_files_with_xattrs(&self, dir: &Path) -> Vec<PathBuf>;
    
    // Create stress test scenarios
    pub fn create_stress_test(&self, dir: &Path) -> TestScenario;
}

// Verification utilities
pub struct VerificationSuite {
    pub fn verify_file_integrity(&self, src: &Path, dst: &Path) -> Result<()>;
    pub fn verify_metadata(&self, src: &Path, dst: &Path) -> Result<()>;
    pub fn verify_xattrs(&self, src: &Path, dst: &Path) -> Result<()>;
    pub fn verify_directory_structure(&self, src: &Path, dst: &Path) -> Result<()>;
}
```

### Test Scenarios

#### Basic Functionality Tests
- **Single File Copy**: Small, medium, large files
- **Directory Copy**: Shallow and deep directory structures
- **Metadata Preservation**: Ownership, permissions, timestamps
- **Extended Attributes**: User attributes, system attributes, ACLs

#### Edge Cases and Error Conditions
- **Permission Errors**: Read-only files, protected directories
- **Disk Space**: Insufficient space scenarios
- **Concurrent Access**: Files being modified during copy
- **Symbolic Links**: Hard links, soft links, broken links
- **Special Files**: Devices, sockets, named pipes

#### Performance and Stress Tests
- **Large Files**: Files larger than available RAM
- **Many Small Files**: Thousands of small files
- **Mixed Workloads**: Various file sizes and types
- **Memory Pressure**: Limited available memory
- **CPU Contention**: Multiple concurrent operations

#### Cross-Filesystem Testing
- **Same Filesystem**: Optimal copy_file_range performance
- **Different Filesystems**: Fallback to read/write operations
- **Network Filesystems**: NFS, CIFS, etc.
- **Special Filesystems**: tmpfs, ramfs, etc.

## Automated Test Suite

### Unit Tests
```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_copy_file_range_same_filesystem() {
        // Test copy_file_range on same filesystem
    }
    
    #[test]
    fn test_copy_file_range_cross_filesystem() {
        // Test fallback behavior
    }
    
    #[test]
    fn test_xattr_preservation() {
        // Test extended attribute preservation
    }
}
```

### Integration Tests
```rust
#[test]
fn test_end_to_end_copy() {
    let temp_dir = create_test_directory();
    let src = temp_dir.path().join("source");
    let dst = temp_dir.path().join("destination");
    
    // Run the copy operation
    let result = sync_files(&args).await;
    assert!(result.is_ok());
    
    // Verify integrity
    let verification = VerificationSuite::new();
    verification.verify_file_integrity(&src, &dst).unwrap();
    verification.verify_metadata(&src, &dst).unwrap();
    verification.verify_xattrs(&src, &dst).unwrap();
}
```

### Property-Based Tests
```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_copy_any_file_size(size in 0..10_000_000usize) {
        // Test copying files of random sizes
        let data = vec![0u8; size];
        // ... test implementation
    }
    
    #[test]
    fn test_copy_with_random_xattrs(
        xattr_count in 0..10usize,
        xattr_size in 0..1000usize
    ) {
        // Test copying files with random extended attributes
        // ... test implementation
    }
}
```

### Benchmark Tests
```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn benchmark_copy_methods(c: &mut Criterion) {
    let mut group = c.benchmark_group("copy_methods");
    
    group.bench_function("copy_file_range", |b| {
        b.iter(|| copy_file_range(black_box(&src), black_box(&dst)))
    });
    
    group.bench_function("read_write", |b| {
        b.iter(|| copy_read_write(black_box(&src), black_box(&dst)))
    });
    
    group.finish();
}
```

## Continuous Integration Testing

### Test Matrix
- **Operating Systems**: Ubuntu 20.04+, CentOS 8+, Debian 11+
- **Kernel Versions**: 5.6+ (minimum), 5.8+ (recommended), latest
- **Architectures**: x86_64, aarch64
- **Filesystems**: ext4, xfs, btrfs, zfs
- **Memory Configurations**: 1GB, 4GB, 16GB+

### Automated Test Execution
```yaml
# .github/workflows/test-matrix.yml
strategy:
  matrix:
    os: [ubuntu-20.04, ubuntu-22.04, centos-8]
    kernel: [5.6, 5.8, 5.15]
    filesystem: [ext4, xfs, btrfs]
    memory: [1gb, 4gb, 16gb]
```

## Test Data Management

### Synthetic Test Data
- **Known Patterns**: Files with predictable content for integrity checking
- **Random Data**: Files with random content for stress testing
- **Real-world Data**: Sample datasets from common use cases
- **Edge Cases**: Empty files, very large files, files with unusual names

### Test Environment Setup
```rust
pub struct TestEnvironment {
    pub temp_dir: TempDir,
    pub source_dir: PathBuf,
    pub dest_dir: PathBuf,
    pub test_files: Vec<TestFile>,
}

impl TestEnvironment {
    pub fn new() -> Self {
        let temp_dir = TempDir::new().unwrap();
        let source_dir = temp_dir.path().join("source");
        let dest_dir = temp_dir.path().join("destination");
        
        fs::create_dir_all(&source_dir).unwrap();
        fs::create_dir_all(&dest_dir).unwrap();
        
        Self {
            temp_dir,
            source_dir,
            dest_dir,
            test_files: Vec::new(),
        }
    }
    
    pub fn create_test_scenario(&mut self, scenario: TestScenario) {
        match scenario {
            TestScenario::BasicFiles => self.create_basic_files(),
            TestScenario::LargeFiles => self.create_large_files(),
            TestScenario::ManyFiles => self.create_many_files(),
            TestScenario::WithXattrs => self.create_files_with_xattrs(),
        }
    }
}
```

## Performance Regression Testing

### Baseline Performance Metrics
- **Throughput**: MB/s for various file sizes
- **Latency**: Time per operation
- **Memory Usage**: Peak and average memory consumption
- **CPU Utilization**: Efficiency across different workloads

### Automated Performance Testing
```rust
pub struct PerformanceBenchmark {
    pub throughput: f64,      // MB/s
    pub latency: Duration,    // Average operation time
    pub memory_usage: usize,  // Peak memory in bytes
    pub cpu_efficiency: f64,  // Operations per CPU second
}

impl PerformanceBenchmark {
    pub fn run_benchmark(&self, test_data: &TestData) -> BenchmarkResults {
        // Run comprehensive performance tests
    }
    
    pub fn compare_with_baseline(&self, baseline: &BenchmarkResults) -> RegressionReport {
        // Compare against previous results
    }
}
```

## Error Injection Testing

### Simulated Failure Conditions
- **IO Errors**: Disk full, permission denied, network failures
- **Memory Pressure**: Limited available memory
- **Interruptions**: Process termination, signal handling
- **Concurrent Modifications**: Files changed during copy

### Chaos Engineering
```rust
pub struct ChaosEngine {
    pub fn inject_io_errors(&self, probability: f64);
    pub fn limit_memory(&self, max_memory: usize);
    pub fn simulate_disk_full(&self);
    pub fn random_delays(&self, max_delay: Duration);
}
```

## Test Categories

### 1. Unit Tests
Test individual functions in isolation with mocked dependencies.

**Coverage Requirements:**
- All public functions must have unit tests
- Error paths must be tested
- Edge cases must be covered

### 2. Integration Tests
Test component interactions and data flow between modules.

**Coverage Requirements:**
- File copying workflows
- Metadata preservation
- Error handling and recovery
- Progress reporting

### 3. End-to-End Tests
Test complete user workflows from command line to file system.

**Coverage Requirements:**
- Complete copy operations
- Directory tree replication
- Cross-filesystem scenarios
- Performance under load

### 4. Performance Tests
Benchmark critical paths and detect performance regressions.

**Coverage Requirements:**
- All copy methods
- Different file sizes
- Various concurrency levels
- Memory usage patterns

### 5. Property Tests
Test invariants with random data to find edge cases.

**Coverage Requirements:**
- File size variations
- Extended attribute combinations
- Directory structure variations
- Permission scenarios

## Test Execution Strategy

### Local Development
```bash
# Run all tests
cargo test

# Run with coverage
cargo tarpaulin --out Html

# Run benchmarks
cargo bench

# Run specific test categories
cargo test --test integration_tests
cargo test --test performance_tests
```

### CI/CD Pipeline
- **Pull Requests**: All test categories must pass
- **Main Branch**: Performance benchmarks must not regress
- **Releases**: Full test matrix execution required

### Test Environment Requirements
- **Linux Systems**: Kernel 5.6+ with io_uring support
- **Storage**: Fast SSD for performance tests
- **Memory**: Sufficient RAM for large file tests
- **CPU**: Multi-core system for parallelism tests

This comprehensive testing strategy ensures that arsync is reliable, performant, and handles all edge cases correctly while preserving data integrity and metadata.
