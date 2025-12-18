//! Integration tests for NetworkProvider.
//!
//! These tests require a full NetworkManager setup to verify
//! the local-first + network fallback behavior.

use node::modules::network::manager::NetworkManager;
use node::modules::storage::content::{
    Block, BlockStore, BlockStoreConfig, ContentId, ContentProvider, LocalProvider,
    NetworkProvider, NetworkProviderConfig,
};
use serial_test::serial;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;

use crate::bootstrap::init::{
    TwoNodeTestConfig, cleanup_two_node_network, create_test_node_data, setup_test_env,
    setup_two_node_network,
};

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
    let ctx = setup_two_node_network(TwoNodeTestConfig::network_provider()).await;

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

    cleanup_two_node_network(ctx).await;
}

#[tokio::test]
#[serial]
async fn test_network_provider_caching_disabled() {
    let ctx = setup_two_node_network(TwoNodeTestConfig::network_provider()).await;

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

    cleanup_two_node_network(ctx).await;
}

#[tokio::test]
#[serial]
async fn test_network_provider_has_block_local_only() {
    let ctx = setup_two_node_network(TwoNodeTestConfig::network_provider()).await;

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

    cleanup_two_node_network(ctx).await;
}

#[tokio::test]
#[serial]
async fn test_network_provider_has_block_with_network_check() {
    let ctx = setup_two_node_network(TwoNodeTestConfig::network_provider()).await;

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

    cleanup_two_node_network(ctx).await;
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
