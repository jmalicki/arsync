//! Command-line interface definitions
//!
//! This module organizes CLI arguments by **functional usage** - each group
//! contains the options needed by a specific component or subsystem.

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

// Import MetadataConfig from metadata module
pub use crate::metadata::MetadataConfig;

/// High-performance bulk file copying utility using `io_uring`
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
    #[arg(value_name = "SOURCE")]
    pub source: PathBuf,

    /// Destination directory or file
    #[arg(value_name = "DESTINATION")]
    pub destination: PathBuf,
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

    /// Parallel copy configuration
    #[command(flatten)]
    pub parallel: ParallelCopyConfig,
}

/// Parallel copy configuration for large files
///
/// Uses recursive binary splitting to copy large files in parallel.
#[derive(clap::Args, Debug, Clone)]
#[command(next_help_heading = "Parallel Copy Options")]
pub struct ParallelCopyConfig {
    /// Enable parallel copying for large files
    ///
    /// When enabled, files larger than --parallel-min-size will be split
    /// recursively and copied by multiple tasks concurrently.
    /// This can significantly improve throughput for large files on fast
    /// storage (`NVMe`), but may not help on slower devices (`HDD`).
    #[arg(long)]
    pub enabled: bool,

    /// Minimum file size (in MB) to trigger parallel copying
    ///
    /// Files smaller than this threshold will be copied sequentially.
    /// Default: 128 MB
    #[arg(long, default_value = "128", requires = "enabled")]
    pub min_file_size_mb: u64,

    /// Maximum recursion depth for parallel splits
    ///
    /// Creates up to 2^depth parallel tasks.
    /// Depth 2 = 4 tasks, Depth 3 = 8 tasks, Depth 4 = 16 tasks
    /// Recommended: 2-3 for `NVMe`, 1-2 for `SSD`
    /// Default: 2 (4 tasks)
    #[arg(long, default_value = "2", requires = "enabled")]
    pub max_depth: usize,

    /// Chunk size (in MB) for each read/write operation
    ///
    /// Larger chunks reduce syscall overhead but increase memory usage.
    /// Default: 2 MB
    #[arg(long, default_value = "2", requires = "enabled")]
    pub chunk_size_mb: usize,
}

impl ParallelCopyConfig {
    /// Get minimum file size in bytes
    #[must_use]
    pub const fn min_file_size_bytes(&self) -> u64 {
        self.min_file_size_mb * 1024 * 1024
    }

    /// Get chunk size in bytes
    #[must_use]
    pub const fn chunk_size_bytes(&self) -> usize {
        self.chunk_size_mb * 1024 * 1024
    }

    /// Determine if a file should be copied in parallel
    #[must_use]
    pub const fn should_use_parallel(&self, file_size: u64) -> bool {
        self.enabled && file_size >= self.min_file_size_bytes()
    }

    /// Validate parallel copy configuration
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - `max_depth` is greater than 6 (2^6 = 64 tasks would be excessive)
    /// - `chunk_size_mb` is 0 or greater than 64
    /// - `min_file_size_mb` is 0
    pub fn validate(&self) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }

        if self.max_depth > 6 {
            anyhow::bail!(
                "Parallel max depth must be <= 6 (2^6=64 tasks), got: {}",
                self.max_depth
            );
        }

        if self.chunk_size_mb == 0 || self.chunk_size_mb > 64 {
            anyhow::bail!(
                "Parallel chunk size must be between 1 and 64 MB, got: {}",
                self.chunk_size_mb
            );
        }

        if self.min_file_size_mb == 0 {
            anyhow::bail!("Parallel minimum file size must be greater than 0");
        }

        Ok(())
    }
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
        if !self.paths.source.exists() {
            anyhow::bail!(
                "Source path does not exist: {}",
                self.paths.source.display()
            );
        }

        // Check if source is readable
        if !self.paths.source.is_dir() && !self.paths.source.is_file() {
            anyhow::bail!(
                "Source path must be a file or directory: {}",
                self.paths.source.display()
            );
        }

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

        // Validate parallel copy configuration
        self.io.parallel.validate()?;

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

    /// Check if the source is a directory
    #[must_use]
    pub fn is_directory_copy(&self) -> bool {
        self.paths.source.is_dir()
    }

    /// Check if the source is a single file
    #[must_use]
    pub fn is_file_copy(&self) -> bool {
        self.paths.source.is_file()
    }

    /// Get buffer size in bytes
    #[must_use]
    pub const fn buffer_size_bytes(&self) -> usize {
        self.io.buffer_size_kb * 1024
    }

    // ========== Convenience accessors for commonly used fields ==========

    /// Get source path (convenience method for backwards compatibility)
    #[must_use]
    pub const fn source(&self) -> &PathBuf {
        &self.paths.source
    }

    /// Get destination path (convenience method for backwards compatibility)
    #[must_use]
    pub const fn destination(&self) -> &PathBuf {
        &self.paths.destination
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
                source,
                destination,
            },
            io: IoConfig {
                queue_depth: 4096,
                buffer_size_kb: 1024,
                copy_method: CopyMethod::Auto,
                cpu_count: 2,
                parallel: ParallelCopyConfig {
                    enabled: false,
                    min_file_size_mb: 128,
                    max_depth: 2,
                    chunk_size_mb: 2,
                },
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
        assert_eq!(args.source(), &PathBuf::from("/test/src"));
        assert_eq!(args.destination(), &PathBuf::from("/test/dst"));
        assert_eq!(args.queue_depth(), 4096);
        assert_eq!(args.max_files_in_flight(), 100);
        assert_eq!(args.verbose(), 0);
        assert!(!args.quiet());
    }
}
