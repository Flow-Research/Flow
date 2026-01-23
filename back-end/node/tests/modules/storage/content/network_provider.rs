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

// ============================================================================
// NetworkProvider get_blocks (Batch) Tests
// ============================================================================

#[tokio::test]
#[serial]
async fn test_network_provider_get_blocks_empty() {
    let temp_dir = TempDir::new().unwrap();
    setup_test_env(&temp_dir, "np_batch_empty");

    let store_config = BlockStoreConfig {
        db_path: temp_dir.path().join("blocks"),
        ..Default::default()
    };
    let store = Arc::new(BlockStore::new(store_config).unwrap());

    let node_data = create_test_node_data();
    let network = Arc::new(NetworkManager::new(&node_data).await.unwrap());
    let local = LocalProvider::new(store);
    let provider = NetworkProvider::with_defaults(local, network);

    // Empty batch should return empty results
    let results = provider.get_blocks(&[]).await.unwrap();
    assert!(results.is_empty());
}

#[tokio::test]
#[serial]
async fn test_network_provider_get_blocks_all_local() {
    let temp_dir = TempDir::new().unwrap();
    setup_test_env(&temp_dir, "np_batch_local");

    let store_config = BlockStoreConfig {
        db_path: temp_dir.path().join("blocks"),
        ..Default::default()
    };
    let store = Arc::new(BlockStore::new(store_config).unwrap());

    // Store multiple blocks locally
    let blocks: Vec<(ContentId, Block)> = (0..5)
        .map(|i| {
            let block = Block::new(format!("local batch block {}", i).into_bytes());
            let cid = store.put(&block).unwrap();
            (cid, block)
        })
        .collect();

    // Create NetworkProvider (network not started - all should come from local)
    let node_data = create_test_node_data();
    let network = Arc::new(NetworkManager::new(&node_data).await.unwrap());
    let local = LocalProvider::new(store);
    let provider = NetworkProvider::with_defaults(local, network);

    // Batch fetch
    let cids: Vec<ContentId> = blocks.iter().map(|(cid, _)| cid.clone()).collect();
    let results = provider.get_blocks(&cids).await.unwrap();

    assert_eq!(results.len(), 5);
    for (i, result) in results.iter().enumerate() {
        assert!(result.is_some(), "Block {} should be found", i);
        assert_eq!(
            result.as_ref().unwrap().data(),
            blocks[i].1.data(),
            "Block {} data mismatch",
            i
        );
    }
}

#[tokio::test]
#[serial]
async fn test_network_provider_get_blocks_network_not_running() {
    let temp_dir = TempDir::new().unwrap();
    setup_test_env(&temp_dir, "np_batch_net_down");

    let store_config = BlockStoreConfig {
        db_path: temp_dir.path().join("blocks"),
        ..Default::default()
    };
    let store = Arc::new(BlockStore::new(store_config).unwrap());

    // One local, one missing
    let local_block = Block::new(b"local only".to_vec());
    let local_cid = store.put(&local_block).unwrap();
    let missing_cid = ContentId::from_bytes(b"missing block");

    // Network not started
    let node_data = create_test_node_data();
    let network = Arc::new(NetworkManager::new(&node_data).await.unwrap());
    let local = LocalProvider::new(store);
    let provider = NetworkProvider::with_defaults(local, network);

    let results = provider
        .get_blocks(&[local_cid, missing_cid])
        .await
        .unwrap();

    assert_eq!(results.len(), 2);
    assert!(results[0].is_some()); // Found locally
    assert!(results[1].is_none()); // Can't fetch - network not running
}

#[tokio::test]
#[serial]
async fn test_network_provider_get_blocks_from_network() {
    let ctx = setup_two_node_network(TwoNodeTestConfig::network_provider()).await;

    // Provider stores multiple blocks and announces them
    let blocks: Vec<(ContentId, Block)> = (0..3)
        .map(|i| {
            let block = Block::new(format!("network batch block {}", i).into_bytes());
            let cid = ctx.provider_store.put(&block).unwrap();
            (cid, block)
        })
        .collect();

    // Announce all blocks
    for (cid, _) in &blocks {
        ctx.provider_manager
            .start_providing(cid.clone())
            .await
            .expect("Failed to start providing");
    }

    // Wait for DHT propagation
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Seeker creates NetworkProvider and batch fetches
    let seeker_local = LocalProvider::new(ctx.seeker_store.clone());
    let seeker_provider = NetworkProvider::new(
        seeker_local,
        ctx.seeker_manager.clone(),
        NetworkProviderConfig::default(),
    );

    // None should be local initially
    for (cid, _) in &blocks {
        assert!(!ctx.seeker_store.has(cid).unwrap());
    }

    // Batch fetch
    let cids: Vec<ContentId> = blocks.iter().map(|(cid, _)| cid.clone()).collect();
    let results = seeker_provider.get_blocks(&cids).await.unwrap();

    assert_eq!(results.len(), 3);
    for (i, result) in results.iter().enumerate() {
        assert!(result.is_some(), "Block {} should be fetched", i);
        assert_eq!(result.as_ref().unwrap().data(), blocks[i].1.data());
    }

    // All should be cached now
    for (cid, _) in &blocks {
        assert!(ctx.seeker_store.has(cid).unwrap());
    }

    cleanup_two_node_network(ctx).await;
}

#[tokio::test]
#[serial]
async fn test_network_provider_get_blocks_mixed_local_and_network() {
    let ctx = setup_two_node_network(TwoNodeTestConfig::network_provider()).await;

    // Provider stores a block and announces it
    let network_block = Block::new(b"from network".to_vec());
    let network_cid = ctx.provider_store.put(&network_block).unwrap();
    ctx.provider_manager
        .start_providing(network_cid.clone())
        .await
        .expect("Failed to start providing");

    // Seeker stores a block locally
    let local_block = Block::new(b"already local".to_vec());
    let local_cid = ctx.seeker_store.put(&local_block).unwrap();

    // Wait for DHT propagation
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Seeker batch fetches mixed CIDs
    let seeker_local = LocalProvider::new(ctx.seeker_store.clone());
    let seeker_provider = NetworkProvider::new(
        seeker_local,
        ctx.seeker_manager.clone(),
        NetworkProviderConfig::default(),
    );

    let results = seeker_provider
        .get_blocks(&[local_cid.clone(), network_cid.clone()])
        .await
        .unwrap();

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].as_ref().unwrap().data(), b"already local");
    assert_eq!(results[1].as_ref().unwrap().data(), b"from network");

    cleanup_two_node_network(ctx).await;
}

#[tokio::test]
#[serial]
async fn test_network_provider_get_blocks_partial_not_found() {
    let ctx = setup_two_node_network(TwoNodeTestConfig::network_provider()).await;

    // Provider stores one block and announces it
    let existing_block = Block::new(b"exists".to_vec());
    let existing_cid = ctx.provider_store.put(&existing_block).unwrap();
    ctx.provider_manager
        .start_providing(existing_cid.clone())
        .await
        .expect("Failed to start providing");

    // This CID is never provided
    let nonexistent_cid = ContentId::from_bytes(b"does not exist anywhere");

    tokio::time::sleep(Duration::from_secs(2)).await;

    let seeker_local = LocalProvider::new(ctx.seeker_store.clone());
    let seeker_provider = NetworkProvider::new(
        seeker_local,
        ctx.seeker_manager.clone(),
        NetworkProviderConfig::default(),
    );

    // Batch fetch mix of existing and non-existing
    let results = seeker_provider
        .get_blocks(&[existing_cid, nonexistent_cid])
        .await
        .unwrap();

    assert_eq!(results.len(), 2);
    assert!(results[0].is_some()); // Found on network
    assert!(results[1].is_none()); // Not found anywhere

    cleanup_two_node_network(ctx).await;
}

#[tokio::test]
#[serial]
async fn test_network_provider_get_blocks_caching_disabled() {
    let ctx = setup_two_node_network(TwoNodeTestConfig::network_provider()).await;

    // Provider stores blocks and announces them
    let blocks: Vec<(ContentId, Block)> = (0..2)
        .map(|i| {
            let block = Block::new(format!("no cache batch {}", i).into_bytes());
            let cid = ctx.provider_store.put(&block).unwrap();
            (cid, block)
        })
        .collect();

    for (cid, _) in &blocks {
        ctx.provider_manager
            .start_providing(cid.clone())
            .await
            .expect("Failed to start providing");
    }

    tokio::time::sleep(Duration::from_secs(2)).await;

    // Create provider with caching disabled
    let seeker_local = LocalProvider::new(ctx.seeker_store.clone());
    let config = NetworkProviderConfig::without_caching();
    let seeker_provider = NetworkProvider::new(seeker_local, ctx.seeker_manager.clone(), config);

    // Batch fetch
    let cids: Vec<ContentId> = blocks.iter().map(|(cid, _)| cid.clone()).collect();
    let results = seeker_provider.get_blocks(&cids).await.unwrap();

    // Should fetch successfully
    assert_eq!(results.len(), 2);
    assert!(results[0].is_some());
    assert!(results[1].is_some());

    // But NOT cached locally
    for (cid, _) in &blocks {
        assert!(!ctx.seeker_store.has(cid).unwrap());
    }

    cleanup_two_node_network(ctx).await;
}

#[tokio::test]
#[serial]
async fn test_network_provider_get_blocks_order_preserved() {
    let ctx = setup_two_node_network(TwoNodeTestConfig::network_provider()).await;

    // Create blocks with identifiable data
    let blocks: Vec<(ContentId, Block)> = (0..5)
        .map(|i| {
            let block = Block::new(vec![i as u8; 100]); // First byte identifies block
            let cid = ctx.provider_store.put(&block).unwrap();
            (cid, block)
        })
        .collect();

    for (cid, _) in &blocks {
        ctx.provider_manager
            .start_providing(cid.clone())
            .await
            .unwrap();
    }

    tokio::time::sleep(Duration::from_secs(2)).await;

    let seeker_local = LocalProvider::new(ctx.seeker_store.clone());
    let seeker_provider = NetworkProvider::new(
        seeker_local,
        ctx.seeker_manager.clone(),
        NetworkProviderConfig::default(),
    );

    // Request in reverse order
    let cids: Vec<ContentId> = blocks.iter().rev().map(|(cid, _)| cid.clone()).collect();
    let results = seeker_provider.get_blocks(&cids).await.unwrap();

    // Verify results maintain request order (reversed)
    assert_eq!(results.len(), 5);
    for (i, result) in results.iter().enumerate() {
        let expected_idx = 4 - i; // Reversed
        assert_eq!(
            result.as_ref().unwrap().data()[0],
            expected_idx as u8,
            "Order not preserved at index {}",
            i
        );
    }

    cleanup_two_node_network(ctx).await;
}

// ============================================================================
// NetworkProvider Edge Cases
// ============================================================================

#[tokio::test]
#[serial]
async fn test_network_provider_clone() {
    let temp_dir = TempDir::new().unwrap();
    setup_test_env(&temp_dir, "np_clone");

    let store_config = BlockStoreConfig {
        db_path: temp_dir.path().join("blocks"),
        ..Default::default()
    };
    let store = Arc::new(BlockStore::new(store_config).unwrap());

    let block = Block::new(b"clone test".to_vec());
    let cid = store.put(&block).unwrap();

    let node_data = create_test_node_data();
    let network = Arc::new(NetworkManager::new(&node_data).await.unwrap());
    let local = LocalProvider::new(store);
    let provider1 = NetworkProvider::with_defaults(local, network);

    // Clone the provider
    let provider2 = provider1.clone();

    // Both should work
    let result1 = provider1.get_block(&cid).await.unwrap();
    let result2 = provider2.get_block(&cid).await.unwrap();

    assert!(result1.is_some());
    assert!(result2.is_some());
    assert_eq!(result1.unwrap().data(), result2.unwrap().data());
}

#[tokio::test]
#[serial]
async fn test_network_provider_debug_impl() {
    let temp_dir = TempDir::new().unwrap();
    setup_test_env(&temp_dir, "np_debug");

    let store_config = BlockStoreConfig {
        db_path: temp_dir.path().join("blocks"),
        ..Default::default()
    };
    let store = Arc::new(BlockStore::new(store_config).unwrap());

    let node_data = create_test_node_data();
    let network = Arc::new(NetworkManager::new(&node_data).await.unwrap());
    let local = LocalProvider::new(store);
    let provider = NetworkProvider::with_defaults(local, network);

    // Debug should not panic and should contain useful info
    let debug_str = format!("{:?}", provider);
    assert!(debug_str.contains("NetworkProvider"));
    assert!(debug_str.contains("NetworkManager"));
}

#[tokio::test]
#[serial]
async fn test_network_provider_config_accessor() {
    let temp_dir = TempDir::new().unwrap();
    setup_test_env(&temp_dir, "np_config_acc");

    let store_config = BlockStoreConfig {
        db_path: temp_dir.path().join("blocks"),
        ..Default::default()
    };
    let store = Arc::new(BlockStore::new(store_config).unwrap());

    let node_data = create_test_node_data();
    let network = Arc::new(NetworkManager::new(&node_data).await.unwrap());
    let local = LocalProvider::new(store);

    let custom_config = NetworkProviderConfig {
        max_provider_attempts: 7,
        cache_remote_blocks: false,
        max_parallel_fetch: 15,
        ..Default::default()
    };

    let provider = NetworkProvider::new(local, network, custom_config);

    // Verify config is accessible
    assert_eq!(provider.config().max_provider_attempts, 7);
    assert!(!provider.config().cache_remote_blocks);
    assert_eq!(provider.config().max_parallel_fetch, 15);
}

#[tokio::test]
#[serial]
async fn test_network_provider_local_provider_accessor() {
    let temp_dir = TempDir::new().unwrap();
    setup_test_env(&temp_dir, "np_local_acc");

    let store_config = BlockStoreConfig {
        db_path: temp_dir.path().join("blocks"),
        ..Default::default()
    };
    let store = Arc::new(BlockStore::new(store_config).unwrap());

    let block = Block::new(b"accessor test".to_vec());
    let cid = store.put(&block).unwrap();

    let node_data = create_test_node_data();
    let network = Arc::new(NetworkManager::new(&node_data).await.unwrap());
    let local = LocalProvider::new(store.clone());
    let provider = NetworkProvider::with_defaults(local, network);

    // Access local provider directly
    let local_result = provider.local_provider().get_block(&cid).await.unwrap();
    assert!(local_result.is_some());
    assert_eq!(local_result.unwrap().data(), b"accessor test");
}
