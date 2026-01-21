//! Search query message for network broadcast.

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use crate::modules::query::distributed::error::{DistributedSearchError, SearchResult};

/// Search query broadcast message.
///
/// This message is broadcast via GossipSub to the `/flow/v1/search` topic
/// to request search results from connected peers.
///
/// # Encoding
///
/// Messages are serialized using DAG-CBOR for consistency with the
/// content protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchQueryMessage {
    /// Unique query ID for correlation with responses.
    pub query_id: Uuid,

    /// The search query text.
    pub query: String,

    /// Maximum results requested from each peer.
    pub limit: u32,

    /// Requester's peer ID for direct response (base58 encoded).
    pub requester_peer_id: String,

    /// Timestamp when the query was created (Unix milliseconds).
    /// Used for TTL validation to prevent replay attacks.
    pub timestamp: u64,
}

impl SearchQueryMessage {
    /// Create a new search query message.
    ///
    /// # Arguments
    ///
    /// * `query` - The search query text.
    /// * `limit` - Maximum results requested.
    /// * `requester_peer_id` - The requester's peer ID (base58).
    pub fn new(query: impl Into<String>, limit: u32, requester_peer_id: impl Into<String>) -> Self {
        Self {
            query_id: Uuid::new_v4(),
            query: query.into(),
            limit,
            requester_peer_id: requester_peer_id.into(),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        }
    }

    /// Create with a specific query ID (useful for testing).
    pub fn with_id(
        query_id: Uuid,
        query: impl Into<String>,
        limit: u32,
        requester_peer_id: impl Into<String>,
    ) -> Self {
        Self {
            query_id,
            query: query.into(),
            limit,
            requester_peer_id: requester_peer_id.into(),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        }
    }

    /// Serialize the message to DAG-CBOR bytes.
    pub fn to_cbor(&self) -> SearchResult<Vec<u8>> {
        serde_ipld_dagcbor::to_vec(self)
            .map_err(|e| DistributedSearchError::Serialization(e.to_string()))
    }

    /// Deserialize a message from DAG-CBOR bytes.
    pub fn from_cbor(bytes: &[u8]) -> SearchResult<Self> {
        serde_ipld_dagcbor::from_slice(bytes)
            .map_err(|e| DistributedSearchError::Deserialization(e.to_string()))
    }

    /// Validate the message.
    ///
    /// Checks:
    /// - Query length limits
    /// - Limit bounds
    /// - Timestamp freshness (within 60 seconds)
    pub fn validate(&self) -> SearchResult<()> {
        // Query length check
        if self.query.is_empty() {
            return Err(DistributedSearchError::InvalidQuery(
                "Query cannot be empty".into(),
            ));
        }

        if self.query.len() > 1000 {
            return Err(DistributedSearchError::InvalidQuery(
                "Query too long (max 1000 chars)".into(),
            ));
        }

        // Limit bounds
        if self.limit == 0 {
            return Err(DistributedSearchError::InvalidQuery(
                "Limit must be greater than 0".into(),
            ));
        }

        if self.limit > 100 {
            return Err(DistributedSearchError::InvalidQuery(
                "Limit too high (max 100)".into(),
            ));
        }

        // Timestamp freshness (prevent replay attacks)
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        // Allow 60 seconds of clock skew
        if self.timestamp < now.saturating_sub(60_000) {
            return Err(DistributedSearchError::InvalidQuery(
                "Query expired".into(),
            ));
        }

        Ok(())
    }

    /// Check if the message has expired.
    ///
    /// Messages older than 60 seconds are considered expired.
    pub fn is_expired(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        self.timestamp < now.saturating_sub(60_000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_creation() {
        let msg = SearchQueryMessage::new("test query", 20, "12D3KooWTest");

        assert_eq!(msg.query, "test query");
        assert_eq!(msg.limit, 20);
        assert_eq!(msg.requester_peer_id, "12D3KooWTest");
        assert!(msg.timestamp > 0);
    }

    #[test]
    fn test_cbor_roundtrip() {
        let original = SearchQueryMessage::new("test query", 20, "12D3KooWTest");

        let bytes = original.to_cbor().unwrap();
        let decoded = SearchQueryMessage::from_cbor(&bytes).unwrap();

        assert_eq!(decoded.query_id, original.query_id);
        assert_eq!(decoded.query, original.query);
        assert_eq!(decoded.limit, original.limit);
        assert_eq!(decoded.requester_peer_id, original.requester_peer_id);
    }

    #[test]
    fn test_validation_empty_query() {
        let msg = SearchQueryMessage::new("", 20, "12D3KooWTest");
        assert!(msg.validate().is_err());
    }

    #[test]
    fn test_validation_long_query() {
        let long_query = "a".repeat(1001);
        let msg = SearchQueryMessage::new(long_query, 20, "12D3KooWTest");
        assert!(msg.validate().is_err());
    }

    #[test]
    fn test_validation_zero_limit() {
        let msg = SearchQueryMessage::new("test", 0, "12D3KooWTest");
        assert!(msg.validate().is_err());
    }

    #[test]
    fn test_validation_high_limit() {
        let msg = SearchQueryMessage::new("test", 101, "12D3KooWTest");
        assert!(msg.validate().is_err());
    }

    #[test]
    fn test_validation_valid() {
        let msg = SearchQueryMessage::new("test query", 20, "12D3KooWTest");
        assert!(msg.validate().is_ok());
    }

    #[test]
    fn test_expired_message() {
        let mut msg = SearchQueryMessage::new("test", 20, "12D3KooWTest");
        msg.timestamp = 0; // Very old

        assert!(msg.is_expired());
        assert!(msg.validate().is_err());
    }

    #[test]
    fn test_fresh_message() {
        let msg = SearchQueryMessage::new("test", 20, "12D3KooWTest");
        assert!(!msg.is_expired());
    }
}
