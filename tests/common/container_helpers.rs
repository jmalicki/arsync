//! Testcontainers helpers for privileged tests
//!
//! This module provides utilities for running tests that require elevated privileges
//! (e.g., chown, mknod) inside Docker containers with root access.

use testcontainers::{core::WaitFor, runners::AsyncRunner, GenericImage, ImageExt};

/// Create a Ubuntu container with Rust for privileged testing
///
/// The container runs with --privileged flag to allow operations that need root.
/// It keeps running with a sleep command so we can execute tests inside it.
pub async fn create_privileged_rust_container() -> testcontainers::ContainerAsync<GenericImage> {
    let image = GenericImage::new("rust", "1.83") // Full image has all libraries
        .with_wait_for(WaitFor::Nothing)
        .with_privileged(true)
        .with_cmd(vec!["sleep", "3600"]); // Keep container alive

    let container = image.start().await.expect("Failed to start container");

    // Give container a moment to fully start
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    container
}

/// Helper to check if Docker is available
pub fn can_use_containers() -> bool {
    std::process::Command::new("docker")
        .arg("version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Execute shell commands inside a privileged container as root
///
/// This runs shell commands inside a container with root privileges,
/// useful for testing operations that require elevated permissions.
#[allow(dead_code)]
pub async fn run_shell_in_container(commands: &str) -> Result<String, Box<dyn std::error::Error>> {
    let container = create_privileged_rust_container().await;
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
