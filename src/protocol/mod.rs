//! Remote synchronization protocol implementation
//!
//! This module implements the rsync wire protocol for compatibility with
//! existing rsync servers, as well as modern extensions using QUIC and
//! merkle trees.

// Protocol types are always available for CLI parsing
use anyhow::Result;
use std::path::PathBuf;

// Protocol implementation modules are only available with remote-sync feature
#[cfg(feature = "remote-sync")]
pub mod checksum;
#[cfg(feature = "remote-sync")]
pub mod handshake;
#[cfg(feature = "remote-sync")]
pub mod pipe;
#[cfg(feature = "remote-sync")]
pub mod rsync;
#[cfg(feature = "remote-sync")]
pub mod rsync_compat;
#[cfg(feature = "remote-sync")]
pub mod ssh;
#[cfg(feature = "remote-sync")]
pub mod transport;
#[cfg(feature = "remote-sync")]
pub mod varint;

#[cfg(feature = "quic")]
pub mod quic;

#[cfg(feature = "remote-sync")]
use crate::cli::Args;
#[cfg(feature = "remote-sync")]
use crate::sync::SyncStats;

/// Parsed location (local or remote)
#[derive(Debug, Clone)]
pub enum Location {
    Local(PathBuf),
    Remote {
        user: Option<String>,
        host: String,
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
    #[allow(dead_code)] // Will be used by protocol implementation
    pub const fn is_local(&self) -> bool {
        matches!(self, Self::Local(_))
    }
}

/// Role in pipe mode
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum PipeRole {
    /// Sender: read files and send via protocol
    Sender,
    /// Receiver: receive via protocol and write files
    Receiver,
}

/// Main entry point for remote sync operations
#[cfg(feature = "remote-sync")]
pub async fn remote_sync(
    args: &Args,
    source: &Location,
    destination: &Location,
) -> Result<SyncStats> {
    // Determine sync direction
    match (source, destination) {
        (Location::Local(src), Location::Remote { user, host, path }) => {
            // Push: local → remote
            push_to_remote(args, src, user.as_deref(), host, path).await
        }
        (Location::Remote { user, host, path }, Location::Local(dest)) => {
            // Pull: remote → local
            pull_from_remote(args, user.as_deref(), host, path, dest).await
        }
        (Location::Remote { .. }, Location::Remote { .. }) => {
            anyhow::bail!("Remote-to-remote sync not supported yet")
        }
        (Location::Local(_), Location::Local(_)) => {
            unreachable!("Local-to-local should have been handled by sync_files")
        }
    }
}

/// Push files from local to remote
#[cfg(feature = "remote-sync")]
async fn push_to_remote(
    args: &Args,
    local_path: &std::path::Path,
    user: Option<&str>,
    host: &str,
    _remote_path: &std::path::Path,
) -> Result<SyncStats> {
    // Connect to remote via SSH
    let username = user.map(String::from).unwrap_or_else(whoami::username);
    let connection =
        ssh::SshConnection::connect(host, &username, &args.remote.remote_shell).await?;

    // Start remote arsync in server mode
    // connection.start_server(remote_path).await?;  // TODO: May not be needed with rsync protocol

    // Try to negotiate QUIC if supported
    #[cfg(feature = "quic")]
    {
        // Note: QUIC negotiation would need to clone/recreate connection
        // For now, skip QUIC and go directly to rsync protocol
        // if let Ok(quic_conn) = quic::negotiate_quic(&mut connection).await {
        //     return quic::push_via_quic(args, local_path, quic_conn).await;
        // }
    }

    // Fall back to rsync wire protocol over SSH
    rsync::push_via_rsync_protocol(args, local_path, connection).await
}

/// Pull files from remote to local
#[cfg(feature = "remote-sync")]
async fn pull_from_remote(
    args: &Args,
    user: Option<&str>,
    host: &str,
    _remote_path: &std::path::Path,
    local_path: &std::path::Path,
) -> Result<SyncStats> {
    // Connect to remote via SSH
    let username = user.map(String::from).unwrap_or_else(whoami::username);
    let connection =
        ssh::SshConnection::connect(host, &username, &args.remote.remote_shell).await?;

    // Start remote arsync in server mode
    // connection.start_server(remote_path).await?;  // TODO: May not be needed with rsync protocol

    // Try to negotiate QUIC if supported
    #[cfg(feature = "quic")]
    {
        // Note: QUIC negotiation would need to clone/recreate connection
        // For now, skip QUIC and go directly to rsync protocol
        // if let Ok(quic_conn) = quic::negotiate_quic(&mut connection).await {
        //     return quic::pull_via_quic(args, quic_conn, local_path).await;
        // }
    }

    // Fall back to rsync wire protocol over SSH
    rsync::pull_via_rsync_protocol(args, connection, local_path).await
}

/// Pipe sender mode (for protocol testing)
#[cfg(feature = "remote-sync")]
pub async fn pipe_sender(args: &Args, source: &Location) -> Result<SyncStats> {
    let source_path = match source {
        Location::Local(path) => path,
        Location::Remote { .. } => {
            anyhow::bail!("Pipe mode requires local source path");
        }
    };

    // Create pipe transport from stdin/stdout
    let transport = pipe::PipeTransport::from_stdio()?;

    // Choose protocol based on --rsync-compat flag
    if args.remote.rsync_compat {
        // Use rsync wire protocol
        rsync_compat::rsync_send(args, source_path, transport).await
    } else {
        // Use arsync native protocol
        rsync::send_via_pipe(args, source_path, transport).await
    }
}

/// Pipe receiver mode (for protocol testing)
#[cfg(feature = "remote-sync")]
pub async fn pipe_receiver(args: &Args, destination: &Location) -> Result<SyncStats> {
    let dest_path = match destination {
        Location::Local(path) => path,
        Location::Remote { .. } => {
            anyhow::bail!("Pipe mode requires local destination path");
        }
    };

    // Create pipe transport from stdin/stdout
    let transport = pipe::PipeTransport::from_stdio()?;

    // Choose protocol based on --rsync-compat flag
    if args.remote.rsync_compat {
        // Use rsync wire protocol
        rsync_compat::rsync_receive(args, transport, dest_path).await
    } else {
        // Use arsync native protocol
        rsync::receive_via_pipe(args, transport, dest_path).await
    }
}
