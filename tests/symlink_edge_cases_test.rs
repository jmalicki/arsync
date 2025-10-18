//! Comprehensive symlink edge case tests
//!
//! Tests for symlink chains, directory symlinks, and dereferencing behavior.

use std::fs;
use std::path::Path;
use tempfile::TempDir;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

mod utils;
use utils::rsync_compat::run_arsync;

// ============================================================================
// REUSABLE METADATA PRESERVATION HELPERS
// ============================================================================

#[cfg(unix)]
fn assert_metadata_preserved<P: AsRef<Path>>(src: P, dst: P, item_type: &str) {
    let src_meta = fs::symlink_metadata(&src).expect("Failed to get source metadata");
    let dst_meta = fs::symlink_metadata(&dst).expect("Failed to get destination metadata");

    // Check permissions (mode)
    let src_mode = src_meta.permissions().mode() & 0o777;
    let dst_mode = dst_meta.permissions().mode() & 0o777;
    assert_eq!(
        src_mode, dst_mode,
        "{}: Permissions not preserved. Source: {:o}, Dest: {:o}",
        item_type, src_mode, dst_mode
    );

    // Check file type
    assert_eq!(
        src_meta.is_file(),
        dst_meta.is_file(),
        "{}: File type mismatch",
        item_type
    );
    assert_eq!(
        src_meta.is_dir(),
        dst_meta.is_dir(),
        "{}: Directory type mismatch",
        item_type
    );
    assert_eq!(
        src_meta.is_symlink(),
        dst_meta.is_symlink(),
        "{}: Symlink type mismatch",
        item_type
    );

    // Check timestamps (with tolerance for filesystem precision)
    use std::os::unix::fs::MetadataExt;
    let time_diff = (src_meta.mtime() - dst_meta.mtime()).abs();
    assert!(
        time_diff <= 2,
        "{}: Modification time not preserved. Diff: {} seconds",
        item_type,
        time_diff
    );
}

#[cfg(unix)]
fn assert_symlink_target_matches<P: AsRef<Path>>(src_link: P, dst_link: P) {
    let src_target = fs::read_link(&src_link).expect("Failed to read source symlink");
    let dst_target = fs::read_link(&dst_link).expect("Failed to read destination symlink");

    assert_eq!(
        src_target, dst_target,
        "Symlink targets don't match. Source: {:?}, Dest: {:?}",
        src_target, dst_target
    );
}

// ============================================================================
// TEST: Symlink Chains (link → link → file)
// ============================================================================

#[test]
#[cfg(unix)]
fn test_symlink_chain_preserved() {
    let temp = TempDir::new().unwrap();
    let src = temp.path().join("src");
    let dst = temp.path().join("dst");

    fs::create_dir(&src).unwrap();

    // Create: file.txt ← link1 ← link2 (chain of symlinks)
    let file = src.join("file.txt");
    fs::write(&file, "content").unwrap();

    let link1 = src.join("link1");
    std::os::unix::fs::symlink("file.txt", &link1).unwrap();

    let link2 = src.join("link2");
    std::os::unix::fs::symlink("link1", &link2).unwrap();

    // Copy with -rl (preserve symlinks)
    run_arsync(&src, &dst, &["-r", "-l"]).expect("Sync failed");

    // Verify chain preserved
    let dst_file = dst.join("file.txt");
    let dst_link1 = dst.join("link1");
    let dst_link2 = dst.join("link2");

    // All should exist
    assert!(dst_file.exists(), "File not copied");
    assert!(dst_link1.exists(), "Link1 not copied");
    assert!(dst_link2.exists(), "Link2 not copied");

    // All should be correct types
    assert!(dst_file.is_file(), "File should be regular file");
    assert!(
        fs::symlink_metadata(&dst_link1).unwrap().is_symlink(),
        "Link1 should be symlink"
    );
    assert!(
        fs::symlink_metadata(&dst_link2).unwrap().is_symlink(),
        "Link2 should be symlink"
    );

    // Targets should match
    assert_symlink_target_matches(&link1, &dst_link1);
    assert_symlink_target_matches(&link2, &dst_link2);

    // Following the chain should work
    let content = fs::read_to_string(&dst_link2).expect("Failed to read through chain");
    assert_eq!(content, "content", "Content via symlink chain incorrect");

    println!("✓ Symlink chain (link→link→file) preserved correctly");
}

#[test]
#[cfg(unix)]
fn test_symlink_chain_dereferenced() {
    let temp = TempDir::new().unwrap();
    let src = temp.path().join("src");
    let dst = temp.path().join("dst");

    fs::create_dir(&src).unwrap();

    // Create: file.txt ← link1 ← link2 (chain of symlinks)
    let file = src.join("file.txt");
    fs::write(&file, "content").unwrap();

    let link1 = src.join("link1");
    std::os::unix::fs::symlink("file.txt", &link1).unwrap();

    let link2 = src.join("link2");
    std::os::unix::fs::symlink("link1", &link2).unwrap();

    // Copy with -r (NO -l: dereference symlinks)
    run_arsync(&src, &dst, &["-r"]).expect("Sync failed");

    // Verify chain dereferenced to files
    let dst_file = dst.join("file.txt");
    let dst_link1 = dst.join("link1");
    let dst_link2 = dst.join("link2");

    // All should exist as REGULAR FILES
    assert!(dst_file.is_file(), "File should be regular file");
    assert!(dst_link1.is_file(), "Link1 should be dereferenced to file");
    assert!(dst_link2.is_file(), "Link2 should be dereferenced to file");

    // All should have same content
    assert_eq!(fs::read_to_string(&dst_file).unwrap(), "content");
    assert_eq!(fs::read_to_string(&dst_link1).unwrap(), "content");
    assert_eq!(fs::read_to_string(&dst_link2).unwrap(), "content");

    println!("✓ Symlink chain (link→link→file) dereferenced correctly");
}

// ============================================================================
// TEST: Directory Symlinks (link → dir/)
// ============================================================================

#[test]
#[cfg(unix)]
fn test_directory_symlink_preserved() {
    let temp = TempDir::new().unwrap();
    let src = temp.path().join("src");
    let dst = temp.path().join("dst");

    fs::create_dir(&src).unwrap();

    // Create: subdir/ with file, and link → subdir/
    let subdir = src.join("subdir");
    fs::create_dir(&subdir).unwrap();
    fs::write(subdir.join("file.txt"), "content").unwrap();

    let dir_link = src.join("link_to_dir");
    std::os::unix::fs::symlink("subdir", &dir_link).unwrap();

    // Copy with -rl (preserve symlinks)
    run_arsync(&src, &dst, &["-r", "-l"]).expect("Sync failed");

    // Verify directory symlink preserved
    let dst_subdir = dst.join("subdir");
    let dst_dir_link = dst.join("link_to_dir");

    assert!(dst_subdir.is_dir(), "Subdir should be directory");
    assert!(
        fs::symlink_metadata(&dst_dir_link).unwrap().is_symlink(),
        "Directory link should be symlink"
    );

    // Target should match
    assert_symlink_target_matches(&dir_link, &dst_dir_link);

    // Following link should show directory contents
    assert!(
        dst_dir_link.join("file.txt").exists(),
        "File via dir symlink not accessible"
    );

    println!("✓ Directory symlink (link→dir/) preserved correctly");
}

#[test]
#[cfg(unix)]
fn test_directory_symlink_dereferenced() {
    let temp = TempDir::new().unwrap();
    let src = temp.path().join("src");
    let dst = temp.path().join("dst");

    fs::create_dir(&src).unwrap();

    // Create: subdir/ with files, and link → subdir/
    let subdir = src.join("subdir");
    fs::create_dir(&subdir).unwrap();
    fs::write(subdir.join("file1.txt"), "content1").unwrap();
    fs::write(subdir.join("file2.txt"), "content2").unwrap();

    let dir_link = src.join("link_to_dir");
    std::os::unix::fs::symlink("subdir", &dir_link).unwrap();

    // Copy with -r (NO -l: dereference symlinks)
    run_arsync(&src, &dst, &["-r"]).expect("Sync failed");

    // Verify directory symlink dereferenced to actual directory
    let dst_subdir = dst.join("subdir");
    let dst_dir_link = dst.join("link_to_dir");

    assert!(dst_subdir.is_dir(), "Subdir should be directory");
    assert!(
        dst_dir_link.is_dir(),
        "Dir link should be dereferenced to directory"
    );

    // Both should have same contents
    assert!(dst_subdir.join("file1.txt").exists());
    assert!(dst_subdir.join("file2.txt").exists());
    assert!(dst_dir_link.join("file1.txt").exists());
    assert!(dst_dir_link.join("file2.txt").exists());

    // Content should match
    assert_eq!(
        fs::read_to_string(dst_subdir.join("file1.txt")).unwrap(),
        "content1"
    );
    assert_eq!(
        fs::read_to_string(dst_dir_link.join("file1.txt")).unwrap(),
        "content1"
    );

    println!("✓ Directory symlink (link→dir/) dereferenced correctly");
}

// ============================================================================
// TEST: Metadata Preservation Across Types
// ============================================================================

#[test]
#[cfg(unix)]
fn test_metadata_preserved_files() {
    let temp = TempDir::new().unwrap();
    let src = temp.path().join("src");
    let dst = temp.path().join("dst");

    fs::create_dir(&src).unwrap();

    let file = src.join("file.txt");
    fs::write(&file, "content").unwrap();

    // Set specific permissions
    let mut perms = fs::metadata(&file).unwrap().permissions();
    perms.set_mode(0o644);
    fs::set_permissions(&file, perms).unwrap();

    // Copy with -a (archive: preserve all metadata)
    run_arsync(&src, &dst, &["-a"]).expect("Sync failed");

    // Verify metadata preserved
    assert_metadata_preserved(&file, &dst.join("file.txt"), "File");

    println!("✓ File metadata preserved correctly");
}

#[test]
#[cfg(unix)]
fn test_metadata_preserved_directories() {
    let temp = TempDir::new().unwrap();
    let src = temp.path().join("src");
    let dst = temp.path().join("dst");

    fs::create_dir(&src).unwrap();

    let subdir = src.join("subdir");
    fs::create_dir(&subdir).unwrap();

    // Set specific permissions
    let mut perms = fs::metadata(&subdir).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&subdir, perms).unwrap();

    // Copy with -a (archive: preserve all metadata)
    run_arsync(&src, &dst, &["-a"]).expect("Sync failed");

    // Verify metadata preserved
    assert_metadata_preserved(&subdir, &dst.join("subdir"), "Directory");

    println!("✓ Directory metadata preserved correctly");
}

#[test]
#[cfg(unix)]
fn test_metadata_preserved_symlinks() {
    let temp = TempDir::new().unwrap();
    let src = temp.path().join("src");
    let dst = temp.path().join("dst");

    fs::create_dir(&src).unwrap();

    let file = src.join("file.txt");
    fs::write(&file, "content").unwrap();

    let link = src.join("link");
    std::os::unix::fs::symlink("file.txt", &link).unwrap();

    // Copy with -a (archive: preserve all metadata, includes -l for symlinks)
    run_arsync(&src, &dst, &["-a"]).expect("Sync failed");

    // Verify symlink and metadata preserved
    let dst_link = dst.join("link");
    assert!(
        fs::symlink_metadata(&dst_link).unwrap().is_symlink(),
        "Should be symlink"
    );

    assert_symlink_target_matches(&link, &dst_link);
    assert_metadata_preserved(&link, &dst_link, "Symlink");

    println!("✓ Symlink metadata preserved correctly");
}

// ============================================================================
// TEST: Complex Scenarios
// ============================================================================

#[test]
#[cfg(unix)]
fn test_complex_symlink_tree() {
    let temp = TempDir::new().unwrap();
    let src = temp.path().join("src");
    let dst = temp.path().join("dst");

    fs::create_dir(&src).unwrap();

    // Create complex structure:
    // src/
    //   dir1/
    //     file.txt
    //   dir2/
    //     link_to_file -> ../dir1/file.txt
    //     link_to_dir -> ../dir1
    //   link_chain1 -> dir1/file.txt
    //   link_chain2 -> link_chain1

    let dir1 = src.join("dir1");
    let dir2 = src.join("dir2");
    fs::create_dir(&dir1).unwrap();
    fs::create_dir(&dir2).unwrap();

    fs::write(dir1.join("file.txt"), "content").unwrap();

    std::os::unix::fs::symlink("../dir1/file.txt", dir2.join("link_to_file")).unwrap();
    std::os::unix::fs::symlink("../dir1", dir2.join("link_to_dir")).unwrap();
    std::os::unix::fs::symlink("dir1/file.txt", src.join("link_chain1")).unwrap();
    std::os::unix::fs::symlink("link_chain1", src.join("link_chain2")).unwrap();

    // Copy with -rl
    run_arsync(&src, &dst, &["-r", "-l"]).expect("Sync failed");

    // Verify all symlinks preserved
    assert!(fs::symlink_metadata(dst.join("dir2/link_to_file"))
        .unwrap()
        .is_symlink());
    assert!(fs::symlink_metadata(dst.join("dir2/link_to_dir"))
        .unwrap()
        .is_symlink());
    assert!(fs::symlink_metadata(dst.join("link_chain1"))
        .unwrap()
        .is_symlink());
    assert!(fs::symlink_metadata(dst.join("link_chain2"))
        .unwrap()
        .is_symlink());

    // Verify all work correctly
    let content_via_chain = fs::read_to_string(dst.join("link_chain2")).unwrap();
    assert_eq!(content_via_chain, "content");

    let content_via_relative = fs::read_to_string(dst.join("dir2/link_to_file")).unwrap();
    assert_eq!(content_via_relative, "content");

    println!("✓ Complex symlink tree preserved correctly");
}
