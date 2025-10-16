//! arsync: High-performance bulk file copying utility
//!
//! This utility provides rsync-like functionality optimized for single-machine operations
//! using `io_uring` for maximum performance and parallelism.

use anyhow::{Context, Result};
use clap::Parser;
#[allow(unused_imports)]
use tracing::warn;
use tracing::{info, Level};

mod adaptive_concurrency;
mod cli;
mod copy;
mod directory;
mod error;
mod i18n;
mod io_uring;
mod metadata;
mod progress;
mod protocol;
mod sync;

use cli::{Args, Location};
use i18n::{set_language, Language, TranslationKey};

#[compio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let args = Args::parse();

    // Set language based on --pirate flag
    if args.output.pirate {
        set_language(Language::Pirate);
    }

    // Initialize logging based on verbosity and quiet mode
    // Note: In pipe mode, logs go to stderr (FD 2) so they don't interfere with protocol (FD 0/1)
    if args.remote.pipe || args.quiet() {
        // In quiet or pipe mode, only log errors (to stderr)
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(Level::ERROR)
            .with_target(false)
            .with_writer(std::io::stderr) // Explicit stderr
            .finish();
        tracing::subscriber::set_global_default(subscriber)?;
    } else {
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(match args.verbose() {
                0 => Level::WARN,
                1 => Level::INFO,
                2 => Level::DEBUG,
                _ => Level::TRACE,
            })
            .with_target(false)
            .with_thread_ids(false)
            .with_thread_names(false)
            .finish();
        tracing::subscriber::set_global_default(subscriber)?;
    }

    // Get source and destination
    let source = args.get_source().context("Failed to parse source")?;
    let destination = args
        .get_destination()
        .context("Failed to parse destination")?;

    // Log startup information (unless in quiet mode)
    if !args.quiet() {
        info!("Starting arsync v{}", env!("CARGO_PKG_VERSION"));

        // Show SIMD capabilities for checksums (verbose mode only)
        if args.verbose() > 0 {
            #[cfg(target_arch = "x86_64")]
            {
                if is_x86_feature_detected!("avx512f") {
                    info!("Checksum acceleration: AVX512 (5x speedup)");
                } else if is_x86_feature_detected!("avx2") {
                    info!("Checksum acceleration: AVX2 (3x speedup)");
                } else if is_x86_feature_detected!("ssse3") {
                    info!("Checksum acceleration: SSSE3 (2x speedup)");
                } else {
                    info!("Checksum acceleration: scalar (no SIMD)");
                }
            }
            #[cfg(not(target_arch = "x86_64"))]
            {
                info!("Checksum acceleration: scalar (x86_64 only)");
            }
        }

        match &source {
            Location::Local(path) => info!("Source: {} (local)", path.display()),
            Location::Remote { user, host, path } => {
                if let Some(u) = user {
                    info!("Source: {}@{}:{} (remote)", u, host, path.display());
                } else {
                    info!("Source: {}:{} (remote)", host, path.display());
                }
            }
        }
        match &destination {
            Location::Local(path) => info!("Destination: {} (local)", path.display()),
            Location::Remote { user, host, path } => {
                if let Some(u) = user {
                    info!("Destination: {}@{}:{} (remote)", u, host, path.display());
                } else {
                    info!("Destination: {}:{} (remote)", host, path.display());
                }
            }
        }
        info!("Copy method: {:?}", args.copy_method());
        info!("Queue depth: {}", args.queue_depth());
        info!("CPU count: {}", args.effective_cpu_count());
        info!("Buffer size: {} KB", args.io.buffer_size_kb);
        info!("Max files in flight: {}", args.max_files_in_flight());
    }

    // Validate arguments
    args.validate().context("Invalid arguments")?;

    // Route to appropriate mode
    let result = if args.remote.pipe {
        // ============================================================
        // PIPE MODE (TESTING ONLY)
        // ============================================================
        // Uses rsync wire protocol over stdin/stdout
        // FOR PROTOCOL TESTING, NOT PRODUCTION USE
        // Local copies should use io_uring direct operations!
        // ============================================================
        #[cfg(feature = "remote-sync")]
        {
            match args.remote.pipe_role {
                Some(cli::PipeRole::Sender) => {
                    info!("Pipe mode: sender");
                    protocol::pipe_sender(&args, &source).await
                }
                Some(cli::PipeRole::Receiver) => {
                    info!("Pipe mode: receiver");
                    protocol::pipe_receiver(&args, &destination).await
                }
                None => {
                    anyhow::bail!("--pipe requires --pipe-role (sender or receiver)")
                }
            }
        }
        #[cfg(not(feature = "remote-sync"))]
        {
            anyhow::bail!(
                "--pipe requires remote-sync feature (compile with --features remote-sync)"
            )
        }
    } else if source.is_remote() || destination.is_remote() {
        // Remote sync mode
        #[cfg(feature = "remote-sync")]
        {
            protocol::remote_sync(&args, &source, &destination).await
        }
        #[cfg(not(feature = "remote-sync"))]
        {
            warn!("Remote sync support not compiled in");
            warn!("To enable remote sync, compile with: cargo build --features remote-sync");
            Err(anyhow::anyhow!("Remote sync not supported in this build"))
        }
    } else {
        // Local sync mode
        sync::sync_files(&args).await.map_err(Into::into)
    };

    match result {
        Ok(stats) => {
            info!(
                "{}",
                TranslationKey::StatusComplete
                    .get()
                    .unwrap_or_else(|_| "Complete".to_string())
            );
            info!(
                "{} {}: {}",
                TranslationKey::ProgressFiles
                    .get()
                    .unwrap_or_else(|_| "Files".to_string()),
                TranslationKey::ProgressCompleted
                    .get()
                    .unwrap_or_else(|_| "Completed".to_string()),
                stats.files_copied
            );
            info!(
                "{} {}: {}",
                TranslationKey::ProgressBytes
                    .get()
                    .unwrap_or_else(|_| "Bytes".to_string()),
                TranslationKey::ProgressCompleted
                    .get()
                    .unwrap_or_else(|_| "Completed".to_string()),
                stats.bytes_copied
            );
            info!("Duration: {:?}", stats.duration);
            Ok(())
        }
        Err(e) => {
            eprintln!(
                "{}: {e}",
                TranslationKey::StatusFailed
                    .get()
                    .unwrap_or_else(|_| "Failed".to_string())
            );
            std::process::exit(1);
        }
    }
}
