//! Testcontainers helpers for privileged tests
//!
//! This module provides utilities for running tests that require elevated privileges
//! (e.g., chown, mknod) inside Docker containers with root access.

use testcontainers::{core::WaitFor, runners::AsyncRunner, GenericImage, ImageExt};

/// Create a Ubuntu container with Rust for privileged testing
///
/// The container runs with --privileged flag to allow operations that need root.
/// It keeps running with a sleep command so we can execute tests inside it.
///
/// # Errors
///
/// Returns error if container fails to start
pub async fn create_privileged_rust_container(
) -> Result<testcontainers::ContainerAsync<GenericImage>, Box<dyn std::error::Error>> {
    let image = GenericImage::new("rust", "1.83") // Full image has all libraries
        .with_wait_for(WaitFor::Nothing)
        .with_privileged(true)
        .with_cmd(vec!["sleep", "3600"]); // Keep container alive

    let container = image.start().await?;

    // Wait for container to be ready (poll with bounded timeout)
    let container_id = container.id();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        let ready = std::process::Command::new("docker")
            .args(["exec", container_id, "true"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        if ready {
            break;
        }

        if std::time::Instant::now() >= deadline {
            return Err("Container not ready within 15s".into());
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    }

    Ok(container)
}

/// Helper to check if Docker is available and supports privileged containers
///
/// This checks both that Docker is installed and that privileged containers
/// can be run (fails early in rootless Docker environments).
#[allow(dead_code)]
pub fn can_use_containers() -> bool {
    // First check if docker is installed
    let docker_available = std::process::Command::new("docker")
        .arg("version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);

    if !docker_available {
        return false;
    }

    // Quick check: rootless Docker often rejects --privileged
    // Use a minimal busybox test to fail fast
    std::process::Command::new("docker")
        .args(["run", "--rm", "--privileged", "busybox", "true"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Execute shell commands inside a privileged container as root
///
/// This runs shell commands inside a container with root privileges,
/// useful for testing operations that require elevated permissions.
///
/// # Errors
///
/// Returns error if container creation or command execution fails
#[allow(dead_code)]
pub async fn run_shell_in_container(commands: &str) -> Result<String, Box<dyn std::error::Error>> {
    let container = create_privileged_rust_container().await?;
    let container_id = container.id();

    // Execute shell commands as root (use bash for better compatibility)
    let exec_output = std::process::Command::new("docker")
        .args(["exec", container_id, "bash", "-c", commands])
        .output()?;

    let stdout = String::from_utf8_lossy(&exec_output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&exec_output.stderr).to_string();

    if !exec_output.status.success() {
        return Err(format!(
            "Container execution failed:\nstdout: {}\nstderr: {}",
            stdout, stderr
        )
        .into());
    }

    Ok(stdout)
}

/// Execute shell commands in an existing container as root
///
/// # Errors
///
/// Returns error if command execution fails
#[allow(dead_code)]
pub fn run_shell_in_existing_container(
    container_id: &str,
    commands: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    // Execute shell commands as root (use bash for better compatibility)
    let exec_output = std::process::Command::new("docker")
        .args(["exec", container_id, "bash", "-c", commands])
        .output()?;

    let stdout = String::from_utf8_lossy(&exec_output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&exec_output.stderr).to_string();

    if !exec_output.status.success() {
        return Err(format!(
            "Container execution failed:\nstdout: {}\nstderr: {}",
            stdout, stderr
        )
        .into());
    }

    Ok(stdout)
}
