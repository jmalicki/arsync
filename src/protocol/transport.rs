//! Generic transport abstraction for rsync protocol
//!
//! The rsync wire protocol is transport-agnostic - it works over any
//! bidirectional byte stream (pipes, TCP, SSH, QUIC, etc.)
//!
//! This module uses **compio** for async I/O with io_uring backend.
//! All transport implementations must provide `AsyncRead + AsyncWrite` from compio.
//!
//! # Architecture
//!
//! ```text
//! Transport Trait
//!     ↓
//! compio::io::AsyncRead + AsyncWrite
//!     ↓
//! io_uring Operations
//! ```
//!
//! # Example
//!
//! ```rust,ignore
//! use arsync::protocol::transport::Transport;
//! use compio::io::{AsyncReadExt, AsyncWriteExt};
//!
//! async fn example<T: Transport>(mut transport: T) -> std::io::Result<()> {
//!     let mut buf = vec![0u8; 1024];
//!     let n = transport.read(&mut buf).await?;
//!     transport.write_all(b"Hello").await?;
//!     Ok(())
//! }
//! ```
#![allow(dead_code)] // Protocol implementation not yet fully used

use compio::io::{AsyncRead, AsyncWrite};
use std::io;

/// Generic transport for rsync protocol
///
/// A transport represents a bidirectional byte stream that carries rsync protocol
/// messages. This is a marker trait that requires `compio::io::AsyncRead` and
/// `compio::io::AsyncWrite`, which provide io_uring-based async I/O.
///
/// # Requirements
///
/// - Must implement `compio::io::AsyncRead` for receiving data
/// - Must implement `compio::io::AsyncWrite` for sending data
/// - Must be `Send` for use across threads
/// - Must be `Unpin` for safe async operations
///
/// # Implementations
///
/// - `PipeTransport` - For testing via stdin/stdout or Unix pipes
/// - `SshConnection` - For production remote sync over SSH
/// - `TcpStream` - For direct network connections (future)
/// - `QuicConnection` - For QUIC-based transport (future)
///
/// # Example Implementation
///
/// ```rust,ignore
/// // Example: Implementing Transport for a custom type
/// use arsync::protocol::transport::Transport;
/// use compio::io::{AsyncRead, AsyncWrite};
///
/// struct MyTransport {
///     // ... fields ...
/// }
///
/// // Implement AsyncRead and AsyncWrite for MyTransport
/// // ... implementations ...
///
/// // Then MyTransport automatically implements Transport
/// impl Transport for MyTransport {
///     fn name(&self) -> &str { "my-custom-transport" }
/// }
/// ```
pub trait Transport: AsyncRead + AsyncWrite + Send + Unpin {
    /// Get transport name for debugging
    ///
    /// Used in log messages to identify which transport is being used.
    fn name(&self) -> &str {
        "unknown"
    }

    /// Check if transport supports multiplexing (multiple parallel streams)
    ///
    /// Returns `true` for transports like QUIC or HTTP/2 that can multiplex.
    /// Returns `false` for simple streams like pipes or SSH.
    fn supports_multiplexing(&self) -> bool {
        false
    }
}

/// Helper to read exact number of bytes
///
/// Reads bytes using compio's buffer ownership model, copying into the provided buffer.
///
/// # Errors
///
/// Returns an error if:
/// - Transport read fails
/// - EOF is reached before buffer is full
///
/// # Example
///
/// ```rust,ignore
/// use arsync::protocol::transport::{Transport, read_exact};
///
/// async fn example<T: Transport>(mut transport: T) -> std::io::Result<()> {
///     let mut buf = [0u8; 100];
///     read_exact(&mut transport, &mut buf).await?;
///     Ok(())
/// }
/// ```
pub async fn read_exact<T>(transport: &mut T, buf: &mut [u8]) -> io::Result<()>
where
    T: AsyncRead + Unpin,
{
    let len = buf.len();
    let mut owned = vec![0u8; len];

    let mut offset = 0;
    while offset < len {
        // Read into owned buffer (compio takes ownership)
        let buf_result = transport.read(owned).await;
        let n = buf_result.0?; // Result
        let returned_buf = buf_result.1; // Buffer

        if n == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!(
                    "Unexpected EOF while reading {} bytes (got {})",
                    len, offset
                ),
            ));
        }

        // Copy from owned buffer to user's buffer
        buf[offset..offset + n].copy_from_slice(&returned_buf[..n]);
        offset += n;

        // Reuse buffer for next iteration
        owned = returned_buf;
    }

    Ok(())
}

/// Helper to write all bytes
///
/// Writes bytes to transport using compio's buffer ownership model, then flushes.
///
/// # Errors
///
/// Returns an error if transport write or flush fails.
///
/// # Example
///
/// ```rust,ignore
/// use arsync::protocol::transport::{Transport, write_all};
///
/// async fn example<T: Transport>(mut transport: T) -> std::io::Result<()> {
///     write_all(&mut transport, b"Hello, World!").await?;
///     Ok(())
/// }
/// ```
pub async fn write_all<T>(transport: &mut T, buf: &[u8]) -> io::Result<()>
where
    T: AsyncWrite + Unpin,
{
    use compio::io::AsyncWriteExt;

    // Convert to owned buffer for compio
    let owned_buf = buf.to_vec();

    // write_all returns BufResult<(), B>
    let buf_result = transport.write_all(owned_buf).await;
    buf_result.0?; // Check result

    transport.flush().await?;
    Ok(())
}
