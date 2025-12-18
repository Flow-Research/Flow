//! Integration tests for DagReader with NetworkProvider.
//!
//! These tests verify that DagReader can fetch multi-block DAGs
//! from remote peers using NetworkProvider.

use node::modules::storage::content::{
    ChunkingConfig, ContentId, DagBuilder, DagMetadata, DagReader, FastCdcChunker, FixedChunker,
    LocalProvider, NetworkProvider, NetworkProviderConfig,
};
use serial_test::serial;
use std::time::Duration;

use crate::bootstrap::init::{TwoNodeTestConfig, cleanup_two_node_network, setup_two_node_network};

fn create_fixed_chunker(chunk_size: usize) -> Box<dyn node::modules::storage::content::Chunker> {
    let config = ChunkingConfig {
        min_size: chunk_size,
        target_size: chunk_size,
        max_size: chunk_size,
    };
    Box::new(FixedChunker::new(config))
}

fn create_fastcdc_chunker() -> Box<dyn node::modules::storage::content::Chunker> {
    let config = ChunkingConfig {
        min_size: 256,
        target_size: 1024,
        max_size: 4096,
    };
    Box::new(FastCdcChunker::new(config))
}

const TEST_DID: &str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";

// ============================================================================
// DagReader Network Tests
// ============================================================================

#[tokio::test]
#[serial]
async fn test_dag_reader_single_chunk_from_network() {
    let ctx = setup_two_node_network(TwoNodeTestConfig::dag_network()).await;

    // Provider: Build a single-chunk DAG
    let chunker = create_fixed_chunker(1024);
    let builder = DagBuilder::new(ctx.provider_store.clone(), chunker);

    let data = b"Hello from the network!";
    let metadata = DagMetadata::new(TEST_DID)
        .name("hello.txt")
        .content_type("text/plain");

    let build_result = builder.build_from_bytes(data, metadata).unwrap();
    let root_cid = build_result.root_cid.clone();

    // Provider: Announce all blocks (manifest + chunks)
    ctx.provider_manager
        .start_providing(root_cid.clone())
        .await
        .expect("Failed to announce manifest");

    // Also announce chunk CIDs
    for chunk_cid in &build_result.manifest.children {
        ctx.provider_manager
            .start_providing(chunk_cid.clone())
            .await
            .expect("Failed to announce chunk");
    }

    // Wait for DHT propagation
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Seeker: Verify content is not local
    assert!(!ctx.seeker_store.has(&root_cid).unwrap());

    // Seeker: Create DagReader with NetworkProvider
    let seeker_local = LocalProvider::new(ctx.seeker_store.clone());
    let network_provider = NetworkProvider::new(
        seeker_local,
        ctx.seeker_manager.clone(),
        NetworkProviderConfig::default(),
    );
    let reader = DagReader::with_provider(network_provider);

    // Seeker: Read content from network
    let content = reader.read_all(&root_cid).await.unwrap();
    assert_eq!(content, data);

    // Verify blocks are now cached locally
    assert!(ctx.seeker_store.has(&root_cid).unwrap());

    cleanup_two_node_network(ctx).await;
}

#[tokio::test]
#[serial]
async fn test_dag_reader_multi_chunk_from_network() {
    let ctx = setup_two_node_network(TwoNodeTestConfig::dag_network()).await;

    // Provider: Build a multi-chunk DAG
    let chunker = create_fixed_chunker(256);
    let builder = DagBuilder::new(ctx.provider_store.clone(), chunker);

    // 1KB of data = 4 chunks
    let data: Vec<u8> = (0..1024).map(|i| (i % 256) as u8).collect();
    let metadata = DagMetadata::new(TEST_DID).name("multi.bin");

    let build_result = builder.build_from_bytes(&data, metadata).unwrap();
    let root_cid = build_result.root_cid.clone();

    assert_eq!(build_result.chunks_stored, 4);

    // Provider: Announce manifest and all chunks
    ctx.provider_manager
        .start_providing(root_cid.clone())
        .await
        .expect("Failed to announce manifest");

    for chunk_cid in &build_result.manifest.children {
        ctx.provider_manager
            .start_providing(chunk_cid.clone())
            .await
            .expect("Failed to announce chunk");
    }

    tokio::time::sleep(Duration::from_secs(2)).await;

    // Seeker: Fetch via DagReader
    let seeker_local = LocalProvider::new(ctx.seeker_store.clone());
    let network_provider = NetworkProvider::new(
        seeker_local,
        ctx.seeker_manager.clone(),
        NetworkProviderConfig::default(),
    );
    let reader = DagReader::with_provider(network_provider);

    let content = reader.read_all(&root_cid).await.unwrap();
    assert_eq!(content, data);

    // All chunks should be cached
    for chunk_cid in &build_result.manifest.children {
        assert!(ctx.seeker_store.has(chunk_cid).unwrap());
    }

    cleanup_two_node_network(ctx).await;
}

#[tokio::test]
#[serial]
async fn test_dag_reader_manifest_from_network() {
    let ctx = setup_two_node_network(TwoNodeTestConfig::dag_network()).await;

    // Provider: Build content
    let chunker = create_fastcdc_chunker();
    let builder = DagBuilder::new(ctx.provider_store.clone(), chunker);

    let data = b"Metadata test content";
    let metadata = DagMetadata::new(TEST_DID)
        .name("document.pdf")
        .content_type("application/pdf")
        .with_metadata("author", "Alice")
        .with_metadata("version", "1.0");

    let build_result = builder.build_from_bytes(data, metadata).unwrap();
    let root_cid = build_result.root_cid.clone();

    // Announce
    ctx.provider_manager
        .start_providing(root_cid.clone())
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_secs(2)).await;

    // Seeker: Read manifest only
    let seeker_local = LocalProvider::new(ctx.seeker_store.clone());
    let network_provider = NetworkProvider::new(
        seeker_local,
        ctx.seeker_manager.clone(),
        NetworkProviderConfig::default(),
    );
    let reader = DagReader::with_provider(network_provider);

    let manifest = reader.read_manifest(&root_cid).await.unwrap();

    assert_eq!(manifest.name, Some("document.pdf".into()));
    assert_eq!(manifest.content_type, Some("application/pdf".into()));
    assert_eq!(manifest.publisher_did, TEST_DID);
    assert_eq!(manifest.get_metadata("author"), Some("Alice"));
    assert_eq!(manifest.get_metadata("version"), Some("1.0"));

    cleanup_two_node_network(ctx).await;
}

#[tokio::test]
#[serial]
async fn test_dag_reader_range_from_network() {
    let ctx = setup_two_node_network(TwoNodeTestConfig::dag_network()).await;

    // Provider: Build multi-chunk content
    let chunker = create_fixed_chunker(256);
    let builder = DagBuilder::new(ctx.provider_store.clone(), chunker);

    let data: Vec<u8> = (0..1024).map(|i| (i % 256) as u8).collect();
    let metadata = DagMetadata::new(TEST_DID);

    let build_result = builder.build_from_bytes(&data, metadata).unwrap();
    let root_cid = build_result.root_cid.clone();

    // Announce all
    ctx.provider_manager
        .start_providing(root_cid.clone())
        .await
        .unwrap();
    for chunk_cid in &build_result.manifest.children {
        ctx.provider_manager
            .start_providing(chunk_cid.clone())
            .await
            .unwrap();
    }

    tokio::time::sleep(Duration::from_secs(2)).await;

    // Seeker: Read a range spanning multiple chunks
    let seeker_local = LocalProvider::new(ctx.seeker_store.clone());
    let network_provider = NetworkProvider::new(
        seeker_local,
        ctx.seeker_manager.clone(),
        NetworkProviderConfig::default(),
    );
    let reader = DagReader::with_provider(network_provider);

    // Range across chunk boundaries (200..400 spans chunks)
    let content = reader.read_range(&root_cid, 200..400).await.unwrap();
    assert_eq!(content, &data[200..400]);

    cleanup_two_node_network(ctx).await;
}

#[tokio::test]
#[serial]
async fn test_dag_reader_collect_chunks_from_network() {
    let ctx = setup_two_node_network(TwoNodeTestConfig::dag_network()).await;

    // Provider: Build content with known chunk count
    let chunker = create_fixed_chunker(256);
    let builder = DagBuilder::new(ctx.provider_store.clone(), chunker);

    let data: Vec<u8> = (0..1024).map(|i| (i % 256) as u8).collect();
    let metadata = DagMetadata::new(TEST_DID);

    let build_result = builder.build_from_bytes(&data, metadata).unwrap();
    let root_cid = build_result.root_cid.clone();

    // Announce all
    ctx.provider_manager
        .start_providing(root_cid.clone())
        .await
        .unwrap();
    for chunk_cid in &build_result.manifest.children {
        ctx.provider_manager
            .start_providing(chunk_cid.clone())
            .await
            .unwrap();
    }

    tokio::time::sleep(Duration::from_secs(2)).await;

    // Seeker: Collect chunks individually
    let seeker_local = LocalProvider::new(ctx.seeker_store.clone());
    let network_provider = NetworkProvider::new(
        seeker_local,
        ctx.seeker_manager.clone(),
        NetworkProviderConfig::default(),
    );
    let reader = DagReader::with_provider(network_provider);

    let chunks = reader.collect_chunks(&root_cid).await.unwrap();

    assert_eq!(chunks.len(), 4);

    // Verify concatenation
    let reconstructed: Vec<u8> = chunks.into_iter().flatten().collect();
    assert_eq!(reconstructed, data);

    cleanup_two_node_network(ctx).await;
}

#[tokio::test]
#[serial]
async fn test_dag_reader_large_file_from_network() {
    let ctx = setup_two_node_network(TwoNodeTestConfig::dag_network()).await;

    // Provider: Build larger content (50KB)
    let chunker = create_fastcdc_chunker();
    let builder = DagBuilder::new(ctx.provider_store.clone(), chunker);

    let data: Vec<u8> = (0..51200).map(|i| (i % 256) as u8).collect();
    let metadata = DagMetadata::new(TEST_DID)
        .name("large.bin")
        .content_type("application/octet-stream");

    let build_result = builder.build_from_bytes(&data, metadata).unwrap();
    let root_cid = build_result.root_cid.clone();

    // Announce manifest and all chunks
    ctx.provider_manager
        .start_providing(root_cid.clone())
        .await
        .unwrap();
    for chunk_cid in &build_result.manifest.children {
        ctx.provider_manager
            .start_providing(chunk_cid.clone())
            .await
            .unwrap();
    }

    tokio::time::sleep(Duration::from_secs(3)).await; // Extra time for larger file

    // Seeker: Fetch entire file
    let seeker_local = LocalProvider::new(ctx.seeker_store.clone());
    let network_provider = NetworkProvider::new(
        seeker_local,
        ctx.seeker_manager.clone(),
        NetworkProviderConfig::default(),
    );
    let reader = DagReader::with_provider(network_provider);

    let content = reader.read_all(&root_cid).await.unwrap();
    assert_eq!(content.len(), 51200);
    assert_eq!(content, data);

    cleanup_two_node_network(ctx).await;
}

#[tokio::test]
#[serial]
async fn test_dag_reader_get_metadata_from_network() {
    let ctx = setup_two_node_network(TwoNodeTestConfig::dag_network()).await;

    // Provider: Build content
    let chunker = create_fastcdc_chunker();
    let builder = DagBuilder::new(ctx.provider_store.clone(), chunker);

    let data = b"Content for metadata";
    let metadata = DagMetadata::new(TEST_DID)
        .name("info.txt")
        .content_type("text/plain")
        .with_metadata("created", "2024-01-01");

    let build_result = builder.build_from_bytes(data, metadata).unwrap();
    let root_cid = build_result.root_cid.clone();

    ctx.provider_manager
        .start_providing(root_cid.clone())
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_secs(2)).await;

    // Seeker: Get metadata without fetching content
    let seeker_local = LocalProvider::new(ctx.seeker_store.clone());
    let network_provider = NetworkProvider::new(
        seeker_local,
        ctx.seeker_manager.clone(),
        NetworkProviderConfig::default(),
    );
    let reader = DagReader::with_provider(network_provider);

    let manifest = reader.get_metadata(&root_cid).await.unwrap();

    assert_eq!(manifest.name, Some("info.txt".into()));
    assert_eq!(manifest.total_size, data.len() as u64);
    assert_eq!(manifest.get_metadata("created"), Some("2024-01-01"));

    cleanup_two_node_network(ctx).await;
}

#[tokio::test]
#[serial]
async fn test_dag_reader_local_then_network_fallback() {
    let ctx = setup_two_node_network(TwoNodeTestConfig::dag_network()).await;

    // Provider: Build content
    let chunker = create_fixed_chunker(256);
    let builder = DagBuilder::new(ctx.provider_store.clone(), chunker);

    let data: Vec<u8> = (0..512).map(|i| (i % 256) as u8).collect();
    let metadata = DagMetadata::new(TEST_DID);

    let build_result = builder.build_from_bytes(&data, metadata).unwrap();
    let root_cid = build_result.root_cid.clone();

    // Announce
    ctx.provider_manager
        .start_providing(root_cid.clone())
        .await
        .unwrap();
    for chunk_cid in &build_result.manifest.children {
        ctx.provider_manager
            .start_providing(chunk_cid.clone())
            .await
            .unwrap();
    }

    tokio::time::sleep(Duration::from_secs(2)).await;

    // Seeker: Pre-cache ONLY the manifest locally
    let manifest_block = ctx.provider_store.get(&root_cid).unwrap().unwrap();
    ctx.seeker_store.put(&manifest_block).unwrap();

    // Verify: manifest is local, chunks are not
    assert!(ctx.seeker_store.has(&root_cid).unwrap());
    for chunk_cid in &build_result.manifest.children {
        assert!(!ctx.seeker_store.has(chunk_cid).unwrap());
    }

    // Seeker: Read all - manifest from local, chunks from network
    let seeker_local = LocalProvider::new(ctx.seeker_store.clone());
    let network_provider = NetworkProvider::new(
        seeker_local,
        ctx.seeker_manager.clone(),
        NetworkProviderConfig::default(),
    );
    let reader = DagReader::with_provider(network_provider);

    let content = reader.read_all(&root_cid).await.unwrap();
    assert_eq!(content, data);

    // Chunks should now be cached
    for chunk_cid in &build_result.manifest.children {
        assert!(ctx.seeker_store.has(chunk_cid).unwrap());
    }

    cleanup_two_node_network(ctx).await;
}

#[tokio::test]
#[serial]
async fn test_dag_reader_manifest_not_found_network() {
    let ctx = setup_two_node_network(TwoNodeTestConfig::dag_network()).await;

    // Create a fake CID that doesn't exist anywhere
    let fake_cid = ContentId::from_bytes(b"nonexistent-dag-manifest");

    // Seeker: Try to read non-existent DAG
    let seeker_local = LocalProvider::new(ctx.seeker_store.clone());
    let network_provider = NetworkProvider::new(
        seeker_local,
        ctx.seeker_manager.clone(),
        NetworkProviderConfig::default(),
    );
    let reader = DagReader::with_provider(network_provider);

    let result = reader.read_all(&fake_cid).await;
    assert!(result.is_err());

    cleanup_two_node_network(ctx).await;
}
