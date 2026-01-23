use node::modules::storage::content::chunking::{
    ChunkData, Chunker, ChunkingConfig, FastCdcChunker, FixedChunker, RabinChunker,
};
use std::fs::File;
use std::io::{Cursor, Write};
use tempfile::NamedTempFile;

fn default_config() -> ChunkingConfig {
    ChunkingConfig::new(1024, 4096, 16384)
}

// ============================================================================
// Fixed Chunker Streaming Tests
// ============================================================================

#[test]
fn test_fixed_stream_vs_inmemory() {
    let data: Vec<u8> = (0..50_000).map(|i| (i % 256) as u8).collect();
    let config = ChunkingConfig::new(1000, 1000, 1000);
    let chunker = FixedChunker::new(config.clone());

    let inmemory = chunker.chunk(&data);
    let streaming: Vec<_> = chunker
        .stream_chunks(Cursor::new(data))
        .map(|r| r.unwrap())
        .collect();

    assert_eq!(inmemory.len(), streaming.len());
    for (im, st) in inmemory.iter().zip(streaming.iter()) {
        assert_eq!(im.offset, st.offset);
        assert_eq!(im.data, st.data);
    }
}

#[test]
fn test_fixed_stream_empty() {
    let chunker = FixedChunker::new(default_config());
    let chunks: Vec<_> = chunker
        .stream_chunks(Cursor::new(Vec::<u8>::new()))
        .collect();
    assert!(chunks.is_empty());
}

#[test]
fn test_fixed_stream_single_byte() {
    let chunker = FixedChunker::new(ChunkingConfig::new(10, 10, 10));
    let chunks: Vec<_> = chunker
        .stream_chunks(Cursor::new(vec![42u8]))
        .map(|r| r.unwrap())
        .collect();

    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].data, vec![42u8]);
}

// ============================================================================
// FastCDC Streaming Tests
// ============================================================================

#[test]
fn test_fastcdc_stream_vs_inmemory() {
    let data: Vec<u8> = (0..50_000).map(|i| ((i * 17) % 256) as u8).collect();
    let config = default_config();
    let chunker = FastCdcChunker::new(config);

    let inmemory = chunker.chunk(&data);
    let streaming: Vec<_> = chunker
        .stream_chunks(Cursor::new(data))
        .map(|r| r.unwrap())
        .collect();

    assert_eq!(inmemory.len(), streaming.len());
    for (im, st) in inmemory.iter().zip(streaming.iter()) {
        assert_eq!(im.offset, st.offset);
        assert_eq!(im.data.len(), st.data.len());
    }
}

#[test]
fn test_fastcdc_stream_determinism() {
    let data: Vec<u8> = (0..20_000).map(|i| ((i * 31) % 256) as u8).collect();
    let config = default_config();

    let chunker1 = FastCdcChunker::new(config.clone());
    let chunker2 = FastCdcChunker::new(config);

    let chunks1: Vec<_> = chunker1
        .stream_chunks(Cursor::new(data.clone()))
        .map(|r| r.unwrap())
        .collect();

    let chunks2: Vec<_> = chunker2
        .stream_chunks(Cursor::new(data))
        .map(|r| r.unwrap())
        .collect();

    assert_eq!(chunks1.len(), chunks2.len());
    for (c1, c2) in chunks1.iter().zip(chunks2.iter()) {
        assert_eq!(c1.offset, c2.offset);
        assert_eq!(c1.data, c2.data);
    }
}

// ============================================================================
// Rabin Streaming Tests
// ============================================================================

#[test]
fn test_rabin_stream_vs_inmemory() {
    let data: Vec<u8> = (0..50_000).map(|i| ((i * 17) % 256) as u8).collect();
    let config = default_config();
    let chunker = RabinChunker::new(config);

    let inmemory = chunker.chunk(&data);
    let streaming: Vec<_> = chunker
        .stream_chunks(Cursor::new(data))
        .map(|r| r.unwrap())
        .collect();

    assert_eq!(inmemory.len(), streaming.len());
    for (im, st) in inmemory.iter().zip(streaming.iter()) {
        assert_eq!(im.offset, st.offset);
        assert_eq!(im.data.len(), st.data.len());
    }
}

#[test]
fn test_rabin_stream_reconstruction() {
    let data: Vec<u8> = (0..30_000).map(|i| ((i * 7) % 256) as u8).collect();
    let chunker = RabinChunker::new(default_config());

    let chunks: Vec<_> = chunker
        .stream_chunks(Cursor::new(data.clone()))
        .map(|r| r.unwrap())
        .collect();

    let reconstructed: Vec<u8> = chunks.into_iter().flat_map(|c| c.data).collect();
    assert_eq!(reconstructed, data);
}

// ============================================================================
// Cross-Algorithm Tests
// ============================================================================

#[test]
fn test_all_algorithms_stream_reconstruction() {
    let data: Vec<u8> = (0..100_000).map(|i| ((i * 13) % 256) as u8).collect();
    let config = default_config();

    // Fixed
    {
        let chunker = FixedChunker::new(config.clone());
        let chunks: Vec<_> = chunker
            .stream_chunks(Cursor::new(data.clone()))
            .map(|r| r.expect("fixed chunk failed"))
            .collect();
        let reconstructed: Vec<u8> = chunks.into_iter().flat_map(|c| c.data).collect();
        assert_eq!(reconstructed, data, "fixed reconstruction failed");
    }

    // FastCDC
    {
        let chunker = FastCdcChunker::new(config.clone());
        let chunks: Vec<_> = chunker
            .stream_chunks(Cursor::new(data.clone()))
            .map(|r| r.expect("fastcdc chunk failed"))
            .collect();
        let reconstructed: Vec<u8> = chunks.into_iter().flat_map(|c| c.data).collect();
        assert_eq!(reconstructed, data, "fastcdc reconstruction failed");
    }

    // Rabin
    {
        let chunker = RabinChunker::new(config.clone());
        let chunks: Vec<_> = chunker
            .stream_chunks(Cursor::new(data.clone()))
            .map(|r| r.expect("rabin chunk failed"))
            .collect();
        let reconstructed: Vec<u8> = chunks.into_iter().flat_map(|c| c.data).collect();
        assert_eq!(reconstructed, data, "rabin reconstruction failed");
    }
}

#[test]
fn test_all_algorithms_continuous_offsets_streaming() {
    let data: Vec<u8> = (0..50_000).map(|i| (i % 256) as u8).collect();
    let config = default_config();

    // Test Fixed
    {
        let chunker = FixedChunker::new(config.clone());
        let chunks: Vec<_> = chunker
            .stream_chunks(Cursor::new(data.clone()))
            .map(|r| r.unwrap())
            .collect();
        verify_continuous_offsets(&chunks, &data, "fixed");
    }

    // Test FastCDC
    {
        let chunker = FastCdcChunker::new(config.clone());
        let chunks: Vec<_> = chunker
            .stream_chunks(Cursor::new(data.clone()))
            .map(|r| r.unwrap())
            .collect();
        verify_continuous_offsets(&chunks, &data, "fastcdc");
    }

    // Test Rabin
    {
        let chunker = RabinChunker::new(config.clone());
        let chunks: Vec<_> = chunker
            .stream_chunks(Cursor::new(data.clone()))
            .map(|r| r.unwrap())
            .collect();
        verify_continuous_offsets(&chunks, &data, "rabin");
    }
}

fn verify_continuous_offsets(chunks: &[ChunkData], data: &[u8], name: &str) {
    let mut expected_offset = 0u64;
    for (i, chunk) in chunks.iter().enumerate() {
        assert_eq!(
            chunk.offset, expected_offset,
            "{} offset mismatch at chunk {}",
            name, i
        );
        expected_offset += chunk.size() as u64;
    }
    assert_eq!(expected_offset, data.len() as u64);
}

#[test]
fn test_stream_from_file() {
    let mut file = NamedTempFile::new().unwrap();
    let data: Vec<u8> = (0..50_000).map(|i| ((i * 17) % 256) as u8).collect();
    file.write_all(&data).unwrap();
    file.flush().unwrap();

    let config = default_config();

    // Fixed
    {
        let chunker = FixedChunker::new(config.clone());
        let reader = File::open(file.path()).unwrap();
        let chunks: Vec<_> = chunker
            .stream_chunks(reader)
            .map(|r| r.expect("fixed read failed"))
            .collect();
        assert!(!chunks.is_empty(), "fixed produced no chunks");
        let reconstructed: Vec<u8> = chunks.into_iter().flat_map(|c| c.data).collect();
        assert_eq!(reconstructed, data, "fixed reconstruction failed");
    }

    // FastCDC
    {
        let chunker = FastCdcChunker::new(config.clone());
        let reader = File::open(file.path()).unwrap();
        let chunks: Vec<_> = chunker
            .stream_chunks(reader)
            .map(|r| r.expect("fastcdc read failed"))
            .collect();
        assert!(!chunks.is_empty(), "fastcdc produced no chunks");
        let reconstructed: Vec<u8> = chunks.into_iter().flat_map(|c| c.data).collect();
        assert_eq!(reconstructed, data, "fastcdc reconstruction failed");
    }

    // Rabin
    {
        let chunker = RabinChunker::new(config.clone());
        let reader = File::open(file.path()).unwrap();
        let chunks: Vec<_> = chunker
            .stream_chunks(reader)
            .map(|r| r.expect("rabin read failed"))
            .collect();
        assert!(!chunks.is_empty(), "rabin produced no chunks");
        let reconstructed: Vec<u8> = chunks.into_iter().flat_map(|c| c.data).collect();
        assert_eq!(reconstructed, data, "rabin reconstruction failed");
    }
}

#[test]
fn test_cdc_algorithms_respect_bounds_streaming() {
    let config = ChunkingConfig::new(512, 2048, 8192);
    let data: Vec<u8> = (0..100_000).map(|i| ((i * 23) % 256) as u8).collect();

    fn verify_bounds(chunks: &[ChunkData], config: &ChunkingConfig, name: &str) {
        for (i, chunk) in chunks.iter().enumerate() {
            if i < chunks.len() - 1 {
                assert!(
                    chunk.size() >= config.min_size,
                    "{} chunk {} size {} below min {}",
                    name,
                    i,
                    chunk.size(),
                    config.min_size
                );
            }
            assert!(
                chunk.size() <= config.max_size,
                "{} chunk {} size {} above max {}",
                name,
                i,
                chunk.size(),
                config.max_size
            );
        }
    }

    // FastCDC
    {
        let chunker = FastCdcChunker::new(config.clone());
        let chunks: Vec<_> = chunker
            .stream_chunks(Cursor::new(data.clone()))
            .map(|r| r.unwrap())
            .collect();
        verify_bounds(&chunks, &config, "fastcdc");
    }

    // Rabin
    {
        let chunker = RabinChunker::new(config.clone());
        let chunks: Vec<_> = chunker
            .stream_chunks(Cursor::new(data.clone()))
            .map(|r| r.unwrap())
            .collect();
        verify_bounds(&chunks, &config, "rabin");
    }
}
