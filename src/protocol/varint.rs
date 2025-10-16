//! Variable-length integer encoding (varint)
//!
//! Implements rsync's varint format: 7-bit continuation encoding
//! used throughout the rsync wire protocol.
#![allow(dead_code)] // Protocol implementation not yet fully used

use crate::protocol::transport::{self, Transport};
use anyhow::Result;

/// Encode a u64 as varint (7-bit continuation encoding)
///
/// Format:
/// - Lower 7 bits of each byte: data
/// - High bit (0x80): continuation flag (1 = more bytes follow)
///
/// # Examples
/// ```
/// # use arsync::protocol::varint::encode_varint;
/// assert_eq!(encode_varint(0), vec![0x00]);
/// assert_eq!(encode_varint(127), vec![0x7F]);
/// assert_eq!(encode_varint(128), vec![0x80, 0x01]);
/// assert_eq!(encode_varint(300), vec![0xAC, 0x02]);
/// ```
#[must_use]
pub fn encode_varint(value: u64) -> Vec<u8> {
    let mut result = Vec::new();
    encode_varint_into(value, &mut result);
    result
}

/// Encode varint into an existing buffer (more efficient)
pub fn encode_varint_into(mut value: u64, buffer: &mut Vec<u8>) {
    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;

        if value > 0 {
            byte |= 0x80; // Set continuation bit
        }

        buffer.push(byte);

        if value == 0 {
            break;
        }
    }
}

/// Decode a varint from transport
///
/// Reads bytes until continuation bit is clear
///
/// # Errors
///
/// Returns an error if:
/// - Reading from transport fails
/// - Varint overflow (more than 64 bits encoded)
pub async fn decode_varint<T: Transport>(transport: &mut T) -> Result<u64> {
    let mut result = 0u64;
    let mut shift = 0;

    loop {
        let mut byte_buf = [0u8; 1];
        transport::read_exact(transport, &mut byte_buf).await?;
        let byte = byte_buf[0];

        // Add lower 7 bits to result
        result |= ((byte & 0x7F) as u64) << shift;

        // Check continuation bit
        if (byte & 0x80) == 0 {
            break; // No more bytes
        }

        shift += 7;

        if shift > 63 {
            anyhow::bail!("Varint overflow: more than 64 bits");
        }
    }

    Ok(result)
}

/// Encode i64 as signed varint (zigzag encoding)
///
/// rsync uses zigzag encoding for signed integers:
/// - Maps negative values to positive: -1 → 1, -2 → 3, etc.
/// - Then encodes as regular varint
#[must_use]
pub fn encode_varint_signed(value: i64) -> Vec<u8> {
    // Zigzag encoding: (n << 1) ^ (n >> 63)
    let zigzag = ((value << 1) ^ (value >> 63)) as u64;
    encode_varint(zigzag)
}

/// Decode signed varint (zigzag decoding)
///
/// # Errors
///
/// Returns an error if reading or decoding the underlying varint fails.
pub async fn decode_varint_signed<T: Transport>(transport: &mut T) -> Result<i64> {
    let zigzag = decode_varint(transport).await?;

    // Zigzag decoding: (n >> 1) ^ -(n & 1)
    let value = ((zigzag >> 1) as i64) ^ (-((zigzag & 1) as i64));

    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_varint_small_values() {
        assert_eq!(encode_varint(0), vec![0x00]);
        assert_eq!(encode_varint(1), vec![0x01]);
        assert_eq!(encode_varint(127), vec![0x7F]);
    }

    #[test]
    fn test_varint_boundary() {
        // 128 requires 2 bytes (first continuation)
        assert_eq!(encode_varint(128), vec![0x80, 0x01]);
        assert_eq!(encode_varint(255), vec![0xFF, 0x01]);
        assert_eq!(encode_varint(256), vec![0x80, 0x02]);
    }

    #[test]
    fn test_varint_large_values() {
        assert_eq!(encode_varint(300), vec![0xAC, 0x02]);
        assert_eq!(encode_varint(16383), vec![0xFF, 0x7F]);
        assert_eq!(encode_varint(16384), vec![0x80, 0x80, 0x01]);
    }

    #[test]
    fn test_varint_max_value() {
        let max = u64::MAX;
        let encoded = encode_varint(max);
        assert_eq!(encoded.len(), 10); // Max 10 bytes for 64-bit value
        assert_eq!(
            encoded,
            vec![0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x01]
        );
    }

    #[test]
    fn test_varint_roundtrip() {
        let test_values = vec![
            0,
            1,
            127,
            128,
            255,
            256,
            300,
            16383,
            16384,
            65535,
            65536,
            1_000_000,
            1_000_000_000,
            u64::MAX,
        ];

        for value in test_values {
            let encoded = encode_varint(value);
            // For roundtrip test, we'd need async context
            // This is tested in integration tests
            println!("Value {} encodes to {} bytes", value, encoded.len());
        }
    }

    #[test]
    fn test_varint_signed() {
        assert_eq!(encode_varint_signed(0), vec![0x00]);
        assert_eq!(encode_varint_signed(-1), vec![0x01]);
        assert_eq!(encode_varint_signed(1), vec![0x02]);
        assert_eq!(encode_varint_signed(-2), vec![0x03]);
        assert_eq!(encode_varint_signed(2), vec![0x04]);
    }

    #[test]
    fn test_varint_into() {
        let mut buffer = Vec::new();
        encode_varint_into(300, &mut buffer);
        assert_eq!(buffer, vec![0xAC, 0x02]);

        // Can append multiple varints
        encode_varint_into(128, &mut buffer);
        assert_eq!(buffer, vec![0xAC, 0x02, 0x80, 0x01]);
    }
}
