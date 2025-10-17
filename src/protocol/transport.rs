//! Generic transport abstraction for rsync protocol
//!
//! The rsync wire protocol is transport-agnostic - it works over any
//! bidirectional byte stream (pipes, TCP, SSH, QUIC, etc.)
//!
//! This module uses **compio** for async I/O with `io_uring` backend.
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
    fn name(&self) -> &'static str {
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
                format!("Unexpected EOF while reading {len} bytes (got {offset})"),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::pipe::PipeTransport;

    #[compio::test]
    async fn test_read_exact_success() {
        // Test: read_exact should read exact number of bytes
        // Requirement: read_exact helper should fill buffer completely
        let (read_fd, write_fd) = PipeTransport::create_pipe().unwrap();

        let read_fd_dup = unsafe { libc::dup(read_fd) };
        let write_fd_dup = unsafe { libc::dup(write_fd) };

        let mut reader =
            unsafe { PipeTransport::from_fds(read_fd, read_fd_dup, "reader".to_string()).unwrap() };
        let mut writer = unsafe {
            PipeTransport::from_fds(write_fd_dup, write_fd, "writer".to_string()).unwrap()
        };

        // Write test data
        let test_data = b"Hello, World! This is a test.";
        write_all(&mut writer, test_data).await.unwrap();

        // Read exact number of bytes
        let mut buf = vec![0u8; test_data.len()];
        read_exact(&mut reader, &mut buf).await.unwrap();

        assert_eq!(&buf, test_data);
    }

    #[compio::test]
    async fn test_read_exact_partial_reads() {
        // Test: read_exact should handle partial reads correctly
        // Requirement: read_exact should keep reading until buffer is full
        let (read_fd, write_fd) = PipeTransport::create_pipe().unwrap();

        let read_fd_dup = unsafe { libc::dup(read_fd) };
        let write_fd_dup = unsafe { libc::dup(write_fd) };

        let mut reader =
            unsafe { PipeTransport::from_fds(read_fd, read_fd_dup, "reader".to_string()).unwrap() };
        let mut writer = unsafe {
            PipeTransport::from_fds(write_fd_dup, write_fd, "writer".to_string()).unwrap()
        };

        // Write 1KB of data
        let test_data: Vec<u8> = (0..1024).map(|i| (i % 256) as u8).collect();
        write_all(&mut writer, &test_data).await.unwrap();

        // Read in smaller buffer (tests multiple read calls)
        let mut buf = vec![0u8; 1024];
        read_exact(&mut reader, &mut buf).await.unwrap();

        assert_eq!(buf, test_data);
    }

    #[compio::test]
    async fn test_read_exact_eof_error() {
        // Test: read_exact should error on unexpected EOF
        // Requirement: read_exact should return UnexpectedEof if stream ends early
        let (read_fd, write_fd) = PipeTransport::create_pipe().unwrap();

        let read_fd_dup = unsafe { libc::dup(read_fd) };
        let write_fd_dup = unsafe { libc::dup(write_fd) };

        let mut reader =
            unsafe { PipeTransport::from_fds(read_fd, read_fd_dup, "reader".to_string()).unwrap() };
        let mut writer = unsafe {
            PipeTransport::from_fds(write_fd_dup, write_fd, "writer".to_string()).unwrap()
        };

        // Write only 5 bytes
        write_all(&mut writer, b"short").await.unwrap();

        // Close writer to trigger EOF
        drop(writer);

        // Try to read 100 bytes (should fail with EOF)
        let mut buf = vec![0u8; 100];
        let result = read_exact(&mut reader, &mut buf).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
    }

    #[compio::test]
    async fn test_write_all_success() {
        // Test: write_all should write all bytes
        // Requirement: write_all helper should write complete buffer
        let (read_fd, write_fd) = PipeTransport::create_pipe().unwrap();

        let read_fd_dup = unsafe { libc::dup(read_fd) };
        let write_fd_dup = unsafe { libc::dup(write_fd) };

        let mut reader =
            unsafe { PipeTransport::from_fds(read_fd, read_fd_dup, "reader".to_string()).unwrap() };
        let mut writer = unsafe {
            PipeTransport::from_fds(write_fd_dup, write_fd, "writer".to_string()).unwrap()
        };

        // Write data
        let test_data = b"Testing write_all function";
        write_all(&mut writer, test_data).await.unwrap();

        // Read back
        let mut buf = vec![0u8; test_data.len()];
        read_exact(&mut reader, &mut buf).await.unwrap();

        assert_eq!(&buf, test_data);
    }

    #[compio::test]
    async fn test_write_all_large_data() {
        // Test: write_all should handle large writes
        // Requirement: write_all should work with large buffers
        let (read_fd, write_fd) = PipeTransport::create_pipe().unwrap();

        let read_fd_dup = unsafe { libc::dup(read_fd) };
        let write_fd_dup = unsafe { libc::dup(write_fd) };

        let mut reader =
            unsafe { PipeTransport::from_fds(read_fd, read_fd_dup, "reader".to_string()).unwrap() };
        let mut writer = unsafe {
            PipeTransport::from_fds(write_fd_dup, write_fd, "writer".to_string()).unwrap()
        };

        // Write 64KB
        let test_data: Vec<u8> = (0..65536).map(|i| (i % 256) as u8).collect();
        write_all(&mut writer, &test_data).await.unwrap();

        // Read back
        let mut buf = vec![0u8; test_data.len()];
        read_exact(&mut reader, &mut buf).await.unwrap();

        assert_eq!(buf, test_data);
    }

    #[compio::test]
    async fn test_transport_trait_properties() {
        // Test: Transport implementations should have correct trait methods
        // Requirement: Transport trait should be implemented with name and multiplexing
        let (read_fd, write_fd) = PipeTransport::create_pipe().unwrap();
        let transport =
            unsafe { PipeTransport::from_fds(read_fd, write_fd, "test".to_string()).unwrap() };

        // Test trait methods
        assert_eq!(transport.name(), "pipe");
        assert!(!transport.supports_multiplexing());
    }

    #[compio::test]
    async fn test_read_write_roundtrip() {
        // Test: Full roundtrip with read_exact and write_all
        // Requirement: Helpers should work together for complete I/O operations
        let (read_fd, write_fd) = PipeTransport::create_pipe().unwrap();

        let read_fd_dup = unsafe { libc::dup(read_fd) };
        let write_fd_dup = unsafe { libc::dup(write_fd) };

        let mut reader =
            unsafe { PipeTransport::from_fds(read_fd, read_fd_dup, "reader".to_string()).unwrap() };
        let mut writer = unsafe {
            PipeTransport::from_fds(write_fd_dup, write_fd, "writer".to_string()).unwrap()
        };

        // Multiple roundtrips
        let messages = [
            b"First message".as_slice(),
            b"Second message with more data".as_slice(),
            b"Third".as_slice(),
        ];

        for msg in messages {
            // Write
            write_all(&mut writer, msg).await.unwrap();

            // Read
            let mut buf = vec![0u8; msg.len()];
            read_exact(&mut reader, &mut buf).await.unwrap();

            assert_eq!(&buf, msg);
        }
    }
}
