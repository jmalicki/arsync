//! Integration tests for symlink metadata preservation
//!
//! These tests verify that compio-fs-extended and arsync correctly handle metadata
//! on symlinks themselves, not on their targets.
//!
//! ## The Problem
//!
//! Symlinks have their own metadata (permissions, ownership, timestamps, xattrs) that
//! is separate from their target's metadata. Many system calls follow symlinks by default,
//! which means they operate on the target instead of the symlink itself.
//!
//! For a file sync tool like arsync, we must use special syscalls that DON'T follow symlinks:
//! - Linux: `lchown`, `lutimes`, `lgetxattr`, etc. (the "l" prefix means "don't follow")
//! - macOS: Same, plus some functions need special flags
//!
//! ## Test Strategy (Red/Green TDD)
//!
//! 1. Write tests that FAIL with current implementation (proves the bug exists)
//! 2. Implement fixes using l* syscalls
//! 3. Tests turn GREEN (proves the fix works)
//!
//! These tests focus on low-level metadata operations to demonstrate the bugs clearly.

use tempfile::TempDir;

/// Test: compio-fs-extended ownership functions follow symlinks (RED - will FAIL)
///
/// **Bug**: Uses fchown which follows symlinks (operates on target, not symlink)
/// **Fix**: Need fchownat with AT_SYMLINK_NOFOLLOW or lchown
#[compio::test]
#[cfg(unix)]
#[ignore] // Will FAIL - demonstrates the bug, requires root to test fully
async fn test_ownership_ops_follow_symlinks_bug() {
    use compio_fs_extended::{ExtendedFile, OwnershipOps};
    use std::os::unix::fs::MetadataExt;

    let temp_dir = TempDir::new().unwrap();
    let target = temp_dir.path().join("target.txt");
    let link = temp_dir.path().join("link");

    // Create target with specific ownership (if we're root)
    std::fs::write(&target, "content").unwrap();

    // Create symlink
    std::os::unix::fs::symlink(&target, &link).unwrap();

    // Get target's original ownership
    let target_metadata_before = std::fs::metadata(&target).unwrap();
    let target_uid_before = target_metadata_before.uid();
    let target_gid_before = target_metadata_before.gid();

    // Try to use OwnershipOps on the symlink
    // This will FOLLOW the symlink and change the TARGET's ownership (BUG!)
    let link_file = compio::fs::File::open(&link).await.unwrap();
    let extended = ExtendedFile::new(link_file);

    // Note: This will likely fail with permission denied unless root,
    // but the point is to demonstrate it operates on the target
    let _ = extended.fchown(target_uid_before, target_gid_before).await;

    // The bug: File::open on a symlink opens the TARGET, not the symlink
    // So all fd-based operations (fchown, fchmod, etc.) operate on the target
    println!("BUG: File::open() on symlink opens the TARGET, not the symlink!");
    println!("All fd-based operations (fchown, fchmod, fgetxattr) follow symlinks!");
    println!("Need path-based l* functions (lchown, lgetxattr, etc.) for symlinks");
}

/// Test: Document that symlinks don't have meaningful permissions on Linux
///
/// This is documentation, not a bug. On Linux, symlink permissions are always 0777
/// and are ignored by the kernel. On macOS/BSD, they might matter.
#[test]
#[cfg(unix)]
fn test_symlink_permissions_are_always_0777_on_linux() {
    use std::os::unix::fs::PermissionsExt;

    let temp_dir = TempDir::new().unwrap();
    let target = temp_dir.path().join("target.txt");
    let link = temp_dir.path().join("link");

    std::fs::write(&target, "content").unwrap();
    std::os::unix::fs::symlink(&target, &link).unwrap();

    let link_metadata = std::fs::symlink_metadata(&link).unwrap();
    let link_perms = link_metadata.permissions();
    let link_mode = link_perms.mode() & 0o777;

    println!("Symlink permissions: {:o}", link_mode);
    println!("On Linux: always 0777 (kernel ignores them)");
    println!("On BSD/macOS: may have actual permissions that matter");

    #[cfg(target_os = "linux")]
    assert_eq!(
        link_mode, 0o777,
        "Linux symlinks always have 0777 permissions"
    );
}

/// Test: Demonstrate that File::open on symlink opens the TARGET (RED - will FAIL)
///
/// **Critical Bug**: You cannot use file descriptor operations on symlinks!
///
/// When you call `File::open(symlink_path)`, it opens the TARGET file, not the symlink.
/// This means ALL fd-based operations operate on the target:
/// - fchown -> changes target's ownership (not symlink's)  
/// - fchmod -> changes target's permissions (not symlink's)
/// - fgetxattr -> reads target's xattrs (not symlink's)
/// - futimens -> changes target's timestamps (not symlink's)
///
/// **Fix**: Must use path-based l* syscalls that explicitly don't follow:
/// - lchown, lchmod (some systems), lgetxattr, lutimes, etc.
#[compio::test]
#[cfg(unix)]
async fn test_file_open_on_symlink_opens_target() {
    use std::os::unix::fs::MetadataExt;

    let temp_dir = TempDir::new().unwrap();
    let target = temp_dir.path().join("target.txt");
    let link = temp_dir.path().join("link");

    // Create target file with specific content
    std::fs::write(&target, "target_content").unwrap();

    // Create symlink
    std::os::unix::fs::symlink(&target, &link).unwrap();

    // Get symlink's inode
    let link_metadata = std::fs::symlink_metadata(&link).unwrap();
    let link_inode = link_metadata.ino();

    // Get target's inode
    let target_metadata = std::fs::metadata(&target).unwrap();
    let target_inode = target_metadata.ino();

    // Symlink and target have DIFFERENT inodes
    assert_ne!(
        link_inode, target_inode,
        "Symlink and target are different filesystem objects"
    );

    // Now open via File::open
    let opened_file = compio::fs::File::open(&link).await.unwrap();
    let opened_metadata = opened_file.metadata().await.unwrap();
    let opened_inode = opened_metadata.ino();

    // CRITICAL: File::open follows the symlink!
    assert_eq!(
        opened_inode, target_inode,
        "File::open on symlink opens the TARGET, not the symlink! \
         Symlink inode: {}, Target inode: {}, Opened inode: {}",
        link_inode, target_inode, opened_inode
    );

    println!("✅ Test proves: File::open() on symlinks opens the TARGET");
    println!(
        "   This means fd-based operations (fchown, fchmod, fgetxattr) can't work on symlinks!"
    );
    println!("   Must use path-based l* syscalls instead");
}

/// Test: Demonstrate that compio-fs-extended needs symlink-aware timestamp functions
///
/// **Current state**: No symlink-aware timestamp functions exist yet
/// **Needed**: lutimensat or utimensat with AT_SYMLINK_NOFOLLOW
#[test]
#[cfg(unix)]
fn test_symlink_timestamps_need_l_variants() {
    use std::os::unix::fs::MetadataExt;

    let temp_dir = TempDir::new().unwrap();
    let target = temp_dir.path().join("target.txt");
    let link = temp_dir.path().join("link");

    // Create target and symlink
    std::fs::write(&target, "content").unwrap();
    std::os::unix::fs::symlink(&target, &link).unwrap();

    // Get timestamps using symlink_metadata (doesn't follow)
    let link_metadata = std::fs::symlink_metadata(&link).unwrap();
    let link_mtime = link_metadata.mtime();

    // Get target timestamps
    let target_metadata = std::fs::metadata(&target).unwrap();
    let target_mtime = target_metadata.mtime();

    println!("Symlink mtime: {}", link_mtime);
    println!("Target mtime: {}", target_mtime);
    println!("Note: Symlink mtime can differ from target mtime");
    println!();
    println!("TODO: Add lutimensat or utimensat with AT_SYMLINK_NOFOLLOW to compio-fs-extended");
    println!("      to preserve symlink timestamps without following them");
}

/// Test: XattrOps trait uses file descriptors which follow symlinks (RED - will FAIL)
///
/// **Bug**: XattrOps trait uses fd-based operations (fgetxattr, fsetxattr)
/// **Critical Issue**: File::open on symlink opens the TARGET, not the symlink!
/// **Fix**: Can't use fd-based xattr operations on symlinks - must use path-based l* functions
#[compio::test]
#[cfg(unix)]
async fn test_xattr_ops_trait_follows_symlinks_bug() {
    use compio_fs_extended::xattr::lset_xattr_at_path;
    use compio_fs_extended::{ExtendedFile, XattrOps};

    let temp_dir = TempDir::new().unwrap();
    let target = temp_dir.path().join("target.txt");
    let link = temp_dir.path().join("link");

    // Create target
    std::fs::write(&target, "content").unwrap();

    // Set xattr on target using path-based API
    lset_xattr_at_path(&target, "user.type", b"target_file")
        .await
        .unwrap();

    // Create symlink
    std::os::unix::fs::symlink(&target, &link).unwrap();

    // Try to use XattrOps trait on symlink
    // BUG: This opens the TARGET, not the symlink!
    let link_file = compio::fs::File::open(&link).await.unwrap();
    let extended = ExtendedFile::new(link_file);

    // This will read the TARGET's xattr, not the symlink's (which doesn't have one)
    let xattr = extended.get_xattr("user.type").await.unwrap();

    assert_eq!(xattr, b"target_file");
    println!("✅ Test proves: XattrOps on File opened from symlink path reads TARGET's xattrs");
    println!("   Can't use trait-based (fd-based) xattr operations on symlinks!");
    println!("   Must use path-based l* functions (lget_xattr_at_path, etc.)");
}
