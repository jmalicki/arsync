//! rsync wire protocol implementation
//!
//! Implements the rsync wire protocol for compatibility with rsync servers.
//! Based on the rsync technical report and protocol specification.
#![allow(dead_code)] // Protocol implementation not yet fully used
#![allow(clippy::missing_errors_doc)] // Protocol spec - errors documented at module level
#![allow(clippy::missing_panics_doc)] // Protocol spec - panics are bugs
#![allow(clippy::must_use_candidate)] // Protocol spec - use is context-dependent

use crate::cli::Args;
use crate::protocol::checksum::{rolling_checksum, strong_checksum};
use crate::protocol::pipe::PipeTransport;
use crate::protocol::ssh::SshConnection;
use crate::protocol::transport::{self, Transport};
use crate::sync::SyncStats;
use anyhow::Result;
use compio::io::AsyncWrite;
use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};
use walkdir;

/// Protocol version we support
const PROTOCOL_VERSION: u8 = 31; // rsync 3.2+

/// Minimum protocol version we accept
const MIN_PROTOCOL_VERSION: u8 = 27; // rsync 3.0+

/// Default block size for checksums (rsync default)
const DEFAULT_BLOCK_SIZE: usize = 700;

/// Minimum block size
const MIN_BLOCK_SIZE: usize = 128;

/// Rolling checksum constants (Adler-32 style)
#[allow(dead_code)]
const ROLLING_MODULUS: u32 = 65521;

/// Push files to remote using rsync protocol over SSH
///
/// Uses the complete rsync wire protocol implementation to send files
/// to a remote arsync server over an SSH connection.
///
/// # Arguments
///
/// * `args` - Command-line arguments with metadata preservation settings
/// * `local_path` - Local path to send
/// * `connection` - SSH connection to remote server
///
/// # Returns
///
/// Returns sync statistics (files copied, bytes transferred, duration)
///
/// # Errors
///
/// Returns an error if:
/// - Protocol negotiation fails
/// - File list transmission fails  
/// - SSH connection fails during transfer
pub async fn push_via_rsync_protocol(
    args: &Args,
    local_path: &Path,
    connection: SshConnection,
) -> Result<SyncStats> {
    use crate::protocol::rsync_compat;
    use tracing::info;

    info!("Starting push to remote via rsync protocol");
    info!("Local path: {}", local_path.display());

    // SshConnection already implements Transport
    // We can use it directly with rsync_compat (no wrapper needed!)
    let stats = rsync_compat::rsync_send(args, local_path, connection)
        .await
        .map_err(|e| anyhow::anyhow!("Push via rsync protocol failed: {}", e))?;

    info!(
        "Push complete: {} files, {} bytes",
        stats.files_copied, stats.bytes_copied
    );

    Ok(stats)
}

/// Pull files from remote using rsync protocol over SSH
///
/// Uses the complete rsync wire protocol implementation to receive files
/// from a remote arsync server over an SSH connection.
///
/// # Arguments
///
/// * `args` - Command-line arguments with metadata preservation settings
/// * `connection` - SSH connection to remote server
/// * `local_path` - Local destination path for received files
///
/// # Returns
///
/// Returns sync statistics (files copied, bytes transferred, duration)
///
/// # Errors
///
/// Returns an error if:
/// - Protocol negotiation fails
/// - File list reception fails
/// - SSH connection fails during transfer
pub async fn pull_via_rsync_protocol(
    args: &Args,
    connection: SshConnection,
    local_path: &Path,
) -> Result<SyncStats> {
    use crate::protocol::rsync_compat;
    use tracing::info;

    info!("Starting pull from remote via rsync protocol");
    info!("Local path: {}", local_path.display());

    // SshConnection already implements Transport
    // We can use it directly with rsync_compat (no wrapper needed!)
    let stats = rsync_compat::rsync_receive(args, connection, local_path)
        .await
        .map_err(|e| anyhow::anyhow!("Pull via rsync protocol failed: {}", e))?;

    info!(
        "Pull complete: {} files, {} bytes",
        stats.files_copied, stats.bytes_copied
    );

    Ok(stats)
}

/// rsync protocol handshake (old stub, kept for reference)
#[allow(dead_code)]
async fn handshake(_connection: &mut SshConnection) -> Result<u8> {
    // TODO: Send our protocol version
    // TODO: Receive remote protocol version
    // TODO: Negotiate common version
    Ok(PROTOCOL_VERSION)
}

/// File list entry in rsync protocol
#[derive(Debug, Clone)]
pub struct FileEntry {
    /// Relative path from source root
    pub path: String,
    /// File size in bytes
    pub size: u64,
    /// Modification time (Unix timestamp)
    pub mtime: i64,
    /// File mode (permissions and file type bits)
    pub mode: u32,
    /// User ID (owner)
    pub uid: u32,
    /// Group ID
    pub gid: u32,
    /// Whether this is a symbolic link
    pub is_symlink: bool,
    /// Symlink target path (if is_symlink is true)
    pub symlink_target: Option<String>,
}

/// Generate file list for transmission (old stub, kept for reference)
#[allow(dead_code)]
async fn generate_file_list(_path: &Path, _args: &Args) -> Result<Vec<FileEntry>> {
    // TODO: Traverse directory
    // TODO: Generate FileEntry for each file
    // TODO: Handle recursion, symlinks, etc.
    Ok(vec![])
}

/// Send file list over connection (old stub, kept for reference)
#[allow(dead_code)]
async fn send_file_list(_connection: &mut SshConnection, _files: &[FileEntry]) -> Result<()> {
    // TODO: Encode file list in rsync wire format
    // TODO: Send over SSH connection
    Ok(())
}

/// Receive file list from connection (old stub, kept for reference)
#[allow(dead_code)]
async fn receive_file_list(_connection: &mut SshConnection) -> Result<Vec<FileEntry>> {
    // TODO: Receive and decode file list
    Ok(vec![])
}

/// Block checksum for rsync delta algorithm
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BlockChecksum {
    /// Rolling checksum (fast, collision-prone)
    pub weak: u32,
    /// MD5 checksum (slow, collision-resistant)
    pub strong: [u8; 16],
    /// Offset in file where this block starts
    pub offset: u64,
    /// Index of this block
    pub block_index: u32,
}

/// Delta instruction for reconstructing files
#[derive(Debug, Clone)]
pub enum DeltaInstruction {
    /// Raw data to insert (when no match found)
    Literal(Vec<u8>),
    /// Copy from basis file using block index
    BlockMatch {
        /// Index of the matching block in the basis file
        block_index: u32,
        /// Length of the match in bytes
        length: u32,
    },
}

/// Calculate optimal block size for a file
fn calculate_block_size(file_size: u64) -> usize {
    if file_size == 0 {
        return DEFAULT_BLOCK_SIZE;
    }

    // rsync's algorithm: sqrt of file size, but clamped
    let block_size = (file_size as f64).sqrt() as usize;
    block_size.clamp(MIN_BLOCK_SIZE, DEFAULT_BLOCK_SIZE * 4)
}

// ============================================================================
// Pipe Mode Implementation (for testing)
// ============================================================================

/// Send files via pipe transport (for testing)
pub async fn send_via_pipe(
    _args: &Args,
    source_path: &Path,
    mut transport: PipeTransport,
) -> Result<SyncStats> {
    let start = Instant::now();

    debug!("Sender: Starting protocol handshake");

    // Phase 1: Handshake
    let remote_version = handshake_sender(&mut transport).await?;
    debug!(
        "Sender: Handshake complete, remote version: {}",
        remote_version
    );

    // Phase 2: Send file list
    debug!(
        "Sender: Generating file list from: {}",
        source_path.display()
    );
    let files = generate_file_list_simple(source_path, _args).await?;
    info!("Sender: Found {} files to send", files.len());

    send_file_list_simple(&mut transport, &files).await?;
    debug!("Sender: File list sent");

    // Phase 3: Delta transfer with block checksums
    let mut bytes_sent = 0u64;
    let mut bytes_matched = 0u64;

    for file in &files {
        if file.is_symlink {
            // Symlinks have no content, skip
            continue;
        }

        debug!("Sender: Processing file: {}", file.path);
        let file_path = source_path.join(&file.path);

        // Read file content
        let content = fs::read(&file_path)
            .map_err(|e| anyhow::anyhow!("Failed to read {}: {}", file.path, e))?;

        // Receive block checksums from receiver
        let block_checksums = receive_block_checksums(&mut transport).await?;

        if block_checksums.is_empty() {
            // No basis file, send everything as literal
            debug!(
                "Sender: No basis file, sending {} bytes as literal",
                content.len()
            );
            let delta = vec![DeltaInstruction::Literal(content.clone())];
            send_delta(&mut transport, &delta).await?;
            bytes_sent += content.len() as u64;
        } else {
            // Generate delta using block matching
            debug!(
                "Sender: Received {} block checksums, generating delta",
                block_checksums.len()
            );
            let delta = generate_delta(&content, &block_checksums)?;

            // Calculate statistics
            let (literal_bytes, matched_bytes) = count_delta_bytes(&delta);
            debug!(
                "Sender: Delta: {} literal bytes, {} matched bytes",
                literal_bytes, matched_bytes
            );

            send_delta(&mut transport, &delta).await?;
            bytes_sent += literal_bytes as u64;
            bytes_matched += matched_bytes as u64;
        }
    }

    // Flush to ensure all data is sent
    transport
        .flush()
        .await
        .map_err(|e| anyhow::anyhow!("Flush failed: {}", e))?;

    info!(
        "Sender: Transfer complete, sent {} bytes, matched {} bytes",
        bytes_sent, bytes_matched
    );

    Ok(SyncStats {
        files_copied: files.len() as u64,
        bytes_copied: files.iter().map(|f| f.size).sum(),
        duration: start.elapsed(),
    })
}

/// Receive files via pipe transport (for testing)
pub async fn receive_via_pipe(
    _args: &Args,
    mut transport: PipeTransport,
    dest_path: &Path,
) -> Result<SyncStats> {
    let start = Instant::now();

    debug!("Receiver: Starting protocol handshake");

    // Phase 1: Handshake
    let remote_version = handshake_receiver(&mut transport).await?;
    debug!(
        "Receiver: Handshake complete, remote version: {}",
        remote_version
    );

    // Phase 2: Receive file list
    debug!("Receiver: Receiving file list");
    let files = receive_file_list_simple(&mut transport).await?;
    info!("Receiver: Received {} files", files.len());

    // Phase 3: Delta transfer with block checksums
    let mut bytes_received = 0u64;
    let mut bytes_matched = 0u64;

    for file in &files {
        debug!("Receiver: Processing file: {}", file.path);
        let file_path = dest_path.join(&file.path);

        // Create parent directories
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)?;
        }

        if file.is_symlink {
            // Handle symlink
            if let Some(target) = &file.symlink_target {
                debug!("Receiver: Creating symlink {} -> {}", file.path, target);
                #[cfg(unix)]
                {
                    use std::os::unix::fs as unix_fs;
                    unix_fs::symlink(target, &file_path)?;
                }
            }
        } else {
            // Regular file - use delta transfer
            // Check if basis file exists
            let basis_content = if file_path.exists() {
                fs::read(&file_path).ok()
            } else {
                None
            };

            // Generate and send block checksums
            let block_checksums = if let Some(ref basis) = basis_content {
                let block_size = calculate_block_size(basis.len() as u64);
                debug!(
                    "Receiver: Basis file exists ({} bytes), block size {}",
                    basis.len(),
                    block_size
                );
                generate_block_checksums(basis, block_size)?
            } else {
                debug!("Receiver: No basis file, sending empty checksum list");
                vec![]
            };

            send_block_checksums(&mut transport, &block_checksums).await?;

            // Receive delta and apply
            let delta = receive_delta(&mut transport).await?;
            let (literal_bytes, matched_bytes) = count_delta_bytes(&delta);
            debug!(
                "Receiver: Received delta: {} literal bytes, {} matched bytes",
                literal_bytes, matched_bytes
            );

            let reconstructed = apply_delta(basis_content.as_deref(), &delta, &block_checksums)?;

            // Write reconstructed file
            fs::write(&file_path, &reconstructed)?;
            bytes_received += literal_bytes as u64;
            bytes_matched += matched_bytes as u64;

            debug!("Receiver: Reconstructed {} bytes", reconstructed.len());
        }

        // Apply metadata (permissions, timestamps, ownership)
        apply_metadata(&file_path, file)?;
    }

    info!(
        "Receiver: Transfer complete, received {} literal bytes, matched {} bytes",
        bytes_received, bytes_matched
    );

    Ok(SyncStats {
        files_copied: files.len() as u64,
        bytes_copied: files.iter().map(|f| f.size).sum(),
        duration: start.elapsed(),
    })
}

// ============================================================================
// Protocol Implementation (Minimal for Testing)
// ============================================================================

/// Handshake as sender
async fn handshake_sender<T: Transport>(transport: &mut T) -> Result<u8> {
    // Send our protocol version
    transport::write_all(transport, &[PROTOCOL_VERSION]).await?;

    // Receive remote protocol version
    let mut version_buf = [0u8; 1];
    transport::read_exact(transport, &mut version_buf).await?;
    let remote_version = version_buf[0];

    if remote_version < MIN_PROTOCOL_VERSION {
        anyhow::bail!(
            "Remote protocol version {} too old (need at least {})",
            remote_version,
            MIN_PROTOCOL_VERSION
        );
    }

    Ok(remote_version)
}

/// Handshake as receiver
async fn handshake_receiver<T: Transport>(transport: &mut T) -> Result<u8> {
    // Receive remote protocol version
    let mut version_buf = [0u8; 1];
    transport::read_exact(transport, &mut version_buf).await?;
    let remote_version = version_buf[0];

    if remote_version < MIN_PROTOCOL_VERSION {
        anyhow::bail!(
            "Remote protocol version {} too old (need at least {})",
            remote_version,
            MIN_PROTOCOL_VERSION
        );
    }

    // Send our protocol version
    transport::write_all(transport, &[PROTOCOL_VERSION]).await?;

    Ok(remote_version)
}

/// Generate simple file list (minimal implementation)
async fn generate_file_list_simple(path: &Path, args: &Args) -> Result<Vec<FileEntry>> {
    let mut files = Vec::new();

    if path.is_file() {
        // Single file
        let metadata = fs::metadata(path)?;
        files.push(FileEntry {
            path: path
                .file_name()
                .ok_or_else(|| anyhow::anyhow!("No filename"))?
                .to_string_lossy()
                .to_string(),
            size: metadata.len(),
            mtime: metadata
                .modified()?
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs() as i64,
            mode: metadata.permissions().mode(),
            uid: metadata.uid(),
            gid: metadata.gid(),
            is_symlink: false,
            symlink_target: None,
        });
    } else if path.is_dir() && args.should_recurse() {
        // Recursively list directory (blocking I/O for simplicity in protocol code)
        for entry in walkdir::WalkDir::new(path) {
            let entry = entry?;
            let metadata = entry.metadata()?;
            if metadata.is_file() {
                let rel_path = entry
                    .path()
                    .strip_prefix(path)?
                    .to_string_lossy()
                    .to_string();
                files.push(FileEntry {
                    path: rel_path,
                    size: metadata.len(),
                    mtime: metadata
                        .modified()?
                        .duration_since(std::time::UNIX_EPOCH)?
                        .as_secs() as i64,
                    mode: metadata.permissions().mode(),
                    uid: metadata.uid(),
                    gid: metadata.gid(),
                    is_symlink: false,
                    symlink_target: None,
                });
            } else if metadata.is_symlink() {
                let symlink_target = fs::read_link(entry.path())?.to_string_lossy().to_string();
                let rel_path = entry
                    .path()
                    .strip_prefix(path)?
                    .to_string_lossy()
                    .to_string();
                files.push(FileEntry {
                    path: rel_path,
                    size: 0, // Symlinks have no size
                    mtime: metadata
                        .modified()?
                        .duration_since(std::time::UNIX_EPOCH)?
                        .as_secs() as i64,
                    mode: metadata.permissions().mode(),
                    uid: metadata.uid(),
                    gid: metadata.gid(),
                    is_symlink: true,
                    symlink_target: Some(symlink_target),
                });
            }
        }
    }

    Ok(files)
}

/// Apply full metadata to a file
fn apply_metadata(path: &Path, file: &FileEntry) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    // Set permissions
    let permissions = std::fs::Permissions::from_mode(file.mode);
    if let Err(e) = fs::set_permissions(path, permissions) {
        warn!("Failed to set permissions on {}: {}", path.display(), e);
    }

    // Set ownership (requires root privileges, so we'll try but not fail)
    #[cfg(unix)]
    {
        // Note: chown/lchown not in std, would need nix crate
        // For now, we'll skip ownership (rsync also needs --owner --group flags)
        // This matches rsync's behavior when run without privileges
        debug!(
            "Skipping ownership for {} (uid={}, gid={}) - requires privileges",
            path.display(),
            file.uid,
            file.gid
        );
    }

    // Set modification time
    if !file.is_symlink {
        use std::time::UNIX_EPOCH;
        let mtime = UNIX_EPOCH + Duration::from_secs(file.mtime as u64);
        if let Err(e) = filetime::set_file_mtime(path, filetime::FileTime::from_system_time(mtime))
        {
            warn!("Failed to set mtime on {}: {}", path.display(), e);
        }
    }

    Ok(())
}

/// Send file list (with full metadata)
async fn send_file_list_simple<T: Transport>(transport: &mut T, files: &[FileEntry]) -> Result<()> {
    // Send file count as 4-byte little-endian
    let count = files.len() as u32;
    transport::write_all(transport, &count.to_le_bytes()).await?;

    // Send each file entry
    for file in files {
        // Flags byte: bit 0 = is_symlink
        let flags = if file.is_symlink { 1u8 } else { 0u8 };
        transport::write_all(transport, &[flags]).await?;

        // Path length + path
        let path_bytes = file.path.as_bytes();
        let path_len = path_bytes.len() as u32;
        transport::write_all(transport, &path_len.to_le_bytes()).await?;
        transport::write_all(transport, path_bytes).await?;

        // File size
        transport::write_all(transport, &file.size.to_le_bytes()).await?;

        // Full metadata
        transport::write_all(transport, &file.mtime.to_le_bytes()).await?;
        transport::write_all(transport, &file.mode.to_le_bytes()).await?;
        transport::write_all(transport, &file.uid.to_le_bytes()).await?;
        transport::write_all(transport, &file.gid.to_le_bytes()).await?;

        // Symlink target if applicable
        if file.is_symlink {
            if let Some(target) = &file.symlink_target {
                let target_bytes = target.as_bytes();
                let target_len = target_bytes.len() as u32;
                transport::write_all(transport, &target_len.to_le_bytes()).await?;
                transport::write_all(transport, target_bytes).await?;
            }
        }
    }

    Ok(())
}

/// Receive file list (with full metadata)
async fn receive_file_list_simple<T: Transport>(transport: &mut T) -> Result<Vec<FileEntry>> {
    // Receive file count
    let mut count_buf = [0u8; 4];
    transport::read_exact(transport, &mut count_buf).await?;
    let count = u32::from_le_bytes(count_buf) as usize;

    let mut files = Vec::with_capacity(count);

    // Receive each file entry
    for _ in 0..count {
        // Flags byte
        let mut flags_buf = [0u8; 1];
        transport::read_exact(transport, &mut flags_buf).await?;
        let is_symlink = (flags_buf[0] & 1) != 0;

        // Path length
        let mut path_len_buf = [0u8; 4];
        transport::read_exact(transport, &mut path_len_buf).await?;
        let path_len = u32::from_le_bytes(path_len_buf) as usize;

        // Path
        let mut path_buf = vec![0u8; path_len];
        transport::read_exact(transport, &mut path_buf).await?;
        let path = String::from_utf8(path_buf)?;

        // File size
        let mut size_buf = [0u8; 8];
        transport::read_exact(transport, &mut size_buf).await?;
        let size = u64::from_le_bytes(size_buf);

        // Full metadata
        let mut mtime_buf = [0u8; 8];
        transport::read_exact(transport, &mut mtime_buf).await?;
        let mtime = i64::from_le_bytes(mtime_buf);

        let mut mode_buf = [0u8; 4];
        transport::read_exact(transport, &mut mode_buf).await?;
        let mode = u32::from_le_bytes(mode_buf);

        let mut uid_buf = [0u8; 4];
        transport::read_exact(transport, &mut uid_buf).await?;
        let uid = u32::from_le_bytes(uid_buf);

        let mut gid_buf = [0u8; 4];
        transport::read_exact(transport, &mut gid_buf).await?;
        let gid = u32::from_le_bytes(gid_buf);

        // Symlink target if applicable
        let symlink_target = if is_symlink {
            let mut target_len_buf = [0u8; 4];
            transport::read_exact(transport, &mut target_len_buf).await?;
            let target_len = u32::from_le_bytes(target_len_buf) as usize;

            let mut target_buf = vec![0u8; target_len];
            transport::read_exact(transport, &mut target_buf).await?;
            Some(String::from_utf8(target_buf)?)
        } else {
            None
        };

        files.push(FileEntry {
            path,
            size,
            mtime,
            mode,
            uid,
            gid,
            is_symlink,
            symlink_target,
        });
    }

    Ok(files)
}

// ============================================================================
// Delta Algorithm Implementation
// ============================================================================

/// Generate block checksums for a file (receiver side)
pub fn generate_block_checksums(data: &[u8], block_size: usize) -> Result<Vec<BlockChecksum>> {
    let mut checksums = Vec::new();
    let mut offset = 0;
    let mut block_index = 0;

    while offset < data.len() {
        let end = (offset + block_size).min(data.len());
        let block = &data[offset..end];

        checksums.push(BlockChecksum {
            weak: rolling_checksum(block),
            strong: strong_checksum(block),
            offset: offset as u64,
            block_index,
        });

        offset = end;
        block_index += 1;
    }

    Ok(checksums)
}

/// Generate delta by finding matching blocks (sender side)
pub fn generate_delta(data: &[u8], checksums: &[BlockChecksum]) -> Result<Vec<DeltaInstruction>> {
    if checksums.is_empty() {
        // No basis, send everything
        return Ok(vec![DeltaInstruction::Literal(data.to_vec())]);
    }

    let block_size = if checksums.len() > 1 {
        (checksums[1].offset - checksums[0].offset) as usize
    } else {
        DEFAULT_BLOCK_SIZE
    };

    // Build hash map for fast weak checksum lookup
    let mut weak_map: HashMap<u32, Vec<&BlockChecksum>> = HashMap::new();
    for checksum in checksums {
        weak_map.entry(checksum.weak).or_default().push(checksum);
    }

    let mut delta = Vec::new();
    let mut pos = 0;
    let mut literal_buffer = Vec::new();

    while pos < data.len() {
        let remaining = data.len() - pos;
        let window_size = remaining.min(block_size);

        if window_size < block_size && pos > 0 {
            // Last partial block, send as literal
            literal_buffer.extend_from_slice(&data[pos..]);
            break;
        }

        let window = &data[pos..pos + window_size];
        let weak = rolling_checksum(window);

        // Check for weak match
        if let Some(candidates) = weak_map.get(&weak) {
            // Verify with strong checksum
            let strong = strong_checksum(window);
            if let Some(matched) = candidates.iter().find(|c| c.strong == strong) {
                // Found a match!
                // Flush any pending literal data
                if !literal_buffer.is_empty() {
                    delta.push(DeltaInstruction::Literal(literal_buffer.clone()));
                    literal_buffer.clear();
                }

                // Add block match instruction
                delta.push(DeltaInstruction::BlockMatch {
                    block_index: matched.block_index,
                    length: window_size as u32,
                });

                pos += window_size;
                continue;
            }
        }

        // No match, add byte to literal buffer
        literal_buffer.push(data[pos]);
        pos += 1;
    }

    // Flush any remaining literal data
    if !literal_buffer.is_empty() {
        delta.push(DeltaInstruction::Literal(literal_buffer));
    }

    Ok(delta)
}

/// Apply delta to reconstruct file (receiver side)
pub fn apply_delta(
    basis: Option<&[u8]>,
    delta: &[DeltaInstruction],
    checksums: &[BlockChecksum],
) -> Result<Vec<u8>> {
    let mut output = Vec::new();

    for instruction in delta {
        match instruction {
            DeltaInstruction::Literal(data) => {
                // Append literal data
                output.extend_from_slice(data);
            }
            DeltaInstruction::BlockMatch {
                block_index,
                length,
            } => {
                // Copy block from basis file
                if let Some(basis_data) = basis {
                    if let Some(checksum) = checksums.iter().find(|c| c.block_index == *block_index)
                    {
                        let start = checksum.offset as usize;
                        let end = (start + *length as usize).min(basis_data.len());
                        output.extend_from_slice(&basis_data[start..end]);
                    } else {
                        anyhow::bail!("Block index {} not found in checksums", block_index);
                    }
                } else {
                    anyhow::bail!("BlockMatch instruction but no basis file");
                }
            }
        }
    }

    Ok(output)
}

/// Send block checksums over transport
async fn send_block_checksums<T: Transport>(
    transport: &mut T,
    checksums: &[BlockChecksum],
) -> Result<()> {
    // Send count
    let count = checksums.len() as u32;
    transport::write_all(transport, &count.to_le_bytes()).await?;

    // Send each checksum
    for checksum in checksums {
        transport::write_all(transport, &checksum.weak.to_le_bytes()).await?;
        transport::write_all(transport, &checksum.strong).await?;
        transport::write_all(transport, &checksum.offset.to_le_bytes()).await?;
        transport::write_all(transport, &checksum.block_index.to_le_bytes()).await?;
    }

    Ok(())
}

/// Receive block checksums from transport
async fn receive_block_checksums<T: Transport>(transport: &mut T) -> Result<Vec<BlockChecksum>> {
    // Receive count
    let mut count_buf = [0u8; 4];
    transport::read_exact(transport, &mut count_buf).await?;
    let count = u32::from_le_bytes(count_buf) as usize;

    let mut checksums = Vec::with_capacity(count);

    // Receive each checksum
    for _ in 0..count {
        let mut weak_buf = [0u8; 4];
        transport::read_exact(transport, &mut weak_buf).await?;
        let weak = u32::from_le_bytes(weak_buf);

        let mut strong = [0u8; 16];
        transport::read_exact(transport, &mut strong).await?;

        let mut offset_buf = [0u8; 8];
        transport::read_exact(transport, &mut offset_buf).await?;
        let offset = u64::from_le_bytes(offset_buf);

        let mut index_buf = [0u8; 4];
        transport::read_exact(transport, &mut index_buf).await?;
        let block_index = u32::from_le_bytes(index_buf);

        checksums.push(BlockChecksum {
            weak,
            strong,
            offset,
            block_index,
        });
    }

    Ok(checksums)
}

/// Send delta instructions over transport
async fn send_delta<T: Transport>(transport: &mut T, delta: &[DeltaInstruction]) -> Result<()> {
    // Send instruction count
    let count = delta.len() as u32;
    transport::write_all(transport, &count.to_le_bytes()).await?;

    // Send each instruction
    for instruction in delta {
        match instruction {
            DeltaInstruction::Literal(data) => {
                // Type: 0 = Literal
                transport::write_all(transport, &[0u8]).await?;
                // Length + data
                let len = data.len() as u32;
                transport::write_all(transport, &len.to_le_bytes()).await?;
                transport::write_all(transport, data).await?;
            }
            DeltaInstruction::BlockMatch {
                block_index,
                length,
            } => {
                // Type: 1 = BlockMatch
                transport::write_all(transport, &[1u8]).await?;
                transport::write_all(transport, &block_index.to_le_bytes()).await?;
                transport::write_all(transport, &length.to_le_bytes()).await?;
            }
        }
    }

    Ok(())
}

/// Receive delta instructions from transport
async fn receive_delta<T: Transport>(transport: &mut T) -> Result<Vec<DeltaInstruction>> {
    // Receive instruction count
    let mut count_buf = [0u8; 4];
    transport::read_exact(transport, &mut count_buf).await?;
    let count = u32::from_le_bytes(count_buf) as usize;

    let mut delta = Vec::with_capacity(count);

    // Receive each instruction
    for _ in 0..count {
        let mut type_buf = [0u8; 1];
        transport::read_exact(transport, &mut type_buf).await?;

        match type_buf[0] {
            0 => {
                // Literal
                let mut len_buf = [0u8; 4];
                transport::read_exact(transport, &mut len_buf).await?;
                let len = u32::from_le_bytes(len_buf) as usize;

                let mut data = vec![0u8; len];
                transport::read_exact(transport, &mut data).await?;

                delta.push(DeltaInstruction::Literal(data));
            }
            1 => {
                // BlockMatch
                let mut index_buf = [0u8; 4];
                transport::read_exact(transport, &mut index_buf).await?;
                let block_index = u32::from_le_bytes(index_buf);

                let mut length_buf = [0u8; 4];
                transport::read_exact(transport, &mut length_buf).await?;
                let length = u32::from_le_bytes(length_buf);

                delta.push(DeltaInstruction::BlockMatch {
                    block_index,
                    length,
                });
            }
            _ => anyhow::bail!("Unknown delta instruction type: {}", type_buf[0]),
        }
    }

    Ok(delta)
}

/// Count literal and matched bytes in delta
fn count_delta_bytes(delta: &[DeltaInstruction]) -> (usize, usize) {
    let mut literal = 0;
    let mut matched = 0;

    for instruction in delta {
        match instruction {
            DeltaInstruction::Literal(data) => literal += data.len(),
            DeltaInstruction::BlockMatch { length, .. } => matched += *length as usize,
        }
    }

    (literal, matched)
}
