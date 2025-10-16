//! rsync Protocol Handshake Implementation
//!
//! This module implements the rsync wire protocol handshake sequence, which establishes:
//! 1. Protocol version compatibility between client and server
//! 2. Capability negotiation (what features both sides support)
//! 3. Random seed exchange for checksum algorithms
//! 4. Multiplexing mode activation
//!
//! # Protocol Sequence
//!
//! ```text
//! Client                                Server
//!   |                                      |
//!   |-- Version Byte (31) --------------->|
//!   |<-------------- Version Byte (31) ---|
//!   |                                      |
//!   |-- Capability Flags (varint) ------->|
//!   |<----- Capability Flags (varint) ----|
//!   |                                      |
//!   |-- Checksum Seed (optional) -------->|
//!   |<----- Checksum Seed (optional) -----|
//!   |                                      |
//!   |   Handshake Complete                 |
//!   |   (Multiplexing Active)              |
//! ```
//!
//! # Example
//!
//! ```rust,no_run
//! use arsync::protocol::handshake::{handshake, Role};
//! use arsync::protocol::pipe::PipeTransport;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let mut transport = PipeTransport::from_stdio()?;
//! let capabilities = handshake(&mut transport, Role::Sender).await?;
//! println!("Negotiated protocol version: {}", capabilities.version);
//! # Ok(())
//! # }
//! ```

use crate::protocol::transport::{read_exact, write_all, Transport};
use anyhow::Result;
use tracing::{debug, info, warn};

// ============================================================================
// Protocol Constants
// ============================================================================

/// Current protocol version supported by arsync
///
/// This corresponds to rsync protocol version 31, which is used by rsync 3.2+
#[allow(dead_code)] // Part of protocol spec, not yet fully used
pub const PROTOCOL_VERSION: u8 = 31;

/// Minimum protocol version we can negotiate with
///
/// Protocol version 27 introduced multiplexed I/O, which is required for
/// our implementation. Older versions are not supported.
#[allow(dead_code)] // Part of protocol spec, not yet fully used
pub const MIN_PROTOCOL_VERSION: u8 = 27;

/// Maximum protocol version we understand
///
/// This is a safety check to ensure we don't try to speak protocols we
/// don't understand. Modern rsync uses versions 27-32.
#[allow(dead_code)] // Part of protocol spec, not yet fully used
pub const MAX_PROTOCOL_VERSION: u8 = 40;

// ============================================================================
// Capability Flags (from rsync protocol.h)
// ============================================================================

/// File uses checksums (weak + strong) for delta algorithm
#[allow(dead_code)] // Part of protocol spec, not yet fully used
pub const XMIT_CHECKSUMS: u32 = 1 << 0;

/// Hard links are preserved
#[allow(dead_code)] // Part of protocol spec, not yet fully used
pub const XMIT_HARDLINKS: u32 = 1 << 1;

/// Symbolic links are preserved
#[allow(dead_code)] // Part of protocol spec, not yet fully used
pub const XMIT_SYMLINKS: u32 = 1 << 2;

/// Device files and special files are preserved
#[allow(dead_code)] // Part of protocol spec, not yet fully used
pub const XMIT_DEVICES: u32 = 1 << 3;

/// Extended attributes are preserved
#[allow(dead_code)] // Part of protocol spec, not yet fully used
pub const XMIT_XATTRS: u32 = 1 << 4;

/// POSIX ACLs are preserved
#[allow(dead_code)] // Part of protocol spec, not yet fully used
pub const XMIT_ACLS: u32 = 1 << 5;

/// Sparse file optimization (holes not transferred)
#[allow(dead_code)] // Part of protocol spec, not yet fully used
pub const XMIT_SPARSE: u32 = 1 << 6;

/// Use random seed for checksums (security)
#[allow(dead_code)] // Part of protocol spec, not yet fully used
pub const XMIT_CHECKSUM_SEED: u32 = 1 << 7;

/// File permissions are preserved
#[allow(dead_code)] // Part of protocol spec, not yet fully used
pub const XMIT_PROTECTION: u32 = 1 << 8;

/// File timestamps are preserved
#[allow(dead_code)] // Part of protocol spec, not yet fully used
pub const XMIT_TIMES: u32 = 1 << 9;

// ============================================================================
// Role
// ============================================================================

/// Role in the protocol handshake
///
/// The handshake is bidirectional, but each side has a different role:
/// - **Sender**: Sends files to the receiver
/// - **Receiver**: Receives files from the sender
///
/// Some protocol messages are role-specific (e.g., seed exchange direction).
#[allow(dead_code)] // Part of protocol spec, not yet fully used
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// This side is sending files
    Sender,

    /// This side is receiving files
    Receiver,
}

impl Role {
    /// Check if this is the sender role
    #[allow(dead_code)]
    #[must_use]
    pub const fn is_sender(&self) -> bool {
        matches!(self, Self::Sender)
    }

    /// Check if this is the receiver role
    #[allow(dead_code)]
    #[must_use]
    pub const fn is_receiver(&self) -> bool {
        matches!(self, Self::Receiver)
    }

    /// Get the opposite role
    #[allow(dead_code)]
    #[must_use]
    pub const fn opposite(&self) -> Self {
        match self {
            Self::Sender => Self::Receiver,
            Self::Receiver => Self::Sender,
        }
    }
}

// ============================================================================
// ChecksumSeed
// ============================================================================

/// Random seed for checksum algorithms
///
/// The checksum seed is used to randomize rolling checksums, which helps
/// prevent collision attacks in untrusted environments. Both sides exchange
/// seeds during the handshake if `XMIT_CHECKSUM_SEED` is negotiated.
///
/// # Wire Format
///
/// Seeds are transmitted as 4 bytes in little-endian format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChecksumSeed {
    /// The random seed value
    pub seed: u32,
}

impl ChecksumSeed {
    /// Generate a new random checksum seed
    ///
    /// # Example
    ///
    /// ```rust
    /// use arsync::protocol::handshake::ChecksumSeed;
    ///
    /// let seed = ChecksumSeed::generate();
    /// assert_ne!(seed.seed, 0); // Should be random
    /// ```
    #[must_use]
    pub fn generate() -> Self {
        use rand::Rng;
        Self {
            seed: rand::rng().random(),
        }
    }

    /// Create a seed from raw bytes (little-endian)
    ///
    /// # Example
    ///
    /// ```rust
    /// use arsync::protocol::handshake::ChecksumSeed;
    ///
    /// let bytes = [0x01, 0x02, 0x03, 0x04];
    /// let seed = ChecksumSeed::from_bytes(bytes);
    /// assert_eq!(seed.seed, 0x04030201);
    /// ```
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 4]) -> Self {
        Self {
            seed: u32::from_le_bytes(bytes),
        }
    }

    /// Convert seed to bytes (little-endian)
    ///
    /// # Example
    ///
    /// ```rust
    /// use arsync::protocol::handshake::ChecksumSeed;
    ///
    /// let seed = ChecksumSeed { seed: 0x04030201 };
    /// let bytes = seed.to_bytes();
    /// assert_eq!(bytes, [0x01, 0x02, 0x03, 0x04]);
    /// ```
    #[must_use]
    #[allow(clippy::wrong_self_convention)] // to_bytes is standard for wire protocol serialization
    pub const fn to_bytes(&self) -> [u8; 4] {
        self.seed.to_le_bytes()
    }

    /// Check if the seed is zero (uninitialized)
    #[allow(dead_code)]
    #[must_use]
    pub const fn is_zero(&self) -> bool {
        self.seed == 0
    }
}

// ============================================================================
// ProtocolCapabilities
// ============================================================================

/// Negotiated protocol capabilities
///
/// After the handshake, this structure contains the agreed-upon protocol
/// version and feature flags that both sides support.
///
/// # Example
///
/// ```rust
/// use arsync::protocol::handshake::{ProtocolCapabilities, XMIT_CHECKSUMS};
///
/// let caps = ProtocolCapabilities::new(31);
/// assert_eq!(caps.version, 31);
/// ```
#[derive(Debug, Clone)]
pub struct ProtocolCapabilities {
    /// Negotiated protocol version (minimum of both sides)
    pub version: u8,

    /// Capability flags (intersection of both sides)
    pub flags: u32,

    /// Optional checksum seed (if `XMIT_CHECKSUM_SEED` is set)
    pub checksum_seed: Option<u32>,
}

// Part of protocol spec, not yet fully used
#[allow(dead_code)]
impl ProtocolCapabilities {
    /// Create new capabilities with default flags
    ///
    /// # Example
    ///
    /// ```rust
    /// use arsync::protocol::handshake::ProtocolCapabilities;
    ///
    /// let caps = ProtocolCapabilities::new(31);
    /// assert_eq!(caps.version, 31);
    /// assert_eq!(caps.flags, 0);
    /// ```
    #[must_use]
    pub const fn new(version: u8) -> Self {
        Self {
            version,
            flags: 0,
            checksum_seed: None,
        }
    }

    /// Check if checksums are supported
    #[must_use]
    pub const fn supports_checksums(&self) -> bool {
        self.flags & XMIT_CHECKSUMS != 0
    }

    /// Check if hard links are supported
    #[must_use]
    pub const fn supports_hardlinks(&self) -> bool {
        self.flags & XMIT_HARDLINKS != 0
    }

    /// Check if symbolic links are supported
    #[must_use]
    pub const fn supports_symlinks(&self) -> bool {
        self.flags & XMIT_SYMLINKS != 0
    }

    /// Check if device files are supported
    #[must_use]
    pub const fn supports_devices(&self) -> bool {
        self.flags & XMIT_DEVICES != 0
    }

    /// Check if extended attributes are supported
    #[must_use]
    pub const fn supports_xattrs(&self) -> bool {
        self.flags & XMIT_XATTRS != 0
    }

    /// Check if POSIX ACLs are supported
    #[must_use]
    pub const fn supports_acls(&self) -> bool {
        self.flags & XMIT_ACLS != 0
    }

    /// Check if sparse file optimization is supported
    #[must_use]
    pub const fn supports_sparse(&self) -> bool {
        self.flags & XMIT_SPARSE != 0
    }

    /// Check if checksum seed is supported
    #[must_use]
    pub const fn supports_checksum_seed(&self) -> bool {
        self.flags & XMIT_CHECKSUM_SEED != 0
    }

    /// Check if file permissions are supported
    #[must_use]
    pub const fn supports_protection(&self) -> bool {
        self.flags & XMIT_PROTECTION != 0
    }

    /// Check if timestamps are supported
    #[must_use]
    pub const fn supports_times(&self) -> bool {
        self.flags & XMIT_TIMES != 0
    }

    /// Negotiate capabilities between client and server
    ///
    /// The negotiated capabilities are the intersection of what both sides support:
    /// - Version: minimum of both versions
    /// - Flags: bitwise AND of both flags
    ///
    /// # Example
    ///
    /// ```rust
    /// use arsync::protocol::handshake::{ProtocolCapabilities, XMIT_CHECKSUMS, XMIT_SYMLINKS};
    ///
    /// let mut client = ProtocolCapabilities::new(31);
    /// client.flags = XMIT_CHECKSUMS | XMIT_SYMLINKS;
    ///
    /// let mut server = ProtocolCapabilities::new(30);
    /// server.flags = XMIT_CHECKSUMS; // Only checksums, no symlinks
    ///
    /// let negotiated = ProtocolCapabilities::negotiate(&client, &server);
    /// assert_eq!(negotiated.version, 30); // Minimum version
    /// assert!(negotiated.supports_checksums()); // Both support
    /// assert!(!negotiated.supports_symlinks()); // Only client supports
    /// ```
    #[must_use]
    pub fn negotiate(client: &Self, server: &Self) -> Self {
        Self {
            version: client.version.min(server.version),
            flags: client.flags & server.flags, // Intersection
            checksum_seed: None,                // Will be set during seed exchange
        }
    }
}

// ============================================================================
// HandshakeState
// ============================================================================

/// State machine for protocol handshake
///
/// The handshake progresses through multiple states:
///
/// ```text
/// Initial
///   ↓
/// VersionSent (sent our version)
///   ↓
/// VersionReceived (got remote version)
///   ↓
/// VersionNegotiated (computed effective version)
///   ↓
/// FlagsSent (sent our capabilities)
///   ↓
/// FlagsReceived (got remote capabilities)
///   ↓
/// CapabilitiesNegotiated (computed effective capabilities)
///   ↓
/// SeedExchange (exchanging checksum seeds, optional)
///   ↓
/// Complete (handshake done, ready for file transfer)
/// ```
///
/// # Example
///
/// ```rust,no_run
/// use arsync::protocol::handshake::{HandshakeState, Role};
/// use arsync::protocol::pipe::PipeTransport;
///
/// # async fn example() -> anyhow::Result<()> {
/// let mut transport = PipeTransport::from_stdio()?;
/// let mut state = HandshakeState::Initial;
///
/// while !state.is_complete() {
///     state = state.advance(&mut transport, Role::Sender).await?;
/// }
///
/// let capabilities = state.get_capabilities().unwrap();
/// println!("Handshake complete! Version: {}", capabilities.version);
/// # Ok(())
/// # }
/// ```
#[allow(dead_code)] // Part of protocol spec, not yet fully used
#[derive(Debug, Clone)]
pub enum HandshakeState {
    /// Initial state - handshake not started
    Initial,

    /// Sent our protocol version to remote
    VersionSent {
        /// Our protocol version
        our_version: u8,
    },

    /// Received remote protocol version
    VersionReceived {
        /// Our protocol version
        our_version: u8,
        /// Remote protocol version
        remote_version: u8,
    },

    /// Negotiated effective protocol version
    VersionNegotiated {
        /// Effective protocol version (minimum of both)
        protocol_version: u8,
    },

    /// Sent our capability flags to remote
    FlagsSent {
        /// Effective protocol version
        protocol_version: u8,
        /// Our capability flags
        our_flags: u32,
    },

    /// Received remote capability flags
    FlagsReceived {
        /// Effective protocol version
        protocol_version: u8,
        /// Our capability flags
        our_flags: u32,
        /// Remote capability flags
        remote_flags: u32,
    },

    /// Negotiated effective capabilities
    CapabilitiesNegotiated {
        /// Negotiated capabilities
        capabilities: ProtocolCapabilities,
    },

    /// Exchanging checksum seeds (optional phase)
    SeedExchange {
        /// Capabilities with seed exchange in progress
        capabilities: ProtocolCapabilities,
    },

    /// Handshake complete - ready for file transfer
    Complete {
        /// Final negotiated capabilities
        capabilities: ProtocolCapabilities,
        /// Checksum seed (if negotiated)
        seed: Option<ChecksumSeed>,
    },
}

// Part of protocol spec, not yet fully used
#[allow(dead_code)]
impl HandshakeState {
    /// Check if handshake is complete
    ///
    /// # Example
    ///
    /// ```rust
    /// use arsync::protocol::handshake::{HandshakeState, ProtocolCapabilities};
    ///
    /// let state = HandshakeState::Initial;
    /// assert!(!state.is_complete());
    ///
    /// let state = HandshakeState::Complete {
    ///     capabilities: ProtocolCapabilities::new(31),
    ///     seed: None,
    /// };
    /// assert!(state.is_complete());
    /// ```
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        matches!(self, Self::Complete { .. })
    }

    /// Get capabilities if handshake is complete
    ///
    /// Returns `Some(&ProtocolCapabilities)` if in Complete state, `None` otherwise.
    #[must_use]
    pub const fn get_capabilities(&self) -> Option<&ProtocolCapabilities> {
        match self {
            Self::Complete { capabilities, .. } => Some(capabilities),
            _ => None,
        }
    }

    /// Get checksum seed if handshake is complete and seed was negotiated
    #[must_use]
    pub const fn get_seed(&self) -> Option<ChecksumSeed> {
        match self {
            Self::Complete { seed, .. } => *seed,
            _ => None,
        }
    }

    /// Advance the handshake state machine
    ///
    /// This is the core state machine that implements the rsync handshake protocol.
    /// Each call advances the state by one step, performing the necessary I/O and
    /// validation for that step.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Transport I/O fails
    /// - Protocol version is incompatible
    /// - Invalid data is received
    /// - Handshake is already complete
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use arsync::protocol::handshake::{HandshakeState, Role};
    /// # use arsync::protocol::pipe::PipeTransport;
    /// # async fn example() -> anyhow::Result<()> {
    /// let mut transport = PipeTransport::from_stdio()?;
    /// let mut state = HandshakeState::Initial;
    ///
    /// while !state.is_complete() {
    ///     state = state.advance(&mut transport, Role::Sender).await?;
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn advance<T>(self, transport: &mut T, role: Role) -> Result<Self>
    where
        T: Transport,
    {
        match self {
            // ================================================================
            // Initial → VersionSent: Send our protocol version
            // ================================================================
            Self::Initial => {
                debug!("Handshake: Sending protocol version {}", PROTOCOL_VERSION);

                write_all(transport, &[PROTOCOL_VERSION])
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to send protocol version: {}", e))?;

                debug!("Handshake: Version sent");

                Ok(Self::VersionSent {
                    our_version: PROTOCOL_VERSION,
                })
            }

            // ================================================================
            // VersionSent → VersionReceived: Receive remote protocol version
            // ================================================================
            Self::VersionSent { our_version } => {
                debug!("Handshake: Waiting for remote protocol version");

                let mut buf = [0u8; 1];
                read_exact(transport, &mut buf).await.map_err(|e| {
                    anyhow::anyhow!("Failed to read remote protocol version: {}", e)
                })?;

                let remote_version = buf[0];

                debug!(
                    "Handshake: Received protocol version {} from remote",
                    remote_version
                );

                // Validate remote version
                if remote_version < MIN_PROTOCOL_VERSION {
                    anyhow::bail!(
                        "Unsupported remote protocol version: {} (minimum supported: {})",
                        remote_version,
                        MIN_PROTOCOL_VERSION
                    );
                }

                if remote_version > MAX_PROTOCOL_VERSION {
                    warn!(
                        "Remote protocol version {} is newer than maximum known version {}",
                        remote_version, MAX_PROTOCOL_VERSION
                    );
                }

                Ok(Self::VersionReceived {
                    our_version,
                    remote_version,
                })
            }

            // ================================================================
            // VersionReceived → VersionNegotiated: Compute effective version
            // ================================================================
            Self::VersionReceived {
                our_version,
                remote_version,
            } => {
                // Effective version is minimum of both
                let protocol_version = our_version.min(remote_version);

                info!(
                    "Handshake: Protocol version negotiated: {} (our={}, remote={})",
                    protocol_version, our_version, remote_version
                );

                Ok(Self::VersionNegotiated { protocol_version })
            }

            // ================================================================
            // VersionNegotiated → FlagsSent: Send our capability flags
            // ================================================================
            Self::VersionNegotiated { protocol_version } => {
                let our_flags = get_our_capabilities();

                debug!("Handshake: Sending capability flags: 0x{:08X}", our_flags);

                // Encode flags as varint
                let mut buf = Vec::new();
                crate::protocol::varint::encode_varint_into(our_flags as u64, &mut buf);

                write_all(transport, &buf)
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to send capability flags: {}", e))?;

                debug!("Handshake: Capability flags sent");

                Ok(Self::FlagsSent {
                    protocol_version,
                    our_flags,
                })
            }

            // ================================================================
            // FlagsSent → FlagsReceived: Receive remote capability flags
            // ================================================================
            Self::FlagsSent {
                protocol_version,
                our_flags,
            } => {
                debug!("Handshake: Waiting for remote capability flags");

                // Decode remote flags from varint
                let remote_flags = crate::protocol::varint::decode_varint(transport)
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to read remote capability flags: {}", e))?
                    as u32;

                debug!(
                    "Handshake: Received capability flags: 0x{:08X}",
                    remote_flags
                );

                Ok(Self::FlagsReceived {
                    protocol_version,
                    our_flags,
                    remote_flags,
                })
            }

            // ================================================================
            // FlagsReceived → CapabilitiesNegotiated: Compute effective capabilities
            // ================================================================
            Self::FlagsReceived {
                protocol_version,
                our_flags,
                remote_flags,
            } => {
                // Negotiated capabilities are intersection of flags
                let mut capabilities = ProtocolCapabilities::new(protocol_version);
                capabilities.flags = our_flags & remote_flags;

                info!(
                    "Handshake: Capabilities negotiated: 0x{:08X} (our=0x{:08X}, remote=0x{:08X})",
                    capabilities.flags, our_flags, remote_flags
                );

                debug!("  Checksums: {}", capabilities.supports_checksums());
                debug!("  Hardlinks: {}", capabilities.supports_hardlinks());
                debug!("  Symlinks: {}", capabilities.supports_symlinks());
                debug!("  Devices: {}", capabilities.supports_devices());
                debug!("  Xattrs: {}", capabilities.supports_xattrs());
                debug!("  ACLs: {}", capabilities.supports_acls());
                debug!("  Checksum seed: {}", capabilities.supports_checksum_seed());

                Ok(Self::CapabilitiesNegotiated { capabilities })
            }

            // ================================================================
            // CapabilitiesNegotiated → SeedExchange or Complete
            // ================================================================
            Self::CapabilitiesNegotiated { mut capabilities } => {
                // Check if we need to exchange checksum seeds
                if capabilities.supports_checksum_seed() {
                    debug!("Handshake: Checksum seed exchange required");

                    let seed = match role {
                        Role::Sender => {
                            // Sender generates and sends seed
                            let seed = ChecksumSeed::generate();
                            debug!("Handshake: Sending checksum seed: 0x{:08X}", seed.seed);

                            let bytes = seed.to_bytes();
                            write_all(transport, &bytes).await.map_err(|e| {
                                anyhow::anyhow!("Failed to send checksum seed: {}", e)
                            })?;

                            Some(seed)
                        }
                        Role::Receiver => {
                            // Receiver reads seed from sender
                            let mut bytes = [0u8; 4];
                            read_exact(transport, &mut bytes).await.map_err(|e| {
                                anyhow::anyhow!("Failed to receive checksum seed: {}", e)
                            })?;

                            let seed = ChecksumSeed::from_bytes(bytes);
                            debug!("Handshake: Received checksum seed: 0x{:08X}", seed.seed);

                            Some(seed)
                        }
                    };

                    capabilities.checksum_seed = seed.map(|s| s.seed);

                    Ok(Self::SeedExchange { capabilities })
                } else {
                    debug!("Handshake: No checksum seed exchange needed");

                    info!("Handshake: Complete (no seed exchange)");

                    Ok(Self::Complete {
                        capabilities,
                        seed: None,
                    })
                }
            }

            // ================================================================
            // SeedExchange → Complete
            // ================================================================
            Self::SeedExchange { capabilities } => {
                let seed = capabilities.checksum_seed.map(|s| ChecksumSeed { seed: s });

                info!("Handshake: Complete (with checksum seed)");

                Ok(Self::Complete { capabilities, seed })
            }

            // ================================================================
            // Complete: Terminal state
            // ================================================================
            Self::Complete { .. } => {
                anyhow::bail!("Handshake already complete")
            }
        }
    }
}

// ============================================================================
// Public API Functions
// ============================================================================

/// Perform handshake as sender
///
/// This is a convenience function that runs the complete handshake state machine
/// in sender mode. The sender is the side that will be sending files.
///
/// # Errors
///
/// Returns an error if:
/// - Transport I/O fails
/// - Protocol version is incompatible
/// - Handshake fails for any reason
///
/// # Example
///
/// ```rust,no_run
/// use arsync::protocol::handshake::handshake_sender;
/// use arsync::protocol::pipe::PipeTransport;
///
/// # async fn example() -> anyhow::Result<()> {
/// let mut transport = PipeTransport::from_stdio()?;
/// let capabilities = handshake_sender(&mut transport).await?;
/// println!("Handshake complete! Version: {}", capabilities.version);
/// # Ok(())
/// # }
/// ```
#[allow(dead_code)] // Part of protocol spec, not yet fully used
pub async fn handshake_sender<T: Transport>(transport: &mut T) -> Result<ProtocolCapabilities> {
    info!("Starting handshake as sender");

    let mut state = HandshakeState::Initial;

    while !state.is_complete() {
        state = state.advance(transport, Role::Sender).await?;
    }

    let capabilities = state
        .get_capabilities()
        .ok_or_else(|| anyhow::anyhow!("Handshake completed but capabilities not set"))?
        .clone();

    info!(
        "Handshake complete (sender): version={}, flags=0x{:08X}",
        capabilities.version, capabilities.flags
    );

    Ok(capabilities)
}

/// Perform handshake as receiver
///
/// This is a convenience function that runs the complete handshake state machine
/// in receiver mode. The receiver is the side that will be receiving files.
///
/// # Errors
///
/// Returns an error if:
/// - Transport I/O fails
/// - Protocol version is incompatible
/// - Handshake fails for any reason
///
/// # Example
///
/// ```rust,no_run
/// use arsync::protocol::handshake::handshake_receiver;
/// use arsync::protocol::pipe::PipeTransport;
///
/// # async fn example() -> anyhow::Result<()> {
/// let mut transport = PipeTransport::from_stdio()?;
/// let capabilities = handshake_receiver(&mut transport).await?;
/// println!("Handshake complete! Version: {}", capabilities.version);
/// # Ok(())
/// # }
/// ```
#[allow(dead_code)] // Part of protocol spec, not yet fully used
pub async fn handshake_receiver<T: Transport>(transport: &mut T) -> Result<ProtocolCapabilities> {
    info!("Starting handshake as receiver");

    let mut state = HandshakeState::Initial;

    while !state.is_complete() {
        state = state.advance(transport, Role::Receiver).await?;
    }

    let capabilities = state
        .get_capabilities()
        .ok_or_else(|| anyhow::anyhow!("Handshake completed but capabilities not set"))?
        .clone();

    info!(
        "Handshake complete (receiver): version={}, flags=0x{:08X}",
        capabilities.version, capabilities.flags
    );

    Ok(capabilities)
}

/// Perform handshake with specified role
///
/// This is the most general handshake function that accepts a role parameter.
/// Use `handshake_sender()` or `handshake_receiver()` for more convenient APIs.
///
/// # Errors
///
/// Returns an error if:
/// - Transport I/O fails
/// - Protocol version is incompatible
/// - Handshake fails for any reason
///
/// # Example
///
/// ```rust,no_run
/// use arsync::protocol::handshake::{handshake, Role};
/// use arsync::protocol::pipe::PipeTransport;
///
/// # async fn example() -> anyhow::Result<()> {
/// let mut transport = PipeTransport::from_stdio()?;
/// let role = Role::Sender;
/// let capabilities = handshake(&mut transport, role).await?;
/// println!("Handshake complete! Version: {}", capabilities.version);
/// # Ok(())
/// # }
/// ```
#[allow(dead_code)] // Part of protocol spec, not yet fully used
pub async fn handshake<T: Transport>(
    transport: &mut T,
    role: Role,
) -> Result<ProtocolCapabilities> {
    match role {
        Role::Sender => handshake_sender(transport).await,
        Role::Receiver => handshake_receiver(transport).await,
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Get our supported capabilities
///
/// Returns the capability flags that arsync supports. These represent features
/// that arsync has ALREADY IMPLEMENTED locally:
///
/// - **Checksums**: Delta algorithm with rolling + strong checksums ✅
/// - **Symlinks**: Copy symlinks as symlinks (-l/--links) ✅
/// - **Hard links**: Track and preserve hard links (-H/--hard-links) ✅
/// - **Devices**: Preserve device files and special files (-D/--devices) ✅
/// - **Extended attributes**: Preserve xattrs (-X/--xattrs) ✅
/// - **ACLs**: Preserve POSIX ACLs (-A/--acls) ✅
/// - **Checksum seed**: Use random seed for security ✅
/// - **Permissions**: Preserve file permissions (-p/--perms) ✅
/// - **Timestamps**: Preserve modification times (-t/--times) ✅
///
/// Note: All these features are fully implemented in arsync's local sync code.
/// The wire protocol just needs to transmit these attributes.
#[must_use]
pub fn get_our_capabilities() -> u32 {
    let mut flags = 0u32;

    // What we support (arsync has LOCAL support for all of these!)
    flags |= XMIT_CHECKSUMS; // ✅ Checksums
    flags |= XMIT_SYMLINKS; // ✅ Symlinks (-l/--links)
    flags |= XMIT_HARDLINKS; // ✅ Hard links (-H/--hard-links)
    flags |= XMIT_DEVICES; // ✅ Device files (-D/--devices)
    flags |= XMIT_XATTRS; // ✅ Extended attributes (-X/--xattrs)
    flags |= XMIT_ACLS; // ✅ POSIX ACLs (-A/--acls)
    flags |= XMIT_CHECKSUM_SEED; // ✅ Checksum seed
    flags |= XMIT_PROTECTION; // ✅ Permissions (-p/--perms)
    flags |= XMIT_TIMES; // ✅ Timestamps (-t/--times)
                         // Note: We also support -U/--atimes and --crtimes locally!

    // What we don't support in wire protocol yet
    // (but may add in future)
    // flags |= XMIT_SPARSE;      // Sparse file optimization (rsync-specific)

    flags
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checksum_seed_roundtrip() {
        let original = ChecksumSeed { seed: 0x12345678 };
        let bytes = original.to_bytes();
        let decoded = ChecksumSeed::from_bytes(bytes);
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_checksum_seed_generate() {
        let seeds: Vec<ChecksumSeed> = (0..100).map(|_| ChecksumSeed::generate()).collect();

        // All should be non-zero
        assert!(seeds.iter().all(|s| !s.is_zero()));

        // They should not all be the same (randomness check)
        let first = seeds[0].seed;
        assert!(seeds.iter().any(|s| s.seed != first));
    }

    #[test]
    fn test_capabilities_negotiation() {
        let mut client = ProtocolCapabilities::new(31);
        client.flags = XMIT_CHECKSUMS | XMIT_SYMLINKS | XMIT_XATTRS;

        let mut server = ProtocolCapabilities::new(30);
        server.flags = XMIT_CHECKSUMS | XMIT_XATTRS | XMIT_ACLS;

        let negotiated = ProtocolCapabilities::negotiate(&client, &server);

        assert_eq!(negotiated.version, 30); // Minimum
        assert!(negotiated.supports_checksums()); // Both support
        assert!(negotiated.supports_xattrs()); // Both support
        assert!(!negotiated.supports_symlinks()); // Only client supports
        assert!(!negotiated.supports_acls()); // Only server supports
    }

    #[test]
    fn test_capabilities_support_methods() {
        let mut caps = ProtocolCapabilities::new(31);

        assert!(!caps.supports_checksums());
        caps.flags |= XMIT_CHECKSUMS;
        assert!(caps.supports_checksums());

        assert!(!caps.supports_symlinks());
        caps.flags |= XMIT_SYMLINKS;
        assert!(caps.supports_symlinks());
    }

    #[test]
    fn test_role_methods() {
        let sender = Role::Sender;
        assert!(sender.is_sender());
        assert!(!sender.is_receiver());
        assert_eq!(sender.opposite(), Role::Receiver);

        let receiver = Role::Receiver;
        assert!(receiver.is_receiver());
        assert!(!receiver.is_sender());
        assert_eq!(receiver.opposite(), Role::Sender);
    }

    #[test]
    fn test_handshake_state_initial() {
        let state = HandshakeState::Initial;
        assert!(!state.is_complete());
        assert!(state.get_capabilities().is_none());
        assert!(state.get_seed().is_none());
    }

    #[test]
    fn test_handshake_state_complete() {
        let caps = ProtocolCapabilities::new(31);
        let seed = ChecksumSeed { seed: 12345 };

        let state = HandshakeState::Complete {
            capabilities: caps.clone(),
            seed: Some(seed),
        };

        assert!(state.is_complete());
        assert!(state.get_capabilities().is_some());
        assert_eq!(state.get_capabilities().unwrap().version, 31);
        assert_eq!(state.get_seed(), Some(seed));
    }
}
