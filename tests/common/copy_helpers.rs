//! Test helpers for copy operations

use arsync::cli::ParallelCopyConfig;
use arsync::copy::copy_file;
use arsync::error::Result;
use arsync::metadata::MetadataConfig;
use std::path::Path;

/// Helper to copy a file in tests
///
/// This is just a thin wrapper around the public `copy_file()` API for consistency in tests.
#[allow(dead_code)] // Not all test files use this
pub async fn copy_file_test(
    src: &Path,
    dst: &Path,
    metadata_config: &MetadataConfig,
    parallel_config: &ParallelCopyConfig,
) -> Result<()> {
    // Simply call the public API - it handles DirectoryFd setup internally
    copy_file(src, dst, metadata_config, parallel_config, 64 * 1024).await
}
