//! Main SearchRouter entry point.
//!
//! The SearchRouter is the public API for distributed search, handling
//! request validation, caching, and delegation to the orchestrator.

use std::sync::Arc;
use std::time::Instant;

use tokio::sync::RwLock;
use tracing::{debug, info};

use super::adapters::{LocalSearchAdapter, PrefetchSearchAdapter};
use super::cache::{cache_key, CacheStats, ResultCache};
use super::config::SearchConfig;
use super::error::{RequestValidator, SearchResult};
use super::live_query::LiveQueryEngine;
use super::orchestrator::SearchOrchestrator;
use super::types::{DistributedSearchRequest, DistributedSearchResponse};
use crate::modules::query::FederatedSearch;

/// Main entry point for distributed search operations.
///
/// # Features
///
/// - Request validation
/// - Result caching with TTL
/// - Parallel search orchestration
/// - Graceful degradation on errors
///
/// # Example
///
/// ```ignore
/// let router = SearchRouter::new(federated_search, config);
/// let request = DistributedSearchRequest::new("search query")
///     .with_scope(DistributedSearchScope::All)
///     .with_limit(10);
///
/// let response = router.search(request).await?;
/// ```
pub struct SearchRouter {
    /// Search orchestrator.
    orchestrator: SearchOrchestrator,

    /// Result cache.
    cache: Arc<RwLock<ResultCache>>,

    /// Configuration.
    config: SearchConfig,
}

impl SearchRouter {
    /// Create a new SearchRouter.
    ///
    /// # Arguments
    ///
    /// * `federated_search` - The federated search instance for local/remote queries.
    /// * `config` - Search configuration.
    pub fn new(federated_search: Arc<FederatedSearch>, config: SearchConfig) -> Self {
        // Create adapters
        let local_adapter = Arc::new(LocalSearchAdapter::new(federated_search.clone()));
        let prefetch_adapter = Arc::new(PrefetchSearchAdapter::new(federated_search));

        // Create live query engine (without network connection for now)
        let live_query = Arc::new(LiveQueryEngine::new(config.clone(), "local"));

        // Create orchestrator
        let orchestrator =
            SearchOrchestrator::new(local_adapter, prefetch_adapter, live_query, config.clone());

        // Create cache
        let cache = Arc::new(RwLock::new(ResultCache::new(
            config.cache_max_entries,
            config.cache_ttl(),
        )));

        Self {
            orchestrator,
            cache,
            config,
        }
    }

    /// Create a SearchRouter with a custom orchestrator.
    ///
    /// This is useful for testing or advanced configurations.
    pub fn with_orchestrator(orchestrator: SearchOrchestrator, config: SearchConfig) -> Self {
        let cache = Arc::new(RwLock::new(ResultCache::new(
            config.cache_max_entries,
            config.cache_ttl(),
        )));

        Self {
            orchestrator,
            cache,
            config,
        }
    }

    /// Execute a search request.
    ///
    /// # Arguments
    ///
    /// * `request` - The search request.
    ///
    /// # Returns
    ///
    /// `SearchResult<DistributedSearchResponse>` with search results.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Request validation fails
    /// - All search sources fail
    pub async fn search(
        &self,
        request: DistributedSearchRequest,
    ) -> SearchResult<DistributedSearchResponse> {
        let start = Instant::now();

        // Validate request
        RequestValidator::validate(&request.query, request.limit)?;

        info!(
            query = %request.query,
            scope = ?request.scope,
            limit = request.limit,
            "SearchRouter processing request"
        );

        // Generate cache key
        let key = cache_key(&request.query, request.scope.as_str(), request.limit);

        // Check cache
        {
            let mut cache = self.cache.write().await;
            if let Some(cached) = cache.get(&key) {
                debug!(cache_key = %key, "Cache hit");
                return Ok(cached);
            }
        }

        debug!(cache_key = %key, "Cache miss");

        // Execute search via orchestrator
        let response = self.orchestrator.search(&request).await?;

        // Store in cache
        {
            let mut cache = self.cache.write().await;
            cache.insert(key.clone(), response.clone());
        }

        info!(
            results = response.results.len(),
            elapsed_ms = start.elapsed().as_millis(),
            "SearchRouter request complete"
        );

        Ok(response)
    }

    /// Execute a search with graceful degradation.
    ///
    /// This method never fails - it returns partial results if some
    /// sources are unavailable.
    pub async fn search_with_fallback(
        &self,
        request: DistributedSearchRequest,
    ) -> DistributedSearchResponse {
        match self.search(request.clone()).await {
            Ok(response) => response,
            Err(e) => {
                tracing::warn!(error = %e, "Search failed, returning empty response");
                DistributedSearchResponse::success(&request.query, request.scope, Vec::new())
            }
        }
    }

    /// Check the health of the search system.
    pub async fn health_check(&self) -> SearchResult<()> {
        self.orchestrator.health_check().await
    }

    /// Get cache statistics.
    pub async fn cache_stats(&self) -> CacheStats {
        self.cache.read().await.stats()
    }

    /// Clear the search cache.
    pub async fn clear_cache(&self) {
        self.cache.write().await.clear();
    }

    /// Get the current configuration.
    pub fn config(&self) -> &SearchConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::ai::config::IndexingConfig;
    use crate::modules::query::distributed::error::DistributedSearchError;
    use crate::modules::query::distributed::types::DistributedSearchScope;

    fn create_test_config() -> SearchConfig {
        SearchConfig::new()
            .with_peer_count(3)
            .with_timeout_ms(100)
            .with_cache_ttl_secs(1)
    }

    fn create_test_indexing_config() -> IndexingConfig {
        IndexingConfig {
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
        }
    }

    fn create_test_router() -> SearchRouter {
        let indexing_config = create_test_indexing_config();

        let federated = Arc::new(FederatedSearch::new(indexing_config));
        SearchRouter::new(federated, create_test_config())
    }

    #[tokio::test]
    async fn test_router_creation() {
        let router = create_test_router();
        assert!(router.health_check().await.is_ok());
    }

    #[tokio::test]
    async fn test_router_validation_empty_query() {
        let router = create_test_router();

        let request = DistributedSearchRequest::new("");
        let result = router.search(request).await;

        assert!(result.is_err());
        match result {
            Err(DistributedSearchError::Validation(_)) => {}
            _ => panic!("Expected validation error"),
        }
    }

    #[tokio::test]
    async fn test_router_validation_high_limit() {
        let router = create_test_router();

        let request = DistributedSearchRequest::new("test").with_limit(200);
        let result = router.search(request).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_router_caching() {
        let router = create_test_router();

        let request = DistributedSearchRequest::new("test query")
            .with_scope(DistributedSearchScope::Local)
            .with_limit(10);

        // First request - cache miss
        let _response1 = router.search(request.clone()).await.unwrap();

        // Second request - should be cache hit
        let _response2 = router.search(request).await.unwrap();

        let stats = router.cache_stats().await;
        assert!(stats.hits > 0);
    }

    #[tokio::test]
    async fn test_router_clear_cache() {
        let router = create_test_router();

        let request = DistributedSearchRequest::new("test query")
            .with_scope(DistributedSearchScope::Local);

        // Populate cache
        let _ = router.search(request).await;

        assert!(router.cache_stats().await.entries > 0);

        // Clear cache
        router.clear_cache().await;

        assert_eq!(router.cache_stats().await.entries, 0);
    }

    #[tokio::test]
    async fn test_router_fallback() {
        let router = create_test_router();

        // Even with an invalid query, fallback should return empty response
        let request = DistributedSearchRequest::new("");
        let response = router.search_with_fallback(request).await;

        assert!(response.success);
        assert!(response.results.is_empty());
    }

    #[tokio::test]
    async fn test_router_local_search() {
        let router = create_test_router();

        let request = DistributedSearchRequest::new("test query")
            .with_scope(DistributedSearchScope::Local)
            .with_limit(10);

        let response = router.search(request).await.unwrap();

        assert!(response.success);
        assert_eq!(response.scope, DistributedSearchScope::Local);
        assert!(response.elapsed_ms > 0);
    }
}
