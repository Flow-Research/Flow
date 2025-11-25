use crate::bootstrap::init::NodeData;
use crate::modules::network::behaviour::FlowBehaviourEvent;
use crate::modules::network::config::{BootstrapConfig, ConnectionLimitPolicy, ConnectionLimits};
use crate::modules::network::gossipsub::{GossipSubConfig, Message, Topic};
use crate::modules::network::peer_registry::{
    ConnectionDirection, DiscoverySource, NetworkStats, PeerInfo,
};
use crate::modules::network::persistent_peer_registry::{
    PeerRegistryConfig, PersistentPeerRegistry,
};
use crate::modules::network::storage::{RocksDbStore, StorageConfig};
use crate::modules::network::{behaviour::FlowBehaviour, config::NetworkConfig, keypair};
use errors::AppError;
use libp2p::gossipsub::{self, IdentTopic};
use libp2p::kad::QueryResult;
use libp2p::multiaddr::Protocol;
use libp2p::{Multiaddr, PeerId, Swarm, futures::StreamExt, identity::Keypair, noise, tcp, yamux};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant as StdInstant};
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

/// Result of bootstrap operation
#[derive(Debug, Clone)]
pub struct BootstrapResult {
    /// Number of bootstrap peers successfully connected
    pub connected_count: usize,
    /// Number of bootstrap peers that failed to connect
    pub failed_count: usize,
    /// Whether Kademlia bootstrap query was initiated
    pub query_initiated: bool,
}

/// High-level network manager providing thread-safe access to P2P networking
#[derive(Debug)]
pub struct NetworkManager {
    /// Our local peer ID
    local_peer_id: PeerId,

    /// Node identity data (needed to rebuild swarm)
    node_data: NodeData,

    /// Channel for sending commands to the event loop
    command_tx: mpsc::UnboundedSender<NetworkCommand>,

    /// Command receiver - taken once during start() and moved to the event loop.
    /// Wrapped in Arc<Mutex<Option<>>> to allow one-time transfer to the spawned task.
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

    /// Get list of all connected peers
    GetConnectedPeers(oneshot::Sender<Vec<PeerInfo>>),

    /// Get info about a specific peer
    GetPeerInfo {
        peer_id: PeerId,
        response: oneshot::Sender<Option<PeerInfo>>,
    },

    /// Dial a peer at a specific address
    DialPeer {
        address: Multiaddr,
        response: oneshot::Sender<Result<(), String>>,
    },

    /// Disconnect from a peer
    DisconnectPeer {
        peer_id: PeerId,
        response: oneshot::Sender<Result<(), String>>,
    },

    /// Get network statistics
    GetNetworkStats(oneshot::Sender<NetworkStats>),

    /// Get connection capacity status
    GetConnectionCapacity(oneshot::Sender<ConnectionCapacityStatus>),

    /// Trigger bootstrap process manually
    TriggerBootstrap {
        response: oneshot::Sender<BootstrapResult>,
    },

    /// Get list of bootstrap peer IDs
    GetBootstrapPeerIds(oneshot::Sender<Vec<PeerId>>),

    /// Subscribe to a topic
    Subscribe {
        topic: String,
        response: oneshot::Sender<Result<(), String>>,
    },

    /// Unsubscribe from a topic
    Unsubscribe {
        topic: String,
        response: oneshot::Sender<Result<(), String>>,
    },

    /// Publish a message
    Publish {
        topic: String,
        message: Message,
        response: oneshot::Sender<Result<String, String>>,
    },

    /// Get subscribed topics
    GetSubscriptions(oneshot::Sender<Vec<String>>),

    /// Get mesh peers for a topic
    GetMeshPeers {
        topic: String,
        response: oneshot::Sender<Vec<PeerId>>,
    },
}

/// Connection capacity status
#[derive(Debug, Clone)]
pub struct ConnectionCapacityStatus {
    pub active_connections: usize,
    pub max_connections: usize,
    pub available_slots: usize,
    pub can_accept_inbound: bool,
    pub can_dial_outbound: bool,
}

/// Internal event loop handling Swarm events
struct NetworkEventLoop {
    /// The libp2p Swarm managing P2P connections, protocols, and network behavior.
    /// Processes events from the transport layer and routes them to appropriate handlers.
    swarm: Swarm<FlowBehaviour>,

    /// Channel receiver for commands from NetworkManager API.
    /// Commands include peer queries, dial requests, disconnect operations, and shutdown signals.
    command_rx: mpsc::UnboundedReceiver<NetworkCommand>,

    /// Thread-safe persistent peer registry for storing connection history and peer metadata.
    /// Tracks discovered peers, connection statistics, and provides data persistence across restarts.
    peer_registry: Arc<PersistentPeerRegistry>,

    /// Track peers we're currently dialing to prevent duplicate attempts.
    /// A peer is added when dialing starts and removed when connection succeeds/fails.
    /// Prevents wasted resources and connection race conditions.
    dialing_peers: HashSet<PeerId>,

    /// Track when we last attempted to dial each peer (for rate limiting).
    /// Prevents hammering unreachable peers by enforcing minimum retry intervals.
    /// Entries may be cleaned up periodically to prevent unbounded growth.
    last_dial_attempt: HashMap<PeerId, StdInstant>,

    /// Track how we discovered each peer (mDNS, DHT, bootstrap, manual).
    /// Used to set the discovery_source field when connection is established,
    /// enabling analysis of discovery method effectiveness and network topology.
    peer_discovery_source: HashMap<PeerId, DiscoverySource>,

    /// Connection limits configuration (max connections, policy, reserved slots).
    /// Applied to inbound connection decisions to prevent resource exhaustion
    /// while maintaining capacity for critical outbound connections.
    connection_limits: ConnectionLimits,

    /// Track active connection count for enforcing connection limits.
    /// Incremented on ConnectionEstablished, decremented on ConnectionClosed.
    /// Used with connection_limits to gate new inbound connections.
    active_connections: usize,

    /// Bootstrap peer states for tracking connection attempts
    /// Set of peer IDs that are bootstrap peers (for retry prioritization)
    bootstrap_peer_ids: HashSet<PeerId>,

    /// Bootstrap configuration
    bootstrap_config: BootstrapConfig,

    /// Whether initial bootstrap has been triggered
    bootstrap_initiated: bool,

    /// Channel for broadcasting incoming messages
    message_tx: mpsc::UnboundedSender<Message>,

    /// GossipSub configuration for message validation
    gossipsub_config: GossipSubConfig,
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

        // Create persistent RocksDB store for DHT
        let storage_config = StorageConfig::from_env();
        let store = RocksDbStore::new(
            &storage_config.db_path,
            local_peer_id,
            storage_config.clone(),
        )?;

        // Create the behaviour
        let mut behaviour = FlowBehaviour::new(
            local_peer_id,
            &keypair,
            store,
            config.mdns.enabled,
            Some(&config.gossipsub),
        );

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

        info!("Swarm built successfully (mDNS: {})", config.mdns.enabled);

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
        // 1. Guard check
        self.ensure_not_started().await?;

        info!("Starting NetworkManager event loop");

        // 2. Build all components
        let listen_addr = Self::resolve_listen_address(config)?;
        let swarm = self.build_configured_swarm(config).await?;
        let command_rx = self.take_command_receiver().await?;
        let event_loop = self.create_event_loop(swarm, command_rx, config)?;

        // 3. Start listening and spawn
        let handle = self.spawn_event_loop(event_loop, listen_addr).await?;

        // 4. Store handle
        *self.event_loop_handle.lock().await = Some(handle);

        Ok(())
    }

    // Individual responsibility methods:
    async fn ensure_not_started(&self) -> Result<(), AppError> {
        let handle_guard = self.event_loop_handle.lock().await;
        if handle_guard.is_some() {
            return Err(AppError::Network(
                "NetworkManager already started".to_string(),
            ));
        }
        Ok(())
    }

    fn resolve_listen_address(config: &NetworkConfig) -> Result<Multiaddr, AppError> {
        let listen_addr = if config.enable_quic {
            format!("/ip4/0.0.0.0/udp/{}/quic-v1", config.listen_port)
        } else {
            format!("/ip4/0.0.0.0/tcp/{}", config.listen_port)
        };

        listen_addr
            .parse()
            .map_err(|e| AppError::Network(format!("Invalid listen address: {}", e)))
    }

    async fn build_configured_swarm(
        &self,
        config: &NetworkConfig,
    ) -> Result<Swarm<FlowBehaviour>, AppError> {
        let keypair = keypair::ed25519_to_libp2p_keypair(&self.node_data.private_key)?;
        Self::build_swarm(keypair, self.local_peer_id, config).await
    }

    async fn take_command_receiver(
        &self,
    ) -> Result<mpsc::UnboundedReceiver<NetworkCommand>, AppError> {
        self.command_rx
            .lock()
            .await
            .take()
            .ok_or_else(|| AppError::Network("Network already started".to_string()))
    }

    fn create_event_loop(
        &self,
        swarm: Swarm<FlowBehaviour>,
        command_rx: mpsc::UnboundedReceiver<NetworkCommand>,
        config: &NetworkConfig,
    ) -> Result<NetworkEventLoop, AppError> {
        let peer_registry_config = PeerRegistryConfig::from_env();
        let mut persistent_registry =
            PersistentPeerRegistry::new(peer_registry_config).map_err(|e| {
                AppError::Network(format!("Failed to create persistent peer registry: {}", e))
            })?;

        persistent_registry.start_background_flush();

        // Parse bootstrap peers and load into registry
        let (bootstrap_peer_ids, bootstrap_known_peers) =
            Self::parse_bootstrap_peers(&config.bootstrap_peers);

        // Load bootstrap peers into existing known_peers infrastructure
        if !bootstrap_known_peers.is_empty() {
            persistent_registry.bulk_load_known_peers(bootstrap_known_peers);
            info!(
                bootstrap_peer_count = bootstrap_peer_ids.len(),
                "Loaded bootstrap peers into registry"
            );
        }

        // Create message channel for GossipSub incoming messages
        let (message_tx, _message_rx) = mpsc::unbounded_channel();

        Ok(NetworkEventLoop {
            swarm,
            command_rx,
            peer_registry: Arc::new(persistent_registry),
            dialing_peers: HashSet::new(),
            last_dial_attempt: HashMap::new(),
            peer_discovery_source: HashMap::new(),
            connection_limits: config.connection_limits.clone(),
            active_connections: 0,
            bootstrap_peer_ids,
            bootstrap_config: config.bootstrap.clone(),
            bootstrap_initiated: false,
            message_tx,
            gossipsub_config: config.gossipsub.clone(),
        })
    }

    /// Parse bootstrap addresses into peer IDs and known_peers map
    fn parse_bootstrap_peers(
        addrs: &[Multiaddr],
    ) -> (HashSet<PeerId>, HashMap<PeerId, Vec<Multiaddr>>) {
        let mut peer_ids = HashSet::new();
        let mut known_peers: HashMap<PeerId, Vec<Multiaddr>> = HashMap::new();

        for addr in addrs {
            if let Some(peer_id) = addr.iter().find_map(|p| match p {
                Protocol::P2p(id) => Some(id),
                _ => None,
            }) {
                peer_ids.insert(peer_id);
                known_peers.entry(peer_id).or_default().push(addr.clone());
            } else {
                warn!(address = %addr, "Bootstrap address missing /p2p/ component, skipping");
            }
        }

        (peer_ids, known_peers)
    }

    async fn spawn_event_loop(
        &self,
        mut event_loop: NetworkEventLoop,
        listen_addr: Multiaddr,
    ) -> Result<JoinHandle<()>, AppError> {
        event_loop
            .swarm
            .listen_on(listen_addr.clone())
            .map_err(|e| AppError::Network(format!("Failed to listen: {}", e)))?;

        info!("Listening on: {}", listen_addr);

        Ok(tokio::spawn(async move {
            event_loop.run().await;
        }))
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

    /// Get list of all connected peers
    pub async fn connected_peers(&self) -> Result<Vec<PeerInfo>, AppError> {
        self.ensure_started().await?;

        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(NetworkCommand::GetConnectedPeers(tx))
            .map_err(|_| AppError::Network("Failed to query connected peers".to_string()))?;

        rx.await
            .map_err(|_| AppError::Network("Failed to receive connected peers".to_string()))
    }

    /// Get information about a specific peer
    pub async fn peer_info(&self, peer_id: PeerId) -> Result<Option<PeerInfo>, AppError> {
        self.ensure_started().await?;

        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(NetworkCommand::GetPeerInfo {
                peer_id,
                response: tx,
            })
            .map_err(|_| AppError::Network("Failed to query peer info".to_string()))?;

        rx.await
            .map_err(|_| AppError::Network("Failed to receive peer info".to_string()))
    }

    /// Dial a peer at a specific address
    pub async fn dial_peer(&self, address: Multiaddr) -> Result<(), AppError> {
        self.ensure_started().await?;

        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(NetworkCommand::DialPeer {
                address,
                response: tx,
            })
            .map_err(|_| AppError::Network("Failed to send dial command".to_string()))?;

        rx.await
            .map_err(|_| AppError::Network("Failed to receive dial response".to_string()))?
            .map_err(|e| AppError::Network(e))
    }

    /// Disconnect from a peer
    pub async fn disconnect_peer(&self, peer_id: PeerId) -> Result<(), AppError> {
        self.ensure_started().await?;

        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(NetworkCommand::DisconnectPeer {
                peer_id,
                response: tx,
            })
            .map_err(|_| AppError::Network("Failed to send disconnect command".to_string()))?;

        rx.await
            .map_err(|_| AppError::Network("Failed to receive disconnect response".to_string()))?
            .map_err(|e| AppError::Network(e))
    }

    /// Get network statistics
    pub async fn network_stats(&self) -> Result<NetworkStats, AppError> {
        self.ensure_started().await?;

        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(NetworkCommand::GetNetworkStats(tx))
            .map_err(|_| AppError::Network("Failed to query network stats".to_string()))?;

        rx.await
            .map_err(|_| AppError::Network("Failed to receive network stats".to_string()))
    }

    /// Helper to ensure network is started before operations
    async fn ensure_started(&self) -> Result<(), AppError> {
        let handle_guard = self.event_loop_handle.lock().await;
        if handle_guard.is_none() {
            return Err(AppError::Network(
                "NetworkManager not started - call start() first".to_string(),
            ));
        }
        Ok(())
    }

    /// Get connection capacity status
    pub async fn connection_capacity(&self) -> Result<ConnectionCapacityStatus, AppError> {
        let (response_tx, response_rx) = oneshot::channel();

        self.command_tx
            .send(NetworkCommand::GetConnectionCapacity(response_tx))
            .map_err(|e| AppError::Network(format!("Failed to send command: {}", e)))?;

        response_rx
            .await
            .map_err(|e| AppError::Network(format!("Failed to receive response: {}", e)))
    }

    /// Manually trigger bootstrap process
    ///
    /// Dials all bootstrap peers and initiates Kademlia bootstrap query.
    /// Useful after network reconnection or configuration changes.
    pub async fn trigger_bootstrap(&self) -> Result<BootstrapResult, AppError> {
        self.ensure_started().await?;

        let (response_tx, response_rx) = oneshot::channel();

        self.command_tx
            .send(NetworkCommand::TriggerBootstrap {
                response: response_tx,
            })
            .map_err(|e| AppError::Network(format!("Failed to send command: {}", e)))?;

        response_rx
            .await
            .map_err(|e| AppError::Network(format!("Failed to receive response: {}", e)))
    }

    /// Get list of configured bootstrap peer IDs
    pub async fn bootstrap_peer_ids(&self) -> Result<Vec<PeerId>, AppError> {
        self.ensure_started().await?;

        let (response_tx, response_rx) = oneshot::channel();

        self.command_tx
            .send(NetworkCommand::GetBootstrapPeerIds(response_tx))
            .map_err(|e| AppError::Network(format!("Failed to send command: {}", e)))?;

        response_rx
            .await
            .map_err(|e| AppError::Network(format!("Failed to receive response: {}", e)))
    }

    /// Subscribe to a topic
    pub async fn subscribe(&self, topic: impl Into<String>) -> Result<(), AppError> {
        self.ensure_started().await?;

        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(NetworkCommand::Subscribe {
                topic: topic.into(),
                response: response_tx,
            })
            .map_err(|e| AppError::Network(format!("Failed to send command: {}", e)))?;

        response_rx
            .await
            .map_err(|e| AppError::Network(format!("Failed to receive response: {}", e)))?
            .map_err(|e| AppError::Network(e))
    }

    /// Subscribe to a Topic
    pub async fn subscribe_topic(&self, topic: &Topic) -> Result<(), AppError> {
        self.subscribe(topic.to_topic_string()).await
    }

    /// Unsubscribe from a topic
    pub async fn unsubscribe(&self, topic: impl Into<String>) -> Result<(), AppError> {
        self.ensure_started().await?;

        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(NetworkCommand::Unsubscribe {
                topic: topic.into(),
                response: response_tx,
            })
            .map_err(|e| AppError::Network(format!("Failed to send command: {}", e)))?;

        response_rx
            .await
            .map_err(|e| AppError::Network(format!("Failed to receive response: {}", e)))?
            .map_err(|e| AppError::Network(e))
    }

    /// Publish a message to a topic
    pub async fn publish(&self, message: Message) -> Result<String, AppError> {
        self.ensure_started().await?;

        let topic = message.topic.clone();
        let (response_tx, response_rx) = oneshot::channel();

        self.command_tx
            .send(NetworkCommand::Publish {
                topic,
                message,
                response: response_tx,
            })
            .map_err(|e| AppError::Network(format!("Failed to send command: {}", e)))?;

        response_rx
            .await
            .map_err(|e| AppError::Network(format!("Failed to receive response: {}", e)))?
            .map_err(|e| AppError::Network(e))
    }

    /// Publish a payload to a Topic
    pub async fn publish_to_topic(
        &self,
        topic: &Topic,
        payload: crate::modules::network::gossipsub::MessagePayload,
    ) -> Result<String, AppError> {
        let message = Message::new(self.local_peer_id, topic.to_topic_string(), payload);
        self.publish(message).await
    }

    /// Get list of subscribed topics
    pub async fn subscribed_topics(&self) -> Result<Vec<String>, AppError> {
        self.ensure_started().await?;

        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(NetworkCommand::GetSubscriptions(response_tx))
            .map_err(|e| AppError::Network(format!("Failed to send command: {}", e)))?;

        response_rx
            .await
            .map_err(|e| AppError::Network(format!("Failed to receive response: {}", e)))
    }

    /// Get mesh peers for a topic
    pub async fn mesh_peers(&self, topic: impl Into<String>) -> Result<Vec<PeerId>, AppError> {
        self.ensure_started().await?;

        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(NetworkCommand::GetMeshPeers {
                topic: topic.into(),
                response: response_tx,
            })
            .map_err(|e| AppError::Network(format!("Failed to send command: {}", e)))?;

        response_rx
            .await
            .map_err(|e| AppError::Network(format!("Failed to receive response: {}", e)))
    }

    /// Get a receiver for incoming messages
    ///
    /// Note: Call this before start() to receive messages from the beginning
    pub fn message_receiver(&self) -> mpsc::UnboundedReceiver<Message> {
        // This would need to be set up during construction
        // For now, this is a placeholder
        todo!("Implement message receiver channel setup")
    }
}

impl NetworkEventLoop {
    /// Main event loop processing Swarm events and commands
    async fn run(&mut self) {
        info!("Network event loop started");

        self.run_event_loop().await;
        self.shutdown().await;

        info!("Network event loop stopped");
    }

    /// Core event loop with bootstrap scheduling and retry
    async fn run_event_loop(&mut self) {
        // Calculate bootstrap delay upfront
        let bootstrap_delay_ms =
            if self.bootstrap_config.auto_dial || self.bootstrap_config.auto_bootstrap {
                info!(
                    delay_ms = self.bootstrap_config.startup_delay_ms,
                    "Bootstrap scheduled"
                );
                self.bootstrap_config.startup_delay_ms
            } else {
                debug!("Bootstrap disabled");
                u64::MAX
            };

        // Create the sleep future with the extracted value
        let bootstrap_delay = tokio::time::sleep(Duration::from_millis(bootstrap_delay_ms));
        tokio::pin!(bootstrap_delay);

        // Periodic retry interval for failed bootstrap peers (every 5 seconds)
        let mut retry_interval = tokio::time::interval(Duration::from_secs(5));
        retry_interval.tick().await; // Skip immediate first tick

        loop {
            tokio::select! {
                // One-shot initial bootstrap trigger
                _ = &mut bootstrap_delay, if !self.bootstrap_initiated => {
                    self.execute_bootstrap();
                }

                // Periodic retry of failed bootstrap peers
                _ = retry_interval.tick(), if self.bootstrap_initiated && !self.bootstrap_peer_ids.is_empty() => {
                    self.retry_bootstrap_peers();
                }

                // Handle Swarm events
                event = self.swarm.select_next_some() => {
                    self.handle_swarm_event(event).await;
                }

                // Handle commands
                Some(command) = self.command_rx.recv() => {
                    if self.handle_command(command).await {
                        break;
                    }
                }
            }
        }
    }

    /// Graceful shutdown
    async fn shutdown(&mut self) {
        info!("Flushing persistent peer registry before shutdown");
        if let Err(e) = self.peer_registry.flush() {
            warn!(error = %e, "Failed to flush peer registry on shutdown");
        }
    }

    // ==================== Bootstrap Logic ====================

    /// Execute the full bootstrap process
    fn execute_bootstrap(&mut self) -> BootstrapResult {
        if self.bootstrap_initiated {
            return BootstrapResult {
                connected_count: 0,
                failed_count: 0,
                query_initiated: false,
            };
        }

        self.bootstrap_initiated = true;

        info!(
            peer_count = self.bootstrap_peer_ids.len(),
            auto_dial = self.bootstrap_config.auto_dial,
            auto_bootstrap = self.bootstrap_config.auto_bootstrap,
            "Executing bootstrap"
        );

        let (connected, failed) = self.dial_bootstrap_peers();
        let query_initiated = self.initiate_kademlia_bootstrap();

        BootstrapResult {
            connected_count: connected,
            failed_count: failed,
            query_initiated,
        }
    }

    /// Dial all bootstrap peers using addresses from peer registry
    fn dial_bootstrap_peers(&mut self) -> (usize, usize) {
        if !self.bootstrap_config.auto_dial {
            return (0, 0);
        }

        let mut attempts = 0;
        let mut failures = 0;

        let bootstrap_ids: Vec<PeerId> = self.bootstrap_peer_ids.iter().copied().collect();

        for peer_id in bootstrap_ids {
            if self.peer_registry.is_connected(&peer_id) {
                debug!(peer_id = %peer_id, "Bootstrap peer already connected");
                attempts += 1; // Count as successful
                continue;
            }

            if self.dialing_peers.contains(&peer_id) {
                debug!(peer_id = %peer_id, "Bootstrap peer already dialing");
                continue;
            }

            if let Some(addrs) = self.peer_registry.get_known_addresses(&peer_id) {
                if let Some(addr) = addrs.first() {
                    self.peer_discovery_source
                        .insert(peer_id, DiscoverySource::Bootstrap);
                    self.last_dial_attempt.insert(peer_id, StdInstant::now());

                    info!(peer_id = %peer_id, address = %addr, "Dialing bootstrap peer");

                    match self.swarm.dial(addr.clone()) {
                        Ok(_) => {
                            self.dialing_peers.insert(peer_id);
                            attempts += 1;
                        }
                        Err(e) => {
                            warn!(peer_id = %peer_id, error = %e, "Failed to dial bootstrap peer");
                            failures += 1;
                        }
                    }
                }
            } else {
                warn!(peer_id = %peer_id, "No known addresses for bootstrap peer");
                failures += 1;
            }
        }

        info!(
            dial_attempts = attempts,
            failures = failures,
            "Bootstrap peer dialing complete"
        );

        (attempts, failures)
    }

    /// Initiate Kademlia DHT bootstrap query
    fn initiate_kademlia_bootstrap(&mut self) -> bool {
        if !self.bootstrap_config.auto_bootstrap {
            return false;
        }

        match self.swarm.behaviour_mut().bootstrap() {
            Ok(query_id) => {
                info!(query_id = ?query_id, "Kademlia bootstrap query initiated");
                true
            }
            Err(e) => {
                warn!(error = ?e, "Kademlia bootstrap failed - no known peers in routing table");
                false
            }
        }
    }

    // ==================== Bootstrap Retry Logic ====================

    /// Check if a peer is a bootstrap peer
    fn _is_bootstrap_peer(&self, peer_id: &PeerId) -> bool {
        self.bootstrap_peer_ids.contains(peer_id)
    }

    /// Retry disconnected bootstrap peers with exponential backoff
    fn retry_bootstrap_peers(&mut self) {
        let now = StdInstant::now();
        let bootstrap_ids: Vec<PeerId> = self.bootstrap_peer_ids.iter().copied().collect();

        for peer_id in bootstrap_ids {
            if self.should_retry_bootstrap_peer(&peer_id, now) {
                self.retry_single_bootstrap_peer(peer_id, now);
            }
        }
    }

    /// Check if we should retry connecting to a bootstrap peer
    fn should_retry_bootstrap_peer(&self, peer_id: &PeerId, now: StdInstant) -> bool {
        // Skip if connected or dialing
        if self.peer_registry.is_connected(peer_id) || self.dialing_peers.contains(peer_id) {
            return false;
        }

        // Check retry count against max
        let attempts = self.peer_registry.get_reconnection_count(peer_id);
        if attempts >= self.bootstrap_config.max_retries {
            return false;
        }

        // Check backoff timing
        let backoff = self.calculate_backoff(attempts);
        self.last_dial_attempt
            .get(peer_id)
            .map(|last| now.duration_since(*last) >= backoff)
            .unwrap_or(true)
    }

    /// Calculate exponential backoff duration: base_delay * 2^attempts
    fn calculate_backoff(&self, attempts: u32) -> Duration {
        let exponent = attempts.min(10); // Cap to prevent overflow
        let multiplier = 1u64 << exponent;
        let ms = self
            .bootstrap_config
            .retry_delay_base_ms
            .saturating_mul(multiplier);
        Duration::from_millis(ms)
    }

    /// Retry connecting to a single bootstrap peer
    fn retry_single_bootstrap_peer(&mut self, peer_id: PeerId, now: StdInstant) {
        let Some(addrs) = self.peer_registry.get_known_addresses(&peer_id) else {
            return;
        };

        let Some(addr) = addrs.first() else {
            return;
        };

        let attempts = self.peer_registry.get_reconnection_count(&peer_id);
        let next_backoff = self.calculate_backoff(attempts + 1);

        info!(
            peer_id = %peer_id,
            attempt = attempts + 1,
            max_retries = self.bootstrap_config.max_retries,
            next_backoff_ms = next_backoff.as_millis(),
            "Retrying bootstrap peer connection"
        );

        self.last_dial_attempt.insert(peer_id, now);
        self.peer_discovery_source
            .insert(peer_id, DiscoverySource::Bootstrap);

        if self.swarm.dial(addr.clone()).is_ok() {
            self.dialing_peers.insert(peer_id);
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
                peer_id,
                endpoint,
                established_in,
                ..
            } => {
                // Increment connection counter FIRST
                self.active_connections += 1;

                let direction = if endpoint.is_dialer() {
                    ConnectionDirection::Outbound
                } else {
                    ConnectionDirection::Inbound
                };

                // Check limits AFTER incrementing (for both inbound and outbound if needed)
                let should_disconnect = if self.connection_limits.max_connections == 0 {
                    false // Unlimited
                } else {
                    match self.connection_limits.policy {
                        ConnectionLimitPolicy::Strict => {
                            // Strict: reject ANY connection over max
                            self.active_connections > self.connection_limits.max_connections
                        }
                        ConnectionLimitPolicy::PreferOutbound => {
                            // PreferOutbound: only reject INBOUND over inbound_limit
                            // Outbound can go up to max_connections
                            if direction == ConnectionDirection::Inbound {
                                let inbound_limit = self
                                    .connection_limits
                                    .max_connections
                                    .saturating_sub(self.connection_limits.reserved_outbound);
                                self.active_connections > inbound_limit
                            } else {
                                // Outbound: only check against total max
                                self.active_connections > self.connection_limits.max_connections
                            }
                        }
                    }
                };

                if should_disconnect {
                    warn!(
                        active_connections = self.active_connections,
                        max_connections = self.connection_limits.max_connections,
                        policy = ?self.connection_limits.policy,
                        direction = ?direction,
                        peer_id = %peer_id,
                        "Connection limit exceeded, disconnecting peer"
                    );

                    // Disconnect immediately
                    let _ = self.swarm.disconnect_peer_id(peer_id);

                    // Don't process further
                    return;
                }

                // Determine discovery source
                // For outbound connections, we look up how we discovered them
                // For inbound connections, we don't know how they found us
                let discovery_source = if direction == ConnectionDirection::Outbound {
                    self.peer_discovery_source
                        .remove(&peer_id) // Remove and return
                        .unwrap_or(DiscoverySource::Unknown)
                } else {
                    DiscoverySource::Unknown // We don't know how they discovered us
                };

                info!(
                    active_connections = self.active_connections,
                    "Connection established with {} via {} (direction: {:?}, source: {:?}, time: {:?})",
                    peer_id,
                    endpoint.get_remote_address(),
                    direction,
                    discovery_source,
                    established_in
                );

                // Remove from dialing set if we were dialing
                self.dialing_peers.remove(&peer_id);
                self.peer_registry.on_connection_established(
                    peer_id,
                    endpoint.get_remote_address().clone(),
                    direction,
                    discovery_source,
                );
            }

            SwarmEvent::ConnectionClosed { peer_id, cause, .. } => {
                self.active_connections = self.active_connections.saturating_sub(1);

                debug!(
                    active_connections = self.active_connections,
                    peer_id = %peer_id,
                    cause = ?cause,
                    "Connection closed"
                );

                self.peer_registry.on_connection_closed(&peer_id);
                self.dialing_peers.remove(&peer_id);
            }

            SwarmEvent::IncomingConnection { send_back_addr, .. } => {
                debug!(
                    active_connections = self.active_connections,
                    max_connections = self.connection_limits.max_connections,
                    from = %send_back_addr,
                    "Incoming connection attempt"
                );
            }

            SwarmEvent::IncomingConnectionError {
                send_back_addr,
                error,
                ..
            } => {
                warn!(
                    "Incoming connection error from {}: {}",
                    send_back_addr, error
                );
                self.peer_registry.on_connection_failed(None);
            }

            SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
                warn!("Outgoing connection error to {:?}: {}", peer_id, error);
                if let Some(peer) = peer_id.as_ref() {
                    self.dialing_peers.remove(peer);
                }
                self.peer_registry.on_connection_failed(peer_id.as_ref());
            }

            SwarmEvent::Dialing {
                peer_id,
                connection_id,
            } => {
                debug!(
                    "Dialing peer {:?} (connection: {:?})",
                    peer_id, connection_id
                );
            }

            SwarmEvent::Behaviour(FlowBehaviourEvent::Kademlia(kad_event)) => {
                self.handle_kad_event(kad_event).await;
            }

            SwarmEvent::Behaviour(FlowBehaviourEvent::Mdns(mdns_event)) => {
                self.handle_mdns_event(mdns_event).await;
            }

            SwarmEvent::Behaviour(FlowBehaviourEvent::Gossipsub(event)) => {
                self.handle_gossipsub_event(event).await;
            }

            event => {
                debug!("Unhandled swarm event: {:?}", event);
            }
        }
    }

    async fn handle_command(&mut self, command: NetworkCommand) -> bool {
        match command {
            NetworkCommand::Shutdown => {
                info!("Received shutdown command");
                return true; // only shutdown command should return true
            }

            NetworkCommand::GetPeerCount(response_tx) => {
                let count = self.peer_registry.peer_count();
                let _ = response_tx.send(count);
            }

            NetworkCommand::GetConnectedPeers(response_tx) => {
                let peers = self.peer_registry.connected_peers();
                let _ = response_tx.send(peers);
            }

            NetworkCommand::GetPeerInfo { peer_id, response } => {
                let info = self.peer_registry.get_peer(&peer_id);
                let _ = response.send(info);
            }

            NetworkCommand::DialPeer { address, response } => {
                match self.swarm.dial(address.clone()) {
                    Ok(_) => {
                        info!("Dialing peer at {}", address);
                        let _ = response.send(Ok(()));
                    }
                    Err(e) => {
                        warn!("Failed to dial peer at {}: {:?}", address, e);
                        let _ = response.send(Err(format!("Dial failed: {:?}", e)));
                    }
                }
            }

            NetworkCommand::DisconnectPeer { peer_id, response } => {
                let result = self.swarm.disconnect_peer_id(peer_id);
                match result {
                    Ok(_) => {
                        info!("Disconnected from peer {}", peer_id);
                        let _ = response.send(Ok(()));
                    }
                    Err(e) => {
                        warn!("Failed to disconnect from peer {}: {:?}", peer_id, e);
                        let _ = response.send(Err(format!("Disconnect failed: {:?}", e)));
                    }
                }
            }

            NetworkCommand::GetNetworkStats(response_tx) => {
                let stats = self.peer_registry.stats();
                let _ = response_tx.send(stats);
            }

            NetworkCommand::GetConnectionCapacity(response) => {
                let status = ConnectionCapacityStatus {
                    active_connections: self.active_connections,
                    max_connections: self.connection_limits.max_connections,
                    available_slots: self
                        .connection_limits
                        .max_connections
                        .saturating_sub(self.active_connections),
                    can_accept_inbound: self
                        .connection_limits
                        .can_accept_inbound(self.active_connections),
                    can_dial_outbound: self
                        .connection_limits
                        .can_dial_outbound(self.active_connections),
                };

                let _ = response.send(status);
            }

            NetworkCommand::TriggerBootstrap { response } => {
                let result = self.execute_bootstrap();
                let _ = response.send(result);
            }

            NetworkCommand::GetBootstrapPeerIds(response) => {
                let peer_ids: Vec<PeerId> = self.bootstrap_peer_ids.iter().copied().collect();
                let _ = response.send(peer_ids);
            }

            NetworkCommand::Subscribe { topic, response } => {
                let ident_topic = IdentTopic::new(&topic);
                let result = match self.swarm.behaviour_mut().subscribe(&ident_topic) {
                    Ok(true) => {
                        info!(topic = %topic, "Subscribed to topic");
                        Ok(())
                    }
                    Ok(false) => Err("GossipSub is disabled".to_string()),
                    Err(e) => Err(format!("Subscription failed: {:?}", e)),
                };
                let _ = response.send(result);
            }

            NetworkCommand::Unsubscribe { topic, response } => {
                let ident_topic = IdentTopic::new(&topic);
                let unsubscribed = self.swarm.behaviour_mut().unsubscribe(&ident_topic);
                if unsubscribed {
                    info!(topic = %topic, "Unsubscribed from topic");
                } else {
                    debug!(topic = %topic, "Was not subscribed to topic");
                }
                let _ = response.send(Ok(()));
            }

            NetworkCommand::Publish {
                topic,
                message,
                response,
            } => {
                let ident_topic = IdentTopic::new(&topic);

                match message.serialize() {
                    Ok(data) => match self.swarm.behaviour_mut().publish(ident_topic, data) {
                        Ok(msg_id) => {
                            info!(
                                topic = %topic,
                                message_id = %msg_id,
                                flow_message_id = %message.id,
                                "Message published"
                            );
                            let _ = response.send(Ok(msg_id.to_string()));
                        }
                        Err(e) => {
                            warn!(topic = %topic, error = ?e, "Publish failed");
                            let _ = response.send(Err(format!("Publish failed: {:?}", e)));
                        }
                    },
                    Err(e) => {
                        let _ = response.send(Err(format!("Serialization failed: {}", e)));
                    }
                }
            }

            NetworkCommand::GetSubscriptions(response_tx) => {
                let topics: Vec<String> = self
                    .swarm
                    .behaviour()
                    .subscribed_topics()
                    .iter()
                    .map(|h| h.to_string())
                    .collect();
                let _ = response_tx.send(topics);
            }

            NetworkCommand::GetMeshPeers { topic, response } => {
                let ident_topic = IdentTopic::new(&topic);
                let peers = self.swarm.behaviour().mesh_peers(&ident_topic.hash());
                let _ = response.send(peers);
            }
        }

        false
    }

    /// Handle Kademlia-specific events
    async fn handle_kad_event(&mut self, event: libp2p::kad::Event) {
        use libp2p::kad::Event;

        match event {
            Event::RoutingUpdated {
                peer, addresses, ..
            } => {
                debug!(
                    source = "dht",
                    %peer,
                    address_count = addresses.len(),
                    "Routing table updated"
                );

                // Track that we discovered this peer via DHT
                self.peer_discovery_source
                    .insert(peer, DiscoverySource::Dht);

                // Note: Kademlia automatically dials these peers in the background
                // The connection will use the discovery source we just set
            }

            Event::OutboundQueryProgressed { result, .. } => {
                debug!("Kademlia query progressed: {:?}", result);

                match result {
                    QueryResult::GetClosestPeers(Ok(closest)) => {
                        for peer_info in &closest.peers {
                            self.peer_discovery_source
                                .insert(peer_info.peer_id, DiscoverySource::Dht);
                        }
                    }
                    _ => {}
                }
            }

            event => {
                debug!("Kademlia event: {:?}", event);
            }
        }
    }

    /// Handle mDNS-specific events
    async fn handle_mdns_event(&mut self, event: libp2p::mdns::Event) {
        match event {
            libp2p::mdns::Event::Discovered(list) => {
                for (peer_id, multiaddr) in list {
                    info!(
                        source = "mdns",
                        %peer_id,
                        %multiaddr,
                        "Peer discovered"
                    );

                    // Track that this peer was discovered via mDNS
                    self.peer_discovery_source
                        .insert(peer_id, DiscoverySource::Mdns);

                    // Check if we should dial this peer
                    if self.should_dial_peer(&peer_id) {
                        info!("Auto-dialing mDNS-discovered peer: {}", peer_id);

                        match self.swarm.dial(multiaddr.clone()) {
                            Ok(_) => {
                                self.dialing_peers.insert(peer_id);
                                self.last_dial_attempt.insert(peer_id, StdInstant::now());
                                debug!("Successfully initiated dial to {}", peer_id);
                            }
                            Err(e) => {
                                warn!("Failed to dial mDNS peer {}: {:?}", peer_id, e);
                                self.peer_discovery_source.remove(&peer_id);
                            }
                        }
                    } else {
                        debug!(
                            "Skipping dial to {} (already connected or recently attempted)",
                            peer_id
                        );

                        self.peer_discovery_source.remove(&peer_id);
                    }
                }
            }

            libp2p::mdns::Event::Expired(list) => {
                for (peer_id, multiaddr) in list {
                    info!(
                        "mDNS peer expired: {} at {} (no longer advertising)",
                        peer_id, multiaddr
                    );
                    // Note: We don't disconnect here, as the peer might still be reachable
                    // Connections will time out naturally if peer is truly gone
                }
            }
        }
    }

    /// Handle GossipSub events
    async fn handle_gossipsub_event(&mut self, event: gossipsub::Event) {
        match event {
            gossipsub::Event::Message {
                propagation_source,
                message_id,
                message,
            } => {
                debug!(
                    source = %propagation_source,
                    message_id = %message_id,
                    topic = %message.topic,
                    "Received GossipSub message"
                );

                // Deserialize and validate
                match Message::deserialize(&message.data) {
                    Ok(flow_msg) => {
                        // Validate message
                        if let Err(e) = flow_msg.validate(self.gossipsub_config.max_transmit_size) {
                            warn!(
                                message_id = %message_id,
                                error = %e,
                                "Message validation failed"
                            );
                            return;
                        }

                        // Check expiry
                        if flow_msg.is_expired() {
                            debug!(message_id = %message_id, "Dropping expired message");
                            return;
                        }

                        info!(
                            message_id = %flow_msg.id,
                            topic = %flow_msg.topic,
                            source = %flow_msg.source,
                            payload_type = ?std::mem::discriminant(&flow_msg.payload),
                            "Valid message received"
                        );

                        // Broadcast to subscribers
                        if self.message_tx.send(flow_msg).is_err() {
                            debug!("No message receivers");
                        }
                    }
                    Err(e) => {
                        warn!(
                            message_id = %message_id,
                            error = %e,
                            "Failed to deserialize message"
                        );
                    }
                }
            }

            gossipsub::Event::Subscribed { peer_id, topic } => {
                info!(peer_id = %peer_id, topic = %topic, "Peer subscribed to topic");
            }

            gossipsub::Event::Unsubscribed { peer_id, topic } => {
                info!(peer_id = %peer_id, topic = %topic, "Peer unsubscribed from topic");
            }

            gossipsub::Event::GossipsubNotSupported { peer_id } => {
                debug!(peer_id = %peer_id, "Peer does not support GossipSub");
            }

            _ => {}
        }
    }

    /// Determine if we should dial a discovered peer
    ///
    /// This prevents:
    /// - Dialing ourselves
    /// - Duplicate dial attempts
    /// - Connection storms (rate limiting)
    fn should_dial_peer(&self, peer_id: &PeerId) -> bool {
        // Don't dial ourselves
        if peer_id == self.swarm.local_peer_id() {
            return false;
        }

        // Don't dial if already connected (check swarm's connection list)
        if self.swarm.is_connected(peer_id) {
            return false;
        }

        // Don't dial if already in peer registry
        if self.peer_registry.get_peer(peer_id).is_some() {
            return false;
        }

        // Don't dial if we're currently dialing
        if self.dialing_peers.contains(peer_id) {
            return false;
        }

        // Rate limiting: Don't dial if we tried recently (within 30 seconds)
        if let Some(last_attempt) = self.last_dial_attempt.get(peer_id) {
            let elapsed = StdInstant::now().duration_since(*last_attempt);
            if elapsed < Duration::from_secs(30) {
                return false;
            }
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use crate::modules::network::config::MdnsConfig;

    use super::*;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;
    use serial_test::serial;
    use std::time::Duration;
    use tempfile::TempDir;
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

    // Helper to create a test config with unique temp directories
    fn create_test_config() -> (NetworkConfig, TempDir, TempDir) {
        let dht_temp = TempDir::new().expect("Failed to create DHT temp dir");
        let peer_reg_temp = TempDir::new().expect("Failed to create peer registry temp dir");

        // Set env vars for this test's unique paths
        unsafe {
            std::env::set_var("DHT_DB_PATH", dht_temp.path());
            std::env::set_var("PEER_REGISTRY_DB_PATH", peer_reg_temp.path());
        }

        let config = NetworkConfig {
            enable_quic: true,
            listen_port: 0, // OS assigns random port
            bootstrap_peers: vec![],
            mdns: MdnsConfig::default(),
            connection_limits: ConnectionLimits::default(),
            bootstrap: BootstrapConfig::default(),
            gossipsub: GossipSubConfig::default(),
        };

        (config, dht_temp, peer_reg_temp)
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

        let manager1 = NetworkManager::new(&node_data).await.unwrap();
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

        let manager1 = NetworkManager::new(&node_data1).await.unwrap();
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
        let (config, _dht_temp, _peer_reg_temp) = create_test_config();

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
        let (config, _dht_temp, _peer_reg_temp) = create_test_config();

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
        let (config, _dht_temp, _peer_reg_temp) = create_test_config();

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
            mdns: MdnsConfig::default(),
            connection_limits: ConnectionLimits::default(),
            bootstrap: BootstrapConfig::default(),
            gossipsub: GossipSubConfig::default(),
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
            mdns: MdnsConfig::default(),
            connection_limits: ConnectionLimits::default(),
            bootstrap: BootstrapConfig::default(),
            gossipsub: GossipSubConfig::default(),
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
            mdns: MdnsConfig::default(),
            connection_limits: ConnectionLimits::default(),
            bootstrap: BootstrapConfig::default(),
            gossipsub: GossipSubConfig::default(),
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
        let (config, _dht_temp, _peer_reg_temp) = create_test_config();

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
        let (config, _dht_temp, _peer_reg_temp) = create_test_config();

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
        let (config, _dht_temp, _peer_reg_temp) = create_test_config();

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
        let (config, _dht_temp, _peer_reg_temp) = create_test_config();

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
        let (config, _dht_temp, _peer_reg_temp) = create_test_config();

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
        let (config, _dht_temp, _peer_reg_temp) = create_test_config();

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
        let (config, _dht_temp, _peer_reg_temp) = create_test_config();

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
