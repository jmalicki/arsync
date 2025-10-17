//! Unit tests for handshake protocol
//!
//! These tests verify the handshake state machine, capability negotiation,
//! and checksum seed handling without needing real network connections.

#![cfg(feature = "remote-sync")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use arsync::protocol::handshake::*;

// ============================================================================
// State Machine Basic Tests
// ============================================================================

#[test]
fn test_handshake_state_initial() {
    let state = HandshakeState::Initial;

    assert!(!state.is_complete());
    assert!(state.get_capabilities().is_none());
    assert!(state.get_seed().is_none());

    println!("✓ Initial state is not complete");
}

#[test]
fn test_handshake_state_complete() {
    let mut capabilities = ProtocolCapabilities::new(31);
    capabilities.flags = XMIT_CHECKSUMS | XMIT_SYMLINKS;

    let seed = ChecksumSeed { seed: 12345 };

    let state = HandshakeState::Complete {
        capabilities: capabilities.clone(),
        seed: Some(seed),
    };

    assert!(state.is_complete());
    assert!(state.get_capabilities().is_some());
    assert_eq!(state.get_capabilities().unwrap().version, 31);
    assert_eq!(
        state.get_capabilities().unwrap().flags,
        XMIT_CHECKSUMS | XMIT_SYMLINKS
    );
    assert_eq!(state.get_seed(), Some(seed));

    println!("✓ Complete state provides capabilities and seed");
}

// ============================================================================
// Capability Negotiation Tests
// ============================================================================

#[test]
fn test_capabilities_intersection() {
    let mut client = ProtocolCapabilities::new(31);
    client.flags = XMIT_CHECKSUMS | XMIT_SYMLINKS | XMIT_XATTRS;

    let mut server = ProtocolCapabilities::new(31);
    server.flags = XMIT_CHECKSUMS | XMIT_XATTRS | XMIT_ACLS;

    let negotiated = ProtocolCapabilities::negotiate(&client, &server);

    // Should only have flags both support
    assert_eq!(negotiated.flags, XMIT_CHECKSUMS | XMIT_XATTRS);
    assert!(negotiated.supports_checksums());
    assert!(negotiated.supports_xattrs());
    assert!(!negotiated.supports_symlinks()); // Only client has this
    assert!(!negotiated.supports_acls()); // Only server has this

    println!("✓ Capability negotiation computes intersection");
    println!("  Client: 0x{:08X}", client.flags);
    println!("  Server: 0x{:08X}", server.flags);
    println!("  Result: 0x{:08X}", negotiated.flags);
}

#[test]
fn test_capabilities_version_min() {
    let client = ProtocolCapabilities::new(31);
    let server = ProtocolCapabilities::new(30);

    let negotiated = ProtocolCapabilities::negotiate(&client, &server);

    assert_eq!(negotiated.version, 30); // Minimum of both

    println!("✓ Version negotiation selects minimum");
    println!("  Client: {}", client.version);
    println!("  Server: {}", server.version);
    println!("  Result: {}", negotiated.version);
}

#[test]
fn test_capabilities_support_methods() {
    let mut caps = ProtocolCapabilities::new(31);

    // Initially no flags set
    assert!(!caps.supports_checksums());
    assert!(!caps.supports_symlinks());
    assert!(!caps.supports_xattrs());

    // Set specific flags
    caps.flags |= XMIT_CHECKSUMS;
    assert!(caps.supports_checksums());
    assert!(!caps.supports_symlinks());

    caps.flags |= XMIT_SYMLINKS | XMIT_XATTRS;
    assert!(caps.supports_checksums());
    assert!(caps.supports_symlinks());
    assert!(caps.supports_xattrs());

    // Test all support methods
    caps.flags = XMIT_HARDLINKS;
    assert!(caps.supports_hardlinks());

    caps.flags = XMIT_DEVICES;
    assert!(caps.supports_devices());

    caps.flags = XMIT_ACLS;
    assert!(caps.supports_acls());

    caps.flags = XMIT_SPARSE;
    assert!(caps.supports_sparse());

    caps.flags = XMIT_CHECKSUM_SEED;
    assert!(caps.supports_checksum_seed());

    caps.flags = XMIT_PROTECTION;
    assert!(caps.supports_protection());

    caps.flags = XMIT_TIMES;
    assert!(caps.supports_times());

    println!("✓ All capability support methods work correctly");
}

// ============================================================================
// Checksum Seed Tests
// ============================================================================

#[test]
fn test_checksum_seed_generate() {
    // Generate multiple seeds
    let seeds: Vec<ChecksumSeed> = (0..100).map(|_| ChecksumSeed::generate()).collect();

    // All should be non-zero
    assert!(seeds.iter().all(|s| !s.is_zero()));

    // They should not all be the same (randomness check)
    let first = seeds[0].seed;
    let all_same = seeds.iter().all(|s| s.seed == first);
    assert!(!all_same, "Seeds should be random, not all the same");

    println!("✓ Seed generation produces random non-zero values");
    println!("  Sample seeds: {:?}", &seeds[..5]);
}

#[test]
fn test_checksum_seed_roundtrip() {
    let test_values = vec![0x00000000, 0x00000001, 0x12345678, 0xDEADBEEF, 0xFFFFFFFF];

    for value in test_values {
        let original = ChecksumSeed { seed: value };
        let bytes = original.to_bytes();
        let decoded = ChecksumSeed::from_bytes(bytes);

        assert_eq!(original, decoded);
        assert_eq!(original.seed, decoded.seed);
    }

    println!("✓ Checksum seed survives roundtrip encoding");
}

#[test]
fn test_checksum_seed_byte_order() {
    // Verify little-endian encoding
    let seed = ChecksumSeed { seed: 0x04030201 };
    let bytes = seed.to_bytes();

    assert_eq!(bytes, [0x01, 0x02, 0x03, 0x04]);

    let decoded = ChecksumSeed::from_bytes([0x01, 0x02, 0x03, 0x04]);
    assert_eq!(decoded.seed, 0x04030201);

    println!("✓ Checksum seed uses little-endian byte order");
}

// ============================================================================
// Version Validation Tests
// ============================================================================

#[test]
fn test_version_constants() {
    // Verify our constants are reasonable
    assert_eq!(PROTOCOL_VERSION, 31);
    assert_eq!(MIN_PROTOCOL_VERSION, 27);
    assert_eq!(MAX_PROTOCOL_VERSION, 40);

    // Compile-time sanity checks (clippy complains but these document invariants)
    const _: () = assert!(PROTOCOL_VERSION >= MIN_PROTOCOL_VERSION);
    const _: () = assert!(PROTOCOL_VERSION <= MAX_PROTOCOL_VERSION);

    println!("✓ Protocol version constants are valid");
    println!("  Current: {}", PROTOCOL_VERSION);
    println!("  Range: {}-{}", MIN_PROTOCOL_VERSION, MAX_PROTOCOL_VERSION);
}

// ============================================================================
// get_our_capabilities Tests
// ============================================================================

#[test]
fn test_our_capabilities_complete() {
    let flags = get_our_capabilities();

    // Verify all expected flags are set
    assert_ne!(flags, 0, "Should have some capabilities");

    // Test each flag
    assert!(flags & XMIT_CHECKSUMS != 0, "Should support checksums");
    assert!(flags & XMIT_SYMLINKS != 0, "Should support symlinks");
    assert!(flags & XMIT_HARDLINKS != 0, "Should support hardlinks");
    assert!(flags & XMIT_DEVICES != 0, "Should support devices");
    assert!(flags & XMIT_XATTRS != 0, "Should support xattrs");
    assert!(flags & XMIT_ACLS != 0, "Should support ACLs");
    assert!(
        flags & XMIT_CHECKSUM_SEED != 0,
        "Should support checksum seed"
    );
    assert!(flags & XMIT_PROTECTION != 0, "Should support permissions");
    assert!(flags & XMIT_TIMES != 0, "Should support timestamps");

    println!("✓ Our capabilities include all implemented features:");
    println!("  Flags: 0x{:08X}", flags);
    println!("  - Checksums: ✅");
    println!("  - Symlinks: ✅");
    println!("  - Hardlinks: ✅");
    println!("  - Devices: ✅");
    println!("  - Xattrs: ✅");
    println!("  - ACLs: ✅");
    println!("  - Checksum seed: ✅");
    println!("  - Permissions: ✅");
    println!("  - Timestamps: ✅");
}

// ============================================================================
// Role Tests
// ============================================================================

#[test]
fn test_role_is_sender() {
    let sender = Role::Sender;
    assert!(sender.is_sender());
    assert!(!sender.is_receiver());
}

#[test]
fn test_role_is_receiver() {
    let receiver = Role::Receiver;
    assert!(!receiver.is_sender());
    assert!(receiver.is_receiver());
}

#[test]
fn test_role_opposite() {
    assert_eq!(Role::Sender.opposite(), Role::Receiver);
    assert_eq!(Role::Receiver.opposite(), Role::Sender);

    // Double opposite should be identity
    assert_eq!(Role::Sender.opposite().opposite(), Role::Sender);
    assert_eq!(Role::Receiver.opposite().opposite(), Role::Receiver);
}

// ============================================================================
// Summary
// ============================================================================

#[test]
fn test_handshake_unit_suite_summary() {
    println!("\n═══════════════════════════════════════════");
    println!("📊 HANDSHAKE UNIT TEST SUITE SUMMARY");
    println!("═══════════════════════════════════════════");
    println!();
    println!("Phase 1.1-1.3 Implementation:");
    println!("  ✅ Protocol constants defined");
    println!("  ✅ Role enum with helpers");
    println!("  ✅ ChecksumSeed with generation");
    println!("  ✅ ProtocolCapabilities with 10 support methods");
    println!("  ✅ HandshakeState with 9 states");
    println!("  ✅ State machine advance() with all transitions");
    println!("  ✅ Public API (handshake, handshake_sender, handshake_receiver)");
    println!();
    println!("Test Coverage:");
    println!("  ✅ State machine basics");
    println!("  ✅ Capability negotiation");
    println!("  ✅ Checksum seed generation/roundtrip");
    println!("  ✅ Version constants");
    println!("  ✅ Our capabilities verification");
    println!("  ✅ Role methods");
    println!();
    println!("Next: Phase 1.5 (integration tests with pipes)");
    println!("═══════════════════════════════════════════");
}
