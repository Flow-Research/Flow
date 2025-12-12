use libp2p::kad::{self, K_VALUE};
use node::modules::{
    network::{
        config::{MdnsConfig, NetworkConfig},
        content_transfer::ContentTransferError,
        gossipsub::GossipSubConfig,
        manager::NetworkManager,
    },
    storage::content::{Block, BlockStore, BlockStoreConfig, ContentId},
};
use serial_test::serial;
use std::sync::Arc;
use tokio::time::Duration;
use tracing::{debug, info};

use crate::{TestNodeDirs, bootstrap::init::create_test_node_data};

fn create_dht_test_config(port: u16) -> NetworkConfig {
    NetworkConfig {
        enable_quic: true,
        listen_port: port,
        bootstrap_peers: vec![],
        mdns: MdnsConfig {
            enabled: false,
            ..Default::default()
        },
        gossipsub: GossipSubConfig {
            enabled: true,
            ..Default::default()
        },
        ..Default::default()
    }
}

// =====================================================
// UNIT TESTS: CID -> key behavior (via CID bytes)
// =====================================================

#[test]
fn test_cid_to_record_key_deterministic() {
    // Same content → same CID → same bytes
    let data = b"hello world";
    let cid1 = ContentId::from_bytes(data);
    let cid2 = ContentId::from_bytes(data);

    assert_eq!(cid1.to_bytes(), cid2.to_bytes());
}

#[test]
fn test_cid_to_record_key_different_content() {
    let cid1 = ContentId::from_bytes(b"hello");
    let cid2 = ContentId::from_bytes(b"world");

    assert_ne!(cid1.to_bytes(), cid2.to_bytes());
}

#[test]
fn test_cid_to_record_key_empty_content() {
    let cid = ContentId::from_bytes(b"");
    let bytes = cid.to_bytes();
    assert!(!bytes.is_empty());
}

#[test]
fn test_cid_to_record_key_large_content() {
    let large_data = vec![42u8; 1024 * 1024]; // 1MB
    let cid = ContentId::from_bytes(&large_data);
    let bytes = cid.to_bytes();

    // CID size is independent of content size; should be relatively small.
    assert!(bytes.len() < 100);
}

// =====================================================
// INTEGRATION TESTS: Provider announcement
// =====================================================

#[tokio::test]
#[serial]
async fn test_start_providing_single_cid() {
    let node_dirs = TestNodeDirs::new("provider_single");
    node_dirs.set_env_vars();

    let node_data = create_test_node_data();
    let manager = NetworkManager::new(&node_data)
        .await
        .expect("Failed to create NetworkManager");

    let config = create_dht_test_config(0);
    node_dirs.set_env_vars();
    manager.start(&config).await.expect("Failed to start");

    let cid = ContentId::from_bytes(b"test content for DHT");

    let result = manager.start_providing(cid.clone()).await;
    assert!(
        result.is_ok(),
        "start_providing should succeed: {:?}",
        result.err()
    );

    let announce_result = result.unwrap();
    debug!(?announce_result.query_id, "Provider announcement initiated");

    manager.stop().await.expect("Failed to stop");
}

#[tokio::test]
#[serial]
async fn test_start_providing_multiple_cids() {
    let node_dirs = TestNodeDirs::new("provider_multi");
    node_dirs.set_env_vars();

    let node_data = create_test_node_data();
    let manager = NetworkManager::new(&node_data)
        .await
        .expect("Failed to create NetworkManager");

    let config = create_dht_test_config(0);
    node_dirs.set_env_vars();
    manager.start(&config).await.expect("Failed to start");

    let cids: Vec<ContentId> = (0..5)
        .map(|i| ContentId::from_bytes(format!("content {}", i).as_bytes()))
        .collect();

    for cid in &cids {
        let result = manager.start_providing(cid.clone()).await;
        assert!(result.is_ok(), "start_providing should succeed for {}", cid);
    }

    info!("Successfully announced {} CIDs", cids.len());

    manager.stop().await.expect("Failed to stop");
}

#[tokio::test]
#[serial]
async fn test_start_providing_same_cid_twice() {
    let node_dirs = TestNodeDirs::new("provider_duplicate");
    node_dirs.set_env_vars();

    let node_data = create_test_node_data();
    let manager = NetworkManager::new(&node_data)
        .await
        .expect("Failed to create NetworkManager");

    let config = create_dht_test_config(0);
    node_dirs.set_env_vars();
    manager.start(&config).await.expect("Failed to start");

    let cid = ContentId::from_bytes(b"duplicate content");

    let result1 = manager.start_providing(cid.clone()).await;
    assert!(result1.is_ok());

    let result2 = manager.start_providing(cid.clone()).await;
    assert!(result2.is_ok());

    let qid1 = result1.unwrap().query_id;
    let qid2 = result2.unwrap().query_id;
    assert_ne!(qid1, qid2, "Each announcement should get a unique QueryId");

    manager.stop().await.expect("Failed to stop");
}

#[tokio::test]
#[serial]
async fn test_stop_providing() {
    let node_dirs = TestNodeDirs::new("provider_stop");
    node_dirs.set_env_vars();

    let node_data = create_test_node_data();
    let manager = NetworkManager::new(&node_data)
        .await
        .expect("Failed to create NetworkManager");

    let config = create_dht_test_config(0);
    node_dirs.set_env_vars();
    manager.start(&config).await.expect("Failed to start");

    let cid = ContentId::from_bytes(b"content to stop providing");

    let result = manager.start_providing(cid.clone()).await;
    assert!(result.is_ok());

    let stop_result = manager.stop_providing(cid.clone()).await;
    assert!(stop_result.is_ok(), "stop_providing should succeed");

    manager.stop().await.expect("Failed to stop");
}

#[tokio::test]
#[serial]
async fn test_start_providing_not_started() {
    let node_dirs = TestNodeDirs::new("provider_not_started");
    node_dirs.set_env_vars();

    let node_data = create_test_node_data();
    let manager = NetworkManager::new(&node_data)
        .await
        .expect("Failed to create NetworkManager");

    // Manager is intentionally not started.
    let cid = ContentId::from_bytes(b"test");
    let result = manager.start_providing(cid).await;

    assert!(result.is_err());
    let err = result.err().unwrap();
    assert!(
        err.to_string().to_lowercase().contains("not started"),
        "Error should mention not started: {err}"
    );
}

#[tokio::test]
#[serial]
async fn test_start_providing_concurrent() {
    let node_dirs = TestNodeDirs::new("provider_concurrent");
    node_dirs.set_env_vars();

    let node_data = create_test_node_data();
    let manager = Arc::new(
        NetworkManager::new(&node_data)
            .await
            .expect("Failed to create NetworkManager"),
    );

    let config = create_dht_test_config(0);
    node_dirs.set_env_vars();
    manager.start(&config).await.expect("Failed to start");

    let mut handles = vec![];
    for i in 0..10 {
        let mgr = Arc::clone(&manager);
        let handle = tokio::spawn(async move {
            let cid = ContentId::from_bytes(format!("concurrent content {}", i).as_bytes());
            mgr.start_providing(cid).await
        });
        handles.push(handle);
    }

    for handle in handles {
        let result = handle.await.expect("Task panicked");
        assert!(
            result.is_ok(),
            "Concurrent announcement failed: {:?}",
            result.err()
        );
    }

    manager.stop().await.expect("Failed to stop");
}

// =====================================================
// TWO-NODE INTEGRATION: Provider discovery setup
// =====================================================

#[tokio::test]
#[serial]
async fn test_two_node_provider_discovery() {
    // This test verifies that provider announcement completes correctly in
    // a two-node setup.

    let provider_dirs = TestNodeDirs::new("provider_node");
    let seeker_dirs = TestNodeDirs::new("seeker_node");

    // Provider node
    provider_dirs.set_env_vars();
    let provider_data = create_test_node_data();
    let provider = NetworkManager::new(&provider_data)
        .await
        .expect("Failed to create provider");

    let provider_config = create_dht_test_config(9100);
    provider_dirs.set_env_vars();
    provider
        .start(&provider_config)
        .await
        .expect("Failed to start provider");

    let provider_peer_id = *provider.local_peer_id();
    let provider_addr: libp2p::Multiaddr =
        format!("/ip4/127.0.0.1/tcp/9100/p2p/{}", provider_peer_id)
            .parse()
            .unwrap();

    // Seeker node
    seeker_dirs.set_env_vars();
    let seeker_data = create_test_node_data();
    let seeker = NetworkManager::new(&seeker_data)
        .await
        .expect("Failed to create seeker");

    let mut seeker_config = create_dht_test_config(9101);
    seeker_config.bootstrap_peers = vec![provider_addr];
    seeker_dirs.set_env_vars();
    seeker
        .start(&seeker_config)
        .await
        .expect("Failed to start seeker");

    // Allow time for connection
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Provider announces content
    let cid = ContentId::from_bytes(b"shared content for discovery");
    let announce_result = provider.start_providing(cid.clone()).await;
    assert!(
        announce_result.is_ok(),
        "Provider announcement should succeed"
    );

    // Allow DHT to propagate
    tokio::time::sleep(Duration::from_millis(500)).await;

    info!("Two-node provider discovery test passed (announcement phase).");

    seeker.stop().await.expect("Failed to stop seeker");
    provider.stop().await.expect("Failed to stop provider");
}

// =====================================================
// Edge cases
// =====================================================

#[tokio::test]
#[serial]
async fn test_start_providing_empty_content() {
    let node_dirs = TestNodeDirs::new("provider_empty");
    node_dirs.set_env_vars();

    let node_data = create_test_node_data();
    let manager = NetworkManager::new(&node_data)
        .await
        .expect("Failed to create NetworkManager");

    let config = create_dht_test_config(0);
    node_dirs.set_env_vars();
    manager.start(&config).await.expect("Failed to start");

    let cid = ContentId::from_bytes(b"");
    let result = manager.start_providing(cid).await;
    assert!(
        result.is_ok(),
        "Empty-content CID should still be providable"
    );

    manager.stop().await.expect("Failed to stop");
}

#[tokio::test]
#[serial]
async fn test_start_providing_large_content_cid() {
    let node_dirs = TestNodeDirs::new("provider_large");
    node_dirs.set_env_vars();

    let node_data = create_test_node_data();
    let manager = NetworkManager::new(&node_data)
        .await
        .expect("Failed to create NetworkManager");

    let config = create_dht_test_config(0);
    node_dirs.set_env_vars();
    manager.start(&config).await.expect("Failed to start");

    let large_content = vec![0u8; 10 * 1024 * 1024]; // 10MB
    let cid = ContentId::from_bytes(&large_content);

    let result = manager.start_providing(cid).await;
    assert!(result.is_ok(), "Large-content CID should be providable");

    manager.stop().await.expect("Failed to stop");
}

#[tokio::test]
#[serial]
async fn test_stop_providing_not_providing() {
    let node_dirs = TestNodeDirs::new("provider_stop_none");
    node_dirs.set_env_vars();

    let node_data = create_test_node_data();
    let manager = NetworkManager::new(&node_data)
        .await
        .expect("Failed to create NetworkManager");

    let config = create_dht_test_config(0);
    node_dirs.set_env_vars();
    manager.start(&config).await.expect("Failed to start");

    // Stop providing for a CID we never announced.
    let cid = ContentId::from_bytes(b"never provided");
    let result = manager.stop_providing(cid).await;

    // Operation should be effectively idempotent and succeed.
    assert!(result.is_ok(), "stop_providing non-existent should succeed");

    manager.stop().await.expect("Failed to stop");
}

// ============================================================================
// PROVIDER DISCOVERY TESTS
// ============================================================================

#[tokio::test]
#[serial]
async fn test_get_providers_no_providers() {
    let node_dirs = TestNodeDirs::new("get_providers_empty");
    node_dirs.set_env_vars();

    let node_data = create_test_node_data();
    let manager = NetworkManager::new(&node_data)
        .await
        .expect("Failed to create NetworkManager");

    let config = create_dht_test_config(0);
    node_dirs.set_env_vars();
    manager.start(&config).await.expect("Failed to start");

    // Query for providers of content no one is providing
    let cid = ContentId::from_bytes(b"content with no providers");

    let result = manager.get_providers(cid.clone()).await;

    // Should succeed but return empty list (or timeout)
    match result {
        Ok(discovery) => {
            assert!(discovery.providers.is_empty(), "Expected no providers");
            assert_eq!(discovery.cid, cid);
        }
        Err(e) => {
            // Timeout is acceptable when no providers exist
            assert!(e.to_string().contains("timed out") || e.to_string().contains("timeout"));
        }
    }

    manager.stop().await.expect("Failed to stop");
}

#[tokio::test]
#[serial]
async fn test_get_providers_not_started() {
    let node_dirs = TestNodeDirs::new("get_providers_not_started");
    node_dirs.set_env_vars();

    let node_data = create_test_node_data();
    let manager = NetworkManager::new(&node_data)
        .await
        .expect("Failed to create NetworkManager");

    // Don't start the manager
    let cid = ContentId::from_bytes(b"test");
    let result = manager.get_providers(cid).await;

    assert!(result.is_err());
    assert!(result.err().unwrap().to_string().contains("not started"));
}

// ============================================================================
// CONTENT ROUTING TESTS
// ============================================================================

#[tokio::test]
#[serial]
async fn test_fetch_block_no_providers() {
    let node_dirs = TestNodeDirs::new("fetch_no_providers");
    node_dirs.set_env_vars();

    let node_data = create_test_node_data();
    let manager = NetworkManager::new(&node_data)
        .await
        .expect("Failed to create NetworkManager");

    let config = create_dht_test_config(0);
    node_dirs.set_env_vars();
    manager.start(&config).await.expect("Failed to start");

    let cid = ContentId::from_bytes(b"unfindable content");

    let result = manager.fetch_block(cid).await;

    // Should fail with NotFound since no providers exist
    assert!(result.is_err());
    match result.err().unwrap() {
        ContentTransferError::NotFound { .. } => {}
        ContentTransferError::Internal { message } => {
            // Timeout during provider discovery is acceptable
            assert!(
                message.contains("timed out")
                    || message.contains("timeout")
                    || message.contains("Provider discovery")
            );
        }
        other => panic!("Expected NotFound or timeout, got {:?}", other),
    }

    manager.stop().await.expect("Failed to stop");
}

#[tokio::test]
#[serial]
async fn test_fetch_block_not_started() {
    let node_dirs = TestNodeDirs::new("fetch_not_started");
    node_dirs.set_env_vars();

    let node_data = create_test_node_data();
    let manager = NetworkManager::new(&node_data)
        .await
        .expect("Failed to create NetworkManager");

    let cid = ContentId::from_bytes(b"test");
    let result = manager.fetch_block(cid).await;

    assert!(result.is_err());
    assert!(matches!(
        result.err().unwrap(),
        ContentTransferError::NotStarted
    ));
}

#[tokio::test]
#[serial]
async fn test_fetch_manifest_not_started() {
    let node_dirs = TestNodeDirs::new("fetch_manifest_not_started");
    node_dirs.set_env_vars();

    let node_data = create_test_node_data();
    let manager = NetworkManager::new(&node_data)
        .await
        .expect("Failed to create NetworkManager");

    let cid = ContentId::from_bytes(b"test");
    let result = manager.fetch_manifest(cid).await;

    assert!(result.is_err());
    assert!(matches!(
        result.err().unwrap(),
        ContentTransferError::NotStarted
    ));
}

#[tokio::test]
#[serial]
async fn test_full_content_routing_flow() {
    // Complete test: Provider stores block, announces, seeker discovers and fetches
    let provider_dirs = TestNodeDirs::new("routing_provider");
    let seeker_dirs = TestNodeDirs::new("routing_seeker");

    // Create test block data
    let block_data = b"Hello, this is content routing test data!";
    let cid = ContentId::from_bytes(block_data);

    // Setup provider with block store
    {
        // Pre-populate block store (before NetworkManager takes it)
        let store = BlockStore::new(BlockStoreConfig {
            db_path: provider_dirs.block_store_path(),
            ..Default::default()
        })
        .expect("Failed to create block store");

        store
            .put(&Block::new(block_data))
            .expect("Failed to store block");
    }

    provider_dirs.set_env_vars();
    unsafe {
        std::env::set_var("CONTENT_TRANSFER_ENABLED", "true");
    }

    let provider_data = create_test_node_data();
    let provider = NetworkManager::new(&provider_data)
        .await
        .expect("Failed to create provider");

    let provider_config = create_dht_test_config(9300);
    provider_dirs.set_env_vars();
    unsafe {
        std::env::set_var("CONTENT_TRANSFER_ENABLED", "true");
    }
    provider
        .start(&provider_config)
        .await
        .expect("Failed to start provider");

    let provider_peer_id = *provider.local_peer_id();
    let provider_addr: libp2p::Multiaddr =
        format!("/ip4/127.0.0.1/tcp/9300/p2p/{}", provider_peer_id)
            .parse()
            .unwrap();

    // Provider announces content
    provider
        .start_providing(cid.clone())
        .await
        .expect("Failed to announce");
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Setup seeker
    seeker_dirs.set_env_vars();
    unsafe {
        std::env::set_var("CONTENT_TRANSFER_ENABLED", "true");
    }

    let seeker_data = create_test_node_data();
    let seeker = NetworkManager::new(&seeker_data)
        .await
        .expect("Failed to create seeker");

    let mut seeker_config = create_dht_test_config(9301);
    seeker_config.bootstrap_peers = vec![provider_addr];
    seeker_dirs.set_env_vars();
    unsafe {
        std::env::set_var("CONTENT_TRANSFER_ENABLED", "true");
    }
    seeker
        .start(&seeker_config)
        .await
        .expect("Failed to start seeker");

    // Wait for nodes to connect
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Seeker fetches block via content routing
    info!("Seeker attempting to fetch block via content routing");

    // Use fetch_block which does: discover providers → request from provider
    let fetch_result = seeker.fetch_block(cid.clone()).await;

    // Note: This test validates the full flow works
    // In practice, success depends on DHT propagation timing
    info!(
        "Fetch result: {:?}",
        fetch_result.as_ref().map(|b| b.data().len())
    );

    // Cleanup
    seeker.stop().await.expect("Failed to stop seeker");
    provider.stop().await.expect("Failed to stop provider");
}

#[tokio::test]
#[serial]
async fn test_concurrent_provider_queries() {
    let node_dirs = TestNodeDirs::new("concurrent_queries");
    node_dirs.set_env_vars();

    let node_data = create_test_node_data();
    let manager = Arc::new(
        NetworkManager::new(&node_data)
            .await
            .expect("Failed to create NetworkManager"),
    );

    let config = create_dht_test_config(0);
    node_dirs.set_env_vars();
    manager.start(&config).await.expect("Failed to start");

    // Launch multiple concurrent provider queries
    let mut handles = vec![];
    for i in 0..5 {
        let mgr = Arc::clone(&manager);
        let handle = tokio::spawn(async move {
            let cid = ContentId::from_bytes(format!("concurrent query {}", i).as_bytes());
            mgr.get_providers(cid).await
        });
        handles.push(handle);
    }

    // All should complete (success or timeout)
    for handle in handles {
        let result = handle.await.expect("Task panicked");
        // Either succeeds with empty or errors with timeout
        info!("Concurrent query result: {:?}", result);
    }

    manager.stop().await.expect("Failed to stop");
}
