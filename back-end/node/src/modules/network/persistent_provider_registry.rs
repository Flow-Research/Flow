//! Persistent provider registry with RocksDB backing.
//!
//! Tracks which CIDs this node is providing to the DHT network.
//! Data persists across restarts, enabling automatic re-announcement.

use crate::modules::network::config::ProviderConfig;
use crate::modules::network::storage::provider_registry_store::{
    ProvidedCidMetadata, ProviderRegistryStore,
};
use crate::modules::storage::content::ContentId;
use errors::AppError;
use std::collections::HashSet;
use std::sync::{Arc, RwLock};
use tracing::{debug, info, warn};

/// Configuration for persistent provider registry
#[derive(Debug, Clone)]
pub struct ProviderRegistryConfig {
    /// Path to RocksDB directory
    pub db_path: std::path::PathBuf,
}

impl Default for ProviderRegistryConfig {
    fn default() -> Self {
        Self {
            db_path: crate::bootstrap::init::get_flow_data_dir()
                .join("network")
                .join("provider_registry"),
        }
    }
}

impl From<&ProviderConfig> for ProviderRegistryConfig {
    fn from(config: &ProviderConfig) -> Self {
        Self {
            db_path: config.db_path.clone(),
        }
    }
}

/// Persistent provider registry with RocksDB backing.
///
/// Maintains an in-memory set for fast lookup and persists all changes to RocksDB.
/// On startup, loads historical data to enable automatic re-announcement.
pub struct PersistentProviderRegistry {
    /// In-memory set of provided CIDs (for fast iteration during reprovide)
    provided_cids: Arc<RwLock<HashSet<ContentId>>>,
    /// Persistent store
    store: Arc<ProviderRegistryStore>,
}

impl PersistentProviderRegistry {
    /// Create a new persistent provider registry.
    ///
    /// Loads existing provided CIDs from RocksDB.
    pub fn new(config: ProviderRegistryConfig) -> Result<Self, AppError> {
        info!(
            "Initializing persistent provider registry at: {:?}",
            config.db_path
        );

        // Open RocksDB store
        let store = ProviderRegistryStore::new(&config.db_path)?;
        let store = Arc::new(store);

        // Load persisted provided CIDs
        let loaded = store.load_all_provided_cids()?;
        let cid_count = loaded.len();

        let provided_cids: HashSet<ContentId> = loaded.keys().cloned().collect();

        info!("Loaded {} provided CIDs from persistent storage", cid_count);

        Ok(Self {
            provided_cids: Arc::new(RwLock::new(provided_cids)),
            store,
        })
    }

    /// Add a CID to the provided set.
    ///
    /// Persists to RocksDB immediately.
    pub fn add(&self, cid: ContentId) -> Result<bool, AppError> {
        let is_new = {
            let mut cids = self.provided_cids.write().unwrap();
            cids.insert(cid.clone())
        };

        if is_new {
            let metadata = ProvidedCidMetadata::new(&cid);
            self.store.add_provided_cid(&cid, &metadata)?;
            debug!(%cid, "Added to provided set");
        } else {
            debug!(%cid, "Already in provided set");
        }

        Ok(is_new)
    }

    /// Remove a CID from the provided set.
    ///
    /// Removes from RocksDB immediately.
    pub fn remove(&self, cid: &ContentId) -> Result<bool, AppError> {
        let was_present = {
            let mut cids = self.provided_cids.write().unwrap();
            cids.remove(cid)
        };

        if was_present {
            self.store.remove_provided_cid(cid)?;
            debug!(%cid, "Removed from provided set");
        } else {
            debug!(%cid, "Not in provided set");
        }

        Ok(was_present)
    }

    /// Check if a CID is being provided.
    pub fn contains(&self, cid: &ContentId) -> bool {
        let cids = self.provided_cids.read().unwrap();
        cids.contains(cid)
    }

    /// Get all provided CIDs.
    ///
    /// Returns a cloned set for safe iteration.
    pub fn get_all(&self) -> HashSet<ContentId> {
        let cids = self.provided_cids.read().unwrap();
        cids.clone()
    }

    /// Get count of provided CIDs.
    pub fn count(&self) -> usize {
        let cids = self.provided_cids.read().unwrap();
        cids.len()
    }

    /// Check if the registry is empty.
    pub fn is_empty(&self) -> bool {
        let cids = self.provided_cids.read().unwrap();
        cids.is_empty()
    }

    /// Record a successful announcement for a CID.
    ///
    /// Updates the metadata in RocksDB.
    pub fn record_announcement(&self, cid: &ContentId) -> Result<(), AppError> {
        if let Some(mut metadata) = self.store.get_provided_cid(cid)? {
            metadata.record_announcement();
            self.store.update_provided_cid(cid, &metadata)?;
            debug!(
                %cid,
                count = metadata.announcement_count,
                "Recorded successful announcement"
            );
        } else {
            warn!(%cid, "Tried to record announcement for non-existent CID");
        }
        Ok(())
    }

    /// Get metadata for a provided CID.
    pub fn get_metadata(&self, cid: &ContentId) -> Result<Option<ProvidedCidMetadata>, AppError> {
        self.store.get_provided_cid(cid)
    }

    /// Flush changes to disk.
    pub fn flush(&self) -> Result<(), AppError> {
        self.store.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_registry() -> (PersistentProviderRegistry, TempDir) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config = ProviderRegistryConfig {
            db_path: temp_dir.path().join("provider_registry"),
        };
        let registry = PersistentProviderRegistry::new(config).expect("Failed to create registry");
        (registry, temp_dir)
    }

    #[test]
    fn test_add_and_contains() {
        let (registry, _temp) = create_test_registry();
        let cid = ContentId::from_bytes(b"test content");

        assert!(!registry.contains(&cid));

        let is_new = registry.add(cid.clone()).expect("Failed to add");
        assert!(is_new);
        assert!(registry.contains(&cid));

        // Adding again should return false
        let is_new = registry.add(cid.clone()).expect("Failed to add");
        assert!(!is_new);
    }

    #[test]
    fn test_remove() {
        let (registry, _temp) = create_test_registry();
        let cid = ContentId::from_bytes(b"test content");

        registry.add(cid.clone()).expect("Failed to add");
        assert!(registry.contains(&cid));

        let was_present = registry.remove(&cid).expect("Failed to remove");
        assert!(was_present);
        assert!(!registry.contains(&cid));

        // Removing again should return false
        let was_present = registry.remove(&cid).expect("Failed to remove");
        assert!(!was_present);
    }

    #[test]
    fn test_get_all() {
        let (registry, _temp) = create_test_registry();

        for i in 0..5 {
            let cid = ContentId::from_bytes(format!("content {}", i).as_bytes());
            registry.add(cid).expect("Failed to add");
        }

        let all = registry.get_all();
        assert_eq!(all.len(), 5);
    }

    #[test]
    fn test_persistence_across_reopens() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config = ProviderRegistryConfig {
            db_path: temp_dir.path().join("provider_registry"),
        };
        let cid = ContentId::from_bytes(b"persistent content");

        // Add with first registry
        {
            let registry =
                PersistentProviderRegistry::new(config.clone()).expect("Failed to create");
            registry.add(cid.clone()).expect("Failed to add");
            registry.flush().expect("Failed to flush");
        }

        // Verify with second registry
        {
            let registry = PersistentProviderRegistry::new(config).expect("Failed to reopen");
            assert!(registry.contains(&cid));
            assert_eq!(registry.count(), 1);
        }
    }

    #[test]
    fn test_record_announcement() {
        let (registry, _temp) = create_test_registry();
        let cid = ContentId::from_bytes(b"test content");

        registry.add(cid.clone()).expect("Failed to add");

        // Initial metadata
        let metadata = registry.get_metadata(&cid).expect("Failed to get").unwrap();
        assert_eq!(metadata.announcement_count, 0);
        assert!(metadata.last_announced.is_none());

        // Record announcement
        registry
            .record_announcement(&cid)
            .expect("Failed to record");

        let metadata = registry.get_metadata(&cid).expect("Failed to get").unwrap();
        assert_eq!(metadata.announcement_count, 1);
        assert!(metadata.last_announced.is_some());
    }

    #[test]
    fn test_count_and_is_empty() {
        let (registry, _temp) = create_test_registry();

        assert!(registry.is_empty());
        assert_eq!(registry.count(), 0);

        let cid = ContentId::from_bytes(b"test content");
        registry.add(cid.clone()).expect("Failed to add");

        assert!(!registry.is_empty());
        assert_eq!(registry.count(), 1);

        registry.remove(&cid).expect("Failed to remove");

        assert!(registry.is_empty());
        assert_eq!(registry.count(), 0);
    }
}
