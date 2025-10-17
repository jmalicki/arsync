//! Remote synchronization protocol foundation
//!
//! This module provides the foundational types for remote sync:
//! - `Location` enum for parsing local/remote paths
//! - `PipeRole` enum for sender/receiver roles
//! - `Transport` trait for bidirectional byte streams
//! - `PipeTransport` for testing

use anyhow::Result;
use std::path::PathBuf;

// Protocol implementation modules (only available with remote-sync feature)
#[cfg(feature = "remote-sync")]
pub mod checksum;
#[cfg(feature = "remote-sync")]
pub mod handshake;
#[cfg(feature = "remote-sync")]
pub mod pipe;
#[cfg(feature = "remote-sync")]
pub mod ssh;
#[cfg(feature = "remote-sync")]
pub mod transport;
#[cfg(feature = "remote-sync")]
pub mod varint;

/// Parsed location (local or remote)
#[derive(Debug, Clone)]
pub enum Location {
    /// Local filesystem path
    Local(PathBuf),
    /// Remote path accessed via SSH
    Remote {
        /// Remote username (None = current user)
        user: Option<String>,
        /// Remote hostname or IP address
        host: String,
        /// Remote filesystem path
        path: PathBuf,
    },
}

impl Location {
    /// Parse rsync-style path: `[user@]host:path` or `/local/path`
    ///
    /// # Errors
    ///
    /// Returns an error if the path string is invalid (reserved for future validation)
    #[allow(clippy::unnecessary_wraps)] // May add validation in future
    pub fn parse(s: &str) -> Result<Self> {
        // Check for remote syntax: [user@]host:path
        if let Some(colon_pos) = s.find(':') {
            // Could be remote or Windows path (C:\...)
            // Windows paths have letter:\ pattern
            if colon_pos == 1 && s.chars().nth(0).is_some_and(|c| c.is_ascii_alphabetic()) {
                // Likely Windows path
                return Ok(Self::Local(PathBuf::from(s)));
            }

            let host_part = &s[..colon_pos];
            let path_part = &s[colon_pos + 1..];

            // Parse user@host or just host
            let (user, host) = host_part.find('@').map_or_else(
                || (None, host_part.to_string()),
                |at_pos| {
                    (
                        Some(host_part[..at_pos].to_string()),
                        host_part[at_pos + 1..].to_string(),
                    )
                },
            );

            Ok(Self::Remote {
                user,
                host,
                path: PathBuf::from(path_part),
            })
        } else {
            // Local path
            Ok(Self::Local(PathBuf::from(s)))
        }
    }

    /// Get the path component
    #[must_use]
    #[allow(dead_code)] // Will be used by protocol implementation
    pub const fn path(&self) -> &PathBuf {
        match self {
            Self::Local(path) | Self::Remote { path, .. } => path,
        }
    }

    /// Check if this is a remote location
    #[must_use]
    pub const fn is_remote(&self) -> bool {
        matches!(self, Self::Remote { .. })
    }

    /// Check if this is a local location
    #[must_use]
    pub const fn is_local(&self) -> bool {
        matches!(self, Self::Local(_))
    }
}

/// Role in pipe-based protocol testing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipeRole {
    /// Send data (like rsync client)
    Sender,
    /// Receive data (like rsync server)
    Receiver,
}
