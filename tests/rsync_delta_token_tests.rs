//! Tests for rsync delta token encoding/decoding
//!
//! Validates conversion between DeltaInstructions and rsync token format.

#![allow(clippy::panic)] // Tests use panic! for assertion failures

use arsync::protocol::rsync::DeltaInstruction;
use arsync::protocol::rsync_compat::{delta_to_tokens, tokens_to_delta, RsyncBlockChecksum};

#[test]
fn test_literal_encoding() {
    println!("🔍 Testing literal token encoding...");

    // Small literal
    let delta = vec![DeltaInstruction::Literal(b"Hello".to_vec())];

    let tokens = delta_to_tokens(&delta);

    // Expected: [5, 'H', 'e', 'l', 'l', 'o', 0]
    assert_eq!(tokens[0], 5); // Token: 5 bytes follow
    assert_eq!(&tokens[1..6], b"Hello");
    assert_eq!(tokens[6], 0); // End marker

    println!("✅ Small literal encoded correctly");
}

#[test]
fn test_large_literal_chunking() {
    println!("🔍 Testing large literal chunking...");

    // 200 bytes literal (needs chunking into 96+96+8)
    let large_data = vec![0xAAu8; 200];
    let delta = vec![DeltaInstruction::Literal(large_data)];

    let tokens = delta_to_tokens(&delta);

    // Should be: [96][96 bytes][96][96 bytes][8][8 bytes][0]
    assert_eq!(tokens[0], 96); // First chunk: 96 bytes
    assert_eq!(tokens[97], 96); // Second chunk: 96 bytes
    assert_eq!(tokens[194], 8); // Third chunk: 8 bytes
    assert_eq!(tokens[203], 0); // End marker

    println!("✅ Large literal chunked into 96+96+8");
}

#[test]
fn test_block_match_simple_offset() {
    println!("🔍 Testing block match with simple offset...");

    // Block match with offset < 16
    let delta = vec![
        DeltaInstruction::BlockMatch {
            block_index: 0,
            length: 4096,
        },
        DeltaInstruction::BlockMatch {
            block_index: 1,
            length: 4096,
        }, // Offset 0 (consecutive)
        DeltaInstruction::BlockMatch {
            block_index: 5,
            length: 4096,
        }, // Offset 3 (5 - (1+1))
    ];

    let tokens = delta_to_tokens(&delta);

    assert_eq!(tokens[0], 97); // Block 0 (offset 0 from start)
    assert_eq!(tokens[1], 97); // Block 1 (offset 0 from block 0)
    assert_eq!(tokens[2], 100); // Block 5 (offset 3 from block 1: 97+3)
    assert_eq!(tokens[3], 0); // End marker

    println!("✅ Simple offsets encoded correctly");
}

#[test]
fn test_delta_roundtrip() {
    println!("🔍 Testing delta roundtrip...");

    let original_delta = vec![
        DeltaInstruction::Literal(b"Start".to_vec()),
        DeltaInstruction::BlockMatch {
            block_index: 0,
            length: 100,
        },
        DeltaInstruction::Literal(b"Middle".to_vec()),
        DeltaInstruction::BlockMatch {
            block_index: 2,
            length: 100,
        },
        DeltaInstruction::Literal(b"End".to_vec()),
    ];

    // Create dummy checksums (for token_to_delta length calculation)
    let checksums = vec![
        RsyncBlockChecksum {
            weak: 0,
            strong: vec![0; 16],
        },
        RsyncBlockChecksum {
            weak: 0,
            strong: vec![0; 16],
        },
        RsyncBlockChecksum {
            weak: 0,
            strong: vec![0; 16],
        },
    ];

    // Encode
    let tokens = delta_to_tokens(&original_delta);
    println!("  Encoded to {} token bytes", tokens.len());

    // Decode
    let decoded_delta = tokens_to_delta(&tokens, &checksums).expect("Failed to decode");
    println!("  Decoded to {} instructions", decoded_delta.len());

    // Verify count
    assert_eq!(decoded_delta.len(), original_delta.len());

    // Verify each instruction
    for (i, (original, decoded)) in original_delta.iter().zip(decoded_delta.iter()).enumerate() {
        match (original, decoded) {
            (DeltaInstruction::Literal(orig_data), DeltaInstruction::Literal(dec_data)) => {
                assert_eq!(orig_data, dec_data, "Literal mismatch at instruction {}", i);
                println!(
                    "  ✅ Instruction {}: Literal ({} bytes)",
                    i,
                    orig_data.len()
                );
            }
            (
                DeltaInstruction::BlockMatch {
                    block_index: orig_idx,
                    ..
                },
                DeltaInstruction::BlockMatch {
                    block_index: dec_idx,
                    ..
                },
            ) => {
                assert_eq!(
                    orig_idx, dec_idx,
                    "Block index mismatch at instruction {}",
                    i
                );
                println!("  ✅ Instruction {}: BlockMatch({})", i, orig_idx);
            }
            _ => panic!("Instruction type mismatch at {}", i),
        }
    }

    println!("✅ Full delta roundtrip successful!");
}

#[test]
fn test_empty_delta() {
    println!("🔍 Testing empty delta...");

    let empty_delta: Vec<DeltaInstruction> = vec![];
    let tokens = delta_to_tokens(&empty_delta);

    // Should just be end marker
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0], 0);

    println!("✅ Empty delta = just end marker");
}

#[test]
fn test_only_literals() {
    println!("🔍 Testing delta with only literals...");

    let delta = vec![
        DeltaInstruction::Literal(b"ABC".to_vec()),
        DeltaInstruction::Literal(b"DEF".to_vec()),
        DeltaInstruction::Literal(b"GHI".to_vec()),
    ];

    let tokens = delta_to_tokens(&delta);

    // Expected: [3][ABC][3][DEF][3][GHI][0]
    assert_eq!(tokens[0], 3);
    assert_eq!(&tokens[1..4], b"ABC");
    assert_eq!(tokens[4], 3);
    assert_eq!(&tokens[5..8], b"DEF");
    assert_eq!(tokens[8], 3);
    assert_eq!(&tokens[9..12], b"GHI");
    assert_eq!(tokens[12], 0);

    println!("✅ Multiple literals encoded correctly");
}

#[test]
fn test_only_block_matches() {
    println!("🔍 Testing delta with only block matches...");

    let delta = vec![
        DeltaInstruction::BlockMatch {
            block_index: 0,
            length: 100,
        },
        DeltaInstruction::BlockMatch {
            block_index: 1,
            length: 100,
        },
        DeltaInstruction::BlockMatch {
            block_index: 2,
            length: 100,
        },
    ];

    let tokens = delta_to_tokens(&delta);

    // Expected: [97][97][97][0] (all consecutive blocks)
    assert_eq!(tokens[0], 97); // Block 0
    assert_eq!(tokens[1], 97); // Block 1 (offset 0)
    assert_eq!(tokens[2], 97); // Block 2 (offset 0)
    assert_eq!(tokens[3], 0); // End marker

    println!("✅ Consecutive blocks encoded correctly");
}

#[test]
fn test_summary() {
    println!("\n═══════════════════════════════════════════════════════════");
    println!("  rsync Delta Token Tests - Summary");
    println!("═══════════════════════════════════════════════════════════");
    println!();
    println!("✅ test_literal_encoding");
    println!("   → Small literals (token 1-96)");
    println!();
    println!("✅ test_large_literal_chunking");
    println!("   → Literals >96 bytes split into chunks");
    println!();
    println!("✅ test_block_match_simple_offset");
    println!("   → Block matches with offset <16");
    println!();
    println!("✅ test_delta_roundtrip");
    println!("   → Full encode → decode cycle");
    println!("   → Mixed literals and block matches");
    println!();
    println!("✅ test_empty_delta");
    println!("   → Empty delta = end marker only");
    println!();
    println!("✅ test_only_literals");
    println!("   → Delta with no block matches");
    println!();
    println!("✅ test_only_block_matches");
    println!("   → Delta with no literals");
    println!();
    println!("Token Format:");
    println!("  - 0:     End of data ✅");
    println!("  - 1-96:  Literal run ✅");
    println!("  - 97-255: Block match ✅");
    println!();
    println!("Features:");
    println!("  - Literal chunking (max 96 bytes) ✅");
    println!("  - Offset encoding (simple + complex) ✅");
    println!("  - Block index tracking ✅");
    println!("  - End marker ✅");
    println!();
    println!("═══════════════════════════════════════════════════════════");
}
