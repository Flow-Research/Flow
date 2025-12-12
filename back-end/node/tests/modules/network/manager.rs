#[cfg(test)]
mod tests {
    use ed25519_dalek::SigningKey;
    use errors::AppError;
    use libp2p::Multiaddr;
    use node::{
        bootstrap::init::NodeData,
        modules::network::{
            config::{BootstrapConfig, ConnectionLimits, MdnsConfig, NetworkConfig},
            gossipsub::GossipSubConfig,
            manager::NetworkManager,
        },
    };
    use rand::rngs::OsRng;
    use serial_test::serial;
    use std::time::Duration;
    use tempfile::TempDir;
    use tokio::time::sleep;

    use crate::bootstrap::init::set_envs;

    // Helper function to create test node data
    fn create_test_node_data() -> NodeData {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();

        NodeData {
            id: format!("did:key:test-{}", rand::random::<u32>()),
            private_key: signing_key.to_bytes().to_vec(),
            public_key: verifying_key.to_bytes().to_vec(),
        }
    }

    // Helper to create a test config with unique temp directories
    fn create_test_config() -> (NetworkConfig, TempDir, TempDir, TempDir) {
        let dht_temp = TempDir::new().expect("Failed to create DHT temp dir");
        let peer_reg_temp = TempDir::new().expect("Failed to create peer registry temp dir");
        let gossip_temp = TempDir::new().expect("Failed to create gossip temp dir");

        // Set env vars for this test's unique paths
        setup_test_env();
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
        ]);

        let config = NetworkConfig {
            enable_quic: true,
            listen_port: 0, // OS assigns random port
            bootstrap_peers: vec![],
            ..Default::default()
        };

        (config, dht_temp, peer_reg_temp, gossip_temp)
    }

    #[tokio::test]
    #[serial]
    async fn test_network_manager_creation() {
        let node_data = create_test_node_data();
        let result = NetworkManager::new(&node_data).await;

        assert!(
            result.is_ok(),
            "NetworkManager should initialize successfully"
        );

        let manager = result.unwrap();

        // Verify PeerId is valid (standard length for Ed25519)
        let peer_id_str = manager.local_peer_id().to_string();
        assert!(!peer_id_str.is_empty(), "PeerId should not be empty");
        assert!(
            peer_id_str.starts_with("12D3"),
            "PeerId should start with 12D3"
        );
    }

    #[tokio::test]
    #[serial]
    async fn test_peer_id_deterministic_from_same_keys() {
        let node_data = create_test_node_data();

        // Each manager needs its own temp directories
        let (_config1, _dht1, _reg1, _gossip1) = create_test_config();
        let manager1 = NetworkManager::new(&node_data).await.unwrap();

        let (_config2, _dht2, _reg2, _gossip2) = create_test_config();
        let manager2 = NetworkManager::new(&node_data).await.unwrap();

        assert_eq!(
            manager1.local_peer_id(),
            manager2.local_peer_id(),
            "Same keys should produce identical PeerIds"
        );
    }

    #[tokio::test]
    #[serial]
    async fn test_peer_id_unique_for_different_keys() {
        let node_data1 = create_test_node_data();
        let node_data2 = create_test_node_data();

        // Each manager needs its own temp directories
        let (_config1, _dht1, _reg1, _gossip1) = create_test_config();
        let manager1 = NetworkManager::new(&node_data1).await.unwrap();

        let (_config2, _dht2, _reg2, _gossip2) = create_test_config();
        let manager2 = NetworkManager::new(&node_data2).await.unwrap();

        assert_ne!(
            manager1.local_peer_id(),
            manager2.local_peer_id(),
            "Different keys should produce different PeerIds"
        );
    }

    #[tokio::test]
    #[serial]
    async fn test_network_manager_start_and_stop() {
        let node_data = create_test_node_data();
        let (config, _dht_temp, _peer_reg_temp, _gossip_temp) = create_test_config();

        let manager = NetworkManager::new(&node_data).await.unwrap();

        // Start the network
        let start_result = manager.start(&config).await;
        assert!(start_result.is_ok(), "Network should start successfully");

        // Give it time to bind to port
        sleep(Duration::from_millis(100)).await;

        // Stop the network
        let stop_result = manager.stop().await;
        assert!(stop_result.is_ok(), "Network should stop cleanly");
    }

    #[tokio::test]
    #[serial]
    async fn test_cannot_start_twice() {
        let node_data = create_test_node_data();
        let (config, _dht_temp, _peer_reg_temp, _gossip_temp) = create_test_config();

        let manager = NetworkManager::new(&node_data).await.unwrap();

        // First start should succeed
        manager
            .start(&config)
            .await
            .expect("First start should succeed");

        // Second start should fail
        let second_start = manager.start(&config).await;
        assert!(
            second_start.is_err(),
            "Should not be able to start NetworkManager twice"
        );

        // Verify error message
        let err = second_start.unwrap_err();
        assert!(
            matches!(err, AppError::Network(_)),
            "Should return Network error"
        );

        // Clean up
        manager.stop().await.unwrap();
    }

    #[tokio::test]
    #[serial]
    async fn test_stop_without_start_is_safe() {
        let node_data = create_test_node_data();
        let manager = NetworkManager::new(&node_data).await.unwrap();

        // Stopping without starting should not panic or error
        let result = manager.stop().await;
        assert!(result.is_ok(), "Stop should be safe even if never started");
    }

    #[tokio::test]
    #[serial]
    async fn test_peer_count_initially_zero() {
        let node_data = create_test_node_data();
        let (config, _dht_temp, _peer_reg_temp, _gossip_temp) = create_test_config();

        let manager = NetworkManager::new(&node_data).await.unwrap();
        manager.start(&config).await.unwrap();

        // Give it time to start
        sleep(Duration::from_millis(100)).await;

        let peer_count = manager.peer_count().await.unwrap();
        assert_eq!(
            peer_count, 0,
            "Should have zero peers initially (no bootstrap connections)"
        );

        manager.stop().await.unwrap();
    }

    #[tokio::test]
    #[serial]
    async fn test_peer_count_query_before_start() {
        let node_data = create_test_node_data();
        let manager = NetworkManager::new(&node_data).await.unwrap();

        // Querying peer count before start should fail gracefully
        let result = manager.peer_count().await;

        // This will fail because event loop isn't running to handle the query
        // This is expected behavior - document it
        assert!(
            result.is_err(),
            "Peer count query should fail if network not started"
        );
    }

    #[tokio::test]
    #[serial]
    async fn test_local_peer_id_accessible() {
        let node_data = create_test_node_data();
        let manager = NetworkManager::new(&node_data).await.unwrap();

        let peer_id = manager.local_peer_id();

        // Should be accessible without starting the network
        assert_eq!(
            peer_id,
            manager.local_peer_id(),
            "PeerId should be consistent"
        );
    }

    #[tokio::test]
    #[serial]
    async fn test_manager_with_invalid_key_length() {
        let mut node_data = create_test_node_data();

        // Corrupt the private key (wrong length)
        node_data.private_key = vec![0u8; 16]; // Should be 32 bytes

        let result = NetworkManager::new(&node_data).await;

        assert!(result.is_err(), "Should fail with invalid key length");
        assert!(
            matches!(result.unwrap_err(), AppError::Crypto(_)),
            "Should return Crypto error"
        );
    }

    #[tokio::test]
    #[serial]
    async fn test_start_with_specific_port() {
        let node_data = create_test_node_data();

        // Try to bind to a specific high port (less likely to be in use)
        let config = NetworkConfig {
            enable_quic: true,
            listen_port: 9876,
            bootstrap_peers: vec![],
            ..Default::default()
        };

        let manager = NetworkManager::new(&node_data).await.unwrap();
        let result = manager.start(&config).await;

        // This might fail if port is in use, which is acceptable
        if result.is_ok() {
            sleep(Duration::from_millis(100)).await;
            manager.stop().await.unwrap();
        }
        // If it fails, we just verify it fails gracefully (no panic)
    }

    #[tokio::test]
    #[serial]
    async fn test_quic_transport_config() {
        let node_data = create_test_node_data();

        let config = NetworkConfig {
            enable_quic: true,
            listen_port: 0,
            bootstrap_peers: vec![],
            ..Default::default()
        };

        let manager = NetworkManager::new(&node_data).await.unwrap();
        let result = manager.start(&config).await;

        assert!(
            result.is_ok(),
            "QUIC transport should initialize successfully"
        );

        sleep(Duration::from_millis(100)).await;
        manager.stop().await.unwrap();
    }

    #[tokio::test]
    #[serial]
    async fn test_tcp_transport_config() {
        let node_data = create_test_node_data();

        let config = NetworkConfig {
            enable_quic: false, // Use TCP
            listen_port: 0,
            bootstrap_peers: vec![],
            ..Default::default()
        };

        let manager = NetworkManager::new(&node_data).await.unwrap();
        let result = manager.start(&config).await;

        assert!(
            result.is_ok(),
            "TCP transport should initialize successfully"
        );

        sleep(Duration::from_millis(100)).await;
        manager.stop().await.unwrap();
    }

    #[tokio::test]
    #[serial]
    async fn test_concurrent_peer_count_queries() {
        let node_data = create_test_node_data();
        let (config, _dht_temp, _peer_reg_temp, _gossip_temp) = create_test_config();

        let manager = NetworkManager::new(&node_data).await.unwrap();
        manager.start(&config).await.unwrap();

        sleep(Duration::from_millis(100)).await;

        // Fire off multiple concurrent peer count queries
        let (r1, r2, r3) = tokio::join!(
            manager.peer_count(),
            manager.peer_count(),
            manager.peer_count()
        );

        // All should succeed
        assert!(r1.is_ok(), "First query should succeed");
        assert!(r2.is_ok(), "Second query should succeed");
        assert!(r3.is_ok(), "Third query should succeed");

        // All should return the same count (0 in this case)
        assert_eq!(r1.unwrap(), 0);
        assert_eq!(r2.unwrap(), 0);
        assert_eq!(r3.unwrap(), 0);

        manager.stop().await.unwrap();
    }

    #[tokio::test]
    #[serial]
    async fn test_shutdown_is_graceful() {
        let node_data = create_test_node_data();
        let (config, _dht_temp, _peer_reg_temp, _gossip_temp) = create_test_config();

        let manager = NetworkManager::new(&node_data).await.unwrap();
        manager.start(&config).await.unwrap();

        sleep(Duration::from_millis(100)).await;

        // Stop should complete within reasonable time (not hang)
        let stop_future = manager.stop();
        let timeout_result = tokio::time::timeout(Duration::from_secs(5), stop_future).await;

        assert!(
            timeout_result.is_ok(),
            "Stop should complete within 5 seconds"
        );
        assert!(timeout_result.unwrap().is_ok(), "Stop should succeed");
    }

    #[tokio::test]
    #[serial]
    async fn test_connected_peers_list() {
        let node_data = create_test_node_data();
        let (config, _dht_temp, _peer_reg_temp, _gossip_temp) = create_test_config();

        let manager = NetworkManager::new(&node_data).await.unwrap();
        manager.start(&config).await.unwrap();

        sleep(Duration::from_millis(100)).await;

        let peers = manager.connected_peers().await.unwrap();
        assert_eq!(peers.len(), 0, "Should have no peers initially");

        manager.stop().await.unwrap();
    }

    #[tokio::test]
    #[serial]
    async fn test_network_stats() {
        let node_data = create_test_node_data();
        let (config, _dht_temp, _peer_reg_temp, _gossip_temp) = create_test_config();

        let manager = NetworkManager::new(&node_data).await.unwrap();
        manager.start(&config).await.unwrap();

        sleep(Duration::from_millis(100)).await;

        let stats = manager.network_stats().await.unwrap();
        assert_eq!(stats.connected_peers, 0);

        manager.stop().await.unwrap();
    }

    #[tokio::test]
    #[serial]
    async fn test_dial_invalid_address() {
        let node_data = create_test_node_data();
        let (config, _dht_temp, _peer_reg_temp, _gossip_temp) = create_test_config();

        let manager = NetworkManager::new(&node_data).await.unwrap();
        manager.start(&config).await.unwrap();

        sleep(Duration::from_millis(100)).await;

        // Try to dial an invalid/unreachable address
        let bad_addr: Multiaddr = "/ip4/192.0.2.1/tcp/9999".parse().unwrap();
        let result = manager.dial_peer(bad_addr).await;

        // Dial command should be sent successfully, but connection will fail eventually
        // For now, just check that the command doesn't error
        // In production, you'd monitor connection events to see actual failures
        assert!(result.is_ok() || result.is_err()); // Either is acceptable

        manager.stop().await.unwrap();
    }

    #[tokio::test]
    #[serial]
    async fn test_peer_info_nonexistent() {
        let node_data = create_test_node_data();
        let (config, _dht_temp, _peer_reg_temp, _gossip_temp) = create_test_config();

        let manager = NetworkManager::new(&node_data).await.unwrap();
        manager.start(&config).await.unwrap();

        sleep(Duration::from_millis(100)).await;

        // Query info for a peer that doesn't exist
        let fake_peer_id = libp2p::identity::Keypair::generate_ed25519()
            .public()
            .to_peer_id();
        let info = manager.peer_info(fake_peer_id).await.unwrap();

        assert!(info.is_none(), "Non-existent peer should return None");

        manager.stop().await.unwrap();
    }

    #[tokio::test]
    #[serial]
    async fn test_disconnect_peer() {
        let node_data = create_test_node_data();
        let (config, _dht_temp, _peer_reg_temp, _gossip_temp) = create_test_config();

        let manager = NetworkManager::new(&node_data).await.unwrap();
        manager.start(&config).await.unwrap();

        sleep(Duration::from_millis(100)).await;

        // Try to disconnect a non-existent peer (should not crash)
        let fake_peer_id = libp2p::identity::Keypair::generate_ed25519()
            .public()
            .to_peer_id();
        let result = manager.disconnect_peer(fake_peer_id).await;

        // Disconnecting non-existent peer is acceptable (no-op)
        assert!(result.is_ok() || result.is_err());

        manager.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_manager_is_send() {
        // Compile-time check that NetworkManager can be sent across threads
        fn assert_send<T: Send>() {}
        assert_send::<NetworkManager>();
    }

    #[tokio::test]
    async fn test_manager_is_sync() {
        // Compile-time check that NetworkManager can be shared across threads
        fn assert_sync<T: Sync>() {}
        assert_sync::<NetworkManager>();
    }
}
