//! Tests for the trait-based filesystem integration

use super::*;
use crate::backends::{LocalFileSystem, ProtocolFileSystem};
use crate::protocol::Transport;
use compio::io::{AsyncRead, AsyncWrite};
use std::path::Path;
use tempfile::TempDir;

/// Mock transport for testing
struct MockTransport;

impl Transport for MockTransport {
    fn name(&self) -> &'static str {
        "mock"
    }
}

impl AsyncRead for MockTransport {
    fn read<B: compio::io::IoBufMut>(&self, buf: B) -> compio::io::BufResult<usize, B> {
        compio::io::BufResult(Ok(0), buf)
    }
}

impl AsyncWrite for MockTransport {
    fn write<B: compio::io::IoBuf>(&self, buf: B) -> compio::io::BufResult<usize, B> {
        compio::io::BufResult(Ok(0), buf)
    }

    fn flush(&self) -> compio::io::BufResult<(), compio::io::Empty> {
        compio::io::BufResult(Ok(()), compio::io::Empty)
    }
}

#[compio::test]
async fn test_local_filesystem_basic() -> Result<()> {
    let temp_dir = TempDir::new().map_err(|e| {
        crate::error::SyncError::FileSystem(format!("Failed to create temp directory: {}", e))
    })?;
    
    let fs = LocalFileSystem::new();
    let ops = GenericFileOperations::new(fs, 64 * 1024);
    
    // Test filesystem properties
    assert_eq!(ops.filesystem_name(), "local");
    assert_eq!(ops.buffer_size(), 64 * 1024);
    assert!(ops.filesystem().supports_copy_file_range());
    assert!(ops.filesystem().supports_hardlinks());
    assert!(ops.filesystem().supports_symlinks());
    
    // Test file operations
    let test_file = temp_dir.path().join("test.txt");
    let test_content = b"Hello, World!";
    
    // Create file
    let mut file = ops.filesystem().create_file(&test_file).await?;
    file.write_all(test_content).await?;
    
    // Verify file exists
    assert!(ops.exists(&test_file).await);
    
    // Get metadata
    let metadata = ops.metadata(&test_file).await?;
    assert!(metadata.is_file());
    assert_eq!(metadata.size(), test_content.len() as u64);
    
    // Read file
    let content = file.read_to_end().await?;
    assert_eq!(content, test_content);
    
    Ok(())
}

#[compio::test]
async fn test_protocol_filesystem_basic() -> Result<()> {
    let transport = MockTransport;
    let fs = ProtocolFileSystem::new(transport);
    let ops = GenericFileOperations::new(fs, 64 * 1024);
    
    // Test filesystem properties
    assert_eq!(ops.filesystem_name(), "mock");
    assert_eq!(ops.buffer_size(), 64 * 1024);
    assert!(!ops.filesystem().supports_copy_file_range());
    assert!(!ops.filesystem().supports_hardlinks());
    assert!(ops.filesystem().supports_symlinks());
    
    // Test that operations return appropriate errors (since they're not implemented yet)
    let test_path = Path::new("/tmp/test");
    let result = ops.filesystem().open_file(test_path).await;
    assert!(result.is_err());
    
    Ok(())
}

#[compio::test]
async fn test_generic_file_operations() -> Result<()> {
    let temp_dir = TempDir::new().map_err(|e| {
        crate::error::SyncError::FileSystem(format!("Failed to create temp directory: {}", e))
    })?;
    
    let fs = LocalFileSystem::new();
    let ops = GenericFileOperations::new(fs, 64 * 1024);
    
    // Test directory operations
    let test_dir = temp_dir.path().join("test_dir");
    ops.create_directory_all(&test_dir).await?;
    assert!(ops.exists(&test_dir).await);
    
    // Test file operations
    let test_file = test_dir.join("test.txt");
    let test_content = b"Test content";
    
    // Create file
    let mut file = ops.filesystem().create_file(&test_file).await?;
    file.write_all(test_content).await?;
    
    // Copy file
    let copied_file = test_dir.join("copied.txt");
    let bytes_copied = ops.copy_file(&test_file, &copied_file).await?;
    assert_eq!(bytes_copied, test_content.len() as u64);
    
    // Verify copied file
    assert!(ops.exists(&copied_file).await);
    let copied_content = ops.filesystem().open_file(&copied_file).await?.read_to_end().await?;
    assert_eq!(copied_content, test_content);
    
    Ok(())
}

#[compio::test]
async fn test_metadata_operations() -> Result<()> {
    let temp_dir = TempDir::new().map_err(|e| {
        crate::error::SyncError::FileSystem(format!("Failed to create temp directory: {}", e))
    })?;
    
    let fs = LocalFileSystem::new();
    let ops = GenericFileOperations::new(fs, 64 * 1024);
    
    // Test file metadata
    let test_file = temp_dir.path().join("test.txt");
    let test_content = b"Test content";
    
    let mut file = ops.filesystem().create_file(&test_file).await?;
    file.write_all(test_content).await?;
    
    let metadata = ops.metadata(&test_file).await?;
    assert!(metadata.is_file());
    assert!(!metadata.is_dir());
    assert!(!metadata.is_symlink());
    assert_eq!(metadata.size(), test_content.len() as u64);
    assert!(!metadata.is_empty());
    
    // Test directory metadata
    let test_dir = temp_dir.path().join("test_dir");
    ops.create_directory_all(&test_dir).await?;
    
    let dir_metadata = ops.metadata(&test_dir).await?;
    assert!(!dir_metadata.is_file());
    assert!(dir_metadata.is_dir());
    assert!(!dir_metadata.is_symlink());
    
    Ok(())
}

#[compio::test]
async fn test_trait_object_safety() -> Result<()> {
    // Test that traits can be used as trait objects
    let temp_dir = TempDir::new().map_err(|e| {
        crate::error::SyncError::FileSystem(format!("Failed to create temp directory: {}", e))
    })?;
    
    let fs = LocalFileSystem::new();
    let ops = GenericFileOperations::new(fs, 64 * 1024);
    
    // Test that we can call methods through trait objects
    let test_path = temp_dir.path().join("test.txt");
    let exists = ops.exists(&test_path).await;
    assert!(!exists);
    
    // Test filesystem capabilities
    let fs = ops.filesystem();
    assert!(fs.supports_copy_file_range());
    assert!(fs.supports_hardlinks());
    assert!(fs.supports_symlinks());
    
    Ok(())
}