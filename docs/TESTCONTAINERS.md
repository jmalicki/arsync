# Testcontainers for Privileged Tests

This document explains how to run tests that require root privileges using Docker containers.

## Overview

Some filesystem operations require elevated privileges:
- Changing file/symlink ownership (`chown`, `lchown`)
- Creating device files (`mknod`)
- Setting capabilities or ACLs

Instead of requiring developers to run `sudo cargo test`, we use **testcontainers** to run these tests inside Docker containers with root privileges.

## Prerequisites

- Docker installed and running
- User has permission to run Docker commands

```bash
# Check Docker is available
docker version

# Add user to docker group (if needed)
sudo usermod -aG docker $USER
# Then logout/login for changes to take effect
```

## Running Privileged Tests

### Run Single Test with Docker

```bash
# Run a specific privileged test (requires --ignored flag)
cargo test --test symlink_ownership_privileged_test test_symlink_ownership_with_root_container -- --ignored --nocapture

# Run all privileged tests
cargo test -- --ignored --nocapture
```

### Tests Currently Using Containers

1. **`test_symlink_ownership_with_root_container`** (`tests/symlink_ownership_privileged_test.rs`)
   - Validates that `lchown` changes symlink ownership (not target)
   - Runs shell script inside privileged container
   - Verifies our `lchown_at_path()` implementation will work correctly

## How It Works

### Architecture

```
Developer Machine (no root)
  └─ cargo test
      └─ testcontainers-rs
          └─ Docker Container (--privileged, runs as root)
              └─ Test execution (can use chown, lchown, etc.)
```

### Test Flow

1. Test checks if Docker is available (`can_use_containers()`)
2. If not available, test is skipped with message
3. If available:
   - Testcontainers starts a `rust:1.83-slim` container with `--privileged`
   - Test executes shell commands inside container as root
   - Container automatically cleaned up after test

### Helper Functions

In `tests/common/container_helpers.rs`:

- **`create_privileged_rust_container()`** - Starts container with root access
- **`can_use_containers()`** - Checks if Docker is available
- **`run_shell_in_container(commands)`** - Executes shell script as root

## Writing Privileged Tests

### Example: Test Symlink Ownership

```rust
#[tokio::test]
#[ignore] // Requires Docker
async fn test_something_needing_root() {
    if !common::container_helpers::can_use_containers() {
        eprintln!("SKIPPED: Docker not available");
        return;
    }

    let script = r#"
    # Your shell commands running as root
    ln -s target link
    chown -h 1000:1000 link
    stat -c '%u' link  # Should output 1000
    "#;
    
    let output = common::container_helpers::run_shell_in_container(script)
        .await
        .expect("Test should succeed");
    
    assert!(output.contains("1000"));
}
```

### Best Practices

1. **Always mark as `#[ignore]`** - Prevents automatic execution in normal test runs
2. **Check Docker availability** - Skip gracefully if not available
3. **Use shell scripts** - Simpler than compiling Rust inside container
4. **Validate, don't assume** - Check return codes and outputs
5. **Keep containers lightweight** - Use `rust:slim` not full image
6. **Single-threaded** - Use `--test-threads=1` for container tests to avoid conflicts

## CI Integration

### GitHub Actions

```yaml
# Example CI job for privileged tests
- name: Run Privileged Tests
  run: |
    # Ensure Docker is available
    docker version
    # Run tests marked as ignored (privileged tests)
    cargo test -- --ignored --test-threads=1
```

### When to Use Containers vs Skip

**Use containers for:**
- Tests that validate correctness of root-only operations
- Integration tests that need full privileges
- Tests critical for security or metadata preservation

**Skip/ignore for:**
- Tests that would be slow (large Docker images)
- Tests that are nice-to-have but not critical
- Platform-specific tests (Windows can't test Unix ownership)

## Performance

- Container startup: ~2-3 seconds (with warm cache)
- Docker image pull (first time): ~30 seconds for rust:slim
- Test execution: Depends on test complexity
- **Total overhead**: ~5 seconds per test with warm cache

## Troubleshooting

### "Docker not available"

```bash
# Check Docker daemon is running
systemctl status docker  # Linux
docker ps  # Should show containers

# Check permissions
docker run hello-world  # Should work without sudo
```

### "Container execution failed"

- Check Docker logs: `docker logs <container-id>`
- Run container interactively: `docker run -it --privileged rust:1.83-slim sh`
- Verify script syntax with `bash -n script.sh`

### "Test times out"

- Increase timeout in test
- Check container isn't stuck (may need to kill manually)
- Verify network connectivity for image pull

## Limitations

- Requires Docker on developer machine (can skip otherwise)
- Slower than native tests (~2-3s overhead)
- Can't test macOS/Windows-specific privileged operations
- Container tests should be kept minimal and focused

## Future Enhancements

- [ ] Pre-build container image with arsync for faster tests
- [ ] Test full arsync copy operations inside container
- [ ] Add macOS container support (if needed)
- [ ] Parallel container execution (careful with resources)

