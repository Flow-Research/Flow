#[cfg(test)]
mod tests {

    // ============================================
    // ChunkRef Tests
    // ============================================

    use node::modules::storage::content::{
        ChunkData, ChunkRef, Chunker, ChunkingAlgorithm, ChunkingConfig, ContentId, FastCdcChunker,
        FixedChunker, RabinChunker,
    };

    #[test]
    fn test_chunk_ref_new() {
        let cid = ContentId::from_bytes(b"test data");
        let chunk_ref = ChunkRef::new(cid.clone(), 100, 50);

        assert_eq!(chunk_ref.cid, cid);
        assert_eq!(chunk_ref.offset, 100);
        assert_eq!(chunk_ref.size, 50);
    }

    #[test]
    fn test_chunk_ref_byte_range() {
        let cid = ContentId::from_bytes(b"test");
        let chunk_ref = ChunkRef::new(cid, 100, 50);

        assert_eq!(chunk_ref.byte_range(), 100..150);
    }

    #[test]
    fn test_chunk_ref_byte_range_at_zero() {
        let cid = ContentId::from_bytes(b"test");
        let chunk_ref = ChunkRef::new(cid, 0, 100);

        assert_eq!(chunk_ref.byte_range(), 0..100);
    }

    #[test]
    fn test_chunk_ref_contains_offset_start() {
        let cid = ContentId::from_bytes(b"test");
        let chunk_ref = ChunkRef::new(cid, 100, 50);

        assert!(chunk_ref.contains_offset(100)); // Start is inclusive
    }

    #[test]
    fn test_chunk_ref_contains_offset_middle() {
        let cid = ContentId::from_bytes(b"test");
        let chunk_ref = ChunkRef::new(cid, 100, 50);

        assert!(chunk_ref.contains_offset(125));
    }

    #[test]
    fn test_chunk_ref_contains_offset_end_minus_one() {
        let cid = ContentId::from_bytes(b"test");
        let chunk_ref = ChunkRef::new(cid, 100, 50);

        assert!(chunk_ref.contains_offset(149)); // Last byte
    }

    #[test]
    fn test_chunk_ref_not_contains_before() {
        let cid = ContentId::from_bytes(b"test");
        let chunk_ref = ChunkRef::new(cid, 100, 50);

        assert!(!chunk_ref.contains_offset(99));
    }

    #[test]
    fn test_chunk_ref_not_contains_at_end() {
        let cid = ContentId::from_bytes(b"test");
        let chunk_ref = ChunkRef::new(cid, 100, 50);

        assert!(!chunk_ref.contains_offset(150)); // End is exclusive
    }

    #[test]
    fn test_chunk_ref_size_zero() {
        let cid = ContentId::from_bytes(b"test");
        let chunk_ref = ChunkRef::new(cid, 100, 0);

        assert_eq!(chunk_ref.byte_range(), 100..100);
        assert!(!chunk_ref.contains_offset(100)); // Empty range
    }

    #[test]
    fn test_chunk_ref_large_offset() {
        let cid = ContentId::from_bytes(b"test");
        let offset = u64::MAX - 100;
        let chunk_ref = ChunkRef::new(cid, offset, 50);

        assert_eq!(chunk_ref.offset, offset);
        assert!(chunk_ref.contains_offset(offset));
        assert!(chunk_ref.contains_offset(offset + 49));
    }

    #[test]
    fn test_chunk_ref_clone() {
        let cid = ContentId::from_bytes(b"test");
        let chunk_ref1 = ChunkRef::new(cid, 100, 50);
        let chunk_ref2 = chunk_ref1.clone();

        assert_eq!(chunk_ref1, chunk_ref2);
    }

    #[test]
    fn test_chunk_ref_debug() {
        let cid = ContentId::from_bytes(b"test");
        let chunk_ref = ChunkRef::new(cid, 100, 50);
        let debug = format!("{:?}", chunk_ref);

        assert!(debug.contains("100"));
        assert!(debug.contains("50"));
    }

    #[test]
    fn test_chunk_ref_serialization_json() {
        let cid = ContentId::from_bytes(b"test data for json");
        let chunk_ref = ChunkRef::new(cid, 100, 50);

        let json = serde_json::to_string(&chunk_ref).unwrap();
        let deserialized: ChunkRef = serde_json::from_str(&json).unwrap();

        assert_eq!(chunk_ref, deserialized);
    }

    #[test]
    fn test_chunk_ref_serialization_cbor() {
        let cid = ContentId::from_bytes(b"test data for cbor");
        let chunk_ref = ChunkRef::new(cid, 100, 50);

        // CBOR roundtrip
        let bytes = serde_ipld_dagcbor::to_vec(&chunk_ref).unwrap();
        let deserialized: ChunkRef = serde_ipld_dagcbor::from_slice(&bytes).unwrap();

        assert_eq!(chunk_ref, deserialized);
    }

    // ============================================
    // ChunkData Tests
    // ============================================

    #[test]
    fn test_chunk_data_new() {
        let data = vec![1, 2, 3, 4, 5];
        let chunk_data = ChunkData::new(data.clone(), 100);

        assert_eq!(chunk_data.data, data);
        assert_eq!(chunk_data.offset, 100);
    }

    #[test]
    fn test_chunk_data_size() {
        let chunk_data = ChunkData::new(vec![0u8; 100], 0);
        assert_eq!(chunk_data.size(), 100);
    }

    #[test]
    fn test_chunk_data_size_empty() {
        let chunk_data = ChunkData::new(vec![], 0);
        assert_eq!(chunk_data.size(), 0);
    }

    #[test]
    fn test_chunk_data_to_chunk_ref() {
        let data = b"hello world".to_vec();
        let chunk_data = ChunkData::new(data.clone(), 100);
        let chunk_ref = chunk_data.to_chunk_ref();

        assert_eq!(chunk_ref.offset, 100);
        assert_eq!(chunk_ref.size, 11);

        // CID should match direct computation
        let expected_cid = ContentId::from_bytes(&data);
        assert_eq!(chunk_ref.cid, expected_cid);
    }

    #[test]
    fn test_chunk_data_to_chunk_ref_empty() {
        let chunk_data = ChunkData::new(vec![], 50);
        let chunk_ref = chunk_data.to_chunk_ref();

        assert_eq!(chunk_ref.offset, 50);
        assert_eq!(chunk_ref.size, 0);
    }

    #[test]
    fn test_chunk_data_to_chunk_ref_deterministic() {
        let data = b"deterministic test".to_vec();
        let chunk1 = ChunkData::new(data.clone(), 0);
        let chunk2 = ChunkData::new(data, 0);

        assert_eq!(chunk1.to_chunk_ref().cid, chunk2.to_chunk_ref().cid);
    }

    #[test]
    fn test_chunk_data_clone() {
        let chunk_data = ChunkData::new(vec![1, 2, 3], 100);
        let cloned = chunk_data.clone();

        assert_eq!(chunk_data, cloned);
    }

    #[test]
    fn test_chunk_data_eq() {
        let chunk1 = ChunkData::new(vec![1, 2, 3], 100);
        let chunk2 = ChunkData::new(vec![1, 2, 3], 100);
        let chunk3 = ChunkData::new(vec![1, 2, 3], 200);
        let chunk4 = ChunkData::new(vec![4, 5, 6], 100);

        assert_eq!(chunk1, chunk2);
        assert_ne!(chunk1, chunk3); // Different offset
        assert_ne!(chunk1, chunk4); // Different data
    }

    #[test]
    fn test_chunk_data_debug() {
        let chunk_data = ChunkData::new(vec![1, 2, 3], 100);
        let debug = format!("{:?}", chunk_data);

        assert!(debug.contains("100"));
        assert!(debug.contains("ChunkData"));
    }

    // ============================================
    // Chunker Trait Tests
    // ============================================

    #[test]
    fn test_chunker_target_size_uses_config() {
        let config = ChunkingConfig::new(100, 500, 1000);
        let chunker = FixedChunker::new(config);

        assert_eq!(chunker.config().target_size, 500);
    }

    // Compile-time check that Chunker is Send + Sync
    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn test_chunker_is_send_sync() {
        _assert_send_sync::<FixedChunker>();
        _assert_send_sync::<FastCdcChunker>();
        _assert_send_sync::<RabinChunker>();
    }

    #[test]
    fn test_boxed_chunker_is_send_sync() {
        fn takes_send_sync<T: Send + Sync>(_: T) {}

        let chunker = ChunkingAlgorithm::FastCdc.create_chunker(ChunkingConfig::default());

        takes_send_sync(chunker);
    }
}
