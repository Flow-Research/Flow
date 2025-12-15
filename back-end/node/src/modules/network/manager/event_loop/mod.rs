//! Network event loop - processes swarm events and commands

mod bootstrap;
mod command_handler;
mod content_transfer_handler;
mod gossipsub_handler;
mod kademlia_handler;
mod mdns_handler;
mod provider_handler;
mod swarm_events;
use crate::modules::network::behaviour::FlowBehaviour;
use crate::modules::network::config::{BootstrapConfig, ConnectionLimits, ProviderConfig};
use crate::modules::network::content_transfer::ContentTransferConfig;
use crate::modules::network::gossipsub::{GossipSubConfig, MessageRouter};
use crate::modules::network::manager::commands::NetworkCommand;
use crate::modules::network::manager::types::{
    PendingContentRequest, PendingProviderAnnouncement, PendingProviderQuery,
};
use crate::modules::network::peer_registry::DiscoverySource;
use crate::modules::network::persistent_peer_registry::PersistentPeerRegistry;
use crate::modules::network::persistent_provider_registry::PersistentProviderRegistry;
use crate::modules::storage::content::BlockStore;
use futures::StreamExt;
use libp2p::kad::QueryId;
use libp2p::request_response::OutboundRequestId;
use libp2p::{PeerId, Swarm};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant as StdInstant};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

/// Configuration for event loop start up
struct StartupParams {
    /// One-shot bootstrap delay
    bootstrap_delay_ms: u64,
    /// Whether to reprovide on startup
    should_reprovide_on_startup: bool,
}

/// Internal event loop handling Swarm events
pub struct NetworkEventLoop {
    /// The libp2p Swarm managing P2P connections, protocols, and network behavior.
    /// Processes events from the transport layer and routes them to appropriate handlers.
    pub swarm: Swarm<FlowBehaviour>,

    /// Channel receiver for commands from NetworkManager API.
    /// Commands include peer queries, dial requests, disconnect operations, and shutdown signals.
    pub command_rx: mpsc::UnboundedReceiver<NetworkCommand>,

    /// Thread-safe persistent peer registry for storing connection history and peer metadata.
    /// Tracks discovered peers, connection statistics, and provides data persistence across restarts.
    pub peer_registry: Arc<PersistentPeerRegistry>,

    /// Track peers we're currently dialing to prevent duplicate attempts.
    /// A peer is added when dialing starts and removed when connection succeeds/fails.
    /// Prevents wasted resources and connection race conditions.
    pub dialing_peers: HashSet<PeerId>,

    /// Track when we last attempted to dial each peer (for rate limiting).
    /// Prevents hammering unreachable peers by enforcing minimum retry intervals.
    /// Entries may be cleaned up periodically to prevent unbounded growth.
    pub last_dial_attempt: HashMap<PeerId, StdInstant>,

    /// Track how we discovered each peer (mDNS, DHT, bootstrap, manual).
    /// Used to set the discovery_source field when connection is established,
    /// enabling analysis of discovery method effectiveness and network topology.
    pub peer_discovery_source: HashMap<PeerId, DiscoverySource>,

    /// Connection limits configuration (max connections, policy, reserved slots).
    /// Applied to inbound connection decisions to prevent resource exhaustion
    /// while maintaining capacity for critical outbound connections.
    pub connection_limits: ConnectionLimits,

    /// Track active connection count for enforcing connection limits.
    /// Incremented on ConnectionEstablished, decremented on ConnectionClosed.
    /// Used with connection_limits to gate new inbound connections.
    pub active_connections: usize,

    /// Bootstrap peer states for tracking connection attempts
    /// Set of peer IDs that are bootstrap peers (for retry prioritization)
    pub bootstrap_peer_ids: HashSet<PeerId>,

    /// Bootstrap configuration
    pub bootstrap_config: BootstrapConfig,

    /// Whether initial bootstrap has been triggered
    pub bootstrap_initiated: bool,

    /// Message router for persisting and delivering incoming messages
    pub message_router: Arc<MessageRouter>,

    /// GossipSub configuration for message validation
    pub gossipsub_config: GossipSubConfig,

    /// Connections rejected due to limits - don't decrement when they close
    pub rejected_connections: HashSet<PeerId>,

    /// Block store for serving content
    pub block_store: Option<Arc<BlockStore>>,

    /// Pending outbound content requests
    pub pending_content_requests: HashMap<OutboundRequestId, PendingContentRequest>,

    /// Content transfer configuration
    pub content_transfer_config: ContentTransferConfig,

    /// Pending provider discovery queries (QueryId → pending query state)
    pub pending_provider_queries: HashMap<QueryId, PendingProviderQuery>,

    /// Persistent provider registry (CIDs we're providing)
    pub provider_registry: Arc<PersistentProviderRegistry>,

    /// Pending provider announcement queries (for confirmation callbacks)
    pub pending_provider_announcements: HashMap<QueryId, PendingProviderAnnouncement>,

    /// Provider configuration
    pub provider_config: ProviderConfig,
}

impl NetworkEventLoop {
    /// Main event loop processing Swarm events and commands
    pub async fn run(&mut self) {
        info!("Network event loop started");

        self.run_event_loop().await;
        self.shutdown().await;

        info!("Network event loop stopped");
    }

    /// Core event loop with bootstrap scheduling and retry
    async fn run_event_loop(&mut self) {
        let params = self.init_startup_params();

        let bootstrap_delay = tokio::time::sleep(Duration::from_millis(params.bootstrap_delay_ms));
        tokio::pin!(bootstrap_delay);

        let mut retry_interval = tokio::time::interval(Duration::from_secs(5));
        let mut reprovide_timer = tokio::time::interval(self.provider_config.reprovide_interval());
        let mut cleanup_timer = tokio::time::interval(self.provider_config.cleanup_interval());

        // Skip immediate first ticks
        retry_interval.tick().await;
        reprovide_timer.tick().await;
        cleanup_timer.tick().await;

        let mut startup_reprovide_done = false;

        loop {
            tokio::select! {
                _ = &mut bootstrap_delay, if !self.bootstrap_initiated => {
                    self.on_bootstrap_trigger(&params, &mut startup_reprovide_done);
                }

                _ = retry_interval.tick(), if self.should_retry_bootstrap() => {
                    self.retry_bootstrap_peers();
                }

                _ = reprovide_timer.tick(), if self.should_reprovide() => {
                    self.reprovide_all();
                }

                _ = cleanup_timer.tick() => {
                    self.cleanup_stale_queries();
                }

                event = self.swarm.select_next_some() => {
                    self.handle_swarm_event(event).await;
                }

                Some(command) = self.command_rx.recv() => {
                    if self.handle_command(command).await {
                        break;
                    }
                }
            }
        }
    }

    /// Initialize timer configuration
    fn init_startup_params(&self) -> StartupParams {
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

        let should_reprovide_on_startup = self.provider_config.reprovide_on_startup
            && self.provider_config.auto_reprovide_enabled
            && !self.provider_registry.is_empty();

        StartupParams {
            bootstrap_delay_ms,
            should_reprovide_on_startup,
        }
    }

    /// Handle bootstrap trigger event
    fn on_bootstrap_trigger(&mut self, params: &StartupParams, startup_reprovide_done: &mut bool) {
        self.execute_bootstrap();

        if params.should_reprovide_on_startup && !*startup_reprovide_done {
            info!("Triggering startup reprovide after bootstrap");
            self.reprovide_all();
            *startup_reprovide_done = true;
        }
    }

    /// Check if bootstrap retry should run
    fn should_retry_bootstrap(&self) -> bool {
        self.bootstrap_initiated && !self.bootstrap_peer_ids.is_empty()
    }

    /// Check if reprovide should run
    fn should_reprovide(&self) -> bool {
        self.provider_config.auto_reprovide_enabled && !self.provider_registry.is_empty()
    }

    /// Graceful shutdown
    async fn shutdown(&mut self) {
        info!("Flushing persistent peer registry before shutdown");
        if let Err(e) = self.peer_registry.flush() {
            warn!(error = %e, "Failed to flush peer registry on shutdown");
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
