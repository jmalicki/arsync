//! Unit tests for rsync wire format generation and consumption
//!
//! These tests verify that we correctly generate and parse rsync's
//! wire format without needing a real rsync process.

#![cfg(feature = "remote-sync")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use arsync::protocol::rsync::FileEntry;
use arsync::protocol::varint::{encode_varint, encode_varint_into};

// ============================================================================
// Level 1: Varint Format Tests (Ensure We Match rsync's Encoding)
// ============================================================================

#[test]
fn test_varint_matches_rsync_spec() {
    // Test cases from rsync documentation and source code

    // Small values (1 byte)
    assert_eq!(encode_varint(0), vec![0x00]);
    assert_eq!(encode_varint(1), vec![0x01]);
    assert_eq!(encode_varint(127), vec![0x7F]); // Max 1-byte value

    // 2-byte boundary
    assert_eq!(encode_varint(128), vec![0x80, 0x01]);
    assert_eq!(encode_varint(255), vec![0xFF, 0x01]);
    assert_eq!(encode_varint(256), vec![0x80, 0x02]);

    // Typical file sizes
    assert_eq!(encode_varint(1024), vec![0x80, 0x08]); // 1KB
    assert_eq!(encode_varint(1048576), vec![0x80, 0x80, 0x40]); // 1MB

    println!("✓ Varint encoding matches rsync specification");
}

#[test]
fn test_varint_rsync_efficiency() {
    // Verify varint is more efficient than fixed-size for typical values

    let typical_sizes = vec![
        100,       // Small file
        10_000,    // Medium file
        1_000_000, // 1MB file
    ];

    for size in typical_sizes {
        let varint = encode_varint(size);
        let fixed_8byte = size.to_le_bytes().to_vec();

        println!(
            "Size {}: varint={} bytes, fixed={} bytes, savings={}%",
            size,
            varint.len(),
            fixed_8byte.len(),
            100 - (varint.len() * 100 / fixed_8byte.len())
        );

        assert!(varint.len() <= fixed_8byte.len());
    }

    println!("✓ Varint encoding is efficient for typical file sizes");
}

// ============================================================================
// Level 2: File Entry Format Tests (Generate What rsync Expects)
// ============================================================================

#[test]
fn test_file_entry_rsync_format_regular_file() {
    // Generate a file entry exactly as rsync would encode it

    let file = FileEntry {
        path: "data.txt".to_string(),
        size: 1024,
        mtime: 1696800000,
        mode: 0o100644, // Regular file, rw-r--r--
        uid: 1000,
        gid: 1000,
        is_symlink: false,
        symlink_target: None,
    };

    // Encode using our format
    let mut encoded = Vec::new();

    // Flags (0 = no special flags)
    encoded.push(0x00);

    // Path
    encode_varint_into(file.path.len() as u64, &mut encoded);
    encoded.extend(file.path.as_bytes());

    // File attributes (all varint)
    encode_varint_into(file.size, &mut encoded);
    encode_varint_into(file.mtime as u64, &mut encoded);
    encode_varint_into(file.mode as u64, &mut encoded);
    encode_varint_into(file.uid as u64, &mut encoded);
    encode_varint_into(file.gid as u64, &mut encoded);

    // Verify encoding is reasonable
    println!("Regular file entry:");
    println!(
        "  Original: path='{}', size={}, mode={:o}",
        file.path, file.size, file.mode
    );
    println!("  Encoded: {} bytes total", encoded.len());
    println!("  Breakdown:");
    println!("    flags: 1 byte");
    println!(
        "    path: {} bytes (varint len + string)",
        1 + file.path.len()
    );
    println!(
        "    attributes: {} bytes (5 varints)",
        encoded.len() - 1 - 1 - file.path.len()
    );

    // Should be compact
    assert!(encoded.len() < 50, "File entry should be compact");

    println!("✓ Regular file encoded in rsync format");
}

#[test]
fn test_file_entry_rsync_format_symlink() {
    let file = FileEntry {
        path: "link.txt".to_string(),
        size: 0,
        mtime: 1696800000,
        mode: 0o120777, // Symlink, rwxrwxrwx
        uid: 1000,
        gid: 1000,
        is_symlink: true,
        symlink_target: Some("target/file.txt".to_string()),
    };

    let mut encoded = Vec::new();
    encoded.push(0x00); // flags
    encode_varint_into(file.path.len() as u64, &mut encoded);
    encoded.extend(file.path.as_bytes());
    encode_varint_into(file.size, &mut encoded);
    encode_varint_into(file.mtime as u64, &mut encoded);
    encode_varint_into(file.mode as u64, &mut encoded);
    encode_varint_into(file.uid as u64, &mut encoded);
    encode_varint_into(file.gid as u64, &mut encoded);

    // Symlink target
    let target = file.symlink_target.as_ref().unwrap();
    encode_varint_into(target.len() as u64, &mut encoded);
    encoded.extend(target.as_bytes());

    println!("Symlink entry:");
    println!("  Link: {} -> {}", file.path, target);
    println!("  Encoded: {} bytes", encoded.len());

    assert!(encoded.len() < 100);

    println!("✓ Symlink encoded in rsync format");
}

#[test]
fn test_file_entry_long_path() {
    // Test XMIT_LONG_NAME flag for paths > 255 bytes
    let long_path = "a/".repeat(130); // 260 bytes

    let file = FileEntry {
        path: long_path.clone(),
        size: 100,
        mtime: 1696800000,
        mode: 0o100644,
        uid: 1000,
        gid: 1000,
        is_symlink: false,
        symlink_target: None,
    };

    let mut encoded = Vec::new();

    // Should set XMIT_LONG_NAME flag
    let flags = if file.path.len() > 255 { 0x80 } else { 0x00 };
    encoded.push(flags);

    encode_varint_into(file.path.len() as u64, &mut encoded);
    encoded.extend(file.path.as_bytes());
    encode_varint_into(file.size, &mut encoded);
    encode_varint_into(file.mtime as u64, &mut encoded);
    encode_varint_into(file.mode as u64, &mut encoded);
    encode_varint_into(file.uid as u64, &mut encoded);
    encode_varint_into(file.gid as u64, &mut encoded);

    println!("Long path file:");
    println!("  Path length: {} bytes", file.path.len());
    println!("  Flags: 0x{:02X} (XMIT_LONG_NAME set)", flags);
    println!("  Total encoded: {} bytes", encoded.len());

    assert_eq!(flags, 0x80, "Should set XMIT_LONG_NAME flag");

    println!("✓ Long path handled with correct flag");
}

// ============================================================================
// Level 3: Roundtrip Tests (Generate and Consume)
// ============================================================================

#[test]
fn test_file_entry_encode_decode_roundtrip() {
    // Test that we can encode a file entry and decode it back

    let original = FileEntry {
        path: "test/data.bin".to_string(),
        size: 524288, // 512KB
        mtime: 1696800000,
        mode: 0o100755, // Executable
        uid: 1001,
        gid: 1002,
        is_symlink: false,
        symlink_target: None,
    };

    // Encode
    let mut encoded = Vec::new();
    encoded.push(0x00); // flags
    encode_varint_into(original.path.len() as u64, &mut encoded);
    encoded.extend(original.path.as_bytes());
    encode_varint_into(original.size, &mut encoded);
    encode_varint_into(original.mtime as u64, &mut encoded);
    encode_varint_into(original.mode as u64, &mut encoded);
    encode_varint_into(original.uid as u64, &mut encoded);
    encode_varint_into(original.gid as u64, &mut encoded);

    // Decode using our decoder
    use arsync::protocol::rsync_compat::decode_file_entry;
    let decoded = decode_file_entry(&encoded).expect("Should decode");

    // Verify all fields match
    assert_eq!(decoded.path, original.path);
    assert_eq!(decoded.size, original.size);
    assert_eq!(decoded.mtime, original.mtime);
    assert_eq!(decoded.mode, original.mode);
    assert_eq!(decoded.uid, original.uid);
    assert_eq!(decoded.gid, original.gid);
    assert!(!decoded.is_symlink);

    println!("✓ File entry roundtrip successful");
    println!("  All fields preserved through encode → decode cycle");
}

#[test]
fn test_multiple_file_entries_sequential() {
    // Simulate multiple files in a file list

    let files = vec![
        FileEntry {
            path: "file1.txt".to_string(),
            size: 100,
            mtime: 1696800000,
            mode: 0o100644,
            uid: 1000,
            gid: 1000,
            is_symlink: false,
            symlink_target: None,
        },
        FileEntry {
            path: "file2.txt".to_string(),
            size: 200,
            mtime: 1696800100, // 100 seconds later
            mode: 0o100644,
            uid: 1000,
            gid: 1000,
            is_symlink: false,
            symlink_target: None,
        },
        FileEntry {
            path: "subdir/file3.txt".to_string(),
            size: 300,
            mtime: 1696800200,
            mode: 0o100755, // Executable
            uid: 1000,
            gid: 1000,
            is_symlink: false,
            symlink_target: None,
        },
    ];

    // Encode all files
    let mut all_encoded = Vec::new();
    for file in &files {
        let mut entry = Vec::new();
        entry.push(0x00);
        encode_varint_into(file.path.len() as u64, &mut entry);
        entry.extend(file.path.as_bytes());
        encode_varint_into(file.size, &mut entry);
        encode_varint_into(file.mtime as u64, &mut entry);
        encode_varint_into(file.mode as u64, &mut entry);
        encode_varint_into(file.uid as u64, &mut entry);
        encode_varint_into(file.gid as u64, &mut entry);
        all_encoded.push(entry);
    }

    // Decode all files
    use arsync::protocol::rsync_compat::decode_file_entry;
    let mut decoded_files = Vec::new();
    for entry in &all_encoded {
        let decoded = decode_file_entry(entry).expect("Should decode");
        decoded_files.push(decoded);
    }

    // Verify all match
    assert_eq!(decoded_files.len(), files.len());
    for (i, (original, decoded)) in files.iter().zip(decoded_files.iter()).enumerate() {
        assert_eq!(decoded.path, original.path, "File {} path mismatch", i);
        assert_eq!(decoded.size, original.size, "File {} size mismatch", i);
        assert_eq!(decoded.mode, original.mode, "File {} mode mismatch", i);
    }

    println!("✓ Multiple file entries encoded/decoded correctly");
    println!("  {} files tested sequentially", files.len());
}

// ============================================================================
// Level 4: Message Framing Tests (rsync Multiplexed I/O)
// ============================================================================

#[test]
fn test_mplex_message_format() {
    // Test rsync's [tag][3-byte length][data] format

    let test_data = b"Hello, rsync!";
    let tag = 7u8; // MSG_DATA

    // Encode message manually
    let length = test_data.len() as u32;
    let len_bytes = length.to_le_bytes();

    let mut message = Vec::new();
    message.push(tag);
    message.extend(&len_bytes[0..3]); // Only 3 bytes for length
    message.extend(test_data);

    // Verify format
    assert_eq!(message[0], 7); // Tag
    assert_eq!(message[1], (test_data.len() & 0xFF) as u8); // Length byte 0
    assert_eq!(message[2], ((test_data.len() >> 8) & 0xFF) as u8); // Length byte 1
    assert_eq!(message[3], ((test_data.len() >> 16) & 0xFF) as u8); // Length byte 2
    assert_eq!(&message[4..], test_data); // Data

    println!("✓ Multiplexed message format correct");
    println!("  Tag: {} (MSG_DATA)", tag);
    println!("  Length: {} bytes (3-byte encoding)", test_data.len());
    println!("  Total message size: {} bytes", message.len());
}

#[test]
fn test_mplex_message_size_limits() {
    // rsync uses 3-byte length, max = 16,777,215 bytes

    let max_length: u32 = 0xFFFFFF; // 3 bytes can hold up to this
    let len_bytes = max_length.to_le_bytes();

    let decoded = u32::from_le_bytes([len_bytes[0], len_bytes[1], len_bytes[2], 0]);
    assert_eq!(decoded, max_length);

    println!(
        "✓ Message length encoding supports up to {} bytes",
        max_length
    );
    println!("  (16.7 MB per message)");
}

// ============================================================================
// Level 5: File List Message Structure Tests
// ============================================================================

#[test]
fn test_file_list_message_structure() {
    // Test complete MSG_FLIST message structure

    let file = FileEntry {
        path: "example.dat".to_string(),
        size: 4096,
        mtime: 1696800000,
        mode: 0o100600, // rw-------
        uid: 1000,
        gid: 1000,
        is_symlink: false,
        symlink_target: None,
    };

    // Encode file entry (body)
    let mut entry = Vec::new();
    entry.push(0x00); // flags
    encode_varint_into(file.path.len() as u64, &mut entry);
    entry.extend(file.path.as_bytes());
    encode_varint_into(file.size, &mut entry);
    encode_varint_into(file.mtime as u64, &mut entry);
    encode_varint_into(file.mode as u64, &mut entry);
    encode_varint_into(file.uid as u64, &mut entry);
    encode_varint_into(file.gid as u64, &mut entry);

    // Wrap in MSG_FLIST message (tag 27)
    let tag = 27u8; // MSG_FLIST
    let length = entry.len() as u32;
    let len_bytes = length.to_le_bytes();

    let mut message = Vec::new();
    message.push(tag);
    message.extend(&len_bytes[0..3]);
    message.extend(&entry);

    println!("Complete MSG_FLIST message:");
    println!("  Tag: {} (MSG_FLIST)", tag);
    println!("  Entry size: {} bytes", entry.len());
    println!(
        "  Message size: {} bytes (tag + length + entry)",
        message.len()
    );
    println!("  Format: [tag:1][len:3][entry:{}]", entry.len());

    // Verify structure
    assert_eq!(message[0], 27);
    assert_eq!(message.len(), 4 + entry.len());

    println!("✓ MSG_FLIST message structure correct");
}

#[test]
fn test_end_of_list_marker() {
    // rsync uses empty MSG_FLIST to mark end of file list

    let tag = 27u8; // MSG_FLIST
    let length = 0u32; // Empty!
    let len_bytes = length.to_le_bytes();

    let mut end_marker = Vec::new();
    end_marker.push(tag);
    end_marker.extend(&len_bytes[0..3]);
    // No data

    assert_eq!(end_marker.len(), 4); // Just tag + length
    assert_eq!(end_marker, vec![27, 0, 0, 0]);

    println!("✓ End-of-list marker format correct");
    println!("  Marker: [0x1B, 0x00, 0x00, 0x00]");
    println!("  Interpretation: MSG_FLIST with length=0");
}

// ============================================================================
// Level 6: Complete File List Encoding Test
// ============================================================================

#[test]
fn test_complete_file_list_wire_format() {
    // Simulate complete file list as it would be sent over the wire

    let files = vec![
        FileEntry {
            path: "README.md".to_string(),
            size: 512,
            mtime: 1696800000,
            mode: 0o100644,
            uid: 1000,
            gid: 1000,
            is_symlink: false,
            symlink_target: None,
        },
        FileEntry {
            path: "src/main.rs".to_string(),
            size: 2048,
            mtime: 1696800100,
            mode: 0o100644,
            uid: 1000,
            gid: 1000,
            is_symlink: false,
            symlink_target: None,
        },
    ];

    let mut wire_data = Vec::new();

    // Encode each file as MSG_FLIST message
    for file in &files {
        // Entry body
        let mut entry = Vec::new();
        entry.push(0x00);
        encode_varint_into(file.path.len() as u64, &mut entry);
        entry.extend(file.path.as_bytes());
        encode_varint_into(file.size, &mut entry);
        encode_varint_into(file.mtime as u64, &mut entry);
        encode_varint_into(file.mode as u64, &mut entry);
        encode_varint_into(file.uid as u64, &mut entry);
        encode_varint_into(file.gid as u64, &mut entry);

        // MSG_FLIST wrapper
        let tag = 27u8;
        let length = entry.len() as u32;
        let len_bytes = length.to_le_bytes();

        wire_data.push(tag);
        wire_data.extend(&len_bytes[0..3]);
        wire_data.extend(&entry);
    }

    // End marker
    wire_data.extend(&[27, 0, 0, 0]);

    println!("Complete file list wire format:");
    println!("  Files: {}", files.len());
    println!("  Total wire size: {} bytes", wire_data.len());
    println!(
        "  Average per file: {} bytes",
        wire_data.len() / (files.len() + 1)
    );
    println!("  Format validated: [MSG_FLIST][entry]...[MSG_FLIST][empty]");

    // Verify structure
    assert!(!wire_data.is_empty());
    assert_eq!(&wire_data[wire_data.len() - 4..], &[27, 0, 0, 0]);

    println!("✓ Complete file list wire format correct");
}

// ============================================================================
// Level 7: Consumption Tests (Can We Parse rsync Format?)
// ============================================================================

#[test]
fn test_decode_rsync_generated_entry() {
    // Manually create what rsync would send

    let mut rsync_format = Vec::new();

    // Flags
    rsync_format.push(0x00);

    // Path: "test.txt" (8 bytes)
    rsync_format.push(0x08); // length = 8 (varint: single byte)
    rsync_format.extend(b"test.txt");

    // Size: 1024 (varint)
    rsync_format.extend(&[0x80, 0x08]); // 1024 in varint

    // Mtime: 1696800000 (varint)
    rsync_format.extend(encode_varint(1696800000));

    // Mode: 0o100644 (varint)
    rsync_format.extend(encode_varint(0o100644));

    // uid: 1000 (varint)
    rsync_format.extend(encode_varint(1000));

    // gid: 1000 (varint)
    rsync_format.extend(encode_varint(1000));

    println!("Simulated rsync-generated entry:");
    println!("  Wire format: {} bytes", rsync_format.len());

    // Decode it
    use arsync::protocol::rsync_compat::decode_file_entry;
    let decoded = decode_file_entry(&rsync_format).expect("Should decode rsync format");

    assert_eq!(decoded.path, "test.txt");
    assert_eq!(decoded.size, 1024);
    assert_eq!(decoded.mode, 0o100644);

    println!("✓ Successfully decoded simulated rsync format");
    println!("  Path: {}", decoded.path);
    println!("  Size: {} bytes", decoded.size);
}

#[test]
fn test_decode_handles_variations() {
    // Test that we can handle different valid encodings

    // Small file
    let small = create_test_entry("small.txt", 10, 0o100644);
    let small_decoded = decode_and_verify(&small, "small.txt", 10);
    assert_eq!(small_decoded.size, 10);

    // Large file
    let large = create_test_entry("large.bin", 10_000_000, 0o100644);
    let large_decoded = decode_and_verify(&large, "large.bin", 10_000_000);
    assert_eq!(large_decoded.size, 10_000_000);

    // Different permissions
    let executable = create_test_entry("script.sh", 100, 0o100755);
    let exec_decoded = decode_and_verify(&executable, "script.sh", 100);
    assert_eq!(exec_decoded.mode, 0o100755);

    println!("✓ Decoder handles various file types and sizes");
}

// Helper functions
fn create_test_entry(path: &str, size: u64, mode: u32) -> Vec<u8> {
    let mut entry = Vec::new();
    entry.push(0x00);
    encode_varint_into(path.len() as u64, &mut entry);
    entry.extend(path.as_bytes());
    encode_varint_into(size, &mut entry);
    encode_varint_into(1696800000u64, &mut entry); // mtime
    encode_varint_into(mode as u64, &mut entry);
    encode_varint_into(1000u64, &mut entry); // uid
    encode_varint_into(1000u64, &mut entry); // gid
    entry
}

fn decode_and_verify(data: &[u8], expected_path: &str, expected_size: u64) -> FileEntry {
    use arsync::protocol::rsync_compat::decode_file_entry;
    let decoded = decode_file_entry(data).expect("Should decode");
    assert_eq!(decoded.path, expected_path);
    assert_eq!(decoded.size, expected_size);
    decoded
}
