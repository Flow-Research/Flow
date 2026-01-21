//! Search orchestrator for parallel execution.
//!
//! Spawns and coordinates parallel search tasks across local indexes,
//! prefetched remote content, and live network queries.

use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

use super::adapters::{LocalSearchAdapter, PrefetchSearchAdapter, SearchAdapter};
use super::config::SearchConfig;
use super::error::SearchResult;
use super::live_query::LiveQueryEngine;
use super::ranking::{AggregationResult, ResultAggregator};
use super::types::{
    DistributedSearchRequest, DistributedSearchResponse, DistributedSearchScope, SourcedResult,
};

/// Orchestrates parallel search execution across multiple sources.
///
/// # Sources
///
/// 1. **Local**: Local space collections via FederatedSearch
/// 2. **Prefetch**: Pre-indexed remote content (remote-content-idx)
/// 3. **Live**: Real-time network query via GossipSub
///
/// # Execution Model
///
/// Tasks are spawned in parallel using tokio and results are collected
/// with a timeout. The orchestrator ensures graceful degradation when
/// some sources fail or timeout.
pub struct SearchOrchestrator {
    /// Local search adapter.
    local_adapter: Arc<LocalSearchAdapter>,

    /// Prefetch search adapter.
    prefetch_adapter: Arc<PrefetchSearchAdapter>,

    /// Live query engine.
    live_query: Arc<LiveQueryEngine>,

    /// Result aggregator.
    aggregator: ResultAggregator,

    /// Configuration.
    config: SearchConfig,
}

impl SearchOrchestrator {
    /// Create a new search orchestrator.
    pub fn new(
        local_adapter: Arc<LocalSearchAdapter>,
        prefetch_adapter: Arc<PrefetchSearchAdapter>,
        live_query: Arc<LiveQueryEngine>,
        config: SearchConfig,
    ) -> Self {
        Self {
            local_adapter,
            prefetch_adapter,
            live_query,
            aggregator: ResultAggregator::new(),
            config,
        }
    }

    /// Execute a search with the given request.
    ///
    /// Spawns tasks based on scope and collects results within timeout.
    pub async fn search(
        &self,
        request: &DistributedSearchRequest,
    ) -> SearchResult<DistributedSearchResponse> {
        let start = Instant::now();

        info!(
            query = %request.query,
            scope = ?request.scope,
            limit = request.limit,
            "Orchestrator starting search"
        );

        // Execute search based on scope
        let (results, peers_queried, peers_responded) = match request.scope {
            DistributedSearchScope::Local => {
                let results = self.execute_local_only(&request.query, request.limit).await?;
                (results, 0, 0)
            }
            DistributedSearchScope::Network => {
                self.execute_network_only(&request.query, request.limit)
                    .await?
            }
            DistributedSearchScope::All => {
                self.execute_hybrid(&request.query, request.limit).await?
            }
        };

        // Aggregate results
        let aggregated = self.aggregator.aggregate(results, request.limit);

        // Build response
        let response = self.build_response(
            request,
            aggregated,
            peers_queried,
            peers_responded,
            start.elapsed(),
        );

        info!(
            results = response.results.len(),
            local_count = response.local_count,
            network_count = response.network_count,
            elapsed_ms = response.elapsed_ms,
            "Orchestrator search complete"
        );

        Ok(response)
    }

    /// Execute local-only search.
    async fn execute_local_only(
        &self,
        query: &str,
        limit: u32,
    ) -> SearchResult<Vec<SourcedResult>> {
        debug!("Executing local-only search");

        self.local_adapter.search(query, limit).await
    }

    /// Execute network-only search (prefetch + live).
    async fn execute_network_only(
        &self,
        query: &str,
        limit: u32,
    ) -> SearchResult<(Vec<SourcedResult>, u32, u32)> {
        debug!("Executing network-only search");

        let timeout = self.config.network_timeout() + Duration::from_millis(200);
        let mut all_results = Vec::new();
        let peers_queried = self.config.peer_count;
        let mut peers_responded = 0;

        // Spawn prefetch and live query in parallel
        let prefetch_adapter = self.prefetch_adapter.clone();
        let live_query = self.live_query.clone();
        let query_clone1 = query.to_string();
        let query_clone2 = query.to_string();

        let prefetch_task: JoinHandle<SearchResult<Vec<SourcedResult>>> =
            tokio::spawn(async move { prefetch_adapter.search(&query_clone1, limit).await });

        let live_task: JoinHandle<SearchResult<(Vec<SourcedResult>, u32)>> =
            tokio::spawn(async move { live_query.query(&query_clone2, limit).await });

        // Wait for prefetch with timeout
        match tokio::time::timeout(timeout, prefetch_task).await {
            Ok(Ok(Ok(results))) => {
                debug!(count = results.len(), "Prefetch search complete");
                all_results.extend(results);
            }
            Ok(Ok(Err(e))) => {
                warn!(error = %e, "Prefetch search failed");
            }
            Ok(Err(e)) => {
                warn!(error = %e, "Prefetch task panicked");
            }
            Err(_) => {
                warn!("Prefetch search timed out");
            }
        }

        // Wait for live query with timeout
        match tokio::time::timeout(timeout, live_task).await {
            Ok(Ok(Ok((results, responded)))) => {
                debug!(count = results.len(), peers = responded, "Live query complete");
                peers_responded = responded;
                all_results.extend(results);
            }
            Ok(Ok(Err(e))) => {
                warn!(error = %e, "Live query failed");
            }
            Ok(Err(e)) => {
                warn!(error = %e, "Live query task panicked");
            }
            Err(_) => {
                warn!("Live query timed out");
            }
        }

        Ok((all_results, peers_queried, peers_responded))
    }

    /// Execute hybrid search (all sources).
    async fn execute_hybrid(
        &self,
        query: &str,
        limit: u32,
    ) -> SearchResult<(Vec<SourcedResult>, u32, u32)> {
        debug!("Executing hybrid search");

        let timeout = self.config.network_timeout() + Duration::from_millis(200);
        let mut all_results = Vec::new();
        let peers_queried = self.config.peer_count;
        let mut peers_responded = 0;

        // Spawn all three tasks in parallel
        let local_adapter = self.local_adapter.clone();
        let prefetch_adapter = self.prefetch_adapter.clone();
        let live_query = self.live_query.clone();

        let query_clone1 = query.to_string();
        let query_clone2 = query.to_string();
        let query_clone3 = query.to_string();

        let local_task: JoinHandle<SearchResult<Vec<SourcedResult>>> =
            tokio::spawn(async move { local_adapter.search(&query_clone1, limit).await });

        let prefetch_task: JoinHandle<SearchResult<Vec<SourcedResult>>> =
            tokio::spawn(async move { prefetch_adapter.search(&query_clone2, limit).await });

        let live_task: JoinHandle<SearchResult<(Vec<SourcedResult>, u32)>> =
            tokio::spawn(async move { live_query.query(&query_clone3, limit).await });

        // Wait for local with shorter timeout (should be fast)
        let local_timeout = Duration::from_millis(200);
        match tokio::time::timeout(local_timeout, local_task).await {
            Ok(Ok(Ok(results))) => {
                debug!(count = results.len(), "Local search complete");
                all_results.extend(results);
            }
            Ok(Ok(Err(e))) => {
                warn!(error = %e, "Local search failed");
            }
            Ok(Err(e)) => {
                warn!(error = %e, "Local task panicked");
            }
            Err(_) => {
                warn!("Local search timed out");
            }
        }

        // Wait for prefetch with timeout
        match tokio::time::timeout(timeout, prefetch_task).await {
            Ok(Ok(Ok(results))) => {
                debug!(count = results.len(), "Prefetch search complete");
                all_results.extend(results);
            }
            Ok(Ok(Err(e))) => {
                warn!(error = %e, "Prefetch search failed");
            }
            Ok(Err(e)) => {
                warn!(error = %e, "Prefetch task panicked");
            }
            Err(_) => {
                warn!("Prefetch search timed out");
            }
        }

        // Wait for live query with timeout
        match tokio::time::timeout(timeout, live_task).await {
            Ok(Ok(Ok((results, responded)))) => {
                debug!(count = results.len(), peers = responded, "Live query complete");
                peers_responded = responded;
                all_results.extend(results);
            }
            Ok(Ok(Err(e))) => {
                warn!(error = %e, "Live query failed");
            }
            Ok(Err(e)) => {
                warn!(error = %e, "Live query task panicked");
            }
            Err(_) => {
                warn!("Live query timed out");
            }
        }

        Ok((all_results, peers_queried, peers_responded))
    }

    /// Build the final response.
    fn build_response(
        &self,
        request: &DistributedSearchRequest,
        aggregated: AggregationResult,
        peers_queried: u32,
        peers_responded: u32,
        elapsed: Duration,
    ) -> DistributedSearchResponse {
        DistributedSearchResponse::success(&request.query, request.scope, aggregated.results)
            .with_network_stats(peers_queried, peers_responded)
            .with_overflow(aggregated.overflow_count)
            .with_elapsed(elapsed.as_millis() as u64)
    }

    /// Health check for all adapters.
    pub async fn health_check(&self) -> SearchResult<()> {
        self.local_adapter.health_check().await?;
        self.prefetch_adapter.health_check().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::ai::config::IndexingConfig;
    use crate::modules::query::FederatedSearch;

    fn create_test_config() -> SearchConfig {
        SearchConfig::new()
            .with_peer_count(3)
            .with_timeout_ms(100)
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

    fn create_test_orchestrator() -> SearchOrchestrator {
        let indexing_config = create_test_indexing_config();

        let federated = Arc::new(FederatedSearch::new(indexing_config));
        let local_adapter = Arc::new(LocalSearchAdapter::new(federated.clone()));
        let prefetch_adapter = Arc::new(PrefetchSearchAdapter::new(federated));
        let live_query = Arc::new(LiveQueryEngine::new(create_test_config(), "12D3KooWTest"));

        SearchOrchestrator::new(local_adapter, prefetch_adapter, live_query, create_test_config())
    }

    #[tokio::test]
    async fn test_orchestrator_creation() {
        let orchestrator = create_test_orchestrator();
        assert!(orchestrator.health_check().await.is_ok());
    }

    #[tokio::test]
    async fn test_orchestrator_local_search() {
        let orchestrator = create_test_orchestrator();

        let request = DistributedSearchRequest::new("test query")
            .with_scope(DistributedSearchScope::Local)
            .with_limit(10);

        let response = orchestrator.search(&request).await.unwrap();

        assert!(response.success);
        assert_eq!(response.scope, DistributedSearchScope::Local);
        assert_eq!(response.peers_queried, 0);
        assert_eq!(response.peers_responded, 0);
    }

    #[tokio::test]
    async fn test_orchestrator_network_search() {
        let orchestrator = create_test_orchestrator();

        let request = DistributedSearchRequest::new("test query")
            .with_scope(DistributedSearchScope::Network)
            .with_limit(10);

        let response = orchestrator.search(&request).await.unwrap();

        assert!(response.success);
        assert_eq!(response.scope, DistributedSearchScope::Network);
        // Network search was attempted
        assert!(response.peers_queried > 0);
    }

    #[tokio::test]
    async fn test_orchestrator_hybrid_search() {
        let orchestrator = create_test_orchestrator();

        let request = DistributedSearchRequest::new("test query")
            .with_scope(DistributedSearchScope::All)
            .with_limit(10);

        let response = orchestrator.search(&request).await.unwrap();

        assert!(response.success);
        assert_eq!(response.scope, DistributedSearchScope::All);
    }

    #[tokio::test]
    async fn test_orchestrator_elapsed_time() {
        let orchestrator = create_test_orchestrator();

        let request = DistributedSearchRequest::new("test query").with_limit(10);

        let response = orchestrator.search(&request).await.unwrap();

        // Should have some elapsed time recorded
        assert!(response.elapsed_ms > 0);
    }
}
