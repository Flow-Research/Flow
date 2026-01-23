use crate::modules::network::peer_registry::DiscoverySource;

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
    /// Maximum consecutive failures before purging a peer
    pub max_failures: u32,
    /// TTL in seconds for known peers (0 = disabled)
    pub peer_ttl_secs: u64,
}

impl Default for PeerRegistryConfig {
    fn default() -> Self {
        Self {
            db_path: crate::bootstrap::init::get_flow_data_dir()
                .join("network")
                .join("peer_registry"),
            flush_interval_secs: 30, // Flush every 30 seconds
            max_failures: 5,         // Purge after 5 consecutive failures
            peer_ttl_secs: 7 * 24 * 60 * 60, // 7 days TTL
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

        let max_failures = std::env::var("PEER_REGISTRY_MAX_FAILURES")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(5);

        let peer_ttl_secs = std::env::var("PEER_REGISTRY_TTL_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(7 * 24 * 60 * 60);

        Self {
            db_path,
            flush_interval_secs,
            max_failures,
            peer_ttl_secs,
        }
    }
}

/// Persistent peer registry with RocksDB backing
///
/// This wraps the in-memory PeerRegistry and persists all changes to RocksDB.
/// On startup, it loads historical peer data to enable faster reconnections.
pub struct PersistentPeerRegistry {
    /// Inner in-memory registry
    pub inner: Arc<RwLock<PeerRegistry>>,
    /// Persistent store
    pub store: Arc<PeerRegistryStore>,
    /// Configuration
    pub config: PeerRegistryConfig,
    /// Shutdown signal sender
    pub shutdown_tx: Option<oneshot::Sender<()>>,
    /// Background flush task handle
    pub flush_task: Option<JoinHandle<()>>,
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

        // Load persisted known peers
        match store.load_known_peers() {
            Ok(known_peers) => {
                info!("Loaded {} known peers from disk", known_peers.len());
                registry.bulk_load_known_peers(known_peers);
            }
            Err(e) => {
                warn!("Failed to load known peers: {}", e);
            }
        }

        // Load reconnection counts
        match store.load_reconnection_counts() {
            Ok(counts) => {
                info!("Loaded {} reconnection counts from disk", counts.len());
                registry.bulk_load_reconnection_counts(counts);
            }
            Err(e) => {
                warn!("Failed to load reconnection counts: {}", e);
            }
        }

        // Load failure counts
        match store.load_failure_counts() {
            Ok(counts) => {
                info!("Loaded {} failure counts from disk", counts.len());
                registry.bulk_load_failure_counts(counts);
            }
            Err(e) => {
                warn!("Failed to load failure counts: {}", e);
            }
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
        discovery_source: DiscoverySource,
    ) {
        // Update in-memory registry
        {
            let mut registry = self.inner.write().unwrap();
            registry.on_connection_established(
                peer_id,
                address.clone(),
                direction,
                discovery_source,
            );
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

        // Connection succeeded - reset failure count and update last success
        drop(registry); // Release the read lock before calling methods that take write locks
        if let Err(e) = self.store.remove_failure_count(&peer_id) {
            // Ignore errors - peer may not have a failure count
            debug!(peer_id = %peer_id, error = %e, "No failure count to remove (expected for new peers)");
        }
        if let Err(e) = self.store.save_last_success(&peer_id, std::time::SystemTime::now()) {
            error!(peer_id = %peer_id, error = %e, "Failed to save last success timestamp");
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

    /// Bulk load known peers (used for bootstrap peers)
    ///
    /// This adds peers to the known_peers map without establishing connections.
    /// Also persists them to the database.
    pub fn bulk_load_known_peers(&self, peers: std::collections::HashMap<PeerId, Vec<Multiaddr>>) {
        // Update in-memory registry
        {
            let mut registry = self.inner.write().unwrap();
            registry.bulk_load_known_peers(peers.clone());
        }

        // Persist to database
        for (peer_id, addresses) in peers {
            if let Err(e) = self.store.save_known_addresses(&peer_id, &addresses) {
                error!(peer_id = %peer_id, error = %e, "Failed to persist bootstrap peer addresses");
            }
        }
    }

    /// Check if a peer is currently connected
    pub fn is_connected(&self, peer_id: &PeerId) -> bool {
        let registry = self.inner.read().unwrap();
        registry.is_connected(peer_id)
    }

    /// Get reconnection count for a peer
    pub fn get_reconnection_count(&self, peer_id: &PeerId) -> u32 {
        let registry = self.inner.read().unwrap();
        registry.get_reconnection_count(peer_id)
    }

    /// Record a connection failure for a specific peer
    ///
    /// Returns true if the peer should be purged (exceeded max failures)
    pub fn record_peer_failure(&self, peer_id: &PeerId) -> bool {
        let failure_count = {
            let mut registry = self.inner.write().unwrap();
            registry.record_peer_failure(peer_id)
        };

        // Persist the failure count
        if let Err(e) = self.store.save_failure_count(peer_id, failure_count) {
            error!(peer_id = %peer_id, error = %e, "Failed to persist failure count");
        }

        // Check if peer should be purged
        if failure_count >= self.config.max_failures {
            info!(
                peer_id = %peer_id,
                failure_count = failure_count,
                max_failures = self.config.max_failures,
                "Peer exceeded max failures, marking for purge"
            );
            return true;
        }

        false
    }

    /// Get the current failure count for a peer
    pub fn get_failure_count(&self, peer_id: &PeerId) -> u32 {
        let registry = self.inner.read().unwrap();
        registry.get_failure_count(peer_id)
    }

    /// Remove a stale peer completely from registry and storage
    pub fn purge_stale_peer(&self, peer_id: &PeerId) {
        // Remove from in-memory registry
        {
            let mut registry = self.inner.write().unwrap();
            registry.remove_known_peer(peer_id);
        }

        // Remove from persistent storage
        if let Err(e) = self.store.remove_known_peer(peer_id) {
            error!(peer_id = %peer_id, error = %e, "Failed to remove stale peer from storage");
        }
    }

    /// Purge all peers that have exceeded the failure threshold
    ///
    /// Returns the number of peers purged
    pub fn purge_failed_peers(&self) -> usize {
        let stale_peers = {
            let registry = self.inner.read().unwrap();
            registry.get_stale_peers(self.config.max_failures)
        };

        let count = stale_peers.len();
        for peer_id in stale_peers {
            self.purge_stale_peer(&peer_id);
        }

        if count > 0 {
            info!(purged_count = count, "Purged stale peers from registry");
        }

        count
    }

    /// Reset failure count for a peer (called on successful connection)
    pub fn reset_peer_failures(&self, peer_id: &PeerId) {
        // Reset in-memory
        {
            let mut registry = self.inner.write().unwrap();
            registry.reset_peer_failures(peer_id);
        }

        // Remove from persistent storage
        if let Err(e) = self.store.remove_failure_count(peer_id) {
            error!(peer_id = %peer_id, error = %e, "Failed to remove failure count from storage");
        }

        // Update last success timestamp
        if let Err(e) = self.store.save_last_success(peer_id, std::time::SystemTime::now()) {
            error!(peer_id = %peer_id, error = %e, "Failed to save last success timestamp");
        }
    }

    /// Get configuration
    pub fn config(&self) -> &PeerRegistryConfig {
        &self.config
    }

    /// Purge peers that have exceeded the TTL without successful connection
    ///
    /// Returns the number of peers purged
    pub fn purge_expired_peers(&self) -> usize {
        // Skip if TTL is disabled
        if self.config.peer_ttl_secs == 0 {
            return 0;
        }

        let ttl_duration = std::time::Duration::from_secs(self.config.peer_ttl_secs);
        let cutoff = std::time::SystemTime::now() - ttl_duration;

        // Load last success timestamps from storage
        let last_success_timestamps = match self.store.load_last_success_timestamps() {
            Ok(timestamps) => timestamps,
            Err(e) => {
                warn!(error = %e, "Failed to load last success timestamps for TTL check");
                return 0;
            }
        };

        // Get all known peer IDs
        let known_peer_ids: Vec<PeerId> = {
            let registry = self.inner.read().unwrap();
            registry.get_known_peer_ids()
        };

        let mut purged_count = 0;

        for peer_id in known_peer_ids {
            // Skip peers that are currently connected
            if self.is_connected(&peer_id) {
                continue;
            }

            // Check if peer has exceeded TTL
            let should_purge = match last_success_timestamps.get(&peer_id) {
                Some(last_success) => *last_success < cutoff,
                // Peers with no last_success record are considered expired
                // (they've never successfully connected)
                None => true,
            };

            if should_purge {
                info!(
                    peer_id = %peer_id,
                    ttl_secs = self.config.peer_ttl_secs,
                    "Purging peer due to TTL expiry"
                );
                self.purge_stale_peer(&peer_id);
                purged_count += 1;
            }
        }

        if purged_count > 0 {
            info!(
                purged_count = purged_count,
                ttl_secs = self.config.peer_ttl_secs,
                "Purged expired peers based on TTL"
            );
        }

        purged_count
    }

    /// Run all maintenance tasks: purge failed peers and expired peers
    ///
    /// Returns (failed_purged, expired_purged)
    pub fn run_maintenance(&self) -> (usize, usize) {
        let failed = self.purge_failed_peers();
        let expired = self.purge_expired_peers();
        (failed, expired)
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
