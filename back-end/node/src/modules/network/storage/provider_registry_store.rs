//! RocksDB-backed storage for provided content IDs.
//!
//! Persists the set of CIDs this node is providing to the DHT,
//! enabling automatic re-announcement after restarts.

use crate::modules::storage::content::ContentId;
use errors::AppError;
use rocksdb::{ColumnFamilyDescriptor, DB, Options};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::Path, sync::Arc, time::SystemTime};
use tracing::{debug, error, info, warn};

const CF_PROVIDED_CIDS: &str = "provided_cids";

/// Metadata for a provided CID
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ProvidedCidMetadata {
    /// CID as string (for debugging)
    pub cid_string: String,
    /// When this CID was first provided
    pub first_provided: SystemTime,
    /// When this CID was last successfully announced to DHT
    pub last_announced: Option<SystemTime>,
    /// Number of successful announcements
    pub announcement_count: u64,
}

impl ProvidedCidMetadata {
    /// Create new metadata for a freshly provided CID
    pub fn new(cid: &ContentId) -> Self {
        Self {
            cid_string: cid.to_string(),
            first_provided: SystemTime::now(),
            last_announced: None,
            announcement_count: 0,
        }
    }

    /// Update metadata after successful announcement
    pub fn record_announcement(&mut self) {
        self.last_announced = Some(SystemTime::now());
        self.announcement_count += 1;
    }
}

/// RocksDB-backed storage for provided CIDs
pub struct ProviderRegistryStore {
    db: Arc<DB>,
}

impl ProviderRegistryStore {
    /// Create a new provider registry store
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self, AppError> {
        info!(
            "Opening RocksDB provider registry store at: {}",
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

        // Define column family
        let cf_provided = ColumnFamilyDescriptor::new(CF_PROVIDED_CIDS, Options::default());

        let db = DB::open_cf_descriptors(&opts, path, vec![cf_provided])
            .map_err(|e| AppError::Storage(Box::new(e)))?;

        info!("RocksDB provider registry store opened successfully");

        Ok(Self { db: Arc::new(db) })
    }

    /// Add a provided CID to the store
    pub fn add_provided_cid(
        &self,
        cid: &ContentId,
        metadata: &ProvidedCidMetadata,
    ) -> Result<(), AppError> {
        let value = serde_json::to_vec(metadata).map_err(|e| {
            AppError::Storage(format!("Failed to serialize metadata: {}", e).into())
        })?;

        let cf = self.db.cf_handle(CF_PROVIDED_CIDS).ok_or_else(|| {
            AppError::Storage(
                "CF_PROVIDED_CIDS column family not found"
                    .to_string()
                    .into(),
            )
        })?;

        self.db.put_cf(&cf, cid.to_bytes(), value).map_err(|e| {
            error!("Failed to save provided CID {}: {}", cid, e);
            AppError::Storage(format!("Failed to save provided CID: {}", e).into())
        })?;

        debug!("Saved provided CID: {}", cid);
        Ok(())
    }

    /// Remove a provided CID from the store
    pub fn remove_provided_cid(&self, cid: &ContentId) -> Result<(), AppError> {
        let cf = self.db.cf_handle(CF_PROVIDED_CIDS).ok_or_else(|| {
            AppError::Storage(
                "CF_PROVIDED_CIDS column family not found"
                    .to_string()
                    .into(),
            )
        })?;

        self.db.delete_cf(&cf, cid.to_bytes()).map_err(|e| {
            AppError::Storage(format!("Failed to remove provided CID: {}", e).into())
        })?;

        debug!("Removed provided CID: {}", cid);
        Ok(())
    }

    /// Get metadata for a provided CID
    pub fn get_provided_cid(
        &self,
        cid: &ContentId,
    ) -> Result<Option<ProvidedCidMetadata>, AppError> {
        let cf = self.db.cf_handle(CF_PROVIDED_CIDS).ok_or_else(|| {
            AppError::Storage(
                "CF_PROVIDED_CIDS column family not found"
                    .to_string()
                    .into(),
            )
        })?;

        match self.db.get_cf(&cf, cid.to_bytes()) {
            Ok(Some(value)) => {
                let metadata: ProvidedCidMetadata =
                    serde_json::from_slice(&value).map_err(|e| {
                        AppError::Storage(format!("Failed to deserialize metadata: {}", e).into())
                    })?;
                Ok(Some(metadata))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(AppError::Storage(
                format!("Failed to get provided CID: {}", e).into(),
            )),
        }
    }

    /// Update metadata for a provided CID (e.g., after successful announcement)
    pub fn update_provided_cid(
        &self,
        cid: &ContentId,
        metadata: &ProvidedCidMetadata,
    ) -> Result<(), AppError> {
        // Same as add - RocksDB overwrites
        self.add_provided_cid(cid, metadata)
    }

    /// Load all provided CIDs and their metadata
    pub fn load_all_provided_cids(
        &self,
    ) -> Result<HashMap<ContentId, ProvidedCidMetadata>, AppError> {
        let cf = self.db.cf_handle(CF_PROVIDED_CIDS).ok_or_else(|| {
            AppError::Storage(
                "CF_PROVIDED_CIDS column family not found"
                    .to_string()
                    .into(),
            )
        })?;

        let mut result = HashMap::new();
        let iter = self.db.iterator_cf(&cf, rocksdb::IteratorMode::Start);

        for item in iter {
            let (key, value) = match item {
                Ok(kv) => kv,
                Err(e) => {
                    error!("Failed to read from provided CIDs CF: {}", e);
                    continue;
                }
            };

            // Parse CID from stored bytes (not hash - this is the serialized CID)
            let cid = match ContentId::from_raw_bytes(&key) {
                Ok(cid) => cid,
                Err(e) => {
                    warn!("Failed to parse ContentId from key: {}", e);
                    continue;
                }
            };

            let metadata: ProvidedCidMetadata = match serde_json::from_slice(&value) {
                Ok(m) => m,
                Err(e) => {
                    warn!("Failed to deserialize metadata for {}: {}", cid, e);
                    continue;
                }
            };

            result.insert(cid, metadata);
        }

        info!("Loaded {} provided CIDs from database", result.len());
        Ok(result)
    }

    /// Get count of provided CIDs
    pub fn count(&self) -> usize {
        let cf = match self.db.cf_handle(CF_PROVIDED_CIDS) {
            Some(cf) => cf,
            None => return 0,
        };

        self.db
            .iterator_cf(&cf, rocksdb::IteratorMode::Start)
            .count()
    }

    /// Check if a CID is provided
    pub fn contains(&self, cid: &ContentId) -> Result<bool, AppError> {
        let cf = self.db.cf_handle(CF_PROVIDED_CIDS).ok_or_else(|| {
            AppError::Storage(
                "CF_PROVIDED_CIDS column family not found"
                    .to_string()
                    .into(),
            )
        })?;

        match self.db.get_cf(&cf, cid.to_bytes()) {
            Ok(Some(_)) => Ok(true),
            Ok(None) => Ok(false),
            Err(e) => Err(AppError::Storage(
                format!("Failed to check provided CID: {}", e).into(),
            )),
        }
    }

    /// Flush all pending writes to disk
    pub fn flush(&self) -> Result<(), AppError> {
        self.db
            .flush()
            .map_err(|e| AppError::Storage(format!("Failed to flush database: {}", e).into()))?;
        debug!("Flushed provider registry database");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_store() -> (ProviderRegistryStore, TempDir) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let store = ProviderRegistryStore::new(temp_dir.path().join("provider_registry"))
            .expect("Failed to create store");
        (store, temp_dir)
    }

    #[test]
    fn test_add_and_load_provided_cid() {
        let (store, _temp) = create_test_store();
        let cid = ContentId::from_bytes(b"test content");
        let metadata = ProvidedCidMetadata::new(&cid);

        store
            .add_provided_cid(&cid, &metadata)
            .expect("Failed to add");

        let loaded = store.load_all_provided_cids().expect("Failed to load");
        assert_eq!(loaded.len(), 1);
        assert!(loaded.contains_key(&cid));
    }

    #[test]
    fn test_remove_provided_cid() {
        let (store, _temp) = create_test_store();
        let cid = ContentId::from_bytes(b"test content");
        let metadata = ProvidedCidMetadata::new(&cid);

        store
            .add_provided_cid(&cid, &metadata)
            .expect("Failed to add");
        store.remove_provided_cid(&cid).expect("Failed to remove");

        let loaded = store.load_all_provided_cids().expect("Failed to load");
        assert_eq!(loaded.len(), 0);
    }

    #[test]
    fn test_update_metadata() {
        let (store, _temp) = create_test_store();
        let cid = ContentId::from_bytes(b"test content");
        let mut metadata = ProvidedCidMetadata::new(&cid);

        store
            .add_provided_cid(&cid, &metadata)
            .expect("Failed to add");

        metadata.record_announcement();
        store
            .update_provided_cid(&cid, &metadata)
            .expect("Failed to update");

        let loaded = store
            .get_provided_cid(&cid)
            .expect("Failed to get")
            .unwrap();
        assert_eq!(loaded.announcement_count, 1);
        assert!(loaded.last_announced.is_some());
    }

    #[test]
    fn test_persistence_across_reopens() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let path = temp_dir.path().join("provider_registry");
        let cid = ContentId::from_bytes(b"persistent content");

        // Write with first store
        {
            let store = ProviderRegistryStore::new(&path).expect("Failed to create");
            let metadata = ProvidedCidMetadata::new(&cid);
            store
                .add_provided_cid(&cid, &metadata)
                .expect("Failed to add");
            store.flush().expect("Failed to flush");
        }

        // Read with second store
        {
            let store = ProviderRegistryStore::new(&path).expect("Failed to reopen");
            let loaded = store.load_all_provided_cids().expect("Failed to load");
            assert_eq!(loaded.len(), 1);
            assert!(loaded.contains_key(&cid));
        }
    }

    #[test]
    fn test_contains() {
        let (store, _temp) = create_test_store();
        let cid = ContentId::from_bytes(b"test content");
        let metadata = ProvidedCidMetadata::new(&cid);

        assert!(!store.contains(&cid).unwrap());

        store
            .add_provided_cid(&cid, &metadata)
            .expect("Failed to add");
        assert!(store.contains(&cid).unwrap());

        store.remove_provided_cid(&cid).expect("Failed to remove");
        assert!(!store.contains(&cid).unwrap());
    }

    #[test]
    fn test_count() {
        let (store, _temp) = create_test_store();
        assert_eq!(store.count(), 0);

        for i in 0..5 {
            let cid = ContentId::from_bytes(format!("content {}", i).as_bytes());
            let metadata = ProvidedCidMetadata::new(&cid);
            store
                .add_provided_cid(&cid, &metadata)
                .expect("Failed to add");
        }

        assert_eq!(store.count(), 5);
    }
}
