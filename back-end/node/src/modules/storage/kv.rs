use crate::utils::env::{env_bool, env_i32, env_path, env_usize};
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
        Self {
            path: env_path("KV_STORE_PATH", "/tmp/flow-kv"),
            enable_compression: env_bool("KV_ENABLE_COMPRESSION", true),
            max_open_files: env_i32("KV_MAX_OPEN_FILES", 1000),
            write_buffer_size: env_usize("KV_WRITE_BUFFER_SIZE", 64 * 1024 * 1024),
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
