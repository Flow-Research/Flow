use std::sync::Arc;

use sea_orm::DatabaseConnection;
use swiftide::integrations;
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{debug, info};

use crate::modules::knowledge::{EntityStore, VectorIndex};

use super::search::{SearchResult, SemanticSearch};

#[derive(Debug, Error)]
pub enum QueryError {
    #[error("Search error: {0}")]
    Search(#[from] super::search::SearchError),
    #[error("LLM error: {0}")]
    Llm(String),
    #[error("No relevant context found")]
    NoContext,
}

#[derive(Debug, Clone)]
pub struct QueryResult {
    pub answer: String,
    pub sources: Vec<SourceCitation>,
    pub entities: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SourceCitation {
    pub cid: String,
    pub score: f32,
    pub snippet: Option<String>,
}

pub struct QueryPipeline {
    semantic_search: SemanticSearch,
    _entity_store: Arc<EntityStore>,
    llm_model: String,
    _space_id: i32,
}

impl QueryPipeline {
    pub fn new(
        vector_index: Arc<RwLock<VectorIndex>>,
        db: Arc<DatabaseConnection>,
        entity_store: Arc<EntityStore>,
        space_id: i32,
        embed_fn: impl Fn(&str) -> Vec<f32> + Send + Sync + 'static,
    ) -> Self {
        Self {
            semantic_search: SemanticSearch::new(vector_index, db, embed_fn),
            _entity_store: entity_store,
            llm_model: "llama3.1:8b".to_string(),
            _space_id: space_id,
        }
    }

    pub fn with_model(mut self, model: &str) -> Self {
        self.llm_model = model.to_string();
        self
    }

    pub async fn query(&self, question: &str) -> Result<QueryResult, QueryError> {
        info!("Processing query: {}", question);

        let relevant_chunks = self.semantic_search.search(question, 10).await?;

        if relevant_chunks.is_empty() {
            return Err(QueryError::NoContext);
        }

        debug!("Found {} relevant chunks", relevant_chunks.len());

        let context = self.build_context(&relevant_chunks);
        let answer = self.generate_answer(question, &context).await?;

        let sources = relevant_chunks
            .iter()
            .take(5)
            .map(|r| SourceCitation {
                cid: r.cid.clone(),
                score: r.score,
                snippet: r.content.clone(),
            })
            .collect();

        Ok(QueryResult {
            answer,
            sources,
            entities: Vec::new(),
        })
    }

    fn build_context(&self, chunks: &[SearchResult]) -> String {
        chunks
            .iter()
            .filter_map(|c| c.content.as_ref())
            .take(5)
            .enumerate()
            .map(|(i, content)| format!("[Source {}]\n{}\n", i + 1, content))
            .collect::<Vec<_>>()
            .join("\n")
    }

    async fn generate_answer(&self, question: &str, context: &str) -> Result<String, QueryError> {
        let ollama = integrations::ollama::Ollama::default()
            .with_default_prompt_model(&self.llm_model)
            .to_owned();

        let prompt = format!(
            r#"Answer the question based on the provided context.
If the answer is not in the context, say "I don't have enough information to answer that."
Be concise and cite sources when possible using [Source N] format.

Context:
{}

Question: {}

Answer:"#,
            context, question
        );

        use swiftide::traits::SimplePrompt;
        let response = ollama
            .prompt(prompt.into())
            .await
            .map_err(|e| QueryError::Llm(e.to_string()))?;

        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::knowledge::vector::VectorIndex;
    use tempfile::TempDir;

    fn mock_embed(_text: &str) -> Vec<f32> {
        vec![0.1; 768]
    }

    #[tokio::test]
    async fn test_query_no_context() {
        let temp_dir = TempDir::new().unwrap();
        let index = VectorIndex::new(temp_dir.path(), 768);
        let index = Arc::new(RwLock::new(index));

        let db = sea_orm::Database::connect("sqlite::memory:").await.unwrap();
        let db = Arc::new(db);

        let entity_store = Arc::new(EntityStore::new(
            sea_orm::Database::connect("sqlite::memory:").await.unwrap(),
        ));

        let pipeline = QueryPipeline::new(index, db, entity_store, 1, mock_embed);

        let result = pipeline.query("test question").await;
        assert!(matches!(result, Err(QueryError::NoContext)));
    }
}
