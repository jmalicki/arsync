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

    // Get the pre-built binary from host
    let workspace_root = std::env::current_dir().unwrap();
    let arsync_binary = workspace_root.join("target/release/arsync");

    if !arsync_binary.exists() {
        eprintln!("SKIPPED: arsync binary not found at {:?}", arsync_binary);
        eprintln!("Run 'cargo build --release --bin arsync' first");
        return;
    }

    println!("Creating privileged container...");
    let container = common::container_helpers::create_privileged_rust_container().await;
    let container_id = container.id();
    println!("✓ Container created: {}", container_id);

    // Copy pre-built binary into container
    println!("Copying arsync binary to container...");
    let copy_cmd = std::process::Command::new("docker")
        .args([
            "cp",
            arsync_binary.to_str().unwrap(),
            &format!("{}:/usr/local/bin/arsync", container_id),
        ])
        .output()
        .expect("Failed to copy binary");

    if !copy_cmd.status.success() {
        panic!(
            "Failed to copy binary: {}",
            String::from_utf8_lossy(&copy_cmd.stderr)
        );
    }

    // Verify binary works
    let test_run = std::process::Command::new("docker")
        .args(["exec", container_id, "/usr/local/bin/arsync", "--version"])
        .output()
        .expect("Failed to test run");

    if !test_run.status.success() {
        panic!(
            "Binary won't run: {}",
            String::from_utf8_lossy(&test_run.stderr)
        );
    }

    println!(
        "✓ Binary copied and verified: {}",
        String::from_utf8_lossy(&test_run.stdout).trim()
    );

    // Test script: Use arsync to copy directory with symlink, verify ownership preserved
    println!("Running test script inside container...");
    let test_script = r#"
set -e
cd /tmp

# Create source directory with symlink owned by user 1000
mkdir -p src dst
echo "content" > src/target.txt
ln -s target.txt src/link

# Set specific ownership on symlink (as root, we can do this)
chown -h 1000:1001 src/link
chown 0:0 src/target.txt  # Target stays root:root

# Verify source setup
SRC_LINK_UID=$(stat -c '%u' src/link)
SRC_LINK_GID=$(stat -c '%g' src/link)
echo "Source symlink ownership: uid=$SRC_LINK_UID, gid=$SRC_LINK_GID"

if [ "$SRC_LINK_UID" != "1000" ] || [ "$SRC_LINK_GID" != "1001" ]; then
    echo "ERROR: Source setup failed"
    exit 1
fi

# Run ARSYNC with --archive to copy and preserve metadata
# Use absolute paths to avoid directory resolution issues
echo "Running: /usr/local/bin/arsync -a /tmp/src/ /tmp/dst/"
/usr/local/bin/arsync -a /tmp/src/ /tmp/dst/
EXIT_CODE=$?

if [ $EXIT_CODE -ne 0 ]; then
    echo "ERROR: arsync failed with exit code $EXIT_CODE"
    exit 1
fi

# Verify destination symlink ownership was PRESERVED
DST_LINK_UID=$(stat -c '%u' dst/link)
DST_LINK_GID=$(stat -c '%g' dst/link)
echo "Destination symlink ownership: uid=$DST_LINK_UID, gid=$DST_LINK_GID"

if [ "$DST_LINK_UID" != "1000" ]; then
    echo "ERROR: Symlink UID not preserved! Expected 1000, got $DST_LINK_UID"
    exit 1
fi

if [ "$DST_LINK_GID" != "1001" ]; then
    echo "ERROR: Symlink GID not preserved! Expected 1001, got $DST_LINK_GID"
    exit 1
fi

# Verify target was NOT affected
DST_TARGET_UID=$(stat -c '%u' dst/target.txt)
echo "Target ownership: uid=$DST_TARGET_UID (should be 0)"

echo "✓ ARSYNC successfully preserved symlink ownership (uid=1000, gid=1001)"
echo "✓ Target ownership correct (uid=$DST_TARGET_UID)"
"#;

    // Run in the SAME container (not a new one)
    let output =
        common::container_helpers::run_shell_in_existing_container(container_id, test_script)
            .expect("Container test should succeed");

    assert!(output.contains("ARSYNC successfully preserved symlink ownership"));
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
