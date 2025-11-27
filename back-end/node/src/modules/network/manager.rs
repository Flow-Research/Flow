use crate::bootstrap::init::NodeData;
use crate::modules::network::behaviour::FlowBehaviourEvent;
use crate::modules::network::config::{BootstrapConfig, ConnectionLimitPolicy, ConnectionLimits};
use crate::modules::network::gossipsub::{
    GossipSubConfig, Message, MessagePayload, MessageRouter, MessageStore, MessageStoreConfig,
    RouterStats, SubscriptionHandle, Topic, TopicSubscriptionManager,
};
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
use tracing::{debug, error, info, warn};

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

    /// Message router for pub/sub (initialized at construction)
    message_router: Arc<MessageRouter>,

    /// Subscription manager (shared with router)
    pub subscription_manager: Arc<TopicSubscriptionManager>,
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

    /// Message router for persisting and delivering incoming messages
    message_router: Arc<MessageRouter>,

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
    /// Returns `AppError::Crypto` if key conversion fails
    pub async fn new(node_data: &NodeData) -> Result<Self, AppError> {
        info!("Initializing NetworkManager");

        let keypair = keypair::ed25519_to_libp2p_keypair(&node_data.private_key)?;
        let local_peer_id = keypair.public().to_peer_id();

        info!(peer_id = %local_peer_id, "Network PeerId initialized");

        let (command_tx, command_rx) = mpsc::unbounded_channel();

        let (message_router, subscription_manager) = Self::create_message_routing()?;

        info!(
            peer_id = %local_peer_id,
            "NetworkManager initialized with message routing"
        );

        Ok(Self {
            local_peer_id,
            node_data: node_data.clone(),
            command_tx,
            command_rx: Arc::new(Mutex::new(Some(command_rx))),
            event_loop_handle: Arc::new(Mutex::new(None)),
            message_router,
            subscription_manager,
        })
    }

    /// Create message store and routing components
    fn create_message_routing()
    -> Result<(Arc<MessageRouter>, Arc<TopicSubscriptionManager>), AppError> {
        let message_store = Self::create_message_store()?;
        let subscription_manager = Arc::new(TopicSubscriptionManager::default());

        let gossipsub_config = GossipSubConfig::from_env();
        let message_router = Arc::new(MessageRouter::new(
            message_store,
            Arc::clone(&subscription_manager),
            gossipsub_config.max_transmit_size,
        ));

        Ok((message_router, subscription_manager))
    }

    /// Create the message store with RocksDB backend
    fn create_message_store() -> Result<Arc<MessageStore>, AppError> {
        let config = MessageStoreConfig::from_env();
        let data_dir = Self::get_message_store_path();

        let store = MessageStore::new(&data_dir, config)
            .map_err(|e| AppError::Network(format!("Failed to create message store: {}", e)))?;

        Ok(Arc::new(store))
    }

    /// Get the path for message store database
    fn get_message_store_path() -> std::path::PathBuf {
        std::env::var("GOSSIP_MESSAGE_DB_PATH")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| {
                directories::ProjectDirs::from("network", "flow", "node")
                    .map(|dirs| dirs.data_dir().join("gossip"))
                    .unwrap_or_else(|| std::path::PathBuf::from("/tmp/flow/gossip"))
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
        let event_loop =
            self.create_event_loop(swarm, command_rx, config, Arc::clone(&self.message_router))?;

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
        message_router: Arc<MessageRouter>,
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
            message_router,
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
    pub async fn subscribe(
        &self,
        topic: impl Into<String>,
    ) -> Result<SubscriptionHandle, AppError> {
        let topic = topic.into();

        // First, subscribe at the GossipSub protocol level
        self.ensure_started().await?;

        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(NetworkCommand::Subscribe {
                topic: topic.clone(),
                response: response_tx,
            })
            .map_err(|e| AppError::Network(format!("Failed to send command: {}", e)))?;

        response_rx
            .await
            .map_err(|e| AppError::Network(format!("Failed to receive response: {}", e)))?
            .map_err(|e| AppError::Network(e))?;

        // Then, create a subscription handle for receiving messages
        let handle = self.subscription_manager.subscribe(&topic).await;

        info!(topic = %topic, "Subscribed to topic with message channel");

        Ok(handle)
    }

    /// Subscribe to a Topic
    pub async fn subscribe_topic(&self, topic: &Topic) -> Result<SubscriptionHandle, AppError> {
        self.subscribe(topic.to_topic_string()).await
    }

    /// Unsubscribe from a topic
    pub async fn unsubscribe(&self, topic: impl Into<String>) -> Result<(), AppError> {
        let topic = topic.into();
        self.ensure_started().await?;

        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(NetworkCommand::Unsubscribe {
                topic: topic.clone(),
                response: response_tx,
            })
            .map_err(|e| AppError::Network(format!("Failed to send command: {}", e)))?;

        response_rx
            .await
            .map_err(|e| AppError::Network(format!("Failed to receive response: {}", e)))?
            .map_err(|e| AppError::Network(e))?;

        // Also clean up local subscription
        self.subscription_manager.unsubscribe(&topic).await;

        Ok(())
    }

    /// Publish a message to a topic
    pub async fn publish(&self, message: Message) -> Result<String, AppError> {
        self.ensure_started().await?;

        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(NetworkCommand::Publish {
                message,
                response: response_tx,
            })
            .map_err(|e| AppError::Network(format!("Failed to send command: {}", e)))?;

        response_rx
            .await
            .map_err(|e| AppError::Network(format!("Failed to receive response: {}", e)))?
            .map_err(|e| AppError::Network(e))
    }

    /// Publish a payload to a FlowTopic
    pub async fn publish_to_topic(
        &self,
        topic: &Topic,
        payload: MessagePayload,
    ) -> Result<String, AppError> {
        let message = Message::new(self.local_peer_id, topic.to_topic_string(), payload);
        self.publish(message).await
    }

    /// Get historical messages for a topic
    pub async fn get_message_history(
        &self,
        topic: &str,
        since: Option<u64>,
        limit: usize,
    ) -> Result<Vec<Message>, AppError> {
        self.message_router
            .get_history(topic, since, limit)
            .await
            .map_err(|e| AppError::Network(format!("Failed to get history: {}", e)))
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

    /// Get message router statistics
    pub async fn message_stats(&self) -> crate::modules::network::gossipsub::RouterStats {
        self.message_router.stats().await
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

    /// Check if any subscribers exist for a topic
    pub async fn has_subscribers(&self, topic: &str) -> bool {
        self.subscription_manager.has_subscribers(topic).await
    }

    /// Get router statistics for debugging/monitoring
    pub async fn get_router_stats(&self) -> Result<RouterStats, AppError> {
        Ok(self.message_router.stats().await)
    }

    /// Check if the network manager is currently running
    pub async fn is_running(&self) -> bool {
        let handle_guard = self.event_loop_handle.lock().await;
        handle_guard.is_some()
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

            NetworkCommand::Publish { message, response } => {
                let topic = message.topic.clone();
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
                    data_len = message.data.len(),
                    "GossipSub message received"
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
                            "Valid message received, routing to subscribers"
                        );

                        // Route via MessageRouter (persists + delivers to subscribers)
                        let router = Arc::clone(&self.message_router);
                        let msg_id = flow_msg.id.clone();
                        let msg_topic = flow_msg.topic.clone();

                        tokio::spawn(async move {
                            match router.route(flow_msg).await {
                                Ok(delivered) => {
                                    if delivered {
                                        debug!(
                                            message_id = %msg_id,
                                            topic = %msg_topic,
                                            "Message delivered to subscribers"
                                        );
                                    } else {
                                        debug!(
                                            message_id = %msg_id,
                                            topic = %msg_topic,
                                            "Message persisted but no active subscribers"
                                        );
                                    }
                                }
                                Err(e) => {
                                    error!(
                                        message_id = %msg_id,
                                        error = %e,
                                        "Failed to route message"
                                    );
                                }
                            }
                        });
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
