use super::peer_registry::{ConnectionDirection, NetworkStats, PeerInfo, PeerRegistry};
use super::storage::peer_registry_store::PeerRegistryStore;
use errors::AppError;
use libp2p::{Multiaddr, PeerId};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

/// Configuration for persistent peer registry
#[derive(Debug, Clone)]
pub struct PeerRegistryConfig {
    /// Path to RocksDB directory
    pub db_path: std::path::PathBuf,
    /// Interval for background flush task (in seconds)
    pub flush_interval_secs: u64,
}

impl Default for PeerRegistryConfig {
    fn default() -> Self {
        Self {
            db_path: crate::bootstrap::init::get_flow_data_dir()
                .join("network")
                .join("peer_registry"),
            flush_interval_secs: 30, // Flush every 30 seconds
        }
    }
}

impl PeerRegistryConfig {
    /// Load configuration from environment variables
    pub fn from_env() -> Self {
        let db_path = std::env::var("PEER_REGISTRY_DB_PATH")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| Self::default().db_path);

        let flush_interval_secs = std::env::var("PEER_REGISTRY_FLUSH_INTERVAL")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(30);

        Self {
            db_path,
            flush_interval_secs,
        }
    }
}

/// Persistent peer registry with RocksDB backing
///
/// This wraps the in-memory PeerRegistry and persists all changes to RocksDB.
/// On startup, it loads historical peer data to enable faster reconnections.
pub struct PersistentPeerRegistry {
    /// Inner in-memory registry
    inner: Arc<RwLock<PeerRegistry>>,
    /// Persistent store
    store: Arc<PeerRegistryStore>,
    /// Configuration
    config: PeerRegistryConfig,
    /// Shutdown signal sender
    shutdown_tx: Option<oneshot::Sender<()>>,
    /// Background flush task handle
    flush_task: Option<JoinHandle<()>>,
}

impl PersistentPeerRegistry {
    /// Create a new persistent peer registry
    ///
    /// This loads historical data from RocksDB and starts a background flush task.
    pub fn new(config: PeerRegistryConfig) -> Result<Self, AppError> {
        info!(
            "Initializing persistent peer registry at: {:?}",
            config.db_path
        );

        // Open RocksDB store
        let store = PeerRegistryStore::new(&config.db_path)?;
        let store = Arc::new(store);

        // Create in-memory registry
        let mut registry = PeerRegistry::new();

        // Load persisted data
        match store.load_known_peers() {
            Ok(known_peers) => {
                info!("Loaded {} known peers from disk", known_peers.len());
                // Inject known peers into the registry
                // Note: We'll need to add a method to PeerRegistry to bulk-load this data
                for (peer_id, addresses) in known_peers {
                    for addr in addresses {
                        // Use a dummy connection/disconnection to populate known_peers
                        // without making the peer "active"
                        registry.on_connection_established(
                            peer_id,
                            addr.clone(),
                            ConnectionDirection::Outbound,
                        );
                        registry.on_connection_closed(&peer_id);
                    }
                }
            }
            Err(e) => warn!("Failed to load known peers: {}", e),
        }

        // Load reconnection counts
        match store.load_reconnection_counts() {
            Ok(counts) => {
                info!("Loaded {} reconnection counts from disk", counts.len());
                // We'll need to add a method to inject this data
                // For now, log it (will implement bulk_load method next)
            }
            Err(e) => warn!("Failed to load reconnection counts: {}", e),
        }

        let inner = Arc::new(RwLock::new(registry));

        Ok(Self {
            inner,
            store,
            config,
            shutdown_tx: None,
            flush_task: None,
        })
    }

    /// Start background flush task
    ///
    /// This periodically flushes the RocksDB WAL to disk for durability.
    pub fn start_background_flush(&mut self) {
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();

        let store = Arc::clone(&self.store);
        let interval_secs = self.config.flush_interval_secs;

        let task = tokio::spawn(async move {
            info!(
                "Starting background flush task (interval: {}s)",
                interval_secs
            );
            let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        debug!("Background flush triggered");
                        if let Err(e) = store.flush() {
                            error!("Background flush failed: {}", e);
                        }
                    }
                    _ = &mut shutdown_rx => {
                        info!("Background flush task shutting down");
                        // Final flush before exit
                        if let Err(e) = store.flush() {
                            error!("Final flush failed: {}", e);
                        }
                        break;
                    }
                }
            }
        });

        self.shutdown_tx = Some(shutdown_tx);
        self.flush_task = Some(task);
    }

    /// Shutdown the persistent registry gracefully
    pub async fn shutdown(mut self) -> Result<(), AppError> {
        info!("Shutting down persistent peer registry");

        // Stop background task
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }

        if let Some(task) = self.flush_task.take() {
            let _ = task.await;
        }

        // Final flush
        self.store.flush()?;

        info!("Persistent peer registry shut down successfully");
        Ok(())
    }

    /// Record a new connection
    pub fn on_connection_established(
        &self,
        peer_id: PeerId,
        address: Multiaddr,
        direction: ConnectionDirection,
    ) {
        // Update in-memory registry
        {
            let mut registry = self.inner.write().unwrap();
            registry.on_connection_established(peer_id, address.clone(), direction);
        }

        // Persist to database
        let registry = self.inner.read().unwrap();
        if let Some(peer_info) = registry.get_peer(&peer_id) {
            if let Err(e) = self.store.save_active_peer(&peer_info) {
                error!("Failed to persist active peer: {}", e);
            }
        }

        // Also save to known peers
        if let Some(addresses) = registry.get_known_addresses(&peer_id) {
            if let Err(e) = self.store.save_known_addresses(&peer_id, &addresses) {
                error!("Failed to persist known addresses: {}", e);
            }
        }

        // Save reconnection count if > 0
        if let Some(peer_info) = registry.get_peer(&peer_id) {
            if peer_info.stats.reconnection_count > 0 {
                if let Err(e) = self
                    .store
                    .save_reconnection_count(&peer_id, peer_info.stats.reconnection_count)
                {
                    error!("Failed to persist reconnection count: {}", e);
                }
            }
        }
    }

    /// Record a connection closure
    pub fn on_connection_closed(&self, peer_id: &PeerId) {
        let was_connected = {
            let registry = self.inner.read().unwrap();
            registry.get_peer(peer_id).is_some()
        };

        // Update in-memory registry
        {
            let mut registry = self.inner.write().unwrap();
            registry.on_connection_closed(peer_id);
        }

        // If peer is now disconnected, remove from active peers in DB
        let is_still_connected = {
            let registry = self.inner.read().unwrap();
            registry.get_peer(peer_id).is_some()
        };

        if was_connected && !is_still_connected {
            if let Err(e) = self.store.remove_active_peer(peer_id) {
                error!("Failed to remove active peer from DB: {}", e);
            }

            // But keep in known_peers
            let registry = self.inner.read().unwrap();
            if let Some(addresses) = registry.get_known_addresses(peer_id) {
                if let Err(e) = self.store.save_known_addresses(peer_id, &addresses) {
                    error!("Failed to persist known addresses: {}", e);
                }
            }
        }
    }

    /// Record a connection failure
    pub fn on_connection_failed(&self, peer_id: Option<&PeerId>) {
        let mut registry = self.inner.write().unwrap();
        registry.on_connection_failed(peer_id);
        // Connection failures are ephemeral, not persisted
    }

    /// Get info about a specific peer
    pub fn get_peer(&self, peer_id: &PeerId) -> Option<PeerInfo> {
        let registry = self.inner.read().unwrap();
        registry.get_peer(peer_id)
    }

    /// Get all connected peers
    pub fn connected_peers(&self) -> Vec<PeerInfo> {
        let registry = self.inner.read().unwrap();
        registry.connected_peers()
    }

    /// Get count of connected peers
    pub fn peer_count(&self) -> usize {
        let registry = self.inner.read().unwrap();
        registry.peer_count()
    }

    /// Get known addresses for a peer (even if not connected)
    pub fn get_known_addresses(&self, peer_id: &PeerId) -> Option<Vec<Multiaddr>> {
        let registry = self.inner.read().unwrap();
        registry.get_known_addresses(peer_id)
    }

    /// Update peer activity timestamp
    pub fn on_peer_activity(&self, peer_id: &PeerId) {
        let mut registry = self.inner.write().unwrap();
        registry.on_peer_activity(peer_id);
        // Activity updates are ephemeral, only persisted on next connection event
    }

    /// Get network statistics
    pub fn stats(&self) -> NetworkStats {
        let registry = self.inner.read().unwrap();
        registry.stats()
    }

    /// Manually flush to disk
    pub fn flush(&self) -> Result<(), AppError> {
        self.store.flush()
    }
}

impl Drop for PersistentPeerRegistry {
    fn drop(&mut self) {
        // Send shutdown signal if still active
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }

        // Best-effort flush
        if let Err(e) = self.store.flush() {
            error!("Failed to flush peer registry in Drop: {}", e);
        }
    }
}
