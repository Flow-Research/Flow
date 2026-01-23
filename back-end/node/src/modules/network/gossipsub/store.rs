use super::message::Message;
use rocksdb::{DB, Direction, IteratorMode, Options, WriteBatch};
use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use tracing::{debug, info, warn};

/// Configuration for message store
#[derive(Debug, Clone)]
pub struct MessageStoreConfig {
    /// Maximum number of messages to retain per topic
    pub max_messages_per_topic: usize,

    /// How often to run cleanup (in seconds)
    pub cleanup_interval_secs: u64,

    /// Whether to enable message persistence
    pub enabled: bool,

    /// Maximum total messages across all topics
    pub max_total_messages: usize,
}

impl Default for MessageStoreConfig {
    fn default() -> Self {
        Self {
            max_messages_per_topic: 10_000,
            cleanup_interval_secs: 60,
            enabled: true,
            max_total_messages: 100_000,
        }
    }
}

impl MessageStoreConfig {
    pub fn from_env() -> Self {
        use std::env;

        Self {
            enabled: env::var("MESSAGE_STORE_ENABLED")
                .map(|v| v.to_lowercase() == "true")
                .unwrap_or(true),
            max_messages_per_topic: env::var("MESSAGE_STORE_MAX_PER_TOPIC")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(10_000),
            cleanup_interval_secs: env::var("MESSAGE_STORE_CLEANUP_INTERVAL")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(60),
            max_total_messages: env::var("MESSAGE_STORE_MAX_TOTAL")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(100_000),
        }
    }
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("RocksDB error: {0}")]
    RocksDb(#[from] rocksdb::Error),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Message not found: {0}")]
    NotFound(String),

    #[error("Store is disabled")]
    Disabled,

    #[error("Column family not found")]
    ColumnFamilyNotFound,
}

/// RocksDB-backed message store with column families:
/// - "messages" → message_id -> serialized Message
/// - "by_topic" → topic:timestamp:message_id -> message_id
/// - "by_time" → timestamp:message_id -> message_id (for cleanup)
pub struct MessageStore {
    db: Arc<DB>,
    config: MessageStoreConfig,
}

impl MessageStore {
    /// Column family names
    const CF_MESSAGES: &'static str = "gossip_messages";
    const CF_BY_TOPIC: &'static str = "gossip_by_topic";
    const CF_BY_TIME: &'static str = "gossip_by_time";

    /// Create or open message store
    pub fn new(path: &Path, config: MessageStoreConfig) -> Result<Self, StoreError> {
        if !config.enabled {
            info!("Message store disabled");
        }

        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);
        opts.set_max_open_files(256);
        opts.set_compression_type(rocksdb::DBCompressionType::Lz4);

        let cf_opts = Options::default();
        let cfs = vec![
            rocksdb::ColumnFamilyDescriptor::new(Self::CF_MESSAGES, cf_opts.clone()),
            rocksdb::ColumnFamilyDescriptor::new(Self::CF_BY_TOPIC, cf_opts.clone()),
            rocksdb::ColumnFamilyDescriptor::new(Self::CF_BY_TIME, cf_opts),
        ];

        let db_path = path.join("gossip_messages");
        let db = DB::open_cf_descriptors(&opts, &db_path, cfs)?;

        info!(path = ?db_path, "Message store opened");

        Ok(Self {
            db: Arc::new(db),
            config,
        })
    }

    /// Store a message
    pub fn store(&self, message: &Message) -> Result<(), StoreError> {
        if !self.config.enabled {
            return Ok(());
        }

        let cf_messages = self
            .db
            .cf_handle(Self::CF_MESSAGES)
            .ok_or_else(|| StoreError::ColumnFamilyNotFound)?;
        let cf_by_topic = self
            .db
            .cf_handle(Self::CF_BY_TOPIC)
            .ok_or_else(|| StoreError::ColumnFamilyNotFound)?;
        let cf_by_time = self
            .db
            .cf_handle(Self::CF_BY_TIME)
            .ok_or_else(|| StoreError::ColumnFamilyNotFound)?;

        let msg_bytes = message
            .serialize()
            .map_err(|e| StoreError::Serialization(e.to_string()))?;

        let mut batch = WriteBatch::default();

        // Primary storage: message_id -> message
        batch.put_cf(&cf_messages, message.id.as_bytes(), &msg_bytes);

        // Topic index: topic:timestamp:message_id -> message_id
        let topic_key = format!("{}:{}:{}", message.topic, message.timestamp, message.id);
        batch.put_cf(&cf_by_topic, topic_key.as_bytes(), message.id.as_bytes());

        // Time index: timestamp:message_id -> message_id (for cleanup)
        let time_key = format!("{}:{}", message.timestamp, message.id);
        batch.put_cf(&cf_by_time, time_key.as_bytes(), message.id.as_bytes());

        self.db.write(batch)?;

        debug!(
            message_id = %message.id,
            topic = %message.topic,
            "Message stored"
        );

        Ok(())
    }

    /// Get a message by ID
    pub fn get(&self, message_id: &str) -> Result<Option<Message>, StoreError> {
        if !self.config.enabled {
            return Ok(None);
        }

        let cf = self
            .db
            .cf_handle(Self::CF_MESSAGES)
            .ok_or_else(|| StoreError::ColumnFamilyNotFound)?;

        match self.db.get_cf(&cf, message_id.as_bytes())? {
            Some(bytes) => {
                let msg = Message::deserialize(&bytes)
                    .map_err(|e| StoreError::Serialization(e.to_string()))?;
                Ok(Some(msg))
            }
            None => Ok(None),
        }
    }

    /// Get messages for a topic, optionally since a timestamp
    pub fn get_by_topic(
        &self,
        topic: &str,
        since_timestamp: Option<u64>,
        limit: usize,
    ) -> Result<Vec<Message>, StoreError> {
        if !self.config.enabled {
            return Ok(Vec::new());
        }

        let cf_by_topic = self
            .db
            .cf_handle(Self::CF_BY_TOPIC)
            .ok_or_else(|| StoreError::ColumnFamilyNotFound)?;
        let cf_messages = self
            .db
            .cf_handle(Self::CF_MESSAGES)
            .ok_or_else(|| StoreError::ColumnFamilyNotFound)?;

        let prefix = match since_timestamp {
            Some(ts) => format!("{}:{}", topic, ts),
            None => format!("{}:", topic),
        };

        let mut messages = Vec::new();
        let iter = self.db.iterator_cf(
            &cf_by_topic,
            IteratorMode::From(prefix.as_bytes(), Direction::Forward),
        );

        for item in iter {
            if messages.len() >= limit {
                break;
            }

            let (key, msg_id) = item?;
            let key_str = String::from_utf8_lossy(&key);

            // Check if still within topic prefix
            if !key_str.starts_with(&format!("{}:", topic)) {
                break;
            }

            // Fetch actual message
            if let Some(msg_bytes) = self.db.get_cf(&cf_messages, &msg_id)? {
                match Message::deserialize(&msg_bytes) {
                    Ok(msg) => {
                        if !msg.is_expired() {
                            messages.push(msg);
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, "Failed to deserialize stored message");
                    }
                }
            }
        }

        debug!(
            topic = %topic,
            count = messages.len(),
            "Retrieved messages from store"
        );

        Ok(messages)
    }

    /// Cleanup expired messages and enforce limits
    pub fn cleanup(&self) -> Result<usize, StoreError> {
        if !self.config.enabled {
            return Ok(0);
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let cf_messages = self
            .db
            .cf_handle(Self::CF_MESSAGES)
            .ok_or_else(|| StoreError::ColumnFamilyNotFound)?;
        let cf_by_topic = self
            .db
            .cf_handle(Self::CF_BY_TOPIC)
            .ok_or_else(|| StoreError::ColumnFamilyNotFound)?;
        let cf_by_time = self
            .db
            .cf_handle(Self::CF_BY_TIME)
            .ok_or_else(|| StoreError::ColumnFamilyNotFound)?;

        let mut deleted = 0;
        let mut batch = WriteBatch::default();
        let mut to_delete = Vec::new();

        // Scan time index for expired messages
        let iter = self.db.iterator_cf(&cf_by_time, IteratorMode::Start);

        for item in iter {
            let (key, msg_id) = item?;
            let key_str = String::from_utf8_lossy(&key);

            // Parse timestamp from key
            if let Some(ts_str) = key_str.split(':').next() {
                if let Ok(ts) = ts_str.parse::<u64>() {
                    // Check if message with default TTL would be expired
                    // (5 min default TTL = 300 * 1000 ms)
                    if ts + 300_000 < now {
                        to_delete.push((key.to_vec(), msg_id.to_vec()));
                    }
                }
            }

            // Limit batch size
            if to_delete.len() >= 1000 {
                break;
            }
        }

        // Delete expired entries
        for (time_key, msg_id) in to_delete {
            // Get message to find topic for index cleanup
            if let Some(msg_bytes) = self.db.get_cf(&cf_messages, &msg_id)? {
                if let Ok(msg) = Message::deserialize(&msg_bytes) {
                    let topic_key = format!("{}:{}:{}", msg.topic, msg.timestamp, msg.id);
                    batch.delete_cf(&cf_by_topic, topic_key.as_bytes());
                }
            }

            batch.delete_cf(&cf_messages, &msg_id);
            batch.delete_cf(&cf_by_time, &time_key);
            deleted += 1;
        }

        if deleted > 0 {
            self.db.write(batch)?;
            info!(deleted = deleted, "Cleaned up expired messages");
        }

        Ok(deleted)
    }

    /// Get message count
    pub fn count(&self) -> Result<usize, StoreError> {
        if !self.config.enabled {
            return Ok(0);
        }

        let cf = self
            .db
            .cf_handle(Self::CF_MESSAGES)
            .ok_or_else(|| StoreError::ColumnFamilyNotFound)?;

        let mut count = 0;
        let iter = self.db.iterator_cf(&cf, IteratorMode::Start);
        for _ in iter {
            count += 1;
        }

        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::network::gossipsub::{Message, MessagePayload};
    use libp2p::identity::Keypair;
    use tempfile::TempDir;

    fn test_peer_id() -> libp2p::PeerId {
        Keypair::generate_ed25519().public().to_peer_id()
    }

    #[test]
    fn test_store_and_retrieve() {
        let temp_dir = TempDir::new().unwrap();
        let config = MessageStoreConfig::default();
        let store = MessageStore::new(temp_dir.path(), config).unwrap();

        let msg = Message::new(
            test_peer_id(),
            "/flow/v1/test",
            MessagePayload::Ping { nonce: 12345 },
        );

        store.store(&msg).unwrap();

        let retrieved = store.get(&msg.id).unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().id, msg.id);
    }

    #[test]
    fn test_get_by_topic() {
        let temp_dir = TempDir::new().unwrap();
        let config = MessageStoreConfig::default();
        let store = MessageStore::new(temp_dir.path(), config).unwrap();

        let peer_id = test_peer_id();
        let topic = "/flow/v1/test";

        // Store multiple messages
        for i in 0..5 {
            let msg = Message::new(peer_id, topic, MessagePayload::Ping { nonce: i });
            store.store(&msg).unwrap();
        }

        let messages = store.get_by_topic(topic, None, 10).unwrap();
        assert_eq!(messages.len(), 5);
    }

    #[test]
    fn test_disabled_store() {
        let temp_dir = TempDir::new().unwrap();
        let config = MessageStoreConfig {
            enabled: false,
            ..Default::default()
        };
        let store = MessageStore::new(temp_dir.path(), config).unwrap();

        let msg = Message::new(test_peer_id(), "/test", MessagePayload::Ping { nonce: 1 });

        // Should not error, just no-op
        store.store(&msg).unwrap();
        assert!(store.get(&msg.id).unwrap().is_none());
    }
}
