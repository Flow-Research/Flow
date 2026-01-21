//! Handler for incoming search queries from the network.
//!
//! Processes search queries received via GossipSub and sends responses
//! back to the requester when opt-in is enabled.

use std::sync::Arc;
use std::time::Instant;

use tracing::{debug, info, warn};

use super::config::SearchConfig;
use super::error::SearchResult;
use super::messages::{NetworkSearchResult, SearchQueryMessage, SearchResponseMessage};
use crate::modules::query::{FederatedSearch, SearchScope as FederatedScope};

/// Handler for incoming search queries from network peers.
///
/// # Opt-in Model
///
/// The handler only processes queries when `respond_to_queries` is enabled
/// in the configuration. This provides privacy-first behavior where nodes
/// don't share their indexed content unless explicitly enabled.
///
/// # Processing Flow
///
/// 1. Validate incoming query
/// 2. Check opt-in configuration
/// 3. Search local indexes
/// 4. Build and return response
pub struct SearchQueryHandler {
    /// Search configuration.
    config: SearchConfig,

    /// Local peer ID (base58).
    local_peer_id: String,

    /// Federated search for querying local indexes.
    federated_search: Arc<FederatedSearch>,
}

impl SearchQueryHandler {
    /// Create a new query handler.
    ///
    /// # Arguments
    ///
    /// * `config` - Search configuration.
    /// * `local_peer_id` - This node's peer ID (base58).
    /// * `federated_search` - Federated search instance for local queries.
    pub fn new(
        config: SearchConfig,
        local_peer_id: impl Into<String>,
        federated_search: Arc<FederatedSearch>,
    ) -> Self {
        Self {
            config,
            local_peer_id: local_peer_id.into(),
            federated_search,
        }
    }

    /// Update the configuration.
    ///
    /// This allows runtime updates to opt-in settings.
    pub fn update_config(&mut self, config: SearchConfig) {
        self.config = config;
    }

    /// Check if query response is enabled.
    pub fn is_responding(&self) -> bool {
        self.config.respond_to_queries
    }

    /// Handle an incoming search query.
    ///
    /// # Arguments
    ///
    /// * `query` - The incoming search query message.
    ///
    /// # Returns
    ///
    /// `Some(SearchResponseMessage)` if opt-in is enabled and query is valid,
    /// `None` otherwise.
    pub async fn handle_query(
        &self,
        query: SearchQueryMessage,
    ) -> Option<SearchResponseMessage> {
        // Log query receipt (metadata only, not query text)
        info!(
            query_id = %query.query_id,
            requester = %query.requester_peer_id,
            limit = query.limit,
            "Received search query"
        );

        // Check opt-in configuration
        if !self.config.respond_to_queries {
            debug!(
                query_id = %query.query_id,
                "Ignoring search query - opt-in disabled"
            );
            return None;
        }

        // Validate query
        if let Err(e) = query.validate() {
            warn!(
                query_id = %query.query_id,
                error = %e,
                "Invalid search query"
            );
            return None;
        }

        // Don't respond to our own queries
        if query.requester_peer_id == self.local_peer_id {
            debug!(query_id = %query.query_id, "Ignoring own query");
            return None;
        }

        // Execute search
        let start = Instant::now();
        match self.execute_search(&query).await {
            Ok(response) => {
                let elapsed = start.elapsed().as_millis() as u32;
                info!(
                    query_id = %query.query_id,
                    results = response.results.len(),
                    elapsed_ms = elapsed,
                    "Search query processed"
                );
                Some(response.with_elapsed(elapsed))
            }
            Err(e) => {
                warn!(
                    query_id = %query.query_id,
                    error = %e,
                    "Failed to process search query"
                );
                None
            }
        }
    }

    /// Execute the search and build response.
    async fn execute_search(
        &self,
        query: &SearchQueryMessage,
    ) -> SearchResult<SearchResponseMessage> {
        // Apply configured limit (don't exceed max_results_per_query)
        let limit = query.limit.min(self.config.max_results_per_query);

        // Search local indexes
        let federated_results = self
            .federated_search
            .search(&query.query, FederatedScope::Local, limit as usize)
            .await
            .map_err(|e| super::error::DistributedSearchError::Qdrant(e.to_string()))?;

        // Convert to network results
        let results: Vec<NetworkSearchResult> = federated_results
            .results
            .into_iter()
            .map(|r| {
                let mut result = NetworkSearchResult::new(&r.cid, r.score);
                if let Some(content) = r.content {
                    result = result.with_snippet(content);
                }
                result
            })
            .collect();

        let total_matches = federated_results.local_count as u32;

        Ok(SearchResponseMessage::new(
            query.query_id,
            &self.local_peer_id,
            results,
            total_matches,
        ))
    }

    /// Get handler statistics.
    pub fn stats(&self) -> QueryHandlerStats {
        QueryHandlerStats {
            responding: self.config.respond_to_queries,
            max_results: self.config.max_results_per_query,
        }
    }
}

/// Statistics for the query handler.
#[derive(Debug, Clone)]
pub struct QueryHandlerStats {
    /// Whether query response is enabled.
    pub responding: bool,
    /// Maximum results per query.
    pub max_results: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::ai::config::IndexingConfig;

    fn create_test_config(respond: bool) -> SearchConfig {
        SearchConfig::new().with_respond_to_queries(respond)
    }

    fn create_test_federated() -> Arc<FederatedSearch> {
        let config = IndexingConfig {
            ai_api_key: String::new(),
            redis_url: None,
            qdrant_url: "http://localhost:6334".to_string(),
            vector_size: 384,
            min_chunk_size: 10,
            max_chunk_size: 20_000,
            max_file_size: 10_000_000,
            embed_batch_size: 20,
            storage_batch_size: 50,
            concurrency: 4,
            rate_limit_ms: 100,
            enable_metadata_qa: false,
            enable_metadata_summary: false,
            enable_metadata_keywords: false,
            allowed_extensions: None,
            exclude_patterns: vec![],
            max_failure_rate: 0.5,
            min_sample_size: 10,
        };
        Arc::new(FederatedSearch::new(config))
    }

    #[tokio::test]
    async fn test_handler_creation() {
        let config = create_test_config(true);
        let federated = create_test_federated();
        let handler = SearchQueryHandler::new(config, "12D3KooWTest", federated);

        assert!(handler.is_responding());
    }

    #[tokio::test]
    async fn test_handler_opt_in_disabled() {
        let config = create_test_config(false);
        let federated = create_test_federated();
        let handler = SearchQueryHandler::new(config, "12D3KooWTest", federated);

        let query = SearchQueryMessage::new("test query", 20, "12D3KooWOther");
        let response = handler.handle_query(query).await;

        // Should return None when opt-in is disabled
        assert!(response.is_none());
    }

    #[tokio::test]
    async fn test_handler_ignores_own_queries() {
        let config = create_test_config(true);
        let federated = create_test_federated();
        let handler = SearchQueryHandler::new(config, "12D3KooWTest", federated);

        // Query from self
        let query = SearchQueryMessage::new("test query", 20, "12D3KooWTest");
        let response = handler.handle_query(query).await;

        // Should return None for own queries
        assert!(response.is_none());
    }

    #[tokio::test]
    async fn test_handler_rejects_invalid_query() {
        let config = create_test_config(true);
        let federated = create_test_federated();
        let handler = SearchQueryHandler::new(config, "12D3KooWTest", federated);

        // Empty query
        let query = SearchQueryMessage::new("", 20, "12D3KooWOther");
        let response = handler.handle_query(query).await;

        // Should return None for invalid query
        assert!(response.is_none());
    }

    #[tokio::test]
    async fn test_handler_update_config() {
        let config = create_test_config(false);
        let federated = create_test_federated();
        let mut handler = SearchQueryHandler::new(config, "12D3KooWTest", federated);

        assert!(!handler.is_responding());

        // Update config to enable responding
        handler.update_config(create_test_config(true));
        assert!(handler.is_responding());
    }

    #[tokio::test]
    async fn test_handler_stats() {
        let config = SearchConfig::new()
            .with_respond_to_queries(true)
            .with_max_results(50);
        let federated = create_test_federated();
        let handler = SearchQueryHandler::new(config, "12D3KooWTest", federated);

        let stats = handler.stats();
        assert!(stats.responding);
        assert_eq!(stats.max_results, 50);
    }
}
