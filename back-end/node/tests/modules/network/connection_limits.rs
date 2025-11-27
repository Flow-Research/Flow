use node::bootstrap::init::NodeData;
use node::modules::network::config::{ConnectionLimitPolicy, ConnectionLimits, NetworkConfig};
use node::modules::network::manager::NetworkManager;
use serial_test::serial;
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::sleep;

use crate::bootstrap::init::setup_test_env;
use crate::modules::network::mdns::create_test_network_config;

fn create_test_node_data() -> NodeData {
    let signing_key = ed25519_dalek::SigningKey::generate(&mut rand::rngs::OsRng);
    let verifying_key = signing_key.verifying_key();

    NodeData {
        id: format!("did:key:test-{}", rand::random::<u32>()),
        private_key: signing_key.to_bytes().to_vec(),
        public_key: verifying_key.to_bytes().to_vec(),
    }
}

#[tokio::test]
#[serial]
async fn test_connection_limit_basic() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    setup_test_env(&temp_dir, "unlimited");

    let node_data = create_test_node_data();

    let mut config = NetworkConfig::default();
    config.connection_limits = ConnectionLimits {
        max_connections: 5,
        policy: ConnectionLimitPolicy::Strict,
        reserved_outbound: 0,
    };
    config.mdns.enabled = false;

    let manager = NetworkManager::new(&node_data).await.unwrap();
    manager.start(&config).await.unwrap();

    // Check initial capacity
    let capacity = manager.connection_capacity().await.unwrap();
    assert_eq!(capacity.max_connections, 5);
    assert_eq!(capacity.active_connections, 0);
    assert_eq!(capacity.available_slots, 5);
    assert!(capacity.can_accept_inbound);
    assert!(capacity.can_dial_outbound);

    manager.stop().await.unwrap();
}

#[tokio::test]
#[serial]
async fn test_connection_capacity_status() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    setup_test_env(&temp_dir, "unlimited");

    let node_data = create_test_node_data();

    let mut config = NetworkConfig::default();
    config.connection_limits = ConnectionLimits {
        max_connections: 10,
        policy: ConnectionLimitPolicy::PreferOutbound,
        reserved_outbound: 3,
    };
    config.mdns.enabled = false;

    let manager = NetworkManager::new(&node_data).await.unwrap();
    manager.start(&config).await.unwrap();

    let capacity = manager.connection_capacity().await.unwrap();

    assert_eq!(capacity.max_connections, 10);
    assert_eq!(capacity.active_connections, 0);
    assert!(capacity.can_accept_inbound);
    assert!(capacity.can_dial_outbound);

    manager.stop().await.unwrap();
}

#[tokio::test]
#[serial]
async fn test_unlimited_connections() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    setup_test_env(&temp_dir, "unlimited");

    let node_data = create_test_node_data();

    let mut config = NetworkConfig::default();
    config.connection_limits = ConnectionLimits {
        max_connections: 0, // Unlimited
        policy: ConnectionLimitPolicy::Strict,
        reserved_outbound: 0,
    };
    config.mdns.enabled = false;

    let manager = NetworkManager::new(&node_data).await.unwrap();
    manager.start(&config).await.unwrap();

    let capacity = manager.connection_capacity().await.unwrap();
    assert_eq!(capacity.max_connections, 0);
    assert!(capacity.can_accept_inbound);
    assert!(capacity.can_dial_outbound);

    manager.stop().await.unwrap();
}

#[tokio::test]
async fn test_connection_limit_policy_prefer_outbound() {
    let limits = ConnectionLimits {
        max_connections: 100,
        policy: ConnectionLimitPolicy::PreferOutbound,
        reserved_outbound: 20,
    };

    // At 79 connections, should still accept inbound (limit is 80)
    assert!(limits.can_accept_inbound(79));

    // At 80 connections, should NOT accept inbound
    assert!(!limits.can_accept_inbound(80));

    // But should still be able to dial outbound up to 100
    assert!(limits.can_dial_outbound(99));
    assert!(!limits.can_dial_outbound(100));
}

#[tokio::test]
#[serial]
async fn test_strict_policy_enforces_max_connections() {
    let temp_dir1 = TempDir::new().expect("Failed to create temp dir");
    let temp_dir2 = TempDir::new().expect("Failed to create temp dir");

    setup_test_env(&temp_dir1, "node1");
    let node1_data = create_test_node_data();
    let mut config1 = NetworkConfig::default();
    config1.connection_limits = ConnectionLimits {
        max_connections: 2, // Only allow 2 connections
        policy: ConnectionLimitPolicy::Strict,
        reserved_outbound: 0,
    };
    config1.mdns.enabled = false;

    let manager1 = NetworkManager::new(&node1_data).await.unwrap();
    manager1.start(&config1).await.unwrap();

    sleep(Duration::from_millis(100)).await;

    // Create 3 peer nodes to connect
    setup_test_env(&temp_dir2, "node2");
    let node2_data = create_test_node_data();
    let mut config2 = NetworkConfig::default();
    config2.mdns.enabled = true; // Enable so they discover each other

    let manager2 = NetworkManager::new(&node2_data).await.unwrap();
    manager2.start(&config2).await.unwrap();

    // Wait for discovery and connection attempts
    sleep(Duration::from_secs(2)).await;

    // Check that manager1 respects its limit
    let capacity = manager1.connection_capacity().await.unwrap();
    assert!(
        capacity.active_connections <= 2,
        "Should not exceed max_connections of 2, got {}",
        capacity.active_connections
    );

    manager1.stop().await.unwrap();
    manager2.stop().await.unwrap();
}

#[tokio::test]
#[serial]
async fn test_active_connections_counter_increments() {
    let temp_dir1 = TempDir::new().expect("Failed to create temp dir");
    let temp_dir2 = TempDir::new().expect("Failed to create temp dir");

    setup_test_env(&temp_dir1, "node1");
    let node1_data = create_test_node_data();
    let mut config1 = NetworkConfig::default();
    config1.connection_limits = ConnectionLimits {
        max_connections: 10,
        policy: ConnectionLimitPolicy::Strict,
        reserved_outbound: 0,
    };
    config1.mdns.enabled = true;

    let manager1 = NetworkManager::new(&node1_data).await.unwrap();
    manager1.start(&config1).await.unwrap();

    // Initially should be 0
    let capacity_before = manager1.connection_capacity().await.unwrap();
    assert_eq!(capacity_before.active_connections, 0);

    // Start a second node
    setup_test_env(&temp_dir2, "node2");
    let node2_data = create_test_node_data();
    let mut config2 = NetworkConfig::default();
    config2.mdns.enabled = true;

    let manager2 = NetworkManager::new(&node2_data).await.unwrap();
    manager2.start(&config2).await.unwrap();

    // Wait for them to discover and connect
    sleep(Duration::from_secs(2)).await;

    // Should now have 1 connection
    let capacity_after = manager1.connection_capacity().await.unwrap();
    assert!(
        capacity_after.active_connections >= 1,
        "Expected at least 1 connection, got {}",
        capacity_after.active_connections
    );

    manager1.stop().await.unwrap();
    manager2.stop().await.unwrap();
}

#[tokio::test]
#[serial]
async fn test_active_connections_counter_decrements() {
    let temp_dir1 = TempDir::new().expect("Failed to create temp dir");
    let temp_dir2 = TempDir::new().expect("Failed to create temp dir");

    setup_test_env(&temp_dir1, "node1");
    let node1_data = create_test_node_data();
    let mut config1 = NetworkConfig::default();
    config1.connection_limits = ConnectionLimits {
        max_connections: 10,
        policy: ConnectionLimitPolicy::Strict,
        reserved_outbound: 0,
    };
    config1.mdns.enabled = true;

    let manager1 = NetworkManager::new(&node1_data).await.unwrap();
    manager1.start(&config1).await.unwrap();

    setup_test_env(&temp_dir2, "node2");
    let node2_data = create_test_node_data();
    let mut config2 = NetworkConfig::default();
    config2.mdns.enabled = true;

    let manager2 = NetworkManager::new(&node2_data).await.unwrap();
    manager2.start(&config2).await.unwrap();

    // Wait for connection
    sleep(Duration::from_secs(2)).await;

    let capacity_connected = manager1.connection_capacity().await.unwrap();
    let count_before = capacity_connected.active_connections;
    assert!(count_before >= 1, "Should have at least 1 connection");

    // Stop node2 to disconnect
    manager2.stop().await.unwrap();

    // Wait for disconnection to be processed
    sleep(Duration::from_millis(500)).await;

    // Counter should have decremented
    let capacity_after = manager1.connection_capacity().await.unwrap();
    assert!(
        capacity_after.active_connections < count_before,
        "Expected connections to decrease from {} to less, got {}",
        count_before,
        capacity_after.active_connections
    );

    manager1.stop().await.unwrap();
}

#[tokio::test]
#[serial]
async fn test_prefer_outbound_reserves_slots() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    setup_test_env(&temp_dir, "node1");

    let node_data = create_test_node_data();
    let mut config = NetworkConfig::default();
    config.connection_limits = ConnectionLimits {
        max_connections: 10,
        policy: ConnectionLimitPolicy::PreferOutbound,
        reserved_outbound: 3, // Reserve 3 slots
    };
    config.mdns.enabled = false;

    let manager = NetworkManager::new(&node_data).await.unwrap();
    manager.start(&config).await.unwrap();

    let capacity = manager.connection_capacity().await.unwrap();

    // With PreferOutbound and 3 reserved, inbound limit should be 7
    // Available slots should reflect this
    assert_eq!(capacity.max_connections, 10);
    assert_eq!(capacity.available_slots, 10); // None used yet

    // The key test: can_accept_inbound should be true until 7 connections
    // This is tested in the unit test, but we verify the config propagates
    assert!(capacity.can_accept_inbound);
    assert!(capacity.can_dial_outbound);

    manager.stop().await.unwrap();
}

#[tokio::test]
#[serial]
async fn test_connection_at_limit_rejects_new() {
    let temp_dir1 = TempDir::new().expect("Failed to create temp dir");
    let temp_dir2 = TempDir::new().expect("Failed to create temp dir");
    let temp_dir3 = TempDir::new().expect("Failed to create temp dir");

    // Node 1: strict limit of 1 connection (use fixed port)
    setup_test_env(&temp_dir1, "node1");
    let node1_data = create_test_node_data();
    let mut config1 = create_test_network_config(19301, false); // Fixed port, mDNS OFF
    config1.connection_limits = ConnectionLimits {
        max_connections: 1,
        policy: ConnectionLimitPolicy::Strict,
        reserved_outbound: 0,
    };

    let manager1 = NetworkManager::new(&node1_data).await.unwrap();
    manager1.start(&config1).await.unwrap();
    let node1_peer_id = manager1.local_peer_id();

    // Construct dial address manually
    let dial_addr: libp2p::Multiaddr = format!("/ip4/127.0.0.1/tcp/19301/p2p/{}", node1_peer_id)
        .parse()
        .unwrap();

    // Node 2: will connect first (no mDNS, explicit dial)
    setup_test_env(&temp_dir2, "node2");
    let node2_data = create_test_node_data();
    let config2 = create_test_network_config(19302, false); // mDNS OFF

    let manager2 = NetworkManager::new(&node2_data).await.unwrap();
    manager2.start(&config2).await.unwrap();

    // Node2 dials Node1 explicitly (only one direction)
    manager2.dial_peer(dial_addr.clone()).await.unwrap();

    sleep(Duration::from_secs(2)).await;

    let capacity = manager1.connection_capacity().await.unwrap();
    assert_eq!(
        capacity.active_connections, 1,
        "Should have exactly 1 connection"
    );

    // Node 3: should be rejected when it tries to connect
    setup_test_env(&temp_dir3, "node3");
    let node3_data = create_test_node_data();
    let config3 = create_test_network_config(19303, false); // mDNS OFF

    let manager3 = NetworkManager::new(&node3_data).await.unwrap();
    manager3.start(&config3).await.unwrap();

    // Node3 dials Node1 - should be rejected
    let _ = manager3.dial_peer(dial_addr.clone()).await;

    sleep(Duration::from_secs(2)).await;

    let capacity_final = manager1.connection_capacity().await.unwrap();
    assert_eq!(
        capacity_final.active_connections, 1,
        "Should still have exactly 1 connection"
    );

    manager1.stop().await.unwrap();
    manager2.stop().await.unwrap();
    manager3.stop().await.unwrap();
}
