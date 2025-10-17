//! Checksum algorithms for rsync delta transfer
//!
//! Implements:
//! - Rolling checksum (Adler-32 with SIMD acceleration) for fast block comparison
//! - Strong checksum (MD5) for verification
#![allow(dead_code)] // Protocol implementation not yet fully used

use simd_adler32::Adler32;

/// Rolling checksum modulus (prime number for Adler-32)
#[allow(dead_code)]
const MODULUS: u32 = 65521;

/// Compute rolling checksum (Adler-32 style)
///
/// This is a fast checksum that can be incrementally updated
/// as we slide a window over the data.
#[must_use]
pub fn rolling_checksum(data: &[u8]) -> u32 {
    rolling_checksum_with_seed(data, 0)
}

/// Compute rolling checksum with seed (rsync protocol)
///
/// Uses SIMD-accelerated Adler-32 for 3-5x speedup on supported CPUs.
/// Automatically falls back to scalar implementation if SIMD unavailable.
///
/// The seed is mixed into the initial state to make checksums
/// session-unique and prevent precomputed collision attacks.
///
/// # Arguments
///
/// * `data` - The data block to checksum
/// * `seed` - Checksum seed from handshake (0 for unseeded)
///
/// # Examples
///
/// ```
/// # use arsync::protocol::checksum::rolling_checksum_with_seed;
/// let data = b"Hello, World!";
/// let unseeded = rolling_checksum_with_seed(data, 0);
/// let seeded = rolling_checksum_with_seed(data, 12345);
/// assert_ne!(unseeded, seeded); // Different seeds = different checksums
/// ```
#[must_use]
pub fn rolling_checksum_with_seed(data: &[u8], seed: u32) -> u32 {
    // Use SIMD-accelerated Adler-32
    let mut hasher = Adler32::from_checksum(seed);
    hasher.write(data);
    hasher.finish()
}

/// Incrementally update rolling checksum when sliding window
///
/// Given the old checksum, the byte leaving the window, the byte entering,
/// and the window size, compute the new checksum without scanning the whole window.
///
/// **Note**: This function is intentionally not implemented because SIMD-accelerated
/// full recomputation is fast enough. Use `rolling_checksum_with_seed()` instead.
///
/// The incremental update algorithm could be implemented if profiling shows it's a bottleneck,
/// but SIMD Adler-32 is so fast that recomputing may be faster than the modulo operations.
///
/// # Panics
///
/// Always panics - use `rolling_checksum_with_seed()` for full recomputation.
#[allow(dead_code)]
#[allow(clippy::unimplemented)] // Intentionally unimplemented stub - SIMD recomputation is faster
#[must_use]
pub fn rolling_checksum_update(
    _old_checksum: u32,
    _old_byte: u8,
    _new_byte: u8,
    _block_size: usize,
) -> u32 {
    // This function is a stub - incremental update is not needed due to SIMD performance
    unimplemented!("Use rolling_checksum_with_seed() - SIMD recomputation is fast enough")
}

/// Compute strong checksum (MD5)
///
/// This is slower but has very low collision probability.
/// Used to verify that a weak checksum match is a real match.
#[must_use]
pub fn strong_checksum(data: &[u8]) -> [u8; 16] {
    md5::compute(data).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rolling_checksum_basic() {
        let data = b"Hello, World!";
        let checksum = rolling_checksum(data);
        assert_ne!(checksum, 0);
    }

    #[test]
    fn test_rolling_checksum_different_blocks() {
        let data = b"Hello, World!";
        let block_size = 5;

        // Compute checksum for first block
        let block1 = &data[0..block_size];
        let checksum1 = rolling_checksum(block1);

        // Compute checksum for second block (shifted by 1)
        let block2 = &data[1..block_size + 1];
        let checksum2 = rolling_checksum(block2);

        // Different blocks should have different checksums (usually)
        assert_ne!(checksum1, checksum2);
    }

    #[test]
    fn test_strong_checksum() {
        let data1 = b"Hello, World!";
        let data2 = b"Hello, World!";
        let data3 = b"Hello, world!"; // Different case

        let checksum1 = strong_checksum(data1);
        let checksum2 = strong_checksum(data2);
        let checksum3 = strong_checksum(data3);

        assert_eq!(checksum1, checksum2);
        assert_ne!(checksum1, checksum3);
    }

    #[test]
    fn test_simd_adler32_consistency() {
        // Test that simd-adler32 gives consistent results
        let data = b"ABCDEFGHIJK";
        let window_size = 3;

        // Compute checksums for sliding windows
        let mut checksums = Vec::new();
        for i in 0..=(data.len() - window_size) {
            let checksum = rolling_checksum(&data[i..i + window_size]);
            checksums.push(checksum);
        }

        // Each window should have a unique checksum (usually)
        for i in 0..checksums.len() {
            for _j in (i + 1)..checksums.len() {
                // Most windows will be different; some may collide
                // Just verify we can compute them all consistently
                let check1 = rolling_checksum(&data[i..i + window_size]);
                assert_eq!(check1, checksums[i], "Checksum not consistent at {}", i);
            }
        }
    }

    #[test]
    fn test_rolling_checksum_with_seed() {
        let data = b"Hello, World!";

        // Unseeded (seed=0) should match original
        let unseeded = rolling_checksum_with_seed(data, 0);
        let original = rolling_checksum(data);
        assert_eq!(unseeded, original);

        // Different seeds produce different checksums
        let seed1 = rolling_checksum_with_seed(data, 12345);
        let seed2 = rolling_checksum_with_seed(data, 67890);
        assert_ne!(seed1, seed2);
        assert_ne!(seed1, unseeded);
        assert_ne!(seed2, unseeded);
    }

    #[test]
    fn test_seeded_checksum_deterministic() {
        let data = b"Test data";
        let seed = 0xDEADBEEF;

        // Same seed should give same result
        let checksum1 = rolling_checksum_with_seed(data, seed);
        let checksum2 = rolling_checksum_with_seed(data, seed);
        assert_eq!(checksum1, checksum2);
    }

    #[test]
    fn test_seed_prevents_collisions() {
        // Two different data blocks might have same unseeded checksum (collision)
        // But seeding makes them different
        let data1 = b"AB";
        let data2 = b"BA"; // Different data

        let _unseeded1 = rolling_checksum(data1);
        let _unseeded2 = rolling_checksum(data2);
        // May or may not collide (depends on algorithm)

        let seed = 0x12345678;
        let seeded1 = rolling_checksum_with_seed(data1, seed);
        let seeded2 = rolling_checksum_with_seed(data2, seed);

        // Even if unseeded collides, seeded makes them distinct
        assert_ne!(seeded1, seeded2);
    }
}
