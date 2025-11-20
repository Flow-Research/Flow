use crate::bootstrap::init::NodeData;
use crate::modules::network::behaviour::FlowBehaviourEvent;
use crate::modules::network::{behaviour::FlowBehaviour, config::NetworkConfig, keypair};
use errors::AppError;
use libp2p::{Multiaddr, PeerId, Swarm, futures::StreamExt, identity::Keypair, noise, tcp, yamux};
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

/// High-level network manager providing thread-safe access to P2P networking
#[derive(Debug)]
pub struct NetworkManager {
    /// Our local peer ID
    local_peer_id: PeerId,

    /// Node identity data (needed to rebuild swarm)
    node_data: NodeData,

    /// Channel for sending commands to the event loop
    command_tx: mpsc::UnboundedSender<NetworkCommand>,

    command_rx: Arc<Mutex<Option<mpsc::UnboundedReceiver<NetworkCommand>>>>,

    /// Handle to the background event loop task
    event_loop_handle: Arc<Mutex<Option<JoinHandle<()>>>>,
}

/// Commands sent to the NetworkManager event loop
enum NetworkCommand {
    /// Gracefully shutdown the network
    Shutdown,

    /// Query the current number of connected peers
    GetPeerCount(oneshot::Sender<usize>),
}

/// Internal event loop handling Swarm events
struct NetworkEventLoop {
    swarm: Swarm<FlowBehaviour>,
    command_rx: mpsc::UnboundedReceiver<NetworkCommand>,
}

impl NetworkManager {
    /// Create a new NetworkManager from node identity
    ///
    /// This initializes the NetworkManager but does NOT start the swarm.
    /// Call `start()` to begin networking operations.
    ///
    /// # Arguments
    /// * `node_data` - Node identity containing Ed25519 keys
    ///
    /// # Returns
    /// A NetworkManager instance ready to start
    ///
    /// # Errors
    /// Returns `AppError::Network` if key conversion fails
    pub async fn new(node_data: &NodeData) -> Result<Self, AppError> {
        info!("Initializing NetworkManager");

        // Convert Ed25519 key to libp2p format to get PeerId
        let keypair = keypair::ed25519_to_libp2p_keypair(&node_data.private_key)?;
        let local_peer_id = keypair.public().to_peer_id();

        info!("Network PeerId: {}", local_peer_id);

        // Create command channel
        let (command_tx, command_rx) = mpsc::unbounded_channel();
        // Note: We'll create a new receiver in start()

        Ok(Self {
            local_peer_id,
            node_data: node_data.clone(),
            command_tx,
            command_rx: Arc::new(Mutex::new(Some(command_rx))),
            event_loop_handle: Arc::new(Mutex::new(None)),
        })
    }

    /// Build the libp2p Swarm with transport and behaviour
    async fn build_swarm(
        keypair: Keypair,
        local_peer_id: PeerId,
        config: &NetworkConfig,
    ) -> Result<Swarm<FlowBehaviour>, AppError> {
        info!("Building libp2p Swarm");

        // Create the behaviour
        let mut behaviour = FlowBehaviour::new(local_peer_id);

        // Add bootstrap peers to Kademlia
        for addr in &config.bootstrap_peers {
            if let Some(peer_id) = addr.iter().find_map(|p| match p {
                libp2p::multiaddr::Protocol::P2p(peer_id) => Some(peer_id),
                _ => None,
            }) {
                info!("Adding bootstrap peer: {} at {}", peer_id, addr);
                behaviour.add_bootstrap_peer(peer_id, addr.clone());
            }
        }

        // Build Swarm with QUIC and TCP transports
        let swarm = libp2p::SwarmBuilder::with_existing_identity(keypair)
            .with_tokio()
            .with_tcp(
                tcp::Config::default(),
                noise::Config::new,
                yamux::Config::default,
            )
            .map_err(|e| AppError::Network(format!("Failed to build TCP transport: {}", e)))?
            .with_quic()
            .with_behaviour(|_| behaviour)
            .map_err(|e| AppError::Network(format!("Failed to build behaviour: {}", e)))?
            .with_swarm_config(|cfg| {
                cfg.with_idle_connection_timeout(std::time::Duration::from_secs(60))
            })
            .build();

        info!("Swarm built successfully");
        Ok(swarm)
    }

    /// Start the network manager
    ///
    /// This spawns the event loop task and begins listening on the configured
    /// port. The event loop runs until `stop()` is called.
    ///
    /// # Errors
    /// Returns `AppError::Network` if already started or listen fails
    pub async fn start(&self, config: &NetworkConfig) -> Result<(), AppError> {
        let mut handle_guard = self.event_loop_handle.lock().await;

        if handle_guard.is_some() {
            return Err(AppError::Network(
                "NetworkManager already started".to_string(),
            ));
        }

        info!("Starting NetworkManager event loop");

        // Determine listen address
        let listen_addr: Multiaddr = if config.enable_quic {
            format!("/ip4/0.0.0.0/udp/{}/quic-v1", config.listen_port)
                .parse()
                .map_err(|e| AppError::Network(format!("Invalid listen address: {}", e)))?
        } else {
            format!("/ip4/0.0.0.0/tcp/{}", config.listen_port)
                .parse()
                .map_err(|e| AppError::Network(format!("Invalid listen address: {}", e)))?
        };

        // Convert keys and build swarm
        let keypair = keypair::ed25519_to_libp2p_keypair(&self.node_data.private_key)?;
        let swarm = Self::build_swarm(keypair, self.local_peer_id, config).await?;

        // Create NEW command receiver for this event loop
        // let (_new_tx, command_rx) = mpsc::unbounded_channel::<NetworkCommand>();
        let command_rx = self
            .command_rx
            .lock()
            .await
            .take()
            .ok_or_else(|| AppError::Network("Network already started".to_string()))?;

        // Create event loop
        let mut event_loop = NetworkEventLoop { swarm, command_rx };

        // Listen on address
        event_loop
            .swarm
            .listen_on(listen_addr.clone())
            .map_err(|e| AppError::Network(format!("Failed to listen: {}", e)))?;

        info!("Listening on: {}", listen_addr);

        // Spawn event loop task
        let handle = tokio::spawn(async move {
            event_loop.run().await;
        });

        *handle_guard = Some(handle);

        Ok(())
    }

    /// Stop the network manager
    ///
    /// Sends shutdown command and waits for event loop to terminate
    pub async fn stop(&self) -> Result<(), AppError> {
        info!("Stopping NetworkManager");

        let mut handle_guard = self.event_loop_handle.lock().await;

        // If never started, just return success
        if handle_guard.is_none() {
            info!("NetworkManager was never started, nothing to stop");
            return Ok(());
        }

        // Send shutdown command
        if let Err(_) = self.command_tx.send(NetworkCommand::Shutdown) {
            // Channel closed - event loop already stopped, this is okay
            warn!("Event loop already stopped");
            *handle_guard = None;
            return Ok(());
        }

        // Wait for event loop to finish
        if let Some(handle) = handle_guard.take() {
            handle
                .await
                .map_err(|e| AppError::Network(format!("Event loop panic: {}", e)))?;
        }

        info!("NetworkManager stopped");
        Ok(())
    }

    /// Get the local PeerId
    pub fn local_peer_id(&self) -> &PeerId {
        &self.local_peer_id
    }

    /// Get the number of connected peers
    pub async fn peer_count(&self) -> Result<usize, AppError> {
        // Check if started
        {
            let handle_guard = self.event_loop_handle.lock().await;
            if handle_guard.is_none() {
                return Err(AppError::Network(
                    "NetworkManager not started - call start() first".to_string(),
                ));
            }
        }

        let (tx, rx) = oneshot::channel();

        self.command_tx
            .send(NetworkCommand::GetPeerCount(tx))
            .map_err(|_| AppError::Network("Failed to query peer count".to_string()))?;

        rx.await
            .map_err(|_| AppError::Network("Failed to receive peer count".to_string()))
    }
}

impl NetworkEventLoop {
    /// Main event loop processing Swarm events and commands
    async fn run(&mut self) {
        info!("Network event loop started");

        loop {
            tokio::select! {
                // Handle Swarm events
                event = self.swarm.select_next_some() => {
                    self.handle_swarm_event(event).await;
                }

                // Handle commands from NetworkManager
                Some(command) = self.command_rx.recv() => {
                    self.handle_command(command).await;
                }
            }
        }
    }

    /// Handle individual Swarm events
    async fn handle_swarm_event(&mut self, event: libp2p::swarm::SwarmEvent<FlowBehaviourEvent>) {
        use libp2p::swarm::SwarmEvent;

        match event {
            SwarmEvent::NewListenAddr { address, .. } => {
                info!("Listening on: {}", address);
            }
            SwarmEvent::ConnectionEstablished {
                peer_id, endpoint, ..
            } => {
                info!(
                    "Connection established with {} via {}",
                    peer_id,
                    endpoint.get_remote_address()
                );
            }
            SwarmEvent::ConnectionClosed { peer_id, cause, .. } => {
                debug!("Connection closed with {}: {:?}", peer_id, cause);
            }
            SwarmEvent::Behaviour(FlowBehaviourEvent::Kademlia(kad_event)) => {
                self.handle_kad_event(kad_event).await;
            }
            event => {
                debug!("Unhandled swarm event: {:?}", event);
            }
        }
    }

    async fn handle_command(&mut self, command: NetworkCommand) {
        match command {
            NetworkCommand::Shutdown => {
                info!("Received shutdown command");
                return;
            }
            NetworkCommand::GetPeerCount(response_tx) => {
                let count = self.swarm.connected_peers().count();
                let _ = response_tx.send(count);
            }
        }
    }

    /// Handle Kademlia-specific events
    async fn handle_kad_event(&mut self, event: libp2p::kad::Event) {
        use libp2p::kad::Event;

        match event {
            Event::RoutingUpdated {
                peer, addresses, ..
            } => {
                debug!(
                    "Routing table updated: {} with {} addresses",
                    peer,
                    addresses.len()
                );
            }
            Event::OutboundQueryProgressed { result, .. } => {
                debug!("Kademlia query progressed: {:?}", result);
            }
            event => {
                debug!("Kademlia event: {:?}", event);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;
    use std::time::Duration;
    use tokio::time::sleep;

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

    // Helper to create a test config with random port
    fn create_test_config() -> NetworkConfig {
        NetworkConfig {
            enable_quic: true,
            listen_port: 0, // OS assigns random port
            bootstrap_peers: vec![],
        }
    }

    #[tokio::test]
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
    async fn test_peer_id_deterministic_from_same_keys() {
        let node_data = create_test_node_data();

        let manager1 = NetworkManager::new(&node_data).await.unwrap();
        let manager2 = NetworkManager::new(&node_data).await.unwrap();

        assert_eq!(
            manager1.local_peer_id(),
            manager2.local_peer_id(),
            "Same keys should produce identical PeerIds"
        );
    }

    #[tokio::test]
    async fn test_peer_id_unique_for_different_keys() {
        let node_data1 = create_test_node_data();
        let node_data2 = create_test_node_data();

        let manager1 = NetworkManager::new(&node_data1).await.unwrap();
        let manager2 = NetworkManager::new(&node_data2).await.unwrap();

        assert_ne!(
            manager1.local_peer_id(),
            manager2.local_peer_id(),
            "Different keys should produce different PeerIds"
        );
    }

    #[tokio::test]
    async fn test_network_manager_start_and_stop() {
        let node_data = create_test_node_data();
        let config = create_test_config();

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
    async fn test_cannot_start_twice() {
        let node_data = create_test_node_data();
        let config = create_test_config();

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
    async fn test_stop_without_start_is_safe() {
        let node_data = create_test_node_data();
        let manager = NetworkManager::new(&node_data).await.unwrap();

        // Stopping without starting should not panic or error
        let result = manager.stop().await;
        assert!(result.is_ok(), "Stop should be safe even if never started");
    }

    #[tokio::test]
    async fn test_peer_count_initially_zero() {
        let node_data = create_test_node_data();
        let config = create_test_config();

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
    async fn test_multiple_managers_can_coexist() {
        let node_data1 = create_test_node_data();
        let node_data2 = create_test_node_data();
        let config1 = create_test_config();
        let config2 = create_test_config();

        let manager1 = NetworkManager::new(&node_data1).await.unwrap();
        let manager2 = NetworkManager::new(&node_data2).await.unwrap();

        // Both should start successfully on different random ports
        assert!(manager1.start(&config1).await.is_ok());
        assert!(manager2.start(&config2).await.is_ok());

        sleep(Duration::from_millis(100)).await;

        // Both should have zero peers (not connected to each other yet)
        assert_eq!(manager1.peer_count().await.unwrap(), 0);
        assert_eq!(manager2.peer_count().await.unwrap(), 0);

        // Clean up
        manager1.stop().await.unwrap();
        manager2.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_start_with_specific_port() {
        let node_data = create_test_node_data();

        // Try to bind to a specific high port (less likely to be in use)
        let config = NetworkConfig {
            enable_quic: true,
            listen_port: 9876,
            bootstrap_peers: vec![],
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
    async fn test_quic_transport_config() {
        let node_data = create_test_node_data();

        let config = NetworkConfig {
            enable_quic: true,
            listen_port: 0,
            bootstrap_peers: vec![],
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
    async fn test_tcp_transport_config() {
        let node_data = create_test_node_data();

        let config = NetworkConfig {
            enable_quic: false, // Use TCP
            listen_port: 0,
            bootstrap_peers: vec![],
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
    async fn test_concurrent_peer_count_queries() {
        let node_data = create_test_node_data();
        let config = create_test_config();

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
    async fn test_shutdown_is_graceful() {
        let node_data = create_test_node_data();
        let config = create_test_config();

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
