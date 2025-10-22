//! Core traits for filesystem abstraction
//!
//! This module provides foundational traits that enable unified operations
//! across different filesystem backends. The traits are designed to work with
//! compio's async I/O model.
//!
//! See `docs/projects/trait-filesystem-abstraction/` for design documentation.


pub mod directory;
pub mod file;
pub mod metadata;

// Re-export main traits for convenience
#[allow(unused_imports)]
// TODO: Remove after wrappers implemented to avoid masking real warnings
pub use directory::{AsyncDirectory, AsyncDirectoryEntry};
pub use file::AsyncFile;
pub use metadata::AsyncMetadata;
