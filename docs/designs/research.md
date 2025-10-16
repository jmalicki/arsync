# io_uring Bulk Copying Utility Research

## Executive Summary

This document outlines the research findings for developing a highly efficient bulk copying utility in Rust using io_uring for asynchronous I/O operations, similar to rsync but optimized for single-machine operations with parallelism and metadata preservation.

## 1. io_uring Overview and Capabilities

### Core io_uring Features
- **Introduction**: Linux kernel interface introduced in version 5.1 (2019)
- **Purpose**: Provides efficient asynchronous I/O operations using ring buffers
- **Performance**: Reduces system call overhead by batching operations
- **Architecture**: Uses submission and completion queues for asynchronous operation management

### High-Level Primitives Support

#### copy_file_range Support
- **Kernel Version**: Available since kernel 5.3, with io_uring support since 5.6
- **Functionality**: Enables in-kernel file copying without transferring data to user space
- **Benefits**: Significantly reduces context switches and memory copies
- **Performance**: Can achieve near-zero-copy file operations for same-filesystem copies
- **Reference**: [Linux man pages - copy_file_range](https://man7.org/linux/man-pages/man2/copy_file_range.2.html)
- **⚠️ CRITICAL LIMITATION**: While the kernel supports copy_file_range in io_uring, **no Rust io_uring library currently exposes this functionality**
- **Impact**: Cannot achieve optimal zero-copy performance for same-filesystem operations
- **Workaround**: Must use compio read/write operations for all file copying
- **Priority**: **LOW** - This is a nice-to-have optimization, not a blocking issue

#### sendfile Support
- **Availability**: Supported in io_uring for efficient file-to-socket transfers
- **Use Case**: Primarily for network operations, but can be adapted for local file copying
- **Limitation**: Not directly applicable for local file-to-file operations

#### Zero-Copy Operations
- **splice() and tee()**: Available through io_uring for zero-copy operations
- **Use Case**: Efficient for piping data between file descriptors
- **Performance**: Eliminates user-space data copying entirely

### Metadata Operations
- **Extended Attributes (xattr)**: io_uring supports asynchronous xattr operations
- **File Permissions**: Standard stat/chmod operations are supported
- **ACLs**: Can be handled through xattr operations for POSIX ACLs
- **Ownership**: Supported through chown operations

## 2. Rust Libraries for io_uring Integration

### Core io_uring Libraries

#### rio
- **Repository**: [github.com/spacejam/rio](https://github.com/spacejam/rio)
- **Type**: Pure Rust implementation
- **Features**: 
  - Thread and async-friendly
  - Misuse-resistant design
  - Automatic submission queue management
  - Zero-copy system call support
- **Advantages**: No external C dependencies, leverages Rust's type system for safety

#### tokio-uring
- **Repository**: [github.com/tokio-rs/io-uring](https://github.com/tokio-rs/io-uring)
- **Type**: Tokio runtime integration
- **Features**:
  - Seamless integration with existing Tokio ecosystem
  - Async/await support
  - File I/O operations
- **Reference**: [Tokio-uring exploration article](https://developerlife.com/2024/05/25/tokio-uring-exploration-rust/)

#### uring-fs
- **Repository**: [lib.rs/crates/uring-fs](https://lib.rs/crates/uring-fs)
- **Type**: File system operations library
- **Features**:
  - Truly asynchronous file operations
  - Compatible with any async runtime
  - Supports open, stat, read, write operations
- **Advantages**: No thread pool required, pure async implementation

#### compio
- **Repository**: [github.com/compio-rs/compio](https://github.com/compio-rs/compio)
- **Last Maintained**: Active development as of 2024 (version 0.16.0 current)
- **Type**: Completion-based async runtime with io_uring support
- **Features**:
  - Thread-per-core architecture
  - Completion-based I/O (io_uring, IOCP, polling)
  - Async filesystem operations via `compio::fs`
  - Managed buffer pools with `IoBuf`/`IoBufMut` traits
  - Positional I/O operations (`AsyncReadAt`, `AsyncWriteAt`)
- **Current Status**: 
  - Version 0.16.0 available (October 2024)
  - Documentation builds failing for compio-fs 0.9.0 (use source code)
  - Active development with regular releases
- **Architecture**: Modular design with separate crates:
  - `compio-runtime`: Core runtime (0.9.1)
  - `compio-fs`: Filesystem operations (0.9.0)
  - `compio-io`: I/O traits and utilities (0.8.0)
  - `compio-driver`: Low-level drivers (0.9.0)
- **Advantages**: 
  - True async I/O without thread pools
  - Managed buffer system for zero-copy operations
  - Extensible architecture for custom operations
  - Performance-focused design

### High-Performance Runtimes

#### Glommio
- **Repository**: [docs.rs/glommio](https://docs.rs/glommio)
- **Type**: Thread-per-core async framework
- **Features**:
  - Safe Rust interface for io_uring
  - Thread-local I/O operations
  - CPU pinning support
  - Timers, file I/O, and networking abstractions
- **Use Case**: High-performance applications requiring maximum efficiency

#### Monoio
- **Repository**: [github.com/bytedance/monoio](https://github.com/bytedance/monoio)
- **Type**: Thread-per-core runtime
- **Features**:
  - Pure async I/O interface
  - Compatible with Tokio interface
  - High-performance networking solutions
  - Driver detection and switching
- **Reference**: [Monoio introduction](https://www.cloudwego.io/blog/2023/04/17/introducing-monoio-a-high-performance-rust-runtime-based-on-io-uring/)

#### libuio
- **Repository**: [docs.rs/libuio](https://docs.rs/libuio)
- **Type**: Async framework for networking
- **Features**:
  - Fully featured async framework
  - TCP listeners and streams
  - File operation modules

## 3. Implementation Considerations

### File Metadata Handling

#### Ownership and Permissions
- **Standard Operations**: Use stat, chown, chmod operations through io_uring
- **Preservation**: Maintain original file ownership and permissions during copy
- **Error Handling**: Graceful fallback for permission errors

#### Extended Attributes and ACLs
- **POSIX ACLs**: Handle through xattr operations
- **SELinux Contexts**: Preserve security contexts via extended attributes
- **Custom Attributes**: Support application-specific extended attributes

### Parallelism Strategies

#### Thread-Per-Core Model
- **Advantages**: Reduces context switching, improves cache efficiency
- **Implementation**: Use Glommio or Monoio for thread-per-core architecture
- **CPU Pinning**: Pin threads to specific CPU cores for optimal performance

#### Async Task Management
- **Concurrency**: Use Rust's async/await for concurrent file operations
- **Batching**: Submit multiple io_uring operations in batches
- **Backpressure**: Implement proper flow control for large file operations

### Performance Optimization

#### Buffer Management
- **Zero-Copy**: Leverage copy_file_range for same-filesystem operations
- **Buffer Pooling**: Reuse buffers to reduce allocation overhead
- **Size Tuning**: Optimize buffer sizes based on filesystem characteristics

#### File Access Pattern Optimization
- **posix_fadvise**: System call for providing access pattern hints to the kernel
- **POSIX_FADV_SEQUENTIAL**: Indicates sequential access pattern for large files
- **POSIX_FADV_DONTNEED**: Hints that data won't be needed again soon
- **Benefits**: Better kernel caching decisions, reduced memory pressure
- **Implementation**: Use `libc::posix_fadvise` or `nix::fcntl::posix_fadvise`
- **Use Case**: Large file copies that are read once and don't need caching

#### Operation Batching
- **Submission Queues**: Batch multiple operations before submission
- **Completion Handling**: Efficiently process completion events
- **Error Recovery**: Handle partial failures gracefully

## 4. Technical Requirements

### Kernel Version Requirements
- **Minimum**: Linux kernel 5.1 (basic io_uring support)
- **Recommended**: Linux kernel 5.6+ (copy_file_range support in io_uring)
- **Optimal**: Linux kernel 5.8+ (full feature set)

### Dependencies
- **Primary**: Rust async runtime (Tokio, Glommio, or Monoio)
- **io_uring**: rio, tokio-uring, or uring-fs
- **File Operations**: Standard library + xattr support
- **Parallelism**: Rayon or native async concurrency

### Performance Targets
- **Throughput**: Aim for near-line-speed copying on fast storage
- **Latency**: Minimize per-operation overhead
- **Scalability**: Linear scaling with available CPU cores
- **Memory**: Efficient memory usage with minimal copying

## 5. Recommended Architecture

### Core Components
1. **File Scanner**: Async directory traversal with metadata collection
2. **Copy Engine**: io_uring-based file copying with parallelism
3. **Metadata Manager**: Preservation of ownership, permissions, ACLs
4. **Progress Tracker**: Real-time progress reporting
5. **Error Handler**: Comprehensive error recovery and reporting

### Technology Stack
- **Runtime**: Glommio for thread-per-core performance
- **io_uring**: rio for pure Rust implementation
- **Async**: Native async/await with proper task scheduling
- **Parallelism**: CPU-aware task distribution

## 6. Complete io_uring Operations Inventory

### Required Operations for rsync-like Functionality

#### File Copying Operations (Priority: HIGH)
1. **copy_file_range** - In-kernel file copying (kernel 5.6+, io_uring support 5.6+)
   - **Use Case**: Primary file copying mechanism for same-filesystem operations
   - **Performance**: Near-zero-copy, minimal context switches
   - **Status**: ❌ **NOT ACHIEVABLE** - No Rust io_uring library exposes this functionality
   - **Impact**: Must fall back to read/write operations for all file copying

2. **splice** - Zero-copy data transfer between file descriptors
   - **Use Case**: Efficient copying when copy_file_range unavailable
   - **Performance**: Zero-copy operation
   - **Status**: ✅ Available in io_uring

3. **sendfile** - File-to-socket transfers
   - **Use Case**: Network operations, less relevant for local copying
   - **Performance**: Zero-copy for compatible operations
   - **Status**: ✅ Available in io_uring

4. **read/write** - Traditional file I/O operations
   - **Use Case**: Fallback for unsupported operations or cross-filesystem copies
   - **Performance**: Standard user-space copying
   - **Status**: ✅ Core io_uring operations

#### Directory Operations (Priority: HIGH)
5. **getdents64** - Directory entry reading
   - **Use Case**: Async directory traversal
   - **Performance**: Non-blocking directory listing
   - **Status**: ⚠️ Limited support in current Rust libraries

6. **openat/close** - File/directory opening/closing
   - **Use Case**: File descriptor management
   - **Performance**: Async file operations
   - **Status**: ✅ Widely supported

7. **mkdirat** - Directory creation
   - **Use Case**: Creating target directory structure
   - **Performance**: Async directory creation
   - **Status**: ✅ Available in io_uring

#### Metadata Operations (Priority: HIGH)
8. **statx** - Extended file statistics
   - **Use Case**: Comprehensive metadata retrieval
   - **Performance**: Async metadata collection
   - **Status**: ✅ Supported in io_uring

9. **fchmod** - File permission modification
   - **Use Case**: Preserving file permissions
   - **Performance**: Async permission updates
   - **Status**: ✅ Available in io_uring

10. **fchown** - File ownership modification
    - **Use Case**: Preserving file ownership
    - **Performance**: Async ownership updates
    - **Status**: ✅ Available in io_uring

11. **setxattr/getxattr** - Extended attributes management
    - **Use Case**: ACL preservation, SELinux contexts
    - **Performance**: Async xattr operations
    - **Status**: ✅ Available in io_uring

12. **listxattr** - Extended attributes listing
    - **Use Case**: Discovering all extended attributes
    - **Performance**: Async xattr enumeration
    - **Status**: ✅ Available in io_uring

#### Utility Operations (Priority: MEDIUM)
13. **unlinkat** - File/directory removal
    - **Use Case**: Cleanup operations, overwrite handling
    - **Performance**: Async file removal
    - **Status**: ✅ Available in io_uring

14. **renameat2** - File/directory renaming
    - **Use Case**: Atomic file moves, temporary file handling
    - **Performance**: Async rename operations
    - **Status**: ✅ Available in io_uring

15. **sync_file_range** - File synchronization
    - **Use Case**: Ensuring data integrity, explicit flushing
    - **Performance**: Async sync operations
    - **Status**: ✅ Available in io_uring

## 7. Library Support Analysis

### rio Library Support Assessment
- **Repository**: [github.com/spacejam/rio](https://github.com/spacejam/rio)
- **copy_file_range**: ❌ Not directly supported (manual implementation required)
- **splice/sendfile**: ❌ Not directly supported
- **getdents64**: ❌ Not supported (major limitation)
- **statx**: ✅ Supported
- **fchmod/fchown**: ✅ Supported
- **xattr operations**: ❌ Not supported
- **openat/close**: ✅ Supported
- **read/write**: ✅ Core functionality
- **Overall Assessment**: ⚠️ Limited - requires significant manual workarounds

### tokio-uring Library Support Assessment
- **Repository**: [github.com/tokio-rs/io-uring](https://github.com/tokio-rs/io-uring)
- **copy_file_range**: ❌ Not directly supported
- **splice/sendfile**: ❌ Not directly supported
- **getdents64**: ❌ Not supported
- **statx**: ⚠️ Limited support
- **fchmod/fchown**: ❌ Not supported
- **xattr operations**: ❌ Not supported
- **openat/close**: ✅ Supported
- **read/write**: ✅ Core functionality
- **Overall Assessment**: ⚠️ Very Limited - primarily file I/O focused

### Glommio Library Support Assessment
- **Repository**: [docs.rs/glommio](https://docs.rs/glommio)
- **copy_file_range**: ❌ Not directly supported
- **splice/sendfile**: ❌ Not directly supported
- **getdents64**: ❌ Not supported
- **statx**: ✅ Supported
- **fchmod/fchown**: ❌ Not supported
- **xattr operations**: ❌ Not supported
- **openat/close**: ✅ Supported
- **read/write**: ✅ Core functionality
- **Overall Assessment**: ⚠️ Limited - good for basic file I/O only

### Recommended Approach
Given the limited support in existing libraries, a **hybrid approach** is recommended:

1. **Use rio** as the base io_uring interface
2. **Implement custom wrappers** for unsupported operations:
   - Manual copy_file_range implementation
   - Custom xattr handling
   - Fallback directory traversal using std::fs
3. **Consider liburing bindings** for operations not supported by rio

## 8. Scalability and Queueing Architecture

### Queue Depth Limits and Recommendations

#### Submission Queue Depth Guidelines
- **Default**: 4096 operations (reasonable balance of memory usage and concurrency)
- **Minimum**: 1024 operations (for basic functionality)
- **Maximum**: 65536 operations (system resource dependent)
- **Command-line override**: `--queue-depth <N>` parameter

#### Completion Queue Depth Guidelines
- **Default**: Match submission queue depth
- **Memory impact**: ~8KB per 1024 entries (64-bit systems)
- **Recommendation**: Always match submission queue depth

#### Per-CPU Queue Architecture

##### Thread-Per-Core Model
```rust
// Conceptual architecture
struct PerCPUQueue {
    uring: Rio,                    // One io_uring instance per CPU
    submission_queue: Vec<Op>,     // CPU-local submission queue
    completion_handler: Handler,   // CPU-local completion processing
    cpu_id: usize,                 // CPU affinity
}
```

##### Benefits of Per-CPU Queues
- **Reduced contention**: No cross-CPU synchronization
- **Better cache locality**: CPU-local data structures
- **Linear scalability**: Performance scales with CPU count
- **Simplified error handling**: CPU-local error recovery

##### Implementation Strategy
1. **CPU Detection**: Use `num_cpus` crate for CPU count detection
2. **Thread Pinning**: Use `libc::sched_setaffinity` for CPU pinning
3. **Work Distribution**: Round-robin or work-stealing for load balancing
4. **Global Coordination**: Minimal shared state for progress tracking

### File-in-Flight Limits

#### Recommended Limits
- **Default**: 1024 files per CPU core
- **Command-line override**: `--max-files-in-flight <N>`
- **Memory calculation**: ~1KB per file metadata + buffer space
- **Total memory**: ~1MB per 1024 files per CPU

#### Queueing Strategy
```rust
// Async-friendly bounded channel for file operation coordination
let (file_sender, file_receiver) = tokio::sync::mpsc::channel(max_files_per_cpu);

// Per-CPU work queues with async coordination
let cpu_queues: Vec<PerCPUQueue> = (0..num_cpus)
    .map(|cpu_id| PerCPUQueue::new(cpu_id))
    .collect();
```

#### Async-Friendly Backpressure Handling
- **Non-blocking channels**: Use `tokio::sync::mpsc` instead of `crossbeam::channel`
- **Async coordination**: Channels that work seamlessly with async/await
- **Graceful degradation**: Reduce concurrency under memory pressure
- **Progress reporting**: Real-time queue depth monitoring

**Channel Blocking vs Async Compatibility**:
- ❌ **Avoid**: `crossbeam::channel` blocking operations in async contexts
- ✅ **Use**: `tokio::sync::mpsc` for async-friendly communication
- ✅ **Pattern**: `sender.send().await` and `receiver.recv().await` are async-friendly
- ⚠️ **Note**: Blocking channels can monopolize async threads and hurt performance

### Async Semaphore Implementation Strategy

#### Core Implementation Pattern (Based on Tokio)
```rust
// High-level semaphore structure
pub struct Semaphore {
    ll_sem: BatchSemaphore,  // Low-level implementation
}

// Low-level batch semaphore
pub struct BatchSemaphore {
    waiters: Mutex<Waitlist>,
    permits: AtomicUsize,
}

struct Waitlist {
    queue: LinkedList<Waiter>,
    closed: bool,
}

// Waiter node for the queue
struct Waiter {
    state: AtomicUsize,        // Number of permits needed
    waker: UnsafeCell<Option<Waker>>,  // Task waker
    pointers: linked_list::Pointers<Waiter>,
}

// Future for acquiring permits
pub struct Acquire<'a> {
    node: Waiter,
    semaphore: &'a BatchSemaphore,
    num_permits: usize,
    queued: bool,
}
```

#### Async Semaphore Implementation Details
- **Future Implementation**: The `Acquire` struct implements `Future<Output = Result<(), AcquireError>>`
- **Poll Function**: Uses `poll_acquire` method that:
  1. Tries to acquire permits atomically using `compare_exchange`
  2. If insufficient permits, adds waiter to queue and sets waker
  3. Returns `Poll::Pending` if queued, `Poll::Ready` if acquired
- **Waker Management**: Each waiter stores a `Waker` that gets called when permits become available
- **Fairness**: FIFO queue ensures fair permit distribution
- **Atomic Operations**: Uses atomic counters for permit tracking without locks

#### Key Implementation Patterns
```rust
impl Future for Acquire<'_> {
    type Output = Result<(), AcquireError>;
    
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let (node, semaphore, needed, queued) = self.project();
        
        match semaphore.poll_acquire(cx, needed, node, *queued) {
            Poll::Pending => {
                *queued = true;
                Poll::Pending
            }
            Poll::Ready(r) => {
                *queued = false;
                Poll::Ready(r)
            }
        }
    }
}
```

#### Integration with Compio
- **Subcrate Structure**: Create `compio-semaphore` crate
- **Runtime Integration**: Use compio's task system and waker infrastructure
- **Buffer Pool Coordination**: Semaphore can limit concurrent buffer allocations
- **Queue Depth Management**: Semaphore controls io_uring submission queue depth

## 9. Critical Questions and Implementation Challenges

### Directory Traversal Limitations
**Question**: How do we handle directory traversal when getdents64 is not supported in current Rust io_uring libraries?

**Current Status**: 
- ⚠️ **Major Gap**: None of the major Rust io_uring libraries support getdents64
- **Impact**: Cannot perform truly async directory traversal
- **Workarounds**:
  1. **Hybrid approach**: Use std::fs for directory scanning, io_uring for file operations
  2. **Manual implementation**: Direct liburing bindings for getdents64
  3. **Batch processing**: Pre-scan directories synchronously, then process files async

**Recommendation**: Implement hybrid approach initially, with plans for manual getdents64 support

### Direct liburing Bindings Feasibility
**Question**: Is creating direct liburing bindings feasible and how can they integrate with existing libraries?

**Feasibility Assessment**: ✅ **Highly Feasible**
- **Existing Examples**: `rio` and `tokio-uring` demonstrate successful liburing integration
- **liburing Maturity**: Well-established C library with stable API
- **Rust Integration**: liburing's design is amenable to safe Rust abstractions

**Implementation Strategy**:
```rust
// Proposed subcrate: io-uring-extended
// Structure similar to rio but with extended operations

pub struct ExtendedRio {
    inner: rio::Rio,  // Base rio instance
    // Additional state for extended operations
}

impl ExtendedRio {
    // copy_file_range support
    pub async fn copy_file_range(
        &self,
        src_fd: i32,
        dst_fd: i32,
        src_offset: u64,
        dst_offset: u64,
        len: u64,
    ) -> Result<usize> {
        // Manual syscall implementation using liburing
        unsafe {
            let mut src_off = src_offset;
            let mut dst_off = dst_offset;
            libc::copy_file_range(
                src_fd,
                &mut src_off,
                dst_fd,
                &mut dst_off,
                len as usize,
                0,
            )
        }
    }

    // getdents64 support
    pub async fn readdir(&self, dir_fd: i32, buffer: &mut [u8]) -> Result<usize> {
        // Direct liburing getdents64 implementation
    }
}
```

**Subcrate Design Benefits**:
- **Modularity**: Can be released independently as `io-uring-extended`
- **Compatibility**: Works alongside existing rio/tokio-uring
- **Incremental Adoption**: Add operations as needed
- **Community Contribution**: Others can contribute missing operations

**Integration with rio Style**:
- **Safe Abstractions**: Encapsulate unsafe operations in safe APIs
- **Error Handling**: Consistent error types and handling
- **Async/Await**: Native async support with proper `.await` points
- **Documentation**: Comprehensive docs with examples

### copy_file_range Implementation Gap
**Question**: How do we implement copy_file_range when it's not supported by existing libraries?

**Current Status**:
- ❌ **Critical Gap**: No Rust library provides copy_file_range support
- **Impact**: Cannot achieve optimal zero-copy performance for same-filesystem operations
- **⚠️ UPDATED ASSESSMENT**: While the kernel supports copy_file_range in io_uring, **no Rust io_uring library currently exposes this functionality**
- **Workarounds**:
  1. ~~**Manual syscall**: Direct system call implementation using nix or libc~~ **NOT FEASIBLE** - io_uring integration required
  2. **Fallback to read/write**: Use standard io_uring read/write operations (ONLY VIABLE OPTION)
  3. ~~**splice operations**: Use splice as alternative zero-copy mechanism~~ **NOT APPLICABLE** - splice not suitable for file-to-file copying

**Implementation Strategy**:
```rust
// ❌ NOT ACHIEVABLE - No Rust io_uring library exposes copy_file_range
// Must use compio read/write operations for all file copying

// ✅ ACTUAL IMPLEMENTATION - Use compio read/write
async fn copy_file_with_compio(
    src: &Path,
    dst: &Path,
) -> Result<()> {
    // Use compio::fs::File with read_at/write_at operations
    // This is the only viable approach with current Rust io_uring libraries
}
```

### Extended Attributes (xattr) Support
**Question**: How do we handle ACLs and extended attributes when xattr operations aren't supported?

**Current Status**:
- ❌ **Major Gap**: No Rust io_uring library supports xattr operations
- **Impact**: Cannot preserve ACLs, SELinux contexts, or custom attributes

**io_uring xattr Support Investigation**:
- **Kernel Support**: io_uring does support xattr operations since kernel 5.6
- **Available Operations**: `IORING_OP_SETXATTR`, `IORING_OP_GETXATTR`, `IORING_OP_LISTXATTR`
- **liburing Support**: ✅ **Available** - liburing provides xattr wrappers
- **Rust Integration**: ❌ **Missing** - No Rust library exposes these operations

**Direct liburing xattr Implementation**:
```rust
// Proposed xattr support in io-uring-extended crate
impl ExtendedRio {
    // Get extended attribute
    pub async fn getxattr(
        &self,
        path: &Path,
        name: &str,
        buffer: &mut [u8],
    ) -> Result<usize> {
        let path_c = CString::new(path.as_os_str().as_bytes())?;
        let name_c = CString::new(name)?;
        
        // Submit io_uring xattr operation
        let submission = io_uring_sqe {
            opcode: IORING_OP_GETXATTR as u8,
            flags: 0,
            ioprio: 0,
            fd: -1, // Use path instead of fd
            off: 0,
            addr: path_c.as_ptr() as u64,
            len: buffer.len() as u32,
            opcode_flags: 0,
            user_data: self.next_user_data(),
            buf_index: 0,
            personality: 0,
            splice_fd_in: 0,
            __pad2: [0; 2],
        };
        
        self.submit_and_wait(submission).await
    }

    // Set extended attribute
    pub async fn setxattr(
        &self,
        path: &Path,
        name: &str,
        value: &[u8],
        flags: i32,
    ) -> Result<()> {
        // Similar implementation for setxattr
    }

    // List extended attributes
    pub async fn listxattr(
        &self,
        path: &Path,
        buffer: &mut [u8],
    ) -> Result<usize> {
        // Implementation for listxattr
    }
}
```

**Implementation Strategy**:
1. **Direct liburing Integration**: Use liburing's xattr wrappers
2. **Safe Rust Abstractions**: Encapsulate unsafe operations in safe APIs
3. **Async Coordination**: Integrate with io_uring completion system
4. **Error Handling**: Proper error propagation and handling

**Benefits of Direct xattr Support**:
- ✅ **True Async**: Native io_uring async xattr operations
- ✅ **Performance**: No thread pool overhead for xattr operations
- ✅ **Consistency**: All operations use same io_uring infrastructure
- ✅ **Completeness**: Full metadata preservation capabilities

### Queue Depth and Memory Management
**Question**: What are the practical limits for queue depth and how do we prevent memory exhaustion?

**Critical Considerations**:
- **Memory per operation**: ~1KB metadata + buffer space per file
- **Queue overflow**: io_uring submission queue can overflow, causing operation loss
- **Backpressure**: Need mechanisms to prevent overwhelming the system

**Recommended Limits**:
- **Conservative**: 1024 operations per CPU core
- **Aggressive**: 4096 operations per CPU core
- **Maximum**: 65536 operations (system-dependent)

### Thread Safety and Per-CPU Architecture
**Question**: How do we implement thread-safe per-CPU queues without cross-CPU contention?

**Architecture Decisions**:
1. **One io_uring instance per CPU**: Eliminates cross-CPU synchronization
2. **CPU-local work queues**: Each CPU manages its own file queue
3. **Global coordination**: Minimal shared state for progress tracking
4. **Work stealing**: Optional load balancing between CPUs

### Error Handling and Recovery
**Question**: How do we handle partial failures and ensure data integrity?

**Challenges**:
- **Partial file copies**: What happens if copy_file_range fails mid-operation?
- **Metadata failures**: How to handle permission/ownership errors?
- **Queue overflow**: Recovery from submission queue overflow
- **Cross-filesystem operations**: Fallback strategies for different filesystems

### Performance Optimization Questions
**Question**: What are the optimal buffer sizes and batching strategies?

**Research Needed**:
- **Buffer size tuning**: Optimal sizes for different storage types (SSD vs HDD)
- **Batch size optimization**: How many operations to submit per batch
- **Memory mapping**: Should we use mmap for large files?
- **Direct I/O**: When to use O_DIRECT for maximum performance

### Command-Line Interface Design
**Question**: What configuration options should be exposed to users?

**Proposed Options**:
```bash
arsync [OPTIONS] <SOURCE> <DESTINATION>

Options:
  --queue-depth <N>           # Submission queue depth (default: 4096)
  --max-files-in-flight <N>   # Max concurrent files per CPU (default: 1024)
  --cpu-count <N>             # Number of CPUs to use (default: auto-detect)
  --buffer-size <N>           # Buffer size in KB (default: auto-detect)
  --copy-method <METHOD>      # copy_file_range, splice, read/write
  --preserve-xattr            # Preserve extended attributes
  --preserve-acl              # Preserve POSIX ACLs
  --dry-run                   # Show what would be copied
  --progress                  # Show progress information
  --verbose                   # Verbose output
```

## 10. Compio Extension Strategies

### Understanding compio::fs Architecture

#### Current compio::fs Structure
- **File Operations**: `compio::fs::File` with positional I/O (`AsyncReadAt`, `AsyncWriteAt`)
- **OpenOptions**: Configurable file opening with various flags
- **Metadata**: `compio::fs::Metadata` for file information
- **Buffer Management**: Uses `IoBuf`/`IoBufMut` traits for managed buffers
- **Runtime Integration**: Built on `compio-runtime` with completion-based I/O

#### Extension Opportunities
- **Missing Operations**: `copy_file_range`, `symlink`, `readdir`, `xattr` operations
- **Advanced Features**: `fadvise`, `fallocate`, `sync_file_range`
- **Custom Operations**: io_uring operations not exposed by compio

### Extension Implementation Strategies

#### Strategy 1: Wrapper Extension
```rust
// Extend compio::fs::File with additional methods
impl File {
    pub async fn copy_file_range(
        &self,
        dst: &File,
        src_offset: u64,
        dst_offset: u64,
        len: u64,
    ) -> Result<usize> {
        // Direct syscall implementation using compio's runtime
        unsafe {
            let result = libc::copy_file_range(
                self.as_raw_fd(),
                &mut src_offset as *mut _,
                dst.as_raw_fd(),
                &mut dst_offset as *mut _,
                len as usize,
                0,
            );
            // Handle result and integrate with compio's error system
        }
    }
    
    pub async fn fadvise(&self, advice: i32, offset: u64, len: u64) -> Result<()> {
        // Implement posix_fadvise using compio runtime
    }
}
```

#### Strategy 2: Subcrate Extension
```rust
// compio-fs-extended crate
pub mod extended {
    use compio::fs::File;
    
    pub struct ExtendedFile(File);
    
    impl ExtendedFile {
        pub fn new(file: File) -> Self {
            Self(file)
        }
        
        pub async fn copy_file_range(&self, /* ... */) -> Result<usize> {
            // Implementation using compio's submission system
        }
    }
}
```

#### Strategy 3: Custom Operations via compio-driver
```rust
// Direct integration with compio-driver for custom io_uring operations
use compio_driver::op::CustomOp;

pub struct CopyFileRangeOp {
    src_fd: i32,
    dst_fd: i32,
    src_offset: u64,
    dst_offset: u64,
    len: u64,
}

impl CustomOp for CopyFileRangeOp {
    // Implement custom io_uring operation
}
```

### Recommended Approach for arsync

#### Phase 1: Wrapper Extensions
- Extend existing `compio::fs::File` with copy_file_range
- Add fadvise support for file access pattern optimization
- Implement basic symlink operations

#### Phase 2: Subcrate Development
- Create `compio-fs-extended` subcrate
- Add comprehensive xattr support
- Implement directory traversal with getdents64
- Add advanced file operations (fallocate, sync_file_range)

#### Phase 3: Custom Runtime Integration
- Direct integration with compio-driver for specialized operations
- Custom io_uring operation types for optimal performance
- Integration with compio's managed buffer system

## 11. Research Conclusions and Recommendations

### Key Findings Summary

Based on comprehensive research into io_uring capabilities and Rust library support, the following conclusions have been reached:

#### io_uring Capabilities
- **copy_file_range**: ✅ Available in io_uring since kernel 5.6, providing optimal zero-copy performance
- **Extended Attributes**: ✅ Full xattr support available through io_uring operations
- **Directory Traversal**: ⚠️ Limited support in current Rust libraries, requiring custom implementation
- **Metadata Operations**: ✅ Comprehensive support for ownership, permissions, and timestamps

#### Library Support Analysis
- **rio**: ⚠️ Limited - requires significant manual workarounds for missing operations
- **tokio-uring**: ⚠️ Very Limited - primarily file I/O focused
- **Glommio**: ⚠️ Limited - good for basic file I/O only
- **compio**: ✅ **Recommended** - Modern, active development, extensible architecture
- **Recommended Approach**: Use compio as base with custom extensions

#### Critical Implementation Challenges
1. **Directory Traversal**: Limited support in compio, requires custom implementation
2. **copy_file_range**: No library support, requiring manual syscall implementation
3. **Extended Attributes**: No library support, requiring direct integration
4. **Queue Management**: Need careful design for per-CPU architecture and backpressure
5. **Semaphore Coordination**: Need custom async semaphore for queue depth management

### Recommended Architecture

Based on this analysis, the optimal approach is:

1. **Base Library**: Use `compio` as the foundation for async I/O operations
2. **Extended Operations**: Create `compio-fs-extended` subcrate with:
   - `copy_file_range` support using direct syscalls
   - `getdents64` directory traversal
   - Complete xattr operations suite
   - `fadvise` for file access pattern optimization
3. **Async Coordination**: Use custom async semaphore for queue management
4. **Per-CPU Architecture**: One compio runtime instance per CPU core
5. **Modular Design**: Release extended operations as standalone crate

#### Updated Technology Stack
- **Runtime**: compio for completion-based async I/O
- **Filesystem**: compio::fs with custom extensions
- **Concurrency**: Custom async semaphore for queue depth control
- **Performance**: fadvise for large file optimization
- **Architecture**: Thread-per-core with compio runtime

This approach provides the best balance of performance, safety, and maintainability while leveraging compio's modern architecture and extensibility.

### Next Steps

With research complete, the project is ready to proceed to implementation:

1. **Review Implementation Plan**: See `IMPLEMENTATION_PLAN.md` for detailed phases and deliverables
2. **Review Testing Strategy**: See `TESTING_STRATEGY.md` for comprehensive testing approach
3. **Begin Phase 1**: Start with project setup and basic io_uring integration

## References

### Official Documentation
- [Linux Kernel io_uring Header File](https://github.com/torvalds/linux/blob/master/include/uapi/linux/io_uring.h)
- [io_uring Official Repository](https://github.com/axboe/liburing)
- [Linux man pages - copy_file_range](https://man7.org/linux/man-pages/man2/copy_file_range.2.html)
- [Linux man pages - io_uring](https://man7.org/linux/man-pages/man7/io_uring.7.html)

### Rust Libraries
- [compio - Completion-based async runtime](https://github.com/compio-rs/compio)
- [rio - Pure Rust io_uring](https://github.com/spacejam/rio)
- [tokio-uring - Tokio integration](https://github.com/tokio-rs/io-uring)
- [Glommio - Thread-per-core framework](https://docs.rs/glommio)
- [Monoio - High-performance runtime](https://github.com/bytedance/monoio)
- [uring-fs - Async file operations](https://lib.rs/crates/uring-fs)

### Research and Articles
- [Tokio-uring exploration with Rust](https://developerlife.com/2024/05/25/tokio-uring-exploration-rust/)
- [Monoio introduction and architecture](https://www.cloudwego.io/blog/2023/04/17/introducing-monoio-a-high-performance-rust-runtime-based-on-io-uring/)

### Performance Resources
- [io_uring Performance Analysis](https://kernel.dk/io_uring.pdf)
- [Zero-Copy Operations in Linux](https://lwn.net/Articles/667550/)
- [copy_file_range Performance Benefits](https://lwn.net/Articles/720675/)
