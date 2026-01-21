//! DiscoveryHandler for processing incoming content announcements.
//!
//! This handler subscribes to the ContentAnnouncements GossipSub topic
//! and processes incoming announcements by validating them and storing
//! them in the remote_content table.

use super::{ContentAnnouncement, ValidationError};
use chrono::Utc;
use entity::remote_content;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Maximum announcements per peer per minute.
const RATE_LIMIT_PER_PEER: u32 = 100;

/// Rate limit window duration.
const RATE_LIMIT_WINDOW: Duration = Duration::from_secs(60);

/// Errors that can occur during discovery handling.
#[derive(Error, Debug)]
pub enum DiscoveryError {
    #[error("validation failed: {0}")]
    Validation(#[from] ValidationError),

    #[error("database error: {0}")]
    Database(#[from] sea_orm::DbErr),

    #[error("serialization error: {0}")]
    Serialization(String),

    #[error("rate limit exceeded for peer {peer_id}")]
    RateLimitExceeded { peer_id: String },

    #[error("duplicate CID: {cid}")]
    DuplicateCid { cid: String },
}

/// Tracks rate limits per peer.
#[derive(Debug, Clone)]
struct RateLimitEntry {
    count: u32,
    window_start: Instant,
}

/// Result of processing an announcement.
#[derive(Debug, Clone)]
pub struct DiscoveryResult {
    /// The CID of the discovered content.
    pub cid: String,

    /// Whether this was a new discovery (vs. already known).
    pub is_new: bool,

    /// The database ID if newly inserted.
    pub record_id: Option<i32>,
}

/// Handler for processing content announcements from the network.
pub struct DiscoveryHandler {
    /// Database connection for storing discoveries.
    db: Arc<DatabaseConnection>,

    /// Rate limit tracking per peer.
    rate_limits: Arc<RwLock<HashMap<String, RateLimitEntry>>>,

    /// Maximum announcements per peer per rate limit window.
    rate_limit_max: u32,

    /// Rate limit window duration.
    rate_limit_window: Duration,
}

impl std::fmt::Debug for DiscoveryHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DiscoveryHandler")
            .field("rate_limit_max", &self.rate_limit_max)
            .field("rate_limit_window", &self.rate_limit_window)
            .finish_non_exhaustive()
    }
}

impl DiscoveryHandler {
    /// Create a new discovery handler.
    pub fn new(db: Arc<DatabaseConnection>) -> Self {
        Self {
            db,
            rate_limits: Arc::new(RwLock::new(HashMap::new())),
            rate_limit_max: RATE_LIMIT_PER_PEER,
            rate_limit_window: RATE_LIMIT_WINDOW,
        }
    }

    /// Create a handler with custom rate limits (for testing).
    pub fn with_rate_limit(
        db: Arc<DatabaseConnection>,
        max_per_window: u32,
        window: Duration,
    ) -> Self {
        Self {
            db,
            rate_limits: Arc::new(RwLock::new(HashMap::new())),
            rate_limit_max: max_per_window,
            rate_limit_window: window,
        }
    }

    /// Process a raw CBOR-encoded announcement message.
    ///
    /// # Arguments
    ///
    /// * `data` - The raw CBOR-encoded announcement.
    /// * `sender_peer_id` - The peer ID from which the message was received.
    ///
    /// # Returns
    ///
    /// A `DiscoveryResult` on success, or a `DiscoveryError` on failure.
    pub async fn handle_message(
        &self,
        data: &[u8],
        sender_peer_id: &str,
    ) -> Result<DiscoveryResult, DiscoveryError> {
        // Parse the announcement
        let announcement = ContentAnnouncement::from_cbor(data).map_err(|e| {
            DiscoveryError::Serialization(format!("failed to parse announcement: {}", e))
        })?;

        // Process the parsed announcement
        self.handle_announcement(&announcement, sender_peer_id).await
    }

    /// Process a parsed content announcement.
    pub async fn handle_announcement(
        &self,
        announcement: &ContentAnnouncement,
        sender_peer_id: &str,
    ) -> Result<DiscoveryResult, DiscoveryError> {
        debug!(
            cid = %announcement.cid,
            peer_id = %sender_peer_id,
            "Processing content announcement"
        );

        // Validate the announcement
        announcement.validate(sender_peer_id)?;

        // Check rate limit
        self.check_rate_limit(sender_peer_id).await?;

        // Check if CID already exists
        if self.cid_exists(&announcement.cid).await? {
            debug!(cid = %announcement.cid, "CID already known, skipping");
            return Ok(DiscoveryResult {
                cid: announcement.cid.clone(),
                is_new: false,
                record_id: None,
            });
        }

        // Insert new remote_content record
        let record_id = self.insert_record(announcement).await?;

        info!(
            cid = %announcement.cid,
            peer_id = %sender_peer_id,
            title = %announcement.title,
            size = announcement.size,
            "New content discovered"
        );

        Ok(DiscoveryResult {
            cid: announcement.cid.clone(),
            is_new: true,
            record_id: Some(record_id),
        })
    }

    /// Check if a peer has exceeded the rate limit.
    async fn check_rate_limit(&self, peer_id: &str) -> Result<(), DiscoveryError> {
        let mut rate_limits = self.rate_limits.write().await;
        let now = Instant::now();

        if let Some(entry) = rate_limits.get_mut(peer_id) {
            // Check if window has expired
            if now.duration_since(entry.window_start) >= self.rate_limit_window {
                // Reset the window
                entry.count = 1;
                entry.window_start = now;
            } else if entry.count >= self.rate_limit_max {
                warn!(
                    peer_id = %peer_id,
                    count = entry.count,
                    "Rate limit exceeded"
                );
                return Err(DiscoveryError::RateLimitExceeded {
                    peer_id: peer_id.to_string(),
                });
            } else {
                entry.count += 1;
            }
        } else {
            // First announcement from this peer
            rate_limits.insert(
                peer_id.to_string(),
                RateLimitEntry {
                    count: 1,
                    window_start: now,
                },
            );
        }

        Ok(())
    }

    /// Check if a CID already exists in the database.
    async fn cid_exists(&self, cid: &str) -> Result<bool, DiscoveryError> {
        let exists = remote_content::Entity::find()
            .filter(remote_content::Column::Cid.eq(cid))
            .one(self.db.as_ref())
            .await?
            .is_some();

        Ok(exists)
    }

    /// Insert a new remote_content record.
    async fn insert_record(
        &self,
        announcement: &ContentAnnouncement,
    ) -> Result<i32, DiscoveryError> {
        let record = remote_content::ActiveModel {
            cid: Set(announcement.cid.clone()),
            publisher_peer_id: Set(announcement.publisher_peer_id.clone()),
            publisher_did: Set(Some(announcement.publisher_did.clone())),
            metadata: Set(None), // Could extract from announcement if needed
            content_type: Set(Some(announcement.content_type.clone())),
            title: Set(Some(announcement.title.clone())),
            size: Set(announcement.size as i64),
            block_count: Set(announcement.block_count as i32),
            discovered_at: Set(Utc::now().into()),
            cached: Set(false),
            indexed: Set(false),
            index_error: Set(None),
            qdrant_collection: Set(None),
            embedding_count: Set(None),
            indexed_at: Set(None),
            ..Default::default()
        };

        let result = record.insert(self.db.as_ref()).await?;

        Ok(result.id)
    }

    /// Clean up stale rate limit entries.
    ///
    /// Call this periodically to prevent memory growth.
    pub async fn cleanup_rate_limits(&self) {
        let mut rate_limits = self.rate_limits.write().await;
        let now = Instant::now();

        rate_limits.retain(|_, entry| now.duration_since(entry.window_start) < self.rate_limit_window * 2);
    }

    /// Get current statistics.
    pub async fn stats(&self) -> DiscoveryStats {
        let rate_limits = self.rate_limits.read().await;
        DiscoveryStats {
            tracked_peers: rate_limits.len(),
        }
    }
}

/// Statistics about the discovery handler.
#[derive(Debug, Clone)]
pub struct DiscoveryStats {
    /// Number of peers currently being tracked for rate limiting.
    pub tracked_peers: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn valid_announcement(peer_id: &str) -> ContentAnnouncement {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Generate a unique-ish CID for testing using the timestamp
        let unique_suffix = format!("{:016x}", now);

        ContentAnnouncement {
            version: 1,
            cid: format!("bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55{}", &unique_suffix[..8]),
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
    fn test_cbor_serialization() {
        let announcement = valid_announcement("12D3KooWExample");
        let encoded = announcement.to_cbor().expect("should encode");
        let decoded = ContentAnnouncement::from_cbor(&encoded).expect("should decode");
        assert_eq!(announcement.cid, decoded.cid);
    }
}
