//! Public types for network manager operations

use crate::modules::network::content_transfer::{ContentResponse, ContentTransferError};
use crate::modules::storage::content::ContentId;
use libp2p::PeerId;
use libp2p::kad::QueryId;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::oneshot;

/// Result of bootstrap operation
#[derive(Debug, Clone)]
pub struct BootstrapResult {
    /// Number of bootstrap peers successfully connected
    pub connected_count: usize,
    /// Number of bootstrap peers that failed to connect
    pub failed_count: usize,
    /// Whether Kademlia bootstrap query was initiated
    pub query_initiated: bool,
}

/// Result of a provider announcement
#[derive(Debug, Clone)]
pub struct ProviderAnnounceResult {
    /// The Kademlia query ID
    pub query_id: QueryId,
}

/// Result of provider discovery
#[derive(Debug, Clone)]
pub struct ProviderDiscoveryResult {
    /// The CID that was queried
    pub cid: ContentId,
    /// Discovered provider peer IDs
    pub providers: Vec<PeerId>,
}

/// Connection capacity status
#[derive(Debug, Clone)]
pub struct ConnectionCapacityStatus {
    pub active_connections: usize,
    pub max_connections: usize,
    pub available_slots: usize,
    pub can_accept_inbound: bool,
    pub can_dial_outbound: bool,
}

/// Callback for provider announcement confirmation.
/// Called with (cid, result) when DHT announcement completes.
pub type ProviderConfirmationCallback = Arc<dyn Fn(ContentId, Result<(), String>) + Send + Sync>;

/// Tracks a pending content request.
pub struct PendingContentRequest {
    /// Channel to send result back to caller.
    pub response_tx: oneshot::Sender<Result<ContentResponse, ContentTransferError>>,
    /// The CID being requested.
    pub cid: ContentId,
    /// The peer we're requesting from.
    pub peer_id: PeerId,
    /// When the request was created.
    pub created_at: std::time::Instant,
}

/// Tracks a pending get_providers DHT query.
pub struct PendingProviderQuery {
    /// Channel to send discovered providers back to caller.
    pub response_tx: oneshot::Sender<Result<HashSet<PeerId>, String>>,
    /// The CID being queried.
    pub cid: ContentId,
    /// Accumulated providers discovered so far.
    pub providers: HashSet<PeerId>,
    /// When the query was started.
    pub created_at: std::time::Instant,
}

/// Tracks a pending start_providing query for confirmation callbacks.
pub struct PendingProviderAnnouncement {
    /// The CID being announced
    pub cid: ContentId,
    /// Optional callback to invoke on completion
    pub callback: Option<ProviderConfirmationCallback>,
    /// When the announcement was initiated
    pub created_at: std::time::Instant,
}
