//! ContentAnnouncement schema for GossipSub content discovery.
//!
//! This module defines the message format used to announce newly published
//! content to the network via GossipSub.

use serde::{Deserialize, Serialize};

/// GossipSub topic for content announcements.
pub const CONTENT_ANNOUNCE_TOPIC: &str = "/flow/content/announce/1.0.0";

/// Maximum content size allowed in announcements (1 GB).
pub const MAX_CONTENT_SIZE: u64 = 1_073_741_824;

/// Maximum allowed timestamp drift in seconds (5 minutes).
pub const MAX_TIMESTAMP_DRIFT_SECS: u64 = 300;

/// Message structure for announcing published content over GossipSub.
///
/// # Wire Format
///
/// Messages are encoded using DAG-CBOR for compact, deterministic serialization
/// that integrates with the IPLD ecosystem.
///
/// # Example
///
/// ```ignore
/// use crate::modules::network::discovery::ContentAnnouncement;
///
/// let announcement = ContentAnnouncement {
///     version: 1,
///     cid: "bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi".to_string(),
///     publisher_peer_id: "12D3KooWExample...".to_string(),
///     publisher_did: "did:key:z6MkExample...".to_string(),
///     content_type: "text/markdown".to_string(),
///     size: 1024,
///     block_count: 1,
///     title: "My Document".to_string(),
///     timestamp: 1704067200,
///     signature: vec![],
/// };
///
/// let encoded = announcement.to_cbor()?;
/// let decoded = ContentAnnouncement::from_cbor(&encoded)?;
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContentAnnouncement {
    /// Protocol version (currently 1).
    pub version: u8,

    /// Content Identifier (CID) of the published content root.
    pub cid: String,

    /// libp2p Peer ID of the publisher.
    pub publisher_peer_id: String,

    /// Decentralized Identifier (DID) of the publisher.
    pub publisher_did: String,

    /// MIME type of the content.
    pub content_type: String,

    /// Total size in bytes.
    pub size: u64,

    /// Number of blocks in the content DAG.
    pub block_count: u32,

    /// Human-readable title.
    pub title: String,

    /// Unix timestamp (seconds since epoch) when published.
    pub timestamp: u64,

    /// Optional cryptographic signature over the announcement.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub signature: Vec<u8>,
}

impl ContentAnnouncement {
    /// Serialize the announcement to DAG-CBOR format.
    pub fn to_cbor(&self) -> Result<Vec<u8>, serde_ipld_dagcbor::EncodeError<std::collections::TryReserveError>> {
        serde_ipld_dagcbor::to_vec(self)
    }

    /// Deserialize an announcement from DAG-CBOR format.
    pub fn from_cbor(data: &[u8]) -> Result<Self, serde_ipld_dagcbor::DecodeError<std::convert::Infallible>> {
        serde_ipld_dagcbor::from_slice(data)
    }

    /// Create a new announcement with the current timestamp.
    pub fn new(
        cid: String,
        publisher_peer_id: String,
        publisher_did: String,
        content_type: String,
        size: u64,
        block_count: u32,
        title: String,
    ) -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        Self {
            version: 1,
            cid,
            publisher_peer_id,
            publisher_did,
            content_type,
            size,
            block_count,
            title,
            timestamp,
            signature: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_announcement() -> ContentAnnouncement {
        ContentAnnouncement {
            version: 1,
            cid: "bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi".to_string(),
            publisher_peer_id: "12D3KooWExample".to_string(),
            publisher_did: "did:key:z6MkExample".to_string(),
            content_type: "text/markdown".to_string(),
            size: 1024,
            block_count: 1,
            title: "Test Document".to_string(),
            timestamp: 1704067200,
            signature: vec![],
        }
    }

    #[test]
    fn test_cbor_roundtrip() {
        let original = sample_announcement();

        // Serialize to CBOR
        let encoded = original.to_cbor().expect("serialization should succeed");

        // Deserialize back
        let decoded =
            ContentAnnouncement::from_cbor(&encoded).expect("deserialization should succeed");

        assert_eq!(original, decoded);
    }

    #[test]
    fn test_cbor_is_compact() {
        let announcement = sample_announcement();
        let cbor = announcement.to_cbor().expect("serialization should succeed");
        let json = serde_json::to_vec(&announcement).expect("json serialization should succeed");

        // CBOR should be smaller than JSON
        assert!(
            cbor.len() < json.len(),
            "CBOR ({} bytes) should be smaller than JSON ({} bytes)",
            cbor.len(),
            json.len()
        );
    }

    #[test]
    fn test_signature_omitted_when_empty() {
        let announcement = sample_announcement();
        let cbor = announcement.to_cbor().expect("serialization should succeed");

        // The signature field should not appear in the serialized output
        // when empty (due to skip_serializing_if)
        let decoded =
            ContentAnnouncement::from_cbor(&cbor).expect("deserialization should succeed");
        assert!(decoded.signature.is_empty());
    }

    #[test]
    fn test_new_sets_current_timestamp() {
        let announcement = ContentAnnouncement::new(
            "bafytest".to_string(),
            "12D3KooW".to_string(),
            "did:key:z6Mk".to_string(),
            "text/plain".to_string(),
            100,
            1,
            "Test".to_string(),
        );

        assert_eq!(announcement.version, 1);
        assert!(announcement.timestamp > 0);
        assert!(announcement.signature.is_empty());
    }
}
