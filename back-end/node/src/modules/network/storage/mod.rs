pub mod peer_registry_store;
pub mod provider_registry_store;
pub mod rocksdb_store;

use std::env;
use std::path::PathBuf;

pub use rocksdb_store::RocksDbStore;

use crate::bootstrap::init::get_flow_data_dir;

/// Configuration for persistent DHT storage
#[derive(Debug, Clone)]
pub struct StorageConfig {
    /// Path to RocksDB directory
    pub db_path: PathBuf,

    /// Maximum number of records to store
    pub max_records: usize,

    /// Maximum number of provider records per key
    pub max_providers_per_key: usize,

    /// Maximum size of a single record value (bytes)
    pub max_value_bytes: usize,

    /// Enable RocksDB compression (LZ4)
    pub enable_compression: bool,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            db_path: Self::default_db_path(),
            max_records: 65536,        // 64K records
            max_providers_per_key: 20, // libp2p K_VALUE constant
            max_value_bytes: 65536,    // 64KB max value size
            enable_compression: true,
        }
    }
}

impl StorageConfig {
    /// Get default database path (platform-specific, consistent with Flow config)
    fn default_db_path() -> PathBuf {
        get_flow_data_dir().join("network").join("kad_db")
    }

    /// Load configuration from environment variables
    ///
    /// Environment variables:
    /// - DHT_DB_PATH: Database directory path
    /// - DHT_MAX_RECORDS: Maximum record count (default: 65536)
    /// - DHT_MAX_PROVIDERS: Maximum providers per key (default: 20)
    /// - DHT_MAX_VALUE_SIZE: Maximum value size in bytes (default: 65536)
    /// - DHT_ENABLE_COMPRESSION: Enable compression (default: true)
    pub fn from_env() -> Self {
        let db_path = env::var("DHT_DB_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| Self::default_db_path());

        let max_records = env::var("DHT_MAX_RECORDS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(65536);

        let max_providers_per_key = env::var("DHT_MAX_PROVIDERS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(20);

        let max_value_bytes = env::var("DHT_MAX_VALUE_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(65536);

        let enable_compression = env::var("DHT_ENABLE_COMPRESSION")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(true);

        Self {
            db_path,
            max_records,
            max_providers_per_key,
            max_value_bytes,
            enable_compression,
        }
    }
}
