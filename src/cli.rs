//! Command-line interface definitions
//!
//! This module organizes CLI arguments by **functional usage** - each group
//! contains the options needed by a specific component or subsystem.

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

// Import types from other modules
pub use crate::metadata::MetadataConfig;
pub use crate::protocol::{Location, PipeRole};

/// High-performance bulk file copying utility using `io_uring`
///
/// Optimizations:
/// - `io_uring` for zero-copy async I/O
/// - SIMD-accelerated checksums (AVX512/AVX2/SSSE3)
/// - Parallel file processing across CPU cores
/// - Adaptive concurrency control
#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    /// Source and destination paths
    #[command(flatten)]
    pub paths: PathConfig,

    /// `FileOperations` configuration (`io_uring`, buffers)
    #[command(flatten)]
    pub io: IoConfig,

    /// Concurrency control configuration
    #[command(flatten)]
    pub concurrency: ConcurrencyConfig,

    /// Metadata preservation flags (used by copy operations)
    #[command(flatten)]
    pub metadata: MetadataConfig,

    /// Output and logging configuration
    #[command(flatten)]
    pub output: OutputConfig,

    /// Remote sync configuration
    #[command(flatten)]
    pub remote: RemoteConfig,
}

// ============================================================================
// FUNCTIONAL GROUPS: Organized by what component consumes them
// ============================================================================

/// Paths configuration
///
/// Used by: `main()`, `sync_files()`, `copy_directory()`
#[derive(clap::Args, Debug, Clone)]
pub struct PathConfig {
    /// Source directory or file
    ///
    /// Supports rsync-style syntax:
    ///   - Local path: `/path/to/source`
    ///   - Remote path: `user@host:/path/to/source`
    ///   - Remote path: `host:/path/to/source`
    #[arg(value_name = "SOURCE")]
    pub source: String,

    /// Destination directory or file
    ///
    /// Supports rsync-style syntax:
    ///   - Local path: `/path/to/dest`
    ///   - Remote path: `user@host:/path/to/dest`
    ///   - Remote path: `host:/path/to/dest`
    #[arg(value_name = "DESTINATION")]
    pub destination: String,
}

/// I/O and `FileOperations` configuration
///
/// Used by: `FileOperations::new()`, `copy_read_write()`
#[derive(clap::Args, Debug, Clone)]
#[command(next_help_heading = "I/O Performance Options")]
pub struct IoConfig {
    /// Queue depth for `io_uring` operations
    #[arg(long, default_value = "4096")]
    pub queue_depth: usize,

    /// Buffer size in KB (0 = auto-detect, default: 64KB)
    #[arg(long, default_value = "0")]
    pub buffer_size_kb: usize,

    /// Copy method to use
    #[arg(long, default_value = "auto")]
    pub copy_method: CopyMethod,

    /// Number of CPU cores to use (0 = auto-detect)
    #[arg(long, default_value = "0")]
    pub cpu_count: usize,
}

/// Concurrency control configuration
///
/// Used by: `AdaptiveConcurrencyController`, `traverse_and_copy_directory_iterative()`
#[derive(clap::Args, Debug, Clone)]
#[command(next_help_heading = "Concurrency Control")]
pub struct ConcurrencyConfig {
    /// Maximum total files in flight (across all CPU cores)
    ///
    /// Controls memory usage and system load by limiting the total number of
    /// files being copied simultaneously. Higher values increase throughput
    /// but consume more memory and file descriptors.
    ///
    /// Default: 1024
    /// High-performance (`NVMe`, 32GB+ RAM): 2048-4096
    /// Conservative (HDD, limited RAM): 256-512
    #[arg(long, default_value = "1024")]
    pub max_files_in_flight: usize,

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

impl ConcurrencyConfig {
    /// Convert to the options struct used by `AdaptiveConcurrencyController`
    ///
    /// Creates a validated `ConcurrencyOptions` with proper `NonZeroUsize` guarantees.
    #[must_use]
    pub fn to_options(&self) -> crate::adaptive_concurrency::ConcurrencyOptions {
        crate::adaptive_concurrency::ConcurrencyOptions::new(
            self.max_files_in_flight,
            self.no_adaptive_concurrency,
        )
    }
}

/// Output and logging configuration
///
/// Used by: `main()`, logging initialization, progress display
#[derive(clap::Args, Debug, Clone)]
#[command(next_help_heading = "Output Options")]
#[allow(clippy::struct_excessive_bools)]
pub struct OutputConfig {
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

    /// Enable pirate speak (arrr! 🏴‍☠️)
    #[arg(long, default_value = "false")]
    pub pirate: bool,
}

/// Remote sync configuration
///
/// Used by: protocol module, remote sync operations
#[derive(clap::Args, Debug, Clone)]
#[command(next_help_heading = "Remote Sync Options")]
#[allow(clippy::struct_excessive_bools)]
pub struct RemoteConfig {
    /// Run in server mode (for remote sync)
    #[arg(long, hide = true)]
    pub server: bool,

    /// Remote shell to use (default: ssh)
    #[arg(short = 'e', long = "rsh", default_value = "ssh")]
    pub remote_shell: String,

    /// Daemon mode (rsyncd compatibility)
    #[arg(long, hide = true)]
    pub daemon: bool,

    /// Pipe mode: communicate via stdin/stdout (for protocol testing)
    ///
    /// This mode is for testing the rsync wire protocol without SSH.
    /// NOT for normal use - local copies use `io_uring` direct operations!
    #[arg(long, hide = true)]
    pub pipe: bool,

    /// Pipe role: sender or receiver
    #[arg(long, requires = "pipe", value_enum)]
    pub pipe_role: Option<PipeRole>,

    /// Use rsync wire protocol format (for testing compatibility with rsync)
    ///
    /// When set, uses rsync's multiplexed I/O, varint encoding, and message tags.
    /// Without this flag, uses arsync's simpler native protocol.
    #[arg(long, requires = "pipe")]
    pub rsync_compat: bool,
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

impl Default for CopyMethod {
    fn default() -> Self {
        Self::Auto
    }
}

// ============================================================================
// IMPLEMENTATION: Convenience methods and validation
// ============================================================================

impl Args {
    /// Get the source location (parsed from string)
    ///
    /// # Errors
    ///
    /// Returns an error if the source path fails to parse
    pub fn get_source(&self) -> Result<Location> {
        Location::parse(&self.paths.source)
    }

    /// Get the destination location (parsed from string)
    ///
    /// # Errors
    ///
    /// Returns an error if the destination path fails to parse
    pub fn get_destination(&self) -> Result<Location> {
        Location::parse(&self.paths.destination)
    }

    /// Validate command-line arguments
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - Source path does not exist (for local paths)
    /// - Source path is not a file or directory (for local paths)
    /// - Queue depth is outside valid bounds (1024-65536)
    /// - Max files in flight is outside valid bounds (1-10000)
    /// - Buffer size is too large (>1GB)
    /// - No CPU cores are available
    /// - Both --quiet and --verbose options are used
    /// - Pipe mode is used without --pipe-role
    pub fn validate(&self) -> Result<()> {
        // Pipe mode: skip path validation (paths come from stdin/stdout)
        if self.remote.pipe {
            if self.remote.pipe_role.is_none() {
                anyhow::bail!("--pipe requires --pipe-role (sender or receiver)");
            }
            return self.validate_common();
        }

        // Get source and destination
        let source = self.get_source()?;
        let destination = self.get_destination()?;

        // Validate remote sync options
        if source.is_remote() || destination.is_remote() {
            // Remote sync mode
            if source.is_remote() && destination.is_remote() {
                anyhow::bail!("Cannot sync from remote to remote (yet)");
            }
            // Remote sync validation passes - will be validated when connecting
            return self.validate_common();
        }

        // Local sync mode - validate source exists
        let Location::Local(source_path) = &source else {
            unreachable!("Non-local source should have been handled above")
        };

        // Check if source exists
        if !source_path.exists() {
            anyhow::bail!("Source path does not exist: {}", source_path.display());
        }

        // Check if source is readable
        if !source_path.is_dir() && !source_path.is_file() {
            anyhow::bail!(
                "Source path must be a file or directory: {}",
                source_path.display()
            );
        }

        self.validate_common()
    }

    fn validate_common(&self) -> Result<()> {
        // Check queue depth bounds
        if self.io.queue_depth < 1024 || self.io.queue_depth > 65_536 {
            anyhow::bail!(
                "Queue depth must be between 1024 and 65536, got: {}",
                self.io.queue_depth
            );
        }

        // Check max files in flight bounds
        if self.concurrency.max_files_in_flight < 1 || self.concurrency.max_files_in_flight > 10_000
        {
            anyhow::bail!(
                "Max files in flight must be between 1 and 10000, got: {}",
                self.concurrency.max_files_in_flight
            );
        }

        // Validate buffer size
        if self.io.buffer_size_kb > 1024 * 1024 {
            anyhow::bail!(
                "Buffer size too large (max 1GB): {} KB",
                self.io.buffer_size_kb
            );
        }

        // Check CPU count bounds
        let effective_cpu_count = self.effective_cpu_count();
        if effective_cpu_count == 0 {
            anyhow::bail!("No CPU cores available");
        }

        // Validate conflicting options
        if self.output.quiet && self.output.verbose > 0 {
            anyhow::bail!("Cannot use both --quiet and --verbose options");
        }

        Ok(())
    }

    /// Get the actual CPU count to use
    #[must_use]
    pub fn effective_cpu_count(&self) -> usize {
        if self.io.cpu_count == 0 {
            num_cpus::get()
        } else {
            self.io.cpu_count
        }
    }

    /// Get the actual buffer size in bytes
    #[allow(dead_code)]
    #[must_use]
    pub const fn effective_buffer_size(&self) -> usize {
        if self.io.buffer_size_kb == 0 {
            // Default to 64KB for now
            64 * 1024
        } else {
            self.io.buffer_size_kb * 1024
        }
    }

    /// Check if the source is a directory (for local sources)
    #[must_use]
    pub fn is_directory_copy(&self) -> bool {
        if let Ok(Location::Local(path)) = self.get_source() {
            return path.is_dir();
        }
        false
    }

    /// Check if the source is a single file (for local sources)
    #[must_use]
    pub fn is_file_copy(&self) -> bool {
        if let Ok(Location::Local(path)) = self.get_source() {
            return path.is_file();
        }
        false
    }

    /// Get buffer size in bytes
    #[must_use]
    pub const fn buffer_size_bytes(&self) -> usize {
        self.io.buffer_size_kb * 1024
    }

    // ========== Convenience accessors for commonly used fields ==========

    /// Get source as a `PathBuf` (for local sources only)
    ///
    /// For local sources, returns the path. For remote sources, returns the remote path component.
    /// Use `get_source()` for full `Location` info including remote host/user.
    #[must_use]
    pub fn source(&self) -> PathBuf {
        match Location::parse(&self.paths.source) {
            Ok(Location::Local(path) | Location::Remote { path, .. }) => path,
            Err(_) => PathBuf::from(&self.paths.source),
        }
    }

    /// Get destination as a `PathBuf` (for local destinations only)
    ///
    /// For local destinations, returns the path. For remote destinations, returns the remote path component.
    /// Use `get_destination()` for full `Location` info including remote host/user.
    #[must_use]
    pub fn destination(&self) -> PathBuf {
        match Location::parse(&self.paths.destination) {
            Ok(Location::Local(path) | Location::Remote { path, .. }) => path,
            Err(_) => PathBuf::from(&self.paths.destination),
        }
    }

    /// Get queue depth (convenience method for backwards compatibility)
    #[must_use]
    pub const fn queue_depth(&self) -> usize {
        self.io.queue_depth
    }

    /// Get max files in flight (convenience method for backwards compatibility)
    #[must_use]
    pub const fn max_files_in_flight(&self) -> usize {
        self.concurrency.max_files_in_flight
    }

    /// Get copy method (convenience method for backwards compatibility)
    #[must_use]
    pub const fn copy_method(&self) -> &CopyMethod {
        &self.io.copy_method
    }

    /// Check if verbose mode is enabled (convenience method for backwards compatibility)
    #[must_use]
    pub const fn verbose(&self) -> u8 {
        self.output.verbose
    }

    /// Check if quiet mode is enabled (convenience method for backwards compatibility)
    #[must_use]
    pub const fn quiet(&self) -> bool {
        self.output.quiet
    }

    // ========== rsync-compatible helper methods ==========
    // These delegate to the MetadataConfig group for backwards compatibility
    // where full Args is used (mainly in tests)

    /// Check if permissions should be preserved (delegates to metadata config)
    #[allow(dead_code)] // For backwards compatibility
    #[must_use]
    pub const fn should_preserve_permissions(&self) -> bool {
        self.metadata.should_preserve_permissions()
    }

    /// Check if ownership should be preserved (delegates to metadata config)
    #[allow(dead_code)] // For backwards compatibility
    #[must_use]
    pub const fn should_preserve_ownership(&self) -> bool {
        self.metadata.should_preserve_ownership()
    }

    /// Check if timestamps should be preserved (delegates to metadata config)
    #[allow(dead_code)] // For backwards compatibility
    #[must_use]
    pub const fn should_preserve_timestamps(&self) -> bool {
        self.metadata.should_preserve_timestamps()
    }

    /// Check if xattrs should be preserved (delegates to metadata config)
    #[allow(dead_code)] // For backwards compatibility
    #[must_use]
    pub const fn should_preserve_xattrs(&self) -> bool {
        self.metadata.should_preserve_xattrs()
    }

    /// Check if symlinks should be preserved (delegates to metadata config)
    #[allow(dead_code)]
    #[must_use]
    pub const fn should_preserve_links(&self) -> bool {
        self.metadata.should_preserve_links()
    }

    /// Check if hard links should be preserved (delegates to metadata config)
    #[allow(dead_code)]
    #[must_use]
    pub const fn should_preserve_hard_links(&self) -> bool {
        self.metadata.should_preserve_hard_links()
    }

    /// Check if recursive copying should be performed (delegates to metadata config)
    #[allow(dead_code)]
    #[must_use]
    pub const fn should_recurse(&self) -> bool {
        self.metadata.recursive || self.metadata.archive
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

    /// Helper to create default test Args with custom paths
    fn create_test_args(source: PathBuf, destination: PathBuf) -> Args {
        Args {
            paths: PathConfig {
                source: source.display().to_string(),
                destination: destination.display().to_string(),
            },
            io: IoConfig {
                queue_depth: 4096,
                buffer_size_kb: 1024,
                copy_method: CopyMethod::Auto,
                cpu_count: 2,
            },
            concurrency: ConcurrencyConfig {
                max_files_in_flight: 100,
                no_adaptive_concurrency: false,
            },
            metadata: MetadataConfig {
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
            },
            output: OutputConfig {
                dry_run: false,
                progress: false,
                verbose: 0,
                quiet: false,
                pirate: false,
            },
            remote: RemoteConfig {
                server: false,
                remote_shell: "ssh".to_string(),
                daemon: false,
                pipe: false,
                pipe_role: None,
                rsync_compat: false,
            },
        }
    }

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
        let args = create_test_args(file_path, temp_dir.path().join("dest"));
        assert!(args.validate().is_ok());
    }

    #[compio::test]
    async fn test_validate_with_existing_directory() {
        let (temp_dir, dir_path) = create_temp_dir().await.unwrap();
        let args = create_test_args(dir_path, temp_dir.path().join("dest"));
        assert!(args.validate().is_ok());
    }

    #[test]
    fn test_validate_with_nonexistent_source() {
        let args = create_test_args(
            PathBuf::from("/nonexistent/path"),
            PathBuf::from("/tmp/dest"),
        );
        assert!(args.validate().is_err());
    }

    #[test]
    fn test_convenience_accessors() {
        let args = create_test_args(PathBuf::from("/test/src"), PathBuf::from("/test/dst"));

        // Test convenience methods work
        assert_eq!(args.source(), PathBuf::from("/test/src"));
        assert_eq!(args.destination(), PathBuf::from("/test/dst"));
        assert_eq!(args.queue_depth(), 4096);
        assert_eq!(args.max_files_in_flight(), 100);
        assert_eq!(args.verbose(), 0);
        assert!(!args.quiet());
    }
}
