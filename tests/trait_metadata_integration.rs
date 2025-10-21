//! Integration tests for AsyncMetadata trait
//!
//! Verifies that FileMetadata correctly implements the AsyncMetadata trait
//! and that the trait methods work with real filesystem metadata.

use arsync::traits::AsyncMetadata;
use compio_fs_extended::FileMetadata;
use std::time::SystemTime;
use tempfile::TempDir;

#[compio::test]
async fn test_file_metadata_implements_async_metadata() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let file_path = temp_dir.path().join("test.txt");
    
    // Create a test file
    std::fs::write(&file_path, b"Hello, World!").expect("Failed to write test file");
    
    // Get metadata via compio
    let compio_meta = compio::fs::metadata(&file_path).await.expect("Failed to get metadata");
    
    // Convert to FileMetadata
    use std::os::unix::fs::MetadataExt;
    let file_metadata = FileMetadata {
        size: compio_meta.len(),
        mode: compio_meta.mode(),
        uid: compio_meta.uid(),
        gid: compio_meta.gid(),
        nlink: compio_meta.nlink(),
        ino: compio_meta.ino(),
        dev: compio_meta.dev(),
        accessed: compio_meta.accessed().unwrap_or(SystemTime::UNIX_EPOCH),
        modified: compio_meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
        created: compio_meta.created().ok(),
        #[cfg(target_os = "linux")]
        attributes: None,
        #[cfg(target_os = "linux")]
        attributes_mask: None,
        #[cfg(target_os = "macos")]
        flags: None,
        #[cfg(target_os = "macos")]
        generation: None,
    };
    
    // Test AsyncMetadata trait methods
    assert_eq!(file_metadata.size(), 13); // "Hello, World!" is 13 bytes
    assert!(file_metadata.is_file());
    assert!(!file_metadata.is_dir());
    assert!(!file_metadata.is_symlink());
    assert!(!file_metadata.is_empty());
    assert_eq!(file_metadata.file_type(), "file");
    assert!(!file_metadata.is_special());
    
    // Test summary includes expected info
    let summary = file_metadata.summary();
    assert!(summary.contains("file (13 bytes)"));
    assert!(summary.contains(&format!("uid: {}", file_metadata.uid())));
    assert!(summary.contains(&format!("gid: {}", file_metadata.gid())));
}

#[compio::test]
async fn test_directory_metadata_implements_async_metadata() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    
    // Get metadata for the directory itself
    let compio_meta = compio::fs::metadata(temp_dir.path()).await.expect("Failed to get metadata");
    
    // Convert to FileMetadata
    use std::os::unix::fs::MetadataExt;
    let dir_metadata = FileMetadata {
        size: compio_meta.len(),
        mode: compio_meta.mode(),
        uid: compio_meta.uid(),
        gid: compio_meta.gid(),
        nlink: compio_meta.nlink(),
        ino: compio_meta.ino(),
        dev: compio_meta.dev(),
        accessed: compio_meta.accessed().unwrap_or(SystemTime::UNIX_EPOCH),
        modified: compio_meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
        created: compio_meta.created().ok(),
        #[cfg(target_os = "linux")]
        attributes: None,
        #[cfg(target_os = "linux")]
        attributes_mask: None,
        #[cfg(target_os = "macos")]
        flags: None,
        #[cfg(target_os = "macos")]
        generation: None,
    };
    
    // Test AsyncMetadata trait methods
    assert!(!dir_metadata.is_file());
    assert!(dir_metadata.is_dir());
    assert!(!dir_metadata.is_symlink());
    assert_eq!(dir_metadata.file_type(), "directory");
    assert!(!dir_metadata.is_special());
    
    let summary = dir_metadata.summary();
    assert!(summary.contains("directory"));
}

#[compio::test]
async fn test_is_same_file_with_hardlinks() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let file1 = temp_dir.path().join("file1.txt");
    let file2 = temp_dir.path().join("file2.txt");
    
    // Create file and hardlink
    std::fs::write(&file1, b"test").expect("Failed to write file");
    std::fs::hard_link(&file1, &file2).expect("Failed to create hardlink");
    
    // Get metadata for both
    let meta1_compio = compio::fs::metadata(&file1).await.expect("Failed to get metadata");
    let meta2_compio = compio::fs::metadata(&file2).await.expect("Failed to get metadata");
    
    use std::os::unix::fs::MetadataExt;
    let to_file_metadata = |m: compio::fs::Metadata| FileMetadata {
        size: m.len(),
        mode: m.mode(),
        uid: m.uid(),
        gid: m.gid(),
        nlink: m.nlink(),
        ino: m.ino(),
        dev: m.dev(),
        accessed: m.accessed().unwrap_or(SystemTime::UNIX_EPOCH),
        modified: m.modified().unwrap_or(SystemTime::UNIX_EPOCH),
        created: m.created().ok(),
        #[cfg(target_os = "linux")]
        attributes: None,
        #[cfg(target_os = "linux")]
        attributes_mask: None,
        #[cfg(target_os = "macos")]
        flags: None,
        #[cfg(target_os = "macos")]
        generation: None,
    };
    
    let meta1 = to_file_metadata(meta1_compio);
    let meta2 = to_file_metadata(meta2_compio);
    
    // Should be same file (same inode)
    assert!(meta1.is_same_file(&meta2));
    assert_eq!(meta1.inode_number(), meta2.inode_number());
    assert_eq!(meta1.device_id(), meta2.device_id());
    assert_eq!(meta1.link_count(), 2); // Two hard links
}

