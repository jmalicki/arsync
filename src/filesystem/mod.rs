//! Shared filesystem operations
//!
//! This module provides high-level filesystem operations that can be used
//! by both local and remote backends. All operations use DirectoryFd and
//! *at syscalls for TOCTOU safety.

pub mod metadata;
pub mod read;
pub mod walker;
pub mod write;

pub use metadata::preserve_metadata;
pub use read::read_file_content;
pub use walker::SecureTreeWalker;
pub use write::write_file_content;
