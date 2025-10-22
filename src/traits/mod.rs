//! Core traits for filesystem abstraction
//!
//! This module provides foundational traits that enable unified operations
//! across different filesystem backends. The traits are designed to work with
//! compio's async I/O model.
//!
//! See `docs/projects/trait-filesystem-abstraction/` for design documentation.

// Disallow std::fs usage in this module to enforce async filesystem operations
#![deny(clippy::disallowed_methods)]

pub mod file;
pub mod metadata;

// Re-export main traits for convenience
#[allow(unused_imports)]
// TODO: Remove after PR #4 (file wrapper) to avoid masking real warnings
pub use file::AsyncFile;
pub use metadata::AsyncMetadata;
