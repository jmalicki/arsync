//! Tests to understand how DirectoryFd *at methods behave with symlinks
//!
//! These tests will REVEAL whether the current implementation follows symlinks or not.
//! We need to understand the actual behavior before making changes.

use compio_fs_extended::directory::DirectoryFd;
use std::os::unix::fs::MetadataExt;
use tempfile::TempDir;

/// Test: Does DirectoryFd::fchownat follow symlinks?
///
/// This test will tell us if the current implementation follows symlinks.
/// Expected: Currently uses AtFlags::empty() which FOLLOWS symlinks.
#[compio::test]
#[cfg(unix)]
async fn test_current_fchownat_behavior_with_symlinks() {
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
    let link_gid_before = link_metadata_before.gid();

    // Open DirectoryFd
    let dir_fd = DirectoryFd::open(temp_dir.path()).await.unwrap();

    // Try to change ownership via fchownat on the symlink
    // Current implementation uses AtFlags::empty() which FOLLOWS symlinks
    let result = dir_fd
        .fchownat("link", target_uid_before, target_gid_before)
        .await;

    if result.is_err() {
        println!("fchownat failed (expected if not root): {:?}", result);
        println!("Can't fully test ownership behavior without root");
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

    // If target changed but symlink didn't, it follows symlinks
    if target_uid_after != target_uid_before && link_uid_after == link_uid_before {
        println!("❌ CONFIRMED: fchownat FOLLOWS symlinks (operates on target)");
    } else if link_uid_after != link_uid_before && target_uid_after == target_uid_before {
        println!("✅ SURPRISE: fchownat does NOT follow symlinks!");
    } else {
        println!("⚠️  Inconclusive (both or neither changed)");
    }
}

/// Test: Does DirectoryFd::utimensat follow symlinks?
///
/// This will reveal if utimensat operates on symlink or target.
/// Expected: Currently uses FollowSymlink flag.
#[compio::test]
#[cfg(unix)]
async fn test_current_utimensat_behavior_with_symlinks() {
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

    // Set new timestamp via utimensat on the symlink path
    let new_time = SystemTime::now() - Duration::from_secs(86400); // 1 day ago
    dir_fd.utimensat("link", new_time, new_time).await.unwrap();

    // Check: Did it change the symlink or the target?
    let link_metadata_after = std::fs::symlink_metadata(&link).unwrap();
    let target_metadata_after = std::fs::metadata(&target).unwrap();

    let link_mtime_after = link_metadata_after.mtime();
    let target_mtime_after = target_metadata_after.mtime();

    println!(
        "After:  link mtime={}, target mtime={}",
        link_mtime_after, target_mtime_after
    );

    // If target changed but symlink didn't, it follows symlinks
    if target_mtime_after != target_mtime_before && link_mtime_after == link_mtime_before {
        println!("❌ CONFIRMED: utimensat FOLLOWS symlinks (changed target, not link)");
        panic!(
            "BUG CONFIRMED: DirectoryFd::utimensat follows symlinks! \
             Target mtime changed from {} to {}, but symlink mtime stayed {}",
            target_mtime_before, target_mtime_after, link_mtime_after
        );
    } else if link_mtime_after != link_mtime_before && target_mtime_after == target_mtime_before {
        println!("✅ SURPRISE: utimensat does NOT follow symlinks!");
    } else if link_mtime_after != link_mtime_before && target_mtime_after != target_mtime_before {
        println!("⚠️  BOTH changed - utimensat may have affected both?");
    } else {
        println!("⚠️  Inconclusive");
    }
}

/// Test: Does DirectoryFd::fchmodat follow symlinks?
///
/// This will reveal if fchmodat operates on symlink or target.
/// Expected: Currently uses FollowSymlink flag.
///
/// Note: On Linux, symlink permissions are always 0777 and ignored,
/// so this test documents that behavior.
#[compio::test]
#[cfg(unix)]
async fn test_current_fchmodat_behavior_with_symlinks() {
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

    // Try to change permissions via fchmodat on the symlink path
    dir_fd.fchmodat("link", 0o644).await.unwrap();

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

    // If target changed, it follows symlinks
    if target_perms_after != target_perms_before {
        println!("❌ CONFIRMED: fchmodat FOLLOWS symlinks (changed target)");
        assert_eq!(
            target_perms_after, 0o644,
            "Target permissions should have changed to 0644"
        );
    } else {
        println!("⚠️  Target permissions didn't change (unexpected)");
    }
}
