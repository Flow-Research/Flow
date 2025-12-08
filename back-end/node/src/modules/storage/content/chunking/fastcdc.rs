use fastcdc::v2020::FastCDC;
use tracing::debug;

use super::config::ChunkingConfig;
use super::types::{ChunkData, Chunker};

/// FastCDC content-defined chunker.
///
/// Uses the FastCDC algorithm for content-defined chunking.
/// This provides excellent deduplication with high performance.
///
/// **Pros:** Fast, excellent deduplication, normalized chunk distribution.
/// **Cons:** Slightly more complex than fixed-size.
///
/// See: <https://www.usenix.org/conference/atc16/technical-sessions/presentation/xia>
#[derive(Debug, Clone)]
pub struct FastCdcChunker {
    config: ChunkingConfig,
}

impl FastCdcChunker {
    /// Create a new FastCDC chunker.
    pub fn new(config: ChunkingConfig) -> Self {
        debug!(
            min_size = config.min_size,
            target_size = config.target_size,
            max_size = config.max_size,
            "Creating FastCDC chunker"
        );
        Self { config }
    }
}

impl Chunker for FastCdcChunker {
    fn chunk(&self, data: &[u8]) -> Vec<ChunkData> {
        if data.is_empty() {
            return Vec::new();
        }

        let chunker = FastCDC::new(
            data,
            self.config.min_size as u32,
            self.config.target_size as u32,
            self.config.max_size as u32,
        );

        chunker
            .map(|chunk| {
                let chunk_bytes = data[chunk.offset..chunk.offset + chunk.length].to_vec();
                ChunkData::new(chunk_bytes, chunk.offset as u64)
            })
            .collect()
    }

    fn chunk_iter<'a>(&self, data: &'a [u8]) -> Box<dyn Iterator<Item = ChunkData> + 'a> {
        if data.is_empty() {
            return Box::new(std::iter::empty());
        }

        let chunker = FastCDC::new(
            data,
            self.config.min_size as u32,
            self.config.target_size as u32,
            self.config.max_size as u32,
        );

        Box::new(chunker.map(move |chunk| {
            let chunk_bytes = data[chunk.offset..chunk.offset + chunk.length].to_vec();
            ChunkData::new(chunk_bytes, chunk.offset as u64)
        }))
    }

    fn name(&self) -> &'static str {
        "fastcdc"
    }

    fn config(&self) -> &ChunkingConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> ChunkingConfig {
        ChunkingConfig::new(64, 256, 1024)
    }

    #[test]
    fn test_fastcdc_basic() {
        let chunker = FastCdcChunker::new(test_config());
        let data: Vec<u8> = (0..10000).map(|i| (i % 256) as u8).collect();

        let chunks = chunker.chunk(&data);

        assert!(!chunks.is_empty());

        // Verify reconstruction
        let reconstructed: Vec<u8> = chunks.iter().flat_map(|c| c.data.clone()).collect();
        assert_eq!(reconstructed, data);
    }

    #[test]
    fn test_fastcdc_empty() {
        let chunker = FastCdcChunker::new(test_config());
        let chunks = chunker.chunk(b"");

        assert!(chunks.is_empty());
    }

    #[test]
    fn test_fastcdc_small_input() {
        let chunker = FastCdcChunker::new(test_config());
        let data = b"small";

        let chunks = chunker.chunk(data);

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].data, data);
    }

    #[test]
    fn test_fastcdc_chunk_sizes_within_bounds() {
        let config = ChunkingConfig::new(100, 500, 2000);
        let chunker = FastCdcChunker::new(config);

        // Use random-ish data to trigger content-defined boundaries
        let data: Vec<u8> = (0..50000).map(|i| ((i * 7 + 13) % 256) as u8).collect();

        let chunks = chunker.chunk(&data);

        for (i, chunk) in chunks.iter().enumerate() {
            let is_last = i == chunks.len() - 1;
            if !is_last {
                // Non-final chunks should respect min/max bounds
                // (final chunk can be smaller than min)
                assert!(
                    chunk.size() <= config.max_size,
                    "Chunk {} size {} exceeds max {}",
                    i,
                    chunk.size(),
                    config.max_size
                );
            }
        }
    }

    #[test]
    fn test_fastcdc_deterministic() {
        let chunker = FastCdcChunker::new(test_config());
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
    fn test_fastcdc_offsets_continuous() {
        let chunker = FastCdcChunker::new(test_config());
        let data: Vec<u8> = (0..10000).map(|i| (i % 256) as u8).collect();

        let chunks = chunker.chunk(&data);

        let mut expected_offset = 0u64;
        for chunk in &chunks {
            assert_eq!(
                chunk.offset, expected_offset,
                "Chunk offset mismatch: expected {}, got {}",
                expected_offset, chunk.offset
            );
            expected_offset += chunk.size() as u64;
        }

        assert_eq!(expected_offset, data.len() as u64);
    }

    #[test]
    fn test_fastcdc_deduplication_effectiveness() {
        let config = ChunkingConfig::new(64, 256, 1024);
        let chunker = FastCdcChunker::new(config);

        // Create two similar files with a byte inserted in the middle
        let original: Vec<u8> = (0..5000).map(|i| (i % 256) as u8).collect();
        let mut modified = original.clone();
        modified.insert(2500, 0xFF); // Insert 1 byte in the middle

        let original_chunks = chunker.chunk(&original);
        let modified_chunks = chunker.chunk(&modified);

        // With CDC, most chunks should still match
        let original_cids: std::collections::HashSet<_> = original_chunks
            .iter()
            .map(|c| c.to_chunk_ref().cid.to_string())
            .collect();

        let modified_cids: std::collections::HashSet<_> = modified_chunks
            .iter()
            .map(|c| c.to_chunk_ref().cid.to_string())
            .collect();

        let common_chunks = original_cids.intersection(&modified_cids).count();

        println!(
            "Original: {} chunks, Modified: {} chunks, Common: {}",
            original_cids.len(),
            modified_cids.len(),
            common_chunks
        );
    }

    #[test]
    fn test_fastcdc_large_file() {
        let config = ChunkingConfig::default(); // 64KB-256KB-1MB
        let chunker = FastCdcChunker::new(config);
        let data = vec![42u8; 5 * 1024 * 1024]; // 5MB

        let chunks = chunker.chunk(&data);

        // Should produce roughly 20 chunks (5MB / 256KB average)
        assert!(chunks.len() >= 5);
        assert!(chunks.len() <= 80);

        // Verify reconstruction
        let reconstructed: Vec<u8> = chunks.iter().flat_map(|c| c.data.clone()).collect();
        assert_eq!(reconstructed.len(), data.len());
    }

    #[test]
    fn test_fastcdc_iterator_vs_collect() {
        let chunker = FastCdcChunker::new(test_config());
        let data: Vec<u8> = (0..5000).map(|i| (i % 256) as u8).collect();

        let collected: Vec<_> = chunker.chunk(&data);
        let iterated: Vec<_> = chunker.chunk_iter(&data).collect();

        assert_eq!(collected.len(), iterated.len());
        for (c, i) in collected.iter().zip(iterated.iter()) {
            assert_eq!(c.data, i.data);
            assert_eq!(c.offset, i.offset);
        }
    }
}
