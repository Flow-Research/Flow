pub mod index;

use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, atomic::Ordering},
    time::Duration,
};

use anyhow::Result;
use entity::{prelude::SpaceIndexStatus, space, space_index_status};
use errors::AppError;
use index::metrics::PipelineMetrics;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
};
use swiftide::{
    indexing::{
        self, IndexingStream, Node,
        transformers::{
            ChunkMarkdown, ChunkText, Embed, MetadataKeywords, MetadataQAText, MetadataSummary,
        },
    },
    integrations,
    query::{
        self, answers, query_transformers, search_strategies::SimilaritySingleEmbedding,
        states::Answered,
    },
    traits::ChunkerTransformer,
};
use swiftide_integrations::{fastembed, qdrant::Qdrant};
use tokio::sync::RwLock;

use index::config::IndexingConfig;
use tracing::{debug, error, info, warn};

pub type PipelineHandle = String;

#[derive(Clone)]
pub struct Pipeline {
    pub space_id: i32,
    pub space_key: String,
    pub location: String,
    pub last_indexed: Option<chrono::DateTime<chrono::Utc>>,
    pub indexing_in_progress: bool,
    pub config: IndexingConfig,
    pub metrics: PipelineMetrics,
}

impl Pipeline {
    pub async fn build_indexing_pipeline(&self) -> Result<indexing::Pipeline<String>> {
        let collection_name = format!("space-{}-idx", &self.space_key);

        // Initialize OpenAI client with retry logic
        let ollama_client = integrations::ollama::Ollama::default()
            .with_default_prompt_model("llama3.1:8b")
            .to_owned();

        // Wrap with backoff for rate limit handling
        let ollama_with_backoff =
            indexing::LanguageModelWithBackOff::new(ollama_client.clone(), Default::default());

        // Initialize Qdrant storage
        let qdrant = integrations::qdrant::Qdrant::builder()
            .batch_size(self.config.storage_batch_size)
            .vector_size(self.config.vector_size as u64)
            .collection_name(collection_name.clone())
            .build()?;

        // Initialize file loader
        let mut loader = indexing::loaders::FileLoader::new(&self.location);

        if let Some(extensions) = &self.config.allowed_extensions {
            loader =
                loader.with_extensions(&extensions.iter().map(|s| s.as_str()).collect::<Vec<_>>());
        }

        // Start building the pipeline
        let mut pipeline =
            indexing::Pipeline::from_loader(loader).with_concurrency(self.config.concurrency);

        // Add caching if Redis is configured
        if let Some(redis_url) = &self.config.redis_url {
            match integrations::redis::Redis::try_from_url(
                redis_url.clone(),
                collection_name.clone(),
            ) {
                Ok(cache) => {
                    info!("Redis caching enabled");
                    pipeline = pipeline.filter_cached(cache);
                }
                Err(e) => {
                    warn!(
                        "Failed to initialize Redis cache, continuing without it: {}",
                        e
                    );
                }
            }
        }

        // Apply filters and validation
        let config_clone = self.config.clone();
        let metrics_clone = self.metrics.clone();
        pipeline = pipeline
            .filter(move |result| {
                let should_process = match result {
                    Ok(node) => {
                        // Check file size
                        if node.original_size > config_clone.max_file_size {
                            warn!(
                                "Skipping large file: {:?} ({} bytes > {} max)",
                                node.path, node.original_size, config_clone.max_file_size
                            );
                            metrics_clone.files_skipped.fetch_add(1, Ordering::Relaxed);
                            return false;
                        }

                        // Check if file is likely binary
                        if is_likely_binary(&node.chunk) {
                            warn!("Skipping likely binary file: {:?}", node.path);
                            metrics_clone.files_skipped.fetch_add(1, Ordering::Relaxed);
                            return false;
                        }

                        // Check exclude patterns
                        let path_str = &node.path.to_string_lossy();
                        for pattern in &config_clone.exclude_patterns {
                            if path_str.contains(pattern) {
                                debug!("Skipping excluded path: {:?}", &node.path);
                                metrics_clone.files_skipped.fetch_add(1, Ordering::Relaxed);
                                return false;
                            }
                        }

                        // Check if content is empty or too small
                        let trimmed = node.chunk.trim();
                        if trimmed.is_empty() || trimmed.len() < config_clone.min_chunk_size {
                            debug!("Skipping empty/small file: {:?}", node.path);
                            metrics_clone.files_skipped.fetch_add(1, Ordering::Relaxed);
                            return false;
                        }

                        metrics_clone.files_loaded.fetch_add(1, Ordering::Relaxed);
                        true
                    }
                    Err(e) => {
                        error!("Error loading file: {:#}", e);
                        metrics_clone.files_failed.fetch_add(1, Ordering::Relaxed);
                        false // Skip errors but continue processing
                    }
                };
                should_process
            })
            .log_errors();

        // Add rate limiting
        if self.config.rate_limit_ms > 0 {
            pipeline = pipeline.throttle(Duration::from_millis(self.config.rate_limit_ms));
        }

        // Apply smart chunking
        let chunker = SmartChunker::new(self.config.min_chunk_size..self.config.max_chunk_size);
        pipeline = pipeline.then_chunk(chunker);

        // Filter out failed chunks
        let metrics_clone = self.metrics.clone();
        pipeline = pipeline.filter(move |result| {
            match result {
                Ok(node) => {
                    if node.chunk.trim().is_empty() {
                        debug!("Filtering empty chunk from {:?}", node.path);
                        false
                    } else {
                        metrics_clone.chunks_created.fetch_add(1, Ordering::Relaxed);
                        true
                    }
                }
                Err(e) => {
                    warn!("Chunk creation failed: {:#}", e);
                    false // Don't fail the whole pipeline
                }
            }
        });

        // Add metadata generation
        if self.config.enable_metadata_qa {
            info!("Enabling Q&A metadata generation");
            pipeline = pipeline.then(MetadataQAText::new(ollama_with_backoff.clone()));
        }

        if self.config.enable_metadata_summary {
            info!("Enabling summary metadata generation");
            pipeline = pipeline.then(MetadataSummary::new(ollama_with_backoff.clone()));
        }

        if self.config.enable_metadata_keywords {
            info!("Enabling keyword metadata generation");
            pipeline = pipeline.then(MetadataKeywords::new(ollama_with_backoff.clone()));
        }

        // Embed in batches
        pipeline = pipeline.then_in_batch(
            Embed::new(ollama_with_backoff).with_batch_size(self.config.embed_batch_size),
        );

        // Count stored chunks
        let metrics_clone = self.metrics.clone();
        pipeline = pipeline.log_nodes().filter(move |result| {
            if result.is_ok() {
                metrics_clone.chunks_stored.fetch_add(1, Ordering::Relaxed);
            }
            true
        });

        // Store in Qdrant
        pipeline = pipeline.then_store_with(qdrant);

        // Run the pipeline
        Ok(pipeline)
    }

    pub async fn build_query_pipeline(
        &self,
    ) -> Result<query::Pipeline<'static, SimilaritySingleEmbedding, Answered>> {
        let ollama_client = integrations::ollama::Ollama::default()
            .with_default_prompt_model("llama3.1:8b")
            .to_owned();

        let fastembed = integrations::fastembed::FastEmbed::try_default()?;
        let reranker = fastembed::Rerank::builder().top_k(5).build()?;

        let collection_name = format!("space-{}-idx", &self.space_key);

        let qdrant = Qdrant::builder()
            .batch_size(50)
            .vector_size(384)
            .collection_name(collection_name)
            .build()?;

        // Build query pipeline with the stored config
        Ok(query::Pipeline::default()
            .then_transform_query(query_transformers::GenerateSubquestions::from_client(
                ollama_client.clone(),
            ))
            .then_transform_query(query_transformers::Embed::from_client(fastembed))
            .then_retrieve(qdrant.clone())
            .then_transform_response(reranker)
            .then_answer(answers::Simple::from_client(ollama_client.clone())))
    }
}

fn is_likely_binary(content: &str) -> bool {
    // Check for null bytes
    if content.contains('\0') {
        return true;
    }

    // Check ratio of non-printable characters
    let total_chars = content.chars().count();
    if total_chars == 0 {
        return false;
    }

    let non_printable = content
        .chars()
        .filter(|c| {
            !c.is_ascii_graphic() && !c.is_whitespace() && *c != '\n' && *c != '\r' && *c != '\t'
        })
        .count();

    // If more than 30% non-printable, likely binary
    (non_printable as f64 / total_chars as f64) > 0.3
}

#[derive(Clone)]
struct SmartChunker {
    chunk_range: std::ops::Range<usize>,
}

impl SmartChunker {
    fn new(chunk_range: std::ops::Range<usize>) -> Self {
        Self { chunk_range }
    }

    /// Detect language from file extension
    fn detect_language(path: &Path) -> Option<&'static str> {
        path.extension()
            .and_then(|ext| ext.to_str())
            .and_then(|ext| {
                let ext_lower = ext.to_lowercase();
                match ext_lower.as_str() {
                    "rs" => Some("rust"),
                    "py" => Some("python"),
                    "js" | "jsx" | "mjs" => Some("javascript"),
                    "ts" | "tsx" => Some("typescript"),
                    "java" => Some("java"),
                    "go" => Some("go"),
                    "rb" => Some("ruby"),
                    "cs" => Some("csharp"),
                    "c" | "h" => Some("c"),
                    "cpp" | "cc" | "cxx" | "hpp" | "hxx" => Some("cpp"),
                    "ex" | "exs" => Some("elixir"),
                    "html" | "htm" => Some("html"),
                    "php" => Some("php"),
                    "sol" => Some("solidity"),
                    _ => None,
                }
            })
    }

    /// Check if file is markdown
    fn is_markdown(path: &Path) -> bool {
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| {
                let ext_lower = ext.to_lowercase();
                matches!(ext_lower.as_str(), "md" | "markdown" | "mdx")
            })
            .unwrap_or(false)
    }
}

impl std::fmt::Debug for SmartChunker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SmartChunker")
            .field("chunk_range", &self.chunk_range)
            .finish()
    }
}

#[async_trait::async_trait]
impl ChunkerTransformer for SmartChunker {
    type Input = String;
    type Output = String;

    async fn transform_node(&self, node: Node<String>) -> IndexingStream<String> {
        let path = &node.path;

        if path.as_os_str().is_empty() {
            debug!("No path for node, using text chunker");
            return ChunkText::from_chunk_range(self.chunk_range.clone())
                .transform_node(node)
                .await;
        }

        // Try code chunking first
        if let Some(language) = Self::detect_language(&path) {
            debug!("Attempting to chunk {:?} as {} code", path, language);

            match integrations::treesitter::transformers::ChunkCode::try_for_language_and_chunk_size(
                language,
                self.chunk_range.clone(),
            ) {
                Ok(chunker) => {
                    match tokio::time::timeout(
                        Duration::from_secs(30),
                        chunker.transform_node(node.clone()),
                    )
                    .await
                    {
                        Ok(stream) => {
                            debug!("Successfully chunked {:?} as {}", path, language);
                            return stream;
                        }
                        Err(_) => {
                            warn!(
                                "Timeout chunking {:?} as {}, falling back to text",
                                path, language
                            );
                        }
                    }
                }
                Err(e) => {
                    warn!(
                        "Failed to create code chunker for {} on {:?}: {}",
                        language, path, e
                    );
                }
            }
        }

        // Try markdown chunking
        if Self::is_markdown(&path) {
            debug!("Chunking {:?} as markdown", path);
            return ChunkMarkdown::from_chunk_range(self.chunk_range.clone())
                .transform_node(node)
                .await;
        }

        // Fall back to text chunking
        debug!("Chunking {:?} as plain text", path);
        ChunkText::from_chunk_range(self.chunk_range.clone())
            .transform_node(node)
            .await
    }
}

#[derive(Clone)]
pub struct PipelineManager {
    pipelines: Arc<RwLock<HashMap<String, Pipeline>>>,
    db: DatabaseConnection,
    config: IndexingConfig,
}

impl PipelineManager {
    pub fn new(db: DatabaseConnection, config: IndexingConfig) -> Self {
        Self {
            pipelines: Arc::new(RwLock::new(HashMap::new())),
            db,
            config,
        }
    }

    /// Initialize all spaces from database on startup
    pub async fn initialize_from_database(&self) -> std::result::Result<(), AppError> {
        info!("Loading all spaces from database...");

        let spaces = space::Entity::find()
            .all(&self.db)
            .await
            .map_err(|e| AppError::Storage(Box::new(e)))?;

        info!("Found {} spaces to initialize", spaces.len());

        for space_model in spaces {
            let index_status = SpaceIndexStatus::find()
                .filter(space_index_status::Column::SpaceId.eq(space_model.id))
                .one(&self.db)
                .await
                .map_err(|e| AppError::Storage(Box::new(e)))?;

            self.initialize_space_pipeline(
                space_model.id,
                &space_model.key,
                &space_model.location,
                index_status,
            )
            .await?;

            let manager = self.clone();
            let space_key = space_model.key.clone();
            tokio::spawn(async move {
                if let Err(e) = manager.index_space(&space_key).await {
                    error!("Failed to auto-index space {}: {}", space_key, e);
                }
            });
        }

        Ok(())
    }

    /// Initialize pipeline for a specific space
    pub async fn initialize_space_pipeline(
        &self,
        space_id: i32,
        space_key: &str,
        location: &str,
        index_status: Option<space_index_status::Model>,
    ) -> Result<(), AppError> {
        info!(
            "Initializing pipeline for space: {} at {}",
            space_key, location
        );

        let index_status = if let Some(status) = index_status {
            status
        } else {
            info!(
                "No index status found for space {}, creating initial record",
                space_key
            );

            let new_status = space_index_status::ActiveModel {
                space_id: Set(space_id),
                last_indexed: Set(None),
                indexing_in_progress: Set(Some(false)),
                files_indexed: Set(Some(0)),
                chunks_stored: Set(Some(0)),
                ..Default::default()
            };

            new_status
                .insert(&self.db)
                .await
                .map_err(|e| AppError::Storage(Box::new(e)))?
        };

        let space_pipeline = Pipeline {
            space_id,
            space_key: space_key.to_string(),
            location: location.to_string(),
            last_indexed: index_status
                .last_indexed
                .map(|dt| dt.with_timezone(&chrono::Utc)),
            indexing_in_progress: false,
            config: self.config.clone(),
            metrics: PipelineMetrics::from_model(&index_status),
        };

        self.pipelines
            .write()
            .await
            .insert(space_key.to_string(), space_pipeline);

        info!("Pipeline initialized for space: {}", space_key);
        Ok(())
    }

    /// Trigger indexing for a space
    pub async fn index_space(&self, space_key: &str) -> Result<(), AppError> {
        let mut pipelines = self.pipelines.write().await;
        let pipeline = pipelines
            .get_mut(space_key)
            .ok_or_else(|| AppError::NotFound(format!("Space not found: {}", space_key)))?;

        if pipeline.indexing_in_progress {
            return Err(AppError::Conflict(
                "Indexing already in progress".to_string(),
            ));
        }

        pipeline.indexing_in_progress = true;
        let space_key_clone = space_key.to_string();

        drop(pipelines); // Release lock before long operation

        // Run indexing in background
        let manager_clone = self.clone();
        tokio::spawn(async move {
            match manager_clone.run_indexing(&space_key_clone).await {
                Ok(_) => {
                    info!("Indexing completed for space: {}", space_key_clone);
                    let mut pipelines = manager_clone.pipelines.write().await;
                    if let Some(p) = pipelines.get_mut(&space_key_clone) {
                        p.indexing_in_progress = false;
                        p.last_indexed = Some(chrono::Utc::now());
                    }
                }
                Err(e) => {
                    error!("Indexing failed for space {}: {}", space_key_clone, e);
                    let mut pipelines = manager_clone.pipelines.write().await;
                    if let Some(p) = pipelines.get_mut(&space_key_clone) {
                        p.indexing_in_progress = false;
                    }
                }
            }
        });

        Ok(())
    }

    async fn run_indexing(&self, space_key: &str) -> Result<(), AppError> {
        let pipeline = self.build_index_pipeline(space_key).await?;

        // Run the indexing pipeline
        pipeline
            .run()
            .await
            .map_err(|e| AppError::Internal(format!("Indexing failed: {}", e)))?;

        Ok(())
    }

    async fn build_index_pipeline(
        &self,
        space_key: &str,
    ) -> Result<indexing::Pipeline<String>, AppError> {
        let pipelines = self.pipelines.read().await;
        let pipeline = pipelines
            .get(space_key)
            .ok_or_else(|| AppError::NotFound(format!("Space not found: {}", space_key)))?;

        // Build the indexing pipeline using the Pipeline's method
        pipeline
            .build_indexing_pipeline()
            .await
            .map_err(|e| AppError::Internal(format!("Failed to build indexing pipeline: {}", e)))
    }

    async fn build_query_pipeline(
        &self,
        space_key: &str,
    ) -> Result<query::Pipeline<'static, SimilaritySingleEmbedding, Answered>, AppError> {
        let pipelines = self.pipelines.read().await;
        let pipeline = pipelines
            .get(space_key)
            .ok_or_else(|| AppError::NotFound(format!("Space not found: {}", space_key)))?;

        // Build the indexing pipeline using the Pipeline's method
        pipeline
            .build_query_pipeline()
            .await
            .map_err(|e| AppError::Internal(format!("Failed to build indexing pipeline: {}", e)))
    }

    pub async fn query_space(&self, space_key: &str, query: &str) -> Result<String, AppError> {
        // Verify the space exists
        let pipelines = self.pipelines.read().await;
        if !pipelines.contains_key(space_key) {
            return Err(AppError::NotFound(format!(
                "Space not found: {}",
                space_key
            )));
        }
        drop(pipelines);

        // Build a fresh query pipeline (can't be stored due to lifetimes)
        let query_pipeline = self.build_query_pipeline(space_key).await?;

        // Execute the query
        let answer = query_pipeline
            .query(query)
            .await
            .map_err(|e| AppError::Internal(format!("Query failed: {}", e)))?;

        Ok(answer.answer().to_string())
    }
}
