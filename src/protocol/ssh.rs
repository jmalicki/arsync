//! SSH connection management for remote sync
//!
//! Handles SSH connections using compio::process for io_uring-based async I/O.
//! The SSH process stdin/stdout are wrapped as async streams with io_uring backend.
//!
//! # Architecture
//!
//! ```text
//! SshConnection
//!     ↓
//! compio::process::Command
//!     ↓
//! compio::process::Child{Stdin,Stdout}
//!     ↓
//! compio AsyncRead/AsyncWrite
//!     ↓
//! io_uring operations
//! ```
#![allow(dead_code)] // Protocol implementation not yet fully used

use super::transport::Transport;
use anyhow::Result;
use compio::io::{AsyncRead, AsyncWrite};
use compio::process::{Child, ChildStdin, ChildStdout, Command};
use std::path::Path;
use std::process::Stdio;

/// SSH connection to remote host
///
/// Uses compio::process for async process management with io_uring backend.
pub struct SshConnection {
    /// SSH process
    #[allow(dead_code)]
    process: Child,
    /// stdin pipe to remote arsync
    stdin: ChildStdin,
    /// stdout pipe from remote arsync
    stdout: ChildStdout,
    /// Remote host
    #[allow(dead_code)]
    host: String,
    /// Remote user
    #[allow(dead_code)]
    user: String,
}

impl SshConnection {
    /// Connect to remote host via SSH
    ///
    /// Spawns an SSH process connecting to the remote host and starting arsync in server mode.
    ///
    /// # Arguments
    ///
    /// * `host` - Remote hostname or IP
    /// * `user` - Remote username  
    /// * `remote_shell` - Shell command to use (typically "ssh")
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - SSH process fails to spawn
    /// - Cannot get stdin/stdout from process
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use arsync::protocol::ssh::SshConnection;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let conn = SshConnection::connect("example.com", "user", "ssh").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn connect(host: &str, user: &str, remote_shell: &str) -> Result<Self> {
        // Build SSH command
        let mut cmd = Command::new(remote_shell);
        cmd.arg(format!("{}@{}", user, host))
            .arg("--") // Separator for SSH args vs remote command
            .arg("arsync")
            .arg("--server");

        // Configure stdio (compio methods return Result)
        cmd.stdin(Stdio::piped())
            .map_err(|_| anyhow::anyhow!("Failed to configure stdin"))?;
        cmd.stdout(Stdio::piped())
            .map_err(|_| anyhow::anyhow!("Failed to configure stdout"))?;
        cmd.stderr(Stdio::inherit())
            .map_err(|_| anyhow::anyhow!("Failed to configure stderr"))?;

        // Spawn SSH process (uses compio, will use io_uring for I/O)
        let mut process = cmd.spawn()?;

        let stdin = process
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to get stdin from SSH process"))?;
        let stdout = process
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to get stdout from SSH process"))?;

        Ok(Self {
            process,
            stdin,
            stdout,
            host: host.to_string(),
            user: user.to_string(),
        })
    }

    /// Start remote server (send initial protocol negotiation)
    pub async fn start_server(&mut self, _path: &Path) -> Result<()> {
        // For now, just verify connection is alive
        // TODO: Implement protocol negotiation
        Ok(())
    }
}

// ============================================================================
// compio AsyncRead Implementation
// ============================================================================

impl AsyncRead for SshConnection {
    async fn read<B: compio::buf::IoBufMut>(&mut self, buf: B) -> compio::buf::BufResult<usize, B> {
        // Delegate to stdout
        self.stdout.read(buf).await
    }
}

// ============================================================================
// compio AsyncWrite Implementation
// ============================================================================

impl AsyncWrite for SshConnection {
    async fn write<B: compio::buf::IoBuf>(&mut self, buf: B) -> compio::buf::BufResult<usize, B> {
        // Delegate to stdin
        self.stdin.write(buf).await
    }

    async fn flush(&mut self) -> std::io::Result<()> {
        self.stdin.flush().await
    }

    async fn shutdown(&mut self) -> std::io::Result<()> {
        self.stdin.shutdown().await
    }
}

// ============================================================================
// Transport Marker Implementation
// ============================================================================

impl Transport for SshConnection {
    fn name(&self) -> &str {
        "ssh"
    }

    fn supports_multiplexing(&self) -> bool {
        false // SSH is a simple stream
    }
}
