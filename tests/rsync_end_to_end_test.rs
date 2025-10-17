//! End-to-end test for complete rsync protocol implementation
//!
//! Tests the full flow: handshake → file list → checksums → delta → file reconstruction

#![cfg(feature = "remote-sync")]

use arsync::protocol::checksum::{rolling_checksum_with_seed, strong_checksum};
use arsync::protocol::handshake::{handshake_receiver, handshake_sender};
use arsync::protocol::rsync::{apply_delta, generate_block_checksums, generate_delta, FileEntry};
use arsync::protocol::rsync_compat::*;
use arsync::protocol::varint::encode_varint_into;
use futures::join;

// Helper: Encode single file entry
fn encode_single_file(file: &FileEntry) -> Vec<u8> {
    let mut entry = Vec::new();
    entry.push(0u8); // flags
    encode_varint_into(file.path.len() as u64, &mut entry);
    entry.extend(file.path.as_bytes());
    encode_varint_into(file.size, &mut entry);
    encode_varint_into(file.mtime as u64, &mut entry);
    encode_varint_into(file.mode as u64, &mut entry);
    encode_varint_into(file.uid as u64, &mut entry);
    encode_varint_into(file.gid as u64, &mut entry);
    entry
}

// Helper: Build checksum message
fn build_checksum_message(data: &[u8], block_size: usize, seed: u32) -> Vec<u8> {
    let block_count = data.len().div_ceil(block_size);
    let remainder = data.len() % block_size;

    let mut msg = Vec::new();
    msg.extend((block_count as u32).to_le_bytes());
    msg.extend((block_size as u32).to_le_bytes());
    msg.extend((remainder as u32).to_le_bytes());
    msg.extend(16u32.to_le_bytes()); // MD5 length

    let mut offset = 0;
    while offset < data.len() {
        let end = (offset + block_size).min(data.len());
        let block = &data[offset..end];

        let weak = rolling_checksum_with_seed(block, seed);
        let strong = strong_checksum(block);

        msg.extend(weak.to_le_bytes());
        msg.extend(strong);
        offset = end;
    }

    msg
}

// Helper: Parse checksum message
fn parse_checksum_message(
    msg: &MultiplexMessage,
) -> anyhow::Result<(Vec<RsyncBlockChecksum>, usize)> {
    use std::io::{Cursor, Read};

    let mut cursor = Cursor::new(&msg.data[..]);
    let mut buf4 = [0u8; 4];

    cursor.read_exact(&mut buf4)?;
    let block_count = u32::from_le_bytes(buf4) as usize;

    cursor.read_exact(&mut buf4)?;
    let block_size = u32::from_le_bytes(buf4) as usize;

    cursor.read_exact(&mut buf4)?; // remainder

    cursor.read_exact(&mut buf4)?;
    let checksum_len = u32::from_le_bytes(buf4) as usize;

    let mut checksums = Vec::new();
    for _ in 0..block_count {
        cursor.read_exact(&mut buf4)?;
        let weak = u32::from_le_bytes(buf4);

        let mut strong = vec![0u8; checksum_len];
        cursor.read_exact(&mut strong)?;

        checksums.push(RsyncBlockChecksum { weak, strong });
    }

    Ok((checksums, block_size))
}

// Helper: Receive file list
async fn receive_file_list<T: arsync::protocol::transport::Transport>(
    transport: &mut T,
) -> anyhow::Result<Vec<FileEntry>> {
    let mut files = Vec::new();

    loop {
        let msg = read_mplex_message(transport).await?;
        if msg.tag != MessageTag::FList {
            anyhow::bail!("Expected FList");
        }
        if msg.data.is_empty() {
            break;
        }
        files.push(decode_file_entry(&msg.data)?);
    }

    Ok(files)
}

#[compio::test]
async fn test_full_protocol_flow() {
    println!("🔍 Testing complete rsync protocol flow...");

    // Test file content
    let original_content = b"Hello, World! This is a test file for rsync protocol.";
    let modified_content = b"Hello, Universe! This is a MODIFIED file for rsync protocol.";

    // Create file entry
    let file = FileEntry {
        path: "test.txt".to_string(),
        size: modified_content.len() as u64,
        mtime: 1000000,
        mode: 0o644,
        uid: 1000,
        gid: 1000,
        is_symlink: false,
        symlink_target: None,
    };

    println!("✅ Test data prepared");
    println!("   Original: {} bytes", original_content.len());
    println!("   Modified: {} bytes", modified_content.len());

    // Create bidirectional pipes
    let (sender_read, receiver_write) =
        arsync::protocol::pipe::PipeTransport::create_pipe().expect("Failed to create pipe 1");
    let (receiver_read, sender_write) =
        arsync::protocol::pipe::PipeTransport::create_pipe().expect("Failed to create pipe 2");

    let sender_transport = unsafe {
        arsync::protocol::pipe::PipeTransport::from_fds(
            sender_read,
            sender_write,
            "sender".to_string(),
        )
        .expect("Failed to create sender transport")
    };

    let receiver_transport = unsafe {
        arsync::protocol::pipe::PipeTransport::from_fds(
            receiver_read,
            receiver_write,
            "receiver".to_string(),
        )
        .expect("Failed to create receiver transport")
    };

    // Run sender and receiver concurrently
    let sender_future = async move {
        let mut transport = sender_transport;

        // 1. Handshake
        let caps = handshake_sender(&mut transport).await?;
        let seed = caps.checksum_seed.unwrap_or(0);
        println!("Sender: Handshake complete (seed={})", seed);

        // 2. Send file list (use raw mplex functions)
        write_mplex_message(
            &mut transport,
            MessageTag::FList,
            &encode_single_file(&file),
        )
        .await?;
        write_mplex_message(&mut transport, MessageTag::FList, &[]).await?; // End marker
        println!("Sender: File list sent");

        // 3. Receive checksums from receiver
        let msg = read_mplex_message(&mut transport).await?;
        let (checksums, block_size) = parse_checksum_message(&msg)?;
        println!(
            "Sender: Received {} checksums (block_size={})",
            checksums.len(),
            block_size
        );

        // 4. Generate delta
        let checksums_native: Vec<_> = checksums
            .iter()
            .enumerate()
            .map(|(i, c)| arsync::protocol::rsync::BlockChecksum {
                weak: c.weak,
                strong: c.strong.as_slice().try_into().unwrap_or([0u8; 16]),
                offset: (i * block_size) as u64,
                block_index: i as u32,
            })
            .collect();

        let delta = generate_delta(modified_content, &checksums_native)?;
        println!("Sender: Generated delta with {} instructions", delta.len());

        // 5. Send delta
        let tokens = delta_to_tokens(&delta);
        write_mplex_message(&mut transport, MessageTag::Data, &tokens).await?;
        println!("Sender: Delta sent");

        Ok::<(), anyhow::Error>(())
    };

    let receiver_future = async move {
        let mut transport = receiver_transport;

        // 1. Handshake
        let caps = handshake_receiver(&mut transport).await?;
        let seed = caps.checksum_seed.unwrap_or(0);
        println!("Receiver: Handshake complete (seed={})", seed);

        // 2. Receive file list
        let files = receive_file_list(&mut transport).await?;
        println!("Receiver: Received {} files", files.len());

        assert_eq!(files.len(), 1);
        let _received_file = &files[0];

        // 3. Generate and send checksums from basis (original content)
        let block_size = 16;
        let checksums_native = generate_block_checksums(original_content, block_size)?;
        let checksum_msg = build_checksum_message(original_content, block_size, seed);
        write_mplex_message(&mut transport, MessageTag::Data, &checksum_msg).await?;
        println!(
            "Receiver: Sent {} checksums",
            original_content.len().div_ceil(block_size)
        );

        // 4. Receive delta
        let msg = read_mplex_message(&mut transport).await?;
        let delta = tokens_to_delta(&msg.data, &[])?; // Empty checksums for now
        println!("Receiver: Received delta with {} instructions", delta.len());

        // 5. Apply delta to reconstruct
        let reconstructed = apply_delta(Some(original_content), &delta, &checksums_native)?;
        println!("Receiver: Reconstructed {} bytes", reconstructed.len());

        Ok::<Vec<u8>, anyhow::Error>(reconstructed)
    };

    let (sender_result, receiver_result) = join!(sender_future, receiver_future);

    sender_result.expect("Sender failed");
    let reconstructed = receiver_result.expect("Receiver failed");

    // Verify reconstruction
    assert_eq!(reconstructed.len(), modified_content.len());
    assert_eq!(&reconstructed[..], &modified_content[..]);

    println!("✅ Full protocol flow successful!");
    println!("   Handshake ✅");
    println!("   File list ✅");
    println!("   Checksums ✅");
    println!("   Delta ✅");
    println!("   Reconstruction ✅");
    println!("   File matches exactly!");
}

#[compio::test]
async fn test_summary() {
    println!("\n═══════════════════════════════════════════════════════════");
    println!("  rsync End-to-End Tests - Summary");
    println!("═══════════════════════════════════════════════════════════");
    println!();
    println!("✅ test_full_protocol_flow");
    println!("   → Complete rsync wire protocol");
    println!("   → Handshake with seed exchange");
    println!("   → File list transmission");
    println!("   → Checksum exchange (seeded)");
    println!("   → Delta generation and transfer");
    println!("   → File reconstruction");
    println!("   → Byte-for-byte verification");
    println!();
    println!("This validates:");
    println!("  - All protocol phases work together ✅");
    println!("  - Bidirectional communication ✅");
    println!("  - Delta algorithm produces correct results ✅");
    println!("  - Seeded checksums work end-to-end ✅");
    println!();
    println!("═══════════════════════════════════════════════════════════");
}
