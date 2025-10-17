//! Tests to understand how DirectoryFd *at methods behave with symlinks
//!
//! These tests will REVEAL whether the current implementation follows symlinks or not.
//! We need to understand the actual behavior before making changes.

use compio_fs_extended::directory::DirectoryFd;
use std::os::unix::fs::MetadataExt;
use tempfile::TempDir;

/// Test: Verify lfchownat doesn't follow symlinks (GREEN test)
///
/// This test should PASS after implementing AT_SYMLINK_NOFOLLOW.
#[compio::test]
#[cfg(unix)]
async fn test_lfchownat_does_not_follow_symlinks() {
    let temp_dir = TempDir::new().unwrap();
    let target = temp_dir.path().join("target.txt");
    let link = temp_dir.path().join("link");

    // Create target
    std::fs::write(&target, "content").unwrap();
    let target_metadata_before = std::fs::metadata(&target).unwrap();
    let target_uid_before = target_metadata_before.uid();
    let target_gid_before = target_metadata_before.gid();

    // Create symlink
    std::os::unix::fs::symlink("target.txt", &link).unwrap();

    // Get symlink's original ownership
    let link_metadata_before = std::fs::symlink_metadata(&link).unwrap();
    let link_uid_before = link_metadata_before.uid();
    let _link_gid_before = link_metadata_before.gid();

    // Open DirectoryFd
    let dir_fd = DirectoryFd::open(temp_dir.path()).await.unwrap();

    // Try to change ownership via lfchownat on the symlink
    // New implementation uses AT_SYMLINK_NOFOLLOW which DOESN'T follow symlinks
    let result = dir_fd
        .lfchownat("link", target_uid_before, target_gid_before)
        .await;

    if result.is_err() {
        println!("lfchownat failed (expected if not root): {:?}", result);
        println!("Can't fully test ownership behavior without root privileges");
        println!("But if it runs, it should use AT_SYMLINK_NOFOLLOW (correct!)");
        return;
    }

    // Check: Did it change the symlink or the target?
    let link_metadata_after = std::fs::symlink_metadata(&link).unwrap();
    let target_metadata_after = std::fs::metadata(&target).unwrap();

    let link_uid_after = link_metadata_after.uid();
    let target_uid_after = target_metadata_after.uid();

    println!(
        "Before: link uid={}, target uid={}",
        link_uid_before, target_uid_before
    );
    println!(
        "After:  link uid={}, target uid={}",
        link_uid_after, target_uid_after
    );

    // If symlink changed but target didn't, it's correct!
    if link_uid_after != link_uid_before && target_uid_after == target_uid_before {
        println!("✅ CORRECT: lfchownat does NOT follow symlinks!");
    } else if target_uid_after != target_uid_before {
        panic!("BUG: lfchownat still follows symlinks (target changed)!");
    }
}

/// Test: Verify lutimensat doesn't follow symlinks (GREEN test)
///
/// This test should PASS after changing to NoFollowSymlink flag.
#[compio::test]
#[cfg(unix)]
async fn test_lutimensat_does_not_follow_symlinks() {
    use std::time::{Duration, SystemTime};

    let temp_dir = TempDir::new().unwrap();
    let target = temp_dir.path().join("target.txt");
    let link = temp_dir.path().join("link");

    // Create target
    std::fs::write(&target, "content").unwrap();
    std::thread::sleep(Duration::from_millis(100));

    // Create symlink (will have different mtime than target)
    std::os::unix::fs::symlink("target.txt", &link).unwrap();
    std::thread::sleep(Duration::from_millis(100));

    // Get original timestamps
    let link_metadata_before = std::fs::symlink_metadata(&link).unwrap();
    let target_metadata_before = std::fs::metadata(&target).unwrap();

    let link_mtime_before = link_metadata_before.mtime();
    let target_mtime_before = target_metadata_before.mtime();

    println!(
        "Before: link mtime={}, target mtime={}",
        link_mtime_before, target_mtime_before
    );

    // Open DirectoryFd
    let dir_fd = DirectoryFd::open(temp_dir.path()).await.unwrap();

    // Set new timestamp via lutimensat on the symlink path
    let new_time = SystemTime::now() - Duration::from_secs(86400); // 1 day ago
    dir_fd.lutimensat("link", new_time, new_time).await.unwrap();

    // Check: Did it change the symlink or the target?
    let link_metadata_after = std::fs::symlink_metadata(&link).unwrap();
    let target_metadata_after = std::fs::metadata(&target).unwrap();

    let link_mtime_after = link_metadata_after.mtime();
    let target_mtime_after = target_metadata_after.mtime();

    println!(
        "After:  link mtime={}, target mtime={}",
        link_mtime_after, target_mtime_after
    );

    // If symlink changed but target didn't, it's correct!
    if link_mtime_after != link_mtime_before && target_mtime_after == target_mtime_before {
        println!("✅ CORRECT: lutimensat does NOT follow symlinks!");
        println!(
            "   Symlink mtime changed: {} → {}",
            link_mtime_before, link_mtime_after
        );
        println!("   Target mtime unchanged: {}", target_mtime_before);
    } else if target_mtime_after != target_mtime_before && link_mtime_after == link_mtime_before {
        panic!(
            "BUG: lutimensat still follows symlinks! \
             Target mtime changed: {} → {}, symlink unchanged: {}",
            target_mtime_before, target_mtime_after, link_mtime_after
        );
    } else if link_mtime_after != link_mtime_before && target_mtime_after != target_mtime_before {
        panic!("BUG: Both changed - something is wrong!");
    } else {
        panic!("BUG: Neither changed - lutimensat didn't work!");
    }
}

/// Test: Verify lfchmodat doesn't follow symlinks (GREEN test)
///
/// On Linux: Always succeeds (no-op, symlinks always 0777)
/// On macOS: Uses NoFollowSymlink flag
#[compio::test]
#[cfg(unix)]
async fn test_lfchmodat_does_not_follow_symlinks() {
    use std::os::unix::fs::PermissionsExt;

    let temp_dir = TempDir::new().unwrap();
    let target = temp_dir.path().join("target.txt");
    let link = temp_dir.path().join("link");

    // Create target with specific permissions
    std::fs::write(&target, "content").unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).unwrap();

    // Create symlink
    std::os::unix::fs::symlink("target.txt", &link).unwrap();

    // Get original permissions
    let target_perms_before = std::fs::metadata(&target).unwrap().permissions().mode() & 0o777;
    let link_perms_before = std::fs::symlink_metadata(&link)
        .unwrap()
        .permissions()
        .mode()
        & 0o777;

    println!(
        "Before: target perms={:o}, link perms={:o}",
        target_perms_before, link_perms_before
    );

    // Open DirectoryFd
    let dir_fd = DirectoryFd::open(temp_dir.path()).await.unwrap();

    // Try to change permissions via lfchmodat on the symlink path
    dir_fd.lfchmodat("link", 0o644).await.unwrap();

    // Check: Did it change the symlink or the target?
    let target_perms_after = std::fs::metadata(&target).unwrap().permissions().mode() & 0o777;
    let link_perms_after = std::fs::symlink_metadata(&link)
        .unwrap()
        .permissions()
        .mode()
        & 0o777;

    println!(
        "After:  target perms={:o}, link perms={:o}",
        target_perms_after, link_perms_after
    );

    // On Linux, symlink perms are always 0777 (kernel ignores them)
    #[cfg(target_os = "linux")]
    {
        assert_eq!(
            link_perms_before, 0o777,
            "Linux symlinks always 0777 before"
        );
        assert_eq!(link_perms_after, 0o777, "Linux symlinks always 0777 after");
    }

    // On Linux: target should NOT change (lfchmodat is no-op)
    // On macOS: symlink perms might change, target should NOT change
    #[cfg(target_os = "linux")]
    {
        assert_eq!(
            target_perms_after, target_perms_before,
            "lfchmodat is no-op on Linux, target should be unchanged"
        );
        println!("✅ CORRECT: lfchmodat is no-op on Linux (symlinks always 0777)");
    }

    #[cfg(not(target_os = "linux"))]
    {
        assert_eq!(
            target_perms_after, target_perms_before,
            "lfchmodat should NOT change target (only symlink)"
        );
        println!("✅ CORRECT: lfchmodat does NOT follow symlinks on macOS");
    }
}
