//! Content API handlers for publishing, fetching, and searching content.
//!
//! These handlers follow the thin controller pattern:
//! - Extract request parameters
//! - Validate input
//! - Delegate to ContentService
//! - Convert to HTTP response

use axum::{
    extract::{Multipart, Path, Query, State},
    response::Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::api::dto::ApiError;
use crate::api::servers::app_state::AppState;
use crate::modules::query::SearchScope;
use crate::modules::storage::content::api_service::{
    ContentService, ContentSource, ServiceError,
};

// ============================================================================
// Request/Response DTOs
// ============================================================================

/// Response after publishing content.
#[derive(Debug, Serialize)]
pub struct PublishResponse {
    pub success: bool,
    pub cid: String,
    pub size: i64,
    pub block_count: i32,
    pub title: Option<String>,
    pub announced: bool,
}

/// Response for content info.
#[derive(Debug, Serialize)]
pub struct ContentInfoResponse {
    pub cid: String,
    pub size: i64,
    pub block_count: i32,
    pub content_type: Option<String>,
    pub title: Option<String>,
    pub source: String,
    pub indexed: bool,
    pub published_at: Option<DateTime<Utc>>,
    pub discovered_at: Option<DateTime<Utc>>,
}

/// Item in published content list.
#[derive(Debug, Serialize)]
pub struct PublishedContentItem {
    pub cid: String,
    pub title: Option<String>,
    pub size: i64,
    pub content_type: Option<String>,
    pub published_at: DateTime<Utc>,
}

/// Response for listing published content.
#[derive(Debug, Serialize)]
pub struct ListPublishedResponse {
    pub success: bool,
    pub items: Vec<PublishedContentItem>,
    pub total: usize,
}

/// Item in discovered content list.
#[derive(Debug, Serialize)]
pub struct DiscoveredContentItem {
    pub cid: String,
    pub title: Option<String>,
    pub size: i64,
    pub content_type: Option<String>,
    pub publisher_peer_id: String,
    pub indexed: bool,
    pub discovered_at: DateTime<Utc>,
}

/// Response for listing discovered content.
#[derive(Debug, Serialize)]
pub struct ListDiscoveredResponse {
    pub success: bool,
    pub items: Vec<DiscoveredContentItem>,
    pub total: usize,
}

/// Response after triggering indexing.
#[derive(Debug, Serialize)]
pub struct IndexResponse {
    pub success: bool,
    pub cid: String,
    pub indexed: bool,
    pub embedding_count: Option<i32>,
    pub error: Option<String>,
}

/// Provider info for content.
#[derive(Debug, Serialize)]
pub struct ProviderInfo {
    pub peer_id: String,
    pub is_local: bool,
}

/// Response for listing providers.
#[derive(Debug, Serialize)]
pub struct ProvidersResponse {
    pub success: bool,
    pub cid: String,
    pub providers: Vec<ProviderInfo>,
}

/// Search request body.
#[derive(Debug, Deserialize)]
pub struct SearchRequest {
    pub query: String,
    pub scope: Option<String>,
    pub limit: Option<usize>,
}

/// Search result item.
#[derive(Debug, Serialize)]
pub struct SearchResultItem {
    pub cid: String,
    pub score: f32,
    pub source: String,
    pub title: Option<String>,
    pub snippet: Option<String>,
}

/// Response for search.
#[derive(Debug, Serialize)]
pub struct SearchResponse {
    pub success: bool,
    pub query: String,
    pub results: Vec<SearchResultItem>,
    pub local_count: usize,
    pub remote_count: usize,
}

/// Query parameters for list endpoints.
#[derive(Debug, Deserialize)]
pub struct ListParams {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

// ============================================================================
// Error Conversion
// ============================================================================

impl From<ServiceError> for ApiError {
    fn from(err: ServiceError) -> Self {
        match err {
            ServiceError::Database(e) => ApiError::internal(e.to_string()),
            ServiceError::NotFound(msg) => ApiError::not_found(&msg),
            ServiceError::NotIndexable(msg) => ApiError::validation(msg),
            ServiceError::Search(msg) => ApiError::internal(msg),
            ServiceError::Config(msg) => ApiError::internal(msg),
        }
    }
}

// ============================================================================
// Handlers (Thin Controllers)
// ============================================================================

/// POST /api/v1/content/publish
pub async fn publish(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<PublishResponse>, ApiError> {
    info!("Content publish request received");

    // 1. Extract: Parse multipart form data
    let mut file_data: Option<Vec<u8>> = None;
    let mut title: Option<String> = None;
    let mut content_type: Option<String> = None;

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();

        match name.as_str() {
            "file" => {
                content_type = field.content_type().map(|s| s.to_string());
                if title.is_none() {
                    title = field.file_name().map(|s| s.to_string());
                }
                file_data = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|e| ApiError::validation(format!("Failed to read file: {}", e)))?
                        .to_vec(),
                );
            }
            "title" => {
                title = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| ApiError::validation(format!("Failed to read title: {}", e)))?,
                );
            }
            _ => {}
        }
    }

    // 2. Validate: Ensure file is provided
    let data = file_data.ok_or_else(|| ApiError::validation("No file provided"))?;

    // 3. Delegate: Call service layer
    let node = state.node.read().await;
    let service = ContentService::new(&node.db, node.peer_id());

    let result = service.publish_content(&data, title, content_type).await?;

    // 4. Respond: Convert to API response
    Ok(Json(PublishResponse {
        success: true,
        cid: result.cid,
        size: result.size,
        block_count: result.block_count,
        title: result.title,
        announced: false,
    }))
}

/// GET /api/v1/content/{cid}
pub async fn get(
    State(state): State<AppState>,
    Path(cid): Path<String>,
) -> Result<Json<ContentInfoResponse>, ApiError> {
    info!(cid = %cid, "Content info request");

    let node = state.node.read().await;
    let service = ContentService::new(&node.db, node.peer_id());

    let info = service.get_content(&cid).await?;

    Ok(Json(ContentInfoResponse {
        cid: info.cid,
        size: info.size,
        block_count: info.block_count,
        content_type: info.content_type,
        title: info.title,
        source: match info.source {
            ContentSource::Local => "local".to_string(),
            ContentSource::Remote => "remote".to_string(),
        },
        indexed: info.indexed,
        published_at: info.published_at,
        discovered_at: info.discovered_at,
    }))
}

/// GET /api/v1/content/published
pub async fn list_published(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Result<Json<ListPublishedResponse>, ApiError> {
    let limit = params.limit.unwrap_or(50).min(100);
    let offset = params.offset.unwrap_or(0);

    let node = state.node.read().await;
    let service = ContentService::new(&node.db, node.peer_id());

    let (items, total) = service.list_published(limit, offset).await?;

    Ok(Json(ListPublishedResponse {
        success: true,
        items: items
            .into_iter()
            .map(|p| PublishedContentItem {
                cid: p.cid,
                title: p.title,
                size: p.size,
                content_type: p.content_type,
                published_at: p.published_at,
            })
            .collect(),
        total,
    }))
}

/// GET /api/v1/content/discovered
pub async fn list_discovered(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Result<Json<ListDiscoveredResponse>, ApiError> {
    let limit = params.limit.unwrap_or(50).min(100);
    let offset = params.offset.unwrap_or(0);

    let node = state.node.read().await;
    let service = ContentService::new(&node.db, node.peer_id());

    let (items, total) = service.list_discovered(limit, offset).await?;

    Ok(Json(ListDiscoveredResponse {
        success: true,
        items: items
            .into_iter()
            .map(|r| DiscoveredContentItem {
                cid: r.cid,
                title: r.title,
                size: r.size,
                content_type: r.content_type,
                publisher_peer_id: r.publisher_peer_id,
                indexed: r.indexed,
                discovered_at: r.discovered_at,
            })
            .collect(),
        total,
    }))
}

/// POST /api/v1/content/{cid}/index
pub async fn trigger_index(
    State(state): State<AppState>,
    Path(cid): Path<String>,
) -> Result<Json<IndexResponse>, ApiError> {
    info!(cid = %cid, "Index trigger request");

    let node = state.node.read().await;
    let service = ContentService::new(&node.db, node.peer_id());

    let result = service.trigger_index(&cid).await?;

    Ok(Json(IndexResponse {
        success: result.error.is_none(),
        cid: result.cid,
        indexed: result.indexed,
        embedding_count: result.embedding_count,
        error: result.error,
    }))
}

/// GET /api/v1/content/{cid}/providers
pub async fn providers(
    State(state): State<AppState>,
    Path(cid): Path<String>,
) -> Result<Json<ProvidersResponse>, ApiError> {
    info!(cid = %cid, "Providers request");

    let node = state.node.read().await;
    let service = ContentService::new(&node.db, node.peer_id());

    let providers = service.list_providers(&cid).await?;

    Ok(Json(ProvidersResponse {
        success: true,
        cid,
        providers: providers
            .into_iter()
            .map(|p| ProviderInfo {
                peer_id: p.peer_id,
                is_local: p.is_local,
            })
            .collect(),
    }))
}

/// POST /api/v1/search
pub async fn search(
    State(state): State<AppState>,
    Json(request): Json<SearchRequest>,
) -> Result<Json<SearchResponse>, ApiError> {
    info!(query = %request.query, "Search request");

    let limit = request.limit.unwrap_or(10).min(100);
    let scope = match request.scope.as_deref() {
        Some("local") => SearchScope::Local,
        Some("remote") => SearchScope::Remote,
        _ => SearchScope::All,
    };

    let node = state.node.read().await;
    let service = ContentService::new(&node.db, node.peer_id());

    let results = service.search(&request.query, scope, limit).await?;

    Ok(Json(SearchResponse {
        success: true,
        query: request.query,
        results: results
            .results
            .into_iter()
            .map(|r| SearchResultItem {
                cid: r.cid,
                score: r.score,
                source: match r.source {
                    ContentSource::Local => "local".to_string(),
                    ContentSource::Remote => "remote".to_string(),
                },
                title: r.title,
                snippet: r.snippet,
            })
            .collect(),
        local_count: results.local_count,
        remote_count: results.remote_count,
    }))
}
