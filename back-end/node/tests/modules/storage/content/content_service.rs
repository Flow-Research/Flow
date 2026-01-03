//! Integration tests for ContentService.

use node::modules::network::gossipsub::Topic;
use node::modules::network::manager::NetworkManager;
use node::modules::storage::content::{
    AnnouncementOutcome, BlockStore, BlockStoreConfig, ChunkingConfig, ContentId, ContentService,
    ContentServiceConfig, DagMetadata,
};
use serial_test::serial;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::fs;

use crate::bootstrap::init::{
    TwoNodeTestConfig, cleanup_two_node_network, create_test_node_data, setup_test_env,
    setup_two_node_network,
};

// ============================================================================
// Helper Functions
// ============================================================================

async fn setup_local_service(prefix: &str) -> (ContentService, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    setup_test_env(&temp_dir, prefix);

    let store_config = BlockStoreConfig {
        db_path: temp_dir.path().join("blocks"),
        ..Default::default()
    };
    let store = Arc::new(BlockStore::new(store_config).unwrap());

    let node_data = create_test_node_data();
    let network = Arc::new(NetworkManager::new(&node_data).await.unwrap());

    let config = ContentServiceConfig::local_only();
    let service = ContentService::new(store, network, config);

    (service, temp_dir)
}

// ============================================================================
// Unit-style Tests (no network)
// ============================================================================

#[tokio::test]
#[serial]
async fn test_content_service_publish_local_only() {
    let (service, _temp_dir) = setup_local_service("cs_publish_local").await;

    // Publish
    let content = b"Hello, ContentService!";
    let metadata = DagMetadata::new("did:key:test").name("test.txt");
    let result = service.publish(content, metadata).await.unwrap();

    // Verify result
    assert!(!result.root_cid.to_string().is_empty());
    assert_eq!(result.stats.content_size, content.len() as u64);
    assert!(result.stats.chunk_count >= 1);

    // Announcements should be skipped (network not running + config disabled)
    assert!(result.announcement.is_empty());

    // Content should be locally available
    assert!(service.is_local(&result.root_cid));
}

#[tokio::test]
#[serial]
async fn test_content_service_fetch_local() {
    let (service, _temp_dir) = setup_local_service("cs_fetch_local").await;

    // Publish
    let original = b"Content to fetch locally";
    let metadata = DagMetadata::new("did:key:test");
    let result = service.publish(original, metadata).await.unwrap();

    // Fetch
    let fetched = service.fetch(&result.root_cid).await.unwrap();
    assert_eq!(fetched, original);
}

#[tokio::test]
#[serial]
async fn test_content_service_get_info() {
    let (service, _temp_dir) = setup_local_service("cs_get_info").await;

    // Publish with metadata
    let content = b"Test content for metadata";
    let metadata = DagMetadata::new("did:key:publisher123")
        .name("document.txt")
        .content_type("text/plain");

    let result = service.publish(content, metadata).await.unwrap();

    // Get info
    let manifest = service.get_info(&result.root_cid).await.unwrap();

    assert_eq!(manifest.name, Some("document.txt".to_string()));
    assert_eq!(manifest.content_type, Some("text/plain".to_string()));
    assert_eq!(manifest.publisher_did, "did:key:publisher123");
    assert_eq!(manifest.total_size, content.len() as u64);
}

#[tokio::test]
#[serial]
async fn test_content_service_list_local() {
    let (service, _temp_dir) = setup_local_service("cs_list_local").await;

    // Publish multiple items
    let result1 = service
        .publish(b"Content 1", DagMetadata::new("did:key:test"))
        .await
        .unwrap();
    let result2 = service
        .publish(b"Content 2", DagMetadata::new("did:key:test"))
        .await
        .unwrap();
    let result3 = service
        .publish(b"Content 3", DagMetadata::new("did:key:test"))
        .await
        .unwrap();

    // List
    let local_cids = service.list_local().unwrap();

    assert!(local_cids.len() >= 3);
    assert!(local_cids.contains(&result1.root_cid));
    assert!(local_cids.contains(&result2.root_cid));
    assert!(local_cids.contains(&result3.root_cid));
}

#[tokio::test]
#[serial]
async fn test_content_service_publish_file() {
    let (service, temp_dir) = setup_local_service("cs_publish_file").await;

    // Create test file
    let file_path = temp_dir.path().join("test_file.txt");
    let content = b"File content to publish";
    fs::write(&file_path, content).await.unwrap();

    // Publish file
    let metadata = DagMetadata::new("did:key:test");
    let result = service.publish_file(&file_path, metadata).await.unwrap();

    // Name should be inferred from filename
    assert_eq!(result.manifest.name, Some("test_file.txt".to_string()));
    assert_eq!(result.stats.content_size, content.len() as u64);

    // Fetch and verify
    let fetched = service.fetch(&result.root_cid).await.unwrap();
    assert_eq!(fetched, content);
}

#[tokio::test]
#[serial]
async fn test_content_service_fetch_to_file() {
    let (service, temp_dir) = setup_local_service("cs_fetch_file").await;

    // Publish
    let content = b"Content to save to file";
    let metadata = DagMetadata::new("did:key:test");
    let result = service.publish(content, metadata).await.unwrap();

    // Fetch to file
    let output_path = temp_dir.path().join("output").join("downloaded.txt");
    service
        .fetch_to_file(&result.root_cid, &output_path)
        .await
        .unwrap();

    // Verify file contents
    let file_content = fs::read(&output_path).await.unwrap();
    assert_eq!(file_content, content);
}

#[tokio::test]
#[serial]
async fn test_content_service_is_local_false() {
    let (service, _temp_dir) = setup_local_service("cs_is_local_false").await;

    let fake_cid = ContentId::from_bytes(b"nonexistent content");
    assert!(!service.is_local(&fake_cid));
}

#[tokio::test]
#[serial]
async fn test_content_service_publish_large_content() {
    let temp_dir = TempDir::new().unwrap();
    setup_test_env(&temp_dir, "cs_large_content");

    let store_config = BlockStoreConfig {
        db_path: temp_dir.path().join("blocks"),
        ..Default::default()
    };
    let store = Arc::new(BlockStore::new(store_config).unwrap());

    let node_data = create_test_node_data();
    let network = Arc::new(NetworkManager::new(&node_data).await.unwrap());

    let config = ContentServiceConfig {
        chunking_config: ChunkingConfig::small(),
        ..ContentServiceConfig::local_only()
    };
    let service = ContentService::new(store, network, config);

    let content: Vec<u8> = (0..1_000_000).map(|i| (i % 256) as u8).collect();
    let metadata = DagMetadata::new("did:key:test").name("large.bin");

    let result = service.publish(&content, metadata).await.unwrap();

    assert!(result.stats.chunk_count > 1);

    let fetched = service.fetch(&result.root_cid).await.unwrap();
    assert_eq!(fetched, content);
}

#[tokio::test]
#[serial]
async fn test_content_service_publish_empty_content() {
    let (service, _temp_dir) = setup_local_service("cs_empty_content").await;

    let content = b"";
    let metadata = DagMetadata::new("did:key:test").name("empty.txt");

    let result = service.publish(content, metadata).await.unwrap();
    assert_eq!(result.stats.content_size, 0);

    let fetched = service.fetch(&result.root_cid).await.unwrap();
    assert!(fetched.is_empty());
}

#[tokio::test]
#[serial]
async fn test_content_service_publish_file_not_found() {
    let (service, _temp_dir) = setup_local_service("cs_file_not_found").await;

    let result = service
        .publish_file(
            "/nonexistent/path/file.txt",
            DagMetadata::new("did:key:test"),
        )
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("not found") || err.to_string().contains("Invalid argument"));
}

#[tokio::test]
#[serial]
async fn test_content_service_with_custom_metadata() {
    let (service, _temp_dir) = setup_local_service("cs_custom_metadata").await;

    let content = b"Content with custom metadata";
    let metadata = DagMetadata::new("did:key:alice")
        .name("report.pdf")
        .content_type("application/pdf")
        .with_metadata("author", "Alice")
        .with_metadata("version", "1.0")
        .with_metadata("department", "Engineering");

    let result = service.publish(content, metadata).await.unwrap();

    let manifest = service.get_info(&result.root_cid).await.unwrap();
    assert_eq!(manifest.metadata.get("author"), Some(&"Alice".to_string()));
    assert_eq!(manifest.metadata.get("version"), Some(&"1.0".to_string()));
    assert_eq!(
        manifest.metadata.get("department"),
        Some(&"Engineering".to_string())
    );
}

// ============================================================================
// Integration Tests (two-node network)
// ============================================================================

#[tokio::test]
#[serial]
async fn test_content_service_publish_and_fetch_network() {
    let ctx = setup_two_node_network(TwoNodeTestConfig::content_service()).await;

    let _seeker_sub = ctx
        .seeker_manager
        .subscribe_topic(&Topic::ContentAnnouncements)
        .await
        .expect("Failed to subscribe seeker to announcements");

    // Wait for GossipSub mesh formation (2-3 heartbeat intervals)
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    // Provider publishes with announcements enabled
    let provider_config = ContentServiceConfig::default();
    let provider_service = ContentService::new(
        ctx.provider_store.clone(),
        ctx.provider_manager.clone(),
        provider_config,
    );

    let content = b"Network content via ContentService";
    let metadata = DagMetadata::new("did:key:provider").name("network.txt");
    let result = provider_service.publish(content, metadata).await.unwrap();

    // Verify announcements
    assert!(result.announcement.dht.is_success());
    assert!(result.announcement.gossipsub.is_success());

    // Wait for DHT propagation
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Seeker fetches
    let seeker_config = ContentServiceConfig::local_only();
    let seeker_service = ContentService::new(
        ctx.seeker_store.clone(),
        ctx.seeker_manager.clone(),
        seeker_config,
    );

    // Content should NOT be local on seeker initially
    assert!(!seeker_service.is_local(&result.root_cid));

    // Fetch from network
    let fetched = seeker_service.fetch(&result.root_cid).await.unwrap();
    assert_eq!(fetched, content);

    // Content should now be cached locally on seeker
    assert!(seeker_service.is_local(&result.root_cid));

    cleanup_two_node_network(ctx).await;
}

#[tokio::test]
#[serial]
async fn test_content_service_announcement_dht_only() {
    let ctx = setup_two_node_network(TwoNodeTestConfig::content_service()).await;

    // Provider with only DHT enabled
    let provider_config = ContentServiceConfig::default().without_gossip();
    let provider_service = ContentService::new(
        ctx.provider_store.clone(),
        ctx.provider_manager.clone(),
        provider_config,
    );

    let content = b"DHT only announcement";
    let metadata = DagMetadata::new("did:key:provider");
    let result = provider_service.publish(content, metadata).await.unwrap();

    assert!(result.announcement.dht.is_success());
    assert!(matches!(
        result.announcement.gossipsub,
        AnnouncementOutcome::Skipped
    ));

    cleanup_two_node_network(ctx).await;
}

#[tokio::test]
#[serial]
async fn test_content_service_get_info_from_network() {
    let ctx = setup_two_node_network(TwoNodeTestConfig::content_service()).await;

    // Provider publishes
    let provider_service = ContentService::new(
        ctx.provider_store.clone(),
        ctx.provider_manager.clone(),
        ContentServiceConfig::default(),
    );

    let content = b"Content with rich metadata";
    let metadata = DagMetadata::new("did:key:alice")
        .name("document.pdf")
        .content_type("application/pdf")
        .with_metadata("author", "Alice");

    let result = provider_service.publish(content, metadata).await.unwrap();

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Seeker gets info
    let seeker_service = ContentService::new(
        ctx.seeker_store.clone(),
        ctx.seeker_manager.clone(),
        ContentServiceConfig::local_only(),
    );

    let manifest = seeker_service.get_info(&result.root_cid).await.unwrap();

    assert_eq!(manifest.name, Some("document.pdf".to_string()));
    assert_eq!(manifest.content_type, Some("application/pdf".to_string()));
    assert_eq!(manifest.publisher_did, "did:key:alice");
    assert_eq!(manifest.metadata.get("author"), Some(&"Alice".to_string()));

    cleanup_two_node_network(ctx).await;
}
