//! arsync: High-performance bulk file copying utility
//!
//! This utility provides rsync-like functionality optimized for single-machine operations
//! using `io_uring` for maximum performance and parallelism.

use anyhow::{Context, Result};
use clap::Parser;
use tracing::{info, Level};

mod adaptive_concurrency;
mod cli;
mod copy;
mod copy_trait;
mod directory;
mod error;
mod file_wrapper;
mod filesystem;
mod hardlink_tracker;
mod i18n;
mod io_uring;
mod metadata;
mod progress;
mod protocol;
mod stats;
mod sync;
mod traits;

use cli::Args;
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
    if args.quiet() {
        // In quiet mode, only log errors
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(Level::ERROR)
            .with_target(false)
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

    // Log startup information (unless in quiet mode)
    if !args.quiet() {
        info!(
            "{}: arsync v{}",
            TranslationKey::InfoStartingCopy
                .get()
                .unwrap_or_else(|_| "Starting copy".to_string()),
            env!("CARGO_PKG_VERSION")
        );
        info!(
            "{}: {}",
            TranslationKey::HelpSource
                .get()
                .unwrap_or_else(|_| "Source".to_string()),
            args.source().display()
        );
        info!(
            "{}: {}",
            TranslationKey::HelpDestination
                .get()
                .unwrap_or_else(|_| "Destination".to_string()),
            args.destination().display()
        );
        info!("Copy method: {:?}", args.copy_method());
        info!("Queue depth: {}", args.queue_depth());
        info!("CPU count: {}", args.effective_cpu_count());
        info!(
            "Buffer size: {} KB",
            args.io
                .buffer_size_kb
                .map_or_else(|| "auto".to_string(), |kb| kb.to_string())
        );
        info!("Max files in flight: {}", args.max_files_in_flight());
    }

    // Validate arguments
    args.validate().context("Invalid arguments")?;

    // Perform the sync operation
    let result = sync::sync_files(&args).await;

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
