use crate::modules::network::peer_registry::{ConnectionDirection, ConnectionStats, PeerInfo};
use errors::AppError;
use libp2p::{Multiaddr, PeerId};
use rocksdb::{ColumnFamilyDescriptor, DB, Options};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::Path,
    sync::Arc,
    time::{Instant, SystemTime},
};
use tracing::{debug, error, info, warn};

// Column family names
const CF_ACTIVE_PEERS: &str = "active_peers";
const CF_KNOWN_PEERS: &str = "known_peers";
const CF_PEER_STATS: &str = "peer_stats";

/// Serializable version of PeerInfo with SystemTime instead of Instant
#[derive(Serialize, Deserialize, Clone, Debug)]
struct SerializablePeerInfo {
    peer_id: String,
    addresses: Vec<String>,
    first_seen_timestamp: SystemTime,
    last_seen_timestamp: SystemTime,
    connection_count: usize,
    stats: ConnectionStats,
    connection_direction: ConnectionDirection,
}

impl SerializablePeerInfo {
    fn from_peer_info(info: &PeerInfo) -> Self {
        Self {
            peer_id: info.peer_id.to_string(),
            addresses: info.addresses.iter().map(|a| a.to_string()).collect(),
            first_seen_timestamp: SystemTime::now() - (Instant::now() - info.first_seen),
            last_seen_timestamp: SystemTime::now() - (Instant::now() - info.last_seen),
            connection_count: info.connection_count,
            stats: info.stats.clone(),
            connection_direction: info.connection_direction,
        }
    }

    fn to_peer_info(&self) -> Result<PeerInfo, String> {
        let peer_id = self
            .peer_id
            .parse::<PeerId>()
            .map_err(|e| format!("Invalid PeerId: {}", e))?;

        let addresses: Result<Vec<Multiaddr>, String> = self
            .addresses
            .iter()
            .map(|s| {
                s.parse::<Multiaddr>()
                    .map_err(|e| format!("Invalid Multiaddr: {}", e))
            })
            .collect();

        let addresses = addresses?;

        let now = Instant::now();
        let system_now = SystemTime::now();

        let first_seen = if let Ok(duration) = system_now.duration_since(self.first_seen_timestamp)
        {
            now - duration
        } else {
            // Timestamp is in the future (shouldn't happen, but handle gracefully)
            now
        };

        let last_seen = if let Ok(duration) = system_now.duration_since(self.last_seen_timestamp) {
            now - duration
        } else {
            now
        };

        Ok(PeerInfo {
            peer_id,
            addresses,
            first_seen,
            last_seen,
            connection_count: self.connection_count,
            stats: self.stats.clone(),
            connection_direction: self.connection_direction,
        })
    }
}

/// RocksDB-backed peer registry storage
pub struct PeerRegistryStore {
    db: Arc<DB>,
}

impl PeerRegistryStore {
    /// Create a new peer registry store
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self, AppError> {
        info!(
            "Opening RocksDB peer registry store at: {}",
            path.as_ref().display()
        );

        // Ensure parent directory exists
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent).map_err(|e| AppError::Storage(Box::new(e)))?;
        }

        // Configure RocksDB options
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);
        opts.set_compression_type(rocksdb::DBCompressionType::Lz4);
        opts.set_max_open_files(500);
        opts.set_keep_log_file_num(5);

        // Define column families
        let cf_active = ColumnFamilyDescriptor::new(CF_ACTIVE_PEERS, Options::default());
        let cf_known = ColumnFamilyDescriptor::new(CF_KNOWN_PEERS, Options::default());
        let cf_stats = ColumnFamilyDescriptor::new(CF_PEER_STATS, Options::default());

        let db = DB::open_cf_descriptors(&opts, path, vec![cf_active, cf_known, cf_stats])
            .map_err(|e| AppError::Storage(Box::new(e)))?;

        info!("RocksDB peer registry store opened successfully");

        Ok(Self { db: Arc::new(db) })
    }

    /// Save an active peer to the database
    pub fn save_active_peer(&self, peer_info: &PeerInfo) -> Result<(), AppError> {
        let serializable = SerializablePeerInfo::from_peer_info(peer_info);
        let value =
            serde_json::to_vec(&serializable).map_err(|e| AppError::Storage(Box::new(e)))?;

        let cf = self.db.cf_handle(CF_ACTIVE_PEERS).ok_or_else(|| {
            AppError::Storage("CF_ACTIVE_PEERS column family not found".to_string().into())
        })?;

        self.db
            .put_cf(&cf, peer_info.peer_id.to_bytes(), value)
            .map_err(|e| {
                error!("Failed to save active peer {}: {}", peer_info.peer_id, e);
                AppError::Storage(format!("Failed to save active peer: {}", e).into())
            })?;

        debug!("Saved active peer: {}", peer_info.peer_id);
        Ok(())
    }

    /// Remove an active peer from the database
    pub fn remove_active_peer(&self, peer_id: &PeerId) -> Result<(), AppError> {
        let cf = self.db.cf_handle(CF_ACTIVE_PEERS).ok_or_else(|| {
            AppError::Storage("CF_ACTIVE_PEERS column family not found".to_string().into())
        })?;

        self.db.delete_cf(&cf, peer_id.to_bytes()).map_err(|e| {
            AppError::Storage(format!("Failed to remove active peer: {}", e).into())
        })?;

        debug!("Removed active peer: {}", peer_id);
        Ok(())
    }

    /// Load all active peers from the database
    pub fn load_active_peers(&self) -> Result<Vec<PeerInfo>, AppError> {
        let cf = self.db.cf_handle(CF_ACTIVE_PEERS).ok_or_else(|| {
            AppError::Storage("CF_ACTIVE_PEERS column family not found".to_string().into())
        })?;

        let mut peers = Vec::new();
        let iter = self.db.iterator_cf(&cf, rocksdb::IteratorMode::Start);

        for item in iter {
            match item {
                Ok((_key, value)) => match serde_json::from_slice::<SerializablePeerInfo>(&value) {
                    Ok(serializable) => match serializable.to_peer_info() {
                        Ok(peer_info) => peers.push(peer_info),
                        Err(e) => warn!("Failed to convert peer info: {}", e),
                    },
                    Err(e) => warn!("Failed to deserialize peer info: {}", e),
                },
                Err(e) => error!("Failed to read from active peers CF: {}", e),
            }
        }

        info!("Loaded {} active peers from database", peers.len());
        Ok(peers)
    }

    /// Save known addresses for a peer
    pub fn save_known_addresses(
        &self,
        peer_id: &PeerId,
        addresses: &[Multiaddr],
    ) -> Result<(), AppError> {
        let addrs_str: Vec<String> = addresses.iter().map(|a| a.to_string()).collect();
        let value = serde_json::to_vec(&addrs_str).map_err(|e| {
            AppError::Storage(format!("Failed to serialize addresses: {}", e).into())
        })?;

        let cf = self.db.cf_handle(CF_KNOWN_PEERS).ok_or_else(|| {
            AppError::Storage("CF_KNOWN_PEERS column family not found".to_string().into())
        })?;

        self.db
            .put_cf(&cf, peer_id.to_bytes(), value)
            .map_err(|e| {
                AppError::Storage(format!("Failed to save known addresses: {}", e).into())
            })?;

        debug!(
            "Saved {} known addresses for peer: {}",
            addresses.len(),
            peer_id
        );
        Ok(())
    }

    /// Load all known peers and their addresses
    pub fn load_known_peers(&self) -> Result<HashMap<PeerId, Vec<Multiaddr>>, AppError> {
        let cf = self.db.cf_handle(CF_KNOWN_PEERS).ok_or_else(|| {
            AppError::Storage("CF_KNOWN_PEERS column family not found".to_string().into())
        })?;

        let mut known_peers = HashMap::new();
        let iter = self.db.iterator_cf(&cf, rocksdb::IteratorMode::Start);

        for item in iter {
            match item {
                Ok((key, value)) => match PeerId::from_bytes(&key) {
                    Ok(peer_id) => match serde_json::from_slice::<Vec<String>>(&value) {
                        Ok(addr_strings) => {
                            let addresses: Vec<Multiaddr> =
                                addr_strings.iter().filter_map(|s| s.parse().ok()).collect();

                            if !addresses.is_empty() {
                                known_peers.insert(peer_id, addresses);
                            }
                        }
                        Err(e) => warn!("Failed to deserialize addresses: {}", e),
                    },
                    Err(e) => warn!("Failed to parse PeerId: {}", e),
                },
                Err(e) => error!("Failed to read from known peers CF: {}", e),
            }
        }

        info!("Loaded {} known peers from database", known_peers.len());
        Ok(known_peers)
    }

    /// Save reconnection count for a peer
    pub fn save_reconnection_count(&self, peer_id: &PeerId, count: u32) -> Result<(), AppError> {
        let cf = self.db.cf_handle(CF_PEER_STATS).ok_or_else(|| {
            AppError::Storage("CF_PEER_STATS column family not found".to_string().into())
        })?;

        self.db
            .put_cf(&cf, peer_id.to_bytes(), count.to_le_bytes())
            .map_err(|e| {
                AppError::Storage(format!("Failed to save reconnection count: {}", e).into())
            })?;

        debug!("Saved reconnection count {} for peer: {}", count, peer_id);
        Ok(())
    }

    /// Load all reconnection counts
    pub fn load_reconnection_counts(&self) -> Result<HashMap<PeerId, u32>, AppError> {
        let cf = self.db.cf_handle(CF_PEER_STATS).ok_or_else(|| {
            AppError::Storage("CF_PEER_STATS column family not found".to_string().into())
        })?;

        let mut counts = HashMap::new();
        let iter = self.db.iterator_cf(&cf, rocksdb::IteratorMode::Start);

        for item in iter {
            match item {
                Ok((key, value)) => match PeerId::from_bytes(&key) {
                    Ok(peer_id) => {
                        if value.len() == 4 {
                            let count =
                                u32::from_le_bytes([value[0], value[1], value[2], value[3]]);
                            counts.insert(peer_id, count);
                        }
                    }
                    Err(e) => warn!("Failed to parse PeerId from stats: {}", e),
                },
                Err(e) => error!("Failed to read from peer stats CF: {}", e),
            }
        }

        info!("Loaded {} reconnection counts from database", counts.len());
        Ok(counts)
    }

    /// Flush all pending writes to disk
    pub fn flush(&self) -> Result<(), AppError> {
        self.db
            .flush()
            .map_err(|e| AppError::Storage(format!("Failed to flush database: {}", e).into()))?;
        debug!("Flushed peer registry database");
        Ok(())
    }
}

////////////////////////////
/// TESTS
////////////////////////////

#[cfg(test)]
mod tests {
    use super::*;
    use libp2p::identity::Keypair;
    use std::time::Duration;
    use tempfile::TempDir;

    // Helper function to create a test store
    fn create_test_store() -> (PeerRegistryStore, TempDir) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let store = PeerRegistryStore::new(temp_dir.path().join("peer_registry"))
            .expect("Failed to create store");
        (store, temp_dir)
    }

    // Helper function to create a test peer
    fn create_test_peer(id_suffix: u8) -> (PeerId, Vec<Multiaddr>) {
        let keypair = Keypair::generate_ed25519();
        let peer_id = keypair.public().to_peer_id();
        let addresses = vec![
            format!("/ip4/127.0.0.1/tcp/{}", 4000u16 + id_suffix as u16)
                .parse()
                .unwrap(),
            format!("/ip4/192.168.1.{}/tcp/4001", id_suffix)
                .parse()
                .unwrap(),
        ];
        (peer_id, addresses)
    }

    // Helper function to create a PeerInfo
    fn create_peer_info(
        peer_id: PeerId,
        addresses: Vec<Multiaddr>,
        connection_count: usize,
    ) -> PeerInfo {
        PeerInfo {
            peer_id,
            addresses,
            first_seen: Instant::now() - Duration::from_secs(100),
            last_seen: Instant::now() - Duration::from_secs(10),
            connection_count,
            stats: ConnectionStats {
                bytes_sent: 1024 * connection_count as u64,
                bytes_received: 2048 * connection_count as u64,
                reconnection_count: 0,
                avg_latency_ms: Some(50.0),
            },
            connection_direction: ConnectionDirection::Outbound,
        }
    }

    // ==================== SerializablePeerInfo Tests ====================

    #[test]
    fn test_serializable_peer_info_round_trip() {
        let (peer_id, addresses) = create_test_peer(1);
        let original = create_peer_info(peer_id, addresses, 3);

        let serializable = SerializablePeerInfo::from_peer_info(&original);
        let restored = serializable
            .to_peer_info()
            .expect("Failed to convert back to PeerInfo");

        assert_eq!(restored.peer_id, original.peer_id);
        assert_eq!(restored.addresses, original.addresses);
        assert_eq!(restored.connection_count, original.connection_count);
        assert_eq!(restored.stats.bytes_sent, original.stats.bytes_sent);
        assert_eq!(restored.stats.bytes_received, original.stats.bytes_received);
        assert_eq!(
            restored.stats.reconnection_count,
            original.stats.reconnection_count
        );
        assert_eq!(restored.stats.avg_latency_ms, original.stats.avg_latency_ms);
        assert_eq!(restored.connection_direction, original.connection_direction);

        // Time fields should be approximately equal (within 1 second)
        assert!(
            restored.first_seen.elapsed() >= original.first_seen.elapsed() - Duration::from_secs(1)
        );
        assert!(
            restored.last_seen.elapsed() >= original.last_seen.elapsed() - Duration::from_secs(1)
        );
    }

    #[test]
    fn test_serializable_peer_info_invalid_peer_id() {
        let serializable = SerializablePeerInfo {
            peer_id: "invalid_peer_id".to_string(),
            addresses: vec![],
            first_seen_timestamp: SystemTime::now(),
            last_seen_timestamp: SystemTime::now(),
            connection_count: 0,
            stats: ConnectionStats {
                bytes_sent: 0,
                bytes_received: 0,
                reconnection_count: 0,
                avg_latency_ms: None,
            },
            connection_direction: ConnectionDirection::Inbound,
        };

        assert!(serializable.to_peer_info().is_err());
    }

    #[test]
    fn test_serializable_peer_info_invalid_multiaddr() {
        let (peer_id, _) = create_test_peer(1);
        let serializable = SerializablePeerInfo {
            peer_id: peer_id.to_string(),
            addresses: vec!["invalid_multiaddr".to_string()],
            first_seen_timestamp: SystemTime::now(),
            last_seen_timestamp: SystemTime::now(),
            connection_count: 0,
            stats: ConnectionStats {
                bytes_sent: 0,
                bytes_received: 0,
                reconnection_count: 0,
                avg_latency_ms: None,
            },
            connection_direction: ConnectionDirection::Inbound,
        };

        assert!(serializable.to_peer_info().is_err());
    }

    #[test]
    fn test_serializable_peer_info_future_timestamp() {
        let (peer_id, addresses) = create_test_peer(1);
        let future_time = SystemTime::now() + Duration::from_secs(1000);

        let serializable = SerializablePeerInfo {
            peer_id: peer_id.to_string(),
            addresses: addresses.iter().map(|a| a.to_string()).collect(),
            first_seen_timestamp: future_time,
            last_seen_timestamp: future_time,
            connection_count: 1,
            stats: ConnectionStats {
                bytes_sent: 100,
                bytes_received: 200,
                reconnection_count: 1,
                avg_latency_ms: Some(25.0),
            },
            connection_direction: ConnectionDirection::Inbound,
        };

        // Should handle future timestamps gracefully
        let restored = serializable
            .to_peer_info()
            .expect("Failed to convert with future timestamp");

        // Times should be set to now when timestamp is in future
        assert!(restored.first_seen.elapsed() < Duration::from_secs(1));
        assert!(restored.last_seen.elapsed() < Duration::from_secs(1));
    }

    // ==================== PeerRegistryStore::new Tests ====================

    #[test]
    fn test_create_store_success() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let store = PeerRegistryStore::new(temp_dir.path().join("peer_registry"));
        assert!(store.is_ok());
    }

    #[test]
    fn test_create_store_creates_parent_directory() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let nested_path = temp_dir.path().join("nested/dir/peer_registry");

        let store = PeerRegistryStore::new(&nested_path);
        assert!(store.is_ok());
        assert!(nested_path.exists());
    }

    #[test]
    fn test_reopen_existing_store() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let path = temp_dir.path().join("peer_registry");

        // Create and close first store
        {
            let _store = PeerRegistryStore::new(&path).expect("Failed to create store");
        }

        // Reopen should succeed
        let store = PeerRegistryStore::new(&path);
        assert!(store.is_ok());
    }

    // ==================== Active Peers Tests ====================

    #[test]
    fn test_save_and_load_single_active_peer() {
        let (store, _temp) = create_test_store();
        let (peer_id, addresses) = create_test_peer(1);
        let peer_info = create_peer_info(peer_id, addresses.clone(), 1);

        store
            .save_active_peer(&peer_info)
            .expect("Failed to save active peer");

        let loaded = store
            .load_active_peers()
            .expect("Failed to load active peers");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].peer_id, peer_id);
        assert_eq!(loaded[0].addresses, addresses);
    }

    #[test]
    fn test_save_and_load_multiple_active_peers() {
        let (store, _temp) = create_test_store();
        let mut peers = Vec::new();

        for i in 1..=5 {
            let (peer_id, addresses) = create_test_peer(i);
            let peer_info = create_peer_info(peer_id, addresses, i as usize);
            store
                .save_active_peer(&peer_info)
                .expect("Failed to save active peer");
            peers.push(peer_info);
        }

        let loaded = store
            .load_active_peers()
            .expect("Failed to load active peers");
        assert_eq!(loaded.len(), 5);

        // Verify all peers are present (order may differ)
        for original_peer in &peers {
            assert!(loaded.iter().any(|p| p.peer_id == original_peer.peer_id));
        }
    }

    #[test]
    fn test_update_existing_active_peer() {
        let (store, _temp) = create_test_store();
        let (peer_id, addresses) = create_test_peer(1);
        let peer_info1 = create_peer_info(peer_id, addresses.clone(), 1);

        // Save initial version
        store
            .save_active_peer(&peer_info1)
            .expect("Failed to save active peer");

        // Update with new connection count
        let peer_info2 = create_peer_info(peer_id, addresses.clone(), 5);
        store
            .save_active_peer(&peer_info2)
            .expect("Failed to update active peer");

        let loaded = store
            .load_active_peers()
            .expect("Failed to load active peers");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].connection_count, 5);
    }

    #[test]
    fn test_remove_active_peer() {
        let (store, _temp) = create_test_store();
        let (peer_id, addresses) = create_test_peer(1);
        let peer_info = create_peer_info(peer_id, addresses, 1);

        store
            .save_active_peer(&peer_info)
            .expect("Failed to save active peer");

        let loaded = store
            .load_active_peers()
            .expect("Failed to load active peers");
        assert_eq!(loaded.len(), 1);

        store
            .remove_active_peer(&peer_id)
            .expect("Failed to remove active peer");

        let loaded = store
            .load_active_peers()
            .expect("Failed to load active peers");
        assert_eq!(loaded.len(), 0);
    }

    #[test]
    fn test_remove_nonexistent_active_peer() {
        let (store, _temp) = create_test_store();
        let (peer_id, _) = create_test_peer(1);

        // Should not error when removing non-existent peer
        let result = store.remove_active_peer(&peer_id);
        assert!(result.is_ok());
    }

    #[test]
    fn test_load_active_peers_empty() {
        let (store, _temp) = create_test_store();
        let loaded = store
            .load_active_peers()
            .expect("Failed to load active peers");
        assert_eq!(loaded.len(), 0);
    }

    // ==================== Known Addresses Tests ====================

    #[test]
    fn test_save_and_load_known_addresses() {
        let (store, _temp) = create_test_store();
        let (peer_id, addresses) = create_test_peer(1);

        store
            .save_known_addresses(&peer_id, &addresses)
            .expect("Failed to save known addresses");

        let loaded = store
            .load_known_peers()
            .expect("Failed to load known peers");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded.get(&peer_id).unwrap(), &addresses);
    }

    #[test]
    fn test_save_and_load_multiple_known_peers() {
        let (store, _temp) = create_test_store();
        let mut expected = HashMap::new();

        for i in 1..=5 {
            let (peer_id, addresses) = create_test_peer(i);
            store
                .save_known_addresses(&peer_id, &addresses)
                .expect("Failed to save known addresses");
            expected.insert(peer_id, addresses);
        }

        let loaded = store
            .load_known_peers()
            .expect("Failed to load known peers");
        assert_eq!(loaded.len(), 5);

        for (peer_id, addresses) in &expected {
            assert_eq!(loaded.get(peer_id).unwrap(), addresses);
        }
    }

    #[test]
    fn test_update_known_addresses() {
        let (store, _temp) = create_test_store();
        let (peer_id, addresses1) = create_test_peer(1);
        let (_, addresses2) = create_test_peer(2);

        // Save initial addresses
        store
            .save_known_addresses(&peer_id, &addresses1)
            .expect("Failed to save known addresses");

        // Update with new addresses
        store
            .save_known_addresses(&peer_id, &addresses2)
            .expect("Failed to update known addresses");

        let loaded = store
            .load_known_peers()
            .expect("Failed to load known peers");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded.get(&peer_id).unwrap(), &addresses2);
    }

    #[test]
    fn test_save_empty_addresses() {
        let (store, _temp) = create_test_store();
        let (peer_id, _) = create_test_peer(1);

        store
            .save_known_addresses(&peer_id, &[])
            .expect("Failed to save empty addresses");

        let loaded = store
            .load_known_peers()
            .expect("Failed to load known peers");
        // Empty addresses should not be stored
        assert_eq!(loaded.len(), 0);
    }

    #[test]
    fn test_load_known_peers_empty() {
        let (store, _temp) = create_test_store();
        let loaded = store
            .load_known_peers()
            .expect("Failed to load known peers");
        assert_eq!(loaded.len(), 0);
    }

    // ==================== Reconnection Counts Tests ====================

    #[test]
    fn test_save_and_load_reconnection_count() {
        let (store, _temp) = create_test_store();
        let (peer_id, _) = create_test_peer(1);

        store
            .save_reconnection_count(&peer_id, 5)
            .expect("Failed to save reconnection count");

        let loaded = store
            .load_reconnection_counts()
            .expect("Failed to load reconnection counts");
        assert_eq!(loaded.len(), 1);
        assert_eq!(*loaded.get(&peer_id).unwrap(), 5);
    }

    #[test]
    fn test_save_and_load_multiple_reconnection_counts() {
        let (store, _temp) = create_test_store();
        let mut expected = HashMap::new();

        for i in 1..=5 {
            let (peer_id, _) = create_test_peer(i);
            let count = i as u32 * 10;
            store
                .save_reconnection_count(&peer_id, count)
                .expect("Failed to save reconnection count");
            expected.insert(peer_id, count);
        }

        let loaded = store
            .load_reconnection_counts()
            .expect("Failed to load reconnection counts");
        assert_eq!(loaded.len(), 5);

        for (peer_id, count) in &expected {
            assert_eq!(loaded.get(peer_id).unwrap(), count);
        }
    }

    #[test]
    fn test_update_reconnection_count() {
        let (store, _temp) = create_test_store();
        let (peer_id, _) = create_test_peer(1);

        store
            .save_reconnection_count(&peer_id, 1)
            .expect("Failed to save reconnection count");

        store
            .save_reconnection_count(&peer_id, 10)
            .expect("Failed to update reconnection count");

        let loaded = store
            .load_reconnection_counts()
            .expect("Failed to load reconnection counts");
        assert_eq!(loaded.len(), 1);
        assert_eq!(*loaded.get(&peer_id).unwrap(), 10);
    }

    #[test]
    fn test_load_reconnection_counts_empty() {
        let (store, _temp) = create_test_store();
        let loaded = store
            .load_reconnection_counts()
            .expect("Failed to load reconnection counts");
        assert_eq!(loaded.len(), 0);
    }

    // ==================== Persistence Tests ====================

    #[test]
    fn test_data_persists_across_reopens() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let path = temp_dir.path().join("peer_registry");
        let (peer_id, addresses) = create_test_peer(1);
        let peer_info = create_peer_info(peer_id, addresses.clone(), 3);

        // Write data with first store instance
        {
            let store = PeerRegistryStore::new(&path).expect("Failed to create store");
            store
                .save_active_peer(&peer_info)
                .expect("Failed to save active peer");
            store
                .save_known_addresses(&peer_id, &addresses)
                .expect("Failed to save known addresses");
            store
                .save_reconnection_count(&peer_id, 7)
                .expect("Failed to save reconnection count");
            store.flush().expect("Failed to flush");
        }

        // Read data with second store instance
        {
            let store = PeerRegistryStore::new(&path).expect("Failed to reopen store");

            let active_peers = store
                .load_active_peers()
                .expect("Failed to load active peers");
            assert_eq!(active_peers.len(), 1);
            assert_eq!(active_peers[0].peer_id, peer_id);

            let known_peers = store
                .load_known_peers()
                .expect("Failed to load known peers");
            assert_eq!(known_peers.len(), 1);
            assert_eq!(known_peers.get(&peer_id).unwrap(), &addresses);

            let counts = store
                .load_reconnection_counts()
                .expect("Failed to load reconnection counts");
            assert_eq!(counts.len(), 1);
            assert_eq!(*counts.get(&peer_id).unwrap(), 7);
        }
    }

    #[test]
    fn test_flush_success() {
        let (store, _temp) = create_test_store();
        let (peer_id, addresses) = create_test_peer(1);
        let peer_info = create_peer_info(peer_id, addresses, 1);

        store
            .save_active_peer(&peer_info)
            .expect("Failed to save active peer");

        // Flush should succeed
        let result = store.flush();
        assert!(result.is_ok());
    }

    // ==================== Concurrent Access Tests ====================

    #[test]
    fn test_concurrent_reads() {
        let (store, _temp) = create_test_store();
        let store = Arc::new(store);

        // Populate some data
        for i in 1..=10 {
            let (peer_id, addresses) = create_test_peer(i);
            let peer_info = create_peer_info(peer_id, addresses, i as usize);
            store
                .save_active_peer(&peer_info)
                .expect("Failed to save active peer");
        }

        // Spawn multiple concurrent readers
        let mut handles = vec![];
        for _ in 0..5 {
            let store_clone = Arc::clone(&store);
            let handle = std::thread::spawn(move || {
                let loaded = store_clone.load_active_peers().expect("Failed to load");
                assert_eq!(loaded.len(), 10);
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().expect("Thread panicked");
        }
    }

    #[test]
    fn test_concurrent_writes() {
        let (store, _temp) = create_test_store();
        let store = Arc::new(store);

        // Spawn multiple concurrent writers
        let mut handles = vec![];
        for i in 1..=5 {
            let store_clone = Arc::clone(&store);
            let handle = std::thread::spawn(move || {
                let (peer_id, addresses) = create_test_peer(i);
                let peer_info = create_peer_info(peer_id, addresses, i as usize);
                store_clone
                    .save_active_peer(&peer_info)
                    .expect("Failed to save");
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().expect("Thread panicked");
        }

        // All writes should have succeeded
        let loaded = store
            .load_active_peers()
            .expect("Failed to load active peers");
        assert_eq!(loaded.len(), 5);
    }

    #[test]
    fn test_concurrent_read_write() {
        let (store, _temp) = create_test_store();
        let store = Arc::new(store);

        // Initial data
        for i in 1..=5 {
            let (peer_id, addresses) = create_test_peer(i);
            let peer_info = create_peer_info(peer_id, addresses, i as usize);
            store
                .save_active_peer(&peer_info)
                .expect("Failed to save active peer");
        }

        // Spawn concurrent readers and writers
        let mut handles = vec![];

        // Readers
        for _ in 0..3 {
            let store_clone = Arc::clone(&store);
            let handle = std::thread::spawn(move || {
                for _ in 0..10 {
                    let _ = store_clone.load_active_peers();
                }
            });
            handles.push(handle);
        }

        // Writers
        for i in 6..=8 {
            let store_clone = Arc::clone(&store);
            let handle = std::thread::spawn(move || {
                let (peer_id, addresses) = create_test_peer(i);
                let peer_info = create_peer_info(peer_id, addresses, i as usize);
                store_clone
                    .save_active_peer(&peer_info)
                    .expect("Failed to save");
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().expect("Thread panicked");
        }

        let loaded = store
            .load_active_peers()
            .expect("Failed to load active peers");
        assert_eq!(loaded.len(), 8);
    }

    // ==================== Integration Tests ====================

    #[test]
    fn test_full_peer_lifecycle() {
        let (store, _temp) = create_test_store();
        let (peer_id, addresses) = create_test_peer(1);

        // 1. Save known addresses (peer discovery)
        store
            .save_known_addresses(&peer_id, &addresses)
            .expect("Failed to save known addresses");

        // 2. Connect and save as active peer
        let peer_info = create_peer_info(peer_id, addresses.clone(), 1);
        store
            .save_active_peer(&peer_info)
            .expect("Failed to save active peer");

        // 3. Disconnect
        store
            .remove_active_peer(&peer_id)
            .expect("Failed to remove active peer");

        // 4. Reconnect with incremented count
        store
            .save_reconnection_count(&peer_id, 1)
            .expect("Failed to save reconnection count");
        let peer_info = create_peer_info(peer_id, addresses.clone(), 2);
        store
            .save_active_peer(&peer_info)
            .expect("Failed to save active peer");

        // Verify final state
        let active = store.load_active_peers().expect("Failed to load");
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].connection_count, 2);

        let known = store.load_known_peers().expect("Failed to load");
        assert_eq!(known.len(), 1);

        let counts = store.load_reconnection_counts().expect("Failed to load");
        assert_eq!(*counts.get(&peer_id).unwrap(), 1);
    }

    #[test]
    fn test_mixed_operations() {
        let (store, _temp) = create_test_store();

        // Mix of operations across all column families
        for i in 1..=10 {
            let (peer_id, addresses) = create_test_peer(i);

            if i % 2 == 0 {
                // Even: active peer
                let peer_info = create_peer_info(peer_id, addresses.clone(), i as usize);
                store.save_active_peer(&peer_info).expect("Failed to save");
            }

            if i % 3 == 0 {
                // Multiple of 3: known addresses
                store
                    .save_known_addresses(&peer_id, &addresses)
                    .expect("Failed to save");
            }

            if i % 5 == 0 {
                // Multiple of 5: reconnection count
                store
                    .save_reconnection_count(&peer_id, i as u32)
                    .expect("Failed to save");
            }
        }

        let active = store.load_active_peers().expect("Failed to load");
        assert_eq!(active.len(), 5); // 2, 4, 6, 8, 10

        let known = store.load_known_peers().expect("Failed to load");
        assert_eq!(known.len(), 3); // 3, 6, 9

        let counts = store.load_reconnection_counts().expect("Failed to load");
        assert_eq!(counts.len(), 2); // 5, 10
    }
}
