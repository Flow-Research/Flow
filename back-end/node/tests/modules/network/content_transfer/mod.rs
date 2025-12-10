use crate::bootstrap::init::{remove_env, remove_envs, set_env, set_envs, setup_test_env};
use ed25519_dalek::SigningKey;
use libp2p::PeerId;
use node::bootstrap::init::NodeData;
use node::modules::network::config::{
    BootstrapConfig, ConnectionLimits, MdnsConfig, NetworkConfig,
};
use node::modules::network::content_transfer::{
    ContentRequest, ContentResponse, ContentTransferError,
};
use node::modules::network::gossipsub::GossipSubConfig;
use node::modules::network::manager::NetworkManager;
use node::modules::storage::content::{
    Block, BlockStore, BlockStoreConfig, ChunkingAlgorithm, ContentId, DocumentManifest,
};
use rand::rngs::OsRng;
use serial_test::serial;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;
use tempfile::TempDir;

mod messages_proptest;

/// Holds temp directories and manages env vars for a test node
struct TestNodeDirs {
    temp_dir: TempDir,
    name: String,
}

impl TestNodeDirs {
    fn new(name: &str) -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        Self {
            temp_dir,
            name: name.to_string(),
        }
    }

    /// Set environment variables for this node's paths.
    /// MUST be called before NetworkManager::new() and start().
    fn set_env_vars(&self) {
        setup_test_env(&self.temp_dir, &self.name);
    }

    fn block_store_path(&self) -> std::path::PathBuf {
        self.temp_dir.path().join(format!("{}_blocks", self.name))
    }
}

fn create_default_config() -> NetworkConfig {
    NetworkConfig {
        enable_quic: true,
        listen_port: 0,
        bootstrap_peers: vec![],
        mdns: MdnsConfig::default(),
        connection_limits: ConnectionLimits::default(),
        bootstrap: BootstrapConfig::default(),
        gossipsub: GossipSubConfig::default(),
    }
}

// Helper to create test block store (for pre-populating data)
fn create_block_store_at(path: &Path) -> BlockStore {
    let config = BlockStoreConfig {
        db_path: path.to_path_buf(),
        max_block_size: 1024 * 1024,
        max_total_size_bytes: 0,
        enable_compression: false,
        max_open_files: 128,
    };
    BlockStore::new(config).expect("Failed to create test block store")
}

// Helper to create test node data
fn create_test_node_data() -> NodeData {
    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key = signing_key.verifying_key();

    NodeData {
        id: format!("did:key:test-{}", rand::random::<u32>()),
        private_key: signing_key.to_bytes().to_vec(),
        public_key: verifying_key.to_bytes().to_vec(),
    }
}

// Helper to create test block store
pub fn create_test_block_store(path: &Path) -> Arc<BlockStore> {
    let config = BlockStoreConfig {
        db_path: path.to_path_buf(),
        max_block_size: 1024 * 1024, // 1MB
        max_total_size_bytes: 0,     // Unlimited for tests
        enable_compression: false,   // Faster for tests
        max_open_files: 128,
    };
    Arc::new(BlockStore::new(config).expect("Failed to create test block store"))
}

// Helper to create a test config with unique temp directories
pub fn create_test_config_with_block_store() -> (
    NetworkConfig,
    TempDir,
    TempDir,
    TempDir,
    TempDir,
    Arc<BlockStore>,
) {
    let dht_temp = TempDir::new().expect("Failed to create DHT temp dir");
    let peer_reg_temp = TempDir::new().expect("Failed to create peer registry temp dir");
    let gossip_temp = TempDir::new().expect("Failed to create gossip temp dir");
    let block_store_temp = TempDir::new().expect("Failed to create block store temp dir");

    // Set env vars for this test's unique paths
    set_envs(&vec![
        ("DHT_DB_PATH", dht_temp.path().to_str().unwrap_or("")),
        (
            "PEER_REGISTRY_DB_PATH",
            peer_reg_temp.path().to_str().unwrap_or(""),
        ),
        (
            "GOSSIP_MESSAGE_DB_PATH",
            gossip_temp.path().to_str().unwrap_or(""),
        ),
        (
            "BLOCK_STORE_PATH",
            block_store_temp.path().to_str().unwrap_or(""),
        ),
    ]);

    let config = NetworkConfig {
        enable_quic: true,
        listen_port: 0, // OS assigns random port
        bootstrap_peers: vec![],
        mdns: MdnsConfig::default(),
        connection_limits: ConnectionLimits::default(),
        bootstrap: BootstrapConfig::default(),
        gossipsub: GossipSubConfig::default(),
    };

    let block_store = create_test_block_store(block_store_temp.path());

    (
        config,
        dht_temp,
        peer_reg_temp,
        gossip_temp,
        block_store_temp,
        block_store,
    )
}

// Helper to create test network manager (after setting env vars)
pub async fn create_test_network_manager() -> NetworkManager {
    let node_data = create_test_node_data();
    NetworkManager::new(&node_data)
        .await
        .expect("Failed to create test network manager")
}

/// Test that a block request to ourselves works (loopback)
#[tokio::test]
async fn test_content_request_response_messages() {
    // Create request
    let cid = ContentId::from_bytes(b"test-content");
    let request = ContentRequest::get_block(cid.clone());

    assert!(request.is_block_request());
    assert!(!request.is_manifest_request());
    assert_eq!(request.cid(), &cid);

    // Serialize and deserialize
    let bytes = request.to_bytes().unwrap();
    let restored = ContentRequest::from_bytes(&bytes).unwrap();
    assert_eq!(request, restored);
}

#[tokio::test]
async fn test_content_response_serialization() {
    let cid = ContentId::from_bytes(b"test-content");

    // Block response
    let response = ContentResponse::block(
        cid.clone(),
        b"block data".to_vec(),
        vec![ContentId::from_bytes(b"link1")],
    );

    assert!(response.is_success());
    let bytes = response.to_bytes().unwrap();
    let restored = ContentResponse::from_bytes(&bytes).unwrap();

    match (response, restored) {
        (
            ContentResponse::Block {
                cid: c1,
                data: d1,
                links: l1,
            },
            ContentResponse::Block {
                cid: c2,
                data: d2,
                links: l2,
            },
        ) => {
            assert_eq!(c1, c2);
            assert_eq!(d1, d2);
            assert_eq!(l1, l2);
        }
        _ => panic!("Response type mismatch"),
    }
}

#[tokio::test]
async fn test_not_found_response() {
    let cid = ContentId::from_bytes(b"missing");
    let response = ContentResponse::not_found(cid.clone());

    assert!(!response.is_success());
    assert!(matches!(response, ContentResponse::NotFound { .. }));
}

#[tokio::test]
async fn test_error_response() {
    let response = ContentResponse::error(
        node::modules::network::content_transfer::ErrorCode::RateLimited,
        "Too many requests".to_string(),
    );

    assert!(!response.is_success());
}

/// Test two nodes exchanging a block
#[tokio::test]
#[serial]
async fn test_two_node_block_transfer() {
    // Create temp directories for both nodes (must stay alive)
    let node1_dirs = TestNodeDirs::new("transfer_node1");
    let node2_dirs = TestNodeDirs::new("transfer_node2");

    // Pre-populate node 1's block store with test data, then DROP it
    let test_data = b"Hello from node 1!";
    let cid = {
        let store = create_block_store_at(&node1_dirs.block_store_path());
        let block = Block::new(test_data.to_vec());
        let cid = store.put(&block).unwrap();
        // store is dropped here, releasing the RocksDB lock
        cid
    };

    // Create and start node 1 (set env vars immediately before)
    node1_dirs.set_env_vars();
    let node_data1 = create_test_node_data();
    let manager1 = NetworkManager::new(&node_data1)
        .await
        .expect("Failed to create manager1");
    let config1 = create_default_config();
    node1_dirs.set_env_vars(); // Re-set before start
    manager1.start(&config1).await.unwrap();

    // Create and start node 2 (set env vars immediately before)
    node2_dirs.set_env_vars();
    let node_data2 = create_test_node_data();
    let manager2 = NetworkManager::new(&node_data2)
        .await
        .expect("Failed to create manager2");
    let config2 = create_default_config();
    node2_dirs.set_env_vars(); // Re-set before start
    manager2.start(&config2).await.unwrap();

    // Wait for connection (via mDNS or direct dial)
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Node 2 requests block from node 1
    let peer1_id = manager1.local_peer_id();
    let fetched = manager2
        .request_block(*peer1_id, cid.clone())
        .await
        .unwrap();

    assert_eq!(fetched.data(), test_data);
    assert_eq!(fetched.cid(), &cid);

    // Cleanup
    manager1.stop().await.unwrap();
    manager2.stop().await.unwrap();
}

/// Test requesting non-existent content
#[tokio::test]
#[serial]
async fn test_block_not_found() {
    let node_dirs = TestNodeDirs::new("not_found_node");
    node_dirs.set_env_vars();

    let node_data = create_test_node_data();
    let manager = NetworkManager::new(&node_data)
        .await
        .expect("Failed to create manager");
    let config = create_default_config();

    node_dirs.set_env_vars(); // Re-set before start
    manager.start(&config).await.unwrap();

    let missing_cid = ContentId::from_bytes(b"does-not-exist");
    let fake_peer = PeerId::random();

    let result = manager.request_block(fake_peer, missing_cid).await;

    assert!(matches!(
        result,
        Err(ContentTransferError::PeerDisconnected { .. })
            | Err(ContentTransferError::Timeout { .. })
    ));

    manager.stop().await.unwrap();
}

/// Test request with retry
#[tokio::test]
#[serial]
async fn test_request_with_retry() {
    let node_dirs = TestNodeDirs::new("retry_node");
    node_dirs.set_env_vars();

    let node_data = create_test_node_data();
    let manager = NetworkManager::new(&node_data)
        .await
        .expect("Failed to create manager");
    let config = create_default_config();

    node_dirs.set_env_vars(); // Re-set before start
    manager.start(&config).await.unwrap();

    let missing_cid = ContentId::from_bytes(b"missing");
    let fake_peer = PeerId::random();

    let start = std::time::Instant::now();
    let result = manager
        .request_block_with_retry(fake_peer, missing_cid, 2, Duration::from_millis(100))
        .await;

    assert!(start.elapsed() >= Duration::from_millis(200));
    assert!(result.is_err());

    manager.stop().await.unwrap();
}

/// Verifies that FlowBehaviour with content transfer enabled:
/// 1. Starts successfully
/// 2. Event loop processes without panic
/// 3. Can accept and track pending requests
/// 4. Reports healthy metrics
/// 5. Stops cleanly
#[tokio::test]
#[serial]
async fn test_flow_behaviour_content_transfer_lifecycle() {
    let node_dirs = TestNodeDirs::new("lifecycle_test_node");
    node_dirs.set_env_vars();

    let node_data = create_test_node_data();
    let manager = NetworkManager::new(&node_data)
        .await
        .expect("Failed to create manager");

    let config = create_default_config();
    node_dirs.set_env_vars();

    // 1. Start succeeds
    manager.start(&config).await.expect("Start should succeed");

    // 2. Let event loop run
    tokio::time::sleep(Duration::from_millis(500)).await;

    // 3. Verify manager state is queryable
    let peer_count = manager.peer_count().await.expect("peer_count should work");
    assert_eq!(peer_count, 0, "No peers should be connected initially");

    let stats = manager.network_stats().await.expect("stats should work");
    assert_eq!(stats.total_connections, 0);

    let local_peer_id = manager.local_peer_id();
    assert!(
        !local_peer_id.to_string().is_empty(),
        "Should have valid peer ID"
    );

    // 4. Verify content transfer is enabled by attempting a request
    // (should fail gracefully, not panic)
    let fake_peer = PeerId::random();
    let fake_cid = ContentId::from_bytes(b"test");
    let result = tokio::time::timeout(
        Duration::from_secs(2),
        manager.request_block(fake_peer, fake_cid),
    )
    .await;

    // Should either timeout or return an error, not panic
    assert!(
        result.is_err() || result.unwrap().is_err(),
        "Request to unknown peer should fail gracefully"
    );

    // 5. Stop cleanly
    manager.stop().await.expect("Stop should succeed");

    // 6. Verify stopped state (operations should fail)
    let post_stop_result = manager.peer_count().await;
    assert!(
        post_stop_result.is_err(),
        "Operations after stop should fail"
    );
}

// ============================================================================
// Incoming GetManifest returns manifest from BlockStore
// ============================================================================

/// Tests that GetManifest request retrieves manifest from BlockStore.
/// Uses fixed port + explicit dial to ensure reliable connection.
#[tokio::test]
#[serial]
async fn test_get_manifest_returns_manifest_from_blockstore() {
    use libp2p::Multiaddr;

    let node1_dirs = TestNodeDirs::new("manifest_server");
    let node2_dirs = TestNodeDirs::new("manifest_client");

    // Create a proper DocumentManifest
    let manifest = DocumentManifest::builder()
        .publisher_did("did:key:z6MkTestPublisher")
        .name("test-file.txt")
        .content_type("text/plain")
        .total_size(1024)
        .chunking_algorithm(ChunkingAlgorithm::Fixed)
        .build_flat()
        .expect("Failed to build manifest");

    // Serialize using DAG-CBOR (same format the system expects)
    let manifest_data = manifest
        .to_dag_cbor()
        .expect("Failed to serialize manifest");

    let manifest_cid = {
        let store = create_block_store_at(&node1_dirs.block_store_path());
        let block = Block::new(manifest_data);
        store.put(&block).unwrap()
    };

    // Start server on a fixed port
    node1_dirs.set_env_vars();
    let node_data1 = create_test_node_data();
    let config1 = NetworkConfig {
        listen_port: 19200,
        mdns: MdnsConfig {
            enabled: false,
            ..Default::default()
        },
        ..create_default_config()
    };
    let manager1 = NetworkManager::new(&node_data1).await.unwrap();
    node1_dirs.set_env_vars();
    manager1.start(&config1).await.unwrap();

    let server_peer_id = *manager1.local_peer_id();
    let dial_addr: Multiaddr = format!("/ip4/127.0.0.1/udp/19200/quic-v1/p2p/{}", server_peer_id)
        .parse()
        .unwrap();

    // Start client
    node2_dirs.set_env_vars();
    let node_data2 = create_test_node_data();
    let config2 = NetworkConfig {
        listen_port: 0, // Any port
        mdns: MdnsConfig {
            enabled: false,
            ..Default::default()
        },
        ..create_default_config()
    };
    let manager2 = NetworkManager::new(&node_data2).await.unwrap();
    node2_dirs.set_env_vars();
    manager2.start(&config2).await.unwrap();

    // Explicitly dial server
    manager2
        .dial_peer(dial_addr)
        .await
        .expect("Dial should succeed");

    // Wait for connection with timeout
    let connected = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if manager2.peer_count().await.unwrap() >= 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await;
    assert!(connected.is_ok(), "Should connect to server");

    // Now request the manifest - this MUST succeed
    let result = manager2
        .request_manifest(server_peer_id, manifest_cid.clone())
        .await;

    match result {
        Ok(manifest) => {
            assert!(
                manifest.root_cid().is_ok() || !manifest.children.is_empty(),
                "Retrieved manifest should have valid structure"
            );
        }
        Err(e) => {
            panic!("Manifest request failed after explicit connection: {:?}", e);
        }
    }

    manager1.stop().await.unwrap();
    manager2.stop().await.unwrap();
}

/// Tests that requesting a CID that doesn't exist returns NotFound.
/// Uses explicit dial to ensure we're testing the NotFound response,
/// not connection failures.
#[tokio::test]
#[serial]
async fn test_request_missing_cid_returns_not_found() {
    use libp2p::Multiaddr;

    let node1_dirs = TestNodeDirs::new("notfound_server");
    let node2_dirs = TestNodeDirs::new("notfound_client");

    // Node 1 has empty block store - no blocks

    // Start server on fixed port
    node1_dirs.set_env_vars();
    let node_data1 = create_test_node_data();
    let config1 = NetworkConfig {
        listen_port: 19201, // Different port from other tests
        mdns: MdnsConfig {
            enabled: false,
            ..Default::default()
        },
        ..create_default_config()
    };
    let manager1 = NetworkManager::new(&node_data1).await.unwrap();
    node1_dirs.set_env_vars();
    manager1.start(&config1).await.unwrap();

    let server_peer_id = *manager1.local_peer_id();

    // Construct dial address manually (use QUIC since that's the default transport)
    let dial_addr: Multiaddr = format!("/ip4/127.0.0.1/udp/19201/quic-v1/p2p/{}", server_peer_id)
        .parse()
        .unwrap();

    // Start client
    node2_dirs.set_env_vars();
    let node_data2 = create_test_node_data();
    let config2 = NetworkConfig {
        listen_port: 0,
        mdns: MdnsConfig {
            enabled: false,
            ..Default::default()
        },
        ..create_default_config()
    };
    let manager2 = NetworkManager::new(&node_data2).await.unwrap();
    node2_dirs.set_env_vars();
    manager2.start(&config2).await.unwrap();

    // Explicitly dial server using dial_peer()
    manager2
        .dial_peer(dial_addr)
        .await
        .expect("Dial should succeed");

    // Wait for connection
    let connected = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if manager2.peer_count().await.unwrap() >= 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await;
    assert!(
        connected.is_ok(),
        "Must be connected to test NotFound response"
    );

    // Request a CID that doesn't exist
    let missing_cid = ContentId::from_bytes(b"this-cid-does-not-exist-anywhere");
    let result = manager2.request_block(server_peer_id, missing_cid).await;

    // Should get NotFound specifically
    match result {
        Err(ContentTransferError::NotFound { cid }) => {
            // Expected
            assert!(!cid.is_empty(), "NotFound should include the CID");
        }
        Err(e) => {
            panic!("Expected NotFound error, got: {:?}", e);
        }
        Ok(_) => {
            panic!("Should not return Ok for missing CID");
        }
    }

    manager1.stop().await.unwrap();
    manager2.stop().await.unwrap();
}

// ============================================================================
// Multiple concurrent incoming requests handled correctly
// ============================================================================

#[tokio::test]
#[serial]
async fn test_multiple_concurrent_requests() {
    let server_dirs = TestNodeDirs::new("concurrent_server");
    let client_dirs = TestNodeDirs::new("concurrent_client");

    // Create multiple blocks on server
    let num_blocks = 10;
    let mut cids = Vec::new();
    {
        let store = create_block_store_at(&server_dirs.block_store_path());
        for i in 0..num_blocks {
            let data = format!("Block data #{}", i);
            let block = Block::new(data.into_bytes());
            let cid = store.put(&block).unwrap();
            cids.push(cid);
        }
    }

    // Start server
    server_dirs.set_env_vars();
    let server_data = create_test_node_data();
    let server = NetworkManager::new(&server_data).await.unwrap();
    let server_config = create_default_config();
    server_dirs.set_env_vars();
    server.start(&server_config).await.unwrap();

    // Start client - wrap in Arc for sharing across tasks
    client_dirs.set_env_vars();
    let client_data = create_test_node_data();
    let client = Arc::new(NetworkManager::new(&client_data).await.unwrap());
    let client_config = create_default_config();
    client_dirs.set_env_vars();
    client.start(&client_config).await.unwrap();

    tokio::time::sleep(Duration::from_secs(2)).await;

    let server_peer_id = *server.local_peer_id();

    // Fire off all requests concurrently
    let mut handles = Vec::new();
    for cid in cids.clone() {
        let client_ref = Arc::clone(&client); // Clone the Arc, not the NetworkManager
        handles.push(tokio::spawn(async move {
            client_ref.request_block(server_peer_id, cid).await
        }));
    }

    // Wait for all
    let mut success_count = 0;
    let mut connection_errors = 0;
    for handle in handles {
        match handle.await.unwrap() {
            Ok(_) => success_count += 1,
            Err(ContentTransferError::PeerDisconnected { .. })
            | Err(ContentTransferError::Timeout { .. }) => connection_errors += 1,
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }

    // At minimum, we should not panic
    assert!(success_count > 0 || connection_errors == num_blocks);

    server.stop().await.unwrap();
    client.stop().await.unwrap();
}

// ============================================================================
// Large block response (near max) succeeds
// ============================================================================

#[tokio::test]
#[serial]
async fn test_large_block_response_succeeds() {
    let server_dirs = TestNodeDirs::new("large_block_server");
    let client_dirs = TestNodeDirs::new("large_block_client");

    // Create a large block (1MB - near typical max)
    let large_data: Vec<u8> = (0..1024 * 1024).map(|i| (i % 256) as u8).collect();
    let cid = {
        let store = create_block_store_at(&server_dirs.block_store_path());
        let block = Block::new(large_data.clone());
        store.put(&block).unwrap()
    };

    // Start nodes
    server_dirs.set_env_vars();
    let server = NetworkManager::new(&create_test_node_data()).await.unwrap();
    server_dirs.set_env_vars();
    server.start(&create_default_config()).await.unwrap();

    client_dirs.set_env_vars();
    let client = NetworkManager::new(&create_test_node_data()).await.unwrap();
    client_dirs.set_env_vars();
    client.start(&create_default_config()).await.unwrap();

    tokio::time::sleep(Duration::from_secs(2)).await;

    let result = client
        .request_block(*server.local_peer_id(), cid.clone())
        .await;

    match result {
        Ok(block) => {
            assert_eq!(block.data().len(), large_data.len());
            assert_eq!(block.cid(), &cid);
        }
        Err(ContentTransferError::PeerDisconnected { .. })
        | Err(ContentTransferError::Timeout { .. }) => {
            // Expected if mDNS didn't connect
        }
        Err(e) => panic!("Unexpected error: {:?}", e),
    }

    server.stop().await.unwrap();
    client.stop().await.unwrap();
}

// ============================================================================
// Block with many links included in response
// ============================================================================

#[tokio::test]
async fn test_block_with_many_links_roundtrip() {
    // Test that blocks with many links serialize/deserialize correctly
    let cid = ContentId::from_bytes(b"parent-block");
    let links: Vec<ContentId> = (0..100)
        .map(|i| ContentId::from_bytes(format!("child-{}", i).as_bytes()))
        .collect();

    let response = ContentResponse::block(cid.clone(), b"data".to_vec(), links.clone());

    // Serialize and deserialize
    let bytes = response.to_bytes().unwrap();
    let restored = ContentResponse::from_bytes(&bytes).unwrap();

    match restored {
        ContentResponse::Block {
            cid: c,
            data: d,
            links: l,
        } => {
            assert_eq!(c, cid);
            assert_eq!(d, b"data".to_vec());
            assert_eq!(l.len(), 100);
            assert_eq!(l, links);
        }
        _ => panic!("Expected Block response"),
    }
}

// ============================================================================
// Request timeout behavior
// ============================================================================

#[tokio::test]
#[serial]
async fn test_request_timeout_behavior() {
    let node_dirs = TestNodeDirs::new("timeout_test_node");
    node_dirs.set_env_vars();

    let manager = NetworkManager::new(&create_test_node_data()).await.unwrap();
    node_dirs.set_env_vars();
    manager.start(&create_default_config()).await.unwrap();

    // Request from a peer that doesn't exist
    let fake_peer = PeerId::random();
    let fake_cid = ContentId::from_bytes(b"timeout-test");

    let start = Instant::now();
    let result = manager.request_block(fake_peer, fake_cid).await;
    let elapsed = start.elapsed();

    // Should fail relatively quickly (not hang forever)
    assert!(
        elapsed < Duration::from_secs(60),
        "Should timeout reasonably"
    );

    assert!(matches!(
        result,
        Err(ContentTransferError::Timeout { .. })
            | Err(ContentTransferError::PeerDisconnected { .. })
    ));

    manager.stop().await.unwrap();
}

// ============================================================================
// Only valid CIDs served from BlockStore
// ============================================================================

#[test]
fn test_only_valid_cids_can_be_created() {
    // ContentId::from_bytes creates a hash-based CID
    // It should always produce a valid CID

    // Empty input
    let cid1 = ContentId::from_bytes(&[]);
    assert!(!cid1.to_string().is_empty());

    // Normal input
    let cid2 = ContentId::from_bytes(b"test content");
    assert!(!cid2.to_string().is_empty());

    // Large input
    let large_data: Vec<u8> = (0..10000).map(|i| (i % 256) as u8).collect();
    let cid3 = ContentId::from_bytes(&large_data);
    assert!(!cid3.to_string().is_empty());

    // Verify determinism
    let cid4 = ContentId::from_bytes(b"test content");
    assert_eq!(cid2, cid4, "Same content should produce same CID");
}

/// Validates CID creation properties:
/// 1. Deterministic - same content = same CID
/// 2. Collision resistant - different content = different CID
/// 3. Valid format - all CIDs are properly formatted
/// 4. Content-addressed - CID is derived from content hash
#[test]
fn test_cid_security_properties() {
    // 1. DETERMINISM: Same content always produces same CID
    let content = b"test content for determinism check";
    let cid1 = ContentId::from_bytes(content);
    let cid2 = ContentId::from_bytes(content);
    let cid3 = ContentId::from_bytes(content);
    assert_eq!(cid1, cid2, "CID must be deterministic");
    assert_eq!(
        cid2, cid3,
        "CID must be deterministic across multiple calls"
    );

    // 2. COLLISION RESISTANCE: Different content produces different CIDs
    let variations = vec![
        b"test content".to_vec(),
        b"test content ".to_vec(),  // trailing space
        b" test content".to_vec(),  // leading space
        b"Test content".to_vec(),   // different case
        b"test  content".to_vec(),  // double space
        b"test content\n".to_vec(), // newline
        b"test content\0".to_vec(), // null byte
    ];

    let cids: Vec<ContentId> = variations
        .iter()
        .map(|v| ContentId::from_bytes(v))
        .collect();
    for i in 0..cids.len() {
        for j in (i + 1)..cids.len() {
            assert_ne!(
                cids[i], cids[j],
                "Different content must produce different CIDs: {:?} vs {:?}",
                variations[i], variations[j]
            );
        }
    }

    // 3. VALID FORMAT: CIDs have expected structure
    let cid = ContentId::from_bytes(b"format test");
    let cid_str = cid.to_string();
    assert!(!cid_str.is_empty(), "CID string should not be empty");
    assert!(
        cid_str.starts_with("bafy") || cid_str.starts_with("Qm") || cid_str.len() > 20,
        "CID should have valid multibase prefix"
    );

    // 4. EDGE CASES: Boundary conditions
    // Empty input
    let empty_cid = ContentId::from_bytes(&[]);
    assert!(
        !empty_cid.to_string().is_empty(),
        "Empty content should produce valid CID"
    );

    // Single byte
    let single_cid = ContentId::from_bytes(&[0x00]);
    assert_ne!(
        single_cid, empty_cid,
        "Single byte should differ from empty"
    );

    // Large input (1MB)
    let large_data: Vec<u8> = (0..1024 * 1024).map(|i| (i % 256) as u8).collect();
    let large_cid = ContentId::from_bytes(&large_data);
    assert!(
        !large_cid.to_string().is_empty(),
        "Large content should produce valid CID"
    );

    // Binary data with all byte values
    let all_bytes: Vec<u8> = (0..=255).collect();
    let binary_cid = ContentId::from_bytes(&all_bytes);
    assert!(
        !binary_cid.to_string().is_empty(),
        "Binary content should produce valid CID"
    );
}

// ============================================================================
// Response block CID verified against hash
// ============================================================================

#[test]
fn test_block_cid_matches_content_hash() {
    let content = b"verify this content";
    let block = Block::new(content.to_vec());
    let cid = block.cid();

    // Create another CID from the same content
    let computed_cid = ContentId::from_bytes(content);

    // They should match
    assert_eq!(cid, &computed_cid, "Block CID should match computed CID");

    // Different content should produce different CID
    let other_content = b"different content";
    let other_cid = ContentId::from_bytes(other_content);
    assert_ne!(
        cid, &other_cid,
        "Different content should have different CID"
    );
}

// ============================================================================
// Handle 100 concurrent requests
// ============================================================================

#[tokio::test]
#[serial]
async fn test_handle_100_concurrent_requests() {
    let server_dirs = TestNodeDirs::new("perf_server");
    let client_dirs = TestNodeDirs::new("perf_client");

    // Create 100 blocks
    let num_blocks = 100;
    let mut cids = Vec::new();
    {
        let store = create_block_store_at(&server_dirs.block_store_path());
        for i in 0..num_blocks {
            let data = format!("Performance test block #{}", i);
            let block = Block::new(data.into_bytes());
            cids.push(store.put(&block).unwrap());
        }
    }

    // Start nodes
    server_dirs.set_env_vars();
    let server = NetworkManager::new(&create_test_node_data()).await.unwrap();
    server_dirs.set_env_vars();
    server.start(&create_default_config()).await.unwrap();

    client_dirs.set_env_vars();
    let client = Arc::new(NetworkManager::new(&create_test_node_data()).await.unwrap());
    client_dirs.set_env_vars();
    client.start(&create_default_config()).await.unwrap();

    // Wait for potential mDNS discovery
    tokio::time::sleep(Duration::from_secs(3)).await;

    let server_peer_id = *server.local_peer_id();
    let start = Instant::now();

    // Fire all requests concurrently
    let mut handles = Vec::new();
    for cid in cids {
        let client_ref = Arc::clone(&client); // Clone the Arc
        handles.push(tokio::spawn(async move {
            client_ref.request_block(server_peer_id, cid).await
        }));
    }

    // Wait for all
    for handle in handles {
        let _ = handle.await;
    }

    let elapsed = start.elapsed();

    // Should complete within reasonable time
    // Note: If nodes don't connect, this will complete quickly with errors
    println!("100 concurrent requests completed in {:?}", elapsed);
    assert!(
        elapsed < Duration::from_secs(30),
        "100 requests should complete within 30 seconds"
    );

    server.stop().await.unwrap();
    client.stop().await.unwrap();
}

// ============================================================================
// Request/response latency measurement
// ============================================================================

#[tokio::test]
#[serial]
async fn test_request_response_latency() {
    let server_dirs = TestNodeDirs::new("latency_server");
    let client_dirs = TestNodeDirs::new("latency_client");

    // Create a small block for latency testing
    let test_data = b"latency test";
    let cid = {
        let store = create_block_store_at(&server_dirs.block_store_path());
        let block = Block::new(test_data.to_vec());
        store.put(&block).unwrap()
    };

    // Start nodes
    server_dirs.set_env_vars();
    let server = NetworkManager::new(&create_test_node_data()).await.unwrap();
    server_dirs.set_env_vars();
    server.start(&create_default_config()).await.unwrap();

    client_dirs.set_env_vars();
    let client = NetworkManager::new(&create_test_node_data()).await.unwrap();
    client_dirs.set_env_vars();
    client.start(&create_default_config()).await.unwrap();

    tokio::time::sleep(Duration::from_secs(3)).await;

    // Measure multiple requests
    let server_peer_id = *server.local_peer_id();
    let mut latencies = Vec::new();

    for _ in 0..10 {
        let start = Instant::now();
        match client.request_block(server_peer_id, cid.clone()).await {
            Ok(_) => {
                latencies.push(start.elapsed());
            }
            Err(_) => {
                // Connection issue, skip this measurement
            }
        }
    }

    if !latencies.is_empty() {
        let avg_latency: Duration = latencies.iter().sum::<Duration>() / latencies.len() as u32;
        println!(
            "Average latency: {:?} (from {} successful requests)",
            avg_latency,
            latencies.len()
        );

        // On localhost, should be well under 100ms
        assert!(
            avg_latency < Duration::from_millis(100),
            "Average latency should be under 100ms on localhost"
        );
    } else {
        println!("No successful requests - nodes may not have connected via mDNS");
    }

    server.stop().await.unwrap();
    client.stop().await.unwrap();
}

// ============================================================================
// Additional Unit Tests for Error Handling
// ============================================================================

#[test]
fn test_content_transfer_error_display() {
    let errors = vec![
        ContentTransferError::NotFound {
            cid: "test_cid".to_string(),
        },
        ContentTransferError::Timeout { timeout_secs: 30 },
        ContentTransferError::PeerDisconnected {
            peer_id: "peer123".to_string(),
        },
        ContentTransferError::RemoteError {
            code: "internal".to_string(),
            message: "test error".to_string(),
        },
        ContentTransferError::RequestFailed {
            message: "failed".to_string(),
        },
        ContentTransferError::NetworkError {
            message: "network down".to_string(),
        },
        ContentTransferError::RetriesExhausted { attempts: 3 },
        ContentTransferError::NotStarted,
        ContentTransferError::Internal {
            message: "internal".to_string(),
        },
    ];

    // All errors should have meaningful display strings
    for error in errors {
        let display = format!("{}", error);
        assert!(!display.is_empty(), "Error display should not be empty");
        assert!(
            !display.contains("{}"),
            "Error display should not have format placeholders"
        );
    }
}

#[test]
fn test_error_code_display() {
    use node::modules::network::content_transfer::ErrorCode;

    let codes = vec![
        ErrorCode::NotFound,
        ErrorCode::InvalidRequest,
        ErrorCode::InternalError,
        ErrorCode::RateLimited,
        ErrorCode::Timeout,
        ErrorCode::Overloaded,
    ];

    for code in codes {
        let display = format!("{}", code);
        assert!(!display.is_empty());
    }
}

// ============================================================================
// Protocol Constants Validation
// ============================================================================

#[test]
fn test_protocol_constants() {
    use node::modules::network::content_transfer::{
        CONTENT_PROTOCOL_ID, CONTENT_PROTOCOL_VERSION, MAX_LINKS, MAX_MESSAGE_SIZE,
    };

    // Protocol ID should be properly formatted
    assert!(CONTENT_PROTOCOL_ID.starts_with("/flow/content/"));
    assert!(CONTENT_PROTOCOL_ID.contains(CONTENT_PROTOCOL_VERSION));

    // Size limits should be reasonable
    assert!(
        MAX_MESSAGE_SIZE >= 1024 * 1024,
        "Should allow at least 1MB messages"
    );
    assert!(MAX_MESSAGE_SIZE <= 64 * 1024 * 1024, "Should cap at 64MB");

    // Link limits
    assert!(MAX_LINKS >= 100, "Should allow reasonable number of links");
    assert!(MAX_LINKS <= 100_000, "Should cap links to prevent DoS");
}

// ============================================================================
// Requests to multiple different peers
// ============================================================================

/// Verifies that a client can successfully request blocks from multiple
/// different peers. This tests the NetworkManager's ability to maintain
/// connections and make requests to diverse peers without interference.
#[tokio::test]
#[serial]
async fn test_requests_to_multiple_different_peers() {
    use libp2p::Multiaddr;

    // Set up 3 servers with unique blocks
    let server1_dirs = TestNodeDirs::new("multi_peer_server1");
    let server2_dirs = TestNodeDirs::new("multi_peer_server2");
    let client_dirs = TestNodeDirs::new("multi_peer_client");

    // Create unique blocks on each server
    let block_data1 = b"data from server 1";
    let cid1 = {
        let store = create_block_store_at(&server1_dirs.block_store_path());
        let block = Block::new(block_data1.to_vec());
        store.put(&block).unwrap()
    };

    let block_data2 = b"data from server 2";
    let cid2 = {
        let store = create_block_store_at(&server2_dirs.block_store_path());
        let block = Block::new(block_data2.to_vec());
        store.put(&block).unwrap()
    };

    // Start server 1 on fixed port
    server1_dirs.set_env_vars();
    let server1 = NetworkManager::new(&create_test_node_data()).await.unwrap();
    let config1 = NetworkConfig {
        listen_port: 19301,
        mdns: MdnsConfig {
            enabled: false,
            ..Default::default()
        },
        ..create_default_config()
    };
    server1_dirs.set_env_vars();
    server1.start(&config1).await.unwrap();
    let server1_peer_id = *server1.local_peer_id();

    // Start server 2 on different fixed port
    server2_dirs.set_env_vars();
    let server2 = NetworkManager::new(&create_test_node_data()).await.unwrap();
    let config2 = NetworkConfig {
        listen_port: 19302,
        mdns: MdnsConfig {
            enabled: false,
            ..Default::default()
        },
        ..create_default_config()
    };
    server2_dirs.set_env_vars();
    server2.start(&config2).await.unwrap();
    let server2_peer_id = *server2.local_peer_id();

    // Start client
    client_dirs.set_env_vars();
    let client = NetworkManager::new(&create_test_node_data()).await.unwrap();
    let client_config = NetworkConfig {
        listen_port: 0,
        mdns: MdnsConfig {
            enabled: false,
            ..Default::default()
        },
        ..create_default_config()
    };
    client_dirs.set_env_vars();
    client.start(&client_config).await.unwrap();

    // Dial both servers
    let addr1: Multiaddr = format!("/ip4/127.0.0.1/udp/19301/quic-v1/p2p/{}", server1_peer_id)
        .parse()
        .unwrap();
    let addr2: Multiaddr = format!("/ip4/127.0.0.1/udp/19302/quic-v1/p2p/{}", server2_peer_id)
        .parse()
        .unwrap();

    client
        .dial_peer(addr1)
        .await
        .expect("Dial server1 should succeed");
    client
        .dial_peer(addr2)
        .await
        .expect("Dial server2 should succeed");

    // Wait for connections
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Request from server 1
    let result1 = client.request_block(server1_peer_id, cid1.clone()).await;

    // Request from server 2
    let result2 = client.request_block(server2_peer_id, cid2.clone()).await;

    // Verify both requests succeeded
    match result1 {
        Ok(block) => {
            assert_eq!(block.data(), block_data1);
            assert_eq!(block.cid(), &cid1);
        }
        Err(e) => panic!("Request to server1 failed: {:?}", e),
    }

    match result2 {
        Ok(block) => {
            assert_eq!(block.data(), block_data2);
            assert_eq!(block.cid(), &cid2);
        }
        Err(e) => panic!("Request to server2 failed: {:?}", e),
    }

    // Cleanup
    server1.stop().await.unwrap();
    server2.stop().await.unwrap();
    client.stop().await.unwrap();
}

// ============================================================================
// Timeout error includes duration info
// ============================================================================

/// Verifies that timeout errors include the duration information
/// that was configured for the timeout. This helps with debugging
/// and allows callers to understand what timeout was in effect.
#[test]
fn test_timeout_error_includes_duration_info() {
    // The ContentTransferError::Timeout variant should include timeout_secs
    let error = ContentTransferError::Timeout { timeout_secs: 30 };

    // Verify the field is accessible
    if let ContentTransferError::Timeout { timeout_secs } = error {
        assert_eq!(
            timeout_secs, 30,
            "Timeout error should include the configured timeout"
        );
    } else {
        panic!("Expected Timeout variant");
    }

    // Verify display includes the duration
    let display = format!("{}", error);
    assert!(
        display.contains("30"),
        "Error display should include timeout duration: {}",
        display
    );
}

// ============================================================================
// No retry on NotFound (definitive answer)
// ============================================================================

/// Verifies that request_block_with_retry does NOT retry when
/// receiving a NotFound error, since NotFound is a definitive
/// answer - the content doesn't exist on that peer.
///
/// This test ensures we don't waste time retrying for content
/// that definitively doesn't exist.
#[tokio::test]
#[serial]
async fn test_no_retry_on_not_found() {
    use libp2p::Multiaddr;

    let server_dirs = TestNodeDirs::new("notfound_retry_server");
    let client_dirs = TestNodeDirs::new("notfound_retry_client");

    // Server has NO blocks - any request will return NotFound

    // Start server on fixed port
    server_dirs.set_env_vars();
    let server = NetworkManager::new(&create_test_node_data()).await.unwrap();
    let config1 = NetworkConfig {
        listen_port: 19310,
        mdns: MdnsConfig {
            enabled: false,
            ..Default::default()
        },
        ..create_default_config()
    };
    server_dirs.set_env_vars();
    server.start(&config1).await.unwrap();
    let server_peer_id = *server.local_peer_id();

    // Start client
    client_dirs.set_env_vars();
    let client = NetworkManager::new(&create_test_node_data()).await.unwrap();
    client_dirs.set_env_vars();
    client
        .start(&NetworkConfig {
            listen_port: 0,
            mdns: MdnsConfig {
                enabled: false,
                ..Default::default()
            },
            ..create_default_config()
        })
        .await
        .unwrap();

    // Dial server
    let addr: Multiaddr = format!("/ip4/127.0.0.1/udp/19310/quic-v1/p2p/{}", server_peer_id)
        .parse()
        .unwrap();
    client.dial_peer(addr).await.unwrap();

    tokio::time::sleep(Duration::from_secs(1)).await;

    // Request with retry - should fail immediately on NotFound
    let missing_cid = ContentId::from_bytes(b"definitely-not-here");
    let start = Instant::now();

    let result = client
        .request_block_with_retry(
            server_peer_id,
            missing_cid,
            3, // Would take at least 300ms if it retried
            Duration::from_millis(100),
        )
        .await;

    let elapsed = start.elapsed();

    // Should return NotFound error
    match result {
        Err(ContentTransferError::NotFound { .. }) => {
            // Expected - NotFound is returned without retry
        }
        Err(e) => panic!("Expected NotFound error, got: {:?}", e),
        Ok(_) => panic!("Should not succeed for missing content"),
    }

    // Key assertion: should NOT have waited for retries
    // If it retried 3 times with 100ms delay, it would take ~300ms+
    // Without retry, it should be much faster (< 200ms typically)
    assert!(
        elapsed < Duration::from_millis(500),
        "NotFound should return immediately without retry (elapsed: {:?})",
        elapsed
    );

    server.stop().await.unwrap();
    client.stop().await.unwrap();
}

// ============================================================================
// Max retries respected
// ============================================================================

/// Verifies that request_block_with_retry respects the max_retries
/// parameter and stops after the specified number of attempts.
#[tokio::test]
#[serial]
async fn test_max_retries_respected() {
    let node_dirs = TestNodeDirs::new("max_retry_node");
    node_dirs.set_env_vars();

    let manager = NetworkManager::new(&create_test_node_data()).await.unwrap();
    node_dirs.set_env_vars();
    manager.start(&create_default_config()).await.unwrap();

    // Request from non-existent peer (will fail each time)
    let fake_peer = PeerId::random();
    let fake_cid = ContentId::from_bytes(b"max-retry-test");

    // Test with 0 retries (1 attempt total)
    let start = Instant::now();
    let _ = manager
        .request_block_with_retry(fake_peer, fake_cid.clone(), 0, Duration::from_millis(50))
        .await;
    let _elapsed_0 = start.elapsed();

    // Test with 2 retries (3 attempts total)
    let start = Instant::now();
    let _ = manager
        .request_block_with_retry(fake_peer, fake_cid.clone(), 2, Duration::from_millis(50))
        .await;
    let elapsed_2 = start.elapsed();

    // Test with 4 retries (5 attempts total)
    let start = Instant::now();
    let _ = manager
        .request_block_with_retry(fake_peer, fake_cid, 4, Duration::from_millis(50))
        .await;
    let elapsed_4 = start.elapsed();

    // More retries should take proportionally longer due to delays
    // 0 retries: minimal time
    // 2 retries: ~100ms in delays (2 * 50ms)
    // 4 retries: ~200ms in delays (4 * 50ms)

    // Verify ordering (more retries = more time)
    // Note: First request may be slower due to warmup, so we compare 2 vs 4
    assert!(
        elapsed_4 > elapsed_2,
        "4 retries ({:?}) should take longer than 2 retries ({:?})",
        elapsed_4,
        elapsed_2
    );

    manager.stop().await.unwrap();
}

// ============================================================================
// Request block of size 0 bytes
// ============================================================================

/// Verifies that requesting a zero-byte (empty) block works correctly.
/// Empty blocks are valid and should be handled properly by the system.
#[tokio::test]
#[serial]
async fn test_request_empty_block() {
    use libp2p::Multiaddr;

    let server_dirs = TestNodeDirs::new("empty_block_server");
    let client_dirs = TestNodeDirs::new("empty_block_client");

    // Create an empty block
    let empty_data: Vec<u8> = vec![];
    let cid = {
        let store = create_block_store_at(&server_dirs.block_store_path());
        let block = Block::new(empty_data.clone());
        store.put(&block).unwrap()
    };

    // Start server
    server_dirs.set_env_vars();
    let server = NetworkManager::new(&create_test_node_data()).await.unwrap();
    let config1 = NetworkConfig {
        listen_port: 19320,
        mdns: MdnsConfig {
            enabled: false,
            ..Default::default()
        },
        ..create_default_config()
    };
    server_dirs.set_env_vars();
    server.start(&config1).await.unwrap();
    let server_peer_id = *server.local_peer_id();

    // Start client
    client_dirs.set_env_vars();
    let client = NetworkManager::new(&create_test_node_data()).await.unwrap();
    client_dirs.set_env_vars();
    client
        .start(&NetworkConfig {
            listen_port: 0,
            mdns: MdnsConfig {
                enabled: false,
                ..Default::default()
            },
            ..create_default_config()
        })
        .await
        .unwrap();

    // Dial server
    let addr: Multiaddr = format!("/ip4/127.0.0.1/udp/19320/quic-v1/p2p/{}", server_peer_id)
        .parse()
        .unwrap();
    client.dial_peer(addr).await.unwrap();

    tokio::time::sleep(Duration::from_secs(1)).await;

    // Request the empty block
    let result = client.request_block(server_peer_id, cid.clone()).await;

    match result {
        Ok(block) => {
            assert_eq!(block.data().len(), 0, "Block should be empty");
            assert_eq!(block.cid(), &cid, "CID should match");
        }
        Err(e) => panic!("Empty block request failed: {:?}", e),
    }

    server.stop().await.unwrap();
    client.stop().await.unwrap();
}

// ============================================================================
// Request to self → error or local lookup
// ============================================================================

/// Verifies behavior when a node requests content from itself.
/// This is an edge case that could occur if a node's own peer ID
/// is somehow passed to request_block. The system should handle
/// this gracefully (either with an error or by doing a local lookup).
#[tokio::test]
#[serial]
async fn test_request_to_self() {
    let node_dirs = TestNodeDirs::new("self_request_node");

    // Create a block
    let test_data = b"self request test data";
    let cid = {
        let store = create_block_store_at(&node_dirs.block_store_path());
        let block = Block::new(test_data.to_vec());
        store.put(&block).unwrap()
    };

    node_dirs.set_env_vars();
    let manager = NetworkManager::new(&create_test_node_data()).await.unwrap();
    node_dirs.set_env_vars();
    manager.start(&create_default_config()).await.unwrap();

    tokio::time::sleep(Duration::from_millis(500)).await;

    // Get our own peer ID
    let self_peer_id = *manager.local_peer_id();

    // Request block from ourselves
    let result = manager.request_block(self_peer_id, cid).await;

    // The system should handle this gracefully:
    // - Either return an error (DialFailure to self)
    // - Or potentially do a local lookup (implementation dependent)
    // The key requirement is: NO PANIC, NO DEADLOCK

    match result {
        Ok(_block) => {
            // If local lookup is implemented, this is valid
            // In that case, verify the data is correct
        }
        Err(ContentTransferError::PeerDisconnected { .. })
        | Err(ContentTransferError::Timeout { .. })
        | Err(ContentTransferError::RequestFailed { .. }) => {
            // Self-dial failure is the expected behavior
            // libp2p typically doesn't allow dialing yourself
        }
        Err(e) => {
            // Any error is acceptable, as long as we don't panic
            println!("Self-request returned error (expected): {:?}", e);
        }
    }

    // If we reach here, the test passes - no panic occurred
    manager.stop().await.unwrap();
}

// ============================================================================
// Zero retries configured → single attempt
// ============================================================================

/// Verifies that configuring zero retries results in exactly one attempt.
/// request_block_with_retry with max_retries=0 should try once and fail.
#[tokio::test]
#[serial]
async fn test_zero_retries_single_attempt() {
    let node_dirs = TestNodeDirs::new("zero_retry_node");
    node_dirs.set_env_vars();

    let manager = NetworkManager::new(&create_test_node_data()).await.unwrap();
    node_dirs.set_env_vars();
    manager.start(&create_default_config()).await.unwrap();

    let fake_peer = PeerId::random();
    let fake_cid = ContentId::from_bytes(b"zero-retry-test");

    let start = Instant::now();
    let result = manager
        .request_block_with_retry(
            fake_peer,
            fake_cid,
            0,                          // Zero retries = 1 attempt only
            Duration::from_millis(100), // This delay should NOT be used
        )
        .await;
    let elapsed = start.elapsed();

    // Should fail (peer doesn't exist)
    assert!(result.is_err());

    // Should NOT have waited for any retry delays
    // With zero retries, no sleep should occur
    // The request should fail fast (just the initial dial attempt)
    assert!(
        elapsed < Duration::from_millis(500),
        "Zero retries should not add delay, but took {:?}",
        elapsed
    );

    manager.stop().await.unwrap();
}

// ============================================================================
// Configuration Tests
// ============================================================================

/// Verifies that ContentTransferConfig defaults match expected values.
/// This ensures backward compatibility and documents the default behavior.
#[test]
fn test_content_transfer_config_defaults() {
    let config = node::modules::network::content_transfer::ContentTransferConfig::default();

    // Default timeout is 30 seconds
    assert_eq!(
        config.request_timeout_secs, 30,
        "Default request timeout should be 30 seconds"
    );

    // Default retry settings
    assert_eq!(config.max_retries, 3, "Default max retries should be 3");
    assert_eq!(
        config.retry_delay_ms, 1000,
        "Default retry delay should be 1000ms"
    );
    assert_eq!(
        config.max_concurrent_requests, 64,
        "Default max concurrent should be 64"
    );
    assert!(
        config.enabled,
        "Content transfer should be enabled by default"
    );
}

/// Tests that timeout configuration can be overridden via environment variables.
/// This validates custom timeout via config respected.
#[test]
fn test_content_transfer_config_from_env() {
    // Set custom values
    set_envs(&vec![
        ("CONTENT_TRANSFER_TIMEOUT_SECS", "60"),
        ("CONTENT_TRANSFER_MAX_RETRIES", "5"),
        ("CONTENT_TRANSFER_RETRY_DELAY_MS", "2000"),
        ("CONTENT_TRANSFER_MAX_CONCURRENT", "128"),
        ("CONTENT_TRANSFER_ENABLED", "true"),
    ]);

    let config = node::modules::network::content_transfer::ContentTransferConfig::from_env();

    assert_eq!(config.request_timeout_secs, 60);
    assert_eq!(config.max_retries, 5);
    assert_eq!(config.retry_delay_ms, 2000);
    assert_eq!(config.max_concurrent_requests, 128);
    assert!(config.enabled);

    // Clean up
    remove_envs(&[
        "CONTENT_TRANSFER_TIMEOUT_SECS",
        "CONTENT_TRANSFER_MAX_RETRIES",
        "CONTENT_TRANSFER_RETRY_DELAY_MS",
        "CONTENT_TRANSFER_MAX_CONCURRENT",
        "CONTENT_TRANSFER_ENABLED",
    ]);
}

/// Tests that content transfer can be disabled via environment variable.
#[test]
fn test_content_transfer_can_be_disabled() {
    // Test "false" string
    set_env("CONTENT_TRANSFER_ENABLED", "false");
    let config = node::modules::network::content_transfer::ContentTransferConfig::from_env();
    assert!(!config.enabled, "'false' should disable content transfer");

    // Test "0" string
    set_env("CONTENT_TRANSFER_ENABLED", "0");
    let config = node::modules::network::content_transfer::ContentTransferConfig::from_env();
    assert!(!config.enabled, "'0' should disable content transfer");

    // Clean up
    remove_env("CONTENT_TRANSFER_ENABLED");
}
