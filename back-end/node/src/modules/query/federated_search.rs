//! FederatedSearch for searching across local and remote content.
//!
//! This module provides unified search across:
//! - Local space collections (space-{key}-idx)
//! - Remote content collection (remote-content-idx)
//!
//! Remote results receive a 0.9x score penalty to prefer local content
//! when relevance is similar.

use std::sync::Arc;

use swiftide::integrations::qdrant::qdrant_client;
use swiftide::traits::EmbeddingModel;
use swiftide_integrations::fastembed;
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

// Re-export qdrant types for search
use qdrant_client::qdrant::SearchPointsBuilder;

use super::search::SearchResult;
use crate::modules::ai::config::IndexingConfig;
use crate::modules::ai::remote_indexer::REMOTE_CONTENT_COLLECTION;

/// Score penalty applied to remote results (0.9 = 10% reduction).
const REMOTE_SCORE_PENALTY: f32 = 0.9;

/// Errors that can occur during federated search.
#[derive(Error, Debug)]
pub enum FederatedSearchError {
    #[error("embedding failed: {0}")]
    Embedding(String),

    #[error("qdrant error: {0}")]
    Qdrant(String),

    #[error("no collections available")]
    NoCollections,
}

/// Search scope for federated queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchScope {
    /// Search only local space collections.
    Local,

    /// Search only remote content collection.
    Remote,

    /// Search both local and remote (default).
    All,
}

impl Default for SearchScope {
    fn default() -> Self {
        Self::All
    }
}

/// Source of a search result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultSource {
    /// Result from local space collection.
    Local,

    /// Result from remote content collection.
    Remote,
}

/// Extended search result with source information.
#[derive(Debug, Clone)]
pub struct FederatedResult {
    /// Content identifier.
    pub cid: String,

    /// Similarity score (higher is better, typically 0.0-1.0).
    pub score: f32,

    /// Adjusted score after applying penalties.
    pub adjusted_score: f32,

    /// Source collection name.
    pub collection: String,

    /// Whether this is from local or remote.
    pub source: ResultSource,

    /// Text content snippet (if available).
    pub content: Option<String>,

    /// Chunk index within document.
    pub chunk_index: Option<i32>,
}

/// Aggregated federated search results.
#[derive(Debug, Clone)]
pub struct FederatedResults {
    /// All results, sorted by adjusted score.
    pub results: Vec<FederatedResult>,

    /// Number of local results.
    pub local_count: usize,

    /// Number of remote results.
    pub remote_count: usize,

    /// Collections searched.
    pub collections_searched: Vec<String>,
}

/// Federated search across local and remote content.
pub struct FederatedSearch {
    /// List of local space collection names.
    local_collections: Arc<RwLock<Vec<String>>>,

    /// Indexing configuration (for Qdrant connection).
    config: IndexingConfig,
}

impl std::fmt::Debug for FederatedSearch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FederatedSearch")
            .field("config.qdrant_url", &self.config.qdrant_url)
            .finish_non_exhaustive()
    }
}

impl FederatedSearch {
    /// Create a new federated search instance.
    pub fn new(config: IndexingConfig) -> Self {
        Self {
            local_collections: Arc::new(RwLock::new(Vec::new())),
            config,
        }
    }

    /// Register a local space collection for searching.
    pub async fn add_local_collection(&self, collection_name: String) {
        let mut collections = self.local_collections.write().await;
        if !collections.contains(&collection_name) {
            collections.push(collection_name);
        }
    }

    /// Remove a local space collection.
    pub async fn remove_local_collection(&self, collection_name: &str) {
        let mut collections = self.local_collections.write().await;
        collections.retain(|c| c != collection_name);
    }

    /// Get list of local collections.
    pub async fn local_collections(&self) -> Vec<String> {
        self.local_collections.read().await.clone()
    }

    /// Discover local space collections from Qdrant.
    ///
    /// Queries Qdrant for collections matching the pattern `space-*-idx`
    /// and registers them for searching.
    pub async fn discover_collections(&self) -> Result<Vec<String>, FederatedSearchError> {
        let client = self.create_qdrant_client()?;

        // List all collections
        let collections = client
            .list_collections()
            .await
            .map_err(|e| FederatedSearchError::Qdrant(e.to_string()))?;

        // Filter for space collections (space-*-idx pattern)
        let space_collections: Vec<String> = collections
            .collections
            .iter()
            .filter_map(|c| {
                if c.name.starts_with("space-") && c.name.ends_with("-idx") {
                    Some(c.name.clone())
                } else {
                    None
                }
            })
            .collect();

        // Register discovered collections
        {
            let mut local_cols = self.local_collections.write().await;
            for col in &space_collections {
                if !local_cols.contains(col) {
                    local_cols.push(col.clone());
                }
            }
        }

        info!(
            count = space_collections.len(),
            collections = ?space_collections,
            "Discovered local space collections"
        );

        Ok(space_collections)
    }

    /// Perform federated search across collections.
    ///
    /// # Arguments
    ///
    /// * `query` - The search query text.
    /// * `scope` - Which collections to search (Local, Remote, or All).
    /// * `limit` - Maximum number of results to return.
    ///
    /// # Returns
    ///
    /// `FederatedResults` containing merged and sorted results.
    pub async fn search(
        &self,
        query: &str,
        scope: SearchScope,
        limit: usize,
    ) -> Result<FederatedResults, FederatedSearchError> {
        info!(query = %query, ?scope, limit = limit, "Starting federated search");

        // Generate query embedding
        let embedder = fastembed::FastEmbed::try_default()
            .map_err(|e| FederatedSearchError::Embedding(e.to_string()))?;

        let embeddings = embedder
            .embed(vec![query.to_string()])
            .await
            .map_err(|e| FederatedSearchError::Embedding(e.to_string()))?;

        if embeddings.is_empty() {
            return Err(FederatedSearchError::Embedding(
                "No embedding generated".to_string(),
            ));
        }

        let query_embedding = &embeddings[0];
        let mut all_results = Vec::new();
        let mut collections_searched = Vec::new();

        // Search local collections
        if scope == SearchScope::Local || scope == SearchScope::All {
            // Auto-discover collections if none registered
            {
                let local_cols = self.local_collections.read().await;
                if local_cols.is_empty() {
                    drop(local_cols); // Release read lock before discover
                    if let Err(e) = self.discover_collections().await {
                        warn!(error = %e, "Failed to auto-discover collections");
                    }
                }
            }

            let local_cols = self.local_collections.read().await;

            for collection in local_cols.iter() {
                match self
                    .search_collection(collection, query_embedding, limit)
                    .await
                {
                    Ok(results) => {
                        collections_searched.push(collection.clone());
                        for (cid, score, content, chunk_idx) in results {
                            all_results.push(FederatedResult {
                                cid,
                                score,
                                adjusted_score: score, // No penalty for local
                                collection: collection.clone(),
                                source: ResultSource::Local,
                                content,
                                chunk_index: chunk_idx,
                            });
                        }
                    }
                    Err(e) => {
                        warn!(collection = %collection, error = %e, "Failed to search local collection");
                    }
                }
            }
        }

        // Search remote collection
        if scope == SearchScope::Remote || scope == SearchScope::All {
            match self
                .search_collection(REMOTE_CONTENT_COLLECTION, query_embedding, limit)
                .await
            {
                Ok(results) => {
                    collections_searched.push(REMOTE_CONTENT_COLLECTION.to_string());
                    for (cid, score, content, chunk_idx) in results {
                        all_results.push(FederatedResult {
                            cid,
                            score,
                            adjusted_score: score * REMOTE_SCORE_PENALTY, // Apply penalty
                            collection: REMOTE_CONTENT_COLLECTION.to_string(),
                            source: ResultSource::Remote,
                            content,
                            chunk_index: chunk_idx,
                        });
                    }
                }
                Err(e) => {
                    warn!(
                        collection = REMOTE_CONTENT_COLLECTION,
                        error = %e,
                        "Failed to search remote collection"
                    );
                }
            }
        }

        // Sort by adjusted score (descending - higher is better)
        all_results.sort_by(|a, b| {
            b.adjusted_score
                .partial_cmp(&a.adjusted_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Truncate to limit
        all_results.truncate(limit);

        // Count by source
        let local_count = all_results
            .iter()
            .filter(|r| r.source == ResultSource::Local)
            .count();
        let remote_count = all_results
            .iter()
            .filter(|r| r.source == ResultSource::Remote)
            .count();

        info!(
            total = all_results.len(),
            local = local_count,
            remote = remote_count,
            "Federated search complete"
        );

        Ok(FederatedResults {
            results: all_results,
            local_count,
            remote_count,
            collections_searched,
        })
    }

    /// Search a single Qdrant collection.
    async fn search_collection(
        &self,
        collection: &str,
        query_embedding: &[f32],
        limit: usize,
    ) -> Result<Vec<(String, f32, Option<String>, Option<i32>)>, FederatedSearchError> {
        debug!(collection = %collection, "Searching collection");

        // Build Qdrant client (with or without API key)
        let client = self.create_qdrant_client()?;

        // Check if collection exists
        match client.collection_exists(collection).await {
            Ok(false) => {
                debug!(collection = %collection, "Collection does not exist");
                return Ok(Vec::new());
            }
            Err(e) => {
                debug!(collection = %collection, error = %e, "Failed to check collection existence");
                return Ok(Vec::new());
            }
            Ok(true) => {}
        }

        // Perform vector search using qdrant_client types
        let search_request = SearchPointsBuilder::new(
            collection,
            query_embedding.to_vec(),
            limit as u64,
        )
        .with_payload(true)
        .build();

        let search_result = client
            .search_points(search_request)
            .await
            .map_err(|e| FederatedSearchError::Qdrant(e.to_string()))?;

        // Extract results
        let mut results = Vec::new();
        for point in search_result.result {
            // Get CID from payload (stored as "path" in swiftide)
            let cid = point
                .payload
                .get("path")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("point-{:?}", point.id));

            // Get content snippet from payload
            let content = point
                .payload
                .get("content")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            // Get chunk index if available
            let chunk_idx = point
                .payload
                .get("chunk_index")
                .and_then(|v| v.as_integer())
                .map(|i| i as i32);

            results.push((cid, point.score, content, chunk_idx));
        }

        debug!(collection = %collection, count = results.len(), "Collection search complete");
        Ok(results)
    }

    /// Create a Qdrant client, respecting the skip_api_key config.
    fn create_qdrant_client(&self) -> Result<qdrant_client::Qdrant, FederatedSearchError> {
        qdrant_client::Qdrant::from_url(&self.config.qdrant_url)
            .build()
            .map_err(|e| FederatedSearchError::Qdrant(e.to_string()))
    }

    /// Convert federated results to standard SearchResult format.
    pub fn to_search_results(results: &FederatedResults) -> Vec<SearchResult> {
        results
            .results
            .iter()
            .map(|r| SearchResult {
                cid: r.cid.clone(),
                score: r.adjusted_score,
                content: r.content.clone(),
                document_id: None,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_scope_default() {
        let scope = SearchScope::default();
        assert_eq!(scope, SearchScope::All);
    }

    #[test]
    fn test_remote_penalty() {
        let original_score = 0.95;
        let penalized = original_score * REMOTE_SCORE_PENALTY;

        assert!(penalized < original_score);
        assert!((penalized - 0.855).abs() < 0.001);
    }

    #[test]
    fn test_result_sorting() {
        let mut results = vec![
            FederatedResult {
                cid: "cid1".to_string(),
                score: 0.8,
                adjusted_score: 0.8,
                collection: "local".to_string(),
                source: ResultSource::Local,
                content: None,
                chunk_index: None,
            },
            FederatedResult {
                cid: "cid2".to_string(),
                score: 0.95,
                adjusted_score: 0.95 * REMOTE_SCORE_PENALTY, // 0.855
                collection: "remote".to_string(),
                source: ResultSource::Remote,
                content: None,
                chunk_index: None,
            },
            FederatedResult {
                cid: "cid3".to_string(),
                score: 0.9,
                adjusted_score: 0.9,
                collection: "local".to_string(),
                source: ResultSource::Local,
                content: None,
                chunk_index: None,
            },
        ];

        // Sort by adjusted score descending
        results.sort_by(|a, b| {
            b.adjusted_score
                .partial_cmp(&a.adjusted_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Local 0.9 should be first (highest adjusted score)
        assert_eq!(results[0].cid, "cid3");
        assert_eq!(results[0].adjusted_score, 0.9);

        // Remote 0.855 should be second
        assert_eq!(results[1].cid, "cid2");

        // Local 0.8 should be third
        assert_eq!(results[2].cid, "cid1");
    }
}
