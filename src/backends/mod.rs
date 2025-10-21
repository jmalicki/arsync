//! Filesystem backend implementations
//!
//! This module provides concrete implementations of the AsyncFileSystem trait
//! for different storage backends, including local filesystem and remote protocol backends.

pub mod local;
pub mod protocol;

// Re-export main types for convenience
pub use local::LocalFileSystem;
pub use protocol::ProtocolFileSystem;