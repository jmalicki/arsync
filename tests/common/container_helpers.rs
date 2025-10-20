//! Testcontainers helpers for privileged tests
//!
//! This module provides utilities for running tests that require elevated privileges
//! (e.g., chown, mknod) inside Docker containers with root access.

use std::path::PathBuf;
use testcontainers::{core::WaitFor, runners::AsyncRunner, GenericImage, ImageExt};

/// Create a Ubuntu container with Rust and arsync for privileged testing
///
/// The container runs with --privileged flag to allow operations that need root.
pub async fn create_privileged_container() -> testcontainers::ContainerAsync<GenericImage> {
    let image = GenericImage::new("rust", "latest")
        .with_wait_for(WaitFor::message_on_stdout("rustc"))
        .with_privileged(true);

    image.start().await.expect("Failed to start container")
}

/// Helper to check if we're running in CI or can use Docker
pub fn can_use_containers() -> bool {
    // Check if Docker is available
    std::process::Command::new("docker")
        .arg("version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Skip test if containers aren't available
#[macro_export]
macro_rules! skip_if_no_containers {
    () => {
        if !$crate::common::container_helpers::can_use_containers() {
            eprintln!("SKIPPED: Docker not available for container tests");
            return;
        }
    };
}
