//! Example demonstrating the trait-based integration between protocol mod and compio
//!
//! This example shows how the new trait system allows for unified operations
//! between local filesystem and remote protocol backends.

use arsync::traits::{AsyncFileSystem, FileOperations};
use arsync::backends::{LocalFileSystem, ProtocolFileSystem};
use arsync::protocol::Transport;
use std::path::Path;

/// Example transport implementation for demonstration
struct ExampleTransport;

impl Transport for ExampleTransport {
    fn name(&self) -> &'static str {
        "example"
    }
}

// Implement AsyncRead and AsyncWrite for ExampleTransport
// This is a simplified implementation for demonstration
impl compio::io::AsyncRead for ExampleTransport {
    fn read<B: compio::io::IoBufMut>(&self, _buf: B) -> compio::io::BufResult<usize, B> {
        // Placeholder implementation
        compio::io::BufResult(Ok(0), _buf)
    }
}

impl compio::io::AsyncWrite for ExampleTransport {
    fn write<B: compio::io::IoBuf>(&self, _buf: B) -> compio::io::BufResult<usize, B> {
        // Placeholder implementation
        compio::io::BufResult(Ok(0), _buf)
    }

    fn flush(&self) -> compio::io::BufResult<(), compio::io::Empty> {
        // Placeholder implementation
        compio::io::BufResult(Ok(()), compio::io::Empty)
    }
}

#[compio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Trait-based Integration Example");
    println!("==============================");

    // Example 1: Local filesystem operations
    println!("\n1. Local Filesystem Operations:");
    let local_fs = LocalFileSystem::new();
    let local_ops = arsync::traits::GenericFileOperations::new(local_fs, 64 * 1024);
    
    println!("   Filesystem name: {}", local_ops.filesystem_name());
    println!("   Buffer size: {} bytes", local_ops.buffer_size());
    println!("   Supports copy_file_range: {}", local_ops.filesystem().supports_copy_file_range());
    println!("   Supports hardlinks: {}", local_ops.filesystem().supports_hardlinks());
    println!("   Supports symlinks: {}", local_ops.filesystem().supports_symlinks());

    // Example 2: Protocol filesystem operations
    println!("\n2. Protocol Filesystem Operations:");
    let transport = ExampleTransport;
    let protocol_fs = ProtocolFileSystem::new(transport);
    let protocol_ops = arsync::traits::GenericFileOperations::new(protocol_fs, 64 * 1024);
    
    println!("   Filesystem name: {}", protocol_ops.filesystem_name());
    println!("   Buffer size: {} bytes", protocol_ops.buffer_size());
    println!("   Supports copy_file_range: {}", protocol_ops.filesystem().supports_copy_file_range());
    println!("   Supports hardlinks: {}", protocol_ops.filesystem().supports_hardlinks());
    println!("   Supports symlinks: {}", protocol_ops.filesystem().supports_symlinks());

    // Example 3: Unified operations interface
    println!("\n3. Unified Operations Interface:");
    println!("   Both filesystem types implement the same FileOperations trait");
    println!("   This allows for code reuse and consistent APIs");
    
    // Example 4: Generic function that works with any filesystem
    println!("\n4. Generic Function Example:");
    demonstrate_unified_operations(&local_ops).await?;
    demonstrate_unified_operations(&protocol_ops).await?;

    println!("\nExample completed successfully!");
    Ok(())
}

/// Generic function that works with any filesystem implementing AsyncFileSystem
async fn demonstrate_unified_operations<FS: AsyncFileSystem>(
    operations: &arsync::traits::GenericFileOperations<FS>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("   Operating on filesystem: {}", operations.filesystem_name());
    
    // These operations would work the same way regardless of the underlying filesystem
    // (local or remote protocol)
    
    // Check if a path exists
    let test_path = Path::new("/tmp/test_file");
    let exists = operations.exists(test_path).await;
    println!("   Path {} exists: {}", test_path.display(), exists);
    
    // Get filesystem capabilities
    let fs = operations.filesystem();
    println!("   Capabilities:");
    println!("     - copy_file_range: {}", fs.supports_copy_file_range());
    println!("     - hardlinks: {}", fs.supports_hardlinks());
    println!("     - symlinks: {}", fs.supports_symlinks());
    
    Ok(())
}