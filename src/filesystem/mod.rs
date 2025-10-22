//! Shared filesystem operations
//!
//! This module provides high-level filesystem operations that can be used
//! by both local and remote backends. All operations use DirectoryFd and
//! *at syscalls for TOCTOU safety.

pub mod metadata;
pub mod read;
pub mod walker;

#[allow(unused_imports)] // Will be used in PR #12 and later
pub use metadata::preserve_metadata;
pub use read::read_file_content;
pub use walker::SecureTreeWalker;
