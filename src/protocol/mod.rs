//! Remote synchronization protocol foundation
//!
//! This module provides the foundational types for remote sync:
//! - `Location` enum for parsing local/remote paths
//! - `PipeRole` enum for sender/receiver roles
//! - `Transport` trait for bidirectional byte streams
//! - `PipeTransport` for testing

use anyhow::Result;
use std::path::PathBuf;

// Protocol implementation modules (only available with remote-sync feature)
#[cfg(feature = "remote-sync")]
pub mod checksum;
#[cfg(feature = "remote-sync")]
pub mod handshake;
#[cfg(feature = "remote-sync")]
pub mod pipe;
#[cfg(feature = "remote-sync")]
pub mod rsync;
#[cfg(feature = "remote-sync")]
pub mod rsync_compat;
#[cfg(feature = "remote-sync")]
pub mod ssh;
#[cfg(feature = "remote-sync")]
pub mod transport;
#[cfg(feature = "remote-sync")]
pub mod varint;

/// Parsed location (local or remote)
#[derive(Debug, Clone)]
pub enum Location {
    /// Local filesystem path
    Local(PathBuf),
    /// Remote path accessed via SSH
    Remote {
        /// Remote username (None = current user)
        user: Option<String>,
        /// Remote hostname or IP address
        host: String,
        /// Remote filesystem path
        path: PathBuf,
    },
}

impl Location {
    /// Parse rsync-style path: `[user@]host:path` or `/local/path`
    ///
    /// # Errors
    ///
    /// Returns an error if the path string is invalid (reserved for future validation)
    #[allow(clippy::unnecessary_wraps)] // May add validation in future
    pub fn parse(s: &str) -> Result<Self> {
        // Check for remote syntax: [user@]host:path
        if let Some(colon_pos) = s.find(':') {
            // Could be remote or Windows path (C:\...)
            // Windows paths have letter:\ pattern
            if colon_pos == 1 && s.chars().nth(0).is_some_and(|c| c.is_ascii_alphabetic()) {
                // Likely Windows path
                return Ok(Self::Local(PathBuf::from(s)));
            }

            let host_part = &s[..colon_pos];
            let path_part = &s[colon_pos + 1..];

            // Parse user@host or just host
            let (user, host) = host_part.find('@').map_or_else(
                || (None, host_part.to_string()),
                |at_pos| {
                    (
                        Some(host_part[..at_pos].to_string()),
                        host_part[at_pos + 1..].to_string(),
                    )
                },
            );

            Ok(Self::Remote {
                user,
                host,
                path: PathBuf::from(path_part),
            })
        } else {
            // Local path
            Ok(Self::Local(PathBuf::from(s)))
        }
    }

    /// Get the path component
    #[must_use]
    #[allow(dead_code)] // Will be used by protocol implementation
    pub const fn path(&self) -> &PathBuf {
        match self {
            Self::Local(path) | Self::Remote { path, .. } => path,
        }
    }

    /// Check if this is a remote location
    #[must_use]
    pub const fn is_remote(&self) -> bool {
        matches!(self, Self::Remote { .. })
    }

    /// Check if this is a local location
    #[must_use]
    pub const fn is_local(&self) -> bool {
        matches!(self, Self::Local(_))
    }
}

/// Role in pipe-based protocol testing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipeRole {
    /// Send data (like rsync client)
    Sender,
    /// Receive data (like rsync server)
    Receiver,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_location_parse_local_path() {
        // Test: Parse simple local path
        // Requirement: Location::parse should recognize local paths without colons
        let loc = Location::parse("/home/user/data").unwrap();
        assert!(loc.is_local());
        assert!(!loc.is_remote());
        assert_eq!(loc.path(), &PathBuf::from("/home/user/data"));
    }

    #[test]
    fn test_location_parse_local_relative_path() {
        // Test: Parse relative local path
        // Requirement: Location::parse should handle relative paths
        let loc = Location::parse("./data").unwrap();
        assert!(loc.is_local());
        assert_eq!(loc.path(), &PathBuf::from("./data"));
    }

    #[test]
    fn test_location_parse_windows_path() {
        // Test: Parse Windows-style path (C:\path)
        // Requirement: Location::parse should recognize Windows paths as local
        let loc = Location::parse("C:\\Users\\user\\data").unwrap();
        assert!(loc.is_local());
        assert_eq!(loc.path(), &PathBuf::from("C:\\Users\\user\\data"));
    }

    #[test]
    fn test_location_parse_remote_with_user() {
        // Test: Parse remote path with username (user@host:path)
        // Requirement: Location::parse should extract user, host, and path
        let loc = Location::parse("alice@example.com:/data").unwrap();
        assert!(loc.is_remote());
        assert!(!loc.is_local());
        assert!(matches!(loc, Location::Remote { .. }));

        if let Location::Remote { user, host, path } = loc {
            assert_eq!(user, Some("alice".to_string()));
            assert_eq!(host, "example.com");
            assert_eq!(path, PathBuf::from("/data"));
        }
    }

    #[test]
    fn test_location_parse_remote_without_user() {
        // Test: Parse remote path without username (host:path)
        // Requirement: Location::parse should handle missing username
        let loc = Location::parse("server.local:/backup").unwrap();
        assert!(loc.is_remote());
        assert!(matches!(loc, Location::Remote { .. }));

        if let Location::Remote { user, host, path } = loc {
            assert_eq!(user, None);
            assert_eq!(host, "server.local");
            assert_eq!(path, PathBuf::from("/backup"));
        }
    }

    #[test]
    fn test_location_parse_remote_with_ip() {
        // Test: Parse remote path with IP address
        // Requirement: Location::parse should handle IP addresses as hosts
        let loc = Location::parse("192.168.1.100:/mnt/storage").unwrap();
        assert!(loc.is_remote());
        assert!(matches!(loc, Location::Remote { .. }));

        if let Location::Remote { user, host, path } = loc {
            assert_eq!(user, None);
            assert_eq!(host, "192.168.1.100");
            assert_eq!(path, PathBuf::from("/mnt/storage"));
        }
    }

    #[test]
    fn test_location_parse_remote_relative_path() {
        // Test: Parse remote path with relative path
        // Requirement: Location::parse should handle relative paths on remote hosts
        let loc = Location::parse("user@host:relative/path").unwrap();
        assert!(loc.is_remote());
        assert!(matches!(loc, Location::Remote { .. }));

        if let Location::Remote { user, host, path } = loc {
            assert_eq!(user, Some("user".to_string()));
            assert_eq!(host, "host");
            assert_eq!(path, PathBuf::from("relative/path"));
        }
    }

    #[test]
    fn test_location_parse_remote_empty_path() {
        // Test: Parse remote path with empty path component
        // Requirement: Location::parse should handle empty paths
        let loc = Location::parse("host:").unwrap();
        assert!(loc.is_remote());
        assert!(matches!(loc, Location::Remote { .. }));

        if let Location::Remote { user, host, path } = loc {
            assert_eq!(user, None);
            assert_eq!(host, "host");
            assert_eq!(path, PathBuf::from(""));
        }
    }

    #[test]
    fn test_location_path_getter() {
        // Test: path() method returns correct path for both Local and Remote
        // Requirement: Location::path() should work for all Location variants
        let local = Location::parse("/local/path").unwrap();
        assert_eq!(local.path(), &PathBuf::from("/local/path"));

        let remote = Location::parse("host:/remote/path").unwrap();
        assert_eq!(remote.path(), &PathBuf::from("/remote/path"));
    }
}
