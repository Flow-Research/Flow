use libp2p::PeerId;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use tracing::{debug, warn};

use crate::modules::storage::content::ContentId;

/// Serialization format for messages
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SerializationFormat {
    #[default]
    Json,
    Cbor,
}

/// Errors that can occur during message operations
#[derive(Debug, Error)]
pub enum MessageError {
    #[error("JSON serialization error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("CBOR serialization error: {0}")]
    CborError(String),

    #[error("Invalid message format")]
    InvalidFormat,

    #[error("Message too large: {size} bytes (max: {max})")]
    MessageTooLarge { size: usize, max: usize },

    #[error("Invalid signature")]
    InvalidSignature,

    #[error("Message expired")]
    Expired,

    #[error("Missing required field: {0}")]
    MissingField(String),
}

impl From<serde_cbor::Error> for MessageError {
    fn from(e: serde_cbor::Error) -> Self {
        MessageError::CborError(e.to_string())
    }
}

/// Message payload types for different Flow operations
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "data")]
pub enum MessagePayload {
    /// Update notification
    Update {
        space_key: String,
        update_type: String,
        metadata: serde_json::Value,
    },

    /// Task announcement
    TaskAnnouncement {
        task_id: String,
        task_type: String,
        requirements: serde_json::Value,
        reward: Option<u64>,
    },

    /// Agent status broadcast
    AgentStatus {
        agent_id: String,
        status: String,
        capabilities: Vec<String>,
        load: f64,
    },

    /// Compute offer
    ComputeOffer {
        offer_id: String,
        compute_type: String,
        capacity: u64,
        price_per_unit: u64,
    },

    /// Generic/custom payload
    Custom {
        kind: String,
        data: serde_json::Value,
    },

    /// Ping for testing/health checks
    Ping { nonce: u64 },

    /// Pong response
    Pong { nonce: u64 },

    /// Network lifecycle and peer events
    NetworkEvent {
        /// Event type: "NetworkStarted", "NetworkStopped", "PeerConnected", "PeerDisconnected", etc.
        event_type: String,
        /// Event-specific data
        data: serde_json::Value,
    },

    ContentPublished {
        cid: ContentId,
        name: String,
        size: u64,
        content_type: String,
    },
}

/// A Flow network message with metadata and optional signature
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Message version for future compatibility
    pub version: u8,

    /// Unique message ID (typically hash of content)
    pub id: String,

    /// Timestamp when message was created (Unix millis)
    pub timestamp: u64,

    /// Source peer ID (as base58 string)
    pub source: String,

    /// Topic this message is published to
    pub topic: String,

    /// The actual payload
    pub payload: MessagePayload,

    /// Time-to-live in seconds (0 = no expiry)
    pub ttl: u64,

    /// Serialization format indicator (not serialized itself)
    #[serde(skip)]
    pub format: SerializationFormat,
}

impl Message {
    /// Create a new message with the given payload
    pub fn new(source: PeerId, topic: impl Into<String>, payload: MessagePayload) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let id = Self::generate_id(&source, timestamp, &payload);

        Self {
            version: 1,
            id,
            timestamp,
            source: source.to_base58(),
            topic: topic.into(),
            payload,
            ttl: 300, // 5 minutes default
            format: SerializationFormat::Json,
        }
    }

    /// Create a ping message for testing
    pub fn ping(source: PeerId, topic: impl Into<String>) -> Self {
        let nonce = rand::random::<u64>();
        Self::new(source, topic, MessagePayload::Ping { nonce })
    }

    /// Create a pong response
    pub fn pong(source: PeerId, topic: impl Into<String>, nonce: u64) -> Self {
        Self::new(source, topic, MessagePayload::Pong { nonce })
    }

    /// Set serialization format
    pub fn with_format(mut self, format: SerializationFormat) -> Self {
        self.format = format;
        self
    }

    /// Set TTL
    pub fn with_ttl(mut self, ttl_secs: u64) -> Self {
        self.ttl = ttl_secs;
        self
    }

    /// Generate a unique message ID
    fn generate_id(source: &PeerId, timestamp: u64, payload: &MessagePayload) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(source.to_bytes());
        hasher.update(timestamp.to_le_bytes());
        hasher.update(format!("{:?}", payload).as_bytes());
        let hash = hasher.finalize();
        hex::encode(&hash[..16]) // First 16 bytes as hex
    }

    /// Serialize the message to bytes
    pub fn serialize(&self) -> Result<Vec<u8>, MessageError> {
        match self.format {
            SerializationFormat::Json => {
                let bytes = serde_json::to_vec(self)?;
                debug!(
                    message_id = %self.id,
                    size = bytes.len(),
                    format = "JSON",
                    "Message serialized"
                );
                Ok(bytes)
            }
            SerializationFormat::Cbor => {
                let bytes = serde_cbor::to_vec(self)?;
                debug!(
                    message_id = %self.id,
                    size = bytes.len(),
                    format = "CBOR",
                    "Message serialized"
                );
                Ok(bytes)
            }
        }
    }

    /// Deserialize a message from bytes (auto-detects format)
    pub fn deserialize(data: &[u8]) -> Result<Self, MessageError> {
        // Try JSON first (starts with '{')
        if data.first() == Some(&b'{') {
            let mut msg: Message = serde_json::from_slice(data)?;
            msg.format = SerializationFormat::Json;
            debug!(
                message_id = %msg.id,
                format = "JSON",
                "Message deserialized"
            );
            return Ok(msg);
        }

        // Try CBOR
        match serde_cbor::from_slice::<Message>(data) {
            Ok(mut msg) => {
                msg.format = SerializationFormat::Cbor;
                debug!(
                    message_id = %msg.id,
                    format = "CBOR",
                    "Message deserialized"
                );
                Ok(msg)
            }
            Err(e) => {
                warn!(error = %e, "Failed to deserialize message");
                Err(MessageError::InvalidFormat)
            }
        }
    }

    /// Check if the message has expired
    pub fn is_expired(&self) -> bool {
        if self.ttl == 0 {
            return false; // No expiry
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let expiry = self.timestamp + (self.ttl * 1000);
        now > expiry
    }

    /// Get the source PeerId
    pub fn source_peer_id(&self) -> Option<PeerId> {
        self.source.parse().ok()
    }

    /// Validate message structure
    pub fn validate(&self, max_size: usize) -> Result<(), MessageError> {
        // Check version
        if self.version != 1 {
            warn!(version = self.version, "Unknown message version");
        }

        // Check expiry
        if self.is_expired() {
            return Err(MessageError::Expired);
        }

        // Estimate size
        let size = self.serialize()?.len();
        if size > max_size {
            return Err(MessageError::MessageTooLarge {
                size,
                max: max_size,
            });
        }

        Ok(())
    }
}

// Add hex encoding dependency for message ID
mod hex {
    const HEX_CHARS: &[u8; 16] = b"0123456789abcdef";

    pub fn encode(data: &[u8]) -> String {
        let mut s = String::with_capacity(data.len() * 2);
        for byte in data {
            s.push(HEX_CHARS[(byte >> 4) as usize] as char);
            s.push(HEX_CHARS[(byte & 0xf) as usize] as char);
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libp2p::identity::Keypair;

    fn test_peer_id() -> PeerId {
        Keypair::generate_ed25519().public().to_peer_id()
    }

    #[test]
    fn test_message_creation() {
        let peer_id = test_peer_id();
        let msg = Message::new(peer_id, "test/topic", MessagePayload::Ping { nonce: 12345 });

        assert_eq!(msg.version, 1);
        assert_eq!(msg.topic, "test/topic");
        assert!(!msg.id.is_empty());
        assert!(msg.timestamp > 0);
    }

    #[test]
    fn test_json_serialization_roundtrip() {
        let peer_id = test_peer_id();
        let original = Message::new(
            peer_id,
            "test/topic",
            MessagePayload::Update {
                space_key: "space-123".to_string(),
                update_type: "add".to_string(),
                metadata: serde_json::json!({"files": 10}),
            },
        );

        let bytes = original.serialize().unwrap();
        let deserialized = Message::deserialize(&bytes).unwrap();

        assert_eq!(original.id, deserialized.id);
        assert_eq!(original.topic, deserialized.topic);
        assert_eq!(deserialized.format, SerializationFormat::Json);
    }

    #[test]
    fn test_cbor_serialization_roundtrip() {
        let peer_id = test_peer_id();
        let original = Message::new(
            peer_id,
            "test/topic",
            MessagePayload::TaskAnnouncement {
                task_id: "task-456".to_string(),
                task_type: "compute".to_string(),
                requirements: serde_json::json!({"cpu": 4, "memory": "8GB"}),
                reward: Some(100),
            },
        )
        .with_format(SerializationFormat::Cbor);

        let bytes = original.serialize().unwrap();
        let deserialized = Message::deserialize(&bytes).unwrap();

        assert_eq!(original.id, deserialized.id);
        assert_eq!(deserialized.format, SerializationFormat::Cbor);
    }

    #[test]
    fn test_message_expiry() {
        let peer_id = test_peer_id();
        let mut msg = Message::new(peer_id, "test", MessagePayload::Ping { nonce: 1 });

        // Not expired with default TTL
        assert!(!msg.is_expired());

        // Force expiry by setting old timestamp
        msg.timestamp = 0;
        msg.ttl = 1;
        assert!(msg.is_expired());
    }

    #[test]
    fn test_no_expiry() {
        let peer_id = test_peer_id();
        let mut msg = Message::new(peer_id, "test", MessagePayload::Ping { nonce: 1 });
        msg.ttl = 0; // No expiry
        msg.timestamp = 0; // Very old

        assert!(!msg.is_expired());
    }

    #[test]
    fn test_ping_pong() {
        let peer_id = test_peer_id();
        let ping = Message::ping(peer_id, "test");

        if let MessagePayload::Ping { nonce } = ping.payload {
            let pong = Message::pong(peer_id, "test", nonce);
            if let MessagePayload::Pong { nonce: pong_nonce } = pong.payload {
                assert_eq!(nonce, pong_nonce);
            } else {
                panic!("Expected Pong payload");
            }
        } else {
            panic!("Expected Ping payload");
        }
    }

    #[test]
    fn test_all_payload_types() {
        let peer_id = test_peer_id();

        let payloads = vec![
            MessagePayload::Update {
                space_key: "s1".to_string(),
                update_type: "update".to_string(),
                metadata: serde_json::json!({}),
            },
            MessagePayload::TaskAnnouncement {
                task_id: "t1".to_string(),
                task_type: "index".to_string(),
                requirements: serde_json::json!({}),
                reward: None,
            },
            MessagePayload::AgentStatus {
                agent_id: "a1".to_string(),
                status: "online".to_string(),
                capabilities: vec!["compute".to_string()],
                load: 0.5,
            },
            MessagePayload::ComputeOffer {
                offer_id: "o1".to_string(),
                compute_type: "gpu".to_string(),
                capacity: 1000,
                price_per_unit: 10,
            },
            MessagePayload::Custom {
                kind: "custom".to_string(),
                data: serde_json::json!({"key": "value"}),
            },
        ];

        for payload in payloads {
            let msg = Message::new(peer_id, "test", payload);
            let bytes = msg.serialize().unwrap();
            let decoded = Message::deserialize(&bytes).unwrap();
            assert_eq!(msg.id, decoded.id);
        }
    }
}
