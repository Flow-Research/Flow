use node::modules::network::config::{
    BootstrapConfig, ConnectionLimits, MdnsConfig, NetworkConfig,
};
use node::modules::network::gossipsub::GossipSubConfig;
use node::modules::network::manager::NetworkManager;
use serial_test::serial;
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::sleep;

use crate::bootstrap::init::{NETWORK_MANAGER_LOCK, create_test_node_data, setup_test_env_auto};

// Helper to create unique test configs
pub fn create_test_network_config(port: u16, mdns_enabled: bool) -> NetworkConfig {
    NetworkConfig {
        enable_quic: false, // Use TCP for simpler local testing
        listen_port: port,
        bootstrap_peers: vec![],
        mdns: MdnsConfig {
            enabled: mdns_enabled,
            service_name: "_flow-p2p._udp.local".to_string(),
            query_interval_secs: 5,
        },
        ..Default::default()
    }
}

#[tokio::test]
#[serial]
async fn test_mdns_disabled_mode() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    setup_test_env_auto(&temp_dir);

    let node_data = create_test_node_data();
    let config = create_test_network_config(0, false);

    let manager = NetworkManager::new(&node_data).await.unwrap();
    let result = manager.start(&config).await;

    assert!(
        result.is_ok(),
        "Should start successfully with mDNS disabled"
    );

    sleep(Duration::from_millis(500)).await;

    let peer_count = manager.peer_count().await.unwrap();
    assert_eq!(peer_count, 0, "Should have no peers discovered");

    manager.stop().await.unwrap();
}

#[tokio::test]
#[serial]
async fn test_mdns_enabled_single_node() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    setup_test_env_auto(&temp_dir);

    let node_data = create_test_node_data();
    let config = create_test_network_config(0, true);

    let manager = NetworkManager::new(&node_data).await.unwrap();
    manager.start(&config).await.unwrap();

    // Wait for mDNS to advertise
    sleep(Duration::from_secs(2)).await;

    let peer_count = manager.peer_count().await.unwrap();
    assert_eq!(
        peer_count, 0,
        "Single node should not discover itself via mDNS"
    );

    manager.stop().await.unwrap();
}

#[tokio::test]
#[serial]
async fn test_mdns_two_nodes_discover_each_other() {
    // Acquire the global mutex before setting env vars and creating NetworkManager
    let _guard = NETWORK_MANAGER_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());

    let temp_dir1 = TempDir::new().expect("Failed to create temp dir 1");
    let temp_dir2 = TempDir::new().expect("Failed to create temp dir 2");

    // Setup node 1
    setup_test_env_auto(&temp_dir1);

    let node_data1 = create_test_node_data();
    let config1 = create_test_network_config(0, true);

    let manager1 = NetworkManager::new(&node_data1).await.unwrap();
    manager1.start(&config1).await.unwrap();
    let peer_id1 = manager1.local_peer_id();

    // Release lock before the next manager by scoping or dropping guard
    drop(_guard);

    // Wait for node 1 to start advertising
    sleep(Duration::from_secs(1)).await;

    let _guard2 = NETWORK_MANAGER_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());

    // Setup node 2
    setup_test_env_auto(&temp_dir2);

    let node_data2 = create_test_node_data();
    let config2 = create_test_network_config(0, true);

    let manager2 = NetworkManager::new(&node_data2).await.unwrap();
    manager2.start(&config2).await.unwrap();
    let peer_id2 = manager2.local_peer_id();

    drop(_guard2);

    // Poll with retries instead of fixed sleep
    let max_attempts = 20;
    let poll_interval = Duration::from_millis(500);
    let mut discovered = false;

    for attempt in 1..=max_attempts {
        sleep(poll_interval).await;

        let peer_count1 = manager1.peer_count().await.unwrap();
        let peer_count2 = manager2.peer_count().await.unwrap();

        if peer_count1 >= 1 && peer_count2 >= 1 {
            // Verify mutual discovery
            let peers1 = manager1.connected_peers().await.unwrap();
            let peers2 = manager2.connected_peers().await.unwrap();

            let found_node2 = peers1.iter().any(|p| p.peer_id == *peer_id2);
            let found_node1 = peers2.iter().any(|p| p.peer_id == *peer_id1);

            if found_node2 && found_node1 {
                discovered = true;
                tracing::info!(attempt = attempt, "Nodes discovered each other");
                break;
            }
        }

        tracing::debug!(
            attempt = attempt,
            peer_count1 = peer_count1,
            peer_count2 = peer_count2,
            "Waiting for discovery..."
        );
    }

    assert!(
        discovered,
        "Nodes should discover each other within {} seconds",
        max_attempts as f64 * poll_interval.as_secs_f64()
    );

    // Cleanup
    manager1.stop().await.unwrap();
    manager2.stop().await.unwrap();
}

#[tokio::test]
#[serial]
async fn test_mdns_discovered_peers_stored_in_database() {
    let _guard = NETWORK_MANAGER_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());

    let temp_dir1 = TempDir::new().expect("Failed to create temp dir 1");
    let temp_dir2 = TempDir::new().expect("Failed to create temp dir 2");

    // Setup and start node 1
    setup_test_env_auto(&temp_dir1);

    let node_data1 = create_test_node_data();
    let config1 = create_test_network_config(0, true);
    let manager1 = NetworkManager::new(&node_data1).await.unwrap();
    manager1.start(&config1).await.unwrap();

    drop(_guard);

    sleep(Duration::from_secs(1)).await;

    let _guard2 = NETWORK_MANAGER_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());

    // Setup and start node 2
    setup_test_env_auto(&temp_dir2);

    let node_data2 = create_test_node_data();
    let config2 = create_test_network_config(0, true);
    let manager2 = NetworkManager::new(&node_data2).await.unwrap();
    manager2.start(&config2).await.unwrap();
    let _peer_id2 = manager2.local_peer_id();

    drop(_guard2);

    // Wait for discovery
    sleep(Duration::from_secs(10)).await;

    // Stop node 1 to force flush to database
    manager1.stop().await.unwrap();
    drop(manager1);

    let _guard3 = NETWORK_MANAGER_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());

    // Reset environment variables to temp_dir1's paths before restarting
    setup_test_env_auto(&temp_dir1);

    // Restart node 1 and verify peer is loaded from database
    let manager1_restarted = NetworkManager::new(&node_data1).await.unwrap();
    manager1_restarted.start(&config1).await.unwrap();

    drop(_guard3);

    sleep(Duration::from_secs(2)).await;

    // Check if peer was persisted and loaded
    let stats = manager1_restarted.network_stats().await.unwrap();
    // Known peers should include node 2 even if not currently connected
    assert!(
        stats.known_peers >= 1,
        "Restarted node should have loaded known peers from database"
    );

    // Cleanup
    manager1_restarted.stop().await.unwrap();
    manager2.stop().await.unwrap();
}

#[tokio::test]
#[serial]
async fn test_mdns_peer_expiration_does_not_disconnect() {
    // This test verifies that when an mDNS advertisement expires,
    // we don't forcibly disconnect (the connection remains active)

    let temp_dir1 = TempDir::new().expect("Failed to create temp dir 1");
    let temp_dir2 = TempDir::new().expect("Failed to create temp dir 2");

    // Setup nodes (similar to previous test)
    setup_test_env_auto(&temp_dir1);

    let node_data1 = create_test_node_data();
    let config1 = create_test_network_config(0, true);
    let manager1 = NetworkManager::new(&node_data1).await.unwrap();
    manager1.start(&config1).await.unwrap();

    setup_test_env_auto(&temp_dir2);

    let node_data2 = create_test_node_data();
    let config2 = create_test_network_config(0, true);
    let manager2 = NetworkManager::new(&node_data2).await.unwrap();
    manager2.start(&config2).await.unwrap();

    // Wait for discovery and connection
    sleep(Duration::from_secs(10)).await;

    let initial_peer_count = manager1.peer_count().await.unwrap();

    // Even if mDNS expires (which happens after ~2 minutes typically),
    // connections should remain active. We just verify the connection
    // is stable for a reasonable period.
    sleep(Duration::from_secs(5)).await;

    let later_peer_count = manager1.peer_count().await.unwrap();
    assert_eq!(
        initial_peer_count, later_peer_count,
        "Peer count should remain stable"
    );

    // Cleanup
    manager1.stop().await.unwrap();
    manager2.stop().await.unwrap();
}

#[tokio::test]
#[serial]
async fn test_mdns_does_not_dial_self() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    setup_test_env_auto(&temp_dir);

    let node_data = create_test_node_data();
    let config = create_test_network_config(0, true);

    let manager = {
        let _guard = crate::bootstrap::init::NETWORK_MANAGER_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        setup_test_env_auto(&temp_dir);
        NetworkManager::new(&node_data).await.unwrap()
    };
    manager.start(&config).await.unwrap();

    // Wait for mDNS to potentially discover ourselves (shouldn't happen)
    sleep(Duration::from_secs(5)).await;

    let peer_count = manager.peer_count().await.unwrap();
    assert_eq!(peer_count, 0, "Node should not connect to itself");

    let stats = manager.network_stats().await.unwrap();
    assert_eq!(
        stats.total_connections, 0,
        "Should not have attempted self-connection"
    );

    manager.stop().await.unwrap();
}

#[tokio::test]
#[serial]
async fn test_mdns_rate_limiting() {
    // This test ensures we don't repeatedly dial the same peer
    // if discovered multiple times in quick succession

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    setup_test_env_auto(&temp_dir);

    let node_data = create_test_node_data();
    let config = create_test_network_config(0, true);

    let manager = NetworkManager::new(&node_data).await.unwrap();
    manager.start(&config).await.unwrap();

    // In practice, rate limiting is internal to NetworkEventLoop
    // This test verifies the system remains stable under repeated
    // discovery events (which mDNS generates periodically)

    for _ in 0..5 {
        sleep(Duration::from_secs(1)).await;

        // Query stats to ensure system is responsive
        let stats = manager.network_stats().await.unwrap();
        assert_eq!(stats.connected_peers, 0);
    }

    manager.stop().await.unwrap();
}
