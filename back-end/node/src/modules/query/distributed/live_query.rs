//! Live network query engine.
//!
//! Broadcasts search queries to connected peers via GossipSub and
//! collects responses within the timeout window.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{mpsc, RwLock};
use tracing::{debug, info, warn};
use uuid::Uuid;

use super::config::SearchConfig;
use super::error::SearchResult;
use super::messages::{NetworkSearchResult, SearchQueryMessage, SearchResponseMessage};
use super::types::SourcedResult;

/// Pending query tracker.
#[derive(Debug)]
struct PendingQuery {
    /// Channel to send responses.
    sender: mpsc::Sender<SearchResponseMessage>,
    /// When the query was started.
    started_at: Instant,
}

/// Engine for broadcasting search queries to the network and collecting responses.
///
/// # Protocol
///
/// 1. Generate unique query ID
/// 2. Register pending query with response channel
/// 3. Broadcast `SearchQueryMessage` to GossipSub topic
/// 4. Collect `SearchResponseMessage` until timeout or all peers respond
/// 5. Convert responses to `SourcedResult` format
pub struct LiveQueryEngine {
    /// Configuration for search.
    config: SearchConfig,

    /// Local peer ID (base58).
    local_peer_id: String,

    /// Pending queries awaiting responses.
    pending_queries: Arc<RwLock<HashMap<Uuid, PendingQuery>>>,

    /// Channel for outgoing messages (to be sent via GossipSub).
    /// In a real implementation, this would be connected to the NetworkManager.
    outgoing_tx: Option<mpsc::Sender<SearchQueryMessage>>,
}

impl LiveQueryEngine {
    /// Create a new live query engine.
    ///
    /// # Arguments
    ///
    /// * `config` - Search configuration.
    /// * `local_peer_id` - This node's peer ID (base58).
    pub fn new(config: SearchConfig, local_peer_id: impl Into<String>) -> Self {
        Self {
            config,
            local_peer_id: local_peer_id.into(),
            pending_queries: Arc::new(RwLock::new(HashMap::new())),
            outgoing_tx: None,
        }
    }

    /// Set the outgoing message channel.
    ///
    /// Messages sent through this channel should be broadcast via GossipSub.
    pub fn with_outgoing_channel(mut self, tx: mpsc::Sender<SearchQueryMessage>) -> Self {
        self.outgoing_tx = Some(tx);
        self
    }

    /// Get a handle for delivering responses to pending queries.
    pub fn response_handle(&self) -> LiveQueryResponseHandle {
        LiveQueryResponseHandle {
            pending_queries: self.pending_queries.clone(),
        }
    }

    /// Broadcast a search query and collect responses.
    ///
    /// # Arguments
    ///
    /// * `query` - The search query text.
    /// * `limit` - Maximum results per peer.
    ///
    /// # Returns
    ///
    /// Tuple of (results, peers_responded).
    pub async fn query(
        &self,
        query: &str,
        limit: u32,
    ) -> SearchResult<(Vec<SourcedResult>, u32)> {
        let query_id = Uuid::new_v4();
        let peer_count = self.config.peer_count;
        let timeout = Duration::from_millis(self.config.network_timeout_ms);

        info!(
            query_id = %query_id,
            query = %query,
            peer_count = peer_count,
            timeout_ms = self.config.network_timeout_ms,
            "Starting live network query"
        );

        // Create response channel
        let (tx, mut rx) = mpsc::channel(peer_count as usize);

        // Build query message with our query_id
        let message = SearchQueryMessage::with_id(query_id, query, limit, &self.local_peer_id);

        // Register pending query
        {
            let mut pending = self.pending_queries.write().await;
            pending.insert(
                query_id,
                PendingQuery {
                    sender: tx,
                    started_at: Instant::now(),
                },
            );
        }

        // Broadcast query
        if let Some(ref outgoing_tx) = self.outgoing_tx {
            if outgoing_tx.send(message.clone()).await.is_err() {
                warn!(query_id = %query_id, "Failed to send query to outgoing channel");
            }
        } else {
            debug!(query_id = %query_id, "No outgoing channel configured - query not broadcast");
        }

        // Collect responses with timeout
        let mut results = Vec::new();
        let mut peers_responded = 0u32;

        let deadline = Instant::now() + timeout;

        while Instant::now() < deadline && peers_responded < peer_count {
            let remaining = deadline.saturating_duration_since(Instant::now());

            match tokio::time::timeout(remaining, rx.recv()).await {
                Ok(Some(response)) => {
                    peers_responded += 1;
                    debug!(
                        query_id = %query_id,
                        peer = %response.responder_peer_id,
                        results = response.results.len(),
                        "Received response from peer"
                    );

                    // Convert to SourcedResult
                    for network_result in response.results {
                        results.push(self.convert_result(network_result, &response.responder_peer_id));
                    }
                }
                Ok(None) => {
                    // Channel closed
                    break;
                }
                Err(_) => {
                    // Timeout
                    debug!(query_id = %query_id, "Query timeout reached");
                    break;
                }
            }
        }

        // Cleanup pending query
        {
            let mut pending = self.pending_queries.write().await;
            pending.remove(&query_id);
        }

        info!(
            query_id = %query_id,
            peers_responded = peers_responded,
            results = results.len(),
            "Live query complete"
        );

        Ok((results, peers_responded))
    }

    /// Convert a network result to a sourced result.
    fn convert_result(&self, result: NetworkSearchResult, peer_id: &str) -> SourcedResult {
        let mut sourced = SourcedResult::network(&result.cid, result.score, peer_id);

        if let Some(title) = result.title {
            sourced = sourced.with_title(title);
        }

        if let Some(snippet) = result.snippet {
            sourced = sourced.with_snippet(snippet);
        }

        sourced
    }

    /// Get pending query count (for metrics/debugging).
    pub async fn pending_count(&self) -> usize {
        self.pending_queries.read().await.len()
    }
}

/// Handle for delivering responses to pending queries.
///
/// This is used by the query handler to route incoming responses
/// to the correct pending query.
#[derive(Clone)]
pub struct LiveQueryResponseHandle {
    pending_queries: Arc<RwLock<HashMap<Uuid, PendingQuery>>>,
}

impl LiveQueryResponseHandle {
    /// Deliver a response to a pending query.
    ///
    /// # Returns
    ///
    /// `true` if the response was delivered, `false` if no pending query found.
    pub async fn deliver_response(&self, response: SearchResponseMessage) -> bool {
        let pending = self.pending_queries.read().await;

        if let Some(query) = pending.get(&response.query_id) {
            // Check if query hasn't timed out
            if query.started_at.elapsed() < Duration::from_secs(5) {
                if query.sender.send(response).await.is_ok() {
                    return true;
                }
            }
        }

        false
    }

    /// Check if a query is pending.
    pub async fn is_pending(&self, query_id: &Uuid) -> bool {
        self.pending_queries.read().await.contains_key(query_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::query::distributed::types::ResultSource;

    fn create_test_config() -> SearchConfig {
        SearchConfig::new()
            .with_peer_count(3)
            .with_timeout_ms(100) // Short timeout for testing
    }

    #[tokio::test]
    async fn test_live_query_engine_creation() {
        let config = create_test_config();
        let engine = LiveQueryEngine::new(config, "12D3KooWTest");

        assert_eq!(engine.pending_count().await, 0);
    }

    #[tokio::test]
    async fn test_live_query_no_channel() {
        let config = create_test_config();
        let engine = LiveQueryEngine::new(config, "12D3KooWTest");

        // Query without outgoing channel should still work (just won't broadcast)
        let (results, peers) = engine.query("test", 10).await.unwrap();

        assert!(results.is_empty());
        assert_eq!(peers, 0);
    }

    #[tokio::test]
    async fn test_live_query_with_responses() {
        // Use a longer timeout for the test to avoid race conditions
        let config = SearchConfig::new()
            .with_peer_count(3)
            .with_timeout_ms(2000);
        let (tx, mut rx) = mpsc::channel(10);

        let engine = LiveQueryEngine::new(config, "12D3KooWTest")
            .with_outgoing_channel(tx);

        let response_handle = engine.response_handle();

        // Spawn query in background
        let query_handle = tokio::spawn(async move {
            engine.query("test query", 20).await
        });

        // Receive the outgoing query - receiving it means the query was sent,
        // and by that point the pending query is registered
        let query_msg = rx.recv().await.expect("Should receive query message");

        // Create mock response using the query_id from the received message
        let response = SearchResponseMessage::new(
            query_msg.query_id,
            "12D3KooWPeer1",
            vec![
                NetworkSearchResult::new("cid1", 0.9),
                NetworkSearchResult::new("cid2", 0.8),
            ],
            2,
        );

        // Deliver response - it should succeed because the query is registered
        let delivered = response_handle.deliver_response(response).await;
        assert!(delivered, "Response should be delivered to pending query");

        // Wait for query to complete
        let (results, peers_responded) = query_handle.await.unwrap().unwrap();

        assert_eq!(peers_responded, 1);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].cid, "cid1");
        assert_eq!(results[0].source, ResultSource::Network);
    }

    #[tokio::test]
    async fn test_response_handle_unknown_query() {
        let config = create_test_config();
        let engine = LiveQueryEngine::new(config, "12D3KooWTest");
        let handle = engine.response_handle();

        // Response for unknown query should return false
        let response = SearchResponseMessage::new(
            Uuid::new_v4(),
            "peer1",
            Vec::new(),
            0,
        );

        assert!(!handle.deliver_response(response).await);
    }

    #[tokio::test]
    async fn test_response_handle_is_pending() {
        let config = create_test_config();
        let engine = LiveQueryEngine::new(config, "12D3KooWTest");
        let handle = engine.response_handle();

        // Unknown query should not be pending
        assert!(!handle.is_pending(&Uuid::new_v4()).await);
    }
}
