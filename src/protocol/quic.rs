//! QUIC-based remote sync (SSH-QUIC hybrid protocol)
//!
//! Implements the SSH-QUIC hybrid protocol where SSH provides authentication
//! and control channel, while QUIC provides high-performance parallel data transfer.
#![allow(dead_code)] // Protocol implementation not yet fully used
#![allow(clippy::missing_errors_doc)] // Protocol spec - errors documented at module level
#![allow(clippy::missing_panics_doc)] // Protocol spec - panics are bugs

use crate::cli::Args;
use crate::protocol::ssh::SshConnection;
use crate::sync::SyncStats;
use anyhow::Result;
use std::path::Path;

/// QUIC connection for file transfer
pub struct QuicConnection {
    // TODO: Implement with quinn
}

/// Negotiate QUIC capability with remote server via SSH
pub async fn negotiate_quic(_ssh: &mut SshConnection) -> Result<QuicConnection> {
    // TODO: Implement QUIC negotiation
    // 1. Send capability request via SSH
    // 2. Receive QUIC port and PSK via SSH
    // 3. Establish QUIC connection using PSK
    anyhow::bail!("QUIC negotiation not implemented yet")
}

/// Push files via QUIC
pub async fn push_via_quic(
    _args: &Args,
    _local_path: &Path,
    _quic: QuicConnection,
) -> Result<SyncStats> {
    // TODO: Implement QUIC push
    // 1. Open parallel QUIC streams (1000+)
    // 2. Transfer files concurrently
    // 3. Use merkle trees for optimization
    anyhow::bail!("QUIC push not implemented yet")
}

/// Pull files via QUIC
pub async fn pull_via_quic(
    _args: &Args,
    _quic: QuicConnection,
    _local_path: &Path,
) -> Result<SyncStats> {
    // TODO: Implement QUIC pull
    anyhow::bail!("QUIC pull not implemented yet")
}
