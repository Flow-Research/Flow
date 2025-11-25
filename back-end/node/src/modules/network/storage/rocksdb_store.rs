use super::StorageConfig;
use errors::AppError;
use libp2p::{
    Multiaddr, PeerId,
    kad::{
        ProviderRecord, Record, RecordKey,
        store::{Error as StoreError, RecordStore, Result as StoreResult},
    },
};
use rocksdb::{ColumnFamilyDescriptor, DB, Options};
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, path::Path, sync::Arc, time::SystemTime};
use tracing::{debug, error, info, warn};
use web_time::Instant;

// Column family names
const CF_RECORDS: &str = "records";
const CF_PROVIDERS: &str = "providers";

/// Serializable wrapper for libp2p Record
#[derive(Serialize, Deserialize, Clone)]
struct SerializableRecord {
    key: Vec<u8>,
    value: Vec<u8>,
    publisher: Option<Vec<u8>>,
    expires_at: Option<SystemTime>,
}

impl SerializableRecord {
    fn from_record(record: &Record, base_time: Instant) -> Self {
        let expires_at = record.expires.map(|instant| {
            let duration_since_base = instant.duration_since(base_time);
            SystemTime::now() + duration_since_base
        });

        Self {
            key: record.key.to_vec(),
            value: record.value.clone(),
            publisher: record.publisher.map(|p| p.to_bytes()),
            expires_at,
        }
    }

    fn to_record(&self, base_time: Instant) -> Result<Record, String> {
        let expires = self.expires_at.map(|sys_time| {
            sys_time
                .duration_since(SystemTime::now())
                .map(|d| base_time + d)
                .unwrap_or(base_time)
        });

        let publisher = if let Some(ref bytes) = self.publisher {
            Some(PeerId::from_bytes(bytes).map_err(|e| format!("Invalid PeerId: {}", e))?)
        } else {
            None
        };

        Ok(Record {
            key: RecordKey::from(self.key.clone()),
            value: self.value.clone(),
            publisher,
            expires,
        })
    }
}

/// Serializable wrapper for libp2p ProviderRecord
#[derive(Serialize, Deserialize, Clone)]
struct SerializableProviderRecord {
    key: Vec<u8>,
    provider: Vec<u8>,
    expires_at: Option<SystemTime>,
    addresses: Vec<String>,
}

impl SerializableProviderRecord {
    fn from_provider_record(record: &ProviderRecord, base_time: Instant) -> Self {
        let expires_at = record.expires.map(|instant| {
            let duration_since_base = instant.duration_since(base_time);
            SystemTime::now() + duration_since_base
        });

        Self {
            key: record.key.to_vec(),
            provider: record.provider.to_bytes(),
            expires_at,
            addresses: record.addresses.iter().map(|a| a.to_string()).collect(),
        }
    }

    fn to_provider_record(&self, base_time: Instant) -> Result<ProviderRecord, String> {
        let expires = self.expires_at.map(|sys_time| {
            sys_time
                .duration_since(SystemTime::now())
                .map(|d| base_time + d)
                .unwrap_or(base_time)
        });

        let provider = PeerId::from_bytes(&self.provider)
            .map_err(|e| format!("Invalid provider PeerId: {}", e))?;

        let addresses: Result<Vec<Multiaddr>, _> = self
            .addresses
            .iter()
            .map(|s| s.parse().map_err(|e| format!("Invalid Multiaddr: {}", e)))
            .collect();

        Ok(ProviderRecord {
            key: RecordKey::from(self.key.clone()),
            provider,
            expires,
            addresses: addresses?,
        })
    }
}

/// RocksDB-backed implementation of Kademlia RecordStore
pub struct RocksDbStore {
    db: Arc<DB>,
    local_peer_id: PeerId,
    config: StorageConfig,
    base_time: Instant,
}

impl RocksDbStore {
    /// Create a new RocksDB store at the specified path
    pub fn new<P: AsRef<Path>>(
        path: P,
        local_peer_id: PeerId,
        config: StorageConfig,
    ) -> Result<Self, AppError> {
        info!("Opening RocksDB DHT store at: {}", path.as_ref().display());

        // Ensure parent directory exists
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                AppError::Storage(format!("Failed to create DB directory: {}", e).into())
            })?;
        }

        // Configure RocksDB options
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        if config.enable_compression {
            opts.set_compression_type(rocksdb::DBCompressionType::Lz4);
        }

        // Set performance options
        opts.set_max_open_files(1000);
        opts.set_keep_log_file_num(10);
        opts.set_max_total_wal_size(64 * 1024 * 1024); // 64MB WAL

        // Define column families
        let cf_records = ColumnFamilyDescriptor::new(CF_RECORDS, Options::default());
        let cf_providers = ColumnFamilyDescriptor::new(CF_PROVIDERS, Options::default());

        let db = DB::open_cf_descriptors(&opts, path, vec![cf_records, cf_providers])
            .map_err(|e| AppError::Storage(format!("Failed to open RocksDB: {}", e).into()))?;

        info!(
            "RocksDB DHT store opened successfully (max_records: {}, max_providers_per_key: {})",
            config.max_records, config.max_providers_per_key
        );

        Ok(Self {
            db: Arc::new(db),
            local_peer_id,
            config,
            base_time: Instant::now(),
        })
    }

    /// Count total records in the store
    pub fn record_count(&self) -> usize {
        let cf = self.db.cf_handle(CF_RECORDS).unwrap();
        self.db
            .iterator_cf(&cf, rocksdb::IteratorMode::Start)
            .count()
    }

    /// Count total provider records
    pub fn provider_count(&self) -> usize {
        let cf = self.db.cf_handle(CF_PROVIDERS).unwrap();
        self.db
            .iterator_cf(&cf, rocksdb::IteratorMode::Start)
            .count()
    }

    /// Check if a record is expired
    fn is_expired(&self, expires: Option<Instant>) -> bool {
        expires.map(|t| Instant::now() >= t).unwrap_or(false)
    }

    /// Serialize and store a record
    fn put_serialized_record(&mut self, record: &Record) -> StoreResult<()> {
        let serializable = SerializableRecord::from_record(record, self.base_time);
        let value = serde_json::to_vec(&serializable).map_err(|_| StoreError::ValueTooLarge)?;

        let cf = self.db.cf_handle(CF_RECORDS).unwrap();
        self.db
            .put_cf(&cf, record.key.as_ref(), value)
            .map_err(|e| {
                error!("Failed to put record: {}", e);
                StoreError::MaxRecords
            })?;

        debug!("Stored record with key: {:?}", record.key);
        Ok(())
    }

    /// Get providers for a specific key from column family
    fn get_providers_for_key(&self, key: &RecordKey) -> Vec<ProviderRecord> {
        let cf = self.db.cf_handle(CF_PROVIDERS).unwrap();
        let prefix = key.as_ref();

        let mut providers = Vec::new();
        let iter = self.db.prefix_iterator_cf(&cf, prefix);

        for item in iter {
            if let Ok((db_key, db_value)) = item {
                // Check if key matches (prefix iteration may return extra keys)
                if db_key.starts_with(prefix) {
                    if let Ok(serializable) =
                        serde_json::from_slice::<SerializableProviderRecord>(&db_value)
                    {
                        if let Ok(provider_record) = serializable.to_provider_record(self.base_time)
                        {
                            if !self.is_expired(provider_record.expires) {
                                providers.push(provider_record);
                            }
                        }
                    }
                }
            }
        }

        debug!("Retrieved {} providers for key", providers.len());
        providers
    }
}

impl RecordStore for RocksDbStore {
    type RecordsIter<'a> = RecordsIterator<'a>;
    type ProvidedIter<'a> = ProvidedIterator<'a>;

    fn get(&self, k: &RecordKey) -> Option<Cow<'_, Record>> {
        let cf = self.db.cf_handle(CF_RECORDS)?;

        match self.db.get_cf(&cf, k.as_ref()) {
            Ok(Some(value)) => match serde_json::from_slice::<SerializableRecord>(&value) {
                Ok(serializable) => match serializable.to_record(self.base_time) {
                    Ok(record) => {
                        if !self.is_expired(record.expires) {
                            debug!("Retrieved record for key: {:?}", k);
                            Some(Cow::Owned(record))
                        } else {
                            debug!("Record expired for key: {:?}", k);
                            None
                        }
                    }
                    Err(e) => {
                        warn!("Failed to deserialize record: {}", e);
                        None
                    }
                },
                Err(e) => {
                    warn!("Failed to deserialize record: {}", e);
                    None
                }
            },
            Ok(None) => None,
            Err(e) => {
                error!("Failed to get record: {}", e);
                None
            }
        }
    }

    fn put(&mut self, r: Record) -> StoreResult<()> {
        // Check value size limit
        if r.value.len() > self.config.max_value_bytes {
            warn!(
                "Record value too large: {} bytes (max: {})",
                r.value.len(),
                self.config.max_value_bytes
            );
            return Err(StoreError::ValueTooLarge);
        }

        // Check record count limit
        if self.record_count() >= self.config.max_records {
            warn!(
                "Record store at capacity: {} records",
                self.config.max_records
            );
            return Err(StoreError::MaxRecords);
        }

        self.put_serialized_record(&r)?;
        info!("Put record with key: {:?}", r.key);
        Ok(())
    }

    fn remove(&mut self, k: &RecordKey) {
        let cf = self.db.cf_handle(CF_RECORDS).unwrap();
        match self.db.delete_cf(&cf, k.as_ref()) {
            Ok(_) => {
                debug!("Removed record with key: {:?}", k);
            }
            Err(e) => {
                error!("Failed to remove record: {}", e);
            }
        }
    }

    fn records(&self) -> Self::RecordsIter<'_> {
        RecordsIterator::new(&self.db, self.base_time)
    }

    fn add_provider(&mut self, record: ProviderRecord) -> StoreResult<()> {
        let key = &record.key;
        let existing_providers = self.get_providers_for_key(key);

        // Check provider limit per key
        if existing_providers.len() >= self.config.max_providers_per_key {
            warn!(
                "Provider limit reached for key: {} providers",
                existing_providers.len()
            );
            return Err(StoreError::MaxProvidedKeys);
        }

        // Create composite key: record_key + provider_peer_id
        let mut composite_key = key.to_vec();
        composite_key.extend_from_slice(&record.provider.to_bytes());

        let serializable =
            SerializableProviderRecord::from_provider_record(&record, self.base_time);
        let value = serde_json::to_vec(&serializable).map_err(|_| StoreError::ValueTooLarge)?;

        let cf = self.db.cf_handle(CF_PROVIDERS).unwrap();
        self.db.put_cf(&cf, &composite_key, value).map_err(|e| {
            error!("Failed to add provider: {}", e);
            StoreError::MaxProvidedKeys
        })?;

        info!(
            "Added provider {:?} for key: {:?}",
            record.provider, record.key
        );
        Ok(())
    }

    fn providers(&self, key: &RecordKey) -> Vec<ProviderRecord> {
        self.get_providers_for_key(key)
    }

    fn provided(&self) -> Self::ProvidedIter<'_> {
        ProvidedIterator::new(&self.db, self.local_peer_id, self.base_time)
    }

    fn remove_provider(&mut self, k: &RecordKey, p: &PeerId) {
        let mut composite_key = k.to_vec();
        composite_key.extend_from_slice(&p.to_bytes());

        let cf = self.db.cf_handle(CF_PROVIDERS).unwrap();
        match self.db.delete_cf(&cf, &composite_key) {
            Ok(_) => {
                debug!("Removed provider {:?} for key: {:?}", p, k);
            }
            Err(e) => {
                error!("Failed to remove provider: {}", e);
            }
        }
    }
}

/// Iterator over all records
pub struct RecordsIterator<'a> {
    iter: rocksdb::DBIterator<'a>,
    base_time: Instant,
}

impl<'a> RecordsIterator<'a> {
    fn new(db: &'a DB, base_time: Instant) -> Self {
        let cf = db.cf_handle(CF_RECORDS).unwrap();
        let iter = db.iterator_cf(&cf, rocksdb::IteratorMode::Start);
        Self { iter, base_time }
    }
}

impl<'a> Iterator for RecordsIterator<'a> {
    type Item = Cow<'a, Record>;

    fn next(&mut self) -> Option<Self::Item> {
        for item in &mut self.iter {
            if let Ok((_key, value)) = item {
                if let Ok(serializable) = serde_json::from_slice::<SerializableRecord>(&value) {
                    if let Ok(record) = serializable.to_record(self.base_time) {
                        let is_expired =
                            record.expires.map(|t| Instant::now() >= t).unwrap_or(false);
                        if !is_expired {
                            return Some(Cow::Owned(record));
                        }
                    }
                }
            }
        }
        None
    }
}

/// Iterator over provided records (records we provide)
pub struct ProvidedIterator<'a> {
    iter: rocksdb::DBIterator<'a>,
    local_peer_id: PeerId,
    base_time: Instant,
}

impl<'a> ProvidedIterator<'a> {
    fn new(db: &'a DB, local_peer_id: PeerId, base_time: Instant) -> Self {
        let cf = db.cf_handle(CF_PROVIDERS).unwrap();
        let iter = db.iterator_cf(&cf, rocksdb::IteratorMode::Start);
        Self {
            iter,
            local_peer_id,
            base_time,
        }
    }
}

impl<'a> Iterator for ProvidedIterator<'a> {
    type Item = Cow<'a, ProviderRecord>;

    fn next(&mut self) -> Option<Self::Item> {
        for item in &mut self.iter {
            if let Ok((_key, value)) = item {
                if let Ok(serializable) =
                    serde_json::from_slice::<SerializableProviderRecord>(&value)
                {
                    if let Ok(record) = serializable.to_provider_record(self.base_time) {
                        if record.provider == self.local_peer_id {
                            let is_expired =
                                record.expires.map(|t| Instant::now() >= t).unwrap_or(false);
                            if !is_expired {
                                return Some(Cow::Owned(record));
                            }
                        }
                    }
                }
            }
        }
        None
    }
}

impl Drop for RocksDbStore {
    fn drop(&mut self) {
        info!("Closing RocksDB DHT store");
    }
}

///////////////////
///// TESTS
///////////////////

#[cfg(test)]
mod tests {
    use super::*;
    use libp2p::identity::Keypair;
    use std::time::Duration;
    use tempfile::TempDir;

    // ==================== TEST HELPERS ====================

    fn create_temp_store() -> (RocksDbStore, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let peer_id = Keypair::generate_ed25519().public().to_peer_id();
        let config = StorageConfig {
            db_path: temp_dir.path().to_path_buf(),
            max_records: 100,
            max_providers_per_key: 5,
            max_value_bytes: 1024,
            enable_compression: false,
        };
        let store = RocksDbStore::new(temp_dir.path(), peer_id, config.clone()).unwrap();
        (store, temp_dir)
    }

    fn create_test_peer_id() -> PeerId {
        Keypair::generate_ed25519().public().to_peer_id()
    }

    fn create_test_record(key: &str, value: &str) -> Record {
        Record::new(RecordKey::new(&key.as_bytes()), value.as_bytes().to_vec())
    }

    fn create_test_multiaddr() -> Multiaddr {
        "/ip4/127.0.0.1/tcp/4001".parse().unwrap()
    }

    // ==================== STORE CREATION & INITIALIZATION ====================

    #[test]
    fn test_store_creation() {
        let (store, _temp_dir) = create_temp_store();
        assert_eq!(store.record_count(), 0);
        assert_eq!(store.provider_count(), 0);
    }

    #[test]
    fn test_store_creation_with_compression() {
        let temp_dir = TempDir::new().unwrap();
        let peer_id = create_test_peer_id();
        let config = StorageConfig {
            db_path: temp_dir.path().to_path_buf(),
            max_records: 100,
            max_providers_per_key: 5,
            max_value_bytes: 1024,
            enable_compression: true,
        };
        let store = RocksDbStore::new(temp_dir.path(), peer_id, config).unwrap();
        assert_eq!(store.record_count(), 0);
    }

    #[test]
    fn test_store_creation_creates_parent_directory() {
        let temp_dir = TempDir::new().unwrap();
        let nested_path = temp_dir.path().join("nested").join("path").join("db");
        let peer_id = create_test_peer_id();
        let config = StorageConfig::default();

        let store = RocksDbStore::new(&nested_path, peer_id, config);
        assert!(store.is_ok());
        assert!(nested_path.parent().unwrap().exists());
    }

    // ==================== RECORD PUT & GET ====================

    #[test]
    fn test_put_and_get_record() {
        let (mut store, _temp_dir) = create_temp_store();

        let record = create_test_record("test_key", "test_value");
        let key = record.key.clone();

        // Put record
        assert!(store.put(record).is_ok());

        // Get record
        let retrieved = store.get(&key);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().value, b"test_value");
    }

    #[test]
    fn test_get_nonexistent_record() {
        let (store, _temp_dir) = create_temp_store();

        let key = RecordKey::new(&b"nonexistent");
        let result = store.get(&key);
        assert!(result.is_none());
    }

    #[test]
    fn test_put_record_with_publisher() {
        let (mut store, _temp_dir) = create_temp_store();

        let publisher = create_test_peer_id();
        let key = RecordKey::new(&b"test_key");
        let mut record = Record::new(key.clone(), b"value".to_vec());
        record.publisher = Some(publisher);

        store.put(record).unwrap();

        let retrieved = store.get(&key).unwrap();
        assert_eq!(retrieved.publisher, Some(publisher));
    }

    #[test]
    fn test_put_multiple_records() {
        let (mut store, _temp_dir) = create_temp_store();

        for i in 0..10 {
            let record = create_test_record(&format!("key_{}", i), &format!("value_{}", i));
            assert!(store.put(record).is_ok());
        }

        assert_eq!(store.record_count(), 10);
    }

    #[test]
    fn test_put_overwrites_existing_record() {
        let (mut store, _temp_dir) = create_temp_store();

        let key = RecordKey::new(&b"test_key");
        let record1 = Record::new(key.clone(), b"value1".to_vec());
        let record2 = Record::new(key.clone(), b"value2".to_vec());

        store.put(record1).unwrap();
        store.put(record2).unwrap();

        let retrieved = store.get(&key).unwrap();
        assert_eq!(retrieved.value, b"value2");
        assert_eq!(store.record_count(), 1); // Still only 1 record
    }

    // ==================== RECORD REMOVAL ====================

    #[test]
    fn test_remove_record() {
        let (mut store, _temp_dir) = create_temp_store();

        let record = create_test_record("test_key", "test_value");
        let key = record.key.clone();

        store.put(record).unwrap();
        assert!(store.get(&key).is_some());

        store.remove(&key);
        assert!(store.get(&key).is_none());
    }

    #[test]
    fn test_remove_nonexistent_record() {
        let (mut store, _temp_dir) = create_temp_store();

        let key = RecordKey::new(&b"nonexistent");
        // Should not panic or error
        store.remove(&key);
        assert_eq!(store.record_count(), 0);
    }

    #[test]
    fn test_remove_multiple_records() {
        let (mut store, _temp_dir) = create_temp_store();

        // Add 10 records
        for i in 0..10 {
            let record = create_test_record(&format!("key_{}", i), &format!("value_{}", i));
            store.put(record).unwrap();
        }

        // Remove 5 records
        for i in 0..5 {
            let key = RecordKey::new(&format!("key_{}", i).as_bytes());
            store.remove(&key);
        }

        assert_eq!(store.record_count(), 5);
    }

    // ==================== CAPACITY LIMITS ====================

    #[test]
    fn test_record_capacity_limit() {
        let (mut store, _temp_dir) = create_temp_store();

        // Fill up to max capacity (100)
        for i in 0..100 {
            let record = create_test_record(&format!("key_{}", i), "value");
            assert!(store.put(record).is_ok());
        }

        // Next insert should fail
        let record = create_test_record("overflow", "value");
        let result = store.put(record);
        assert!(matches!(result, Err(StoreError::MaxRecords)));
    }

    #[test]
    fn test_value_size_limit() {
        let (mut store, _temp_dir) = create_temp_store();

        let key = RecordKey::new(&b"large");
        let large_value = vec![0u8; 2048]; // Exceeds 1024 byte limit
        let record = Record::new(key, large_value);

        let result = store.put(record);
        assert!(matches!(result, Err(StoreError::ValueTooLarge)));
    }

    #[test]
    fn test_value_at_exact_limit() {
        let (mut store, _temp_dir) = create_temp_store();

        let key = RecordKey::new(&b"exact");
        let value = vec![0u8; 1024]; // Exactly at limit
        let record = Record::new(key.clone(), value);

        assert!(store.put(record).is_ok());
        assert!(store.get(&key).is_some());
    }

    // ==================== PROVIDER RECORDS ====================

    #[test]
    fn test_add_and_get_provider() {
        let (mut store, _temp_dir) = create_temp_store();

        let key = RecordKey::new(&b"content_key");
        let provider = create_test_peer_id();
        let addr = create_test_multiaddr();

        let provider_record = ProviderRecord::new(key.clone(), provider, vec![addr]);
        assert!(store.add_provider(provider_record).is_ok());

        let providers = store.providers(&key);
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].provider, provider);
    }

    #[test]
    fn test_add_multiple_providers_for_same_key() {
        let (mut store, _temp_dir) = create_temp_store();

        let key = RecordKey::new(&b"content_key");
        let addr = create_test_multiaddr();

        // Add 3 different providers for the same key
        for _ in 0..3 {
            let provider = create_test_peer_id();
            let record = ProviderRecord::new(key.clone(), provider, vec![addr.clone()]);
            assert!(store.add_provider(record).is_ok());
        }

        let providers = store.providers(&key);
        assert_eq!(providers.len(), 3);
    }

    #[test]
    fn test_provider_capacity_limit_per_key() {
        let (mut store, _temp_dir) = create_temp_store();

        let key = RecordKey::new(&b"content_key");
        let addr = create_test_multiaddr();

        // Add up to max providers (5)
        for _ in 0..5 {
            let provider = create_test_peer_id();
            let record = ProviderRecord::new(key.clone(), provider, vec![addr.clone()]);
            assert!(store.add_provider(record).is_ok());
        }

        // Next provider should fail
        let provider = create_test_peer_id();
        let record = ProviderRecord::new(key.clone(), provider, vec![addr]);
        assert!(matches!(
            store.add_provider(record),
            Err(StoreError::MaxProvidedKeys)
        ));
    }

    #[test]
    fn test_provider_with_multiple_addresses() {
        let (mut store, _temp_dir) = create_temp_store();

        let key = RecordKey::new(&b"content_key");
        let provider = create_test_peer_id();
        let addr1: Multiaddr = "/ip4/127.0.0.1/tcp/4001".parse().unwrap();
        let addr2: Multiaddr = "/ip6/::1/tcp/4001".parse().unwrap();
        let addr3: Multiaddr = "/ip4/192.168.1.1/tcp/8080".parse().unwrap();

        let record = ProviderRecord::new(
            key.clone(),
            provider,
            vec![addr1.clone(), addr2.clone(), addr3.clone()],
        );
        store.add_provider(record).unwrap();

        let providers = store.providers(&key);
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].addresses.len(), 3);
        assert!(providers[0].addresses.contains(&addr1));
        assert!(providers[0].addresses.contains(&addr2));
        assert!(providers[0].addresses.contains(&addr3));
    }

    #[test]
    fn test_remove_provider() {
        let (mut store, _temp_dir) = create_temp_store();

        let key = RecordKey::new(&b"content_key");
        let provider = create_test_peer_id();
        let addr = create_test_multiaddr();

        let record = ProviderRecord::new(key.clone(), provider, vec![addr]);
        store.add_provider(record).unwrap();

        assert_eq!(store.providers(&key).len(), 1);

        store.remove_provider(&key, &provider);
        assert_eq!(store.providers(&key).len(), 0);
    }

    #[test]
    fn test_remove_nonexistent_provider() {
        let (mut store, _temp_dir) = create_temp_store();

        let key = RecordKey::new(&b"content_key");
        let provider = create_test_peer_id();

        // Should not panic or error
        store.remove_provider(&key, &provider);
    }

    #[test]
    fn test_remove_specific_provider_keeps_others() {
        let (mut store, _temp_dir) = create_temp_store();

        let key = RecordKey::new(&b"content_key");
        let addr = create_test_multiaddr();

        let provider1 = create_test_peer_id();
        let provider2 = create_test_peer_id();

        store
            .add_provider(ProviderRecord::new(
                key.clone(),
                provider1,
                vec![addr.clone()],
            ))
            .unwrap();
        store
            .add_provider(ProviderRecord::new(key.clone(), provider2, vec![addr]))
            .unwrap();

        assert_eq!(store.providers(&key).len(), 2);

        store.remove_provider(&key, &provider1);

        let remaining = store.providers(&key);
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].provider, provider2);
    }

    // ==================== EXPIRATION ====================

    #[test]
    fn test_expired_record_not_retrieved() {
        let (mut store, _temp_dir) = create_temp_store();

        let key = RecordKey::new(&b"expiring_key");
        let mut record = Record::new(key.clone(), b"value".to_vec());

        // Set expiration in the past
        record.expires = Some(Instant::now() - Duration::from_secs(10));

        store.put(record).unwrap();

        // Should not retrieve expired record
        let retrieved = store.get(&key);
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_non_expired_record_is_retrieved() {
        let (mut store, _temp_dir) = create_temp_store();

        let key = RecordKey::new(&b"future_key");
        let mut record = Record::new(key.clone(), b"value".to_vec());

        // Set expiration in the future
        record.expires = Some(Instant::now() + Duration::from_secs(3600));

        store.put(record).unwrap();

        // Should retrieve non-expired record
        let retrieved = store.get(&key);
        assert!(retrieved.is_some());
    }

    #[test]
    fn test_record_without_expiration() {
        let (mut store, _temp_dir) = create_temp_store();

        let key = RecordKey::new(&b"no_expiry");
        let record = Record::new(key.clone(), b"value".to_vec());

        store.put(record).unwrap();

        // Should retrieve record without expiration
        let retrieved = store.get(&key);
        assert!(retrieved.is_some());
    }

    #[test]
    fn test_expired_provider_not_retrieved() {
        let (mut store, _temp_dir) = create_temp_store();

        let key = RecordKey::new(&b"content_key");
        let provider = create_test_peer_id();
        let mut record = ProviderRecord::new(key.clone(), provider, vec![create_test_multiaddr()]);

        // Set expiration in the past
        record.expires = Some(Instant::now() - Duration::from_secs(10));

        store.add_provider(record).unwrap();

        // Should not retrieve expired provider
        let providers = store.providers(&key);
        assert_eq!(providers.len(), 0);
    }

    // ==================== PERSISTENCE ====================

    #[test]
    fn test_persistence_across_restart() {
        let temp_dir = TempDir::new().unwrap();
        let peer_id = create_test_peer_id();
        let config = StorageConfig {
            db_path: temp_dir.path().to_path_buf(),
            max_records: 100,
            max_providers_per_key: 5,
            max_value_bytes: 1024,
            enable_compression: false,
        };

        let key = RecordKey::new(&b"persistent_key");
        let value = b"persistent_value".to_vec();

        // Create store and write data
        {
            let mut store = RocksDbStore::new(temp_dir.path(), peer_id, config.clone()).unwrap();
            let record = Record::new(key.clone(), value.clone());
            store.put(record).unwrap();
        }

        // Reopen store and verify data persists
        {
            let store = RocksDbStore::new(temp_dir.path(), peer_id, config).unwrap();
            let retrieved = store.get(&key);
            assert!(retrieved.is_some());
            assert_eq!(retrieved.unwrap().value, value);
        }
    }

    #[test]
    fn test_provider_persistence_across_restart() {
        let temp_dir = TempDir::new().unwrap();
        let peer_id = create_test_peer_id();
        let config = StorageConfig {
            db_path: temp_dir.path().to_path_buf(),
            max_records: 100,
            max_providers_per_key: 5,
            max_value_bytes: 1024,
            enable_compression: false,
        };

        let key = RecordKey::new(&b"content_key");
        let provider = create_test_peer_id();
        let addr = create_test_multiaddr();

        // Create store and write provider
        {
            let mut store = RocksDbStore::new(temp_dir.path(), peer_id, config.clone()).unwrap();
            let record = ProviderRecord::new(key.clone(), provider, vec![addr.clone()]);
            store.add_provider(record).unwrap();
        }

        // Reopen store and verify provider persists
        {
            let store = RocksDbStore::new(temp_dir.path(), peer_id, config).unwrap();
            let providers = store.providers(&key);
            assert_eq!(providers.len(), 1);
            assert_eq!(providers[0].provider, provider);
            assert!(providers[0].addresses.contains(&addr));
        }
    }

    #[test]
    fn test_multiple_records_persist() {
        let temp_dir = TempDir::new().unwrap();
        let peer_id = create_test_peer_id();
        let config = StorageConfig {
            db_path: temp_dir.path().to_path_buf(),
            max_records: 100,
            max_providers_per_key: 5,
            max_value_bytes: 1024,
            enable_compression: false,
        };

        // Create store and write multiple records
        {
            let mut store = RocksDbStore::new(temp_dir.path(), peer_id, config.clone()).unwrap();
            for i in 0..50 {
                let record = create_test_record(&format!("key_{}", i), &format!("value_{}", i));
                store.put(record).unwrap();
            }
        }

        // Reopen store and verify all records persist
        {
            let store = RocksDbStore::new(temp_dir.path(), peer_id, config).unwrap();
            assert_eq!(store.record_count(), 50);

            for i in 0..50 {
                let key = RecordKey::new(&format!("key_{}", i).as_bytes());
                let retrieved = store.get(&key);
                assert!(retrieved.is_some());
                assert_eq!(retrieved.unwrap().value, format!("value_{}", i).as_bytes());
            }
        }
    }

    // ==================== ITERATORS ====================

    #[test]
    fn test_records_iterator_empty() {
        let (store, _temp_dir) = create_temp_store();
        let count = store.records().count();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_records_iterator() {
        let (mut store, _temp_dir) = create_temp_store();

        // Add multiple records
        for i in 0..10 {
            let record = create_test_record(&format!("key_{}", i), &format!("value_{}", i));
            store.put(record).unwrap();
        }

        // Iterate and count
        let count = store.records().count();
        assert_eq!(count, 10);
    }

    #[test]
    fn test_records_iterator_skips_expired() {
        let (mut store, _temp_dir) = create_temp_store();

        // Add non-expired record
        let record1 = create_test_record("key_1", "value_1");
        store.put(record1).unwrap();

        // Add expired record
        let mut record2 = create_test_record("key_2", "value_2");
        record2.expires = Some(Instant::now() - Duration::from_secs(10));
        store.put(record2).unwrap();

        // Add another non-expired record
        let record3 = create_test_record("key_3", "value_3");
        store.put(record3).unwrap();

        // Iterator should only return non-expired records
        let count = store.records().count();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_provided_iterator_empty() {
        let (store, _temp_dir) = create_temp_store();
        let count = store.provided().count();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_provided_iterator_filters_by_local_peer() {
        let temp_dir = TempDir::new().unwrap();
        let local_peer_id = create_test_peer_id();
        let other_peer_id = create_test_peer_id();

        let config = StorageConfig {
            db_path: temp_dir.path().to_path_buf(),
            max_records: 100,
            max_providers_per_key: 5,
            max_value_bytes: 1024,
            enable_compression: false,
        };

        let mut store = RocksDbStore::new(temp_dir.path(), local_peer_id, config).unwrap();

        let key = RecordKey::new(&b"content_key");
        let addr = create_test_multiaddr();

        // Add provider record for local peer
        let local_record = ProviderRecord::new(key.clone(), local_peer_id, vec![addr.clone()]);
        store.add_provider(local_record).unwrap();

        // Add provider record for other peer
        let other_record = ProviderRecord::new(key.clone(), other_peer_id, vec![addr]);
        store.add_provider(other_record).unwrap();

        // provided() should only return records where we are the provider
        let provided_count = store.provided().count();
        assert_eq!(provided_count, 1);
    }

    // ==================== EDGE CASES ====================

    #[test]
    fn test_empty_value_record() {
        let (mut store, _temp_dir) = create_temp_store();

        let key = RecordKey::new(&b"empty");
        let record = Record::new(key.clone(), vec![]);

        store.put(record).unwrap();

        let retrieved = store.get(&key);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().value.len(), 0);
    }

    #[test]
    fn test_binary_value_record() {
        let (mut store, _temp_dir) = create_temp_store();

        let key = RecordKey::new(&b"binary");
        let binary_value = vec![0xFF, 0x00, 0xAB, 0xCD, 0xEF];
        let record = Record::new(key.clone(), binary_value.clone());

        store.put(record).unwrap();

        let retrieved = store.get(&key);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().value, binary_value);
    }

    #[test]
    fn test_unicode_in_values() {
        let (mut store, _temp_dir) = create_temp_store();

        let key = RecordKey::new(&b"unicode");
        let unicode_value = "Hello 世界 🌍".as_bytes().to_vec();
        let record = Record::new(key.clone(), unicode_value.clone());

        store.put(record).unwrap();

        let retrieved = store.get(&key);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().value, unicode_value);
    }

    #[test]
    fn test_providers_for_nonexistent_key() {
        let (store, _temp_dir) = create_temp_store();

        let key = RecordKey::new(&b"nonexistent");
        let providers = store.providers(&key);
        assert_eq!(providers.len(), 0);
    }

    #[test]
    fn test_provider_with_no_addresses() {
        let (mut store, _temp_dir) = create_temp_store();

        let key = RecordKey::new(&b"content_key");
        let provider = create_test_peer_id();
        let record = ProviderRecord::new(key.clone(), provider, vec![]);

        store.add_provider(record).unwrap();

        let providers = store.providers(&key);
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].addresses.len(), 0);
    }

    // ==================== CONCURRENCY ====================

    #[test]
    fn test_concurrent_reads() {
        use std::sync::Arc;
        use std::thread;

        let (mut store, _temp_dir) = create_temp_store();

        // Pre-populate with data
        for i in 0..20 {
            let record = create_test_record(&format!("key_{}", i), &format!("value_{}", i));
            store.put(record).unwrap();
        }

        let store = Arc::new(store);
        let mut handles = vec![];

        // Spawn multiple reader threads
        for i in 0..10 {
            let store_clone = Arc::clone(&store);
            let handle = thread::spawn(move || {
                let key = RecordKey::new(&format!("key_{}", i).as_bytes());
                store_clone.get(&key).is_some()
            });
            handles.push(handle);
        }

        // All reads should succeed
        for handle in handles {
            let result = handle.join().unwrap();
            assert!(result, "Concurrent read failed");
        }
    }

    #[test]
    fn test_sequential_writes_and_reads() {
        let (mut store, _temp_dir) = create_temp_store();

        // Write and immediately read in sequence
        for i in 0..50 {
            let record = create_test_record(&format!("key_{}", i), &format!("value_{}", i));
            let key = record.key.clone();

            store.put(record).unwrap();

            let retrieved = store.get(&key);
            assert!(retrieved.is_some());
            assert_eq!(retrieved.unwrap().value, format!("value_{}", i).as_bytes());
        }
    }

    // ==================== PERFORMANCE & STRESS ====================

    #[test]
    fn test_large_dataset() {
        let temp_dir = TempDir::new().unwrap();
        let peer_id = create_test_peer_id();
        let config = StorageConfig {
            db_path: temp_dir.path().to_path_buf(),
            max_records: 10000,
            max_providers_per_key: 20,
            max_value_bytes: 4096,
            enable_compression: true,
        };

        let mut store = RocksDbStore::new(temp_dir.path(), peer_id, config).unwrap();

        // Add 1000 records
        for i in 0..1000 {
            let value = format!("value_{}_", i).repeat(10); // ~100 bytes each
            let record = create_test_record(&format!("key_{}", i), &value);
            store.put(record).unwrap();
        }

        assert_eq!(store.record_count(), 1000);

        // Verify random access
        let key = RecordKey::new(&b"key_500");
        assert!(store.get(&key).is_some());
    }

    #[test]
    fn test_mixed_operations() {
        let (mut store, _temp_dir) = create_temp_store();

        // Mix of operations
        for i in 0..30 {
            let record = create_test_record(&format!("key_{}", i), &format!("value_{}", i));
            store.put(record).unwrap();

            if i % 3 == 0 {
                let key = RecordKey::new(&format!("key_{}", i / 2).as_bytes());
                store.remove(&key);
            }

            if i % 5 == 0 {
                let provider_key = RecordKey::new(&format!("content_{}", i).as_bytes());
                let provider = create_test_peer_id();
                let record =
                    ProviderRecord::new(provider_key, provider, vec![create_test_multiaddr()]);
                let _ = store.add_provider(record);
            }
        }

        // Store should still be in a valid state
        assert!(store.record_count() > 0);
        assert!(store.provider_count() > 0);
    }

    // ==================== ERROR HANDLING ====================

    #[test]
    fn test_corrupted_serialized_data_handling() {
        let (mut store, _temp_dir) = create_temp_store();

        let key = RecordKey::new(&b"test_key");
        let record = Record::new(key.clone(), b"value".to_vec());
        store.put(record).unwrap();

        // Manually corrupt the data (direct DB access)
        let cf = store.db.cf_handle(CF_RECORDS).unwrap();
        store
            .db
            .put_cf(&cf, key.as_ref(), b"corrupted_json")
            .unwrap();

        // Get should return None on deserialization error
        let result = store.get(&key);
        assert!(result.is_none());
    }

    #[test]
    fn test_invalid_multiaddr_in_provider() {
        // This tests the serialization/deserialization roundtrip
        // with edge case multiaddrs
        let (mut store, _temp_dir) = create_temp_store();

        let key = RecordKey::new(&b"content_key");
        let provider = create_test_peer_id();

        // Valid but uncommon multiaddrs
        let addrs = vec![
            "/ip4/0.0.0.0/tcp/0".parse().unwrap(),
            "/ip6/::/tcp/0".parse().unwrap(),
        ];

        let record = ProviderRecord::new(key.clone(), provider, addrs.clone());
        store.add_provider(record).unwrap();

        let providers = store.providers(&key);
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].addresses, addrs);
    }

    // ==================== SPECIAL SCENARIOS ====================

    #[test]
    fn test_same_key_different_providers() {
        let (mut store, _temp_dir) = create_temp_store();

        let key = RecordKey::new(&b"popular_content");

        // Add 5 different providers for the same content
        for i in 0..5 {
            let provider = create_test_peer_id();
            let addr: Multiaddr = format!("/ip4/127.0.0.{}/tcp/4001", i + 1).parse().unwrap();
            let record = ProviderRecord::new(key.clone(), provider, vec![addr]);
            store.add_provider(record).unwrap();
        }

        let providers = store.providers(&key);
        assert_eq!(providers.len(), 5);

        // All providers should have different peer IDs
        let peer_ids: Vec<PeerId> = providers.iter().map(|p| p.provider).collect();
        let unique_peers: std::collections::HashSet<PeerId> = peer_ids.iter().cloned().collect();
        assert_eq!(unique_peers.len(), 5);
    }

    #[test]
    fn test_record_update_preserves_count() {
        let (mut store, _temp_dir) = create_temp_store();

        let key = RecordKey::new(&b"update_key");

        // Insert initial record
        let record1 = Record::new(key.clone(), b"value1".to_vec());
        store.put(record1).unwrap();
        assert_eq!(store.record_count(), 1);

        // Update with new value
        let record2 = Record::new(key.clone(), b"value2_updated".to_vec());
        store.put(record2).unwrap();
        assert_eq!(store.record_count(), 1); // Count should not increase

        // Verify updated value
        let retrieved = store.get(&key).unwrap();
        assert_eq!(retrieved.value, b"value2_updated");
    }

    #[test]
    fn test_drop_closes_db_cleanly() {
        let temp_dir = TempDir::new().unwrap();
        let peer_id = create_test_peer_id();
        let config = StorageConfig::default();
        let db_path = temp_dir.path().to_path_buf();

        {
            let mut store = RocksDbStore::new(&db_path, peer_id, config.clone()).unwrap();
            store.put(create_test_record("key", "value")).unwrap();
            // Drop happens here
        }

        // Should be able to reopen without corruption
        let store = RocksDbStore::new(&db_path, peer_id, config).unwrap();
        let key = RecordKey::new(&b"key");
        assert!(store.get(&key).is_some());
    }
}
