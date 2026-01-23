use node::modules::network::config::{
    BootstrapConfig, ConnectionLimits, MdnsConfig, NetworkConfig,
};
use node::modules::network::manager::NetworkManager;
use serial_test::serial;
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::sleep;

use crate::bootstrap::init::{create_test_node_data, setup_test_env};

#[tokio::test]
#[serial]
async fn test_bootstrap_config_defaults() {
    let config = BootstrapConfig::default();

    assert!(config.auto_dial);
    assert!(config.auto_bootstrap);
    assert_eq!(config.startup_delay_ms, 100);
    assert_eq!(config.max_retries, 3);
    assert_eq!(config.retry_delay_base_ms, 1000);
}

#[tokio::test]
#[serial]
async fn test_bootstrap_without_peers() {
    let temp_dir = TempDir::new().unwrap();
    setup_test_env(&temp_dir, "no_peers");

    let node_data = create_test_node_data();
    let config = NetworkConfig {
        enable_quic: true,
        listen_port: 0,
        bootstrap_peers: vec![],
        mdns: MdnsConfig {
            enabled: false,
            ..Default::default()
        },
        ..Default::default()
    };

    let manager = NetworkManager::new(&node_data).await.unwrap();
    manager.start(&config).await.unwrap();

    sleep(Duration::from_millis(200)).await;

    let peer_ids = manager.bootstrap_peer_ids().await.unwrap();
    assert!(peer_ids.is_empty());

    manager.stop().await.unwrap();
}

#[tokio::test]
#[serial]
async fn test_bootstrap_peers_loaded() {
    let temp_dir = TempDir::new().unwrap();
    setup_test_env(&temp_dir, "loaded");

    let node_data = create_test_node_data();

    let fake_peer_id = libp2p::identity::Keypair::generate_ed25519()
        .public()
        .to_peer_id();
    let bootstrap_addr: libp2p::Multiaddr =
        format!("/ip4/192.168.1.100/tcp/4001/p2p/{}", fake_peer_id)
            .parse()
            .unwrap();

    let config = NetworkConfig {
        enable_quic: false,
        listen_port: 0,
        bootstrap_peers: vec![bootstrap_addr],
        mdns: MdnsConfig {
            enabled: false,
            ..Default::default()
        },
        bootstrap: BootstrapConfig {
            auto_dial: false,
            auto_bootstrap: false,
            ..Default::default()
        },
        ..Default::default()
    };

    let manager = NetworkManager::new(&node_data).await.unwrap();
    manager.start(&config).await.unwrap();

    sleep(Duration::from_millis(100)).await;

    let peer_ids = manager.bootstrap_peer_ids().await.unwrap();
    assert_eq!(peer_ids.len(), 1);
    assert_eq!(peer_ids[0], fake_peer_id);

    let stats = manager.network_stats().await.unwrap();
    assert!(stats.known_peers >= 1);

    manager.stop().await.unwrap();
}

#[tokio::test]
#[serial]
async fn test_manual_bootstrap_trigger() {
    let temp_dir = TempDir::new().unwrap();
    setup_test_env(&temp_dir, "manual");

    let node_data = create_test_node_data();
    let config = NetworkConfig {
        enable_quic: true,
        listen_port: 0,
        bootstrap_peers: vec![],
        mdns: MdnsConfig {
            enabled: false,
            ..Default::default()
        },
        bootstrap: BootstrapConfig {
            auto_dial: false,
            auto_bootstrap: false,
            ..Default::default()
        },
        ..Default::default()
    };

    let manager = NetworkManager::new(&node_data).await.unwrap();
    manager.start(&config).await.unwrap();

    let result = manager.trigger_bootstrap().await.unwrap();

    assert_eq!(result.connected_count, 0);
    assert_eq!(result.failed_count, 0);
    assert!(!result.query_initiated);

    manager.stop().await.unwrap();
}

#[tokio::test]
#[serial]
async fn test_bootstrap_invalid_address_filtered() {
    let temp_dir = TempDir::new().unwrap();
    setup_test_env(&temp_dir, "invalid");

    let node_data = create_test_node_data();

    // Address WITHOUT /p2p/ component
    let invalid_addr: libp2p::Multiaddr = "/ip4/192.168.1.1/tcp/4001".parse().unwrap();

    let config = NetworkConfig {
        enable_quic: true,
        listen_port: 0,
        bootstrap_peers: vec![invalid_addr],
        mdns: MdnsConfig {
            enabled: false,
            ..Default::default()
        },
        ..Default::default()
    };

    let manager = NetworkManager::new(&node_data).await.unwrap();
    manager.start(&config).await.unwrap();

    sleep(Duration::from_millis(200)).await;

    let peer_ids = manager.bootstrap_peer_ids().await.unwrap();
    assert!(peer_ids.is_empty());

    manager.stop().await.unwrap();
}

/// Helper to create config for a node that will act as a bootstrap target
fn create_bootstrap_target_config(port: u16) -> NetworkConfig {
    NetworkConfig {
        enable_quic: false, // Use TCP for simpler address construction
        listen_port: port,
        bootstrap_peers: vec![],
        mdns: MdnsConfig {
            enabled: false,
            ..Default::default()
        },
        bootstrap: BootstrapConfig {
            auto_dial: false,
            auto_bootstrap: false,
            ..Default::default()
        },
        ..Default::default()
    }
}

/// Helper to create config for a node that bootstraps from another node
fn create_bootstrap_client_config(bootstrap_addr: libp2p::Multiaddr) -> NetworkConfig {
    NetworkConfig {
        enable_quic: false,
        listen_port: 0,
        bootstrap_peers: vec![bootstrap_addr],
        mdns: MdnsConfig {
            enabled: false,
            ..Default::default()
        },
        bootstrap: BootstrapConfig {
            auto_dial: true,
            auto_bootstrap: true,
            startup_delay_ms: 100,
            ..Default::default()
        },
        ..Default::default()
    }
}

/// Test two-node bootstrap: Node B connects to Node A as a bootstrap peer
#[tokio::test]
#[serial]
async fn test_two_node_bootstrap_connection() {
    let temp_dir1 = TempDir::new().unwrap();
    let temp_dir2 = TempDir::new().unwrap();

    // ===== Setup Node A (bootstrap target) =====
    setup_test_env(&temp_dir1, "bootstrap_node_a");

    let node_a_data = create_test_node_data();
    let node_a_port = 19001; // Fixed port for predictable address
    let config_a = create_bootstrap_target_config(node_a_port);

    let manager_a = NetworkManager::new(&node_a_data).await.unwrap();
    manager_a.start(&config_a).await.unwrap();
    let peer_id_a = *manager_a.local_peer_id();

    // Wait for Node A to start listening
    sleep(Duration::from_millis(500)).await;

    // ===== Setup Node B (bootstrap client) =====
    setup_test_env(&temp_dir2, "bootstrap_node_b");

    // Construct bootstrap address pointing to Node A
    let bootstrap_addr: libp2p::Multiaddr =
        format!("/ip4/127.0.0.1/tcp/{}/p2p/{}", node_a_port, peer_id_a)
            .parse()
            .unwrap();

    let node_b_data = create_test_node_data();
    let config_b = create_bootstrap_client_config(bootstrap_addr);

    let manager_b = NetworkManager::new(&node_b_data).await.unwrap();
    manager_b.start(&config_b).await.unwrap();
    let peer_id_b = *manager_b.local_peer_id();

    // ===== Verify connection =====
    let max_attempts = 20;
    let poll_interval = Duration::from_millis(250);
    let mut connected = false;

    for attempt in 1..=max_attempts {
        sleep(poll_interval).await;

        let peers_a = manager_a.connected_peers().await.unwrap();
        let peers_b = manager_b.connected_peers().await.unwrap();

        let a_sees_b = peers_a.iter().any(|p| p.peer_id == peer_id_b);
        let b_sees_a = peers_b.iter().any(|p| p.peer_id == peer_id_a);

        if a_sees_b && b_sees_a {
            connected = true;
            tracing::info!(
                attempt = attempt,
                "Bootstrap connection established successfully"
            );
            break;
        }

        tracing::debug!(
            attempt = attempt,
            peers_a = peers_a.len(),
            peers_b = peers_b.len(),
            "Waiting for bootstrap connection..."
        );
    }

    assert!(
        connected,
        "Node B should connect to Node A via bootstrap within timeout"
    );

    // Verify bootstrap peer tracking
    let bootstrap_ids = manager_b.bootstrap_peer_ids().await.unwrap();
    assert!(bootstrap_ids.contains(&peer_id_a));

    // Verify Node A has Node B in known peers (inbound connection)
    let stats_a = manager_a.network_stats().await.unwrap();
    assert!(stats_a.connected_peers >= 1);

    // Cleanup
    manager_b.stop().await.unwrap();
    manager_a.stop().await.unwrap();
}

/// Test bootstrap peer connection failure with retry behavior
#[tokio::test]
#[serial]
async fn test_bootstrap_retry_on_connection_failure() {
    let temp_dir = TempDir::new().unwrap();
    setup_test_env(&temp_dir, "retry_failure");

    let node_data = create_test_node_data();

    // Create a fake peer ID for an unreachable bootstrap peer
    let fake_peer_id = libp2p::identity::Keypair::generate_ed25519()
        .public()
        .to_peer_id();

    // Point to an address that will refuse connections (localhost with unlikely port)
    let unreachable_addr: libp2p::Multiaddr =
        format!("/ip4/127.0.0.1/tcp/19999/p2p/{}", fake_peer_id)
            .parse()
            .unwrap();

    let config = NetworkConfig {
        enable_quic: false,
        listen_port: 0,
        bootstrap_peers: vec![unreachable_addr],
        mdns: MdnsConfig {
            enabled: false,
            ..Default::default()
        },
        bootstrap: BootstrapConfig {
            auto_dial: true,
            auto_bootstrap: false, // Disable DHT bootstrap query for cleaner test
            startup_delay_ms: 50,
            max_retries: 2,
            retry_delay_base_ms: 100, // Fast retries for test
        },
        ..Default::default()
    };

    let manager = NetworkManager::new(&node_data).await.unwrap();
    manager.start(&config).await.unwrap();

    // Verify bootstrap peer is tracked
    let bootstrap_ids = manager.bootstrap_peer_ids().await.unwrap();
    assert_eq!(bootstrap_ids.len(), 1);
    assert_eq!(bootstrap_ids[0], fake_peer_id);

    // Wait for initial dial attempt + retries
    // With startup_delay=50ms, retry_interval=5s, and retry_delay_base=100ms:
    // - Initial dial at ~50ms
    // - Retry check at ~5s (first retry, backoff=100ms)
    // - Retry check at ~10s (second retry, backoff=200ms)
    // - Retry check at ~15s (third check, max_retries=2 reached, no more retries)
    //
    // For faster test, we wait 6s to see at least one retry attempted
    sleep(Duration::from_secs(6)).await;

    // Node should NOT be connected (unreachable peer)
    let stats = manager.network_stats().await.unwrap();
    assert_eq!(
        stats.connected_peers, 0,
        "Should have no connections to unreachable peer"
    );

    // Bootstrap peer should still be in known_peers (not removed after failures)
    assert!(
        stats.known_peers >= 1,
        "Bootstrap peer should remain in known_peers after connection failures"
    );

    // Manual trigger should also fail gracefully
    let result = manager.trigger_bootstrap().await.unwrap();
    assert_eq!(result.connected_count, 0);

    manager.stop().await.unwrap();
}

/// Test that bootstrap succeeds even when one peer is unreachable
#[tokio::test]
#[serial]
async fn test_bootstrap_partial_success_with_mixed_peers() {
    let temp_dir1 = TempDir::new().unwrap();
    let temp_dir2 = TempDir::new().unwrap();

    // ===== Setup Node A (reachable bootstrap) =====
    setup_test_env(&temp_dir1, "partial_node_a");

    let node_a_data = create_test_node_data();
    let node_a_port = 19002;
    let config_a = create_bootstrap_target_config(node_a_port);

    let manager_a = NetworkManager::new(&node_a_data).await.unwrap();
    manager_a.start(&config_a).await.unwrap();
    let peer_id_a = *manager_a.local_peer_id();

    sleep(Duration::from_millis(500)).await;

    // ===== Setup Node B with mixed bootstrap peers =====
    setup_test_env(&temp_dir2, "partial_node_b");

    // One reachable, one unreachable bootstrap peer
    let reachable_addr: libp2p::Multiaddr =
        format!("/ip4/127.0.0.1/tcp/{}/p2p/{}", node_a_port, peer_id_a)
            .parse()
            .unwrap();

    let fake_peer_id = libp2p::identity::Keypair::generate_ed25519()
        .public()
        .to_peer_id();
    let unreachable_addr: libp2p::Multiaddr =
        format!("/ip4/127.0.0.1/tcp/19998/p2p/{}", fake_peer_id)
            .parse()
            .unwrap();

    let node_b_data = create_test_node_data();
    let config_b = NetworkConfig {
        enable_quic: false,
        listen_port: 0,
        bootstrap_peers: vec![reachable_addr, unreachable_addr],
        mdns: MdnsConfig {
            enabled: false,
            ..Default::default()
        },
        connection_limits: ConnectionLimits::default(),
        bootstrap: BootstrapConfig {
            auto_dial: true,
            auto_bootstrap: false,
            startup_delay_ms: 100,
            max_retries: 1, // Quick failure for unreachable peer
            retry_delay_base_ms: 100,
        },
        ..Default::default()
    };

    let manager_b = NetworkManager::new(&node_b_data).await.unwrap();
    manager_b.start(&config_b).await.unwrap();

    // Verify both bootstrap peers are tracked
    let bootstrap_ids = manager_b.bootstrap_peer_ids().await.unwrap();
    assert_eq!(bootstrap_ids.len(), 2);

    // Wait for connection to reachable peer
    let max_attempts = 20;
    let poll_interval = Duration::from_millis(250);
    let mut connected_to_a = false;

    for _ in 1..=max_attempts {
        sleep(poll_interval).await;

        let peers_b = manager_b.connected_peers().await.unwrap();
        if peers_b.iter().any(|p| p.peer_id == peer_id_a) {
            connected_to_a = true;
            break;
        }
    }

    assert!(
        connected_to_a,
        "Should connect to reachable bootstrap peer despite unreachable one"
    );

    // Should have exactly 1 connection (only reachable peer)
    let stats = manager_b.network_stats().await.unwrap();
    assert_eq!(stats.connected_peers, 1);

    // Cleanup
    manager_b.stop().await.unwrap();
    manager_a.stop().await.unwrap();
}
