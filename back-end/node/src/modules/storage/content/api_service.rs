//! Content service layer for API business logic.
//!
//! This service encapsulates business logic for content operations,
//! keeping REST handlers thin and focused on HTTP concerns.
//!
//! For block-level operations (chunking, DAG, network), see `service.rs`.

use chrono::{DateTime, Utc};
use entity::{published_content, remote_content};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder, QuerySelect,
};
use tracing::{info, warn};

use crate::modules::ai::config::IndexingConfig;
use crate::modules::query::{FederatedSearch, ResultSource, SearchScope};
use crate::modules::storage::content::ContentId;

// ============================================================================
// Service Error Types
// ============================================================================

/// Errors that can occur in content API service operations.
#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("Database error: {0}")]
    Database(#[from] sea_orm::DbErr),

    #[error("Content not found: {0}")]
    NotFound(String),

    #[error("Content not indexable: {0}")]
    NotIndexable(String),

    #[error("Search error: {0}")]
    Search(String),

    #[error("Configuration error: {0}")]
    Config(String),
}

// ============================================================================
// Domain Types (Service Layer)
// ============================================================================

/// Published content record from service layer.
#[derive(Debug, Clone)]
pub struct PublishedContent {
    pub cid: String,
    pub size: i64,
    pub block_count: i32,
    pub title: Option<String>,
    pub content_type: Option<String>,
    pub published_at: DateTime<Utc>,
    pub indexed: bool,
}

/// Discovered remote content record from service layer.
#[derive(Debug, Clone)]
pub struct DiscoveredContent {
    pub cid: String,
    pub size: i64,
    pub block_count: i32,
    pub title: Option<String>,
    pub content_type: Option<String>,
    pub publisher_peer_id: String,
    pub indexed: bool,
    pub discovered_at: DateTime<Utc>,
}

/// Unified content info from either local or remote source.
#[derive(Debug)]
pub struct ContentInfo {
    pub cid: String,
    pub size: i64,
    pub block_count: i32,
    pub content_type: Option<String>,
    pub title: Option<String>,
    pub source: ContentSource,
    pub indexed: bool,
    pub published_at: Option<DateTime<Utc>>,
    pub discovered_at: Option<DateTime<Utc>>,
}

/// Source of content (local or remote).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentSource {
    Local,
    Remote,
}

/// Result of a publish operation.
#[derive(Debug)]
pub struct PublishResult {
    pub cid: String,
    pub size: i64,
    pub block_count: i32,
    pub title: Option<String>,
    pub already_published: bool,
}

/// Result of an index operation.
#[derive(Debug)]
pub struct IndexResult {
    pub cid: String,
    pub indexed: bool,
    pub embedding_count: Option<i32>,
    pub error: Option<String>,
}

/// Provider information.
#[derive(Debug)]
pub struct Provider {
    pub peer_id: String,
    pub is_local: bool,
}

/// Search result from federated search.
#[derive(Debug)]
pub struct SearchResult {
    pub cid: String,
    pub score: f32,
    pub source: ContentSource,
    pub title: Option<String>,
    pub snippet: Option<String>,
}

/// Aggregated search results.
#[derive(Debug)]
pub struct SearchResults {
    pub results: Vec<SearchResult>,
    pub local_count: usize,
    pub remote_count: usize,
}

// ============================================================================
// Content API Service
// ============================================================================

/// Service for content-related API business logic.
///
/// This service handles database operations and business rules for the API layer,
/// keeping HTTP handlers thin and focused on request/response handling.
pub struct ContentService<'a> {
    db: &'a DatabaseConnection,
    local_peer_id: String,
}

impl<'a> ContentService<'a> {
    /// Create a new content API service instance.
    pub fn new(db: &'a DatabaseConnection, local_peer_id: String) -> Self {
        Self { db, local_peer_id }
    }

    /// Publish content with the given data.
    ///
    /// Computes the CID from content bytes, checks for duplicates (idempotent),
    /// and inserts a new record if needed.
    pub async fn publish_content(
        &self,
        data: &[u8],
        title: Option<String>,
        content_type: Option<String>,
    ) -> Result<PublishResult, ServiceError> {
        let size = data.len() as i64;

        // Compute real CID using ContentId (SHA256 + CIDv1)
        let content_id = ContentId::from_bytes(data);
        let cid = content_id.to_string();

        info!(cid = %cid, size = size, title = ?title, "Publishing content with computed CID");

        // Check if already published (idempotent)
        if let Some(existing) = published_content::Entity::find()
            .filter(published_content::Column::Cid.eq(&cid))
            .one(self.db)
            .await?
        {
            info!(cid = %cid, "Content already published, returning existing record");
            return Ok(PublishResult {
                cid: existing.cid,
                size: existing.size,
                block_count: existing.block_count,
                title: existing.title,
                already_published: true,
            });
        }

        // Insert published_content record
        let record = published_content::ActiveModel {
            cid: Set(cid.clone()),
            content_type: Set(content_type),
            title: Set(title.clone()),
            size: Set(size),
            block_count: Set(1), // Single block for now (no chunking in MVP)
            published_at: Set(Utc::now().into()),
            qdrant_collection: Set(None),
            ..Default::default()
        };

        record.insert(self.db).await?;

        info!(cid = %cid, "Content published successfully");

        Ok(PublishResult {
            cid,
            size,
            block_count: 1,
            title,
            already_published: false,
        })
    }

    /// Get content info by CID.
    ///
    /// Checks both local (published) and remote (discovered) content.
    pub async fn get_content(&self, cid: &str) -> Result<ContentInfo, ServiceError> {
        // Try published_content first
        if let Some(published) = published_content::Entity::find()
            .filter(published_content::Column::Cid.eq(cid))
            .one(self.db)
            .await?
        {
            return Ok(ContentInfo {
                cid: published.cid,
                size: published.size,
                block_count: published.block_count,
                content_type: published.content_type,
                title: published.title,
                source: ContentSource::Local,
                indexed: published.qdrant_collection.is_some(),
                published_at: Some(published.published_at.into()),
                discovered_at: None,
            });
        }

        // Try remote_content
        if let Some(remote) = remote_content::Entity::find()
            .filter(remote_content::Column::Cid.eq(cid))
            .one(self.db)
            .await?
        {
            return Ok(ContentInfo {
                cid: remote.cid,
                size: remote.size,
                block_count: remote.block_count,
                content_type: remote.content_type,
                title: remote.title,
                source: ContentSource::Remote,
                indexed: remote.indexed,
                published_at: None,
                discovered_at: Some(remote.discovered_at.into()),
            });
        }

        Err(ServiceError::NotFound(cid.to_string()))
    }

    /// List published content with pagination.
    pub async fn list_published(
        &self,
        limit: usize,
        offset: usize,
    ) -> Result<(Vec<PublishedContent>, usize), ServiceError> {
        let items: Vec<published_content::Model> = published_content::Entity::find()
            .order_by_desc(published_content::Column::PublishedAt)
            .offset(offset as u64)
            .limit(limit as u64)
            .all(self.db)
            .await?;

        let total = items.len();

        let result = items
            .into_iter()
            .map(|p| PublishedContent {
                cid: p.cid,
                size: p.size,
                block_count: p.block_count,
                title: p.title,
                content_type: p.content_type,
                published_at: p.published_at.into(),
                indexed: p.qdrant_collection.is_some(),
            })
            .collect();

        Ok((result, total))
    }

    /// List discovered remote content with pagination.
    pub async fn list_discovered(
        &self,
        limit: usize,
        offset: usize,
    ) -> Result<(Vec<DiscoveredContent>, usize), ServiceError> {
        let items: Vec<remote_content::Model> = remote_content::Entity::find()
            .order_by_desc(remote_content::Column::DiscoveredAt)
            .offset(offset as u64)
            .limit(limit as u64)
            .all(self.db)
            .await?;

        let total = items.len();

        let result = items
            .into_iter()
            .map(|r| DiscoveredContent {
                cid: r.cid,
                size: r.size,
                block_count: r.block_count,
                title: r.title,
                content_type: r.content_type,
                publisher_peer_id: r.publisher_peer_id,
                indexed: r.indexed,
                discovered_at: r.discovered_at.into(),
            })
            .collect();

        Ok((result, total))
    }

    /// Trigger indexing for remote content.
    ///
    /// Validates content type and queues for indexing.
    pub async fn trigger_index(&self, cid: &str) -> Result<IndexResult, ServiceError> {
        info!(cid = %cid, "Triggering index for content");

        // Check if content exists in remote_content
        let remote = remote_content::Entity::find()
            .filter(remote_content::Column::Cid.eq(cid))
            .one(self.db)
            .await?
            .ok_or_else(|| ServiceError::NotFound(format!("Remote content: {}", cid)))?;

        // Already indexed - return success
        if remote.indexed {
            info!(cid = %cid, "Content already indexed");
            return Ok(IndexResult {
                cid: cid.to_string(),
                indexed: true,
                embedding_count: remote.embedding_count,
                error: None,
            });
        }

        // Check content type is indexable
        let is_indexable = remote.content_type.as_ref().map_or(false, |ct| {
            ct.starts_with("text/") || ct == "application/json" || ct == "application/markdown"
        });

        if !is_indexable {
            let error_msg = format!(
                "Content type '{}' is not indexable",
                remote.content_type.as_deref().unwrap_or("unknown")
            );
            warn!(cid = %cid, content_type = ?remote.content_type, "Content not indexable");

            // Update record with error
            let mut active: remote_content::ActiveModel = remote.into();
            active.index_error = Set(Some(error_msg.clone()));
            active.update(self.db).await?;

            return Ok(IndexResult {
                cid: cid.to_string(),
                indexed: false,
                embedding_count: None,
                error: Some(error_msg),
            });
        }

        // Queue for async indexing via PipelineManager
        info!(cid = %cid, "Queueing content for indexing");

        // Mark as pending indexing (clear any previous error)
        let mut active: remote_content::ActiveModel = remote.into();
        active.index_error = Set(None);
        active.update(self.db).await?;

        Ok(IndexResult {
            cid: cid.to_string(),
            indexed: false,
            embedding_count: None,
            error: None,
        })
    }

    /// List providers for content.
    pub async fn list_providers(&self, cid: &str) -> Result<Vec<Provider>, ServiceError> {
        let mut providers = Vec::new();

        // Check if we have it locally
        if published_content::Entity::find()
            .filter(published_content::Column::Cid.eq(cid))
            .one(self.db)
            .await?
            .is_some()
        {
            providers.push(Provider {
                peer_id: self.local_peer_id.clone(),
                is_local: true,
            });
        }

        // Check remote content for publisher
        if let Some(remote) = remote_content::Entity::find()
            .filter(remote_content::Column::Cid.eq(cid))
            .one(self.db)
            .await?
        {
            providers.push(Provider {
                peer_id: remote.publisher_peer_id,
                is_local: false,
            });
        }

        Ok(providers)
    }

    /// Perform federated search across content.
    pub async fn search(
        &self,
        query: &str,
        scope: SearchScope,
        limit: usize,
    ) -> Result<SearchResults, ServiceError> {
        info!(query = %query, "Performing federated search");

        // Create FederatedSearch instance with config from env
        let config = IndexingConfig::from_env()
            .map_err(|e| ServiceError::Config(e.to_string()))?;

        let federated_search = FederatedSearch::new(config);

        // Perform federated search
        match federated_search.search(query, scope, limit).await {
            Ok(results) => {
                info!(
                    query = %query,
                    total = results.results.len(),
                    local = results.local_count,
                    remote = results.remote_count,
                    "Federated search completed"
                );

                let items = results
                    .results
                    .into_iter()
                    .map(|r| SearchResult {
                        cid: r.cid,
                        score: r.adjusted_score,
                        source: match r.source {
                            ResultSource::Local => ContentSource::Local,
                            ResultSource::Remote => ContentSource::Remote,
                        },
                        title: None,
                        snippet: r.content,
                    })
                    .collect();

                Ok(SearchResults {
                    results: items,
                    local_count: results.local_count,
                    remote_count: results.remote_count,
                })
            }
            Err(e) => {
                warn!(error = %e, query = %query, "Federated search failed");
                // Return empty results on error rather than failing
                Ok(SearchResults {
                    results: Vec::new(),
                    local_count: 0,
                    remote_count: 0,
                })
            }
        }
    }
}
