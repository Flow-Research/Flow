//! Integration tests for chunking algorithms.

#[cfg(test)]
mod integration_tests {
    use node::modules::storage::content::{
        ChunkData, ChunkRef, Chunker, ChunkingAlgorithm, ChunkingConfig,
    };

    /// Test that all algorithms produce valid reconstructable output.
    #[test]
    fn test_all_algorithms_reconstruct() {
        let configs = vec![
            ChunkingConfig::small(),
            ChunkingConfig::default(),
            ChunkingConfig::large(),
        ];

        let algorithms = vec![
            ChunkingAlgorithm::Fixed,
            ChunkingAlgorithm::FastCdc,
            ChunkingAlgorithm::Rabin,
        ];

        // Various test data patterns
        let test_data: Vec<Vec<u8>> = vec![
            vec![],                                               // Empty
            vec![42],                                             // Single byte
            b"Hello, World!".to_vec(),                            // Small text
            (0..1000).map(|i| (i % 256) as u8).collect(),         // Repeating pattern
            (0..100000).map(|i| ((i * 7) % 256) as u8).collect(), // Large pseudo-random
        ];

        for config in &configs {
            for algorithm in &algorithms {
                let chunker = algorithm.create_chunker(config.clone());

                for (i, data) in test_data.iter().enumerate() {
                    let chunks = chunker.chunk(data);
                    let reconstructed: Vec<u8> =
                        chunks.iter().flat_map(|c| c.data.clone()).collect();

                    assert_eq!(
                        reconstructed, *data,
                        "Reconstruction failed for algorithm {:?}, config {:?}, data index {}",
                        algorithm, config, i
                    );
                }
            }
        }
    }

    /// Test that all algorithms are deterministic.
    #[test]
    fn test_all_algorithms_deterministic() {
        let config = ChunkingConfig::default();
        let data: Vec<u8> = (0..50000).map(|i| ((i * 13) % 256) as u8).collect();

        for algorithm in [
            ChunkingAlgorithm::Fixed,
            ChunkingAlgorithm::FastCdc,
            ChunkingAlgorithm::Rabin,
        ] {
            let chunker = algorithm.create_chunker(config.clone());

            let chunks1 = chunker.chunk(&data);
            let chunks2 = chunker.chunk(&data);

            assert_eq!(
                chunks1.len(),
                chunks2.len(),
                "{:?} produced different chunk counts",
                algorithm
            );

            for (c1, c2) in chunks1.iter().zip(chunks2.iter()) {
                assert_eq!(
                    c1.data, c2.data,
                    "{:?} produced different chunk data",
                    algorithm
                );
                assert_eq!(
                    c1.offset, c2.offset,
                    "{:?} produced different offsets",
                    algorithm
                );
            }
        }
    }

    /// Test that offsets are continuous (no gaps or overlaps).
    #[test]
    fn test_all_algorithms_continuous_offsets() {
        let config = ChunkingConfig::small();
        let data: Vec<u8> = (0..10000).map(|i| (i % 256) as u8).collect();

        for algorithm in [
            ChunkingAlgorithm::Fixed,
            ChunkingAlgorithm::FastCdc,
            ChunkingAlgorithm::Rabin,
        ] {
            let chunker = algorithm.create_chunker(config.clone());
            let chunks = chunker.chunk(&data);

            let mut expected_offset = 0u64;
            for (i, chunk) in chunks.iter().enumerate() {
                assert_eq!(
                    chunk.offset, expected_offset,
                    "{:?}: Chunk {} offset mismatch (expected {}, got {})",
                    algorithm, i, expected_offset, chunk.offset
                );
                expected_offset += chunk.size() as u64;
            }

            assert_eq!(
                expected_offset,
                data.len() as u64,
                "{:?}: Total size mismatch",
                algorithm
            );
        }
    }

    /// Test ChunkRef creation from ChunkData.
    #[test]
    fn test_chunk_ref_from_chunk_data() {
        let chunk_data = ChunkData::new(b"hello world".to_vec(), 100);
        let chunk_ref = chunk_data.to_chunk_ref();

        assert_eq!(chunk_ref.offset, 100);
        assert_eq!(chunk_ref.size, 11);
        assert!(!chunk_ref.cid.to_string().is_empty());
    }

    /// Test ChunkRef byte range and contains methods.
    #[test]
    fn test_chunk_ref_byte_operations() {
        let cid = node::modules::storage::content::ContentId::from_bytes(b"test");
        let chunk_ref = ChunkRef::new(cid, 100, 50);

        assert_eq!(chunk_ref.byte_range(), 100..150);
        assert!(chunk_ref.contains_offset(100));
        assert!(chunk_ref.contains_offset(149));
        assert!(!chunk_ref.contains_offset(99));
        assert!(!chunk_ref.contains_offset(150));
    }

    /// Test factory creates correct chunker types.
    #[test]
    fn test_algorithm_factory() {
        let config = ChunkingConfig::default();

        let fixed = ChunkingAlgorithm::Fixed.create_chunker(config.clone());
        assert_eq!(fixed.name(), "fixed");

        let fastcdc = ChunkingAlgorithm::FastCdc.create_chunker(config.clone());
        assert_eq!(fastcdc.name(), "fastcdc");

        let rabin = ChunkingAlgorithm::Rabin.create_chunker(config);
        assert_eq!(rabin.name(), "rabin");
    }

    /// Test algorithm parsing from string.
    #[test]
    fn test_algorithm_from_str() {
        assert_eq!(
            ChunkingAlgorithm::from_str("fixed"),
            Some(ChunkingAlgorithm::Fixed)
        );
        assert_eq!(
            ChunkingAlgorithm::from_str("FIXED"),
            Some(ChunkingAlgorithm::Fixed)
        );
        assert_eq!(
            ChunkingAlgorithm::from_str("fastcdc"),
            Some(ChunkingAlgorithm::FastCdc)
        );
        assert_eq!(
            ChunkingAlgorithm::from_str("fast-cdc"),
            Some(ChunkingAlgorithm::FastCdc)
        );
        assert_eq!(
            ChunkingAlgorithm::from_str("rabin"),
            Some(ChunkingAlgorithm::Rabin)
        );
        assert_eq!(ChunkingAlgorithm::from_str("unknown"), None);
    }

    /// Test config validation.
    #[test]
    fn test_config_validation() {
        let valid = ChunkingConfig::new(100, 500, 1000);
        assert!(valid.validate().is_ok());

        let invalid = ChunkingConfig {
            min_size: 1000,
            target_size: 500,
            max_size: 2000,
        };
        assert!(invalid.validate().is_err());
    }

    /// Compare deduplication effectiveness between fixed and CDC.
    #[test]
    fn test_deduplication_comparison() {
        let config = ChunkingConfig::small();

        // Original file
        let original: Vec<u8> = (0..10000).map(|i| (i % 256) as u8).collect();

        // Modified file with 1 byte inserted in the middle
        let mut modified = original.clone();
        modified.insert(5000, 0xFF);

        // Chunk with each algorithm
        let fixed = ChunkingAlgorithm::Fixed.create_chunker(config.clone());
        let fastcdc = ChunkingAlgorithm::FastCdc.create_chunker(config.clone());
        let rabin = ChunkingAlgorithm::Rabin.create_chunker(config);

        fn count_common_chunks(chunker: &dyn Chunker, original: &[u8], modified: &[u8]) -> usize {
            let original_cids: std::collections::HashSet<_> = chunker
                .chunk(original)
                .iter()
                .map(|c| c.to_chunk_ref().cid.to_string())
                .collect();

            let modified_cids: std::collections::HashSet<_> = chunker
                .chunk(modified)
                .iter()
                .map(|c| c.to_chunk_ref().cid.to_string())
                .collect();

            original_cids.intersection(&modified_cids).count()
        }

        let fixed_common = count_common_chunks(fixed.as_ref(), &original, &modified);
        let fastcdc_common = count_common_chunks(fastcdc.as_ref(), &original, &modified);
        let rabin_common = count_common_chunks(rabin.as_ref(), &original, &modified);

        println!("Deduplication effectiveness (higher is better):");
        println!("  Fixed:   {} common chunks", fixed_common);
        println!("  FastCDC: {} common chunks", fastcdc_common);
        println!("  Rabin:   {} common chunks", rabin_common);

        // CDC algorithms should generally have more common chunks
        // (not guaranteed for all inputs, but typical)
    }
}
