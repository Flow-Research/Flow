//! Search handlers.
//!
//! This module provides search endpoints:
//! - `/api/v1/spaces/search` - Query a specific space
//! - `/api/v1/search/distributed` - Distributed search across local and network sources

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::response::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::info;

use crate::api::dto::ApiError;
use crate::api::servers::app_state::AppState;
use crate::modules::ai::config::IndexingConfig;
use crate::modules::query::distributed::{
    DistributedSearchRequest, DistributedSearchResponse, DistributedSearchScope, SearchConfig,
    SearchRouter,
};
use crate::modules::query::FederatedSearch;

// ============================================================================
// Request/Response DTOs for Distributed Search
// ============================================================================

/// Request body for distributed search.
#[derive(Debug, Deserialize)]
pub struct DistributedSearchRequestDto {
    /// The search query text.
    pub query: String,

    /// Search scope: "local", "network", or "all" (default).
    #[serde(default)]
    pub scope: Option<String>,

    /// Maximum results to return (1-100, default 20).
    #[serde(default)]
    pub limit: Option<u32>,
}

/// A single search result item.
#[derive(Debug, Serialize)]
pub struct DistributedSearchResultDto {
    /// Content identifier.
    pub cid: String,
    /// Relevance score (0-1).
    pub score: f32,
    /// Result source: "local" or "network".
    pub source: String,
    /// Content title if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Content snippet if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
    /// Source identifier (collection name or peer ID).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
}

/// Response for distributed search.
#[derive(Debug, Serialize)]
pub struct DistributedSearchResponseDto {
    /// Whether the search was successful.
    pub success: bool,
    /// The original query.
    pub query: String,
    /// Search scope used.
    pub scope: String,
    /// Search results sorted by relevance.
    pub results: Vec<DistributedSearchResultDto>,
    /// Number of results from local sources.
    pub local_count: u32,
    /// Number of results from network sources.
    pub network_count: u32,
    /// Number of peers queried (for network searches).
    pub peers_queried: u32,
    /// Number of peers that responded.
    pub peers_responded: u32,
    /// Total results found (may be more than returned).
    pub total_found: u32,
    /// Search time in milliseconds.
    pub elapsed_ms: u64,
}

// ============================================================================
// Handlers
// ============================================================================

/// Query a space.
///
/// GET /api/v1/spaces/search?space_key=...&query=...
pub async fn query(
    State(app_state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let space_key = params
        .get("space_key")
        .ok_or_else(|| ApiError::validation("space_key is required"))?;
    let query = params
        .get("query")
        .ok_or_else(|| ApiError::validation("query is required"))?;

    let node = app_state.node.read().await;

    let response = node
        .query_space(space_key, query)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;

    Ok(Json(json!({
        "success": true,
        "response": response
    })))
}

/// Distributed search across local and network sources.
///
/// POST /api/v1/search/distributed
///
/// Searches local indexes, pre-indexed remote content, and optionally
/// broadcasts queries to network peers for live results.
pub async fn distributed_search(
    State(_app_state): State<AppState>,
    Json(request): Json<DistributedSearchRequestDto>,
) -> Result<Json<DistributedSearchResponseDto>, ApiError> {
    info!(query = %request.query, scope = ?request.scope, "Distributed search request");

    // Parse scope
    let scope = match request.scope.as_deref() {
        Some("local") => DistributedSearchScope::Local,
        Some("network") => DistributedSearchScope::Network,
        Some("all") | None => DistributedSearchScope::All,
        Some(other) => {
            return Err(ApiError::validation(format!(
                "Invalid scope: '{}'. Must be 'local', 'network', or 'all'",
                other
            )));
        }
    };

    // Validate limit
    let limit = request.limit.unwrap_or(20);
    if limit == 0 || limit > 100 {
        return Err(ApiError::validation(
            "Limit must be between 1 and 100",
        ));
    }

    // Validate query
    if request.query.trim().is_empty() {
        return Err(ApiError::validation("Query cannot be empty"));
    }

    if request.query.len() > 1000 {
        return Err(ApiError::validation("Query too long (max 1000 chars)"));
    }

    // Create search request
    let search_request = DistributedSearchRequest::new(&request.query)
        .with_scope(scope)
        .with_limit(limit);

    // Create indexing config from environment (or use defaults for now)
    let indexing_config = IndexingConfig::from_env()
        .unwrap_or_else(|_| create_default_indexing_config());

    let federated_search = Arc::new(FederatedSearch::new(indexing_config));
    let search_config = SearchConfig::from_env();
    let router = SearchRouter::new(federated_search, search_config);

    // Execute search with graceful fallback
    let response = router.search_with_fallback(search_request).await;

    // Convert to DTO
    Ok(Json(convert_response(response)))
}

/// Health check for distributed search.
///
/// GET /api/v1/search/distributed/health
pub async fn distributed_search_health(
    State(_app_state): State<AppState>,
) -> Result<Json<Value>, ApiError> {

    let indexing_config = IndexingConfig::from_env()
        .unwrap_or_else(|_| create_default_indexing_config());

    let federated_search = Arc::new(FederatedSearch::new(indexing_config));
    let search_config = SearchConfig::from_env();
    let router = SearchRouter::new(federated_search, search_config);

    // Check health
    match router.health_check().await {
        Ok(_) => Ok(Json(json!({
            "status": "healthy",
            "cache": router.cache_stats().await,
        }))),
        Err(e) => Ok(Json(json!({
            "status": "degraded",
            "error": e.to_string(),
        }))),
    }
}

// ============================================================================
// Helpers
// ============================================================================

fn convert_response(response: DistributedSearchResponse) -> DistributedSearchResponseDto {
    DistributedSearchResponseDto {
        success: response.success,
        query: response.query,
        scope: response.scope.as_str().to_string(),
        results: response
            .results
            .into_iter()
            .map(|r| DistributedSearchResultDto {
                cid: r.cid,
                score: r.score,
                source: r.source.to_string(),
                title: r.title,
                snippet: r.snippet,
                source_id: r.publisher_peer_id,
            })
            .collect(),
        local_count: response.local_count,
        network_count: response.network_count,
        peers_queried: response.peers_queried,
        peers_responded: response.peers_responded,
        total_found: response.local_count
            .saturating_add(response.network_count)
            .saturating_add(response.more_available),
        elapsed_ms: response.elapsed_ms,
    }
}

fn create_default_indexing_config() -> IndexingConfig {
    IndexingConfig {
        ai_api_key: String::new(),
        redis_url: None,
        qdrant_url: std::env::var("QDRANT_URL")
            .unwrap_or_else(|_| "http://localhost:6334".to_string()),
        qdrant_skip_api_key: std::env::var("QDRANT_SKIP_API_KEY")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(false),
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
