## Overview

This PR implements the initial phase of trait-based filesystem integration, enabling unified operations between local filesystem and remote protocol backends.

## What's Implemented

### Core Traits
- **`AsyncFileSystem`** - Main trait defining filesystem operations
- **`AsyncFile`** - File operations (read, write, copy_file_range, etc.)
- **`AsyncDirectory`** - Directory operations (read_dir, create_file, etc.)
- **`AsyncMetadata`** - Metadata operations (size, permissions, timestamps, etc.)
- **`FileOperations`** - High-level operations that work with any filesystem

### Backend Implementations
- **`LocalFileSystem`** - Uses compio-fs-extended for high-performance local operations
  - Full io_uring integration
  - Supports copy_file_range, hardlinks, symlinks
  - Complete metadata preservation
- **`ProtocolFileSystem`** - Uses existing Transport trait for remote operations
  - Protocol-agnostic design
  - Extensible for new transport types
  - Placeholder implementation (ready for completion)

### Key Benefits
- **Code Reuse** - Single implementation works with both local and remote filesystems
- **Type Safety** - Compile-time guarantees that operations are supported
- **Performance** - Direct use of compio's io_uring backend
- **Extensibility** - Easy to add new filesystem types or transport protocols
- **Testing** - Easy to mock filesystem operations for testing

## Files Added
- `src/traits/` - Core trait definitions
- `src/backends/` - Backend implementations
- `examples/trait_integration_example.rs` - Usage example
- `tests/trait_integration_test.rs` - Integration tests
- `docs/TRAIT_INTEGRATION.md` - Comprehensive documentation
- `docs/design/protocol_compio_integration.md` - Design document

## Usage Example
```rust
use arsync::traits::{AsyncFileSystem, FileOperations, GenericFileOperations};
use arsync::backends::{LocalFileSystem, ProtocolFileSystem};

// Local filesystem
let local_fs = LocalFileSystem::new();
let local_ops = GenericFileOperations::new(local_fs, 64 * 1024);
local_ops.copy_file(src, dst).await?;

// Remote filesystem via SSH (when implemented)
let transport = SshTransport::new("user@host").await?;
let remote_fs = ProtocolFileSystem::new(transport);
let remote_ops = GenericFileOperations::new(remote_fs, 64 * 1024);
remote_ops.copy_file(src, dst).await?;
```

## Next Phase
The next phase will focus on:
1. Updating existing code to use the new trait-based system
2. Completing the protocol backend implementation
3. Adding performance optimizations
4. Comprehensive integration testing

## Testing
- Unit tests for all traits and implementations
- Integration tests demonstrating end-to-end functionality
- Example code showing usage patterns

This PR establishes the foundation for unified filesystem operations and enables significant code reuse between local and remote backends.