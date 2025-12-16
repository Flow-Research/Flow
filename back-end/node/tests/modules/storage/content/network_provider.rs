//! Integration tests for NetworkProvider.
//!
//! These tests require a full NetworkManager setup to verify
//! the local-first + network fallback behavior.

use node::modules::network::config::NetworkConfig;
use node::modules::network::manager::NetworkManager;
use node::modules::storage::content::{
    Block, BlockStore, BlockStoreConfig, ContentId, ContentProvider, LocalProvider,
    NetworkProvider, NetworkProviderConfig,
};
use serial_test::serial;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;

use crate::bootstrap::init::{create_test_node_data, set_env, setup_test_env};

// ============================================================================
// Test Helpers
// ============================================================================

struct TestContext {
    provider_manager: Arc<NetworkManager>,
    seeker_manager: Arc<NetworkManager>,
    provider_store: Arc<BlockStore>,
    seeker_store: Arc<BlockStore>,
    _provider_dir: TempDir,
    _seeker_dir: TempDir,
}

async fn setup_two_node_test() -> TestContext {
    // Create temp directories
    let provider_dir = TempDir::new().expect("Failed to create provider temp dir");
    let seeker_dir = TempDir::new().expect("Failed to create seeker temp dir");

    // Setup provider node - set env vars BEFORE creating components
    setup_test_env(&provider_dir, "np_provider");

    set_env("CONTENT_TRANSFER_ENABLED", "true");

    // Create provider's BlockStore FIRST
    let provider_store_config = BlockStoreConfig {
        db_path: provider_dir.path().join("blocks"),
        ..Default::default()
    };
    let provider_store =
        Arc::new(BlockStore::new(provider_store_config).expect("Failed to create provider store"));

    // Create provider NetworkManager with INJECTED BlockStore
    let provider_data = create_test_node_data();
    let provider_manager = NetworkManager::with(&provider_data, Some(Arc::clone(&provider_store)))
        .await
        .expect("Failed to create provider manager");
    let provider_manager = Arc::new(provider_manager);

    // Setup seeker node - set env vars BEFORE creating components
    setup_test_env(&seeker_dir, "np_seeker");

    // Create seeker's BlockStore FIRST
    let seeker_store_config = BlockStoreConfig {
        db_path: seeker_dir.path().join("blocks"),
        ..Default::default()
    };
    let seeker_store =
        Arc::new(BlockStore::new(seeker_store_config).expect("Failed to create seeker store"));

    // Create seeker NetworkManager with INJECTED BlockStore
    let seeker_data = create_test_node_data();
    let seeker_manager = NetworkManager::with(&seeker_data, Some(Arc::clone(&seeker_store)))
        .await
        .expect("Failed to create seeker manager");
    let seeker_manager = Arc::new(seeker_manager);

    // Start provider - re-set env vars before start() since seeker's setup_test_env overwrote them
    setup_test_env(&provider_dir, "np_provider");
    let mut provider_config = NetworkConfig::default();
    provider_config.listen_port = 9300;
    provider_config.mdns.enabled = false;
    provider_manager
        .start(&provider_config)
        .await
        .expect("Failed to start provider");

    // Use QUIC address format (matches default NetworkConfig)
    let provider_addr: libp2p::Multiaddr = format!(
        "/ip4/127.0.0.1/udp/9300/quic-v1/p2p/{}",
        provider_manager.local_peer_id()
    )
    .parse()
    .unwrap();

    // Start seeker - re-set env vars before start()
    setup_test_env(&seeker_dir, "np_seeker");
    let mut seeker_config = NetworkConfig::default();
    seeker_config.listen_port = 9301;
    seeker_config.bootstrap_peers = vec![provider_addr];
    seeker_manager
        .start(&seeker_config)
        .await
        .expect("Failed to start seeker");

    // Wait for connection and DHT sync
    tokio::time::sleep(Duration::from_secs(2)).await;

    TestContext {
        provider_manager,
        seeker_manager,
        provider_store,
        seeker_store,
        _provider_dir: provider_dir,
        _seeker_dir: seeker_dir,
    }
}

async fn cleanup_test(ctx: TestContext) {
    drop(ctx.provider_store);
    drop(ctx.seeker_store);
    let _ = ctx.seeker_manager.stop().await;
    let _ = ctx.provider_manager.stop().await;
}

// ============================================================================
// Tests
// ============================================================================

#[tokio::test]
#[serial]
async fn test_network_provider_local_block_found() {
    let temp_dir = TempDir::new().unwrap();

    setup_test_env(&temp_dir, "np_local_found");

    let store_config = BlockStoreConfig {
        db_path: temp_dir.path().join("blocks"),
        ..Default::default()
    };
    let store = Arc::new(BlockStore::new(store_config).unwrap());

    // Put block in local store
    let block = Block::new(b"local block data".to_vec());
    let cid = store.put(&block).unwrap();

    // Create NetworkManager (not started - simulates offline/local-only mode)
    let node_data = create_test_node_data();
    let network = Arc::new(
        NetworkManager::new(&node_data)
            .await
            .expect("Failed to create network manager"),
    );

    // Create NetworkProvider with the unstarted NetworkManager
    let local = LocalProvider::new(store.clone());
    let provider = NetworkProvider::with_defaults(local, network);

    // NetworkProvider should find the block locally
    let result = provider.get_block(&cid).await;
    assert!(result.is_ok());
    let retrieved = result.unwrap().expect("Block should be found locally");
    assert_eq!(retrieved.data(), b"local block data");
}

#[tokio::test]
#[serial]
async fn test_network_provider_local_block_not_found() {
    let temp_dir = TempDir::new().unwrap();

    setup_test_env(&temp_dir, "np_local_not_found");

    let store_config = BlockStoreConfig {
        db_path: temp_dir.path().join("blocks"),
        ..Default::default()
    };
    let store = Arc::new(BlockStore::new(store_config).unwrap());

    // Create NetworkManager (not started)
    let node_data = create_test_node_data();
    let network = Arc::new(
        NetworkManager::new(&node_data)
            .await
            .expect("Failed to create network manager"),
    );

    // Create NetworkProvider
    let local = LocalProvider::new(store);
    let provider = NetworkProvider::with_defaults(local, network);

    // Block doesn't exist locally, and network isn't running
    // NetworkProvider should gracefully return None
    let cid = ContentId::from_bytes(b"nonexistent");
    let result = provider.get_block(&cid).await;
    assert!(result.is_ok());
    assert!(result.unwrap().is_none());
}

#[tokio::test]
#[serial]
async fn test_network_provider_fetch_from_network() {
    let ctx = setup_two_node_test().await;

    // Provider stores a block and announces it
    let block = Block::new(b"network fetched block".to_vec());
    let cid = ctx.provider_store.put(&block).unwrap();
    ctx.provider_manager
        .start_providing(cid.clone())
        .await
        .expect("Failed to start providing");

    // Wait for DHT propagation
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Seeker creates NetworkProvider and fetches
    let seeker_local = LocalProvider::new(ctx.seeker_store.clone());
    let seeker_provider = NetworkProvider::new(
        seeker_local,
        ctx.seeker_manager.clone(),
        NetworkProviderConfig::default(),
    );

    // Block should not be local
    assert!(!ctx.seeker_store.has(&cid).unwrap());

    // Fetch via NetworkProvider
    let result = seeker_provider.get_block(&cid).await;
    assert!(result.is_ok());
    let fetched = result
        .unwrap()
        .expect("Block should be fetched from network");
    assert_eq!(fetched.data(), b"network fetched block");

    // Block should now be cached locally
    assert!(ctx.seeker_store.has(&cid).unwrap());

    cleanup_test(ctx).await;
}

#[tokio::test]
#[serial]
async fn test_network_provider_caching_disabled() {
    let ctx = setup_two_node_test().await;

    // Provider stores a block and announces it
    let block = Block::new(b"no cache block".to_vec());
    let cid = ctx.provider_store.put(&block).unwrap();
    ctx.provider_manager
        .start_providing(cid.clone())
        .await
        .expect("Failed to start providing");

    tokio::time::sleep(Duration::from_secs(2)).await;

    // Seeker creates NetworkProvider with caching disabled
    let seeker_local = LocalProvider::new(ctx.seeker_store.clone());
    let config = NetworkProviderConfig::without_caching();
    let seeker_provider = NetworkProvider::new(seeker_local, ctx.seeker_manager.clone(), config);

    // Fetch - should work but not cache
    let result = seeker_provider.get_block(&cid).await;
    assert!(result.is_ok());
    assert!(result.unwrap().is_some());

    // Block should NOT be cached locally
    assert!(!ctx.seeker_store.has(&cid).unwrap());

    cleanup_test(ctx).await;
}

#[tokio::test]
#[serial]
async fn test_network_provider_has_block_local_only() {
    let ctx = setup_two_node_test().await;

    // Provider stores and announces a block
    let block = Block::new(b"has block test".to_vec());
    let cid = ctx.provider_store.put(&block).unwrap();
    ctx.provider_manager
        .start_providing(cid.clone())
        .await
        .expect("Failed to start providing");

    tokio::time::sleep(Duration::from_secs(1)).await;

    // Seeker with default config (check_network_for_existence = false)
    let seeker_local = LocalProvider::new(ctx.seeker_store.clone());
    let seeker_provider = NetworkProvider::with_defaults(seeker_local, ctx.seeker_manager.clone());

    // has_block should return false (doesn't check network by default)
    let has = seeker_provider.has_block(&cid).await.unwrap();
    assert!(!has);

    cleanup_test(ctx).await;
}

#[tokio::test]
#[serial]
async fn test_network_provider_has_block_with_network_check() {
    let ctx = setup_two_node_test().await;

    // Provider stores and announces a block
    let block = Block::new(b"has block network test".to_vec());
    let cid = ctx.provider_store.put(&block).unwrap();
    ctx.provider_manager
        .start_providing(cid.clone())
        .await
        .expect("Failed to start providing");

    tokio::time::sleep(Duration::from_secs(1)).await;

    // Seeker with network existence checking enabled
    let seeker_local = LocalProvider::new(ctx.seeker_store.clone());
    let config = NetworkProviderConfig {
        check_network_for_existence: true,
        ..Default::default()
    };
    let seeker_provider = NetworkProvider::new(seeker_local, ctx.seeker_manager.clone(), config);

    // has_block should return true (checks DHT)
    let has = seeker_provider.has_block(&cid).await.unwrap();
    assert!(has);

    cleanup_test(ctx).await;
}

#[tokio::test]
#[serial]
async fn test_network_provider_accessors() {
    let temp_dir = TempDir::new().unwrap();

    setup_test_env(&temp_dir, "np_accessors");

    let store_config = BlockStoreConfig {
        db_path: temp_dir.path().join("blocks"),
        ..Default::default()
    };
    let store = Arc::new(BlockStore::new(store_config).unwrap());

    let node_data = create_test_node_data();
    let network = Arc::new(NetworkManager::new(&node_data).await.unwrap());
    let local = LocalProvider::new(store.clone());
    let config = NetworkProviderConfig {
        max_provider_attempts: 5,
        ..Default::default()
    };

    let provider = NetworkProvider::new(local, network.clone(), config);

    // Test accessors
    assert!(Arc::ptr_eq(provider.block_store(), &store));
    assert_eq!(provider.config().max_provider_attempts, 5);
    assert_eq!(
        provider.network_manager().local_peer_id(),
        network.local_peer_id()
    );
}
