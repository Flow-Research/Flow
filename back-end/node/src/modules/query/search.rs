use std::sync::Arc;

use sea_orm::DatabaseConnection;
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::debug;

use crate::modules::knowledge::VectorIndex;

#[derive(Debug, Error)]
pub enum SearchError {
    #[error("Vector error: {0}")]
    Vector(#[from] crate::modules::knowledge::vector::VectorError),
    #[error("Embedding error: {0}")]
    Embedding(String),
    #[error("Database error: {0}")]
    Database(#[from] sea_orm::DbErr),
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub cid: String,
    pub score: f32,
    pub content: Option<String>,
    pub document_id: Option<i32>,
}

pub struct SemanticSearch {
    vector_index: Arc<RwLock<VectorIndex>>,
    _db: Arc<DatabaseConnection>,
    embed_fn: Arc<dyn Fn(&str) -> Vec<f32> + Send + Sync>,
}

impl SemanticSearch {
    pub fn new(
        vector_index: Arc<RwLock<VectorIndex>>,
        db: Arc<DatabaseConnection>,
        embed_fn: impl Fn(&str) -> Vec<f32> + Send + Sync + 'static,
    ) -> Self {
        Self {
            vector_index,
            _db: db,
            embed_fn: Arc::new(embed_fn),
        }
    }

    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>, SearchError> {
        let query_embedding = (self.embed_fn)(query);

        let index = self.vector_index.read().await;
        let results = index.search(&query_embedding, limit)?;

        debug!("Semantic search returned {} results", results.len());

        Ok(results
            .into_iter()
            .map(|(cid, score)| SearchResult {
                cid,
                score,
                content: None,
                document_id: None,
            })
            .collect())
    }

    pub async fn search_with_threshold(
        &self,
        query: &str,
        limit: usize,
        threshold: f32,
    ) -> Result<Vec<SearchResult>, SearchError> {
        let results = self.search(query, limit * 2).await?;

        Ok(results
            .into_iter()
            .filter(|r| r.score <= threshold)
            .take(limit)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn mock_embed(_text: &str) -> Vec<f32> {
        vec![0.1; 768]
    }

    #[tokio::test]
    async fn test_semantic_search_empty_index() {
        let temp_dir = TempDir::new().unwrap();
        let index = VectorIndex::new(temp_dir.path(), 768);
        let index = Arc::new(RwLock::new(index));

        let db = sea_orm::Database::connect("sqlite::memory:").await.unwrap();
        let db = Arc::new(db);

        let search = SemanticSearch::new(index, db, mock_embed);
        let results = search.search("test query", 10).await.unwrap();

        assert!(results.is_empty());
    }
}
