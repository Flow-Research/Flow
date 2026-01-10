//! Network Manager - High-level P2P networking interface
//!
//! Provides thread-safe access to libp2p networking operations.

mod commands;
mod event_loop;
mod types;

use libp2p::identity::Keypair;
use libp2p::multiaddr::Protocol;
pub use types::{
    BootstrapResult, ConnectionCapacityStatus, ProviderAnnounceResult,
    ProviderConfirmationCallback, ProviderDiscoveryResult,
};

use crate::bootstrap::init::NodeData;
use crate::modules::network::behaviour::FlowBehaviour;
use crate::modules::network::config::NetworkConfig;
use crate::modules::network::content_transfer::{ContentTransferConfig, ContentTransferError};
use crate::modules::network::gossipsub::{
    GossipSubConfig, Message, MessagePayload, MessageRouter, MessageStore, MessageStoreConfig,
    RouterStats, SubscriptionHandle, Topic, TopicSubscriptionManager,
};
use crate::modules::network::keypair;
use crate::modules::network::peer_registry::{NetworkStats, PeerInfo};
use crate::modules::network::persistent_peer_registry::{
    PeerRegistryConfig, PersistentPeerRegistry,
};
use crate::modules::network::persistent_provider_registry::{
    PersistentProviderRegistry, ProviderRegistryConfig,
};
use crate::modules::network::storage::{RocksDbStore, StorageConfig};
use crate::modules::storage::content::{
    Block, BlockStore, BlockStoreConfig, ContentId, DocumentManifest,
};
use commands::NetworkCommand;
use errors::AppError;
use event_loop::NetworkEventLoop;
use libp2p::{Multiaddr, PeerId, Swarm, noise, tcp, yamux};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, instrument, warn};

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

    /// Optional injected block store
    block_store: Option<Arc<BlockStore>>,
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
        Self::with(node_data, None).await
    }

    /// Create a new NetworkManager with an injected BlockStore
    ///
    /// Use this when you need to share the BlockStore with other components
    /// (e.g., NetworkProvider). If block_store is None and content transfer
    /// is enabled, one will be created internally during start().
    pub async fn with(
        node_data: &NodeData,
        block_store: Option<Arc<BlockStore>>,
    ) -> Result<Self, AppError> {
        info!("Initializing NetworkManager");

        let keypair = keypair::ed25519_to_libp2p_keypair(&node_data.private_key)?;
        let local_peer_id = keypair.public().to_peer_id();

        info!(peer_id = %local_peer_id, "Network PeerId initialized");

        let (command_tx, command_rx) = mpsc::unbounded_channel();
        let (message_router, subscription_manager) = Self::create_message_routing()?;

        if block_store.is_some() {
            info!("NetworkManager initialized with injected BlockStore");
        }

        Ok(Self {
            local_peer_id,
            node_data: node_data.clone(),
            command_tx,
            command_rx: Arc::new(Mutex::new(Some(command_rx))),
            event_loop_handle: Arc::new(Mutex::new(None)),
            message_router,
            subscription_manager,
            block_store,
        })
    }

    pub fn block_store(&self) -> Option<Arc<BlockStore>> {
        self.block_store.clone()
    }

    /// Resolve which BlockStore to use:
    /// 1. If injected at construction, use that
    /// 2. If content transfer enabled, create one
    /// 3. Otherwise, None
    fn resolve_block_store(
        &self,
        content_transfer_config: &ContentTransferConfig,
    ) -> Option<Arc<BlockStore>> {
        // Injected BlockStore takes precedence
        if let Some(store) = &self.block_store {
            info!("Using injected BlockStore for content transfer");
            return Some(Arc::clone(store));
        }

        // Fallback: create internally if enabled
        self.create_block_store(content_transfer_config)
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

        let block_store = self.resolve_block_store(&content_transfer_config);
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
            .map_err(|e| {
                error!(error = ?e, "Listen failed");
                AppError::Network(format!("Failed to listen: {:?}", e))
            })?;

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
        self.send_command(NetworkCommand::GetPeerCount, "get peer count")
            .await
    }

    /// Get list of all connected peers
    pub async fn connected_peers(&self) -> Result<Vec<PeerInfo>, AppError> {
        self.send_command(NetworkCommand::GetConnectedPeers, "get connected peers")
            .await
    }

    /// Get information about a specific peer
    pub async fn peer_info(&self, peer_id: PeerId) -> Result<Option<PeerInfo>, AppError> {
        self.send_command(
            |tx| NetworkCommand::GetPeerInfo { peer_id, response: tx },
            "get peer info",
        )
        .await
    }

    /// Dial a peer at a specific address
    pub async fn dial_peer(&self, address: Multiaddr) -> Result<(), AppError> {
        self.send_fallible_command(
            |tx| NetworkCommand::DialPeer { address, response: tx },
            "dial peer",
        )
        .await
    }

    /// Disconnect from a peer
    pub async fn disconnect_peer(&self, peer_id: PeerId) -> Result<(), AppError> {
        self.send_fallible_command(
            |tx| NetworkCommand::DisconnectPeer { peer_id, response: tx },
            "disconnect peer",
        )
        .await
    }

    /// Get network statistics
    pub async fn network_stats(&self) -> Result<NetworkStats, AppError> {
        self.send_command(NetworkCommand::GetNetworkStats, "get network stats")
            .await
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

    /// Helper to send a command and await its response.
    ///
    /// Combines ensure_started + channel creation + send + await into one call.
    /// Use this for simple request-response commands where the channel returns T directly.
    ///
    /// # Arguments
    /// * `make_command` - Closure that takes a response sender and returns the command
    /// * `action` - Description for error messages (e.g., "get peers", "dial")
    async fn send_command<T, F>(&self, make_command: F, action: &str) -> Result<T, AppError>
    where
        F: FnOnce(oneshot::Sender<T>) -> NetworkCommand,
    {
        self.ensure_started().await?;
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(make_command(tx))
            .map_err(|_| AppError::Network(format!("Failed to send {} command", action)))?;
        rx.await
            .map_err(|_| AppError::Network(format!("Failed to receive {} response", action)))
    }

    /// Helper for commands that return Result<T, String> through the channel.
    ///
    /// Like send_command, but for commands where the event loop can fail with a String error.
    async fn send_fallible_command<T, F>(&self, make_command: F, action: &str) -> Result<T, AppError>
    where
        F: FnOnce(oneshot::Sender<Result<T, String>>) -> NetworkCommand,
    {
        self.ensure_started().await?;
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(make_command(tx))
            .map_err(|_| AppError::Network(format!("Failed to send {} command", action)))?;
        rx.await
            .map_err(|_| AppError::Network(format!("Failed to receive {} response", action)))?
            .map_err(AppError::Network)
    }

    /// Get connection capacity status
    pub async fn connection_capacity(&self) -> Result<ConnectionCapacityStatus, AppError> {
        self.send_command(NetworkCommand::GetConnectionCapacity, "get connection capacity")
            .await
    }

    /// Manually trigger bootstrap process
    ///
    /// Dials all bootstrap peers and initiates Kademlia bootstrap query.
    /// Useful after network reconnection or configuration changes.
    pub async fn trigger_bootstrap(&self) -> Result<BootstrapResult, AppError> {
        self.send_command(|tx| NetworkCommand::TriggerBootstrap { response: tx }, "trigger bootstrap")
            .await
    }

    /// Get list of configured bootstrap peer IDs
    pub async fn bootstrap_peer_ids(&self) -> Result<Vec<PeerId>, AppError> {
        self.send_command(NetworkCommand::GetBootstrapPeerIds, "get bootstrap peers")
            .await
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
        self.send_fallible_command(
            |tx| NetworkCommand::Publish { message, response: tx },
            "publish message",
        )
        .await
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
        self.send_command(NetworkCommand::GetSubscriptions, "get subscriptions")
            .await
    }

    /// Get message router statistics
    pub async fn message_stats(&self) -> crate::modules::network::gossipsub::RouterStats {
        self.message_router.stats().await
    }

    /// Get mesh peers for a topic
    pub async fn mesh_peers(&self, topic: impl Into<String>) -> Result<Vec<PeerId>, AppError> {
        let topic = topic.into();
        self.send_command(
            |tx| NetworkCommand::GetMeshPeers { topic, response: tx },
            "get mesh peers",
        )
        .await
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
