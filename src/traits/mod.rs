//! Core traits for filesystem abstraction
//!
//! This module provides foundational traits that enable unified operations
//! across different filesystem backends. The traits are designed to work with
//! compio's async I/O model.
//!
//! See `docs/projects/trait-filesystem-abstraction/` for design documentation.

pub mod metadata;

// Re-export main traits for convenience
pub use metadata::AsyncMetadata;

