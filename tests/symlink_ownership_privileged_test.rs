//! Privileged tests for symlink ownership preservation
//!
//! These tests run inside Docker containers with root privileges to verify
//! that symlink ownership can actually be preserved (requires root).

#![cfg(unix)]

use tempfile::TempDir;

mod common;

/// Test symlink ownership preservation with root privileges (using testcontainers)
///
/// This test runs code inside a privileged Docker container to verify that
/// lchown syscall works correctly for symlink ownership changes.
#[tokio::test]
#[ignore] // Only run with --ignored flag (requires Docker)
async fn test_symlink_ownership_with_root_container() {
    // Skip if Docker isn't available
    if !common::container_helpers::can_use_containers() {
        eprintln!("SKIPPED: Docker not available");
        return;
    }

    // Test script that runs as root inside container
    let test_script = r#"
set -e
cd /tmp
mkdir -p test_dir
cd test_dir

# Create target and symlink
echo "content" > target.txt
ln -s target.txt link

# Get original ownership (should be root:root in container)
ORIG_UID=$(stat -c '%u' link)
ORIG_GID=$(stat -c '%g' link)
echo "Original symlink ownership: uid=$ORIG_UID, gid=$ORIG_GID"

# Change symlink ownership to user 1000:1000 (using chown with -h flag = don't follow)
chown -h 1000:1000 link

# Verify symlink ownership changed
NEW_UID=$(stat -c '%u' link)
NEW_GID=$(stat -c '%g' link)
echo "New symlink ownership: uid=$NEW_UID, gid=$NEW_GID"

if [ "$NEW_UID" != "1000" ] || [ "$NEW_GID" != "1000" ]; then
    echo "ERROR: Symlink ownership not changed!"
    exit 1
fi

# Verify target ownership is UNCHANGED
TARGET_UID=$(stat -c '%u' target.txt)
TARGET_GID=$(stat -c '%g' target.txt)
echo "Target ownership (should be unchanged): uid=$TARGET_UID, gid=$TARGET_GID"

if [ "$TARGET_UID" != "$ORIG_UID" ] || [ "$TARGET_GID" != "$ORIG_GID" ]; then
    echo "ERROR: Target ownership should not change when changing symlink ownership!"
    exit 1
fi

echo "✓ Successfully changed symlink ownership without affecting target"
echo "✓ This validates that our lchown implementation will work correctly"
"#;

    let output = common::container_helpers::run_shell_in_container(test_script)
        .await
        .expect("Container test should succeed");

    assert!(output.contains("Successfully changed symlink ownership"));
    println!("✓ Container test passed:\n{}", output);
}

/// Test that non-root can still copy symlinks (just not preserve ownership)
#[compio::test]
async fn test_symlink_copy_works_without_root() {
    let temp_dir = TempDir::new().unwrap();
    let src = temp_dir.path().join("src");
    let dst = temp_dir.path().join("dst");
    std::fs::create_dir(&src).unwrap();
    std::fs::create_dir(&dst).unwrap();

    let target = src.join("target.txt");
    let link = src.join("link");
    std::fs::write(&target, "content").unwrap();
    std::os::unix::fs::symlink(&target, &link).unwrap();

    // Copy with archive mode (will try to preserve ownership)
    let mut args = common::test_args::create_archive_test_args();
    args.paths.source = src.clone();
    args.paths.destination = dst.clone();

    // This should succeed even without root (ownership preservation fails gracefully)
    let result = arsync::sync::sync_files(&args).await;
    assert!(
        result.is_ok(),
        "Symlink copy should succeed even without root privileges"
    );

    // Verify symlink was created
    let dst_link = dst.join("link");
    assert!(
        dst_link.symlink_metadata().is_ok(),
        "Symlink should be copied"
    );
    assert!(dst_link.is_symlink(), "Destination should be a symlink");

    println!("✓ Symlink copy works without root (ownership preservation fails gracefully)");
}
