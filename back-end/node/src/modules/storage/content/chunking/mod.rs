//! Chunking infrastructure for splitting files into content-addressed blocks.
//!
//! This module provides a flexible chunking system supporting multiple algorithms:
//! - **Fixed**: Simple fixed-size chunks (fast, poor deduplication)
//! - **FastCDC**: Content-defined chunking (fast, excellent deduplication)
//! - **Rabin**: Content-defined chunking (IPFS-compatible)
//!
//! # Example
//!
//! ```rust
//! use node::modules::storage::content::chunking::{ChunkingAlgorithm, ChunkingConfig};
//!
//! let config = ChunkingConfig::default();
//! let chunker = ChunkingAlgorithm::FastCdc.create_chunker(config);
//!
//! let data = b"Hello, world!".repeat(10000);
//! let chunks = chunker.chunk(&data);
//! ```

mod config;
mod fastcdc;
mod fixed;
mod rabin;
mod rabin_core;
mod streaming;
mod types;

pub use config::ChunkingConfig;
pub use fastcdc::FastCdcChunker;
pub use fixed::FixedChunker;
pub use rabin::RabinChunker;
pub use rabin_core::{IPFS_POLYNOMIAL, RabinCore, WINDOW_SIZE};
use serde::{Deserialize, Serialize};
pub use types::{ChunkData, ChunkRef, Chunker};

use tracing::info;

/// Available chunking algorithms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChunkingAlgorithm {
    /// Fixed-size chunks. Simple but poor deduplication when content shifts.
    Fixed,
    /// FastCDC content-defined chunking. Fast with excellent deduplication.
    #[default]
    FastCdc,
    /// Rabin fingerprint chunking. IPFS-compatible content-defined chunking.
    Rabin,
}

impl ChunkingAlgorithm {
    /// Create a chunker with the specified algorithm and configuration.
    pub fn create_chunker(self, config: ChunkingConfig) -> Box<dyn Chunker> {
        info!(
            algorithm = self.name(),
            min_size = config.min_size,
            target_size = config.target_size,
            max_size = config.max_size,
            "Creating chunker"
        );

        match self {
            ChunkingAlgorithm::Fixed => Box::new(FixedChunker::new(config)),
            ChunkingAlgorithm::FastCdc => Box::new(FastCdcChunker::new(config)),
            ChunkingAlgorithm::Rabin => Box::new(RabinChunker::new(config)),
        }
    }

    /// Get the algorithm name as a string.
    pub fn name(&self) -> &'static str {
        match self {
            ChunkingAlgorithm::Fixed => "fixed",
            ChunkingAlgorithm::FastCdc => "fastcdc",
            ChunkingAlgorithm::Rabin => "rabin",
        }
    }

    /// Parse algorithm from string (case-insensitive).
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "fixed" => Some(ChunkingAlgorithm::Fixed),
            "fastcdc" | "fast_cdc" | "fast-cdc" => Some(ChunkingAlgorithm::FastCdc),
            "rabin" => Some(ChunkingAlgorithm::Rabin),
            _ => None,
        }
    }

    /// Load algorithm from environment variable `CHUNKING_ALGORITHM`.
    pub fn from_env() -> Self {
        std::env::var("CHUNKING_ALGORITHM")
            .ok()
            .and_then(|s| Self::from_str(&s))
            .unwrap_or_default()
    }
}

impl std::fmt::Display for ChunkingAlgorithm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}
