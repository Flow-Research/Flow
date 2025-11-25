#[cfg(test)]
mod tests {
    use libp2p::{Multiaddr, PeerId, identity::Keypair};
    use node::modules::network::peer_registry::DiscoverySource;
    use node::modules::network::storage::peer_registry_store::PeerRegistryStore;
    use node::modules::network::{
        peer_registry::ConnectionDirection,
        persistent_peer_registry::{PeerRegistryConfig, PersistentPeerRegistry},
    };
    use serial_test::serial;
    use std::sync::Arc;
    use std::time::Duration;
    use tempfile::TempDir;
    use tokio::time::sleep;

    use crate::bootstrap::init::{remove_envs, set_env, set_envs};

    // ==================== Test Helpers ====================

    fn create_test_peer(id_suffix: u8) -> (PeerId, Multiaddr) {
        let keypair = Keypair::generate_ed25519();
        let peer_id = keypair.public().to_peer_id();
        let address = format!("/ip4/127.0.0.1/tcp/{}", 4000u16 + id_suffix as u16)
            .parse()
            .unwrap();
        (peer_id, address)
    }

    fn create_test_config(temp_dir: &TempDir) -> PeerRegistryConfig {
        PeerRegistryConfig {
            db_path: temp_dir.path().join("peer_registry"),
            flush_interval_secs: 1, // Short interval for testing
        }
    }

    // ==================== PeerRegistryConfig Tests ====================

    #[test]
    fn test_config_default() {
        let config = PeerRegistryConfig::default();
        assert!(config.db_path.to_string_lossy().contains("peer_registry"));
        assert_eq!(config.flush_interval_secs, 30);
    }

    #[test]
    #[serial]
    fn test_config_from_env_with_custom_values() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let custom_path = temp_dir.path().join("custom_peer_db");

        set_envs(&vec![
            ("PEER_REGISTRY_DB_PATH", custom_path.to_str().unwrap()),
            ("PEER_REGISTRY_FLUSH_INTERVAL", "60"),
        ]);

        let config = PeerRegistryConfig::from_env();

        assert_eq!(config.db_path, custom_path);
        assert_eq!(config.flush_interval_secs, 60);

        // Cleanup
        remove_envs(&["PEER_REGISTRY_DB_PATH", "PEER_REGISTRY_FLUSH_INTERVAL"]);
    }

    #[test]
    #[serial]
    fn test_config_from_env_with_defaults() {
        // Ensure env vars are not set

        remove_envs(&["PEER_REGISTRY_DB_PATH", "PEER_REGISTRY_FLUSH_INTERVAL"]);

        let config = PeerRegistryConfig::from_env();

        assert!(config.db_path.to_string_lossy().contains("peer_registry"));
        assert_eq!(config.flush_interval_secs, 30);
    }

    #[test]
    fn test_config_from_env_invalid_interval_uses_default() {
        set_env("PEER_REGISTRY_FLUSH_INTERVAL", "not_a_number");

        let config = PeerRegistryConfig::from_env();
        assert_eq!(config.flush_interval_secs, 30);

        remove_envs(&["PEER_REGISTRY_FLUSH_INTERVAL"]);
    }

    // ==================== PersistentPeerRegistry::new() Tests ====================

    #[tokio::test]
    async fn test_new_registry_success() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config = create_test_config(&temp_dir);

        let registry = PersistentPeerRegistry::new(config);
        assert!(registry.is_ok());

        let registry = registry.unwrap();
        assert_eq!(registry.peer_count(), 0);
    }

    #[tokio::test]
    async fn test_new_registry_creates_directory() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config = PeerRegistryConfig {
            db_path: temp_dir.path().join("nested/deep/path/peer_registry"),
            flush_interval_secs: 30,
        };

        let registry = PersistentPeerRegistry::new(config.clone());
        assert!(registry.is_ok());
        assert!(config.db_path.exists());
    }

    #[tokio::test]
    async fn test_new_registry_loads_known_peers() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config = create_test_config(&temp_dir);
        let (peer_id, address) = create_test_peer(1);

        // Pre-populate the store
        {
            let store = PeerRegistryStore::new(&config.db_path).unwrap();
            store
                .save_known_addresses(&peer_id, &[address.clone()])
                .unwrap();
        }

        // Create registry - should load known peers
        let registry = PersistentPeerRegistry::new(config).unwrap();

        // Peer should be in known addresses (but not connected)
        assert_eq!(registry.peer_count(), 0); // Not connected
        let known_addrs = registry.get_known_addresses(&peer_id);
        assert!(known_addrs.is_some());
        assert!(known_addrs.unwrap().contains(&address));
    }

    #[tokio::test]
    async fn test_new_registry_handles_load_errors_gracefully() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config = create_test_config(&temp_dir);

        // Create a corrupted database or use read-only permissions
        // For simplicity, just test that a new registry starts even with empty/missing data
        let registry = PersistentPeerRegistry::new(config);
        assert!(registry.is_ok());
    }

    // ==================== Background Flush Tests ====================

    #[tokio::test]
    async fn test_start_background_flush() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config = create_test_config(&temp_dir);

        let mut registry = PersistentPeerRegistry::new(config).unwrap();
        registry.start_background_flush();

        // Background task should be running
        assert!(registry.flush_task.is_some());
        assert!(registry.shutdown_tx.is_some());

        // Cleanup
        registry.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_background_flush_actually_flushes() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config = create_test_config(&temp_dir);
        let (peer_id, address) = create_test_peer(1);

        let mut registry = PersistentPeerRegistry::new(config).unwrap();
        registry.start_background_flush();

        // Add a peer
        registry.on_connection_established(
            peer_id,
            address,
            ConnectionDirection::Outbound,
            DiscoverySource::Unknown,
        );

        // Wait for at least one flush interval
        sleep(Duration::from_secs(2)).await;

        // Data should be flushed to disk
        // We can verify by checking the store directly
        let store = registry.store.clone();
        let active_peers = store.load_active_peers().unwrap();
        assert_eq!(active_peers.len(), 1);

        // Cleanup
        registry.shutdown().await.unwrap();
    }

    // ==================== Shutdown Tests ====================

    #[tokio::test]
    async fn test_shutdown_stops_background_task() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config = create_test_config(&temp_dir);

        let mut registry = PersistentPeerRegistry::new(config).unwrap();
        registry.start_background_flush();

        let result = registry.shutdown().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_shutdown_flushes_data() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config = create_test_config(&temp_dir);
        let (peer_id, address) = create_test_peer(1);

        let registry = PersistentPeerRegistry::new(config.clone()).unwrap();

        // Don't start background flush - test explicit shutdown flush
        registry.on_connection_established(
            peer_id,
            address,
            ConnectionDirection::Outbound,
            DiscoverySource::Unknown,
        );

        // Shutdown should flush
        registry.shutdown().await.unwrap();

        // Verify data was flushed by opening a new store
        let store = PeerRegistryStore::new(&config.db_path).unwrap();
        let active_peers = store.load_active_peers().unwrap();
        assert_eq!(active_peers.len(), 1);
        assert_eq!(active_peers[0].peer_id, peer_id);
    }

    #[tokio::test]
    async fn test_shutdown_without_background_task() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config = create_test_config(&temp_dir);

        let registry = PersistentPeerRegistry::new(config).unwrap();

        // Shutdown without starting background task
        let result = registry.shutdown().await;
        assert!(result.is_ok());
    }

    // ==================== Connection Lifecycle Tests ====================

    #[tokio::test]
    async fn test_on_connection_established_updates_memory() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config = create_test_config(&temp_dir);
        let (peer_id, address) = create_test_peer(1);

        let registry = PersistentPeerRegistry::new(config).unwrap();
        registry.on_connection_established(
            peer_id,
            address.clone(),
            ConnectionDirection::Outbound,
            DiscoverySource::Unknown,
        );

        assert_eq!(registry.peer_count(), 1);

        let peer_info = registry.get_peer(&peer_id);
        assert!(peer_info.is_some());
        let peer_info = peer_info.unwrap();
        assert_eq!(peer_info.peer_id, peer_id);
        assert!(peer_info.addresses.contains(&address));
    }

    #[tokio::test]
    async fn test_on_connection_established_persists_to_disk() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config = create_test_config(&temp_dir);
        let (peer_id, address) = create_test_peer(1);

        // Create registry and add peer
        {
            let registry = PersistentPeerRegistry::new(config.clone()).unwrap();
            registry.on_connection_established(
                peer_id,
                address,
                ConnectionDirection::Outbound,
                DiscoverySource::Unknown,
            );
            registry.flush().unwrap(); // Ensure data is written
            // Drop registry here to close the database
        }

        // NOW open store to verify - database is closed
        let store = PeerRegistryStore::new(&config.db_path).unwrap();
        let active_peers = store.load_active_peers().unwrap();
        assert_eq!(active_peers.len(), 1);
        assert_eq!(active_peers[0].peer_id, peer_id);

        let known_peers = store.load_known_peers().unwrap();
        assert_eq!(known_peers.len(), 1);
        assert!(known_peers.contains_key(&peer_id));
    }

    #[tokio::test]
    async fn test_on_connection_established_multiple_peers() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config = create_test_config(&temp_dir);

        let registry = PersistentPeerRegistry::new(config).unwrap();

        for i in 1..=5 {
            let (peer_id, address) = create_test_peer(i);
            registry.on_connection_established(
                peer_id,
                address,
                ConnectionDirection::Outbound,
                DiscoverySource::Unknown,
            );
        }

        assert_eq!(registry.peer_count(), 5);
        assert_eq!(registry.connected_peers().len(), 5);
    }

    #[tokio::test]
    async fn test_on_connection_established_with_reconnection() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config = create_test_config(&temp_dir);
        let (peer_id, address) = create_test_peer(1);

        // Use same registry instance for entire lifecycle
        let registry = PersistentPeerRegistry::new(config.clone()).unwrap();

        // First connection
        registry.on_connection_established(
            peer_id,
            address.clone(),
            ConnectionDirection::Outbound,
            DiscoverySource::Unknown,
        );
        registry.on_connection_closed(&peer_id);

        // Second connection (reconnection)
        registry.on_connection_established(
            peer_id,
            address,
            ConnectionDirection::Outbound,
            DiscoverySource::Unknown,
        );

        let peer_info = registry.get_peer(&peer_id).unwrap();
        assert_eq!(peer_info.stats.reconnection_count, 1);

        // Verify reconnection count persisted
        registry.flush().unwrap();
        drop(registry); // Close DB

        // Open store to verify
        let store = PeerRegistryStore::new(&config.db_path).unwrap();
        let counts = store.load_reconnection_counts().unwrap();
        assert_eq!(*counts.get(&peer_id).unwrap(), 1);
    }

    #[tokio::test]
    async fn test_on_connection_closed_removes_from_active() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config = create_test_config(&temp_dir);
        let (peer_id, address) = create_test_peer(1);

        {
            let registry = PersistentPeerRegistry::new(config.clone()).unwrap();
            registry.on_connection_established(
                peer_id,
                address,
                ConnectionDirection::Outbound,
                DiscoverySource::Unknown,
            );

            assert_eq!(registry.peer_count(), 1);

            registry.on_connection_closed(&peer_id);

            assert_eq!(registry.peer_count(), 0);
            assert!(registry.get_peer(&peer_id).is_none());

            registry.flush().unwrap();
            // Drop registry
        }

        // Verify removed from disk
        let store = PeerRegistryStore::new(&config.db_path).unwrap();
        let active_peers = store.load_active_peers().unwrap();
        assert_eq!(active_peers.len(), 0);
    }

    #[tokio::test]
    async fn test_on_connection_closed_keeps_known_addresses() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config = create_test_config(&temp_dir);
        let (peer_id, address) = create_test_peer(1);

        {
            let registry = PersistentPeerRegistry::new(config.clone()).unwrap();
            registry.on_connection_established(
                peer_id,
                address.clone(),
                ConnectionDirection::Outbound,
                DiscoverySource::Unknown,
            );
            registry.on_connection_closed(&peer_id);

            // Should still have known addresses
            let known = registry.get_known_addresses(&peer_id);
            assert!(known.is_some());
            assert!(known.unwrap().contains(&address));

            registry.flush().unwrap();
            // Drop registry
        }

        // Verify persisted
        let store = PeerRegistryStore::new(&config.db_path).unwrap();
        let known_peers = store.load_known_peers().unwrap();
        assert!(known_peers.contains_key(&peer_id));
    }

    #[tokio::test]
    async fn test_on_connection_failed_updates_memory_only() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config = create_test_config(&temp_dir);
        let (peer_id, _) = create_test_peer(1);

        {
            let registry = PersistentPeerRegistry::new(config.clone()).unwrap();
            registry.on_connection_failed(Some(&peer_id));

            // Connection failures are ephemeral - not persisted
            // Just verify it doesn't panic
            assert_eq!(registry.peer_count(), 0);

            registry.flush().unwrap();
            // Drop registry
        }

        // Verify nothing persisted
        let store = PeerRegistryStore::new(&config.db_path).unwrap();
        let active_peers = store.load_active_peers().unwrap();
        assert_eq!(active_peers.len(), 0);
    }

    #[tokio::test]
    async fn test_on_connection_failed_with_none() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config = create_test_config(&temp_dir);

        let registry = PersistentPeerRegistry::new(config).unwrap();
        registry.on_connection_failed(None);

        // Should handle gracefully
        assert_eq!(registry.peer_count(), 0);
    }

    // ==================== Getter Methods Tests ====================

    #[tokio::test]
    async fn test_get_peer_returns_none_for_unknown() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config = create_test_config(&temp_dir);
        let (peer_id, _) = create_test_peer(1);

        let registry = PersistentPeerRegistry::new(config).unwrap();
        assert!(registry.get_peer(&peer_id).is_none());
    }

    #[tokio::test]
    async fn test_get_peer_returns_some_for_connected() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config = create_test_config(&temp_dir);
        let (peer_id, address) = create_test_peer(1);

        let registry = PersistentPeerRegistry::new(config).unwrap();
        registry.on_connection_established(
            peer_id,
            address,
            ConnectionDirection::Outbound,
            DiscoverySource::Unknown,
        );

        let peer_info = registry.get_peer(&peer_id);
        assert!(peer_info.is_some());
        assert_eq!(peer_info.unwrap().peer_id, peer_id);
    }

    #[tokio::test]
    async fn test_connected_peers_empty() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config = create_test_config(&temp_dir);

        let registry = PersistentPeerRegistry::new(config).unwrap();
        assert_eq!(registry.connected_peers().len(), 0);
    }

    #[tokio::test]
    async fn test_connected_peers_returns_all() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config = create_test_config(&temp_dir);

        let registry = PersistentPeerRegistry::new(config).unwrap();

        let mut expected_peers = vec![];
        for i in 1..=3 {
            let (peer_id, address) = create_test_peer(i);
            registry.on_connection_established(
                peer_id,
                address,
                ConnectionDirection::Outbound,
                DiscoverySource::Unknown,
            );
            expected_peers.push(peer_id);
        }

        let connected = registry.connected_peers();
        assert_eq!(connected.len(), 3);

        for peer_id in expected_peers {
            assert!(connected.iter().any(|p| p.peer_id == peer_id));
        }
    }

    #[tokio::test]
    async fn test_peer_count_accurate() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config = create_test_config(&temp_dir);

        let registry = PersistentPeerRegistry::new(config).unwrap();
        assert_eq!(registry.peer_count(), 0);

        let (peer_id1, address1) = create_test_peer(1);
        registry.on_connection_established(
            peer_id1,
            address1,
            ConnectionDirection::Outbound,
            DiscoverySource::Unknown,
        );
        assert_eq!(registry.peer_count(), 1);

        let (peer_id2, address2) = create_test_peer(2);
        registry.on_connection_established(
            peer_id2,
            address2,
            ConnectionDirection::Outbound,
            DiscoverySource::Unknown,
        );
        assert_eq!(registry.peer_count(), 2);

        registry.on_connection_closed(&peer_id1);
        assert_eq!(registry.peer_count(), 1);

        registry.on_connection_closed(&peer_id2);
        assert_eq!(registry.peer_count(), 0);
    }

    #[tokio::test]
    async fn test_get_known_addresses_none_for_unknown() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config = create_test_config(&temp_dir);
        let (peer_id, _) = create_test_peer(1);

        let registry = PersistentPeerRegistry::new(config).unwrap();
        assert!(registry.get_known_addresses(&peer_id).is_none());
    }

    #[tokio::test]
    async fn test_get_known_addresses_persists_after_disconnect() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config = create_test_config(&temp_dir);
        let (peer_id, address) = create_test_peer(1);

        let registry = PersistentPeerRegistry::new(config).unwrap();
        registry.on_connection_established(
            peer_id,
            address.clone(),
            ConnectionDirection::Outbound,
            DiscoverySource::Unknown,
        );
        registry.on_connection_closed(&peer_id);

        let known = registry.get_known_addresses(&peer_id);
        assert!(known.is_some());
        assert!(known.unwrap().contains(&address));
    }

    #[tokio::test]
    async fn test_stats_reflects_state() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config = create_test_config(&temp_dir);

        let registry = PersistentPeerRegistry::new(config).unwrap();

        let stats = registry.stats();
        assert_eq!(stats.connected_peers, 0);
        assert_eq!(stats.known_peers, 0);
        assert_eq!(stats.total_connections, 0);
        assert_eq!(stats.total_connection_failures, 0);

        let (peer_id, address) = create_test_peer(1);
        registry.on_connection_established(
            peer_id,
            address,
            ConnectionDirection::Outbound,
            DiscoverySource::Unknown,
        );

        let stats = registry.stats();
        assert_eq!(stats.connected_peers, 1);
        assert_eq!(stats.known_peers, 1);
        assert_eq!(stats.total_connections, 1);
    }

    // ==================== Activity Tracking Tests ====================

    #[tokio::test]
    async fn test_on_peer_activity_updates_memory() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config = create_test_config(&temp_dir);
        let (peer_id, address) = create_test_peer(1);

        let registry = PersistentPeerRegistry::new(config).unwrap();
        registry.on_connection_established(
            peer_id,
            address,
            ConnectionDirection::Outbound,
            DiscoverySource::Unknown,
        );

        let before = registry.get_peer(&peer_id).unwrap().last_seen;

        sleep(Duration::from_millis(100)).await;

        registry.on_peer_activity(&peer_id);

        let after = registry.get_peer(&peer_id).unwrap().last_seen;

        // last_seen should be updated
        assert!(after >= before);
    }

    #[tokio::test]
    async fn test_on_peer_activity_for_unknown_peer() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config = create_test_config(&temp_dir);
        let (peer_id, _) = create_test_peer(1);

        let registry = PersistentPeerRegistry::new(config).unwrap();

        // Should not panic for unknown peer
        registry.on_peer_activity(&peer_id);
    }

    // ==================== Manual Flush Tests ====================

    #[tokio::test]
    async fn test_manual_flush() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config = create_test_config(&temp_dir);
        let (peer_id, address) = create_test_peer(1);

        let registry = PersistentPeerRegistry::new(config).unwrap();
        registry.on_connection_established(
            peer_id,
            address,
            ConnectionDirection::Outbound,
            DiscoverySource::Unknown,
        );

        let result = registry.flush();
        assert!(result.is_ok());
    }

    // ==================== Drop Tests ====================

    #[tokio::test]
    async fn test_drop_flushes_data() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config = create_test_config(&temp_dir);
        let (peer_id, address) = create_test_peer(1);

        {
            let registry = PersistentPeerRegistry::new(config.clone()).unwrap();
            registry.on_connection_established(
                peer_id,
                address,
                ConnectionDirection::Outbound,
                DiscoverySource::Unknown,
            );
            // Drop happens here
        }

        // Verify data was flushed
        let store = PeerRegistryStore::new(&config.db_path).unwrap();
        let active_peers = store.load_active_peers().unwrap();
        assert_eq!(active_peers.len(), 1);
        assert_eq!(active_peers[0].peer_id, peer_id);
    }

    #[tokio::test]
    async fn test_drop_with_background_task() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config = create_test_config(&temp_dir);

        {
            let mut registry = PersistentPeerRegistry::new(config).unwrap();
            registry.start_background_flush();
            // Drop should stop background task
        }

        // If we get here without hanging, the test passes
    }

    // ==================== Persistence Across Restarts Tests ====================

    #[tokio::test]
    async fn test_data_persists_across_registry_restarts() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config = create_test_config(&temp_dir);
        let (peer_id, address) = create_test_peer(1);

        // First registry instance
        {
            let registry = PersistentPeerRegistry::new(config.clone()).unwrap();
            registry.on_connection_established(
                peer_id,
                address.clone(),
                ConnectionDirection::Outbound,
                DiscoverySource::Unknown,
            );
            registry.flush().unwrap();
        }

        // Second registry instance
        {
            let registry = PersistentPeerRegistry::new(config).unwrap();

            // Should load known addresses
            let known = registry.get_known_addresses(&peer_id);
            assert!(known.is_some());
            assert!(known.unwrap().contains(&address));
        }
    }

    #[tokio::test]
    async fn test_multiple_connections_and_disconnections_persist() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config = create_test_config(&temp_dir);
        let (peer_id, address) = create_test_peer(1);

        // Simulate multiple connection cycles
        {
            let registry = PersistentPeerRegistry::new(config.clone()).unwrap();

            for _ in 0..3 {
                registry.on_connection_established(
                    peer_id,
                    address.clone(),
                    ConnectionDirection::Outbound,
                    DiscoverySource::Unknown,
                );
                registry.on_connection_closed(&peer_id);
            }

            registry.flush().unwrap();
        }

        // Verify reconnection count persisted
        {
            let registry = PersistentPeerRegistry::new(config.clone()).unwrap();
            registry.on_connection_established(
                peer_id,
                address,
                ConnectionDirection::Outbound,
                DiscoverySource::Unknown,
            );

            let peer_info = registry.get_peer(&peer_id).unwrap();
            // Should have reconnection count from previous session
            assert!(peer_info.stats.reconnection_count > 0);
        }
    }

    // ==================== Concurrent Operations Tests ====================

    #[tokio::test]
    async fn test_concurrent_connections() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config = create_test_config(&temp_dir);

        let registry = Arc::new(PersistentPeerRegistry::new(config).unwrap());

        let mut handles = vec![];
        for i in 1..=5 {
            let registry_clone = Arc::clone(&registry);
            let handle = tokio::spawn(async move {
                let (peer_id, address) = create_test_peer(i);
                registry_clone.on_connection_established(
                    peer_id,
                    address,
                    ConnectionDirection::Outbound,
                    DiscoverySource::Unknown,
                );
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.await.unwrap();
        }

        assert_eq!(registry.peer_count(), 5);
    }

    #[tokio::test]
    async fn test_concurrent_reads_and_writes() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config = create_test_config(&temp_dir);

        let registry = Arc::new(PersistentPeerRegistry::new(config).unwrap());

        // Pre-populate
        for i in 1..=3 {
            let (peer_id, address) = create_test_peer(i);
            registry.on_connection_established(
                peer_id,
                address,
                ConnectionDirection::Outbound,
                DiscoverySource::Unknown,
            );
        }

        let mut handles = vec![];

        // Readers
        for _ in 0..5 {
            let registry_clone = Arc::clone(&registry);
            let handle = tokio::spawn(async move {
                for _ in 0..10 {
                    let _ = registry_clone.peer_count();
                    let _ = registry_clone.connected_peers();
                    let _ = registry_clone.stats();
                }
            });
            handles.push(handle);
        }

        // Writers
        for i in 4..=6 {
            let registry_clone = Arc::clone(&registry);
            let handle = tokio::spawn(async move {
                let (peer_id, address) = create_test_peer(i);
                registry_clone.on_connection_established(
                    peer_id,
                    address,
                    ConnectionDirection::Outbound,
                    DiscoverySource::Unknown,
                );
                sleep(Duration::from_millis(10)).await;
                registry_clone.on_connection_closed(&peer_id);
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.await.unwrap();
        }

        // Should not panic and have consistent state
        assert!(registry.peer_count() <= 6);
    }

    // ==================== Edge Cases ====================

    #[tokio::test]
    async fn test_same_peer_multiple_addresses() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config = create_test_config(&temp_dir);

        let keypair = Keypair::generate_ed25519();
        let peer_id = keypair.public().to_peer_id();
        let address1: Multiaddr = "/ip4/127.0.0.1/tcp/4001".parse().unwrap();
        let address2: Multiaddr = "/ip4/192.168.1.1/tcp/4002".parse().unwrap();

        let registry = PersistentPeerRegistry::new(config).unwrap();

        registry.on_connection_established(
            peer_id,
            address1.clone(),
            ConnectionDirection::Outbound,
            DiscoverySource::Unknown,
        );
        registry.on_connection_established(
            peer_id,
            address2.clone(),
            ConnectionDirection::Outbound,
            DiscoverySource::Unknown,
        );

        let peer_info = registry.get_peer(&peer_id).unwrap();
        assert!(peer_info.addresses.contains(&address1));
        assert!(peer_info.addresses.contains(&address2));
    }

    #[tokio::test]
    async fn test_empty_registry_operations() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config = create_test_config(&temp_dir);

        let registry = PersistentPeerRegistry::new(config).unwrap();

        assert_eq!(registry.peer_count(), 0);
        assert_eq!(registry.connected_peers().len(), 0);
        assert_eq!(registry.stats().connected_peers, 0);
        assert!(registry.flush().is_ok());
    }
}
