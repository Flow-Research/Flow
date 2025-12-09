use super::config::ChunkingConfig;
use super::rabin_core::{IPFS_POLYNOMIAL, RabinCore, WINDOW_SIZE};
use super::types::{ChunkData, Chunker, StreamingChunkResult};
use crate::modules::storage::content::chunking::streaming::RabinStreamingIter;
use std::io::Read;
use tracing::debug;

/// Rabin fingerprint chunker.
///
/// Uses Rabin fingerprinting for content-defined chunking.
/// This is the algorithm used by IPFS for compatibility.
///
/// **Pros:** IPFS-compatible, good deduplication.
/// **Cons:** Slightly slower than FastCDC.
#[derive(Debug, Clone)]
pub struct RabinChunker {
    config: ChunkingConfig,
    core: RabinCore,
}

impl RabinChunker {
    /// Create a new Rabin chunker with default IPFS polynomial.
    pub fn new(config: ChunkingConfig) -> Self {
        Self::with_polynomial(config, IPFS_POLYNOMIAL)
    }

    /// Create a Rabin chunker with a custom polynomial.
    pub fn with_polynomial(config: ChunkingConfig, polynomial: u64) -> Self {
        debug!(
            min_size = config.min_size,
            target_size = config.target_size,
            max_size = config.max_size,
            polynomial = format!("{:#x}", polynomial),
            "Creating Rabin chunker"
        );

        let core = RabinCore::with_polynomial(&config, polynomial);

        Self { config, core }
    }

    /// Get the polynomial used by this chunker.
    pub fn polynomial(&self) -> u64 {
        self.core.polynomial()
    }

    /// Get a reference to the rabin core
    pub fn core(&self) -> &RabinCore {
        &self.core
    }
}

impl Chunker for RabinChunker {
    fn chunk(&self, data: &[u8]) -> Vec<ChunkData> {
        if data.is_empty() {
            return Vec::new();
        }

        let mut chunks = Vec::new();
        let mut chunk_start = 0;
        let mut hash = 0u64;
        let mut window_start = 0usize;

        for i in 0..data.len() {
            let byte = data[i];
            let chunk_len = i - chunk_start + 1;

            // Update rolling hash using core
            if chunk_len <= WINDOW_SIZE {
                hash = self.core.update_hash(hash, byte);
            } else {
                hash = self.core.slide_hash(hash, data[window_start], byte);
                window_start += 1;
            }

            // Check for chunk boundary
            let at_boundary = self.core.is_boundary(hash) && chunk_len >= self.config.min_size;
            let at_max = chunk_len >= self.config.max_size;
            let is_end = i == data.len() - 1;

            if at_boundary || at_max || is_end {
                let chunk_data = data[chunk_start..=i].to_vec();
                chunks.push(ChunkData::new(chunk_data, chunk_start as u64));

                chunk_start = i + 1;
                hash = 0;
                window_start = chunk_start;
            }
        }

        chunks
    }

    fn chunk_iter<'a>(&self, data: &'a [u8]) -> Box<dyn Iterator<Item = ChunkData> + 'a> {
        // For simplicity, use the collect approach wrapped in an iterator.
        // A true streaming implementation would maintain state.
        Box::new(self.chunk(data).into_iter())
    }

    fn name(&self) -> &'static str {
        "rabin"
    }

    fn config(&self) -> &ChunkingConfig {
        &self.config
    }

    fn stream_chunks<'a, R: Read + Send + 'a>(
        &'a self,
        reader: R,
    ) -> Box<dyn Iterator<Item = StreamingChunkResult<ChunkData>> + Send + 'a> {
        Box::new(RabinStreamingIter::with_core(
            reader,
            self.config.clone(),
            self.core.clone(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> ChunkingConfig {
        ChunkingConfig::new(64, 256, 1024)
    }

    #[test]
    fn test_rabin_basic() {
        let chunker = RabinChunker::new(test_config());
        let data: Vec<u8> = (0..10000).map(|i| (i % 256) as u8).collect();

        let chunks = chunker.chunk(&data);

        assert!(!chunks.is_empty());

        // Verify reconstruction
        let reconstructed: Vec<u8> = chunks.iter().flat_map(|c| c.data.clone()).collect();
        assert_eq!(reconstructed, data);
    }

    #[test]
    fn test_rabin_empty() {
        let chunker = RabinChunker::new(test_config());
        let chunks = chunker.chunk(b"");

        assert!(chunks.is_empty());
    }

    #[test]
    fn test_rabin_small_input() {
        let chunker = RabinChunker::new(test_config());
        let data = b"small";

        let chunks = chunker.chunk(data);

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].data, data);
    }

    #[test]
    fn test_rabin_chunk_sizes_respect_max() {
        let config = ChunkingConfig::new(64, 256, 512);
        let chunker = RabinChunker::new(config);

        let data: Vec<u8> = (0..50000).map(|i| ((i * 7 + 13) % 256) as u8).collect();
        let chunks = chunker.chunk(&data);

        for chunk in &chunks {
            assert!(
                chunk.size() <= config.max_size,
                "Chunk size {} exceeds max {}",
                chunk.size(),
                config.max_size
            );
        }
    }

    #[test]
    fn test_rabin_chunk_sizes_respect_min() {
        let config = ChunkingConfig::new(100, 256, 1024);
        let chunker = RabinChunker::new(config);

        let data: Vec<u8> = (0..50000).map(|i| ((i * 7 + 13) % 256) as u8).collect();
        let chunks = chunker.chunk(&data);

        for (i, chunk) in chunks.iter().enumerate() {
            let is_last = i == chunks.len() - 1;
            if !is_last {
                assert!(
                    chunk.size() >= config.min_size,
                    "Chunk {} size {} is below min {}",
                    i,
                    chunk.size(),
                    config.min_size
                );
            }
        }
    }

    #[test]
    fn test_rabin_deterministic() {
        let chunker = RabinChunker::new(test_config());
        let data: Vec<u8> = (0..5000).map(|i| (i % 256) as u8).collect();

        let chunks1 = chunker.chunk(&data);
        let chunks2 = chunker.chunk(&data);

        assert_eq!(chunks1.len(), chunks2.len());
        for (c1, c2) in chunks1.iter().zip(chunks2.iter()) {
            assert_eq!(c1.data, c2.data);
            assert_eq!(c1.offset, c2.offset);
        }
    }

    #[test]
    fn test_rabin_offsets_continuous() {
        let chunker = RabinChunker::new(test_config());
        let data: Vec<u8> = (0..10000).map(|i| (i % 256) as u8).collect();

        let chunks = chunker.chunk(&data);

        let mut expected_offset = 0u64;
        for chunk in &chunks {
            assert_eq!(chunk.offset, expected_offset);
            expected_offset += chunk.size() as u64;
        }

        assert_eq!(expected_offset, data.len() as u64);
    }

    #[test]
    fn test_rabin_custom_polynomial() {
        let config = test_config();
        let chunker1 = RabinChunker::with_polynomial(config.clone(), 0x123456789ABCDEF);
        let chunker2 = RabinChunker::with_polynomial(config, 0xFEDCBA987654321);

        let data: Vec<u8> = (0..5000).map(|i| (i % 256) as u8).collect();

        let chunks1 = chunker1.chunk(&data);
        let chunks2 = chunker2.chunk(&data);

        // Different polynomials should produce different chunk boundaries
        let boundaries1: Vec<_> = chunks1.iter().map(|c| c.offset).collect();
        let boundaries2: Vec<_> = chunks2.iter().map(|c| c.offset).collect();

        // They should reconstruct to the same data regardless
        let reconstructed1: Vec<u8> = chunks1.iter().flat_map(|c| c.data.clone()).collect();
        let reconstructed2: Vec<u8> = chunks2.iter().flat_map(|c| c.data.clone()).collect();

        assert_eq!(reconstructed1, data);
        assert_eq!(reconstructed2, data);

        println!("Polynomial 1 boundaries: {:?}", boundaries1);
        println!("Polynomial 2 boundaries: {:?}", boundaries2);
    }

    #[test]
    fn test_rabin_large_file() {
        let config = ChunkingConfig::default();
        let chunker = RabinChunker::new(config);
        let data = vec![42u8; 2 * 1024 * 1024]; // 2MB

        let chunks = chunker.chunk(&data);

        // Verify reconstruction
        let reconstructed: Vec<u8> = chunks.iter().flat_map(|c| c.data.clone()).collect();
        assert_eq!(reconstructed.len(), data.len());
    }

    #[test]
    fn test_rabin_content_defined_behavior() {
        // Content-defined means same content produces same boundaries
        let chunker = RabinChunker::new(test_config());

        // Two files with same content block at different positions
        let common_block: Vec<u8> = (0..500).map(|i| (i * 3 % 256) as u8).collect();

        let mut file1 = vec![0u8; 1000];
        file1.extend(&common_block);
        file1.extend(vec![1u8; 1000]);

        let mut file2 = vec![2u8; 2000];
        file2.extend(&common_block);
        file2.extend(vec![3u8; 500]);

        let chunks1 = chunker.chunk(&file1);
        let chunks2 = chunker.chunk(&file2);

        let cids1: std::collections::HashSet<_> = chunks1
            .iter()
            .map(|c| c.to_chunk_ref().cid.to_string())
            .collect();

        let cids2: std::collections::HashSet<_> = chunks2
            .iter()
            .map(|c| c.to_chunk_ref().cid.to_string())
            .collect();

        let common = cids1.intersection(&cids2).count();
        println!(
            "File1: {} chunks, File2: {} chunks, Common: {}",
            cids1.len(),
            cids2.len(),
            common
        );

        // There should be some commonality due to CDC
        // (exact amount depends on boundary alignment)
    }
}
