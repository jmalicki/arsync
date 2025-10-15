//! Command-line interface definitions

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

/// High-performance bulk file copying utility using `io_uring`
#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
#[allow(clippy::struct_excessive_bools)]
pub struct Args {
    /// Source directory or file
    #[arg(value_name = "SOURCE")]
    pub source: PathBuf,

    /// Destination directory or file
    #[arg(value_name = "DESTINATION")]
    pub destination: PathBuf,

    /// Queue depth for `io_uring` operations
    #[arg(long, default_value = "4096")]
    pub queue_depth: usize,

    /// Maximum total files in flight (across all CPU cores)
    ///
    /// Controls memory usage and system load by limiting the total number of
    /// files being copied simultaneously. Higher values increase throughput
    /// but consume more memory and file descriptors.
    ///
    /// Default: 1024
    /// High-performance (`NVMe`, 32GB+ RAM): 2048-4096
    /// Conservative (`HDD`, limited RAM): 256-512
    #[arg(long, default_value = "1024")]
    pub max_files_in_flight: usize,

    /// Number of CPU cores to use (0 = auto-detect)
    #[arg(long, default_value = "0")]
    pub cpu_count: usize,

    /// Buffer size in KB (0 = auto-detect)
    #[arg(long, default_value = "0")]
    pub buffer_size_kb: usize,

    /// Copy method to use
    #[arg(long, default_value = "auto")]
    pub copy_method: CopyMethod,

    // ========== rsync-compatible flags ==========
    /// Archive mode; same as -rlptgoD (recursive, links, perms, times, group, owner, devices)
    #[arg(short = 'a', long)]
    pub archive: bool,

    /// Recurse into directories
    #[arg(short = 'r', long)]
    pub recursive: bool,

    /// Copy symlinks as symlinks
    #[arg(short = 'l', long)]
    pub links: bool,

    /// Preserve permissions
    #[arg(short = 'p', long)]
    pub perms: bool,

    /// Preserve modification times
    #[arg(short = 't', long)]
    pub times: bool,

    /// Preserve group
    #[arg(short = 'g', long)]
    pub group: bool,

    /// Preserve owner (super-user only)
    #[arg(short = 'o', long)]
    pub owner: bool,

    /// Preserve device files (super-user only) and special files
    #[arg(short = 'D', long)]
    pub devices: bool,

    /// Preserve extended attributes
    #[arg(short = 'X', long)]
    pub xattrs: bool,

    /// Preserve ACLs (implies --perms)
    #[arg(short = 'A', long)]
    pub acls: bool,

    /// Preserve hard links
    #[arg(short = 'H', long)]
    pub hard_links: bool,

    /// Preserve access (use) times
    #[arg(short = 'U', long)]
    pub atimes: bool,

    /// Preserve creation times (when supported)
    #[arg(long)]
    pub crtimes: bool,

    // ========== Deprecated flags (for backwards compatibility) ==========
    /// Preserve extended attributes (deprecated: use -X/--xattrs)
    #[arg(long, hide = true)]
    pub preserve_xattr: bool,

    /// Preserve POSIX ACLs (deprecated: use -A/--acls)
    #[arg(long, hide = true)]
    pub preserve_acl: bool,

    // ========== Other flags ==========
    /// Show what would be copied without actually copying
    #[arg(long)]
    pub dry_run: bool,

    /// Show progress information
    #[arg(long)]
    pub progress: bool,

    /// Verbose output (-v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Quiet mode (suppress all output except errors)
    #[arg(short, long)]
    pub quiet: bool,

    // ========== Concurrency control flags ==========
    /// Disable adaptive concurrency control (fail fast on resource exhaustion)
    ///
    /// By default, arsync automatically reduces concurrency when hitting resource
    /// limits like "Too many open files" (EMFILE). This flag disables that behavior
    /// and causes arsync to exit immediately on such errors instead.
    ///
    /// Use this if you want strict resource limit enforcement or in CI/CD environments
    /// where you want to catch configuration issues early.
    #[arg(long)]
    pub no_adaptive_concurrency: bool,
}

#[derive(Debug, Clone, clap::ValueEnum)]
pub enum CopyMethod {
    /// Automatically choose the best method
    Auto,
    /// Use `copy_file_range` for same-filesystem copies
    CopyFileRange,
    /// Use splice for zero-copy operations
    Splice,
    /// Use traditional read/write operations
    ReadWrite,
}

impl Args {
    /// Validate command-line arguments
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - Source path does not exist
    /// - Source path is not a file or directory
    /// - Queue depth is outside valid bounds (1024-65536)
    /// - Max files in flight is outside valid bounds (1-10000)
    /// - Buffer size is too large (>1GB)
    /// - No CPU cores are available
    /// - Both --quiet and --verbose options are used
    pub fn validate(&self) -> Result<()> {
        // Check if source exists
        if !self.source.exists() {
            anyhow::bail!("Source path does not exist: {}", self.source.display());
        }

        // Check if source is readable
        if !self.source.is_dir() && !self.source.is_file() {
            anyhow::bail!(
                "Source path must be a file or directory: {}",
                self.source.display()
            );
        }

        // Check queue depth bounds
        if self.queue_depth < 1024 || self.queue_depth > 65_536 {
            anyhow::bail!(
                "Queue depth must be between 1024 and 65536, got: {}",
                self.queue_depth
            );
        }

        // Check max files in flight bounds
        if self.max_files_in_flight < 1 || self.max_files_in_flight > 10_000 {
            anyhow::bail!(
                "Max files in flight must be between 1 and 10000, got: {}",
                self.max_files_in_flight
            );
        }

        // Validate buffer size
        if self.buffer_size_kb > 1024 * 1024 {
            anyhow::bail!(
                "Buffer size too large (max 1GB): {} KB",
                self.buffer_size_kb
            );
        }

        // Check CPU count bounds
        let effective_cpu_count = self.effective_cpu_count();
        if effective_cpu_count == 0 {
            anyhow::bail!("No CPU cores available");
        }

        // Validate conflicting options
        if self.quiet && self.verbose > 0 {
            anyhow::bail!("Cannot use both --quiet and --verbose options");
        }

        Ok(())
    }

    /// Get the actual CPU count to use
    #[must_use]
    pub fn effective_cpu_count(&self) -> usize {
        if self.cpu_count == 0 {
            num_cpus::get()
        } else {
            self.cpu_count
        }
    }

    /// Get the actual buffer size in bytes
    #[allow(dead_code)]
    #[must_use]
    pub const fn effective_buffer_size(&self) -> usize {
        if self.buffer_size_kb == 0 {
            // Default to 64KB for now
            64 * 1024
        } else {
            self.buffer_size_kb * 1024
        }
    }

    /// Check if the source is a directory
    #[must_use]
    pub fn is_directory_copy(&self) -> bool {
        self.source.is_dir()
    }

    /// Check if the source is a single file
    #[must_use]
    pub fn is_file_copy(&self) -> bool {
        self.source.is_file()
    }

    /// Get buffer size in bytes
    #[must_use]
    pub const fn buffer_size_bytes(&self) -> usize {
        self.buffer_size_kb * 1024
    }

    // ========== rsync-compatible helper methods ==========

    /// Check if permissions should be preserved
    #[must_use]
    pub const fn should_preserve_permissions(&self) -> bool {
        self.perms || self.archive || self.acls
    }

    /// Check if ownership (user and/or group) should be preserved
    #[must_use]
    pub const fn should_preserve_ownership(&self) -> bool {
        self.owner || self.group || self.archive
    }

    /// Check if user ownership should be preserved
    #[allow(dead_code)]
    #[must_use]
    pub const fn should_preserve_owner(&self) -> bool {
        self.owner || self.archive
    }

    /// Check if group ownership should be preserved
    #[allow(dead_code)]
    #[must_use]
    pub const fn should_preserve_group(&self) -> bool {
        self.group || self.archive
    }

    /// Check if timestamps should be preserved
    #[must_use]
    pub const fn should_preserve_timestamps(&self) -> bool {
        self.times || self.archive
    }

    /// Check if access times should be preserved
    #[allow(dead_code)]
    #[must_use]
    pub const fn should_preserve_atimes(&self) -> bool {
        self.atimes
    }

    /// Check if creation times should be preserved
    #[allow(dead_code)]
    #[must_use]
    pub const fn should_preserve_crtimes(&self) -> bool {
        self.crtimes
    }

    /// Check if extended attributes should be preserved
    #[must_use]
    pub const fn should_preserve_xattrs(&self) -> bool {
        self.xattrs || self.preserve_xattr
    }

    /// Check if ACLs should be preserved
    #[allow(dead_code)]
    #[must_use]
    pub const fn should_preserve_acls(&self) -> bool {
        self.acls || self.preserve_acl
    }

    /// Check if symlinks should be copied as symlinks
    #[allow(dead_code)]
    #[must_use]
    pub const fn should_preserve_links(&self) -> bool {
        self.links || self.archive
    }

    /// Check if hard links should be preserved
    #[allow(dead_code)]
    #[must_use]
    pub const fn should_preserve_hard_links(&self) -> bool {
        self.hard_links
    }

    /// Check if device files should be preserved
    #[allow(dead_code)]
    #[must_use]
    pub const fn should_preserve_devices(&self) -> bool {
        self.devices || self.archive
    }

    /// Check if recursive copying should be performed
    #[allow(dead_code)]
    #[must_use]
    pub const fn should_recurse(&self) -> bool {
        self.recursive || self.archive
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    #![allow(clippy::expect_used)]
    use super::*;
    use crate::error::SyncError;
    use compio::fs::File;
    use tempfile::TempDir;

    async fn create_temp_file() -> Result<(TempDir, PathBuf)> {
        let temp_dir = TempDir::new()
            .map_err(|e| SyncError::FileSystem(format!("Failed to create temp directory: {e}")))?;
        let file_path = temp_dir.path().join("test_file.txt");
        File::create(&file_path)
            .await
            .map_err(|e| SyncError::FileSystem(format!("Failed to create test file: {e}")))?;
        Ok((temp_dir, file_path))
    }

    async fn create_temp_dir() -> Result<(TempDir, PathBuf)> {
        let temp_dir = TempDir::new()
            .map_err(|e| SyncError::FileSystem(format!("Failed to create temp directory: {e}")))?;
        let sub_dir = temp_dir.path().join("test_dir");
        compio::fs::create_dir(&sub_dir)
            .await
            .map_err(|e| SyncError::FileSystem(format!("Failed to create test directory: {e}")))?;
        Ok((temp_dir, sub_dir))
    }

    #[compio::test]
    async fn test_validate_with_existing_file() {
        let (temp_dir, file_path) = create_temp_file().await.unwrap();
        let args = Args {
            source: file_path,
            destination: temp_dir.path().join("dest"),
            copy_method: CopyMethod::Auto,
            queue_depth: 4096,
            cpu_count: 2,
            buffer_size_kb: 1024,
            max_files_in_flight: 100,
            archive: false,
            recursive: false,
            links: false,
            perms: false,
            times: false,
            group: false,
            owner: false,
            devices: false,
            xattrs: true,
            acls: false,
            hard_links: false,
            atimes: false,
            crtimes: false,
            preserve_xattr: false,
            preserve_acl: false,
            dry_run: false,
            progress: false,
            verbose: 0,
            quiet: false,
            no_adaptive_concurrency: false,
        };

        assert!(args.validate().is_ok());
    }

    #[compio::test]
    async fn test_validate_with_existing_directory() {
        let (temp_dir, dir_path) = create_temp_dir().await.unwrap();
        let args = Args {
            source: dir_path,
            destination: temp_dir.path().join("dest"),
            copy_method: CopyMethod::Auto,
            queue_depth: 4096,
            cpu_count: 2,
            buffer_size_kb: 1024,
            max_files_in_flight: 100,
            archive: false,
            recursive: false,
            links: false,
            perms: false,
            times: false,
            group: false,
            owner: false,
            devices: false,
            xattrs: true,
            acls: false,
            hard_links: false,
            atimes: false,
            crtimes: false,
            preserve_xattr: false,
            preserve_acl: false,
            dry_run: false,
            progress: false,
            verbose: 0,
            quiet: false,
            no_adaptive_concurrency: false,
        };

        assert!(args.validate().is_ok());
    }

    #[test]
    fn test_validate_with_nonexistent_source() {
        let args = Args {
            source: PathBuf::from("/nonexistent/path"),
            destination: PathBuf::from("/tmp/dest"),
            copy_method: CopyMethod::Auto,
            queue_depth: 4096,
            cpu_count: 2,
            buffer_size_kb: 1024,
            max_files_in_flight: 100,
            archive: false,
            recursive: false,
            links: false,
            perms: false,
            times: false,
            group: false,
            owner: false,
            devices: false,
            xattrs: true,
            acls: false,
            hard_links: false,
            atimes: false,
            crtimes: false,
            preserve_xattr: false,
            preserve_acl: false,
            dry_run: false,
            progress: false,
            verbose: 0,
            quiet: false,
            no_adaptive_concurrency: false,
        };

        assert!(args.validate().is_err());
    }
}
