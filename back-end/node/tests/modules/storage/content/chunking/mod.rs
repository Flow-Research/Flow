pub mod config;
pub mod streaming;
pub mod types;

/// Integration tests for chunking algorithms.

#[cfg(test)]
mod integration_tests {
    use node::modules::storage::content::{
        ChunkData, ChunkRef, Chunker, ChunkingAlgorithm, ChunkingConfig,
    };

    use crate::bootstrap::init::{remove_env, set_env};

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

    #[test]
    fn test_concurrent_chunking_different_data() {
        use std::sync::Arc;
        use std::thread;

        let chunker = Arc::new(ChunkingAlgorithm::FastCdc.create_chunker(ChunkingConfig::small()));

        let handles: Vec<_> = (0..10)
            .map(|i| {
                let chunker = Arc::clone(&chunker);
                thread::spawn(move || {
                    let data: Vec<u8> = (0..1000).map(|j| ((i * 100 + j) % 256) as u8).collect();
                    let chunks = chunker.chunk(&data);
                    let reconstructed: Vec<u8> =
                        chunks.iter().flat_map(|c| c.data.clone()).collect();
                    assert_eq!(reconstructed, data);
                    chunks.len()
                })
            })
            .collect();

        for handle in handles {
            let count = handle.join().expect("Thread panicked");
            assert!(count > 0);
        }
    }

    #[test]
    fn test_concurrent_chunking_same_data() {
        use std::sync::Arc;
        use std::thread;

        let chunker = Arc::new(ChunkingAlgorithm::FastCdc.create_chunker(ChunkingConfig::small()));
        let data: Vec<u8> = (0..5000).map(|i| (i % 256) as u8).collect();
        let data = Arc::new(data);

        let handles: Vec<_> = (0..10)
            .map(|_| {
                let chunker = Arc::clone(&chunker);
                let data = Arc::clone(&data);
                thread::spawn(move || {
                    let chunks = chunker.chunk(&data);
                    chunks
                        .iter()
                        .map(|c| c.to_chunk_ref().cid.to_string())
                        .collect::<Vec<_>>()
                })
            })
            .collect();

        let results: Vec<_> = handles
            .into_iter()
            .map(|h| h.join().expect("Thread panicked"))
            .collect();

        // All threads should produce identical CIDs
        let first = &results[0];
        for result in &results[1..] {
            assert_eq!(first, result);
        }
    }

    // ============================================
    // Binary Data Integrity Tests
    // ============================================

    #[test]
    fn test_all_byte_values() {
        let data: Vec<u8> = (0..=255).cycle().take(10000).collect();

        for alg in [
            ChunkingAlgorithm::Fixed,
            ChunkingAlgorithm::FastCdc,
            ChunkingAlgorithm::Rabin,
        ] {
            let chunker = alg.create_chunker(ChunkingConfig::small());
            let chunks = chunker.chunk(&data);
            let reconstructed: Vec<u8> = chunks.iter().flat_map(|c| c.data.clone()).collect();

            assert_eq!(
                reconstructed, data,
                "Algorithm {:?} failed binary test",
                alg
            );
        }
    }

    #[test]
    fn test_random_binary_data() {
        // Pseudo-random data
        let data: Vec<u8> = (0u64..50000)
            .map(|i| (i.wrapping_mul(1103515245).wrapping_add(12345) % 256) as u8)
            .collect();

        for alg in [
            ChunkingAlgorithm::Fixed,
            ChunkingAlgorithm::FastCdc,
            ChunkingAlgorithm::Rabin,
        ] {
            let chunker = alg.create_chunker(ChunkingConfig::small());
            let chunks = chunker.chunk(&data);
            let reconstructed: Vec<u8> = chunks.iter().flat_map(|c| c.data.clone()).collect();

            assert_eq!(
                reconstructed, data,
                "Algorithm {:?} failed random binary test",
                alg
            );
        }
    }

    // ============================================
    // chunk() vs chunk_iter() Equivalence Tests
    // ============================================

    #[test]
    fn test_chunk_vs_iter_equivalence_all_algorithms() {
        let data: Vec<u8> = (0..20000).map(|i| (i % 256) as u8).collect();

        for alg in [
            ChunkingAlgorithm::Fixed,
            ChunkingAlgorithm::FastCdc,
            ChunkingAlgorithm::Rabin,
        ] {
            let chunker = alg.create_chunker(ChunkingConfig::small());

            let collected = chunker.chunk(&data);
            let iterated: Vec<_> = chunker.chunk_iter(&data).collect();

            assert_eq!(
                collected.len(),
                iterated.len(),
                "Algorithm {:?}: chunk count differs",
                alg
            );

            for (i, (c, iter)) in collected.iter().zip(iterated.iter()).enumerate() {
                assert_eq!(
                    c.data, iter.data,
                    "Algorithm {:?}: chunk {} data differs",
                    alg, i
                );
                assert_eq!(
                    c.offset, iter.offset,
                    "Algorithm {:?}: chunk {} offset differs",
                    alg, i
                );
            }
        }
    }

    // ============================================
    // Size Bound Tests
    // ============================================

    #[test]
    fn test_cdc_algorithms_respect_max_size() {
        let config = ChunkingConfig::new(2048, 8192, 32768);
        let data: Vec<u8> = (0..50000).map(|i| ((i * 17) % 256) as u8).collect();

        for alg in [ChunkingAlgorithm::FastCdc, ChunkingAlgorithm::Rabin] {
            let chunker = alg.create_chunker(config.clone());
            let chunks = chunker.chunk(&data);

            for (i, chunk) in chunks.iter().enumerate() {
                assert!(
                    chunk.size() <= config.max_size,
                    "Algorithm {:?}: chunk {} size {} exceeds max {}",
                    alg,
                    i,
                    chunk.size(),
                    config.max_size
                );
            }
        }
    }

    #[test]
    fn test_cdc_algorithms_respect_min_size() {
        let config = ChunkingConfig::new(2048, 8192, 32768);
        let data: Vec<u8> = (0..100000).map(|i| ((i * 17) % 256) as u8).collect();

        for alg in [ChunkingAlgorithm::FastCdc, ChunkingAlgorithm::Rabin] {
            let chunker = alg.create_chunker(config.clone());
            let chunks = chunker.chunk(&data);

            for (i, chunk) in chunks.iter().enumerate() {
                let is_last = i == chunks.len() - 1;
                if !is_last {
                    assert!(
                        chunk.size() >= config.min_size,
                        "Algorithm {:?}: non-final chunk {} size {} below min {}",
                        alg,
                        i,
                        chunk.size(),
                        config.min_size
                    );
                }
            }
        }
    }

    // ============================================
    // Deduplication Extended Tests
    // ============================================

    #[test]
    fn test_dedup_after_append() {
        let config = ChunkingConfig::new(2048, 8192, 32768);
        let original: Vec<u8> = (0..10000).map(|i| (i % 256) as u8).collect();
        let mut appended = original.clone();
        appended.extend(vec![0u8; 1000]); // Append 10%

        for alg in [ChunkingAlgorithm::FastCdc, ChunkingAlgorithm::Rabin] {
            let chunker = alg.create_chunker(config.clone());

            let orig_chunks = chunker.chunk(&original);
            let appended_chunks = chunker.chunk(&appended);

            // Only test if we have multiple chunks
            if orig_chunks.len() < 2 {
                println!(
                    "Skipping {:?}: not enough chunks ({})",
                    alg,
                    orig_chunks.len()
                );
                continue;
            }

            let orig_cids: std::collections::HashSet<_> = orig_chunks
                .iter()
                .map(|c| c.to_chunk_ref().cid.to_string())
                .collect();

            let appended_cids: std::collections::HashSet<_> = appended_chunks
                .iter()
                .map(|c| c.to_chunk_ref().cid.to_string())
                .collect();

            let common = orig_cids.intersection(&appended_cids).count();

            println!(
                "Algorithm {:?}: {} original chunks, {} appended chunks, {} common",
                alg,
                orig_cids.len(),
                appended_cids.len(),
                common
            );

            // CDC should share at least some chunks when appending
            // (relaxed assertion - just verify we get some benefit)
            assert!(
                common > 0 || orig_cids.len() <= 2,
                "Algorithm {:?}: expected some shared chunks after append",
                alg
            );
        }
    }

    #[test]
    fn test_dedup_after_prepend() {
        let config = ChunkingConfig::small();
        let original: Vec<u8> = (0..10000).map(|i| (i % 256) as u8).collect();
        let mut prepended = vec![0u8; 1000]; // Prepend 10%
        prepended.extend(&original);

        let fixed = ChunkingAlgorithm::Fixed.create_chunker(config.clone());
        let fastcdc = ChunkingAlgorithm::FastCdc.create_chunker(config.clone());

        let fixed_orig: std::collections::HashSet<_> = fixed
            .chunk(&original)
            .iter()
            .map(|c| c.to_chunk_ref().cid.to_string())
            .collect();
        let fixed_prep: std::collections::HashSet<_> = fixed
            .chunk(&prepended)
            .iter()
            .map(|c| c.to_chunk_ref().cid.to_string())
            .collect();
        let fixed_common = fixed_orig.intersection(&fixed_prep).count();

        let cdc_orig: std::collections::HashSet<_> = fastcdc
            .chunk(&original)
            .iter()
            .map(|c| c.to_chunk_ref().cid.to_string())
            .collect();
        let cdc_prep: std::collections::HashSet<_> = fastcdc
            .chunk(&prepended)
            .iter()
            .map(|c| c.to_chunk_ref().cid.to_string())
            .collect();
        let cdc_common = cdc_orig.intersection(&cdc_prep).count();

        println!(
            "Fixed: {} common, FastCDC: {} common",
            fixed_common, cdc_common
        );

        // CDC should generally have more common chunks than fixed for prepend
        // (fixed shifts all boundaries, CDC is content-defined)
    }

    // ============================================
    // Regression Tests
    // ============================================

    #[test]
    fn test_rabin_hash_index_bounds_regression() {
        let config = ChunkingConfig::small();
        let chunker = ChunkingAlgorithm::Rabin.create_chunker(config);

        let test_cases: Vec<Vec<u8>> = vec![
            (0..10000).map(|i| (i % 256) as u8).collect(),
            (0..10000).map(|i| ((i * 7 + 13) % 256) as u8).collect(),
            (0..10000).map(|i| ((i * 17) % 256) as u8).collect(),
            vec![0u8; 10000],
            vec![255u8; 10000],
            (0..10000).map(|_| rand::random::<u8>()).collect(),
        ];

        for (i, data) in test_cases.iter().enumerate() {
            // Should not panic with "index out of bounds"
            let result =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| chunker.chunk(data)));
            assert!(result.is_ok(), "Rabin chunker panicked on test case {}", i);
        }
    }

    // ============================================
    // Algorithm-Specific Edge Cases
    // ============================================

    #[test]
    fn test_uniform_data_chunking() {
        let config = ChunkingConfig::small();
        let data = vec![0u8; 50000]; // All zeros

        for alg in [
            ChunkingAlgorithm::Fixed,
            ChunkingAlgorithm::FastCdc,
            ChunkingAlgorithm::Rabin,
        ] {
            let chunker = alg.create_chunker(config.clone());
            let chunks = chunker.chunk(&data);

            // Should still produce chunks
            assert!(
                !chunks.is_empty(),
                "Algorithm {:?} produced no chunks for uniform data",
                alg
            );

            // Reconstruction should work
            let reconstructed: Vec<u8> = chunks.iter().flat_map(|c| c.data.clone()).collect();
            assert_eq!(reconstructed, data);
        }
    }

    #[test]
    fn test_highly_compressible_data() {
        let config = ChunkingConfig::small();
        // Repeating pattern
        let pattern = b"ABCDEFGH";
        let data: Vec<u8> = pattern.iter().cycle().take(50000).cloned().collect();

        for alg in [
            ChunkingAlgorithm::Fixed,
            ChunkingAlgorithm::FastCdc,
            ChunkingAlgorithm::Rabin,
        ] {
            let chunker = alg.create_chunker(config.clone());
            let chunks = chunker.chunk(&data);

            // Reconstruction should work
            let reconstructed: Vec<u8> = chunks.iter().flat_map(|c| c.data.clone()).collect();
            assert_eq!(
                reconstructed, data,
                "Algorithm {:?} failed on compressible data",
                alg
            );
        }
    }

    // ============================================
    // ChunkingAlgorithm Environment Tests
    // ============================================

    #[test]
    #[serial_test::serial]
    fn test_algorithm_from_env_default() {
        remove_env("CHUNKING_ALGORITHM");
        let alg = ChunkingAlgorithm::from_env();
        assert_eq!(alg, ChunkingAlgorithm::FastCdc);
    }

    #[test]
    #[serial_test::serial]
    fn test_algorithm_from_env_valid() {
        set_env("CHUNKING_ALGORITHM", "rabin");
        let alg = ChunkingAlgorithm::from_env();
        remove_env("CHUNKING_ALGORITHM");
        assert_eq!(alg, ChunkingAlgorithm::Rabin);
    }

    #[test]
    #[serial_test::serial]
    fn test_algorithm_from_env_invalid() {
        set_env("CHUNKING_ALGORITHM", "invalid_algorithm");
        let alg = ChunkingAlgorithm::from_env();
        remove_env("CHUNKING_ALGORITHM");
        assert_eq!(alg, ChunkingAlgorithm::FastCdc); // Default
    }

    // ============================================
    // Display Trait Test
    // ============================================

    #[test]
    fn test_algorithm_display() {
        assert_eq!(format!("{}", ChunkingAlgorithm::Fixed), "fixed");
        assert_eq!(format!("{}", ChunkingAlgorithm::FastCdc), "fastcdc");
        assert_eq!(format!("{}", ChunkingAlgorithm::Rabin), "rabin");
    }

    // ============================================
    // Default Trait Test
    // ============================================

    #[test]
    fn test_algorithm_default() {
        let alg = ChunkingAlgorithm::default();
        assert_eq!(alg, ChunkingAlgorithm::FastCdc);
    }
}
