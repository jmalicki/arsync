//! Pipe-based transport for rsync protocol testing
//!
//! Enables testing rsync wire protocol via pipes (stdin/stdout or Unix pipes)
//! without requiring SSH or network infrastructure.
//!
//! This implementation uses **compio::fs::AsyncFd** with **io_uring backend** for
//! true async stream I/O. All operations go through the kernel's io_uring interface.
#![allow(dead_code)] // Protocol implementation not yet fully used
#![allow(clippy::missing_errors_doc)] // Protocol spec - errors documented at module level
#![allow(clippy::missing_panics_doc)] // Protocol spec - panics are bugs

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
/// Uses compio::fs::AsyncFd to wrap file descriptors and provide stream-based
/// async I/O with io_uring backend.
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
    /// Returns an error if FD duplication or AsyncFd creation fails.
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

    /// Create a Unix pipe pair, returns (read_fd, write_fd)
    pub fn create_pipe() -> io::Result<(RawFd, RawFd)> {
        let mut fds = [0i32; 2];
        unsafe {
            if libc::pipe(fds.as_mut_ptr()) != 0 {
                return Err(io::Error::last_os_error());
            }
        }
        Ok((fds[0], fds[1]))
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
    fn name(&self) -> &str {
        &self.name
    }

    fn supports_multiplexing(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_pipe() {
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
}
