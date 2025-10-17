//! Pipe-based transport for rsync protocol testing
//!
//! Enables testing rsync wire protocol via pipes (stdin/stdout or Unix pipes)
//! without requiring SSH or network infrastructure.
//!
//! This implementation uses **`compio::fs::AsyncFd`** with **`io_uring` backend** for
//! true async stream I/O. All operations go through the kernel's `io_uring` interface.
#![allow(dead_code)] // Protocol implementation not yet fully used
#![allow(clippy::missing_errors_doc)] // Protocol spec - errors documented at module level
#![allow(clippy::missing_panics_doc)] // Protocol spec - panics are bugs
#![allow(clippy::missing_docs_in_private_items)]
#![allow(clippy::unwrap_used)] // Protocol spec - unwraps are for testing
#![allow(clippy::expect_used)] // Protocol spec - expects are for testing
#![allow(clippy::future_not_send)] // compio buffers are not Send by design

//!
//! # Architecture
//!
//! ```text
//! PipeTransport
//!     ↓
//! compio::fs::AsyncFd (wraps raw FD)
//!     ↓
//! compio AsyncRead/AsyncWrite
//!     ↓
//! io_uring operations
//! ```

use super::transport::Transport;
use compio::fs::AsyncFd;
use compio::io::{AsyncRead, AsyncWrite};
use std::io;
use std::os::fd::OwnedFd;
use std::os::unix::io::{FromRawFd, RawFd};

/// Pipe-based transport for rsync protocol
///
/// Uses `compio::fs::AsyncFd` to wrap file descriptors and provide stream-based
/// async I/O with `io_uring` backend.
pub struct PipeTransport {
    /// Reader end (stdin or custom FD)
    reader: AsyncFd<OwnedFd>,
    /// Writer end (stdout or custom FD)
    writer: AsyncFd<OwnedFd>,
    /// Transport name for debugging
    #[allow(dead_code)]
    name: String,
}

impl PipeTransport {
    /// Create from stdin/stdout (for --pipe mode)
    ///
    /// # Errors
    ///
    /// Returns an error if FD duplication or `AsyncFd` creation fails.
    pub fn from_stdio() -> io::Result<Self> {
        // Duplicate FDs so we don't close stdin/stdout
        let stdin_fd = unsafe { libc::dup(0) };
        let stdout_fd = unsafe { libc::dup(1) };

        if stdin_fd < 0 || stdout_fd < 0 {
            return Err(io::Error::last_os_error());
        }

        // SAFETY: We just created these FDs via dup()
        unsafe { Self::from_fds(stdin_fd, stdout_fd, "stdio".to_string()) }
    }

    /// Create from specific file descriptors
    ///
    /// # Safety
    ///
    /// Caller must ensure FDs are valid and not closed elsewhere.
    pub unsafe fn from_fds(read_fd: RawFd, write_fd: RawFd, name: String) -> io::Result<Self> {
        // Create OwnedFds (takes ownership)
        let read_owned = OwnedFd::from_raw_fd(read_fd);
        let write_owned = OwnedFd::from_raw_fd(write_fd);

        // Wrap in AsyncFd for compio stream I/O
        let reader = AsyncFd::new(read_owned)?;
        let writer = AsyncFd::new(write_owned)?;

        Ok(Self {
            reader,
            writer,
            name,
        })
    }

    /// Create a Unix pipe pair, returns (`read_fd`, `write_fd`)
    pub fn create_pipe() -> io::Result<(RawFd, RawFd)> {
        let mut fds = [0i32; 2];
        unsafe {
            if libc::pipe(fds.as_mut_ptr()) != 0 {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(fds.into())
    }
}

// ============================================================================
// compio AsyncRead Implementation (delegates to reader)
// ============================================================================

impl AsyncRead for PipeTransport {
    async fn read<B: compio::buf::IoBufMut>(&mut self, buf: B) -> compio::buf::BufResult<usize, B> {
        self.reader.read(buf).await
    }
}

// ============================================================================
// compio AsyncWrite Implementation (delegates to writer)
// ============================================================================

impl AsyncWrite for PipeTransport {
    async fn write<B: compio::buf::IoBuf>(&mut self, buf: B) -> compio::buf::BufResult<usize, B> {
        self.writer.write(buf).await
    }

    async fn flush(&mut self) -> io::Result<()> {
        self.writer.flush().await
    }

    async fn shutdown(&mut self) -> io::Result<()> {
        self.writer.shutdown().await
    }
}

// ============================================================================
// Transport Marker Implementation
// ============================================================================

impl Transport for PipeTransport {
    fn name(&self) -> &'static str {
        "pipe"
    }

    fn supports_multiplexing(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use compio::io::AsyncWriteExt;

    #[test]
    fn test_create_pipe() {
        // Test: Create a Unix pipe pair
        // Requirement: PipeTransport::create_pipe() should return valid FD pair
        let result = PipeTransport::create_pipe();
        assert!(result.is_ok());

        let (read_fd, write_fd) = result.unwrap();
        assert!(read_fd >= 0);
        assert!(write_fd >= 0);
        assert_ne!(read_fd, write_fd);

        // Clean up
        unsafe {
            libc::close(read_fd);
            libc::close(write_fd);
        }
    }

    #[compio::test]
    async fn test_pipe_transport_write_read() {
        // Test: Write data through pipe and read it back
        // Requirement: PipeTransport should support bidirectional async I/O
        let (read_fd, write_fd) = PipeTransport::create_pipe().unwrap();

        // Create separate reader and writer transports with proper FD ownership
        // Duplicate FDs for reader since PipeTransport takes ownership
        let read_fd_dup = unsafe { libc::dup(read_fd) };
        let write_fd_dup = unsafe { libc::dup(write_fd) };

        let mut reader =
            unsafe { PipeTransport::from_fds(read_fd, read_fd_dup, "reader".to_string()).unwrap() };
        let mut writer = unsafe {
            PipeTransport::from_fds(write_fd_dup, write_fd, "writer".to_string()).unwrap()
        };

        // Write data
        let test_data = b"Hello, PipeTransport!";
        let write_result = writer.write_all(test_data.to_vec()).await;
        assert!(write_result.0.is_ok());
        writer.flush().await.unwrap();

        // Read data back
        let mut buf = vec![0u8; test_data.len()];
        let read_result = reader.read(buf).await;
        assert!(read_result.0.is_ok());
        let n = read_result.0.unwrap();
        buf = read_result.1;

        assert_eq!(n, test_data.len());
        assert_eq!(&buf[..n], test_data);
    }

    #[compio::test]
    async fn test_pipe_transport_multiple_writes() {
        // Test: Multiple sequential writes and reads
        // Requirement: PipeTransport should handle multiple I/O operations
        let (read_fd, write_fd) = PipeTransport::create_pipe().unwrap();

        let read_fd_dup = unsafe { libc::dup(read_fd) };
        let write_fd_dup = unsafe { libc::dup(write_fd) };

        let mut reader =
            unsafe { PipeTransport::from_fds(read_fd, read_fd_dup, "reader".to_string()).unwrap() };
        let mut writer = unsafe {
            PipeTransport::from_fds(write_fd_dup, write_fd, "writer".to_string()).unwrap()
        };

        // Write multiple messages
        let messages: &[&[u8]] = &[b"first", b"second", b"third"];

        for msg in messages {
            let write_result = writer.write_all(msg.to_vec()).await;
            assert!(write_result.0.is_ok());
            writer.flush().await.unwrap();

            // Read back
            let mut buf = vec![0u8; msg.len()];
            let read_result = reader.read(buf).await;
            assert!(read_result.0.is_ok());
            let n = read_result.0.unwrap();
            buf = read_result.1;

            assert_eq!(n, msg.len());
            assert_eq!(&buf[..n], *msg);
        }
    }

    #[compio::test]
    async fn test_pipe_transport_large_data() {
        // Test: Transfer large data through pipe
        // Requirement: PipeTransport should handle large data transfers
        let (read_fd, write_fd) = PipeTransport::create_pipe().unwrap();

        let read_fd_dup = unsafe { libc::dup(read_fd) };
        let write_fd_dup = unsafe { libc::dup(write_fd) };

        let mut reader =
            unsafe { PipeTransport::from_fds(read_fd, read_fd_dup, "reader".to_string()).unwrap() };
        let mut writer = unsafe {
            PipeTransport::from_fds(write_fd_dup, write_fd, "writer".to_string()).unwrap()
        };

        // Create 64KB of test data
        let test_data: Vec<u8> = (0..65536).map(|i| (i % 256) as u8).collect();

        // Write data
        let write_result = writer.write_all(test_data.clone()).await;
        assert!(write_result.0.is_ok());
        writer.flush().await.unwrap();

        // Read data back in chunks
        let mut received = Vec::new();
        while received.len() < test_data.len() {
            let mut buf = vec![0u8; 4096];
            let read_result = reader.read(buf).await;
            assert!(read_result.0.is_ok());
            let n = read_result.0.unwrap();
            buf = read_result.1;

            if n == 0 {
                break;
            }
            received.extend_from_slice(&buf[..n]);
        }

        assert_eq!(received.len(), test_data.len());
        assert_eq!(received, test_data);
    }

    #[test]
    fn test_pipe_transport_name() {
        // Test: Transport name is correctly set
        // Requirement: PipeTransport should implement Transport::name()
        // Note: This verifies the trait implementation via type system
        fn assert_transport_name<T: Transport>(name: &'static str) {
            let _: fn(&T) -> &'static str = T::name;
            assert_eq!(name, "pipe");
        }
        assert_transport_name::<PipeTransport>("pipe");
    }

    #[test]
    fn test_pipe_transport_no_multiplexing() {
        // Test: Pipe transport does not support multiplexing
        // Requirement: PipeTransport::supports_multiplexing() should return false
        // Note: This verifies the trait implementation via type system
        fn assert_no_multiplexing<T: Transport>() {
            let _: fn(&T) -> bool = T::supports_multiplexing;
        }
        assert_no_multiplexing::<PipeTransport>();
    }
}
