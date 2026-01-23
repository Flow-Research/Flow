//! Local search adapter wrapping FederatedSearch.
//!
//! This adapter queries local space collections (space-*-idx) via the
//! existing FederatedSearch infrastructure.

use std::sync::Arc;

use async_trait::async_trait;
use tracing::{debug, info, warn};

use super::SearchAdapter;
use crate::modules::query::distributed::error::{DistributedSearchError, SearchResult};
use crate::modules::query::distributed::types::SourcedResult;
use crate::modules::query::{FederatedSearch, SearchScope as FederatedScope};

/// Adapter for searching local space collections.
///
/// Wraps the existing `FederatedSearch` to query local Qdrant collections
/// (space-*-idx) and returns results in the distributed search format.
pub struct LocalSearchAdapter {
    /// The underlying federated search instance.
    federated_search: Arc<FederatedSearch>,
}

impl LocalSearchAdapter {
    /// Create a new local search adapter.
    pub fn new(federated_search: Arc<FederatedSearch>) -> Self {
        Self { federated_search }
    }

    /// Get the list of local collections being searched.
    pub async fn collections(&self) -> Vec<String> {
        self.federated_search.local_collections().await
    }
}

#[async_trait]
impl SearchAdapter for LocalSearchAdapter {
    async fn search(&self, query: &str, limit: u32) -> SearchResult<Vec<SourcedResult>> {
        info!(query = %query, limit = limit, "Local search starting");

        // Use FederatedSearch with Local scope
        let federated_results = self
            .federated_search
            .search(query, FederatedScope::Local, limit as usize)
            .await
            .map_err(|e| DistributedSearchError::Qdrant(e.to_string()))?;

        debug!(
            count = federated_results.results.len(),
            collections = ?federated_results.collections_searched,
            "Local search complete"
        );

        // Convert FederatedResult to SourcedResult
        let results = federated_results
            .results
            .into_iter()
            .map(|r| {
                let mut sourced = SourcedResult::local(&r.cid, r.score, &r.collection);

                if let Some(content) = r.content {
                    sourced = sourced.with_snippet(content);
                }

                sourced
            })
            .collect();

        Ok(results)
    }

    fn name(&self) -> &'static str {
        "local"
    }

    async fn health_check(&self) -> SearchResult<()> {
        // Check that we have at least one local collection
        let collections = self.federated_search.local_collections().await;

        if collections.is_empty() {
            warn!("No local collections available for search");
            // This is not an error - just means no local content yet
        }

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
    async fn test_local_adapter_creation() {
        let config = create_test_indexing_config();
        let federated = Arc::new(FederatedSearch::new(config));
        let adapter = LocalSearchAdapter::new(federated);

        assert_eq!(adapter.name(), "local");
    }

    #[tokio::test]
    async fn test_local_adapter_health_check() {
        let config = create_test_indexing_config();
        let federated = Arc::new(FederatedSearch::new(config));
        let adapter = LocalSearchAdapter::new(federated);

        // Health check should succeed even with no collections
        let result = adapter.health_check().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_local_adapter_empty_collections() {
        let config = create_test_indexing_config();
        let federated = Arc::new(FederatedSearch::new(config));
        let adapter = LocalSearchAdapter::new(federated);

        let collections = adapter.collections().await;
        assert!(collections.is_empty());
    }
}
