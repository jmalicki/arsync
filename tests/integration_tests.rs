//! Integration tests for arsync

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

#[test]
fn test_help_output() {
    let mut cmd = Command::cargo_bin("arsync").unwrap();
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "High-performance async file copying utility",
        ));
}

#[test]
fn test_version_output() {
    let mut cmd = Command::cargo_bin("arsync").unwrap();
    cmd.arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("arsync"));
}

#[test]
fn test_missing_source() {
    let temp_dir = TempDir::new().unwrap();

    let mut cmd = Command::cargo_bin("arsync").unwrap();
    cmd.args(["/nonexistent/path", temp_dir.path().to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Source path does not exist"));
}

#[test]
fn test_invalid_queue_depth() {
    let temp_dir = TempDir::new().unwrap();

    let mut cmd = Command::cargo_bin("arsync").unwrap();
    cmd.args([
        temp_dir.path().to_str().unwrap(),
        temp_dir.path().to_str().unwrap(),
        "--queue-depth",
        "100", // Too small
    ])
    .assert()
    .failure()
    .stderr(predicate::str::contains(
        "Queue depth must be between 1024 and 65536",
    ));
}

#[test]
fn test_dry_run() {
    let temp_dir = TempDir::new().unwrap();

    let mut cmd = Command::cargo_bin("arsync").unwrap();
    cmd.args([
        temp_dir.path().to_str().unwrap(),
        temp_dir.path().to_str().unwrap(),
        "--dry-run",
    ])
    .assert()
    .success();
}
