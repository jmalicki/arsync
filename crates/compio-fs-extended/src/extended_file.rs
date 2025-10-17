//! Extended file operations wrapper around compio::fs::File

// DirectoryOps removed - use compio::fs directly for basic directory operations
use crate::error::Result;
#[cfg(target_os = "linux")]
use crate::fadvise::{Fadvise, FadviseAdvice};
use crate::fallocate::Fallocate;
use crate::hardlink::HardlinkOps;
use crate::symlink::SymlinkOps;
#[cfg(feature = "xattr")]
use crate::xattr::XattrOps;
use compio::fs::File;

/// Extended file wrapper that adds additional operations to compio::fs::File
///
/// This struct wraps a `compio::fs::File` and provides extended filesystem
/// operations that are not available in the base compio-fs crate, including:
/// - `fadvise` for file access pattern optimization
/// - `fallocate` for space preallocation
/// - Symlink operations (via DirectoryFd)
/// - Hardlink operations
/// - Extended attributes (xattr) operations
/// - Ownership operations
///
/// All operations are integrated with compio's runtime for optimal async performance.
#[derive(Debug)]
pub struct ExtendedFile {
    /// The underlying compio file
    inner: File,
}

impl ExtendedFile {
    /// Create a new ExtendedFile wrapper around a compio::fs::File
    ///
    /// # Arguments
    ///
    /// * `file` - The compio::fs::File to wrap
    ///
    /// # Returns
    ///
    /// A new ExtendedFile instance
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use compio_fs_extended::ExtendedFile;
    /// use compio::fs::File;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let file = File::open("example.txt").await?;
    /// let extended_file = ExtendedFile::new(file);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn new(file: File) -> Self {
        Self { inner: file }
    }

    /// Create a new ExtendedFile wrapper from a reference to a compio::fs::File
    ///
    /// This method is useful when you need to use ExtendedFile operations without
    /// taking ownership of the underlying File.
    ///
    /// # Arguments
    ///
    /// * `file` - A reference to the compio::fs::File to wrap
    ///
    /// # Returns
    ///
    /// A new ExtendedFile instance that borrows the file
    ///
    /// # Note
    ///
    /// This method creates a wrapper that can perform operations on the file
    /// but doesn't own it. The underlying file must remain valid for the lifetime
    /// of the ExtendedFile.
    #[must_use]
    pub fn from_ref(file: &File) -> Self {
        // We need to clone the file handle to avoid lifetime issues
        // This is safe because File implements Clone
        Self {
            inner: file.clone(),
        }
    }

    /// Get a reference to the underlying compio::fs::File
    ///
    /// # Returns
    ///
    /// A reference to the underlying File
    #[must_use]
    pub fn inner(&self) -> &File {
        &self.inner
    }

    /// Get a mutable reference to the underlying compio::fs::File
    ///
    /// # Returns
    ///
    /// A mutable reference to the underlying File
    pub fn inner_mut(&mut self) -> &mut File {
        &mut self.inner
    }

    /// Consume the ExtendedFile and return the underlying compio::fs::File
    ///
    /// # Returns
    ///
    /// The underlying File
    #[must_use]
    pub fn into_inner(self) -> File {
        self.inner
    }
}

// Implement Fadvise trait (Linux-only)
#[cfg(target_os = "linux")]
impl Fadvise for ExtendedFile {
    async fn fadvise(&self, advice: FadviseAdvice, offset: i64, len: i64) -> Result<()> {
        // Delegate to the fadvise module implementation
        crate::fadvise::fadvise(&self.inner, advice, offset, len).await
    }
}

// Implement Fallocate trait
impl Fallocate for ExtendedFile {
    async fn fallocate(&self, offset: u64, len: u64, mode: u32) -> Result<()> {
        // Delegate to the fallocate module implementation
        crate::fallocate::fallocate(&self.inner, offset, len, mode).await
    }
}

// Implement SymlinkOps trait
impl SymlinkOps for ExtendedFile {
    async fn read_symlink(&self) -> Result<std::path::PathBuf> {
        // Symlink operations require path tracking - not implemented yet
        Err(crate::error::symlink_error(
            "read_symlink not yet implemented - requires path tracking",
        ))
    }

    async fn create_symlink(&self, _target: &std::path::Path) -> Result<()> {
        // Symlink operations require path tracking - not implemented yet
        Err(crate::error::symlink_error(
            "create_symlink not yet implemented - requires path tracking",
        ))
    }
}

// Implement HardlinkOps trait
impl HardlinkOps for ExtendedFile {
    async fn create_hardlink(&self, target: &std::path::Path) -> Result<()> {
        // Delegate to the hardlink module implementation
        crate::hardlink::create_hardlink_impl(&self.inner, target).await
    }
}

// DirectoryOps removed - use compio::fs directly for basic directory operations

// Implement XattrOps trait (when xattr feature is enabled)
#[cfg(feature = "xattr")]
impl XattrOps for ExtendedFile {
    async fn get_xattr(&self, name: &str) -> Result<Vec<u8>> {
        // Delegate to the xattr module implementation
        crate::xattr::get_xattr_impl(&self.inner, name).await
    }

    async fn set_xattr(&self, name: &str, value: &[u8]) -> Result<()> {
        // Delegate to the xattr module implementation
        crate::xattr::set_xattr_impl(&self.inner, name, value).await
    }

    async fn list_xattr(&self) -> Result<Vec<String>> {
        // Delegate to the xattr module implementation
        crate::xattr::list_xattr_impl(&self.inner).await
    }
}

// Conversion traits
impl From<File> for ExtendedFile {
    fn from(file: File) -> Self {
        Self::new(file)
    }
}

impl From<ExtendedFile> for File {
    fn from(extended_file: ExtendedFile) -> Self {
        extended_file.into_inner()
    }
}

// Async traits for compio integration
impl std::ops::Deref for ExtendedFile {
    type Target = File;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl std::ops::DerefMut for ExtendedFile {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}
