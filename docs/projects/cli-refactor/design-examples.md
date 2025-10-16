# Modular CLI Patterns in Rust - Code Examples

This document provides concrete, copy-paste examples of modular CLI patterns in Rust using `clap`.

## Table of Contents
- [Current vs Proposed Structure](#current-vs-proposed-structure)
- [Pattern 1: Flatten (Recommended)](#pattern-1-flatten-recommended)
- [Pattern 2: Subcommands](#pattern-2-subcommands)
- [Pattern 3: Hybrid](#pattern-3-hybrid)
- [Real-World Examples](#real-world-examples)

---

## Current vs Proposed Structure

### Current Structure (Monolithic)

```rust
// src/cli.rs - ONE big struct
pub struct Args {
    // Core paths
    pub source: PathBuf,
    pub destination: PathBuf,
    
    // I/O config (5 fields)
    pub queue_depth: usize,
    pub max_files_in_flight: usize,
    pub cpu_count: usize,
    pub buffer_size_kb: usize,
    pub copy_method: CopyMethod,
    
    // Metadata options (15 fields!)
    pub archive: bool,
    pub recursive: bool,
    pub links: bool,
    pub perms: bool,
    pub times: bool,
    pub group: bool,
    pub owner: bool,
    pub devices: bool,
    pub xattrs: bool,
    pub acls: bool,
    pub hard_links: bool,
    pub atimes: bool,
    pub crtimes: bool,
    pub preserve_xattr: bool,  // deprecated
    pub preserve_acl: bool,    // deprecated
    
    // Output options (4 fields)
    pub dry_run: bool,
    pub progress: bool,
    pub verbose: u8,
    pub quiet: bool,
    
    // Advanced (1 field)
    pub no_adaptive_concurrency: bool,
}
```

### Proposed Structure (Modular)

```rust
// src/cli/mod.rs - Clean main struct
pub struct Args {
    /// Source directory or file
    #[arg(short, long)]
    pub source: PathBuf,
    
    /// Destination directory or file
    #[arg(short, long)]
    pub destination: PathBuf,
    
    /// I/O and performance configuration
    #[command(flatten)]
    pub io: IoConfig,
    
    /// Metadata preservation options
    #[command(flatten)]
    pub metadata: MetadataConfig,
    
    /// Output and verbosity control
    #[command(flatten)]
    pub output: OutputConfig,
}

// Each module is self-contained!
```

---

## Pattern 1: Flatten (Recommended)

This is what **cargo**, **rustup**, and most modern Rust CLIs use.

### File Structure

```
src/
├── cli/
│   ├── mod.rs           # Main Args with flatten
│   ├── io_config.rs     # I/O parameters
│   ├── metadata.rs      # Metadata preservation
│   ├── output.rs        # Progress/verbose/quiet
│   └── copy_method.rs   # Copy method enum
├── main.rs
└── lib.rs
```

### Implementation

#### `src/cli/mod.rs`

```rust
use clap::Parser;
use std::path::PathBuf;
use anyhow::Result;

// Re-export modules
pub mod io_config;
pub mod metadata;
pub mod output;
pub mod copy_method;

pub use io_config::IoConfig;
pub use metadata::MetadataConfig;
pub use output::OutputConfig;
pub use copy_method::CopyMethod;

/// High-performance bulk file copying utility using io_uring
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    /// Source directory or file
    #[arg(short, long)]
    pub source: PathBuf,
    
    /// Destination directory or file
    #[arg(short, long)]
    pub destination: PathBuf,
    
    /// I/O and performance configuration
    #[command(flatten)]
    pub io: IoConfig,
    
    /// Metadata preservation options
    #[command(flatten)]
    pub metadata: MetadataConfig,
    
    /// Output and verbosity control
    #[command(flatten)]
    pub output: OutputConfig,
}

impl Args {
    /// Validate all configurations
    pub fn validate(&self) -> Result<()> {
        // Check if source exists
        if !self.source.exists() {
            anyhow::bail!("Source path does not exist: {}", self.source.display());
        }
        
        if !self.source.is_dir() && !self.source.is_file() {
            anyhow::bail!(
                "Source path must be a file or directory: {}",
                self.source.display()
            );
        }
        
        // Validate each subsystem
        self.io.validate()?;
        self.output.validate()?;
        // metadata doesn't need validation (just bools)
        
        Ok(())
    }
    
    /// Check if the source is a directory
    pub fn is_directory_copy(&self) -> bool {
        self.source.is_dir()
    }
}
```

#### `src/cli/io_config.rs`

```rust
use clap::Parser;
use anyhow::Result;
use super::CopyMethod;

/// I/O and performance configuration
#[derive(Parser, Debug, Clone)]
pub struct IoConfig {
    /// Queue depth for io_uring operations
    #[arg(long, default_value = "4096")]
    pub queue_depth: usize,
    
    /// Maximum total files in flight (across all CPU cores)
    ///
    /// Controls memory usage and system load by limiting the total number of
    /// files being copied simultaneously.
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
    
    /// Disable adaptive concurrency control
    #[arg(long)]
    pub no_adaptive_concurrency: bool,
}

impl IoConfig {
    pub fn validate(&self) -> Result<()> {
        if self.queue_depth < 1024 || self.queue_depth > 65_536 {
            anyhow::bail!(
                "Queue depth must be between 1024 and 65536, got: {}",
                self.queue_depth
            );
        }
        
        if self.max_files_in_flight < 1 || self.max_files_in_flight > 10_000 {
            anyhow::bail!(
                "Max files in flight must be between 1 and 10000, got: {}",
                self.max_files_in_flight
            );
        }
        
        if self.buffer_size_kb > 1024 * 1024 {
            anyhow::bail!(
                "Buffer size too large (max 1GB): {} KB",
                self.buffer_size_kb
            );
        }
        
        let cpu_count = self.effective_cpu_count();
        if cpu_count == 0 {
            anyhow::bail!("No CPU cores available");
        }
        
        Ok(())
    }
    
    /// Get the actual CPU count to use
    pub fn effective_cpu_count(&self) -> usize {
        if self.cpu_count == 0 {
            num_cpus::get()
        } else {
            self.cpu_count
        }
    }
    
    /// Get buffer size in bytes
    pub fn buffer_size_bytes(&self) -> usize {
        if self.buffer_size_kb == 0 {
            64 * 1024  // Default 64KB
        } else {
            self.buffer_size_kb * 1024
        }
    }
}
```

#### `src/cli/metadata.rs`

```rust
use clap::Parser;

/// Metadata preservation options (rsync-compatible)
#[derive(Parser, Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct MetadataConfig {
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
}

impl MetadataConfig {
    /// Check if permissions should be preserved
    pub fn should_preserve_permissions(&self) -> bool {
        self.perms || self.archive || self.acls
    }
    
    /// Check if ownership (user and/or group) should be preserved
    pub fn should_preserve_ownership(&self) -> bool {
        self.owner || self.group || self.archive
    }
    
    /// Check if timestamps should be preserved
    pub fn should_preserve_timestamps(&self) -> bool {
        self.times || self.archive
    }
    
    /// Check if extended attributes should be preserved
    pub fn should_preserve_xattrs(&self) -> bool {
        self.xattrs
    }
    
    /// Check if recursive copying should be performed
    pub fn should_recurse(&self) -> bool {
        self.recursive || self.archive
    }
}
```

#### `src/cli/output.rs`

```rust
use clap::Parser;
use anyhow::Result;

/// Output and verbosity control
#[derive(Parser, Debug, Clone)]
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
}

impl OutputConfig {
    pub fn validate(&self) -> Result<()> {
        if self.quiet && self.verbose > 0 {
            anyhow::bail!("Cannot use both --quiet and --verbose options");
        }
        Ok(())
    }
}
```

### Usage in main.rs

```rust
use clap::Parser;
use cli::Args;

mod cli;

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    args.validate()?;
    
    // Access subsystem configs
    println!("CPU count: {}", args.io.effective_cpu_count());
    println!("Preserve perms: {}", args.metadata.should_preserve_permissions());
    
    if args.output.verbose > 0 {
        println!("Verbose mode level: {}", args.output.verbose);
    }
    
    Ok(())
}
```

### Benefits

✅ **Separation of Concerns**: Each module handles one aspect  
✅ **Independent Testing**: Test each module in isolation  
✅ **Validation Distribution**: Each config validates itself  
✅ **Easy to Extend**: Add new options to relevant module  
✅ **Backwards Compatible**: CLI interface unchanged  
✅ **Type Safety**: Full compile-time checking  

---

## Pattern 2: Subcommands

Use when you have **distinct operations** (like git, cargo, kubectl).

### Example: Adding Operations

```rust
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Args {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Copy files and directories (default operation)
    Copy {
        /// Source directory or file
        #[arg(short, long)]
        source: PathBuf,
        
        /// Destination directory or file
        #[arg(short, long)]
        destination: PathBuf,
        
        #[command(flatten)]
        io: IoConfig,
        
        #[command(flatten)]
        metadata: MetadataConfig,
        
        #[command(flatten)]
        output: OutputConfig,
    },
    
    /// Synchronize directories (bidirectional)
    Sync {
        /// First directory
        #[arg(short = 'a', long)]
        dir_a: PathBuf,
        
        /// Second directory
        #[arg(short = 'b', long)]
        dir_b: PathBuf,
        
        /// Conflict resolution strategy
        #[arg(long, default_value = "newer")]
        conflict: ConflictStrategy,
        
        #[command(flatten)]
        io: IoConfig,
        
        #[command(flatten)]
        metadata: MetadataConfig,
    },
    
    /// Verify copied files
    Verify {
        /// Source directory
        #[arg(short, long)]
        source: PathBuf,
        
        /// Destination directory
        #[arg(short, long)]
        destination: PathBuf,
        
        /// Verification method (checksum, metadata, etc.)
        #[arg(long, default_value = "checksum")]
        method: VerifyMethod,
    },
    
    /// Run performance benchmarks
    Benchmark {
        /// Test directory
        #[arg(short, long)]
        test_dir: PathBuf,
        
        /// Benchmark suite to run
        #[arg(long, default_value = "standard")]
        suite: BenchSuite,
        
        /// Output format (human, json, csv)
        #[arg(long, default_value = "human")]
        format: OutputFormat,
    },
}

#[derive(Debug, Clone, clap::ValueEnum)]
pub enum ConflictStrategy {
    Newer,
    Larger,
    Ask,
}

#[derive(Debug, Clone, clap::ValueEnum)]
pub enum VerifyMethod {
    Checksum,
    Metadata,
    Full,
}

#[derive(Debug, Clone, clap::ValueEnum)]
pub enum BenchSuite {
    Standard,
    Quick,
    Thorough,
}

#[derive(Debug, Clone, clap::ValueEnum)]
pub enum OutputFormat {
    Human,
    Json,
    Csv,
}
```

### Usage

```bash
# Copy operation
arsync copy --source /data --destination /backup --archive

# Sync operation
arsync sync --dir-a /data --dir-b /backup --conflict newer

# Verify
arsync verify --source /data --destination /backup --method checksum

# Benchmark
arsync benchmark --test-dir /tmp/bench --suite thorough --format json
```

---

## Pattern 3: Hybrid (Best of Both Worlds)

Combine flatten for shared options + subcommands for operations.

### Implementation

```rust
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Args {
    /// Global I/O configuration (applies to all commands)
    #[command(flatten)]
    pub global_io: GlobalIoConfig,
    
    /// Output configuration (applies to all commands)
    #[command(flatten)]
    pub output: OutputConfig,
    
    #[command(subcommand)]
    pub command: Commands,
}

/// Global I/O settings shared across all commands
#[derive(Parser, Debug, Clone)]
pub struct GlobalIoConfig {
    /// Number of CPU cores to use (0 = auto-detect)
    #[arg(long, default_value = "0", global = true)]
    pub cpu_count: usize,
    
    /// Queue depth for io_uring operations
    #[arg(long, default_value = "4096", global = true)]
    pub queue_depth: usize,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Copy files and directories
    Copy {
        #[arg(short, long)]
        source: PathBuf,
        
        #[arg(short, long)]
        destination: PathBuf,
        
        #[command(flatten)]
        metadata: MetadataConfig,
        
        /// Copy-specific options
        #[command(flatten)]
        copy_opts: CopyOptions,
    },
    
    /// Synchronize directories
    Sync {
        #[arg(short = 'a', long)]
        dir_a: PathBuf,
        
        #[arg(short = 'b', long)]
        dir_b: PathBuf,
        
        /// Sync-specific options
        #[command(flatten)]
        sync_opts: SyncOptions,
    },
}

#[derive(Parser, Debug, Clone)]
pub struct CopyOptions {
    /// Use copy-on-write when possible
    #[arg(long)]
    pub cow: bool,
    
    /// Overwrite existing files
    #[arg(long)]
    pub force: bool,
}

#[derive(Parser, Debug, Clone)]
pub struct SyncOptions {
    /// Perform bidirectional sync
    #[arg(long)]
    pub bidirectional: bool,
    
    /// Delete files that don't exist in source
    #[arg(long)]
    pub delete: bool,
}
```

### Usage

```bash
# Global flags apply to any command
arsync --cpu-count 16 --queue-depth 8192 copy --source /data --destination /backup

# Or with environment-specific subcommand options
arsync sync --dir-a /data --dir-b /backup --bidirectional --delete
```

---

## Real-World Examples

### Cargo's Architecture

```rust
// Simplified version of cargo's CLI
#[derive(Parser)]
pub struct Cargo {
    /// Global flags
    #[command(flatten)]
    pub global: GlobalArgs,
    
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Parser)]
pub struct GlobalArgs {
    #[arg(short, long, global = true)]
    pub verbose: bool,
    
    #[arg(short, long, global = true)]
    pub quiet: bool,
    
    #[arg(long, global = true)]
    pub color: Option<String>,
}

#[derive(Subcommand)]
pub enum Commands {
    Build {
        #[command(flatten)]
        compile_opts: CompileOptions,
    },
    Test {
        #[command(flatten)]
        compile_opts: CompileOptions,
        
        #[command(flatten)]
        test_opts: TestOptions,
    },
    // ... more commands
}

#[derive(Parser)]
pub struct CompileOptions {
    #[arg(long)]
    pub release: bool,
    
    #[arg(long)]
    pub target: Option<String>,
}
```

### ripgrep's Architecture

```rust
// Simplified ripgrep structure
#[derive(Parser)]
pub struct Args {
    /// Search pattern
    pub pattern: String,
    
    /// Paths to search
    pub paths: Vec<PathBuf>,
    
    /// Search options
    #[command(flatten)]
    pub search: SearchOptions,
    
    /// Output formatting
    #[command(flatten)]
    pub output: OutputOptions,
    
    /// Path filtering
    #[command(flatten)]
    pub filter: FilterOptions,
}

#[derive(Parser)]
pub struct SearchOptions {
    #[arg(short = 'i', long)]
    pub ignore_case: bool,
    
    #[arg(short = 'w', long)]
    pub word_regexp: bool,
    
    #[arg(short = 'F', long)]
    pub fixed_strings: bool,
}

#[derive(Parser)]
pub struct OutputOptions {
    #[arg(short = 'c', long)]
    pub count: bool,
    
    #[arg(long)]
    pub json: bool,
    
    #[arg(long)]
    pub no_heading: bool,
}
```

---

## Testing Patterns

### Unit Testing Individual Modules

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_io_config_validation() {
        let config = IoConfig {
            queue_depth: 500,  // Too low
            max_files_in_flight: 1024,
            cpu_count: 4,
            buffer_size_kb: 64,
            copy_method: CopyMethod::Auto,
            no_adaptive_concurrency: false,
        };
        
        assert!(config.validate().is_err());
    }
    
    #[test]
    fn test_metadata_helpers() {
        let config = MetadataConfig {
            archive: true,
            perms: false,
            times: false,
            // ... other fields false
        };
        
        // Archive mode should preserve permissions
        assert!(config.should_preserve_permissions());
        assert!(config.should_preserve_timestamps());
        assert!(config.should_recurse());
    }
}
```

### Integration Testing

```rust
#[test]
fn test_full_cli() {
    let args = Args::parse_from([
        "arsync",
        "--source", "/tmp/src",
        "--destination", "/tmp/dst",
        "--archive",
        "--queue-depth", "8192",
        "--verbose",
    ]);
    
    assert_eq!(args.io.queue_depth, 8192);
    assert!(args.metadata.archive);
    assert_eq!(args.output.verbose, 1);
}
```

---

## Migration Checklist

If migrating from monolithic to modular:

- [ ] Create `src/cli/` directory
- [ ] Create module files (io_config.rs, metadata.rs, output.rs)
- [ ] Move structs to respective modules
- [ ] Add `#[command(flatten)]` in main Args
- [ ] Move validation logic to module impls
- [ ] Update imports in main.rs and lib.rs
- [ ] Run `cargo test` to verify
- [ ] Update documentation
- [ ] Check `--help` output is unchanged
- [ ] Remove old cli.rs

---

## Conclusion

**For arsync, I recommend Pattern 1 (Flatten) immediately:**

1. **Now**: Extract I/O, Metadata, and Output into separate modules
2. **Later**: If operations grow (sync, verify, benchmark), add subcommands (Pattern 2)
3. **Future**: Hybrid approach for maximum flexibility (Pattern 3)

The modular pattern is how modern Rust CLIs are built. It's the standard in the ecosystem and will make arsync much easier to maintain and extend.

