//! Integration tests for AsyncDirectoryEntry trait implementation

use arsync::directory::DirectoryEntryWrapper;
use arsync::traits::{AsyncDirectoryEntry, AsyncMetadata};
use std::fs;
use std::io::Write;
use tempfile::TempDir;

#[compio::test]
async fn test_directory_entry_wrapper_for_file() -> anyhow::Result<()> {
    // Create a test directory with a file
    let temp_dir = TempDir::new()?;
    let file_path = temp_dir.path().join("test_file.txt");
    let mut file = fs::File::create(&file_path)?;
    file.write_all(b"Hello, World!")?;
    file.sync_all()?;

    // Read directory and get entry
    let mut entries: Vec<_> = fs::read_dir(temp_dir.path())?.collect();
    assert_eq!(entries.len(), 1);

    let entry = entries.pop().unwrap()?;
    let wrapper = DirectoryEntryWrapper::new(entry);

    // Test name
    assert_eq!(wrapper.name(), "test_file.txt");

    // Test path
    assert_eq!(wrapper.path(), file_path);

    // Test metadata
    let metadata = wrapper.metadata().await?;
    assert_eq!(metadata.size(), 13);
    assert!(metadata.is_file());
    assert!(!metadata.is_dir());

    Ok(())
}

#[compio::test]
async fn test_directory_entry_wrapper_for_directory() -> anyhow::Result<()> {
    // Create a test directory with a subdirectory
    let temp_dir = TempDir::new()?;
    let subdir_path = temp_dir.path().join("subdir");
    fs::create_dir(&subdir_path)?;

    // Read directory and get entry
    let mut entries: Vec<_> = fs::read_dir(temp_dir.path())?.collect();
    assert_eq!(entries.len(), 1);

    let entry = entries.pop().unwrap()?;
    let wrapper = DirectoryEntryWrapper::new(entry);

    // Test name
    assert_eq!(wrapper.name(), "subdir");

    // Test metadata
    let metadata = wrapper.metadata().await?;
    assert!(!metadata.is_file());
    assert!(metadata.is_dir());

    Ok(())
}

#[compio::test]
async fn test_directory_entry_convenience_methods() -> anyhow::Result<()> {
    // Create test directory with file and subdirectory
    let temp_dir = TempDir::new()?;

    let file_path = temp_dir.path().join("file.txt");
    fs::File::create(&file_path)?;

    let dir_path = temp_dir.path().join("subdir");
    fs::create_dir(&dir_path)?;

    // Get entries (consume them since DirEntry isn't Clone)
    let mut entries: Vec<_> =
        fs::read_dir(temp_dir.path())?.collect::<std::result::Result<Vec<_>, _>>()?;

    // Find and remove file entry
    let file_idx = entries
        .iter()
        .position(|e| e.file_name() == "file.txt")
        .unwrap();
    let file_entry = entries.remove(file_idx);
    let file_wrapper = DirectoryEntryWrapper::new(file_entry);

    // Find and remove dir entry
    let dir_idx = entries
        .iter()
        .position(|e| e.file_name() == "subdir")
        .unwrap();
    let dir_entry = entries.remove(dir_idx);
    let dir_wrapper = DirectoryEntryWrapper::new(dir_entry);

    // Test convenience methods
    assert!(file_wrapper.is_file().await?);
    assert!(!file_wrapper.is_dir().await?);
    assert!(!file_wrapper.is_symlink().await?);

    assert!(!dir_wrapper.is_file().await?);
    assert!(dir_wrapper.is_dir().await?);
    assert!(!dir_wrapper.is_symlink().await?);

    Ok(())
}

#[compio::test]
async fn test_directory_entry_metadata_fields() -> anyhow::Result<()> {
    // Create a test file
    let temp_dir = TempDir::new()?;
    let file_path = temp_dir.path().join("test.dat");
    let mut file = fs::File::create(&file_path)?;
    file.write_all(b"test data")?;
    file.sync_all()?;

    // Get entry and wrap
    let entry = fs::read_dir(temp_dir.path())?.next().unwrap()?;
    let wrapper = DirectoryEntryWrapper::new(entry);

    // Get metadata via trait
    let metadata = wrapper.metadata().await?;

    // Verify fields
    assert_eq!(metadata.size(), 9);
    assert!(metadata.is_file());
    assert_eq!(metadata.permissions() & 0o777, 0o644); // Default file permissions
    assert!(metadata.link_count() >= 1);
    assert!(metadata.inode_number() > 0);

    Ok(())
}
