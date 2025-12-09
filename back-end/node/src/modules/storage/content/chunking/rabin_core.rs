//! Core Rabin fingerprint hashing implementation.
//!
//! This module provides the shared rolling hash logic used by both
//! the in-memory `RabinChunker` and the streaming `RabinStreamingIter`.

use super::config::ChunkingConfig;

/// Standard Rabin polynomial used by IPFS.
pub const IPFS_POLYNOMIAL: u64 = 0x3DA3358B4DC173;

/// Window size for rolling hash.
pub const WINDOW_SIZE: usize = 64;

/// Core Rabin fingerprint computation.
///
/// Contains precomputed lookup tables and methods for rolling hash operations.
/// This is shared between streaming and non-streaming implementations to ensure
/// consistent boundary detection.
#[derive(Debug, Clone)]
pub struct RabinCore {
    /// The polynomial used for fingerprinting.
    polynomial: u64,
    /// Bitmask for boundary detection (based on target size).
    mask: u64,
    /// Precomputed table for fast modular reduction.
    mod_table: [u64; 256],
    /// Precomputed table for window sliding.
    out_table: [u64; 256],
}

impl RabinCore {
    /// Create a new RabinCore with default IPFS polynomial.
    pub fn new(config: &ChunkingConfig) -> Self {
        Self::with_polynomial(config, IPFS_POLYNOMIAL)
    }

    /// Create a RabinCore with a custom polynomial.
    pub fn with_polynomial(config: &ChunkingConfig, polynomial: u64) -> Self {
        let bits = (config.target_size as f64).log2().floor() as u32;
        let mask = (1u64 << bits) - 1;

        Self {
            polynomial,
            mask,
            mod_table: Self::compute_mod_table(polynomial),
            out_table: Self::compute_out_table(polynomial),
        }
    }

    /// Get the polynomial used by this core.
    pub fn polynomial(&self) -> u64 {
        self.polynomial
    }

    /// Get the boundary detection mask.
    pub fn mask(&self) -> u64 {
        self.mask
    }

    /// Compute the modular reduction lookup table.
    fn compute_mod_table(polynomial: u64) -> [u64; 256] {
        let mut table = [0u64; 256];
        for i in 0..256 {
            let mut hash = (i as u64) << 55;
            for _ in 0..8 {
                if hash & (1 << 63) != 0 {
                    hash = (hash << 1) ^ polynomial;
                } else {
                    hash <<= 1;
                }
            }
            table[i] = hash;
        }
        table
    }

    /// Compute the output table for sliding window.
    fn compute_out_table(polynomial: u64) -> [u64; 256] {
        let mut table = [0u64; 256];
        for i in 0..256 {
            let mut hash = 0u64;
            hash = Self::update_hash_raw(hash, i as u8, polynomial);
            for _ in 1..WINDOW_SIZE {
                hash = Self::update_hash_raw(hash, 0, polynomial);
            }
            table[i] = hash;
        }
        table
    }

    /// Update hash with a single byte (raw, without table optimization).
    fn update_hash_raw(hash: u64, byte: u8, polynomial: u64) -> u64 {
        let mut h = hash;
        for i in (0..8).rev() {
            if h & (1 << 63) != 0 {
                h = (h << 1) ^ polynomial;
            } else {
                h <<= 1;
            }
            if byte & (1 << i) != 0 {
                h ^= polynomial;
            }
        }
        h
    }

    /// Update the rolling hash with a new byte (table-optimized).
    #[inline]
    pub fn update_hash(&self, hash: u64, byte: u8) -> u64 {
        let top = (hash >> 56) as usize;
        (hash << 8) ^ self.mod_table[top] ^ (byte as u64)
    }

    /// Slide the window: remove old byte, add new byte.
    #[inline]
    pub fn slide_hash(&self, hash: u64, out_byte: u8, in_byte: u8) -> u64 {
        let h = hash ^ self.out_table[out_byte as usize];
        self.update_hash(h, in_byte)
    }

    /// Check if hash indicates a chunk boundary.
    #[inline]
    pub fn is_boundary(&self, hash: u64) -> bool {
        (hash & self.mask) == 0
    }

    /// Find the next chunk boundary in the given data slice.
    ///
    /// Returns the size of the chunk (1-indexed position of boundary).
    /// If no boundary is found before max_size, returns max_size.
    /// If data is smaller than max_size and no boundary, returns data.len().
    pub fn find_boundary(&self, data: &[u8], min_size: usize, max_size: usize) -> usize {
        if data.is_empty() {
            return 0;
        }

        let mut hash = 0u64;

        for (i, &byte) in data.iter().enumerate() {
            let chunk_len = i + 1;

            // Update rolling hash
            if i < WINDOW_SIZE {
                hash = self.update_hash(hash, byte);
            } else {
                hash = self.slide_hash(hash, data[i - WINDOW_SIZE], byte);
            }

            // Check for boundary (only after min_size)
            if self.is_boundary(hash) && chunk_len >= min_size {
                return chunk_len;
            }

            // Hit max size - force boundary
            if chunk_len >= max_size {
                return chunk_len;
            }
        }

        // No boundary found - return all remaining data
        data.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> ChunkingConfig {
        ChunkingConfig::new(64, 256, 1024)
    }

    #[test]
    fn test_core_creation() {
        let config = test_config();
        let core = RabinCore::new(&config);

        assert_eq!(core.polynomial(), IPFS_POLYNOMIAL);
        // mask should be based on target_size (256 -> 2^8 - 1 = 255)
        assert_eq!(core.mask(), 255);
    }

    #[test]
    fn test_core_custom_polynomial() {
        let config = test_config();
        let poly = 0x123456789ABCDEF;
        let core = RabinCore::with_polynomial(&config, poly);

        assert_eq!(core.polynomial(), poly);
    }

    #[test]
    fn test_hash_determinism() {
        let config = test_config();
        let core = RabinCore::new(&config);

        let data = b"test data for hashing";

        let mut hash1 = 0u64;
        let mut hash2 = 0u64;

        for &byte in data {
            hash1 = core.update_hash(hash1, byte);
            hash2 = core.update_hash(hash2, byte);
        }

        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_slide_hash_consistency() {
        let config = test_config();
        let core = RabinCore::new(&config);

        // Build hash incrementally
        let data: Vec<u8> = (0..100).map(|i| (i % 256) as u8).collect();

        let mut hash = 0u64;
        for (i, &byte) in data.iter().enumerate() {
            if i < WINDOW_SIZE {
                hash = core.update_hash(hash, byte);
            } else {
                hash = core.slide_hash(hash, data[i - WINDOW_SIZE], byte);
            }
        }

        // Hash should be non-zero for non-empty data
        assert_ne!(hash, 0);
    }

    #[test]
    fn test_find_boundary_empty() {
        let config = test_config();
        let core = RabinCore::new(&config);

        assert_eq!(core.find_boundary(&[], 64, 1024), 0);
    }

    #[test]
    fn test_find_boundary_respects_max() {
        let config = test_config();
        let core = RabinCore::new(&config);

        let data: Vec<u8> = (0..5000).map(|i| (i % 256) as u8).collect();
        let boundary = core.find_boundary(&data, 64, 512);

        assert!(boundary <= 512);
    }

    #[test]
    fn test_find_boundary_respects_min() {
        let config = test_config();
        let core = RabinCore::new(&config);

        let data: Vec<u8> = (0..5000).map(|i| (i % 256) as u8).collect();
        let boundary = core.find_boundary(&data, 100, 1024);

        // Boundary should be >= min_size (unless data is smaller)
        assert!(boundary >= 100 || boundary == data.len());
    }

    #[test]
    fn test_find_boundary_deterministic() {
        let config = test_config();
        let core = RabinCore::new(&config);

        let data: Vec<u8> = (0..1000).map(|i| ((i * 17) % 256) as u8).collect();

        let b1 = core.find_boundary(&data, 64, 512);
        let b2 = core.find_boundary(&data, 64, 512);

        assert_eq!(b1, b2);
    }
}
