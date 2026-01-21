//! Validation logic for ContentAnnouncement messages.
//!
//! Ensures incoming announcements from the network meet protocol requirements
//! before being processed.

use super::{ContentAnnouncement, MAX_CONTENT_SIZE, MAX_TIMESTAMP_DRIFT_SECS};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

/// Errors that can occur during announcement validation.
#[derive(Error, Debug, Clone, PartialEq)]
pub enum ValidationError {
    #[error("unsupported protocol version: {0}")]
    UnsupportedVersion(u8),

    #[error("invalid CID format")]
    InvalidCid,

    #[error("publisher peer ID mismatch: announcement claims {claimed}, but received from {actual}")]
    PeerIdMismatch { claimed: String, actual: String },

    #[error("invalid content size: {0} bytes (max: {MAX_CONTENT_SIZE})")]
    InvalidSize(u64),

    #[error("timestamp drift too large: {0} seconds (max: {MAX_TIMESTAMP_DRIFT_SECS})")]
    TimestampDrift(i64),

    #[error("empty required field: {0}")]
    EmptyField(&'static str),
}

/// Policy configuration for announcement validation.
#[derive(Debug, Clone)]
pub struct ValidationPolicy {
    /// Maximum allowed content size in bytes.
    pub max_size: u64,

    /// Maximum allowed timestamp drift in seconds.
    pub max_timestamp_drift_secs: u64,

    /// Whether to verify that publisher_peer_id matches the sender.
    pub verify_peer_id: bool,
}

impl Default for ValidationPolicy {
    fn default() -> Self {
        Self {
            max_size: MAX_CONTENT_SIZE,
            max_timestamp_drift_secs: MAX_TIMESTAMP_DRIFT_SECS,
            verify_peer_id: true,
        }
    }
}

impl ContentAnnouncement {
    /// Validate the announcement with default policy.
    ///
    /// # Arguments
    ///
    /// * `sender_peer_id` - The actual peer ID from which the message was received.
    ///
    /// # Returns
    ///
    /// `Ok(())` if validation passes, or a `ValidationError` describing the failure.
    pub fn validate(&self, sender_peer_id: &str) -> Result<(), ValidationError> {
        self.validate_with_policy(sender_peer_id, &ValidationPolicy::default())
    }

    /// Validate the announcement with custom policy.
    pub fn validate_with_policy(
        &self,
        sender_peer_id: &str,
        policy: &ValidationPolicy,
    ) -> Result<(), ValidationError> {
        // Version check
        if self.version != 1 {
            return Err(ValidationError::UnsupportedVersion(self.version));
        }

        // CID format validation (basic check - must be non-empty and start with standard prefix)
        if self.cid.is_empty() {
            return Err(ValidationError::EmptyField("cid"));
        }
        if !self.cid.starts_with("bafy") && !self.cid.starts_with("Qm") {
            return Err(ValidationError::InvalidCid);
        }

        // Sender verification
        if policy.verify_peer_id && self.publisher_peer_id != sender_peer_id {
            return Err(ValidationError::PeerIdMismatch {
                claimed: self.publisher_peer_id.clone(),
                actual: sender_peer_id.to_string(),
            });
        }

        // Size limits
        if self.size == 0 || self.size > policy.max_size {
            return Err(ValidationError::InvalidSize(self.size));
        }

        // Timestamp freshness
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let drift = (now as i64 - self.timestamp as i64).abs();
        if drift > policy.max_timestamp_drift_secs as i64 {
            return Err(ValidationError::TimestampDrift(drift));
        }

        // Required field checks
        if self.publisher_peer_id.is_empty() {
            return Err(ValidationError::EmptyField("publisher_peer_id"));
        }
        if self.publisher_did.is_empty() {
            return Err(ValidationError::EmptyField("publisher_did"));
        }
        if self.title.is_empty() {
            return Err(ValidationError::EmptyField("title"));
        }

        Ok(())
    }

    /// Check if this content is indexable based on content type and size.
    ///
    /// Returns `true` if the content should be fetched and indexed,
    /// `false` if it should be skipped (e.g., binary files, too large).
    pub fn is_indexable(&self) -> bool {
        // Skip if too large for indexing (100MB limit for indexing)
        const MAX_INDEXABLE_SIZE: u64 = 100 * 1024 * 1024;
        if self.size > MAX_INDEXABLE_SIZE {
            return false;
        }

        // Indexable content types
        let indexable_types = [
            "text/",
            "application/json",
            "application/markdown",
            "application/pdf",
        ];

        indexable_types
            .iter()
            .any(|t| self.content_type.starts_with(t))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_announcement(peer_id: &str) -> ContentAnnouncement {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        ContentAnnouncement {
            version: 1,
            cid: "bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi".to_string(),
            publisher_peer_id: peer_id.to_string(),
            publisher_did: "did:key:z6MkExample".to_string(),
            content_type: "text/markdown".to_string(),
            size: 1024,
            block_count: 1,
            title: "Test Document".to_string(),
            timestamp: now,
            signature: vec![],
        }
    }

    #[test]
    fn test_valid_announcement_passes() {
        let peer_id = "12D3KooWExample";
        let announcement = valid_announcement(peer_id);
        assert!(announcement.validate(peer_id).is_ok());
    }

    #[test]
    fn test_unsupported_version() {
        let peer_id = "12D3KooWExample";
        let mut announcement = valid_announcement(peer_id);
        announcement.version = 2;

        let result = announcement.validate(peer_id);
        assert!(matches!(result, Err(ValidationError::UnsupportedVersion(2))));
    }

    #[test]
    fn test_invalid_cid() {
        let peer_id = "12D3KooWExample";
        let mut announcement = valid_announcement(peer_id);
        announcement.cid = "invalid_cid".to_string();

        let result = announcement.validate(peer_id);
        assert!(matches!(result, Err(ValidationError::InvalidCid)));
    }

    #[test]
    fn test_peer_id_mismatch() {
        let announcement = valid_announcement("12D3KooWClaimed");

        let result = announcement.validate("12D3KooWActual");
        assert!(matches!(result, Err(ValidationError::PeerIdMismatch { .. })));
    }

    #[test]
    fn test_size_zero() {
        let peer_id = "12D3KooWExample";
        let mut announcement = valid_announcement(peer_id);
        announcement.size = 0;

        let result = announcement.validate(peer_id);
        assert!(matches!(result, Err(ValidationError::InvalidSize(0))));
    }

    #[test]
    fn test_size_too_large() {
        let peer_id = "12D3KooWExample";
        let mut announcement = valid_announcement(peer_id);
        announcement.size = MAX_CONTENT_SIZE + 1;

        let result = announcement.validate(peer_id);
        assert!(matches!(result, Err(ValidationError::InvalidSize(_))));
    }

    #[test]
    fn test_timestamp_too_old() {
        let peer_id = "12D3KooWExample";
        let mut announcement = valid_announcement(peer_id);
        announcement.timestamp = 0; // Very old timestamp

        let result = announcement.validate(peer_id);
        assert!(matches!(result, Err(ValidationError::TimestampDrift(_))));
    }

    #[test]
    fn test_is_indexable_text() {
        let mut announcement = valid_announcement("peer");
        announcement.content_type = "text/markdown".to_string();
        announcement.size = 1024;
        assert!(announcement.is_indexable());
    }

    #[test]
    fn test_is_indexable_json() {
        let mut announcement = valid_announcement("peer");
        announcement.content_type = "application/json".to_string();
        assert!(announcement.is_indexable());
    }

    #[test]
    fn test_not_indexable_binary() {
        let mut announcement = valid_announcement("peer");
        announcement.content_type = "application/octet-stream".to_string();
        assert!(!announcement.is_indexable());
    }

    #[test]
    fn test_not_indexable_too_large() {
        let mut announcement = valid_announcement("peer");
        announcement.content_type = "text/plain".to_string();
        announcement.size = 200 * 1024 * 1024; // 200MB
        assert!(!announcement.is_indexable());
    }

    #[test]
    fn test_custom_policy() {
        let peer_id = "12D3KooWClaimed";
        let announcement = valid_announcement(peer_id);

        // Policy that doesn't verify peer ID
        let policy = ValidationPolicy {
            verify_peer_id: false,
            ..Default::default()
        };

        // Should pass even with different sender
        let result = announcement.validate_with_policy("12D3KooWDifferent", &policy);
        assert!(result.is_ok());
    }
}
