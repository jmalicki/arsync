//! Integration tests for the trait-based filesystem integration

use arsync::traits::{AsyncFileSystem, FileOperations, GenericFileOperations};
use arsync::backends::LocalFileSystem;
use std::path::Path;
use tempfile::TempDir;

#[compio::test]
async fn test_end_to_end_file_operations() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = TempDir::new()?;
    let fs = LocalFileSystem::new();
    let ops = GenericFileOperations::new(fs, 64 * 1024);
    
    // Test file creation and writing
    let test_file = temp_dir.path().join("test.txt");
    let test_content = b"Hello, World! This is a test file.";
    
    let mut file = ops.filesystem().create_file(&test_file).await?;
    file.write_all(test_content).await?;
    
    // Test file reading
    let content = file.read_to_end().await?;
    assert_eq!(content, test_content);
    
    // Test file copying
    let copied_file = temp_dir.path().join("copied.txt");
    let bytes_copied = ops.copy_file(&test_file, &copied_file).await?;
    assert_eq!(bytes_copied, test_content.len() as u64);
    
    // Verify copied file
    let copied_content = ops.filesystem().open_file(&copied_file).await?.read_to_end().await?;
    assert_eq!(copied_content, test_content);
    
    // Test metadata
    let metadata = ops.metadata(&test_file).await?;
    assert!(metadata.is_file());
    assert_eq!(metadata.size(), test_content.len() as u64);
    
    Ok(())
}

#[compio::test]
async fn test_end_to_end_directory_operations() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = TempDir::new()?;
    let fs = LocalFileSystem::new();
    let ops = GenericFileOperations::new(fs, 64 * 1024);
    
    // Test directory creation
    let test_dir = temp_dir.path().join("test_dir");
    ops.create_directory_all(&test_dir).await?;
    assert!(ops.exists(&test_dir).await);
    
    // Test directory operations
    let dir = ops.filesystem().open_directory(&test_dir).await?;
    assert_eq!(dir.path(), test_dir);
    
    // Test file creation in directory
    let test_file = test_dir.join("test.txt");
    let test_content = b"Test content";
    
    let mut file = ops.filesystem().create_file(&test_file).await?;
    file.write_all(test_content).await?;
    
    // Test directory listing
    let entries = dir.read_dir().await?;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name(), "test.txt");
    
    // Test entry operations
    let entry = &entries[0];
    assert!(entry.is_file().await?);
    assert!(!entry.is_directory().await?);
    
    // Test file operations through entry
    let file_content = entry.open_file().await?.read_to_end().await?;
    assert_eq!(file_content, test_content);
    
    Ok(())
}

#[compio::test]
async fn test_filesystem_capabilities() -> Result<(), Box<dyn std::error::Error>> {
    let fs = LocalFileSystem::new();
    let ops = GenericFileOperations::new(fs, 64 * 1024);
    
    // Test filesystem capabilities
    assert_eq!(ops.filesystem_name(), "local");
    assert_eq!(ops.buffer_size(), 64 * 1024);
    
    let fs = ops.filesystem();
    assert!(fs.supports_copy_file_range());
    assert!(fs.supports_hardlinks());
    assert!(fs.supports_symlinks());
    
    Ok(())
}

#[compio::test]
async fn test_generic_function_works_with_any_filesystem() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = TempDir::new()?;
    let fs = LocalFileSystem::new();
    let ops = GenericFileOperations::new(fs, 64 * 1024);
    
    // Test that our generic function works
    let result = demonstrate_generic_usage(&ops).await?;
    assert!(result);
    
    Ok(())
}

/// Generic function that works with any filesystem implementing AsyncFileSystem
async fn demonstrate_generic_usage<FS: AsyncFileSystem>(
    operations: &GenericFileOperations<FS>,
) -> Result<bool, Box<dyn std::error::Error>> {
    // This function works with any filesystem backend
    let test_path = Path::new("/tmp/nonexistent");
    let exists = operations.exists(test_path).await;
    
    // Test filesystem capabilities
    let fs = operations.filesystem();
    let supports_copy = fs.supports_copy_file_range();
    let supports_hardlinks = fs.supports_hardlinks();
    let supports_symlinks = fs.supports_symlinks();
    
    // Return true if we can query the filesystem successfully
    Ok(!exists && (supports_copy || supports_hardlinks || supports_symlinks))
}