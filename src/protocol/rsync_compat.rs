//! rsync Wire Protocol Compatibility Layer
//!
//! This module implements the actual rsync wire protocol for interoperability
//! with rsync processes. rsync uses a multiplexed I/O protocol with message tags.
#![allow(dead_code)] // Protocol implementation not yet fully used
#![allow(clippy::missing_errors_doc)] // Protocol spec - errors documented at module level
#![allow(clippy::missing_panics_doc)] // Protocol spec - panics are bugs
#![allow(clippy::must_use_candidate)] // Protocol spec - use is context-dependent

use crate::cli::Args;
use crate::protocol::rsync::FileEntry;
use crate::protocol::transport::{self, Transport};
use crate::protocol::varint::encode_varint_into;
use crate::sync::SyncStats;
use anyhow::Result;
use std::fs;
use std::io::Cursor;
use std::path::Path;
use std::time::Instant;
use tracing::{debug, info, warn};
use walkdir;

/// rsync protocol uses multiplexed I/O with tags
/// Tag values from rsync source code (io.c)
const MPLEX_BASE: u8 = 7;

/// Message tags (rsync protocol)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MessageTag {
    Data = MPLEX_BASE,            // Regular data
    ErrorXfer = MPLEX_BASE + 1,   // Transfer error
    Info = MPLEX_BASE + 2,        // Info message
    Error = MPLEX_BASE + 3,       // Error message
    Warning = MPLEX_BASE + 4,     // Warning
    ErrorSocket = MPLEX_BASE + 5, // Socket error
    Log = MPLEX_BASE + 6,         // Log message
    Client = MPLEX_BASE + 7,      // Client message
    Redo = MPLEX_BASE + 9,        // Redo request
    FList = MPLEX_BASE + 20,      // File list data
    FName = MPLEX_BASE + 21,      // Filename
    IoError = MPLEX_BASE + 22,    // I/O error
    Success = MPLEX_BASE + 100,   // Success
    NoSend = MPLEX_BASE + 101,    // Nothing to send
}

impl MessageTag {
    fn from_u8(tag: u8) -> Option<Self> {
        match tag {
            7 => Some(Self::Data),
            8 => Some(Self::ErrorXfer),
            9 => Some(Self::Info),
            10 => Some(Self::Error),
            11 => Some(Self::Warning),
            12 => Some(Self::ErrorSocket),
            13 => Some(Self::Log),
            14 => Some(Self::Client),
            16 => Some(Self::Redo),
            27 => Some(Self::FList),
            28 => Some(Self::FName),
            29 => Some(Self::IoError),
            107 => Some(Self::Success),
            108 => Some(Self::NoSend),
            _ => None,
        }
    }
}

/// rsync multiplexed message
#[derive(Debug)]
pub struct MultiplexMessage {
    pub tag: MessageTag,
    pub data: Vec<u8>,
}

/// Read a multiplexed message from rsync protocol stream
pub async fn read_mplex_message<T: Transport>(transport: &mut T) -> Result<MultiplexMessage> {
    // Read tag byte
    let mut tag_buf = [0u8; 1];
    transport::read_exact(transport, &mut tag_buf).await?;
    let tag_byte = tag_buf[0];

    // Parse tag
    let tag = MessageTag::from_u8(tag_byte)
        .ok_or_else(|| anyhow::anyhow!("Unknown rsync message tag: {}", tag_byte))?;

    // Read length (3 bytes, little-endian, but only lower 24 bits used)
    let mut len_buf = [0u8; 3];
    transport::read_exact(transport, &mut len_buf).await?;
    let length = u32::from_le_bytes([len_buf[0], len_buf[1], len_buf[2], 0]) as usize;

    // Read data
    let mut data = vec![0u8; length];
    if length > 0 {
        transport::read_exact(transport, &mut data).await?;
    }

    debug!("Read rsync message: tag={:?}, length={}", tag, length);

    Ok(MultiplexMessage { tag, data })
}

/// Write a multiplexed message to rsync protocol stream
pub async fn write_mplex_message<T: Transport>(
    transport: &mut T,
    tag: MessageTag,
    data: &[u8],
) -> Result<()> {
    let length = data.len();

    if length > 0xFFFFFF {
        anyhow::bail!("Message too large for rsync protocol: {} bytes", length);
    }

    // Write tag
    transport::write_all(transport, &[tag as u8]).await?;

    // Write length (3 bytes, little-endian)
    let len_bytes = (length as u32).to_le_bytes();
    transport::write_all(transport, &len_bytes[0..3]).await?;

    // Write data
    if length > 0 {
        transport::write_all(transport, data).await?;
    }

    debug!("Wrote rsync message: tag={:?}, length={}", tag, length);

    Ok(())
}

/// Read a data message, handling multiplexed protocol
pub async fn read_data<T: Transport>(transport: &mut T, buffer: &mut [u8]) -> Result<usize> {
    loop {
        let msg = read_mplex_message(transport).await?;

        match msg.tag {
            MessageTag::Data => {
                // This is the data we want
                let copy_len = msg.data.len().min(buffer.len());
                buffer[..copy_len].copy_from_slice(&msg.data[..copy_len]);
                return Ok(copy_len);
            }
            MessageTag::Info | MessageTag::Log => {
                // Log messages, print to stderr
                if let Ok(text) = String::from_utf8(msg.data) {
                    debug!("rsync: {}", text.trim());
                }
            }
            MessageTag::Error | MessageTag::ErrorXfer => {
                // Error message
                let error_msg = String::from_utf8_lossy(&msg.data);
                anyhow::bail!("rsync error: {}", error_msg);
            }
            MessageTag::Warning => {
                let warn_msg = String::from_utf8_lossy(&msg.data);
                warn!("rsync warning: {}", warn_msg);
            }
            _ => {
                debug!("Unexpected rsync message tag: {:?}", msg.tag);
            }
        }
    }
}

/// Write data with multiplexed protocol
pub async fn write_data<T: Transport>(transport: &mut T, data: &[u8]) -> Result<()> {
    write_mplex_message(transport, MessageTag::Data, data).await
}

/// Multiplexed reader with buffering
pub struct MultiplexReader<T: Transport> {
    transport: T,
    #[allow(dead_code)]
    buffer: Vec<u8>,
    #[allow(dead_code)]
    buffer_pos: usize,
}

impl<T: Transport> MultiplexReader<T> {
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            buffer: Vec::new(),
            buffer_pos: 0,
        }
    }

    /// Read next multiplexed message
    pub async fn read_message(&mut self) -> Result<MultiplexMessage> {
        read_mplex_message(&mut self.transport).await
    }

    /// Read data message, skipping INFO/LOG messages
    pub async fn read_data(&mut self, buf: &mut [u8]) -> Result<usize> {
        loop {
            let msg = self.read_message().await?;

            match msg.tag {
                MessageTag::Data => {
                    let copy_len = msg.data.len().min(buf.len());
                    buf[..copy_len].copy_from_slice(&msg.data[..copy_len]);
                    return Ok(copy_len);
                }
                MessageTag::Info | MessageTag::Log => {
                    if let Ok(text) = String::from_utf8(msg.data) {
                        debug!("rsync: {}", text.trim());
                    }
                }
                MessageTag::Error | MessageTag::ErrorXfer => {
                    let error_msg = String::from_utf8_lossy(&msg.data);
                    anyhow::bail!("rsync error: {}", error_msg);
                }
                MessageTag::Warning => {
                    let warn_msg = String::from_utf8_lossy(&msg.data);
                    warn!("rsync warning: {}", warn_msg);
                }
                _ => {
                    debug!("Ignoring rsync message tag: {:?}", msg.tag);
                }
            }
        }
    }

    /// Expect a specific message tag
    pub async fn expect_tag(&mut self, expected: MessageTag) -> Result<Vec<u8>> {
        let msg = self.read_message().await?;

        if msg.tag == expected {
            Ok(msg.data)
        } else {
            anyhow::bail!("Expected tag {:?}, got {:?}", expected, msg.tag);
        }
    }

    /// Read multiple messages until end marker
    pub async fn read_until_empty(&mut self, expected_tag: MessageTag) -> Result<Vec<Vec<u8>>> {
        let mut messages = Vec::new();

        loop {
            let msg = self.read_message().await?;

            if msg.tag != expected_tag {
                anyhow::bail!("Expected tag {:?}, got {:?}", expected_tag, msg.tag);
            }

            if msg.data.is_empty() {
                // Empty message = end marker
                break;
            }

            messages.push(msg.data);
        }

        Ok(messages)
    }
}

/// Multiplexed writer
pub struct MultiplexWriter<T: Transport> {
    transport: T,
}

impl<T: Transport> MultiplexWriter<T> {
    pub fn new(transport: T) -> Self {
        Self { transport }
    }

    /// Get mutable reference to underlying transport
    pub fn transport_mut(&mut self) -> &mut T {
        &mut self.transport
    }
}

/// Bidirectional multiplex wrapper (can both read and write)
pub struct Multiplex<T: Transport> {
    transport: T,
    #[allow(dead_code)]
    read_buffer: Vec<u8>,
    #[allow(dead_code)]
    read_buffer_pos: usize,
}

impl<T: Transport> Multiplex<T> {
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            read_buffer: Vec::new(),
            read_buffer_pos: 0,
        }
    }

    /// Read a multiplexed message
    pub async fn read_message(&mut self) -> Result<MultiplexMessage> {
        read_mplex_message(&mut self.transport).await
    }

    /// Write a multiplexed message
    pub async fn write_message(&mut self, tag: MessageTag, data: &[u8]) -> Result<()> {
        write_mplex_message(&mut self.transport, tag, data).await
    }

    /// Get mutable reference to underlying transport
    pub fn transport_mut(&mut self) -> &mut T {
        &mut self.transport
    }
}

impl<T: Transport> MultiplexWriter<T> {
    /// Write a tagged message
    pub async fn write_message(&mut self, tag: MessageTag, data: &[u8]) -> Result<()> {
        write_mplex_message(&mut self.transport, tag, data).await
    }

    /// Write data message
    pub async fn write_data(&mut self, data: &[u8]) -> Result<()> {
        self.write_message(MessageTag::Data, data).await
    }

    /// Write info message
    pub async fn write_info(&mut self, message: &str) -> Result<()> {
        self.write_message(MessageTag::Info, message.as_bytes())
            .await
    }

    /// Write error message
    pub async fn write_error(&mut self, message: &str) -> Result<()> {
        self.write_message(MessageTag::Error, message.as_bytes())
            .await
    }

    /// Flush underlying transport
    pub async fn flush(&mut self) -> Result<()> {
        self.transport.flush().await.map_err(Into::into)
    }
}

// ============================================================================
// File List Encoding/Decoding (rsync format)
// ============================================================================

/// rsync file list flags (from flist.c)
mod file_flags {
    #[allow(dead_code)]
    pub const XMIT_TOP_DIR: u8 = 0x01; // Top-level directory
    #[allow(dead_code)]
    pub const XMIT_SAME_MODE: u8 = 0x02; // Mode unchanged
    #[allow(dead_code)]
    pub const XMIT_EXTENDED_FLAGS: u8 = 0x04; // Extended flags follow
    #[allow(dead_code)]
    pub const XMIT_SAME_UID: u8 = 0x10; // UID unchanged
    #[allow(dead_code)]
    pub const XMIT_SAME_GID: u8 = 0x20; // GID unchanged
    #[allow(dead_code)]
    pub const XMIT_SAME_NAME: u8 = 0x40; // Name matches previous (hardlink)
    pub const XMIT_LONG_NAME: u8 = 0x80; // Name > 255 bytes
}

/// Encode file list in rsync wire format (simplified - no delta encoding yet)
///
/// This sends each file as MSG_FLIST tagged message with varint-encoded fields.
/// Simplified version: no mtime deltas, no directory grouping.
pub async fn encode_file_list_rsync<T: Transport>(
    writer: &mut MultiplexWriter<T>,
    files: &[FileEntry],
) -> Result<()> {
    debug!("Encoding {} files in rsync format", files.len());

    for file in files {
        let mut entry = Vec::new();

        // Flags byte
        let mut flags = 0u8;
        if file.path.len() > 255 {
            flags |= file_flags::XMIT_LONG_NAME;
        }
        // For now, no delta encoding, so no SAME_MODE/SAME_UID/SAME_GID flags

        entry.push(flags);

        // Path (varint length + bytes)
        encode_varint_into(file.path.len() as u64, &mut entry);
        entry.extend(file.path.as_bytes());

        // File size (varint)
        encode_varint_into(file.size, &mut entry);

        // Mtime (varint, absolute - no delta encoding yet)
        // rsync uses unsigned for mtime, we need to convert
        let mtime_unsigned = if file.mtime < 0 {
            0u64 // Clamp negative times to 0
        } else {
            file.mtime as u64
        };
        encode_varint_into(mtime_unsigned, &mut entry);

        // Mode (varint)
        encode_varint_into(file.mode as u64, &mut entry);

        // uid/gid (varint)
        encode_varint_into(file.uid as u64, &mut entry);
        encode_varint_into(file.gid as u64, &mut entry);

        // Symlink target if applicable
        if file.is_symlink {
            if let Some(ref target) = file.symlink_target {
                encode_varint_into(target.len() as u64, &mut entry);
                entry.extend(target.as_bytes());
            }
        }

        // Send as MSG_FLIST tagged message
        writer.write_message(MessageTag::FList, &entry).await?;

        debug!("Encoded file: {} ({} bytes)", file.path, entry.len());
    }

    // End-of-list marker (empty MSG_FLIST)
    writer.write_message(MessageTag::FList, &[]).await?;
    debug!("Sent end-of-list marker");

    Ok(())
}

/// Decode file list from rsync wire format
///
/// Reads MSG_FLIST tagged messages until empty message (end marker).
pub async fn decode_file_list_rsync<T: Transport>(
    reader: &mut MultiplexReader<T>,
) -> Result<Vec<FileEntry>> {
    debug!("Decoding file list in rsync format");

    let mut files = Vec::new();

    loop {
        // Read next file list message
        let msg = reader.read_message().await?;

        if msg.tag != MessageTag::FList {
            anyhow::bail!("Expected MSG_FLIST, got {:?}", msg.tag);
        }

        if msg.data.is_empty() {
            // End of list marker
            debug!("Received end-of-list marker");
            break;
        }

        // Decode file entry from message data
        let file = decode_file_entry(&msg.data)?;
        debug!("Decoded file: {} ({} bytes)", file.path, file.size);
        files.push(file);
    }

    debug!("Decoded {} files total", files.len());
    Ok(files)
}

/// Decode a single file entry from bytes
pub fn decode_file_entry(data: &[u8]) -> Result<FileEntry> {
    let mut cursor = Cursor::new(data);

    // Flags byte
    let mut flags_buf = [0u8; 1];
    std::io::Read::read_exact(&mut cursor, &mut flags_buf)?;
    let _flags = flags_buf[0];

    // Path length and path
    let path_len = decode_varint_sync(&mut cursor)? as usize;
    let mut path_buf = vec![0u8; path_len];
    std::io::Read::read_exact(&mut cursor, &mut path_buf)?;
    let path = String::from_utf8(path_buf)?;

    // File size
    let size = decode_varint_sync(&mut cursor)?;

    // Mtime
    let mtime = decode_varint_sync(&mut cursor)? as i64;

    // Mode
    let mode = decode_varint_sync(&mut cursor)? as u32;

    // uid/gid
    let uid = decode_varint_sync(&mut cursor)? as u32;
    let gid = decode_varint_sync(&mut cursor)? as u32;

    // Check if symlink (we need to infer from mode bits)
    let is_symlink = (mode & 0o170000) == 0o120000; // S_IFLNK

    // Symlink target if applicable
    let symlink_target = if is_symlink {
        let target_len = decode_varint_sync(&mut cursor)? as usize;
        let mut target_buf = vec![0u8; target_len];
        std::io::Read::read_exact(&mut cursor, &mut target_buf)?;
        Some(String::from_utf8(target_buf)?)
    } else {
        None
    };

    Ok(FileEntry {
        path,
        size,
        mtime,
        mode,
        uid,
        gid,
        is_symlink,
        symlink_target,
    })
}

/// Decode varint from synchronous reader (for use with Cursor)
fn decode_varint_sync(reader: &mut Cursor<&[u8]>) -> Result<u64> {
    use std::io::Read;

    let mut result = 0u64;
    let mut shift = 0;

    loop {
        let mut byte_buf = [0u8; 1];
        reader.read_exact(&mut byte_buf)?;
        let byte = byte_buf[0];

        result |= ((byte & 0x7F) as u64) << shift;

        if (byte & 0x80) == 0 {
            break;
        }

        shift += 7;

        if shift > 63 {
            anyhow::bail!("Varint overflow");
        }
    }

    Ok(result)
}

// ============================================================================
// Checksum Exchange (rsync format)
// ============================================================================

use crate::protocol::checksum::{rolling_checksum_with_seed, strong_checksum};

/// Block checksum in rsync wire format
#[derive(Debug, Clone)]
pub struct RsyncBlockChecksum {
    pub weak: u32,
    pub strong: Vec<u8>, // Variable length (2 or 16 bytes)
}

/// Send block checksums in rsync wire format
///
/// rsync format:
/// - Header: [block_count][block_size][remainder][checksum2_length]
/// - Then each block: [weak][strong] (no offset/index - implicit!)
///
/// # Errors
///
/// Returns an error if writing to the transport fails.
pub async fn send_block_checksums_rsync<T: Transport>(
    writer: &mut MultiplexWriter<T>,
    data: &[u8],
    block_size: usize,
    seed: u32,
) -> Result<()> {
    if data.is_empty() {
        // Send empty checksum list
        let header = vec![
            0u8, 0, 0, 0, // block_count = 0
            0, 0, 0, 0, // block_size = 0
            0, 0, 0, 0, // remainder = 0
            16, 0, 0, 0, // checksum2_length = 16 (MD5)
        ];
        writer.write_message(MessageTag::Data, &header).await?;
        return Ok(());
    }

    let block_count = data.len().div_ceil(block_size);
    let remainder = data.len() % block_size;
    let checksum2_length = 16u32; // MD5 = 16 bytes

    // Build header
    let mut message = Vec::new();
    message.extend((block_count as u32).to_le_bytes());
    message.extend((block_size as u32).to_le_bytes());
    message.extend((remainder as u32).to_le_bytes());
    message.extend(checksum2_length.to_le_bytes());

    // Generate and append checksums
    let mut offset = 0;
    while offset < data.len() {
        let end = (offset + block_size).min(data.len());
        let block = &data[offset..end];

        // Compute checksums with seed
        let weak = rolling_checksum_with_seed(block, seed);
        let strong = strong_checksum(block);

        message.extend(weak.to_le_bytes());
        message.extend(strong);

        offset = end;
    }

    // Send as MSG_DATA
    writer.write_message(MessageTag::Data, &message).await?;
    debug!(
        "Sent {} block checksums (block_size={}, seed={})",
        block_count, block_size, seed
    );

    Ok(())
}

/// Receive block checksums in rsync wire format
///
/// # Errors
///
/// Returns an error if:
/// - Reading from the transport fails
/// - Invalid message tag received (expected MSG_DATA)
/// - Malformed checksum data
pub async fn receive_block_checksums_rsync<T: Transport>(
    reader: &mut MultiplexReader<T>,
) -> Result<(Vec<RsyncBlockChecksum>, usize)> {
    // Read MSG_DATA containing checksums
    let msg = reader.read_message().await?;

    if msg.tag != MessageTag::Data {
        anyhow::bail!("Expected MSG_DATA for checksums, got {:?}", msg.tag);
    }

    if msg.data.len() < 16 {
        // Empty or invalid checksum list
        return Ok((vec![], 0));
    }

    let mut cursor = Cursor::new(&msg.data[..]);

    // Read header
    let mut buf4 = [0u8; 4];

    std::io::Read::read_exact(&mut cursor, &mut buf4)?;
    let block_count = u32::from_le_bytes(buf4) as usize;

    std::io::Read::read_exact(&mut cursor, &mut buf4)?;
    let block_size = u32::from_le_bytes(buf4) as usize;

    std::io::Read::read_exact(&mut cursor, &mut buf4)?;
    let _remainder = u32::from_le_bytes(buf4);

    std::io::Read::read_exact(&mut cursor, &mut buf4)?;
    let checksum2_length = u32::from_le_bytes(buf4) as usize;

    debug!(
        "Receiving {} checksums (block_size={}, strong_len={})",
        block_count, block_size, checksum2_length
    );

    // Read checksums
    let mut checksums = Vec::with_capacity(block_count);
    for _ in 0..block_count {
        std::io::Read::read_exact(&mut cursor, &mut buf4)?;
        let weak = u32::from_le_bytes(buf4);

        let mut strong = vec![0u8; checksum2_length];
        std::io::Read::read_exact(&mut cursor, &mut strong)?;

        checksums.push(RsyncBlockChecksum { weak, strong });
    }

    Ok((checksums, block_size))
}

// ============================================================================
// Delta Token Encoding/Decoding (rsync format)
// ============================================================================

use crate::protocol::rsync::DeltaInstruction;

/// Convert delta instructions to rsync token stream
///
/// rsync token format:
/// - 0: End of data marker
/// - 1-96: Literal run (N bytes of data follow)
/// - 97-255: Block match with offset encoding
#[must_use]
pub fn delta_to_tokens(delta: &[DeltaInstruction]) -> Vec<u8> {
    let mut tokens = Vec::new();
    let mut last_block_index: i64 = -1;

    for instruction in delta {
        match instruction {
            DeltaInstruction::Literal(data) => {
                // Split into chunks of max 96 bytes
                for chunk in data.chunks(96) {
                    let token = chunk.len() as u8; // 1-96
                    tokens.push(token);
                    tokens.extend_from_slice(chunk);
                }
            }
            DeltaInstruction::BlockMatch {
                block_index,
                length: _,
            } => {
                // Calculate offset from last block
                let offset = if last_block_index < 0 {
                    *block_index as i64
                } else {
                    (*block_index as i64) - (last_block_index + 1)
                };

                // Encode as token
                if offset < 0 {
                    // Shouldn't happen, but handle it
                    tokens.push(97); // Offset 0
                } else if offset < 16 {
                    // Simple encoding: 97 + offset
                    tokens.push(97 + offset as u8);
                } else {
                    // Complex encoding for large offsets
                    // For now, use simplified version
                    // TODO: Implement full rsync offset encoding
                    let bit_count = 64 - (offset as u64).leading_zeros();
                    let token = 97 + ((bit_count as u8) << 4);
                    tokens.push(token);

                    // Send offset bytes (simplified)
                    tokens.extend((offset as u32).to_le_bytes());
                }

                last_block_index = *block_index as i64;
            }
        }
    }

    // End of data marker
    tokens.push(0);

    tokens
}

/// Parse rsync token stream back to delta instructions
///
/// # Errors
///
/// Returns an error if:
/// - Literal data is truncated (insufficient bytes in token stream)
/// - Block offset data is truncated
pub fn tokens_to_delta(
    tokens: &[u8],
    block_checksums: &[RsyncBlockChecksum],
) -> anyhow::Result<Vec<DeltaInstruction>> {
    let mut instructions = Vec::new();
    let mut pos = 0;
    let mut last_block_index: i64 = -1;

    while pos < tokens.len() {
        let token = tokens[pos];
        pos += 1;

        if token == 0 {
            // End of data
            break;
        } else if token <= 96 {
            // Literal run
            let literal_len = token as usize;
            if pos + literal_len > tokens.len() {
                anyhow::bail!("Literal data truncated");
            }

            let literal_data = tokens[pos..pos + literal_len].to_vec();
            pos += literal_len;

            instructions.push(DeltaInstruction::Literal(literal_data));
        } else {
            // Block match (97-255)
            let offset = if token < 113 {
                // Simple encoding: offset = token - 97
                (token - 97) as i64
            } else {
                // Complex encoding
                let _bit_count = ((token - 97) >> 4) as usize;

                if pos + 4 > tokens.len() {
                    anyhow::bail!("Block offset truncated");
                }

                let mut offset_bytes = [0u8; 4];
                offset_bytes.copy_from_slice(&tokens[pos..pos + 4]);
                pos += 4;

                u32::from_le_bytes(offset_bytes) as i64
            };

            // Calculate absolute block index
            let block_index = if last_block_index < 0 {
                offset as u32
            } else {
                ((last_block_index + 1) + offset) as u32
            };

            // Get block length from checksums
            let length = if (block_index as usize) < block_checksums.len() {
                block_checksums[block_index as usize].strong.len() as u32
            } else {
                4096 // Default block size if not in checksums
            };

            instructions.push(DeltaInstruction::BlockMatch {
                block_index,
                length,
            });

            last_block_index = block_index as i64;
        }
    }

    Ok(instructions)
}

/// Send delta in rsync token format
///
/// # Errors
///
/// Returns an error if writing to the transport fails.
pub async fn send_delta_rsync<T: Transport>(
    writer: &mut MultiplexWriter<T>,
    delta: &[DeltaInstruction],
) -> Result<()> {
    let tokens = delta_to_tokens(delta);
    writer.write_message(MessageTag::Data, &tokens).await?;
    debug!("Sent {} bytes of delta tokens", tokens.len());
    Ok(())
}

/// Receive delta in rsync token format
///
/// # Errors
///
/// Returns an error if:
/// - Reading from the transport fails
/// - Invalid message tag received (expected MSG_DATA)
/// - Token parsing fails (malformed delta)
pub async fn receive_delta_rsync<T: Transport>(
    reader: &mut MultiplexReader<T>,
    checksums: &[RsyncBlockChecksum],
) -> Result<Vec<DeltaInstruction>> {
    let msg = reader.read_message().await?;

    if msg.tag != MessageTag::Data {
        anyhow::bail!("Expected MSG_DATA for delta, got {:?}", msg.tag);
    }

    tokens_to_delta(&msg.data, checksums)
}

// ============================================================================
// rsync-Compatible Protocol Functions (File List Exchange Only - Minimal)
// Generic over any Transport (SSH, pipes, etc.)
// ============================================================================

/// Receive files from rsync sender (file list only - minimal implementation)
///
/// Generic over any Transport implementation (SSH, pipes, etc.)
///
/// # Errors
///
/// Returns an error if:
/// - Transport operations fail
/// - File list decoding fails
/// - File system operations fail (creating directories/files)
pub async fn rsync_receive<T: Transport>(
    _args: &Args,
    transport: T,
    dest_path: &Path,
) -> Result<SyncStats> {
    let start = Instant::now();
    info!("rsync-compat receiver: Starting (file list only)");

    // Wrap transport in multiplex reader
    let mut reader = MultiplexReader::new(transport);

    // Phase 1: Handshake (simplified - just version, no seed yet)
    debug!("rsync-compat: Reading version from rsync sender");
    // rsync sends version as raw byte (NOT tagged)
    // We need to read it directly from transport, not via multiplex
    // TODO: This is a challenge - need access to raw transport

    // Phase 2: Receive file list
    info!("rsync-compat: Receiving file list from rsync");
    let files = decode_file_list_rsync(&mut reader).await?;
    info!("rsync-compat: Received {} files from rsync", files.len());

    // Print file list for validation
    for file in &files {
        info!(
            "  File: {} ({} bytes, mode {:o})",
            file.path, file.size, file.mode
        );
    }

    // Phase 3: For now, just create empty files (no actual transfer yet)
    info!("rsync-compat: Creating file structure (no content yet)");
    for file in &files {
        let file_path = dest_path.join(&file.path);

        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)?;
        }

        if file.is_symlink {
            if let Some(ref target) = file.symlink_target {
                #[cfg(unix)]
                {
                    use std::os::unix::fs as unix_fs;
                    unix_fs::symlink(target, &file_path)?;
                }
            }
        } else {
            // Create empty placeholder
            fs::write(&file_path, b"")?;
        }
    }

    info!("rsync-compat: File list exchange complete (content transfer not implemented)");

    Ok(SyncStats {
        files_copied: files.len() as u64,
        bytes_copied: 0, // No actual content transferred yet
        duration: start.elapsed(),
    })
}

/// Send files to rsync receiver (file list only - minimal implementation)
///
/// Generic over any Transport implementation (SSH, pipes, etc.)
///
/// # Errors
///
/// Returns an error if:
/// - File system operations fail (reading source files)
/// - File list encoding fails
/// - Transport write operations fail
pub async fn rsync_send<T: Transport>(
    args: &Args,
    source_path: &Path,
    transport: T,
) -> Result<SyncStats> {
    let start = Instant::now();
    info!("rsync-compat sender: Starting (file list only)");

    // Wrap transport in multiplex writer
    let mut writer = MultiplexWriter::new(transport);

    // Phase 1: Handshake (simplified)
    // TODO: Send version byte (untagged)
    debug!("rsync-compat: Sending handshake");

    // Phase 2: Generate and send file list
    info!(
        "rsync-compat: Generating file list from {}",
        source_path.display()
    );
    let files = generate_file_list_simple(source_path, args)?;
    info!("rsync-compat: Sending {} files to rsync", files.len());

    encode_file_list_rsync(&mut writer, &files).await?;

    info!("rsync-compat: File list sent");

    // Phase 3: For now, no actual file transfer
    info!("rsync-compat: File list exchange complete (content transfer not implemented)");

    Ok(SyncStats {
        files_copied: files.len() as u64,
        bytes_copied: 0, // No actual content transferred yet
        duration: start.elapsed(),
    })
}

/// Generate file list (reuse from rsync.rs)
fn generate_file_list_simple(path: &Path, args: &Args) -> Result<Vec<FileEntry>> {
    let mut files = Vec::new();

    if path.is_file() {
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
            mode: {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    metadata.permissions().mode()
                }
                #[cfg(not(unix))]
                {
                    0o644
                }
            },
            uid: {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt;
                    metadata.uid()
                }
                #[cfg(not(unix))]
                {
                    0
                }
            },
            gid: {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt;
                    metadata.gid()
                }
                #[cfg(not(unix))]
                {
                    0
                }
            },
            is_symlink: false,
            symlink_target: None,
        });
    } else if path.is_dir() && args.should_recurse() {
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
                    mode: {
                        #[cfg(unix)]
                        {
                            use std::os::unix::fs::PermissionsExt;
                            metadata.permissions().mode()
                        }
                        #[cfg(not(unix))]
                        {
                            0o644
                        }
                    },
                    uid: {
                        #[cfg(unix)]
                        {
                            use std::os::unix::fs::MetadataExt;
                            metadata.uid()
                        }
                        #[cfg(not(unix))]
                        {
                            0
                        }
                    },
                    gid: {
                        #[cfg(unix)]
                        {
                            use std::os::unix::fs::MetadataExt;
                            metadata.gid()
                        }
                        #[cfg(not(unix))]
                        {
                            0
                        }
                    },
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
                    size: 0,
                    mtime: metadata
                        .modified()?
                        .duration_since(std::time::UNIX_EPOCH)?
                        .as_secs() as i64,
                    mode: {
                        #[cfg(unix)]
                        {
                            use std::os::unix::fs::PermissionsExt;
                            metadata.permissions().mode()
                        }
                        #[cfg(not(unix))]
                        {
                            0o120777
                        }
                    },
                    uid: {
                        #[cfg(unix)]
                        {
                            use std::os::unix::fs::MetadataExt;
                            metadata.uid()
                        }
                        #[cfg(not(unix))]
                        {
                            0
                        }
                    },
                    gid: {
                        #[cfg(unix)]
                        {
                            use std::os::unix::fs::MetadataExt;
                            metadata.gid()
                        }
                        #[cfg(not(unix))]
                        {
                            0
                        }
                    },
                    is_symlink: true,
                    symlink_target: Some(symlink_target),
                });
            }
        }
    }

    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_tag_parsing() {
        assert_eq!(MessageTag::from_u8(7), Some(MessageTag::Data));
        assert_eq!(MessageTag::from_u8(9), Some(MessageTag::Info));
        assert_eq!(MessageTag::from_u8(10), Some(MessageTag::Error));
        assert_eq!(MessageTag::from_u8(255), None);
    }

    #[test]
    fn test_length_encoding() {
        // rsync uses 3-byte length encoding
        let length = 12345u32;
        let bytes = length.to_le_bytes();
        let encoded = [bytes[0], bytes[1], bytes[2]];

        let decoded = u32::from_le_bytes([encoded[0], encoded[1], encoded[2], 0]);
        assert_eq!(decoded, length);
    }

    #[test]
    fn test_file_entry_roundtrip() {
        // Create a test file entry
        let original = FileEntry {
            path: "test/file.txt".to_string(),
            size: 12345,
            mtime: 1696800000,
            mode: 0o100644,
            uid: 1000,
            gid: 1000,
            is_symlink: false,
            symlink_target: None,
        };

        // Encode it manually (simulating what encode_file_list_rsync does)
        let mut entry = Vec::new();
        entry.push(0u8); // flags
        encode_varint_into(original.path.len() as u64, &mut entry);
        entry.extend(original.path.as_bytes());
        encode_varint_into(original.size, &mut entry);
        encode_varint_into(original.mtime as u64, &mut entry);
        encode_varint_into(original.mode as u64, &mut entry);
        encode_varint_into(original.uid as u64, &mut entry);
        encode_varint_into(original.gid as u64, &mut entry);

        // Decode it
        let decoded = decode_file_entry(&entry).expect("Should decode");

        assert_eq!(decoded.path, original.path);
        assert_eq!(decoded.size, original.size);
        assert_eq!(decoded.mtime, original.mtime);
        assert_eq!(decoded.mode, original.mode);
        assert_eq!(decoded.uid, original.uid);
        assert_eq!(decoded.gid, original.gid);
    }

    #[test]
    fn test_symlink_entry() {
        let symlink = FileEntry {
            path: "link".to_string(),
            size: 0,
            mtime: 1696800000,
            mode: 0o120777, // S_IFLNK | 0777
            uid: 1000,
            gid: 1000,
            is_symlink: true,
            symlink_target: Some("target/path".to_string()),
        };

        // Encode manually
        let mut entry = Vec::new();
        entry.push(0u8);
        encode_varint_into(symlink.path.len() as u64, &mut entry);
        entry.extend(symlink.path.as_bytes());
        encode_varint_into(symlink.size, &mut entry);
        encode_varint_into(symlink.mtime as u64, &mut entry);
        encode_varint_into(symlink.mode as u64, &mut entry);
        encode_varint_into(symlink.uid as u64, &mut entry);
        encode_varint_into(symlink.gid as u64, &mut entry);
        let target = symlink.symlink_target.as_ref().unwrap();
        encode_varint_into(target.len() as u64, &mut entry);
        entry.extend(target.as_bytes());

        // Decode
        let decoded = decode_file_entry(&entry).expect("Should decode");

        assert!(decoded.is_symlink);
        assert_eq!(decoded.symlink_target.as_ref().unwrap(), "target/path");
    }
}
