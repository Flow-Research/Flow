use errors::AppError;
use rocksdb::{DB, IteratorMode, Options};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tracing::{debug, error, info, instrument, warn};

/// Configuration for the key-value store
#[derive(Debug, Clone)]
pub struct KvConfig {
    pub path: PathBuf,
    pub enable_compression: bool,
    pub max_open_files: i32,
    pub write_buffer_size: usize,
}

impl Default for KvConfig {
    fn default() -> Self {
        Self {
            path: PathBuf::from("/tmp/flow-kv"),
            enable_compression: true,
            max_open_files: 1000,
            write_buffer_size: 64 * 1024 * 1024, // 64MB
        }
    }
}

impl KvConfig {
    /// Load configuration from environment variables
    pub fn from_env() -> Self {
        use std::env;

        let path = env::var("KV_STORE_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/tmp/flow-kv"));

        let enable_compression = env::var("KV_ENABLE_COMPRESSION")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(true);

        let max_open_files = env::var("KV_MAX_OPEN_FILES")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1000);

        let write_buffer_size = env::var("KV_WRITE_BUFFER_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(64 * 1024 * 1024); // 64MB

        Self {
            path,
            enable_compression,
            max_open_files,
            write_buffer_size,
        }
    }
}

/// Errors that can occur during KV operations
#[derive(Debug, thiserror::Error)]
pub enum KvError {
    #[error("Key not found")]
    NotFound,

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Configuration error: {0}")]
    Config(String),
}

impl From<KvError> for AppError {
    fn from(err: KvError) -> Self {
        AppError::Storage(format!("KV store error: {}", err).into())
    }
}

/// Trait defining key-value store operations
pub trait KvStore: Send + Sync {
    /// Get a value by key
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, KvError>;

    /// Put a key-value pair
    fn put(&self, key: &[u8], value: &[u8]) -> Result<(), KvError>;

    /// Delete a key
    fn delete(&self, key: &[u8]) -> Result<(), KvError>;

    /// Check if a key exists
    fn exists(&self, key: &[u8]) -> Result<bool, KvError>;

    /// Flush all pending writes to disk
    fn flush(&self) -> Result<(), KvError>;

    /// Iterate over all keys in the store
    fn keys(&self) -> Result<Vec<Vec<u8>>, KvError>;

    /// Get the number of keys in the store (approximate)
    fn len(&self) -> Result<usize, KvError>;

    /// Check if the store is empty
    fn is_empty(&self) -> Result<bool, KvError> {
        Ok(self.len()? == 0)
    }
}

/// RocksDB implementation of the KvStore trait
pub struct RocksDbKvStore {
    db: Arc<DB>,
    path: PathBuf,
}

impl RocksDbKvStore {
    /// Create a new RocksDB-backed KV store
    #[instrument(skip(path, config), fields(path = %path.as_ref().display()))]
    pub fn new<P: AsRef<Path>>(path: P, config: &KvConfig) -> Result<Self, KvError> {
        let path = path.as_ref();
        let path_str = path.display().to_string();

        info!(
            path = %path_str,
            enable_compression = config.enable_compression,
            max_open_files = config.max_open_files,
            "Opening RocksDB KV store"
        );

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                error!(path = %path_str, error = %e, "Failed to create parent directory");
                KvError::Config(format!("Failed to create directory: {}", e))
            })?;
        }

        // Configure RocksDB options
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.set_max_open_files(config.max_open_files);
        opts.set_write_buffer_size(config.write_buffer_size);
        opts.set_max_write_buffer_number(3);
        opts.set_keep_log_file_num(5);

        if config.enable_compression {
            opts.set_compression_type(rocksdb::DBCompressionType::Lz4);
            debug!("Compression enabled (LZ4)");
        }

        // Open database
        let db = DB::open(&opts, path).map_err(|e| {
            error!(path = %path_str, error = %e, "Failed to open RocksDB");
            KvError::Storage(format!("Failed to open database: {}", e))
        })?;

        let db = Arc::new(db);

        // Get approximate entry count
        let entry_count = db
            .property_int_value("rocksdb.estimate-num-keys")
            .unwrap_or(Some(0))
            .unwrap_or(0);

        info!(
            path = %path_str,
            entry_count = entry_count,
            "RocksDB KV store opened successfully"
        );

        Ok(Self {
            db,
            path: path.to_path_buf(),
        })
    }

    /// Get the path where this KV store is located
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Get reference to the underlying RocksDB instance (for advanced use)
    pub fn db(&self) -> &Arc<DB> {
        &self.db
    }
}

impl KvStore for RocksDbKvStore {
    #[instrument(skip(self, key), fields(key_len = key.len()))]
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, KvError> {
        debug!(key_len = key.len(), "Getting value");

        self.db.get(key).map_err(|e| {
            error!(error = %e, "Failed to get value");
            KvError::Storage(format!("Get operation failed: {}", e))
        })
    }

    #[instrument(skip(self, key, value), fields(key_len = key.len(), value_len = value.len()))]
    fn put(&self, key: &[u8], value: &[u8]) -> Result<(), KvError> {
        debug!(
            key_len = key.len(),
            value_len = value.len(),
            "Putting key-value pair"
        );

        self.db.put(key, value).map_err(|e| {
            error!(error = %e, "Failed to put value");
            KvError::Storage(format!("Put operation failed: {}", e))
        })
    }

    #[instrument(skip(self, key), fields(key_len = key.len()))]
    fn delete(&self, key: &[u8]) -> Result<(), KvError> {
        debug!(key_len = key.len(), "Deleting key");

        self.db.delete(key).map_err(|e| {
            error!(error = %e, "Failed to delete key");
            KvError::Storage(format!("Delete operation failed: {}", e))
        })
    }

    fn exists(&self, key: &[u8]) -> Result<bool, KvError> {
        self.get(key).map(|opt| opt.is_some())
    }

    #[instrument(skip(self))]
    fn flush(&self) -> Result<(), KvError> {
        debug!("Flushing KV store to disk");

        self.db.flush().map_err(|e| {
            error!(error = %e, "Failed to flush KV store");
            KvError::Storage(format!("Flush operation failed: {}", e))
        })?;

        info!("KV store flushed successfully");
        Ok(())
    }

    fn keys(&self) -> Result<Vec<Vec<u8>>, KvError> {
        let mut keys = Vec::new();
        let iter = self.db.iterator(IteratorMode::Start);

        for item in iter {
            match item {
                Ok((key, _)) => keys.push(key.to_vec()),
                Err(e) => {
                    error!(error = %e, "Iterator error while reading keys");
                    return Err(KvError::Storage(format!("Iterator error: {}", e)));
                }
            }
        }

        Ok(keys)
    }

    fn len(&self) -> Result<usize, KvError> {
        // Use RocksDB's approximate count
        let count = self
            .db
            .property_int_value("rocksdb.estimate-num-keys")
            .map_err(|e| {
                warn!(error = %e, "Failed to get approximate key count");
                KvError::Storage(format!("Failed to get key count: {}", e))
            })?
            .unwrap_or(0) as usize;

        Ok(count)
    }
}

impl Drop for RocksDbKvStore {
    fn drop(&mut self) {
        if let Err(e) = self.flush() {
            error!(error = %e, "Failed to flush KV store during drop");
        }
        info!(path = %self.path.display(), "RocksDB KV store closed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use tempfile::TempDir;

    fn create_test_store() -> (RocksDbKvStore, TempDir) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config = KvConfig {
            path: temp_dir.path().to_path_buf(),
            enable_compression: false, // Disable for faster tests
            max_open_files: 100,
            write_buffer_size: 1024 * 1024, // 1MB
        };

        let store = RocksDbKvStore::new(temp_dir.path(), &config).expect("Failed to create store");

        (store, temp_dir)
    }

    #[test]
    fn test_create_store() {
        let (store, _temp) = create_test_store();
        assert!(store.is_empty().unwrap());
    }

    #[test]
    fn test_put_and_get() {
        let (store, _temp) = create_test_store();

        let key = b"test_key";
        let value = b"test_value";

        store.put(key, value).unwrap();
        let retrieved = store.get(key).unwrap();

        assert_eq!(retrieved, Some(value.to_vec()));
    }

    #[test]
    fn test_get_nonexistent() {
        let (store, _temp) = create_test_store();

        let result = store.get(b"nonexistent").unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_delete() {
        let (store, _temp) = create_test_store();

        let key = b"test_key";
        let value = b"test_value";

        store.put(key, value).unwrap();
        assert!(store.exists(key).unwrap());

        store.delete(key).unwrap();
        assert!(!store.exists(key).unwrap());
    }

    #[test]
    fn test_exists() {
        let (store, _temp) = create_test_store();

        let key = b"test_key";
        assert!(!store.exists(key).unwrap());

        store.put(key, b"value").unwrap();
        assert!(store.exists(key).unwrap());
    }

    #[test]
    fn test_keys() {
        let (store, _temp) = create_test_store();

        store.put(b"key1", b"value1").unwrap();
        store.put(b"key2", b"value2").unwrap();
        store.put(b"key3", b"value3").unwrap();

        let keys = store.keys().unwrap();
        assert_eq!(keys.len(), 3);

        let key_set: std::collections::HashSet<_> = keys.into_iter().collect();
        assert!(key_set.contains(&b"key1".to_vec()));
        assert!(key_set.contains(&b"key2".to_vec()));
        assert!(key_set.contains(&b"key3".to_vec()));
    }

    #[test]
    fn test_len() {
        let (store, _temp) = create_test_store();

        assert_eq!(store.len().unwrap(), 0);

        store.put(b"key1", b"value1").unwrap();
        store.put(b"key2", b"value2").unwrap();

        let len = store.len().unwrap();
        assert!(len >= 2); // Approximate count
    }

    #[test]
    fn test_flush() {
        let (store, _temp) = create_test_store();

        store.put(b"key", b"value").unwrap();
        store.flush().unwrap();

        // Verify data survives flush
        assert_eq!(store.get(b"key").unwrap(), Some(b"value".to_vec()));
    }

    #[test]
    fn test_overwrite() {
        let (store, _temp) = create_test_store();

        let key = b"test_key";
        store.put(key, b"value1").unwrap();
        store.put(key, b"value2").unwrap();

        assert_eq!(store.get(key).unwrap(), Some(b"value2".to_vec()));
    }

    #[test]
    fn test_persistence() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().to_path_buf();
        let config = KvConfig {
            path: path.clone(),
            enable_compression: false,
            max_open_files: 100,
            write_buffer_size: 1024 * 1024,
        };

        // Create store, write data, and drop it
        {
            let store = RocksDbKvStore::new(&path, &config).unwrap();
            store.put(b"persistent_key", b"persistent_value").unwrap();
        }

        // Reopen and verify data persists
        {
            let store = RocksDbKvStore::new(&path, &config).unwrap();
            assert_eq!(
                store.get(b"persistent_key").unwrap(),
                Some(b"persistent_value".to_vec())
            );
        }
    }

    #[test]
    fn test_large_values() {
        let (store, _temp) = create_test_store();

        let key = b"large_key";
        let large_value = vec![42u8; 1024 * 1024]; // 1MB

        store.put(key, &large_value).unwrap();
        let retrieved = store.get(key).unwrap().unwrap();

        assert_eq!(retrieved.len(), large_value.len());
        assert_eq!(retrieved, large_value);
    }

    #[test]
    fn test_binary_keys_and_values() {
        let (store, _temp) = create_test_store();

        let key = &[0u8, 1, 2, 255, 254, 253];
        let value = &[100u8, 200, 50, 150];

        store.put(key, value).unwrap();
        assert_eq!(store.get(key).unwrap(), Some(value.to_vec()));
    }

    #[test]
    fn test_concurrent_reads() {
        use std::thread;

        let (store, _temp) = create_test_store();
        let store = Arc::new(store);

        // Write some data
        for i in 0u32..100 {
            store
                .put(&i.to_be_bytes(), &format!("value_{}", i).into_bytes())
                .unwrap();
        }

        // Spawn multiple reader threads
        let mut handles = vec![];
        for i in 0..10 {
            let store_clone = Arc::clone(&store);
            let handle = thread::spawn(move || {
                for j in 0u32..100 {
                    let value = store_clone.get(&j.to_be_bytes()).unwrap();
                    assert!(value.is_some());
                }
                i
            });
            handles.push(handle);
        }

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[test]
    #[serial]
    fn test_config_from_env() {
        use std::env;

        let temp_dir = TempDir::new().unwrap();
        let test_path = temp_dir.path().join("test_kv");

        unsafe {
            env::set_var("KV_STORE_PATH", test_path.to_str().unwrap());
            env::set_var("KV_ENABLE_COMPRESSION", "false");
            env::set_var("KV_MAX_OPEN_FILES", "500");
        }

        let config = KvConfig::from_env();

        assert_eq!(config.path, test_path);
        assert_eq!(config.enable_compression, false);
        assert_eq!(config.max_open_files, 500);

        unsafe {
            env::remove_var("KV_STORE_PATH");
            env::remove_var("KV_ENABLE_COMPRESSION");
            env::remove_var("KV_MAX_OPEN_FILES");
        }
    }

    #[test]
    fn test_trait_object_usage() {
        let (store, _temp) = create_test_store();
        let store_arc: Arc<dyn KvStore> = Arc::new(store);

        store_arc.put(b"key", b"value").unwrap();
        assert_eq!(store_arc.get(b"key").unwrap(), Some(b"value".to_vec()));
    }

    #[test]
    fn test_error_conversion() {
        let err = KvError::NotFound;
        let app_err: AppError = err.into();

        match app_err {
            AppError::Storage(msg) => assert!(msg.to_string().contains("not found")),
            _ => panic!("Wrong error variant"),
        }
    }
}
