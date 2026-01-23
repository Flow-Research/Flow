//! Prefetch search adapter for pre-indexed remote content.
//!
//! This adapter queries the remote-content-idx Qdrant collection which
//! contains content that was announced via GossipSub and pre-indexed locally.

use std::sync::Arc;

use async_trait::async_trait;
use tracing::{debug, info};

use super::SearchAdapter;
use crate::modules::query::distributed::error::{DistributedSearchError, SearchResult};
use crate::modules::query::distributed::types::{ResultSource, SourcedResult};
use crate::modules::query::{FederatedSearch, SearchScope as FederatedScope};

/// Collection name for pre-indexed remote content.
pub const REMOTE_CONTENT_COLLECTION: &str = "remote-content-idx";

/// Adapter for searching pre-indexed remote content.
///
/// Queries the `remote-content-idx` Qdrant collection which contains
/// embeddings for content announced via GossipSub and indexed locally.
/// This provides a fast path for discovering remote content without
/// requiring live network queries.
pub struct PrefetchSearchAdapter {
    /// The underlying federated search instance.
    federated_search: Arc<FederatedSearch>,
}

impl PrefetchSearchAdapter {
    /// Create a new prefetch search adapter.
    pub fn new(federated_search: Arc<FederatedSearch>) -> Self {
        Self { federated_search }
    }

    /// Get the collection name being searched.
    pub fn collection_name(&self) -> &'static str {
        REMOTE_CONTENT_COLLECTION
    }
}

#[async_trait]
impl SearchAdapter for PrefetchSearchAdapter {
    async fn search(&self, query: &str, limit: u32) -> SearchResult<Vec<SourcedResult>> {
        info!(
            query = %query,
            limit = limit,
            collection = REMOTE_CONTENT_COLLECTION,
            "Prefetch search starting"
        );

        // Use FederatedSearch with Remote scope to query remote-content-idx
        let federated_results = self
            .federated_search
            .search(query, FederatedScope::Remote, limit as usize)
            .await
            .map_err(|e| DistributedSearchError::Qdrant(e.to_string()))?;

        debug!(
            count = federated_results.results.len(),
            "Prefetch search complete"
        );

        // Convert FederatedResult to SourcedResult
        // Note: Prefetch results are considered network results since they come
        // from remote content, just pre-indexed locally for speed
        let results = federated_results
            .results
            .into_iter()
            .map(|r| {
                // Mark as network result since the content is from remote peers
                let mut sourced =
                    SourcedResult::new(&r.cid, r.score, ResultSource::Network, "prefetch:remote");

                if let Some(content) = r.content {
                    sourced = sourced.with_snippet(content);
                }

                sourced
            })
            .collect();

        Ok(results)
    }

    fn name(&self) -> &'static str {
        "prefetch"
    }

    async fn health_check(&self) -> SearchResult<()> {
        // Prefetch adapter is healthy as long as federated search is available
        // The remote-content-idx collection may or may not exist depending on
        // whether any remote content has been indexed
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::ai::config::IndexingConfig;

    fn create_test_indexing_config() -> IndexingConfig {
        IndexingConfig {
            ai_api_key: String::new(),
            redis_url: None,
            qdrant_url: "http://localhost:6334".to_string(),
            qdrant_skip_api_key: true,
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
        }
    }

    #[tokio::test]
    async fn test_prefetch_adapter_creation() {
        let config = create_test_indexing_config();
        let federated = Arc::new(FederatedSearch::new(config));
        let adapter = PrefetchSearchAdapter::new(federated);

        assert_eq!(adapter.name(), "prefetch");
        assert_eq!(adapter.collection_name(), "remote-content-idx");
    }

    #[tokio::test]
    async fn test_prefetch_adapter_health_check() {
        let config = create_test_indexing_config();
        let federated = Arc::new(FederatedSearch::new(config));
        let adapter = PrefetchSearchAdapter::new(federated);

        // Health check should succeed
        let result = adapter.health_check().await;
        assert!(result.is_ok());
    }
}
