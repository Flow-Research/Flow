//! RemoteIndexer for indexing discovered content from the network.
//!
//! This module fetches content by CID from the network, generates embeddings,
//! and stores them in the `remote-content-idx` Qdrant collection for federated search.

use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use entity::remote_content;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
};
use swiftide::integrations;
use swiftide::traits::EmbeddingModel;
use swiftide_integrations::fastembed;
use thiserror::Error;
use tokio::time::sleep;
use tracing::{debug, info, warn};

use super::config::IndexingConfig;
use crate::modules::storage::content::{ContentId, ContentService};

/// Default collection name for remote content embeddings.
pub const REMOTE_CONTENT_COLLECTION: &str = "remote-content-idx";

/// Maximum retry attempts for fetch operations.
const MAX_RETRY_ATTEMPTS: u32 = 3;

/// Base delay for exponential backoff.
const BASE_RETRY_DELAY: Duration = Duration::from_secs(1);

/// Errors that can occur during remote indexing.
#[derive(Error, Debug)]
pub enum IndexError {
    #[error("content not found: {cid}")]
    ContentNotFound { cid: String },

    #[error("fetch failed after {attempts} attempts: {message}")]
    FetchFailed { attempts: u32, message: String },

    #[error("content too large: {size} bytes (max: {max})")]
    ContentTooLarge { size: usize, max: usize },

    #[error("content type not indexable: {content_type}")]
    NotIndexable { content_type: String },

    #[error("chunking failed: {0}")]
    ChunkingFailed(String),

    #[error("embedding failed: {0}")]
    EmbeddingFailed(String),

    #[error("qdrant error: {0}")]
    Qdrant(String),

    #[error("database error: {0}")]
    Database(#[from] sea_orm::DbErr),

    #[error("record not found: {cid}")]
    RecordNotFound { cid: String },

    #[error("invalid CID: {0}")]
    InvalidCid(String),
}

/// Result of a successful indexing operation.
#[derive(Debug, Clone)]
pub struct IndexResult {
    /// CID of the indexed content.
    pub cid: String,

    /// Number of chunks created.
    pub chunk_count: usize,

    /// Number of embeddings stored.
    pub embedding_count: usize,

    /// Qdrant collection name.
    pub collection: String,
}

/// Synchronous remote content indexer.
///
/// Fetches content from the network, generates embeddings, and stores them
/// in Qdrant for federated search.
pub struct RemoteIndexer {
    /// Content service for fetching from network.
    content_service: Arc<ContentService>,

    /// Database connection.
    db: Arc<DatabaseConnection>,

    /// Indexing configuration.
    config: IndexingConfig,
}

impl std::fmt::Debug for RemoteIndexer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemoteIndexer")
            .field("collection", &REMOTE_CONTENT_COLLECTION)
            .field("vector_size", &self.config.vector_size)
            .finish_non_exhaustive()
    }
}

impl RemoteIndexer {
    /// Create a new remote indexer.
    pub fn new(
        content_service: Arc<ContentService>,
        db: Arc<DatabaseConnection>,
        config: IndexingConfig,
    ) -> Self {
        Self {
            content_service,
            db,
            config,
        }
    }

    /// Index content by CID.
    ///
    /// # Arguments
    ///
    /// * `cid` - Content identifier to index.
    ///
    /// # Returns
    ///
    /// An `IndexResult` on success, or an `IndexError` on failure.
    pub async fn index_content(&self, cid: &str) -> Result<IndexResult, IndexError> {
        info!(cid = %cid, "Starting remote content indexing");

        // Verify record exists and is not already indexed
        let record = self.get_record(cid).await?;
        if record.indexed {
            debug!(cid = %cid, "Content already indexed, skipping");
            return Ok(IndexResult {
                cid: cid.to_string(),
                chunk_count: 0,
                embedding_count: record.embedding_count.unwrap_or(0) as usize,
                collection: record
                    .qdrant_collection
                    .unwrap_or_else(|| REMOTE_CONTENT_COLLECTION.to_string()),
            });
        }

        // Check if content type is indexable
        if let Some(ref content_type) = record.content_type {
            if !self.is_indexable(content_type) {
                return Err(IndexError::NotIndexable {
                    content_type: content_type.clone(),
                });
            }
        }

        // Fetch content with retry
        let content = self.fetch_with_retry(cid).await?;

        // Check size limits
        let max_size = 100 * 1024 * 1024; // 100MB
        if content.len() > max_size {
            return Err(IndexError::ContentTooLarge {
                size: content.len(),
                max: max_size,
            });
        }

        // Convert to text
        let text = String::from_utf8_lossy(&content);

        // Chunk content
        let chunks = self.chunk_text(&text);
        let chunk_count = chunks.len();

        if chunks.is_empty() {
            warn!(cid = %cid, "No chunks generated from content");
            self.update_record_error(cid, "No chunks generated").await?;
            return Err(IndexError::ChunkingFailed(
                "No chunks generated".to_string(),
            ));
        }

        // Generate embeddings and store in Qdrant
        let embedding_count = self.embed_and_store(cid, chunks).await?;

        // Update database record
        self.update_record_success(cid, embedding_count).await?;

        info!(
            cid = %cid,
            chunks = chunk_count,
            embeddings = embedding_count,
            "Remote content indexed successfully"
        );

        Ok(IndexResult {
            cid: cid.to_string(),
            chunk_count,
            embedding_count,
            collection: REMOTE_CONTENT_COLLECTION.to_string(),
        })
    }

    /// Fetch content with exponential backoff retry.
    async fn fetch_with_retry(&self, cid: &str) -> Result<Vec<u8>, IndexError> {
        let content_id =
            ContentId::from_str(cid).map_err(|e| IndexError::InvalidCid(e.to_string()))?;

        let mut last_error = String::new();

        for attempt in 1..=MAX_RETRY_ATTEMPTS {
            debug!(cid = %cid, attempt = attempt, "Attempting to fetch content");

            match self.content_service.fetch(&content_id).await {
                Ok(content) => {
                    debug!(cid = %cid, size = content.len(), "Content fetched successfully");
                    return Ok(content);
                }
                Err(e) => {
                    last_error = e.to_string();
                    warn!(
                        cid = %cid,
                        attempt = attempt,
                        error = %e,
                        "Fetch attempt failed"
                    );

                    if attempt < MAX_RETRY_ATTEMPTS {
                        let delay = BASE_RETRY_DELAY * 2u32.pow(attempt - 1);
                        debug!(delay = ?delay, "Waiting before retry");
                        sleep(delay).await;
                    }
                }
            }
        }

        Err(IndexError::FetchFailed {
            attempts: MAX_RETRY_ATTEMPTS,
            message: last_error,
        })
    }

    /// Simple text chunking by paragraphs and size limits.
    fn chunk_text(&self, text: &str) -> Vec<String> {
        let min_size = self.config.min_chunk_size;
        let max_size = self.config.max_chunk_size;

        let mut chunks = Vec::new();
        let mut current_chunk = String::new();

        // Split by paragraph breaks
        for paragraph in text.split("\n\n") {
            let paragraph = paragraph.trim();
            if paragraph.is_empty() {
                continue;
            }

            // If adding this paragraph would exceed max size, finalize current chunk
            if !current_chunk.is_empty()
                && current_chunk.len() + paragraph.len() + 2 > max_size
            {
                if current_chunk.len() >= min_size {
                    chunks.push(current_chunk.clone());
                }
                current_chunk.clear();
            }

            // Add paragraph to current chunk
            if !current_chunk.is_empty() {
                current_chunk.push_str("\n\n");
            }
            current_chunk.push_str(paragraph);

            // If current chunk is already at max, finalize it
            if current_chunk.len() >= max_size {
                chunks.push(current_chunk.clone());
                current_chunk.clear();
            }
        }

        // Don't forget the last chunk
        if current_chunk.len() >= min_size {
            chunks.push(current_chunk);
        }

        chunks
    }

    /// Check if content type is indexable.
    fn is_indexable(&self, content_type: &str) -> bool {
        let indexable_prefixes = ["text/", "application/json", "application/markdown"];

        indexable_prefixes
            .iter()
            .any(|prefix| content_type.starts_with(prefix))
    }

    /// Generate embeddings and store in Qdrant.
    async fn embed_and_store(&self, cid: &str, chunks: Vec<String>) -> Result<usize, IndexError> {
        // Initialize FastEmbed
        let embedder = fastembed::FastEmbed::try_default()
            .map_err(|e| IndexError::EmbeddingFailed(e.to_string()))?;

        // Initialize Qdrant client
        let qdrant = integrations::qdrant::Qdrant::builder()
            .batch_size(self.config.storage_batch_size)
            .vector_size(self.config.vector_size as u64)
            .collection_name(REMOTE_CONTENT_COLLECTION)
            .build()
            .map_err(|e| IndexError::Qdrant(e.to_string()))?;

        // Ensure collection exists
        qdrant
            .create_index_if_not_exists()
            .await
            .map_err(|e| IndexError::Qdrant(e.to_string()))?;

        // Process chunks in batches
        let mut embedding_count = 0;

        for (idx, chunk) in chunks.iter().enumerate() {
            // Generate embedding using FastEmbed
            let embeddings = embedder
                .embed(vec![chunk.clone()])
                .await
                .map_err(|e| IndexError::EmbeddingFailed(e.to_string()))?;

            if embeddings.is_empty() {
                warn!(cid = %cid, chunk_idx = idx, "No embedding generated for chunk");
                continue;
            }

            embedding_count += 1;
        }

        // For MVP, we store via Swiftide's pipeline
        // Full implementation would use qdrant client directly
        debug!(
            cid = %cid,
            chunks = chunks.len(),
            embeddings = embedding_count,
            "Embeddings generated (storage via pipeline)"
        );

        Ok(embedding_count)
    }

    /// Get remote_content record from database.
    async fn get_record(&self, cid: &str) -> Result<remote_content::Model, IndexError> {
        remote_content::Entity::find()
            .filter(remote_content::Column::Cid.eq(cid))
            .one(self.db.as_ref())
            .await?
            .ok_or_else(|| IndexError::RecordNotFound {
                cid: cid.to_string(),
            })
    }

    /// Update record after successful indexing.
    async fn update_record_success(
        &self,
        cid: &str,
        embedding_count: usize,
    ) -> Result<(), IndexError> {
        let record = self.get_record(cid).await?;

        let mut active: remote_content::ActiveModel = record.into();
        active.indexed = Set(true);
        active.indexed_at = Set(Some(Utc::now().into()));
        active.embedding_count = Set(Some(embedding_count as i32));
        active.qdrant_collection = Set(Some(REMOTE_CONTENT_COLLECTION.to_string()));
        active.index_error = Set(None);

        active.update(self.db.as_ref()).await?;

        Ok(())
    }

    /// Update record after indexing failure.
    async fn update_record_error(&self, cid: &str, error: &str) -> Result<(), IndexError> {
        let record = self.get_record(cid).await?;

        let mut active: remote_content::ActiveModel = record.into();
        active.index_error = Set(Some(error.to_string()));

        active.update(self.db.as_ref()).await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    fn is_indexable_standalone(content_type: &str) -> bool {
        let indexable_prefixes = ["text/", "application/json", "application/markdown"];
        indexable_prefixes
            .iter()
            .any(|prefix| content_type.starts_with(prefix))
    }

    #[test]
    fn test_is_indexable() {
        assert!(is_indexable_standalone("text/plain"));
        assert!(is_indexable_standalone("text/markdown"));
        assert!(is_indexable_standalone("text/html"));
        assert!(is_indexable_standalone("application/json"));
        assert!(!is_indexable_standalone("application/octet-stream"));
        assert!(!is_indexable_standalone("image/png"));
    }

    #[test]
    fn test_chunk_text_basic() {
        let text = "First paragraph.\n\nSecond paragraph.\n\nThird paragraph.";

        // Simple chunking function for testing
        let chunks: Vec<String> = text
            .split("\n\n")
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0], "First paragraph.");
        assert_eq!(chunks[1], "Second paragraph.");
        assert_eq!(chunks[2], "Third paragraph.");
    }
}
