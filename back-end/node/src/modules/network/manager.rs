use crate::bootstrap::init::NodeData;
use crate::modules::network::behaviour::FlowBehaviourEvent;
use crate::modules::network::config::ProviderConfig;
use crate::modules::network::config::{BootstrapConfig, ConnectionLimitPolicy, ConnectionLimits};
use crate::modules::network::content_transfer::{
    ContentRequest, ContentResponse, ContentTransferConfig, ContentTransferError,
};
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
use crate::modules::network::persistent_provider_registry::{
    PersistentProviderRegistry, ProviderRegistryConfig,
};
use crate::modules::network::storage::{RocksDbStore, StorageConfig};
use crate::modules::network::{behaviour::FlowBehaviour, config::NetworkConfig, keypair};
use crate::modules::storage::content::{
    Block, BlockStore, BlockStoreConfig, ContentId, DocumentManifest,
};
use errors::AppError;
use libp2p::gossipsub::{self, IdentTopic};
use libp2p::kad::{QueryId, QueryResult, RecordKey};
use libp2p::multiaddr::Protocol;
use libp2p::request_response::{self, OutboundRequestId};
use libp2p::{Multiaddr, PeerId, Swarm, futures::StreamExt, identity::Keypair, noise, tcp, yamux};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant as StdInstant};
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, instrument, warn};

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

/// Result of a provider announcement
#[derive(Debug, Clone)]
pub struct ProviderAnnounceResult {
    /// The Kademlia query ID
    pub query_id: QueryId,
}

/// Callback for provider announcement confirmation.
/// Called with (cid, result) when DHT announcement completes.
pub type ProviderConfirmationCallback = Arc<dyn Fn(ContentId, Result<(), String>) + Send + Sync>;

/// Tracks a pending start_providing query for confirmation callbacks.
struct PendingProviderAnnouncement {
    /// The CID being announced
    cid: ContentId,
    /// Optional callback to invoke on completion
    callback: Option<ProviderConfirmationCallback>,
    /// When the announcement was initiated
    created_at: std::time::Instant,
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

    /// Request a block from a peer
    RequestBlock {
        peer_id: PeerId,
        cid: ContentId,
        response: oneshot::Sender<Result<Block, ContentTransferError>>,
    },

    /// Request a manifest from a peer  
    RequestManifest {
        peer_id: PeerId,
        cid: ContentId,
        response: oneshot::Sender<Result<DocumentManifest, ContentTransferError>>,
    },

    /// Start providing content (DHT provider announcement)
    StartProviding {
        cid: ContentId,
        response: oneshot::Sender<Result<QueryId, String>>,
    },

    /// Stop providing content
    StopProviding {
        cid: ContentId,
        response: oneshot::Sender<Result<(), String>>,
    },

    /// Start providing content with optional confirmation callback
    StartProvidingWithCallback {
        cid: ContentId,
        callback: Option<ProviderConfirmationCallback>,
        response: oneshot::Sender<Result<QueryId, String>>,
    },

    /// Query DHT for content providers
    GetProviders {
        cid: ContentId,
        response: oneshot::Sender<Result<HashSet<PeerId>, String>>,
    },

    /// Check local block store for a block
    GetLocalBlock {
        cid: ContentId,
        response: oneshot::Sender<Option<Block>>,
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

    /// Connections rejected due to limits - don't decrement when they close
    rejected_connections: HashSet<PeerId>,

    /// Block store for serving content
    block_store: Option<Arc<BlockStore>>,

    /// Pending outbound content requests
    pending_content_requests: HashMap<OutboundRequestId, PendingContentRequest>,

    /// Content transfer configuration
    content_transfer_config: ContentTransferConfig,

    /// Pending provider discovery queries (QueryId → pending query state)
    pending_provider_queries: HashMap<QueryId, PendingProviderQuery>,

    /// Persistent provider registry (CIDs we're providing)
    provider_registry: Arc<PersistentProviderRegistry>,

    /// Pending provider announcement queries (for confirmation callbacks)
    pending_provider_announcements: HashMap<QueryId, PendingProviderAnnouncement>,

    /// Provider configuration
    provider_config: ProviderConfig,
}

/// Tracks a pending content request.
struct PendingContentRequest {
    /// Channel to send result back to caller.
    response_tx: oneshot::Sender<Result<ContentResponse, ContentTransferError>>,
    /// The CID being requested.
    cid: ContentId,
    /// The peer we're requesting from.
    peer_id: PeerId,
    /// When the request was created.
    created_at: std::time::Instant,
}

/// Tracks a pending get_providers DHT query.
struct PendingProviderQuery {
    /// Channel to send discovered providers back to caller.
    response_tx: oneshot::Sender<Result<HashSet<PeerId>, String>>,
    /// The CID being queried.
    cid: ContentId,
    /// Accumulated providers discovered so far.
    providers: HashSet<PeerId>,
    /// When the query was started.
    created_at: std::time::Instant,
}

/// Result of provider discovery
#[derive(Debug, Clone)]
pub struct ProviderDiscoveryResult {
    /// The CID that was queried
    pub cid: ContentId,
    /// Discovered provider peer IDs
    pub providers: Vec<PeerId>,
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
        let content_transfer_config = ContentTransferConfig::from_env();

        // Create the behaviour
        let mut behaviour = FlowBehaviour::new(
            local_peer_id,
            &keypair,
            store,
            if config.mdns.enabled {
                Some(&config.mdns)
            } else {
                None
            },
            Some(&config.gossipsub),
            Some(&content_transfer_config),
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
        // Guard check
        self.ensure_not_started().await?;

        info!("Starting NetworkManager event loop");

        // Build all components

        let content_transfer_config = ContentTransferConfig::from_env();

        let block_store = self.create_block_store(&content_transfer_config);
        let listen_addr = Self::resolve_listen_address(config)?;
        let swarm = self.build_configured_swarm(config).await?;
        let command_rx = self.take_command_receiver().await?;
        let event_loop = self.create_event_loop(
            swarm,
            command_rx,
            config,
            Arc::clone(&self.message_router),
            block_store,
            content_transfer_config,
        )?;

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

    fn create_block_store(
        &self,
        content_transfer_config: &ContentTransferConfig,
    ) -> Option<Arc<BlockStore>> {
        let block_store = if content_transfer_config.enabled {
            let block_store_config = BlockStoreConfig::from_env();
            match BlockStore::new(block_store_config) {
                Ok(store) => {
                    info!("Block store initialized for content transfer");
                    Some(Arc::new(store))
                }
                Err(e) => {
                    warn!(error = %e, "Failed to initialize block store, content transfer disabled");
                    None
                }
            }
        } else {
            debug!("Content transfer disabled, skipping block store initialization");
            None
        };

        block_store
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
        block_store: Option<Arc<BlockStore>>,
        content_transfer_config: ContentTransferConfig,
    ) -> Result<NetworkEventLoop, AppError> {
        let peer_registry_config = PeerRegistryConfig::from_env();
        let mut persistent_registry =
            PersistentPeerRegistry::new(peer_registry_config).map_err(|e| {
                AppError::Network(format!("Failed to create persistent peer registry: {}", e))
            })?;

        persistent_registry.start_background_flush();

        // Create provider registry
        let provider_registry_config = ProviderRegistryConfig::from(&config.provider);
        let provider_registry =
            PersistentProviderRegistry::new(provider_registry_config).map_err(|e| {
                AppError::Network(format!(
                    "Failed to create persistent provider registry: {}",
                    e
                ))
            })?;

        info!(
            provided_cid_count = provider_registry.count(),
            "Provider registry initialized"
        );

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
            rejected_connections: HashSet::new(),
            block_store,
            pending_content_requests: HashMap::new(),
            content_transfer_config,
            pending_provider_queries: HashMap::new(),
            provider_registry: Arc::new(provider_registry),
            pending_provider_announcements: HashMap::new(),
            provider_config: config.provider.clone(),
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

    /// Request a block from a remote peer.
    ///
    /// # Arguments
    /// * `peer_id` - The peer to request from
    /// * `cid` - Content ID of the block
    ///
    /// # Returns
    /// The block data if found, or an error.
    #[instrument(skip(self), fields(peer = %peer_id, cid = %cid))]
    pub async fn request_block(
        &self,
        peer_id: PeerId,
        cid: ContentId,
    ) -> Result<Block, ContentTransferError> {
        self.ensure_started()
            .await
            .map_err(|_| ContentTransferError::NotStarted)?;

        let (response_tx, response_rx) = oneshot::channel();

        self.command_tx
            .send(NetworkCommand::RequestBlock {
                peer_id,
                cid: cid.clone(),
                response: response_tx,
            })
            .map_err(|e| ContentTransferError::Internal {
                message: format!("Failed to send command: {}", e),
            })?;

        response_rx
            .await
            .map_err(|e| ContentTransferError::Internal {
                message: format!("Response channel closed: {}", e),
            })?
    }

    /// Request a manifest from a remote peer.
    ///
    /// # Arguments
    /// * `peer_id` - The peer to request from
    /// * `cid` - Content ID of the manifest (root CID)
    ///
    /// # Returns
    /// The parsed manifest if found, or an error.
    #[instrument(skip(self), fields(peer = %peer_id, cid = %cid))]
    pub async fn request_manifest(
        &self,
        peer_id: PeerId,
        cid: ContentId,
    ) -> Result<DocumentManifest, ContentTransferError> {
        self.ensure_started()
            .await
            .map_err(|_| ContentTransferError::NotStarted)?;

        let (response_tx, response_rx) = oneshot::channel();

        self.command_tx
            .send(NetworkCommand::RequestManifest {
                peer_id,
                cid: cid.clone(),
                response: response_tx,
            })
            .map_err(|e| ContentTransferError::Internal {
                message: format!("Failed to send command: {}", e),
            })?;

        response_rx
            .await
            .map_err(|e| ContentTransferError::Internal {
                message: format!("Response channel closed: {}", e),
            })?
    }

    /// Request a block with retry logic.
    ///
    /// Automatically retries on transient failures.
    pub async fn request_block_with_retry(
        &self,
        peer_id: PeerId,
        cid: ContentId,
        max_retries: u32,
        retry_delay: Duration,
    ) -> Result<Block, ContentTransferError> {
        let mut last_error = None;

        for attempt in 0..=max_retries {
            if attempt > 0 {
                debug!(attempt, cid = %cid, "Retrying block request");
                tokio::time::sleep(retry_delay).await;
            }

            match self.request_block(peer_id, cid.clone()).await {
                Ok(block) => return Ok(block),
                Err(e) => {
                    // Don't retry on NotFound - it's definitive
                    if matches!(e, ContentTransferError::NotFound { .. }) {
                        return Err(e);
                    }
                    warn!(
                        attempt,
                        max_retries,
                        error = %e,
                        "Block request failed, will retry"
                    );
                    last_error = Some(e);
                }
            }
        }

        Err(
            last_error.unwrap_or(ContentTransferError::RetriesExhausted {
                attempts: max_retries + 1,
            }),
        )
    }

    /// Announce this node as a provider for the given content.
    ///
    /// This registers us in the Kademlia DHT as a provider for the content,
    /// allowing other peers to discover that we have this content available.
    ///
    /// # Arguments
    /// * `cid` - The ContentId of the content we're providing
    ///
    /// # Returns
    /// * `Ok(ProviderAnnounceResult)` - The query was initiated successfully
    /// * `Err(AppError)` - Failed to initiate the query
    ///
    /// # Example
    ///
    /// let cid = ContentId::from_bytes(b"hello world");
    /// let result = manager.start_providing(cid).await?;
    /// info!("Provider announcement started: {:?}", result.query_id);
    #[instrument(skip(self), fields(cid = %cid))]
    pub async fn start_providing(
        &self,
        cid: ContentId,
    ) -> Result<ProviderAnnounceResult, AppError> {
        self.ensure_started().await?;

        let (response_tx, response_rx) = oneshot::channel();

        self.command_tx
            .send(NetworkCommand::StartProviding {
                cid: cid.clone(),
                response: response_tx,
            })
            .map_err(|_| AppError::Network("Failed to send start_providing command".to_string()))?;

        let query_id = response_rx
            .await
            .map_err(|_| AppError::Network("start_providing response channel closed".to_string()))?
            .map_err(|e| AppError::Network(format!("start_providing failed: {}", e)))?;

        info!(%cid, ?query_id, "Provider announcement initiated");

        Ok(ProviderAnnounceResult { query_id })
    }

    /// Start providing content with optional confirmation callback.
    ///
    /// The callback is invoked when the DHT confirms the provider record
    /// has been stored, or when the announcement fails or times out.
    #[instrument(skip(self, callback), fields(cid = %cid))]
    pub async fn start_providing_with_callback(
        &self,
        cid: ContentId,
        callback: Option<ProviderConfirmationCallback>,
    ) -> Result<ProviderAnnounceResult, AppError> {
        self.ensure_started().await?;

        let (response_tx, response_rx) = oneshot::channel();

        self.command_tx
            .send(NetworkCommand::StartProvidingWithCallback {
                cid: cid.clone(),
                callback,
                response: response_tx,
            })
            .map_err(|_| AppError::Network("Failed to send command".to_string()))?;

        let query_id = response_rx
            .await
            .map_err(|_| AppError::Network("Response channel closed".to_string()))?
            .map_err(|e| AppError::Network(format!("start_providing failed: {}", e)))?;

        info!(%cid, ?query_id, "Provider announcement with callback initiated");

        Ok(ProviderAnnounceResult { query_id })
    }

    /// Stop providing content (remove provider record from DHT).
    ///
    /// Note: This only removes the local announcement. Existing DHT records
    /// will expire naturally based on Kademlia's record TTL.
    #[instrument(skip(self), fields(cid = %cid))]
    pub async fn stop_providing(&self, cid: ContentId) -> Result<(), AppError> {
        self.ensure_started().await?;

        let (response_tx, response_rx) = oneshot::channel();

        self.command_tx
            .send(NetworkCommand::StopProviding {
                cid: cid.clone(),
                response: response_tx,
            })
            .map_err(|_| AppError::Network("Failed to send stop_providing command".to_string()))?;

        response_rx
            .await
            .map_err(|_| AppError::Network("stop_providing response channel closed".to_string()))?
            .map_err(|e| AppError::Network(format!("stop_providing failed: {}", e)))?;

        info!(%cid, "Stopped providing content");

        Ok(())
    }

    /// Discover peers that are providing the given content.
    ///
    /// Queries the Kademlia DHT to find peers that have announced themselves
    /// as providers for the content. This is useful for finding who has
    /// content before requesting it.
    ///
    /// # Arguments
    /// * `cid` - The ContentId to find providers for
    ///
    /// # Returns
    /// * `Ok(ProviderDiscoveryResult)` - Contains list of discovered provider PeerIds
    /// * `Err(AppError)` - Query failed or timed out
    #[instrument(skip(self), fields(cid = %cid))]
    pub async fn get_providers(&self, cid: ContentId) -> Result<ProviderDiscoveryResult, AppError> {
        self.ensure_started().await?;

        let (response_tx, response_rx) = oneshot::channel();

        self.command_tx
            .send(NetworkCommand::GetProviders {
                cid: cid.clone(),
                response: response_tx,
            })
            .map_err(|_| AppError::Network("Failed to send get_providers command".to_string()))?;

        let providers = response_rx
            .await
            .map_err(|_| AppError::Network("get_providers response channel closed".to_string()))?
            .map_err(|e| AppError::Network(format!("get_providers failed: {}", e)))?;

        let provider_count = providers.len();
        info!(%cid, provider_count, "Provider discovery completed");

        Ok(ProviderDiscoveryResult {
            cid,
            providers: providers.into_iter().collect(),
        })
    }

    /// Fetch a block from the network with automatic content routing.
    ///
    /// This method implements smart content discovery:
    /// 1. First checks the local BlockStore (if available)
    /// 2. If not found locally, queries DHT for providers
    /// 3. Attempts to fetch from discovered providers
    ///
    /// # Arguments
    /// * `cid` - The ContentId of the block to fetch
    ///
    /// # Returns
    /// * `Ok(Block)` - The block was found and retrieved
    /// * `Err(ContentTransferError)` - Block not found or all providers failed
    #[instrument(skip(self), fields(cid = %cid))]
    pub async fn fetch_block(&self, cid: ContentId) -> Result<Block, ContentTransferError> {
        self.ensure_started()
            .await
            .map_err(|_| ContentTransferError::NotStarted)?;

        // Step 1: Try local block store first
        // Note: We send a command to check since block_store is in the event loop
        debug!(%cid, "Attempting to fetch block - checking local store first");

        if let Some(block) = self.get_local_block(cid.clone()).await? {
            info!(%cid, "Block found in local store");
            return Ok(block);
        }

        // Step 2: Query DHT for providers
        debug!(%cid, "Block not in local store, querying DHT for providers");

        let discovery_result =
            self.get_providers(cid.clone())
                .await
                .map_err(|e| ContentTransferError::Internal {
                    message: format!("Provider discovery failed: {}", e),
                })?;

        if discovery_result.providers.is_empty() {
            warn!(%cid, "No providers found for content");
            return Err(ContentTransferError::NotFound {
                cid: cid.to_string(),
            });
        }

        info!(
            %cid,
            provider_count = discovery_result.providers.len(),
            "Found providers, attempting to fetch"
        );

        // Step 3: Try each provider until success
        let max_attempts = 3.min(discovery_result.providers.len());
        let mut last_error = None;

        for (i, peer_id) in discovery_result
            .providers
            .iter()
            .take(max_attempts)
            .enumerate()
        {
            debug!(
                %cid,
                %peer_id,
                attempt = i + 1,
                max_attempts,
                "Requesting block from provider"
            );

            match self.request_block(*peer_id, cid.clone()).await {
                Ok(block) => {
                    info!(%cid, %peer_id, "Successfully fetched block from provider");
                    return Ok(block);
                }
                Err(e) => {
                    warn!(
                        %cid,
                        %peer_id,
                        error = %e,
                        attempt = i + 1,
                        "Failed to fetch from provider"
                    );
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or(ContentTransferError::NotFound {
            cid: cid.to_string(),
        }))
    }

    /// Check if a block exists locally without network access
    async fn get_local_block(&self, cid: ContentId) -> Result<Option<Block>, ContentTransferError> {
        let (response_tx, response_rx) = oneshot::channel();

        self.command_tx
            .send(NetworkCommand::GetLocalBlock {
                cid,
                response: response_tx,
            })
            .map_err(|e| ContentTransferError::Internal {
                message: format!("Failed to send command: {}", e),
            })?;

        response_rx
            .await
            .map_err(|e| ContentTransferError::Internal {
                message: format!("Response channel closed: {}", e),
            })
    }

    /// Fetch a manifest from the network with automatic content routing.
    ///
    /// Similar to `fetch_block` but for manifests.
    #[instrument(skip(self), fields(cid = %cid))]
    pub async fn fetch_manifest(
        &self,
        cid: ContentId,
    ) -> Result<DocumentManifest, ContentTransferError> {
        self.ensure_started()
            .await
            .map_err(|_| ContentTransferError::NotStarted)?;

        debug!(%cid, "Fetching manifest via content routing");

        // Query DHT for providers
        let discovery_result =
            self.get_providers(cid.clone())
                .await
                .map_err(|e| ContentTransferError::Internal {
                    message: format!("Provider discovery failed: {}", e),
                })?;

        if discovery_result.providers.is_empty() {
            warn!(%cid, "No providers found for manifest");
            return Err(ContentTransferError::NotFound {
                cid: cid.to_string(),
            });
        }

        // Try each provider
        let max_attempts = 3.min(discovery_result.providers.len());
        let mut last_error = None;

        for (i, peer_id) in discovery_result
            .providers
            .iter()
            .take(max_attempts)
            .enumerate()
        {
            debug!(%cid, %peer_id, attempt = i + 1, "Requesting manifest from provider");

            match self.request_manifest(*peer_id, cid.clone()).await {
                Ok(manifest) => {
                    info!(%cid, %peer_id, "Successfully fetched manifest from provider");
                    return Ok(manifest);
                }
                Err(e) => {
                    warn!(%cid, %peer_id, error = %e, "Failed to fetch manifest from provider");
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or(ContentTransferError::NotFound {
            cid: cid.to_string(),
        }))
    }

    /// Announce that content has been published.
    ///
    /// This publishes a `ContentPublished` message to the `ContentAnnouncements` topic
    /// via GossipSub, allowing subscribers to proactively discover and fetch new content.
    ///
    /// # Arguments
    /// * `cid` - Content identifier of the published content
    /// * `name` - Human-readable name/filename
    /// * `size` - Size of the content in bytes
    /// * `content_type` - MIME type or content type descriptor
    ///
    /// # Example
    /// ```ignore
    /// let cid = ContentId::from_bytes(content_data);
    /// manager.announce_content_published(
    ///     cid,
    ///     "document.pdf".to_string(),
    ///     content_data.len() as u64,
    ///     "application/pdf".to_string(),
    /// ).await?;
    /// ```
    #[instrument(skip(self), fields(cid = %cid, name = %name, size = size))]
    pub async fn announce_content_published(
        &self,
        cid: ContentId,
        name: String,
        size: u64,
        content_type: String,
    ) -> Result<String, AppError> {
        self.ensure_started().await?;

        let payload = MessagePayload::ContentPublished {
            cid,
            name: name.clone(),
            size,
            content_type: content_type.clone(),
        };

        info!(
            content_name = %name,
            content_type = %content_type,
            size_bytes = size,
            "Announcing content publication via GossipSub"
        );

        self.publish_to_topic(&Topic::ContentAnnouncements, payload)
            .await
    }

    /// Publish content and announce it to the network.
    ///
    /// Convenience method that:
    /// 1. Announces provider status in the DHT via `start_providing()`
    /// 2. Publishes a `ContentPublished` message via GossipSub
    ///
    /// # Arguments
    /// * `cid` - Content identifier
    /// * `name` - Human-readable name
    /// * `size` - Content size in bytes
    /// * `content_type` - MIME type
    ///
    /// # Returns
    /// `(ProviderAnnounceResult, GossipSubMessageId)`
    #[instrument(skip(self), fields(cid = %cid, name = %name))]
    pub async fn publish_and_announce(
        &self,
        cid: ContentId,
        name: String,
        size: u64,
        content_type: String,
    ) -> Result<(ProviderAnnounceResult, String), AppError> {
        // 1. Announce in DHT
        let provider_result = self.start_providing(cid.clone()).await?;

        // 2. Announce via GossipSub
        let message_id = self
            .announce_content_published(cid, name, size, content_type)
            .await?;

        Ok((provider_result, message_id))
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

        // Reprovide timer
        let mut reprovide_timer = tokio::time::interval(self.provider_config.reprovide_interval());
        reprovide_timer.tick().await; // skip immediate tick

        let should_reprovide_on_startup = self.provider_config.reprovide_on_startup
            && self.provider_config.auto_reprovide_enabled
            && !self.provider_registry.is_empty();

        // Cleanup timer
        let mut cleanup_timer = tokio::time::interval(self.provider_config.cleanup_interval());
        cleanup_timer.tick().await; // skip immediate tick

        let mut startup_reprovide_done = false;

        loop {
            tokio::select! {
                // One-shot initial bootstrap trigger
                _ = &mut bootstrap_delay, if !self.bootstrap_initiated => {
                    self.execute_bootstrap();

                    if should_reprovide_on_startup && !startup_reprovide_done {
                        info!("Triggering startup reprovide after bootstrap");
                        self.reprovide_all();
                        startup_reprovide_done = true;
                    }
                }

                // Periodic retry of failed bootstrap peers
                _ = retry_interval.tick(), if self.bootstrap_initiated && !self.bootstrap_peer_ids.is_empty() => {
                    self.retry_bootstrap_peers();
                }

                _ = reprovide_timer.tick(),
                    if self.provider_config.auto_reprovide_enabled && !self.provider_registry.is_empty() =>
                {
                    self.reprovide_all();
                }

                _ = cleanup_timer.tick() => {
                    self.cleanup_stale_queries();
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
                let direction = if endpoint.is_dialer() {
                    ConnectionDirection::Outbound
                } else {
                    ConnectionDirection::Inbound
                };

                let should_disconnect = if self.connection_limits.max_connections == 0 {
                    false // Unlimited
                } else {
                    match self.connection_limits.policy {
                        ConnectionLimitPolicy::Strict => {
                            // Strict: reject ANY connection over max
                            self.active_connections >= self.connection_limits.max_connections
                        }
                        ConnectionLimitPolicy::PreferOutbound => {
                            // PreferOutbound: only reject INBOUND over inbound_limit
                            // Outbound can go up to max_connections
                            if direction == ConnectionDirection::Inbound {
                                let inbound_limit = self
                                    .connection_limits
                                    .max_connections
                                    .saturating_sub(self.connection_limits.reserved_outbound);
                                self.active_connections >= inbound_limit
                            } else {
                                // Outbound: only check against total max
                                self.active_connections >= self.connection_limits.max_connections
                            }
                        }
                    }
                };

                if should_disconnect {
                    self.rejected_connections.insert(peer_id);

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

                // Increment connection counter
                self.active_connections += 1;

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
                // Only decrement if this wasn't a rejected connection
                if self.rejected_connections.remove(&peer_id) {
                    debug!(peer_id = %peer_id, "Ignoring close for rejected connection");
                } else {
                    self.active_connections = self.active_connections.saturating_sub(1);
                }

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

            SwarmEvent::Behaviour(FlowBehaviourEvent::ContentTransfer(event)) => {
                self.handle_content_transfer_event(event).await;
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

            NetworkCommand::RequestBlock {
                peer_id,
                cid,
                response,
            } => {
                self.handle_request_block(peer_id, cid, response);
            }

            NetworkCommand::RequestManifest {
                peer_id,
                cid,
                response,
            } => {
                self.handle_request_manifest(peer_id, cid, response);
            }

            NetworkCommand::StartProviding { cid, response } => {
                let result = self.handle_start_providing(&cid);
                let _ = response.send(result);
            }

            NetworkCommand::StartProvidingWithCallback {
                cid,
                callback,
                response,
            } => {
                let result = self.handle_start_providing_impl(&cid, callback);
                let _ = response.send(result);
            }

            NetworkCommand::StopProviding { cid, response } => {
                let result = self.handle_stop_providing(&cid);
                let _ = response.send(result);
            }

            NetworkCommand::GetProviders { cid, response } => {
                self.handle_get_providers(cid, response);
            }

            NetworkCommand::GetLocalBlock { cid, response } => {
                let block = self
                    .block_store
                    .as_ref()
                    .and_then(|store| store.get(&cid).ok().flatten());
                let _ = response.send(block);
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

            Event::OutboundQueryProgressed { id, result, .. } => match result {
                QueryResult::GetClosestPeers(Ok(closest)) => {
                    debug!(
                        ?id,
                        peers = closest.peers.len(),
                        "GetClosestPeers completed"
                    );
                    for peer_info in &closest.peers {
                        self.peer_discovery_source
                            .insert(peer_info.peer_id, DiscoverySource::Dht);
                    }
                }

                QueryResult::StartProviding(Ok(add_provider_ok)) => {
                    info!(
                        query_id = ?id,
                        key = ?add_provider_ok.key,
                        "Successfully announced as provider"
                    );

                    // Invoke callback and update metadata if tracked
                    if let Some(pending) = self.pending_provider_announcements.remove(&id) {
                        let elapsed = pending.created_at.elapsed();
                        info!(
                            %pending.cid,
                            elapsed_ms = elapsed.as_millis(),
                            "Provider announcement confirmed"
                        );

                        // Update announcement metadata
                        if let Err(e) = self.provider_registry.record_announcement(&pending.cid) {
                            warn!(%pending.cid, error = %e, "Failed to record announcement");
                        }

                        // Invoke callback
                        if let Some(callback) = pending.callback {
                            callback(pending.cid, Ok(()));
                        }
                    }
                }

                QueryResult::StartProviding(Err(add_provider_err)) => {
                    warn!(
                        query_id = ?id,
                        key = ?add_provider_err.key(),
                        "Failed to announce as provider"
                    );

                    if let Some(pending) = self.pending_provider_announcements.remove(&id) {
                        if let Some(callback) = pending.callback {
                            callback(
                                pending.cid,
                                Err(format!(
                                    "Provider announcement failed for key {:?}",
                                    add_provider_err.key()
                                )),
                            );
                        }
                    }
                }

                QueryResult::GetProviders(Ok(get_providers_ok)) => match get_providers_ok {
                    libp2p::kad::GetProvidersOk::FoundProviders { key, providers } => {
                        debug!(
                            query_id = ?id,
                            key = ?key,
                            provider_count = providers.len(),
                            "GetProviders found providers"
                        );

                        // Accumulate providers in pending query
                        if let Some(pending) = self.pending_provider_queries.get_mut(&id) {
                            pending.providers.extend(providers);
                            debug!(
                                query_id = ?id,
                                total_providers = pending.providers.len(),
                                "Accumulated providers for query"
                            );
                        }
                    }
                    libp2p::kad::GetProvidersOk::FinishedWithNoAdditionalRecord {
                        closest_peers,
                    } => {
                        debug!(
                            query_id = ?id,
                            closest_peers_count = closest_peers.len(),
                            "GetProviders finished with no additional records"
                        );

                        // Complete the pending query
                        if let Some(pending) = self.pending_provider_queries.remove(&id) {
                            let provider_count = pending.providers.len();
                            let elapsed = pending.created_at.elapsed();

                            info!(
                                %pending.cid,
                                provider_count,
                                elapsed_ms = elapsed.as_millis(),
                                "Provider discovery completed"
                            );

                            let _ = pending.response_tx.send(Ok(pending.providers));
                        }
                    }
                },

                QueryResult::GetProviders(Err(get_providers_err)) => match get_providers_err {
                    libp2p::kad::GetProvidersError::Timeout { key, closest_peers } => {
                        debug!(
                            query_id = ?id,
                            key = ?key,
                            closest_peers_count = closest_peers.len(),
                            "GetProviders timed out"
                        );

                        // Complete with whatever providers we found (may be empty)
                        if let Some(pending) = self.pending_provider_queries.remove(&id) {
                            let provider_count = pending.providers.len();

                            if provider_count > 0 {
                                info!(
                                    %pending.cid,
                                    provider_count,
                                    "GetProviders timed out but found some providers"
                                );
                                let _ = pending.response_tx.send(Ok(pending.providers));
                            } else {
                                warn!(%pending.cid, "GetProviders timed out with no providers");
                                let _ = pending.response_tx.send(Err(format!(
                                    "Provider query timed out for {}",
                                    pending.cid
                                )));
                            }
                        }
                    }
                },

                QueryResult::Bootstrap(Ok(bootstrap_ok)) => {
                    debug!(
                        query_id = ?id,
                        peer = %bootstrap_ok.peer,
                        remaining = bootstrap_ok.num_remaining,
                        "Bootstrap query progressed"
                    );
                }

                QueryResult::Bootstrap(Err(e)) => {
                    warn!(query_id = ?id, error = ?e, "Bootstrap query failed");
                }

                other => {
                    debug!(query_id = ?id, result = ?other, "Kademlia query progressed");
                }
            },

            Event::InboundRequest { request } => {
                debug!(?request, "Received inbound Kademlia request");
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

    /// Handle content transfer request-response events
    async fn handle_content_transfer_event(
        &mut self,
        event: request_response::Event<ContentRequest, ContentResponse>,
    ) {
        use request_response::Event::*;
        use request_response::Message::*;

        match event {
            // Incoming request from a peer
            Message {
                peer,
                message: Request {
                    request, channel, ..
                },
                ..
            } => {
                info!(peer = %peer, request = ?request, "Received content request");
                self.handle_incoming_content_request(peer, request, channel)
                    .await;
            }

            // Response to our outbound request
            Message {
                peer,
                message:
                    Response {
                        request_id,
                        response,
                    },
                ..
            } => {
                debug!(peer = %peer, request_id = ?request_id, "Received content response");
                self.handle_content_response(request_id, response);
            }

            // Outbound request failed
            OutboundFailure {
                peer,
                request_id,
                error,
                ..
            } => {
                warn!(
                    peer = %peer,
                    request_id = ?request_id,
                    error = ?error,
                    "Content request failed"
                );
                self.handle_content_request_failure(request_id, error);
            }

            // Inbound request failed (we couldn't send response)
            InboundFailure {
                peer,
                request_id,
                error,
                ..
            } => {
                warn!(
                    peer = %peer,
                    request_id = ?request_id,
                    error = ?error,
                    "Failed to send content response"
                );
            }

            // Response sent successfully
            ResponseSent {
                peer, request_id, ..
            } => {
                debug!(peer = %peer, request_id = ?request_id, "Content response sent");
            }
        }
    }

    /// Handle incoming content request - serve from local BlockStore
    async fn handle_incoming_content_request(
        &mut self,
        peer: PeerId,
        request: ContentRequest,
        channel: request_response::ResponseChannel<ContentResponse>,
    ) {
        let response = match &self.block_store {
            Some(store) => {
                match &request {
                    ContentRequest::GetBlock { cid } => match store.get(cid) {
                        Ok(Some(block)) => {
                            info!(peer = %peer, cid = %cid, "Serving block");
                            ContentResponse::block(
                                cid.clone(),
                                block.data().to_vec(),
                                block.links().to_vec(),
                            )
                        }
                        Ok(None) => {
                            debug!(peer = %peer, cid = %cid, "Block not found");
                            ContentResponse::not_found(cid.clone())
                        }
                        Err(e) => {
                            error!(peer = %peer, cid = %cid, error = %e, "BlockStore error");
                            ContentResponse::error(
                                super::content_transfer::ErrorCode::InternalError,
                                format!("Storage error: {}", e),
                            )
                        }
                    },
                    ContentRequest::GetManifest { cid } => {
                        match store.get(cid) {
                            Ok(Some(block)) => {
                                // Manifest is stored as DAG-CBOR block
                                info!(peer = %peer, cid = %cid, "Serving manifest");
                                ContentResponse::manifest(cid.clone(), block.data().to_vec())
                            }
                            Ok(None) => {
                                debug!(peer = %peer, cid = %cid, "Manifest not found");
                                ContentResponse::not_found(cid.clone())
                            }
                            Err(e) => {
                                error!(peer = %peer, cid = %cid, error = %e, "BlockStore error");
                                ContentResponse::error(
                                    super::content_transfer::ErrorCode::InternalError,
                                    format!("Storage error: {}", e),
                                )
                            }
                        }
                    }
                }
            }
            None => {
                warn!(peer = %peer, "No BlockStore configured, cannot serve content");
                ContentResponse::error(
                    super::content_transfer::ErrorCode::InternalError,
                    "Content storage not available".to_string(),
                )
            }
        };

        if let Err(resp) = self
            .swarm
            .behaviour_mut()
            .send_content_response(channel, response)
        {
            error!(peer = %peer, "Failed to send content response: {:?}", resp);
        }
    }

    /// Handle successful content response
    fn handle_content_response(
        &mut self,
        request_id: OutboundRequestId,
        response: ContentResponse,
    ) {
        if let Some(pending) = self.pending_content_requests.remove(&request_id) {
            let _ = pending.response_tx.send(Ok(response));
        } else {
            warn!(request_id = ?request_id, "Received response for unknown request");
        }
    }

    /// Handle content request failure
    fn handle_content_request_failure(
        &mut self,
        request_id: OutboundRequestId,
        error: request_response::OutboundFailure,
    ) {
        if let Some(pending) = self.pending_content_requests.remove(&request_id) {
            let err = match error {
                request_response::OutboundFailure::DialFailure => {
                    ContentTransferError::PeerDisconnected {
                        peer_id: pending.peer_id.to_string(),
                    }
                }
                request_response::OutboundFailure::Timeout => ContentTransferError::Timeout {
                    timeout_secs: self.content_transfer_config.request_timeout_secs,
                },
                request_response::OutboundFailure::ConnectionClosed => {
                    ContentTransferError::PeerDisconnected {
                        peer_id: pending.peer_id.to_string(),
                    }
                }
                request_response::OutboundFailure::UnsupportedProtocols => {
                    ContentTransferError::RequestFailed {
                        message: "Peer does not support content transfer protocol".to_string(),
                    }
                }
                request_response::OutboundFailure::Io(e) => ContentTransferError::RequestFailed {
                    message: format!("I/O error: {}", e),
                },
            };
            let _ = pending.response_tx.send(Err(err));
        }
    }

    fn handle_request_block(
        &mut self,
        peer_id: PeerId,
        cid: ContentId,
        response_tx: oneshot::Sender<Result<Block, ContentTransferError>>,
    ) {
        let request = ContentRequest::get_block(cid.clone());

        // Wrap the block response sender in a ContentResponse handler
        let (content_tx, content_rx) = oneshot::channel();

        // Spawn a task to convert ContentResponse to Block
        let cid_clone = cid.clone();
        tokio::spawn(async move {
            let result = match content_rx.await {
                Ok(Ok(ContentResponse::Block { data, links, .. })) => {
                    match Block::from_parts(cid_clone.clone(), data, links) {
                        Ok(block) => Ok(block),
                        Err(e) => Err(ContentTransferError::RequestFailed {
                            message: format!("Invalid block data: {}", e),
                        }),
                    }
                }
                Ok(Ok(ContentResponse::NotFound { cid })) => Err(ContentTransferError::NotFound {
                    cid: cid.to_string(),
                }),
                Ok(Ok(ContentResponse::Error { code, message, .. })) => {
                    Err(ContentTransferError::RemoteError {
                        code: code.to_string(),
                        message,
                    })
                }
                Ok(Ok(_)) => Err(ContentTransferError::RequestFailed {
                    message: "Unexpected response type".to_string(),
                }),
                Ok(Err(e)) => Err(e),
                Err(_) => Err(ContentTransferError::Internal {
                    message: "Response channel closed".to_string(),
                }),
            };
            let _ = response_tx.send(result);
        });

        self.send_content_request(peer_id, cid, request, content_tx);
    }

    fn handle_request_manifest(
        &mut self,
        peer_id: PeerId,
        cid: ContentId,
        response_tx: oneshot::Sender<Result<DocumentManifest, ContentTransferError>>,
    ) {
        let request = ContentRequest::get_manifest(cid.clone());

        let (content_tx, content_rx) = oneshot::channel();

        tokio::spawn(async move {
            let result = match content_rx.await {
                Ok(Ok(ContentResponse::Manifest { data, .. })) => {
                    match DocumentManifest::from_dag_cbor(&data) {
                        Ok(manifest) => Ok(manifest),
                        Err(e) => Err(ContentTransferError::RequestFailed {
                            message: format!("Invalid manifest data: {}", e),
                        }),
                    }
                }
                Ok(Ok(ContentResponse::NotFound { cid })) => Err(ContentTransferError::NotFound {
                    cid: cid.to_string(),
                }),
                Ok(Ok(ContentResponse::Error { code, message, .. })) => {
                    Err(ContentTransferError::RemoteError {
                        code: code.to_string(),
                        message,
                    })
                }
                Ok(Ok(_)) => Err(ContentTransferError::RequestFailed {
                    message: "Unexpected response type".to_string(),
                }),
                Ok(Err(e)) => Err(e),
                Err(_) => Err(ContentTransferError::Internal {
                    message: "Response channel closed".to_string(),
                }),
            };
            let _ = response_tx.send(result);
        });

        self.send_content_request(peer_id, cid, request, content_tx);
    }

    fn send_content_request(
        &mut self,
        peer_id: PeerId,
        cid: ContentId,
        request: ContentRequest,
        response_tx: oneshot::Sender<Result<ContentResponse, ContentTransferError>>,
    ) {
        match self
            .swarm
            .behaviour_mut()
            .send_content_request(&peer_id, request)
        {
            Some(request_id) => {
                self.pending_content_requests.insert(
                    request_id,
                    PendingContentRequest {
                        response_tx,
                        cid,
                        peer_id,
                        created_at: std::time::Instant::now(),
                    },
                );
            }
            None => {
                let _ = response_tx.send(Err(ContentTransferError::RequestFailed {
                    message: "Content transfer not enabled".to_string(),
                }));
            }
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

    /// Convert a ContentId to a Kademlia RecordKey.
    ///
    /// Uses the raw CID bytes as the key, ensuring content-addressable
    /// lookups work correctly across the DHT.
    fn cid_to_record_key(cid: &ContentId) -> RecordKey {
        RecordKey::new(&cid.to_bytes())
    }

    /// Handle get_providers command - initiate DHT provider query
    fn handle_get_providers(
        &mut self,
        cid: ContentId,
        response: oneshot::Sender<Result<HashSet<PeerId>, String>>,
    ) {
        let key = Self::cid_to_record_key(&cid);

        debug!(%cid, key = ?key, "Starting provider discovery query");

        let query_id = self.swarm.behaviour_mut().get_providers(key);

        info!(
            %cid,
            ?query_id,
            "Provider discovery query initiated"
        );

        self.pending_provider_queries.insert(
            query_id,
            PendingProviderQuery {
                response_tx: response,
                cid,
                providers: HashSet::new(),
                created_at: std::time::Instant::now(),
            },
        );
    }

    fn reprovide_all(&mut self) {
        let cids = self.provider_registry.get_all();
        let cid_count = cids.len();

        if cid_count == 0 {
            debug!("No content to reprovide");
            return;
        }

        info!(cid_count, "Starting auto-reprovide cycle");

        let mut success_count = 0;
        let mut error_count = 0;

        for cid in cids {
            let key = Self::cid_to_record_key(&cid);

            match self.swarm.behaviour_mut().start_providing(key) {
                Ok(query_id) => {
                    debug!(%cid, ?query_id, "Reprovide initiated");
                    success_count += 1;
                }
                Err(e) => {
                    warn!(%cid, error = ?e, "Failed to reprovide");
                    error_count += 1;
                }
            }
        }

        info!(
            cid_count,
            success_count, error_count, "Auto-reprovide cycle completed"
        );
    }

    /// Clean up stale pending queries.
    fn cleanup_stale_queries(&mut self) {
        let timeout = self.provider_config.query_timeout();
        let now = std::time::Instant::now();

        // Cleanup stale get_providers queries
        let stale_provider_queries: Vec<QueryId> = self
            .pending_provider_queries
            .iter()
            .filter(|(_, pending)| now.duration_since(pending.created_at) > timeout)
            .map(|(id, _)| *id)
            .collect();

        for query_id in &stale_provider_queries {
            if let Some(pending) = self.pending_provider_queries.remove(query_id) {
                let elapsed = now.duration_since(pending.created_at);
                warn!(
                    ?query_id,
                    %pending.cid,
                    elapsed_secs = elapsed.as_secs(),
                    "Cleaning up stale get_providers query"
                );
                let _ = pending.response_tx.send(Err(format!(
                    "Provider query timed out for {} after {}s",
                    pending.cid,
                    elapsed.as_secs()
                )));
            }
        }

        // Cleanup stale provider announcement queries
        let stale_announcements: Vec<QueryId> = self
            .pending_provider_announcements
            .iter()
            .filter(|(_, pending)| now.duration_since(pending.created_at) > timeout)
            .map(|(id, _)| *id)
            .collect();

        for query_id in &stale_announcements {
            if let Some(pending) = self.pending_provider_announcements.remove(query_id) {
                let elapsed = now.duration_since(pending.created_at);
                warn!(
                    ?query_id,
                    %pending.cid,
                    elapsed_secs = elapsed.as_secs(),
                    "Cleaning up stale provider announcement"
                );
                if let Some(callback) = pending.callback {
                    callback(
                        pending.cid,
                        Err(format!(
                            "Provider announcement timed out after {}s",
                            elapsed.as_secs()
                        )),
                    );
                }
            }
        }

        // Cleanup stale content transfer outbound requests, if you track them similarly:
        let stale_content_requests: Vec<OutboundRequestId> = self
            .pending_content_requests
            .iter()
            .filter(|(_, pending)| now.duration_since(pending.created_at) > timeout)
            .map(|(id, _)| *id)
            .collect();

        for request_id in &stale_content_requests {
            if let Some(pending) = self.pending_content_requests.remove(request_id) {
                warn!(
                    ?request_id,
                    %pending.cid,
                    peer = %pending.peer_id,
                    "Cleaning up stale content request"
                );
                let _ = pending
                    .response_tx
                    .send(Err(ContentTransferError::RequestFailed {
                        message: format!("Request timed out after {}s", timeout.as_secs()),
                    }));
            }
        }

        let total_cleaned =
            stale_provider_queries.len() + stale_announcements.len() + stale_content_requests.len();

        if total_cleaned > 0 {
            info!(
                provider_queries = stale_provider_queries.len(),
                announcements = stale_announcements.len(),
                content_requests = stale_content_requests.len(),
                "Cleaned up stale queries"
            );
        }
    }

    /// Handle start_providing command (no callback).
    fn handle_start_providing(&mut self, cid: &ContentId) -> Result<QueryId, String> {
        self.handle_start_providing_impl(cid, None)
    }

    /// Handle start_providing command with optional callback.
    fn handle_start_providing_impl(
        &mut self,
        cid: &ContentId,
        callback: Option<ProviderConfirmationCallback>,
    ) -> Result<QueryId, String> {
        let key = Self::cid_to_record_key(cid);
        debug!(%cid, key = ?key, "Starting to provide content");

        match self.swarm.behaviour_mut().start_providing(key.clone()) {
            Ok(query_id) => {
                info!(%cid, ?query_id, "Provider announcement query started");

                // Add to persistent provider registry
                if let Err(e) = self.provider_registry.add(cid.clone()) {
                    error!(%cid, error = %e, "Failed to persist provided CID");
                    // DHT announcement still proceeds
                }

                // Track for callback if provided
                if callback.is_some() {
                    self.pending_provider_announcements.insert(
                        query_id,
                        PendingProviderAnnouncement {
                            cid: cid.clone(),
                            callback,
                            created_at: std::time::Instant::now(),
                        },
                    );
                    debug!(%cid, ?query_id, "Registered callback for provider announcement");
                }

                debug!(
                    %cid,
                    total_provided = self.provider_registry.count(),
                    "Added to provider registry"
                );

                Ok(query_id)
            }
            Err(e) => {
                warn!(%cid, error = ?e, "Failed to start providing");
                Err(format!("Failed to start providing: {:?}", e))
            }
        }
    }

    /// Handle stop_providing command.
    fn handle_stop_providing(&mut self, cid: &ContentId) -> Result<(), String> {
        let key = Self::cid_to_record_key(cid);
        debug!(%cid, key = ?key, "Stopping providing content");

        self.swarm.behaviour_mut().stop_providing(&key);

        // Remove from persistent provider registry
        match self.provider_registry.remove(cid) {
            Ok(was_present) => {
                info!(
                    %cid,
                    was_present,
                    total_provided = self.provider_registry.count(),
                    "Stopped providing content"
                );
            }
            Err(e) => {
                error!(%cid, error = %e, "Failed to remove from provider registry");
            }
        }

        Ok(())
    }
}
