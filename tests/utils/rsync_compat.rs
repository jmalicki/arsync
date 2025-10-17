//! Utility functions for rsync compatibility testing

use assert_cmd::Command;
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;

/// Check if rsync is available on the system
pub fn rsync_available() -> bool {
    StdCommand::new("rsync")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Run rsync with given flags
#[allow(dead_code)]
pub fn run_rsync(source: &Path, dest: &Path, flags: &[&str]) -> Result<(), String> {
    let mut cmd = StdCommand::new("rsync");
    cmd.args(flags);
    cmd.arg(format!("{}/", source.display())); // Note: rsync needs trailing slash
    cmd.arg(format!("{}/", dest.display()));

    let output = cmd.output().map_err(|e| format!("rsync failed: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "rsync failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(())
}

/// Run arsync with given flags
#[allow(dead_code)]
pub fn run_arsync(source: &Path, dest: &Path, flags: &[&str]) -> Result<(), String> {
    let mut cmd = Command::cargo_bin("arsync").unwrap();
    cmd.arg(source);
    cmd.arg(dest);
    cmd.args(flags);

    let output = cmd.output().map_err(|e| format!("arsync failed: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "arsync failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(())
}

/// Compare two files for identical metadata
#[allow(dead_code)]
pub fn compare_file_metadata(path1: &Path, path2: &Path, check_times: bool) -> Result<(), String> {
    let meta1 = fs::metadata(path1)
        .map_err(|e| format!("Failed to get metadata for {}: {}", path1.display(), e))?;
    let meta2 = fs::metadata(path2)
        .map_err(|e| format!("Failed to get metadata for {}: {}", path2.display(), e))?;

    // Compare permissions
    if meta1.permissions().mode() != meta2.permissions().mode() {
        return Err(format!(
            "Permissions differ for {}: {:o} vs {:o}",
            path1.display(),
            meta1.permissions().mode(),
            meta2.permissions().mode()
        ));
    }

    // Compare ownership (UID/GID)
    if meta1.uid() != meta2.uid() {
        return Err(format!(
            "UID differs for {}: {} vs {}",
            path1.display(),
            meta1.uid(),
            meta2.uid()
        ));
    }

    if meta1.gid() != meta2.gid() {
        return Err(format!(
            "GID differs for {}: {} vs {}",
            path1.display(),
            meta1.gid(),
            meta2.gid()
        ));
    }

    // Compare timestamps if requested
    if check_times {
        let mtime1 = meta1.modified().unwrap();
        let mtime2 = meta2.modified().unwrap();

        // Allow 1ms difference for filesystem precision
        let diff = mtime1
            .duration_since(mtime2)
            .or_else(|_| mtime2.duration_since(mtime1))
            .unwrap();

        if diff > std::time::Duration::from_millis(1) {
            return Err(format!(
                "Modified time differs for {} by {:?}: {:?} vs {:?}",
                path1.display(),
                diff,
                mtime1,
                mtime2
            ));
        }
    }

    Ok(())
}

/// Compare two directories recursively for identical content and metadata
#[allow(dead_code)]
pub fn compare_directories(dir1: &Path, dir2: &Path, check_times: bool) -> Result<(), String> {
    // Get all files in both directories
    let entries1: Vec<PathBuf> = walkdir::WalkDir::new(dir1)
        .into_iter()
        .filter_map(|e| e.ok())
        .map(|e| e.path().to_path_buf())
        .collect();

    let entries2: Vec<PathBuf> = walkdir::WalkDir::new(dir2)
        .into_iter()
        .filter_map(|e| e.ok())
        .map(|e| e.path().to_path_buf())
        .collect();

    // Compare count
    if entries1.len() != entries2.len() {
        return Err(format!(
            "Different number of entries: {} vs {}",
            entries1.len(),
            entries2.len()
        ));
    }

    // Compare each file
    for entry1 in &entries1 {
        let rel_path = entry1.strip_prefix(dir1).unwrap();
        let entry2 = dir2.join(rel_path);

        if !entry2.exists() {
            return Err(format!("Missing in destination: {}", rel_path.display()));
        }

        // Compare file type
        let meta1 = fs::symlink_metadata(entry1).unwrap();
        let meta2 = fs::symlink_metadata(&entry2).unwrap();

        if meta1.is_file() != meta2.is_file() {
            return Err(format!("Type mismatch for {}", rel_path.display()));
        }

        if meta1.is_dir() != meta2.is_dir() {
            return Err(format!("Type mismatch for {}", rel_path.display()));
        }

        if meta1.is_symlink() != meta2.is_symlink() {
            return Err(format!("Type mismatch for {}", rel_path.display()));
        }

        // Compare content for regular files
        if meta1.is_file() {
            let content1 = fs::read(entry1).unwrap();
            let content2 = fs::read(&entry2).unwrap();
            if content1 != content2 {
                return Err(format!("Content differs for {}", rel_path.display()));
            }

            // Compare metadata
            compare_file_metadata(entry1, &entry2, check_times)?;
        }

        // Compare symlink targets
        if meta1.is_symlink() {
            let target1 = fs::read_link(entry1).unwrap();
            let target2 = fs::read_link(&entry2).unwrap();
            if target1 != target2 {
                return Err(format!(
                    "Symlink target differs for {}: {:?} vs {:?}",
                    rel_path.display(),
                    target1,
                    target2
                ));
            }
        }
    }

    Ok(())
}
