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

mod common;

/// Test: Documents that FD-based ownership operations follow symlinks (design constraint)
///
/// **Design Constraint**: Uses fchown which operates on the file the FD points to
/// **Issue**: File::open(symlink) opens the TARGET, so fchown changes target's ownership
/// **Solution**: Use path-based l* functions (lchown, lfchownat) for symlink metadata
#[compio::test]
#[cfg(unix)]
#[ignore] // Ignored - requires root to test ownership changes fully
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

/// Test: Demonstrate that File::open on symlink opens the TARGET (GREEN - documents expected behavior)
///
/// **Important Design Constraint**: You cannot use FD-based operations on symlinks!
///
/// When you call `File::open(symlink_path)`, it opens the TARGET file, not the symlink.
/// This means ALL fd-based operations operate on the target:
/// - fchown -> changes target's ownership (not symlink's)  
/// - fchmod -> changes target's permissions (not symlink's)
/// - fgetxattr -> reads target's xattrs (not symlink's)
/// - futimens -> changes target's timestamps (not symlink's)
///
/// **Solution**: Must use path-based l* syscalls that explicitly don't follow:
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

/// Test: Documents that XattrOps trait (FD-based) follows symlinks (design constraint)
///
/// **Design Constraint**: XattrOps trait uses fd-based operations (fgetxattr, fsetxattr)
/// **Fundamental Issue**: File::open(symlink) opens the TARGET, not the symlink!
/// **Solution**: Can't use fd-based xattr operations on symlinks - must use path-based l* functions
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

/// Test: Symlink ownership should be preserved during directory copy (RED - will FAIL)
///
/// This test verifies that when copying symlinks with --archive or --owner/--group flags,
/// the symlink's OWN ownership is preserved, not just the target's.
///
/// **Expected behavior**: FAIL because lchown is not implemented yet
#[compio::test]
#[cfg(unix)]
#[ignore] // Requires root to change ownership - test validates implementation exists
async fn test_symlink_ownership_preservation() {
    use std::os::unix::fs::MetadataExt;

    let temp_dir = TempDir::new().unwrap();
    let src_dir = temp_dir.path().join("src");
    let dst_dir = temp_dir.path().join("dst");

    std::fs::create_dir(&src_dir).unwrap();
    std::fs::create_dir(&dst_dir).unwrap();

    let target = src_dir.join("target.txt");
    let link = src_dir.join("link");

    // Create target
    std::fs::write(&target, "content").unwrap();

    // Create symlink
    std::os::unix::fs::symlink(&target, &link).unwrap();

    // Get symlink's original ownership (not target's)
    let link_metadata_before = std::fs::symlink_metadata(&link).unwrap();
    let link_uid_before = link_metadata_before.uid();
    let link_gid_before = link_metadata_before.gid();

    println!(
        "Source symlink ownership: uid={}, gid={}",
        link_uid_before, link_gid_before
    );

    // Copy directory with --archive (should preserve symlink metadata)
    let mut args = common::test_args::create_archive_test_args();
    args.paths.source = src_dir.clone();
    args.paths.destination = dst_dir.clone();

    arsync::sync::sync_files(&args).await.unwrap();

    // Check that the SYMLINK's ownership was preserved (not just the target)
    let dst_link = dst_dir.join("link");
    let dst_link_metadata = std::fs::symlink_metadata(&dst_link).unwrap();
    let dst_link_uid = dst_link_metadata.uid();
    let dst_link_gid = dst_link_metadata.gid();

    println!(
        "Destination symlink ownership: uid={}, gid={}",
        dst_link_uid, dst_link_gid
    );

    assert_eq!(
        dst_link_uid, link_uid_before,
        "Symlink UID should be preserved (not target's UID)"
    );
    assert_eq!(
        dst_link_gid, link_gid_before,
        "Symlink GID should be preserved (not target's GID)"
    );
}

/// Test: Symlink timestamps should be preserved during directory copy (RED - will FAIL)
///
/// This test verifies that when copying symlinks with --archive or --times flags,
/// the symlink's OWN timestamps are preserved, not just the target's.
///
/// **Expected behavior**: FAIL because lutimensat is not implemented yet
#[compio::test]
#[cfg(unix)]
async fn test_symlink_timestamp_preservation() {
    use std::os::unix::fs::MetadataExt;
    use std::time::Duration;

    let temp_dir = TempDir::new().unwrap();
    let src_dir = temp_dir.path().join("src");
    let dst_dir = temp_dir.path().join("dst");

    std::fs::create_dir(&src_dir).unwrap();
    std::fs::create_dir(&dst_dir).unwrap();

    let target = src_dir.join("target.txt");
    let link = src_dir.join("link");

    // Create target
    std::fs::write(&target, "content").unwrap();

    // Create symlink
    std::os::unix::fs::symlink(&target, &link).unwrap();

    // Set specific timestamps on the SYMLINK (not the target)
    // Using libc::lutimes or utimensat with AT_SYMLINK_NOFOLLOW
    use std::os::unix::ffi::OsStrExt;
    let link_cstr = std::ffi::CString::new(link.as_os_str().as_bytes()).unwrap();
    let specific_time = libc::timespec {
        tv_sec: 1609459200, // Jan 1, 2021
        tv_nsec: 123456789,
    };
    let times = [specific_time, specific_time];

    unsafe {
        libc::utimensat(
            libc::AT_FDCWD,
            link_cstr.as_ptr(),
            times.as_ptr(),
            libc::AT_SYMLINK_NOFOLLOW, // Don't follow!
        )
    };

    // Get symlink's timestamps (not target's)
    let link_metadata_before = std::fs::symlink_metadata(&link).unwrap();
    let link_mtime_before = link_metadata_before.mtime();
    let link_mtime_nsec_before = link_metadata_before.mtime_nsec();

    println!(
        "Source symlink mtime: {}.{:09}s",
        link_mtime_before, link_mtime_nsec_before
    );

    // Wait to ensure different timestamps if metadata isn't preserved
    std::thread::sleep(Duration::from_millis(100));

    // Copy directory with --archive (should preserve symlink timestamps)
    let mut args = common::test_args::create_archive_test_args();
    args.paths.source = src_dir.clone();
    args.paths.destination = dst_dir.clone();

    arsync::sync::sync_files(&args).await.unwrap();

    // Check that the SYMLINK's timestamps were preserved (not just the target)
    let dst_link = dst_dir.join("link");
    let dst_link_metadata = std::fs::symlink_metadata(&dst_link).unwrap();
    let dst_link_mtime = dst_link_metadata.mtime();
    let dst_link_mtime_nsec = dst_link_metadata.mtime_nsec();

    println!(
        "Destination symlink mtime: {}.{:09}s",
        dst_link_mtime, dst_link_mtime_nsec
    );

    assert_eq!(
        dst_link_mtime, link_mtime_before,
        "Symlink mtime seconds should be preserved"
    );

    #[cfg(target_os = "linux")]
    assert_eq!(
        dst_link_mtime_nsec, link_mtime_nsec_before,
        "Symlink mtime nanoseconds should be preserved on Linux"
    );
}

/// Test: Symlink xattrs should be preserved during directory copy (RED - will FAIL?)
///
/// This test verifies that when copying symlinks with --xattrs flag,
/// the symlink's OWN xattrs are preserved, not just the target's.
///
/// **Expected behavior**: Should work if lset_xattr_at_path is used
#[compio::test]
#[cfg(target_os = "linux")] // xattrs are Linux-specific
#[ignore] // TODO: lsetxattr on symlinks requires user_xattr mount option or fails with EPERM
async fn test_symlink_xattr_preservation() {
    use compio_fs_extended::xattr::{lget_xattr_at_path, lset_xattr_at_path};

    let temp_dir = TempDir::new().unwrap();
    let src_dir = temp_dir.path().join("src");
    let dst_dir = temp_dir.path().join("dst");

    std::fs::create_dir(&src_dir).unwrap();
    std::fs::create_dir(&dst_dir).unwrap();

    let target = src_dir.join("target.txt");
    let link = src_dir.join("link");

    // Create target
    std::fs::write(&target, "content").unwrap();

    // Create symlink
    std::os::unix::fs::symlink(&target, &link).unwrap();

    // Set xattr on the SYMLINK (not the target)
    lset_xattr_at_path(&link, "user.symlink_attr", b"symlink_value")
        .await
        .unwrap();

    // Verify symlink has the xattr
    let link_xattr_before = lget_xattr_at_path(&link, "user.symlink_attr")
        .await
        .unwrap();
    assert_eq!(link_xattr_before, b"symlink_value");

    println!(
        "Source symlink xattr: user.symlink_attr = {:?}",
        String::from_utf8_lossy(&link_xattr_before)
    );

    // Copy directory with --xattrs (should preserve symlink xattrs)
    let mut args = common::test_args::create_archive_test_args();
    args.paths.source = src_dir.clone();
    args.paths.destination = dst_dir.clone();
    args.metadata.xattrs = true;
    args.metadata.links = true;

    arsync::sync::sync_files(&args).await.unwrap();

    // Check that the SYMLINK's xattr was preserved (not just the target)
    let dst_link = dst_dir.join("link");
    let dst_link_xattr = lget_xattr_at_path(&dst_link, "user.symlink_attr")
        .await
        .unwrap();

    println!(
        "Destination symlink xattr: user.symlink_attr = {:?}",
        String::from_utf8_lossy(&dst_link_xattr)
    );

    assert_eq!(
        dst_link_xattr, b"symlink_value",
        "Symlink xattr should be preserved"
    );
}
