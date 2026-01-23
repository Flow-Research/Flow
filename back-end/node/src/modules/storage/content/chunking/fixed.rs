use super::config::ChunkingConfig;
use super::streaming::FixedStreamingIter;
use super::types::{ChunkData, Chunker, StreamingChunkResult};
use crate::modules::storage::content::ChunkingAlgorithm;
use std::io::Read;
use tracing::debug;

/// Fixed-size chunker.
///
/// Splits input into chunks of exactly `target_size` bytes,
/// with the final chunk being smaller if necessary.
///
/// **Pros:** Simple, fast, predictable chunk sizes.
/// **Cons:** Poor deduplication - inserting 1 byte shifts all boundaries.
#[derive(Debug, Clone)]
pub struct FixedChunker {
    config: ChunkingConfig,
}

impl FixedChunker {
    /// Create a new fixed-size chunker.
    pub fn new(config: ChunkingConfig) -> Self {
        debug!(
            target_size = config.target_size,
            "Creating fixed-size chunker"
        );
        Self { config }
    }

    /// Create with a specific chunk size (uses same size for min/target/max).
    pub fn with_size(size: usize) -> Self {
        Self::new(ChunkingConfig::new(size, size, size))
    }
}

impl Chunker for FixedChunker {
    fn chunk(&self, data: &[u8]) -> Vec<ChunkData> {
        self.chunk_iter(data).collect()
    }

    fn chunk_iter<'a>(&self, data: &'a [u8]) -> Box<dyn Iterator<Item = ChunkData> + 'a> {
        let chunk_size = self.config.target_size;

        Box::new(FixedChunkerIter {
            data,
            chunk_size,
            offset: 0,
        })
    }

    fn name(&self) -> &'static str {
        "fixed"
    }

    fn algorithm(&self) -> ChunkingAlgorithm {
        ChunkingAlgorithm::Fixed
    }

    fn config(&self) -> &ChunkingConfig {
        &self.config
    }

    fn stream_chunks<'a, R: Read + Send + 'a>(
        &'a self,
        reader: R,
    ) -> Box<dyn Iterator<Item = StreamingChunkResult<ChunkData>> + Send + 'a> {
        Box::new(FixedStreamingIter::new(reader, &self.config))
    }
}

/// Iterator for fixed-size chunking.
struct FixedChunkerIter<'a> {
    data: &'a [u8],
    chunk_size: usize,
    offset: usize,
}

impl<'a> Iterator for FixedChunkerIter<'a> {
    type Item = ChunkData;

    fn next(&mut self) -> Option<Self::Item> {
        if self.offset >= self.data.len() {
            return None;
        }

        let start = self.offset;
        let end = (start + self.chunk_size).min(self.data.len());
        let chunk_data = self.data[start..end].to_vec();

        self.offset = end;

        Some(ChunkData::new(chunk_data, start as u64))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixed_chunker_basic() {
        let chunker = FixedChunker::with_size(10);
        let data = b"hello world, this is a test of fixed chunking";

        let chunks = chunker.chunk(data);

        // Verify chunk count
        let expected_count = (data.len() + 9) / 10; // ceiling division
        assert_eq!(chunks.len(), expected_count);

        // Verify reconstruction
        let reconstructed: Vec<u8> = chunks.iter().flat_map(|c| c.data.clone()).collect();
        assert_eq!(reconstructed, data);
    }

    #[test]
    fn test_fixed_chunker_empty() {
        let chunker = FixedChunker::with_size(100);
        let chunks = chunker.chunk(b"");

        assert!(chunks.is_empty());
    }

    #[test]
    fn test_fixed_chunker_smaller_than_chunk_size() {
        let chunker = FixedChunker::with_size(100);
        let data = b"small";

        let chunks = chunker.chunk(data);

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].data, data);
        assert_eq!(chunks[0].offset, 0);
    }

    #[test]
    fn test_fixed_chunker_exact_boundary() {
        let chunker = FixedChunker::with_size(5);
        let data = b"12345"; // Exactly 5 bytes

        let chunks = chunker.chunk(data);

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].data.len(), 5);
    }

    #[test]
    fn test_fixed_chunker_offsets() {
        let chunker = FixedChunker::with_size(10);
        let data = vec![0u8; 35];

        let chunks = chunker.chunk(&data);

        assert_eq!(chunks.len(), 4);
        assert_eq!(chunks[0].offset, 0);
        assert_eq!(chunks[0].size(), 10);
        assert_eq!(chunks[1].offset, 10);
        assert_eq!(chunks[1].size(), 10);
        assert_eq!(chunks[2].offset, 20);
        assert_eq!(chunks[2].size(), 10);
        assert_eq!(chunks[3].offset, 30);
        assert_eq!(chunks[3].size(), 5);
    }

    #[test]
    fn test_fixed_chunker_deterministic() {
        let chunker = FixedChunker::with_size(100);
        let data = b"determinism test data that should chunk the same way every time";

        let chunks1 = chunker.chunk(data);
        let chunks2 = chunker.chunk(data);

        assert_eq!(chunks1.len(), chunks2.len());
        for (c1, c2) in chunks1.iter().zip(chunks2.iter()) {
            assert_eq!(c1.data, c2.data);
            assert_eq!(c1.offset, c2.offset);
        }
    }

    #[test]
    fn test_fixed_chunker_large_file() {
        let chunker = FixedChunker::with_size(1024);
        let data = vec![42u8; 1024 * 1024]; // 1MB

        let chunks = chunker.chunk(&data);

        assert_eq!(chunks.len(), 1024);

        // Verify reconstruction
        let reconstructed: Vec<u8> = chunks.iter().flat_map(|c| c.data.clone()).collect();
        assert_eq!(reconstructed, data);
    }

    #[test]
    fn test_fixed_chunker_iterator_vs_collect() {
        let chunker = FixedChunker::with_size(50);
        let data = vec![1u8; 200];

        let collected: Vec<_> = chunker.chunk(&data);
        let iterated: Vec<_> = chunker.chunk_iter(&data).collect();

        assert_eq!(collected.len(), iterated.len());
        for (c, i) in collected.iter().zip(iterated.iter()) {
            assert_eq!(c.data, i.data);
            assert_eq!(c.offset, i.offset);
        }
    }

    #[test]
    fn test_fixed_chunker_single_byte_target() {
        let chunker = FixedChunker::with_size(1);
        let data = b"abc";

        let chunks = chunker.chunk(data);

        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].data, vec![b'a']);
        assert_eq!(chunks[1].data, vec![b'b']);
        assert_eq!(chunks[2].data, vec![b'c']);
    }

    #[test]
    fn test_fixed_chunker_exactly_2x_target() {
        let chunker = FixedChunker::with_size(5);
        let data = b"1234567890";

        let chunks = chunker.chunk(data);

        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].data, b"12345".to_vec());
        assert_eq!(chunks[1].data, b"67890".to_vec());
    }

    #[test]
    fn test_fixed_chunker_single_byte_input() {
        let chunker = FixedChunker::with_size(100);
        let data = b"x";

        let chunks = chunker.chunk(data);

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].data, b"x".to_vec());
        assert_eq!(chunks[0].offset, 0);
    }

    #[test]
    fn test_fixed_chunker_all_byte_values() {
        let chunker = FixedChunker::with_size(64);
        let data: Vec<u8> = (0..=255).collect();

        let chunks = chunker.chunk(&data);

        // Verify reconstruction preserves all bytes
        let reconstructed: Vec<u8> = chunks.iter().flat_map(|c| c.data.clone()).collect();
        assert_eq!(reconstructed, data);
    }

    #[test]
    fn test_fixed_chunker_name() {
        let chunker = FixedChunker::with_size(100);
        assert_eq!(chunker.name(), "fixed");
    }

    #[test]
    fn test_fixed_chunker_config() {
        let config = super::super::ChunkingConfig::new(50, 100, 200);
        let chunker = FixedChunker::new(config);

        assert_eq!(chunker.config().min_size, 50);
        assert_eq!(chunker.config().target_size, 100);
        assert_eq!(chunker.config().max_size, 200);
    }

    #[test]
    fn test_fixed_chunker_sum_equals_input_size() {
        let chunker = FixedChunker::with_size(77);
        let data = vec![42u8; 1000];

        let chunks = chunker.chunk(&data);
        let total: usize = chunks.iter().map(|c| c.size()).sum();

        assert_eq!(total, data.len());
    }

    #[test]
    fn test_fixed_chunker_non_final_chunks_equal_target() {
        let target_size = 100;
        let chunker = FixedChunker::with_size(target_size);
        let data = vec![42u8; 350];

        let chunks = chunker.chunk(&data);

        // All but last should be exactly target_size
        for chunk in chunks.iter().take(chunks.len() - 1) {
            assert_eq!(chunk.size(), target_size);
        }
        // Last can be smaller
        assert!(chunks.last().unwrap().size() <= target_size);
    }

    #[test]
    fn test_fixed_chunker_final_chunk_size_le_target() {
        let target_size = 100;
        let chunker = FixedChunker::with_size(target_size);
        let data = vec![42u8; 250];

        let chunks = chunker.chunk(&data);

        assert!(chunks.last().unwrap().size() <= target_size);
    }

    #[test]
    fn test_fixed_chunker_cids_match_content() {
        let chunker = FixedChunker::with_size(10);
        let data = b"hello worldhello world";

        let chunks = chunker.chunk(data);

        // First two chunks have same content "hello worl"
        // so they should have same CID
        let _cid1 = chunks[0].to_chunk_ref().cid;
        let _cid2 = chunks[1].to_chunk_ref().cid;

        // Actually they won't match because content is different
        // "hello worl" vs "dhello wor"
        // But same content DOES produce same CID:
        let chunk_a = ChunkData::new(b"same".to_vec(), 0);
        let chunk_b = ChunkData::new(b"same".to_vec(), 100);
        assert_eq!(chunk_a.to_chunk_ref().cid, chunk_b.to_chunk_ref().cid);
    }
}
