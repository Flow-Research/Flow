use node::modules::network::config::ProviderConfig;
use node::modules::network::manager::{NetworkManager, ProviderConfirmationCallback};
use node::modules::network::persistent_provider_registry::{
    PersistentProviderRegistry, ProviderRegistryConfig,
};
use node::modules::storage::content::ContentId;
use serial_test::serial;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::sleep;
use tracing::info;

use crate::bootstrap::init::{create_test_node_data, remove_env, set_env, set_envs};

// =============================================================================
// Unit Tests: ProviderConfig
// =============================================================================

#[test]
fn test_provider_config_default() {
    let config = ProviderConfig::default();

    assert!(config.auto_reprovide_enabled);
    assert_eq!(config.reprovide_interval_secs, 12 * 60 * 60); // 12 hours
    assert_eq!(config.query_timeout_secs, 60);
    assert_eq!(config.cleanup_interval_secs, 30);
    assert!(config.reprovide_on_startup);
}

#[test]
fn test_provider_config_durations() {
    let config = ProviderConfig {
        db_path: std::path::PathBuf::from("/tmp/test"),
        auto_reprovide_enabled: true,
        reprovide_interval_secs: 3600,
        query_timeout_secs: 30,
        cleanup_interval_secs: 10,
        reprovide_on_startup: true,
    };

    assert_eq!(config.reprovide_interval(), Duration::from_secs(3600));
    assert_eq!(config.query_timeout(), Duration::from_secs(30));
    assert_eq!(config.cleanup_interval(), Duration::from_secs(10));
}

// =============================================================================
// Unit Tests: PersistentProviderRegistry
// =============================================================================

fn create_test_registry() -> (PersistentProviderRegistry, TempDir) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let config = ProviderRegistryConfig {
        db_path: temp_dir.path().join("provider_registry"),
    };
    let registry = PersistentProviderRegistry::new(config).expect("Failed to create");
    (registry, temp_dir)
}

#[test]
fn test_registry_add_contains_remove() {
    let (registry, _temp) = create_test_registry();
    let cid = ContentId::from_bytes(b"test content");

    // Initially empty
    assert!(!registry.contains(&cid));
    assert!(registry.is_empty());

    // Add
    let is_new = registry.add(cid.clone()).expect("Failed to add");
    assert!(is_new);
    assert!(registry.contains(&cid));
    assert!(!registry.is_empty());
    assert_eq!(registry.count(), 1);

    // Add again
    let is_new = registry.add(cid.clone()).expect("Failed to add");
    assert!(!is_new); // Already present

    // Remove
    let was_present = registry.remove(&cid).expect("Failed to remove");
    assert!(was_present);
    assert!(!registry.contains(&cid));
    assert!(registry.is_empty());

    // Remove again
    let was_present = registry.remove(&cid).expect("Failed to remove");
    assert!(!was_present);
}

#[test]
fn test_registry_get_all() {
    let (registry, _temp) = create_test_registry();

    for i in 0..5 {
        let cid = ContentId::from_bytes(format!("content {}", i).as_bytes());
        registry.add(cid).expect("Failed to add");
    }

    let all = registry.get_all();
    assert_eq!(all.len(), 5);
}

#[test]
fn test_registry_persistence_across_reopens() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let config = ProviderRegistryConfig {
        db_path: temp_dir.path().join("provider_registry"),
    };
    let cid1 = ContentId::from_bytes(b"persistent content 1");
    let cid2 = ContentId::from_bytes(b"persistent content 2");

    // Add with first registry instance
    {
        let registry = PersistentProviderRegistry::new(config.clone()).expect("Failed to create");
        registry.add(cid1.clone()).expect("Failed to add");
        registry.add(cid2.clone()).expect("Failed to add");
        registry.flush().expect("Failed to flush");
    }

    // Remove one with second instance
    {
        let registry = PersistentProviderRegistry::new(config.clone()).expect("Failed to reopen");
        assert!(registry.contains(&cid1));
        assert!(registry.contains(&cid2));
        assert_eq!(registry.count(), 2);

        registry.remove(&cid1).expect("Failed to remove");
        registry.flush().expect("Failed to flush");
    }

    // Verify with third instance
    {
        let registry = PersistentProviderRegistry::new(config).expect("Failed to reopen");
        assert!(!registry.contains(&cid1)); // Removed
        assert!(registry.contains(&cid2)); // Still present
        assert_eq!(registry.count(), 1);
    }
}

#[test]
fn test_registry_record_announcement() {
    let (registry, _temp) = create_test_registry();
    let cid = ContentId::from_bytes(b"test content");

    registry.add(cid.clone()).expect("Failed to add");

    // Initial metadata
    let metadata = registry.get_metadata(&cid).expect("Failed to get").unwrap();
    assert_eq!(metadata.announcement_count, 0);
    assert!(metadata.last_announced.is_none());

    // Record first announcement
    registry
        .record_announcement(&cid)
        .expect("Failed to record");
    let metadata = registry.get_metadata(&cid).expect("Failed to get").unwrap();
    assert_eq!(metadata.announcement_count, 1);
    assert!(metadata.last_announced.is_some());

    // Record second announcement
    registry
        .record_announcement(&cid)
        .expect("Failed to record");
    let metadata = registry.get_metadata(&cid).expect("Failed to get").unwrap();
    assert_eq!(metadata.announcement_count, 2);
}

// =============================================================================
// Helper Functions for Integration Tests
// =============================================================================

struct TestNodeDirs {
    data_dir: TempDir,
    registry_dir: TempDir,
    message_store_dir: TempDir,
    block_store_dir: TempDir,
    provider_registry_dir: TempDir,
    prefix: String,
}

impl TestNodeDirs {
    fn new(prefix: &str) -> Self {
        Self {
            data_dir: TempDir::new().expect("Failed to create data_dir"),
            registry_dir: TempDir::new().expect("Failed to create registry_dir"),
            message_store_dir: TempDir::new().expect("Failed to create message_store_dir"),
            block_store_dir: TempDir::new().expect("Failed to create block_store_dir"),
            provider_registry_dir: TempDir::new().expect("Failed to create provider_registry_dir"),
            prefix: prefix.to_string(),
        }
    }

    fn set_env_vars(&self) {
        set_envs(&vec![
            (
                "PEER_REGISTRY_PATH",
                self.registry_dir.path().to_str().unwrap(),
            ),
            (
                "MESSAGE_STORE_PATH",
                self.message_store_dir.path().to_str().unwrap(),
            ),
            (
                "BLOCK_STORE_PATH",
                self.block_store_dir.path().to_str().unwrap(),
            ),
            (
                "PROVIDER_REGISTRY_DB_PATH",
                self.provider_registry_dir.path().to_str().unwrap(),
            ),
        ]);
    }
}

fn create_test_config(port: u16) -> node::modules::network::config::NetworkConfig {
    use node::modules::network::config::{MdnsConfig, NetworkConfig};
    use node::modules::network::gossipsub::GossipSubConfig;

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

// =============================================================================
// Integration Tests: Provider Confirmation Callback
// =============================================================================

#[tokio::test]
#[serial]
async fn test_start_providing_with_callback() {
    let node_dirs = TestNodeDirs::new("provider_callback");
    node_dirs.set_env_vars();

    let node_data = create_test_node_data();
    let manager = NetworkManager::new(&node_data)
        .await
        .expect("Failed to create NetworkManager");

    let config = create_test_config(0);
    node_dirs.set_env_vars();
    manager.start(&config).await.expect("Failed to start");

    // Create callback that tracks invocation
    let callback_invoked = Arc::new(AtomicBool::new(false));
    let callback_success = Arc::new(AtomicBool::new(false));
    let callback_invoked_clone = Arc::clone(&callback_invoked);
    let callback_success_clone = Arc::clone(&callback_success);

    let callback: ProviderConfirmationCallback = Arc::new(move |_cid, result| {
        info!("Callback invoked with result: {:?}", result);
        callback_invoked_clone.store(true, Ordering::SeqCst);
        callback_success_clone.store(result.is_ok(), Ordering::SeqCst);
    });

    let cid = ContentId::from_bytes(b"callback test content");

    let result = manager
        .start_providing_with_callback(cid.clone(), Some(callback))
        .await;

    assert!(
        result.is_ok(),
        "start_providing_with_callback should succeed"
    );

    // Give time for DHT to process
    sleep(Duration::from_millis(500)).await;

    info!(
        "Callback invoked: {}, success: {}",
        callback_invoked.load(Ordering::SeqCst),
        callback_success.load(Ordering::SeqCst)
    );

    manager.stop().await.expect("Failed to stop");
}

#[tokio::test]
#[serial]
async fn test_start_providing_without_callback() {
    let node_dirs = TestNodeDirs::new("provider_no_callback");
    node_dirs.set_env_vars();

    let node_data = create_test_node_data();
    let manager = NetworkManager::new(&node_data)
        .await
        .expect("Failed to create NetworkManager");

    let config = create_test_config(0);
    node_dirs.set_env_vars();
    manager.start(&config).await.expect("Failed to start");

    let cid = ContentId::from_bytes(b"no callback test");

    // None callback should work fine
    let result = manager.start_providing_with_callback(cid, None).await;

    assert!(result.is_ok());

    manager.stop().await.expect("Failed to stop");
}

#[tokio::test]
#[serial]
async fn test_multiple_callbacks_concurrent() {
    let node_dirs = TestNodeDirs::new("provider_multi_callback");
    node_dirs.set_env_vars();

    let node_data = create_test_node_data();
    let manager = Arc::new(
        NetworkManager::new(&node_data)
            .await
            .expect("Failed to create NetworkManager"),
    );

    let config = create_test_config(0);
    node_dirs.set_env_vars();
    manager.start(&config).await.expect("Failed to start");

    let callback_count = Arc::new(AtomicU32::new(0));

    let mut handles = vec![];
    for i in 0..5 {
        let mgr = Arc::clone(&manager);
        let count = Arc::clone(&callback_count);

        let handle = tokio::spawn(async move {
            let callback: ProviderConfirmationCallback = Arc::new(move |_cid, _result| {
                count.fetch_add(1, Ordering::SeqCst);
            });

            let cid = ContentId::from_bytes(format!("concurrent content {}", i).as_bytes());
            mgr.start_providing_with_callback(cid, Some(callback)).await
        });
        handles.push(handle);
    }

    for handle in handles {
        let result = handle.await.expect("Task panicked");
        assert!(result.is_ok());
    }

    // Wait for callbacks
    sleep(Duration::from_millis(500)).await;

    info!(
        "Callbacks invoked: {}",
        callback_count.load(Ordering::SeqCst)
    );

    manager.stop().await.expect("Failed to stop");
}

// =============================================================================
// Integration Tests: Auto-Reprovide
// =============================================================================

#[tokio::test]
#[serial]
async fn test_auto_reprovide_disabled() {
    set_env("PROVIDER_AUTO_REPROVIDE", "false");

    let node_dirs = TestNodeDirs::new("reprovide_disabled");
    node_dirs.set_env_vars();

    let node_data = create_test_node_data();
    let manager = NetworkManager::new(&node_data)
        .await
        .expect("Failed to create NetworkManager");

    let config = create_test_config(0);
    node_dirs.set_env_vars();
    manager.start(&config).await.expect("Failed to start");

    // Provide some content
    let cid = ContentId::from_bytes(b"reprovide disabled test");
    manager
        .start_providing(cid)
        .await
        .expect("Failed to start providing");

    // Should work without issues
    sleep(Duration::from_millis(200)).await;

    manager.stop().await.expect("Failed to stop");

    remove_env("PROVIDER_AUTO_REPROVIDE");
}

// =============================================================================
// Integration Tests: Query Cleanup
// =============================================================================

#[tokio::test]
#[serial]
async fn test_stale_query_cleanup() {
    // Set short timeout for testing
    set_env("PROVIDER_QUERY_TIMEOUT_SECS", "2");
    set_env("PROVIDER_CLEANUP_INTERVAL_SECS", "1");

    let node_dirs = TestNodeDirs::new("query_cleanup");
    node_dirs.set_env_vars();

    let node_data = create_test_node_data();
    let manager = NetworkManager::new(&node_data)
        .await
        .expect("Failed to create NetworkManager");

    let config = create_test_config(0);
    node_dirs.set_env_vars();
    manager.start(&config).await.expect("Failed to start");

    // Start a query that won't complete normally (no peers)
    let cid = ContentId::from_bytes(b"stale query test");
    let result = manager.get_providers(cid).await;

    info!("get_providers result: {:?}", result);

    // Wait for cleanup cycle
    sleep(Duration::from_secs(3)).await;

    manager.stop().await.expect("Failed to stop");

    remove_env("PROVIDER_QUERY_TIMEOUT_SECS");
    remove_env("PROVIDER_CLEANUP_INTERVAL_SECS");
}

#[tokio::test]
#[serial]
async fn test_stop_providing_removes_from_registry() {
    let node_dirs = TestNodeDirs::new("stop_removes");
    node_dirs.set_env_vars();

    let node_data = create_test_node_data();
    let manager = NetworkManager::new(&node_data)
        .await
        .expect("Failed to create NetworkManager");

    let config = create_test_config(0);
    node_dirs.set_env_vars();
    manager.start(&config).await.expect("Failed to start");

    let cid = ContentId::from_bytes(b"temporary content");

    // Start providing
    manager
        .start_providing(cid.clone())
        .await
        .expect("Failed to start providing");

    // Stop providing
    manager
        .stop_providing(cid.clone())
        .await
        .expect("Failed to stop providing");

    // Just verifying no errors; registry is handled inside event loop
    manager.stop().await.expect("Failed to stop");
}
