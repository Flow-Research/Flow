pub mod api;
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

use crate::modules::ai::index::metrics::ThresholdStatus;

pub type PipelineHandle = String;

enum ValidationResult {
    Accept,
    Skip(SkipReason),
}

enum SkipReason {
    TooLarge { size: usize, max: usize },
    Binary,
    ExcludedPattern(String),
    TooSmall,
}

impl SkipReason {
    fn log(&self, node: &indexing::Node<String>) {
        match self {
            Self::TooLarge { size, max } => {
                warn!(
                    "Skipping large file: {:?} ({} bytes > {} max)",
                    node.path, size, max
                );
            }
            Self::Binary => {
                warn!("Skipping likely binary file: {:?}", node.path);
            }
            Self::ExcludedPattern(pattern) => {
                debug!(
                    "Skipping excluded path (pattern: {}): {:?}",
                    pattern, node.path
                );
            }
            Self::TooSmall => {
                debug!("Skipping empty/small file: {:?}", node.path);
            }
        }
    }
}

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
    fn llm_client(&self) -> integrations::ollama::Ollama {
        integrations::ollama::Ollama::default()
            .with_default_prompt_model("llama3.1:8b")
            .to_owned()
    }

    fn qdrant(&self, collection_name: String) -> Result<integrations::qdrant::Qdrant> {
        integrations::qdrant::Qdrant::builder()
            .batch_size(self.config.storage_batch_size)
            .vector_size(self.config.vector_size as u64)
            .collection_name(collection_name)
            .build()
    }

    fn collection_name(&self) -> String {
        format!("space-{}-idx", &self.space_key)
    }

    fn validate_file(&self, node: &indexing::Node<String>) -> ValidationResult {
        // Check file size
        if node.original_size > self.config.max_file_size {
            return ValidationResult::Skip(SkipReason::TooLarge {
                size: node.original_size,
                max: self.config.max_file_size,
            });
        }

        // Check if file is likely binary
        if is_likely_binary(&node.chunk) {
            return ValidationResult::Skip(SkipReason::Binary);
        }

        // Check exclude patterns
        let path_str = node.path.to_string_lossy();
        for pattern in &self.config.exclude_patterns {
            if path_str.contains(pattern) {
                return ValidationResult::Skip(SkipReason::ExcludedPattern(pattern.clone()));
            }
        }

        // Check if content is empty or too small
        let trimmed = node.chunk.trim();
        if trimmed.is_empty() || trimmed.len() < self.config.min_chunk_size {
            return ValidationResult::Skip(SkipReason::TooSmall);
        }

        ValidationResult::Accept
    }

    // ============== Pipeline Stages ==============

    fn create_file_loader(&self) -> indexing::loaders::FileLoader {
        let mut loader = indexing::loaders::FileLoader::new(&self.location);

        if let Some(extensions) = &self.config.allowed_extensions {
            loader =
                loader.with_extensions(&extensions.iter().map(|s| s.as_str()).collect::<Vec<_>>());
        }

        loader
    }

    fn add_caching(&self, mut pipeline: indexing::Pipeline<String>) -> indexing::Pipeline<String> {
        if let Some(redis_url) = &self.config.redis_url {
            match integrations::redis::Redis::try_from_url(
                redis_url.clone(),
                self.collection_name(),
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
        pipeline
    }

    fn add_validation_filter(
        &self,
        pipeline: indexing::Pipeline<String>,
    ) -> indexing::Pipeline<String> {
        let metrics = self.metrics.clone();
        let config = self.config.clone();
        let check_interval = 100;

        let self_clone = self.clone();

        pipeline
            .filter(move |result| {
                // Check threshold status
                let status = metrics.check_realtime_threshold(&config, check_interval);
                if matches!(status, ThresholdStatus::Catastrophic) {
                    warn!("Catastrophic threshold status reached");
                }

                match result {
                    Ok(node) => match self_clone.validate_file(node) {
                        ValidationResult::Accept => {
                            metrics.files_loaded.fetch_add(1, Ordering::Relaxed);
                            true
                        }
                        ValidationResult::Skip(reason) => {
                            reason.log(node);
                            metrics.files_skipped.fetch_add(1, Ordering::Relaxed);
                            false
                        }
                    },
                    Err(e) => {
                        error!("Error loading file: {:#}", e);
                        metrics.files_failed.fetch_add(1, Ordering::Relaxed);
                        false
                    }
                }
            })
            .log_errors()
    }

    fn add_chunking(&self, mut pipeline: indexing::Pipeline<String>) -> indexing::Pipeline<String> {
        // Apply smart chunking
        let chunker = SmartChunker::new(self.config.min_chunk_size..self.config.max_chunk_size);
        pipeline = pipeline.then_chunk(chunker);

        // Filter empty chunks
        let metrics = self.metrics.clone();
        pipeline.filter(move |result| match result {
            Ok(node) => {
                if node.chunk.trim().is_empty() {
                    debug!("Filtering empty chunk from {:?}", node.path);
                    false
                } else {
                    metrics.chunks_created.fetch_add(1, Ordering::Relaxed);
                    true
                }
            }
            Err(e) => {
                warn!("Chunk creation failed: {:#}", e);
                false
            }
        })
    }

    fn add_metadata_generation(
        &self,
        mut pipeline: indexing::Pipeline<String>,
        ollama_with_backoff: indexing::LanguageModelWithBackOff<integrations::ollama::Ollama>,
    ) -> indexing::Pipeline<String> {
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

        pipeline
    }

    fn add_embedding_and_storage(
        &self,
        mut pipeline: indexing::Pipeline<String>,
        qdrant: integrations::qdrant::Qdrant,
    ) -> indexing::Pipeline<String> {
        let fastembed = integrations::fastembed::FastEmbed::try_default()
            .expect("Failed to initialize FastEmbed");

        // Embed in batches
        pipeline = pipeline
            .then_in_batch(Embed::new(fastembed).with_batch_size(self.config.embed_batch_size));

        // Count stored chunks
        let metrics = self.metrics.clone();
        pipeline = pipeline.log_nodes().filter(move |result| {
            if result.is_ok() {
                metrics.chunks_stored.fetch_add(1, Ordering::Relaxed);
            }
            true
        });

        // Store in Qdrant
        pipeline.then_store_with(qdrant)
    }

    pub async fn build_indexing_pipeline(&self) -> Result<indexing::Pipeline<String>> {
        let collection_name = self.collection_name();

        // Initialize shared clients
        let ollama_client = self.llm_client();
        let ollama_with_backoff =
            indexing::LanguageModelWithBackOff::new(ollama_client.clone(), Default::default());
        let qdrant = self.qdrant(collection_name)?;

        // Build pipeline in stages
        let loader = self.create_file_loader();
        let mut pipeline =
            indexing::Pipeline::from_loader(loader).with_concurrency(self.config.concurrency);

        pipeline = self.add_caching(pipeline);
        pipeline = self.add_validation_filter(pipeline);

        // Add rate limiting
        if self.config.rate_limit_ms > 0 {
            pipeline = pipeline.throttle(Duration::from_millis(self.config.rate_limit_ms));
        }

        pipeline = self.add_chunking(pipeline);
        pipeline = self.add_metadata_generation(pipeline, ollama_with_backoff.clone());
        pipeline = self.add_embedding_and_storage(pipeline, qdrant);
        pipeline = pipeline.log_all();

        Ok(pipeline)
    }

    pub async fn build_query_pipeline(
        &self,
    ) -> Result<query::Pipeline<'static, SimilaritySingleEmbedding, Answered>> {
        let ollama_client = self.llm_client();
        let fastembed = integrations::fastembed::FastEmbed::try_default()?;
        let reranker = fastembed::Rerank::builder().top_k(5).build()?;
        let collection_name = self.collection_name();
        let qdrant = self.qdrant(collection_name)?;

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

        drop(pipelines); // Release lock before long operation

        // Run indexing in background
        tokio::spawn({
            let manager_clone = self.clone();
            let space_key = space_key.to_string();

            async move {
                let res = manager_clone.run_indexing(&space_key).await;

                match &res {
                    Ok(_) => info!("Indexing completed for space: {}", space_key),
                    Err(e) => error!("Indexing failed for space {}: {}", space_key, e),
                }

                let mut pipelines = manager_clone.pipelines.write().await;
                if let Some(p) = pipelines.get_mut(&space_key) {
                    p.indexing_in_progress = false;
                    if res.is_ok() {
                        p.last_indexed = Some(chrono::Utc::now());
                    }
                }
            }
        });

        Ok(())
    }

    async fn run_indexing(&self, space_key: &str) -> Result<(), AppError> {
        info!("Starting indexing");

        let (pipeline_struct, indexing_pipeline) = {
            let pipelines = self.pipelines.read().await;
            let pipeline = pipelines
                .get(space_key)
                .ok_or_else(|| AppError::NotFound(format!("Space not found: {}", space_key)))?;

            let pipeline_struct = pipeline.clone();

            // Build the indexing pipeline while we have the lock
            let indexing_pipeline = pipeline.build_indexing_pipeline().await.map_err(|e| {
                AppError::Internal(format!("Failed to build indexing pipeline: {}", e))
            })?;

            (pipeline_struct, indexing_pipeline)
        };

        // Run the indexing pipeline
        indexing_pipeline
            .run()
            .await
            .map_err(|e| AppError::Internal(format!("Indexing failed: {}", e)))?;

        // VALIDATION: Check failure threshold
        let metrics = &pipeline_struct.metrics;

        if let Err(msg) = metrics.validate_threshold(&pipeline_struct.config) {
            error!("Indexing failed for space '{}': {}", space_key, msg);

            // Update DB with failed status
            self.update_index_status(space_key, metrics, Some(&msg))
                .await?;

            return Err(AppError::Internal(format!(
                "Indexing validation failed: {}",
                msg
            )));
        } else {
            // Success
            info!("Indexing completed successfully for space '{}'", space_key);
            self.update_index_status(space_key, metrics, None).await?;

            Ok(())
        }
    }

    pub async fn query_space(&self, space_key: &str, query: &str) -> Result<String, AppError> {
        // Verify the space exists
        let query_pipeline = {
            let pipelines = self.pipelines.read().await;
            let pipeline = pipelines
                .get(space_key)
                .ok_or_else(|| AppError::NotFound(format!("Space not found: {}", space_key)))?;

            // Build the query pipeline while we have the lock
            pipeline
                .build_query_pipeline()
                .await
                .map_err(|e| AppError::Internal(format!("Failed to build query pipeline: {}", e)))?
        }; // Lock released here

        // Execute the query
        let answer = query_pipeline
            .query(query)
            .await
            .map_err(|e| AppError::Internal(format!("Query failed: {}", e)))?;

        Ok(answer.answer().to_string())
    }

    async fn update_index_status(
        &self,
        space_key: &str,
        metrics: &PipelineMetrics,
        error: Option<&str>,
    ) -> Result<(), AppError> {
        use entity::space_index_status;
        use sea_orm::{ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};

        // Find the space by key
        let space = space::Entity::find()
            .filter(space::Column::Key.eq(space_key))
            .one(&self.db)
            .await
            .map_err(|e| AppError::Storage(Box::new(e)))?
            .ok_or_else(|| AppError::NotFound(format!("Space not found: {}", space_key)))?;

        // Find the existing index status
        let status = space_index_status::Entity::find()
            .filter(space_index_status::Column::SpaceId.eq(space.id))
            .one(&self.db)
            .await
            .map_err(|e| AppError::Storage(Box::new(e)))?;

        if let Some(status) = status {
            let mut active_status: space_index_status::ActiveModel = status.into();

            active_status.indexing_in_progress = Set(Some(false));
            active_status.files_indexed =
                Set(Some(metrics.files_loaded.load(Ordering::Relaxed) as i32));
            active_status.chunks_stored =
                Set(Some(metrics.chunks_stored.load(Ordering::Relaxed) as i32));

            active_status.files_failed =
                Set(Some(metrics.files_failed.load(Ordering::Relaxed) as i32));

            if let Some(error_msg) = error {
                active_status.last_error = Set(Some(error_msg.to_string()));

                warn!(
                    space_key = %space_key,
                    files_loaded = metrics.files_loaded.load(Ordering::Relaxed),
                    files_failed = metrics.files_failed.load(Ordering::Relaxed),
                    failure_rate = %format!("{:.1}%", metrics.failure_rate() * 100.0),
                    error = %error_msg,
                    "Updated index status with failure"
                );
            } else {
                active_status.last_indexed = Set(Some(chrono::Utc::now().fixed_offset()));
                active_status.last_error = Set(None);

                info!(
                    space_key = %space_key,
                    files_indexed = metrics.files_loaded.load(Ordering::Relaxed),
                    chunks_stored = metrics.chunks_stored.load(Ordering::Relaxed),
                    files_skipped = metrics.files_skipped.load(Ordering::Relaxed),
                    "Updated index status with success"
                );
            }

            // Persist to database
            active_status
                .update(&self.db)
                .await
                .map_err(|e| AppError::Storage(Box::new(e)))?;
        } else {
            // No existing status record - create one
            warn!(
                space_key = %space_key,
                space_id = space.id,
                "No index status found, creating new record"
            );

            let new_status = space_index_status::ActiveModel {
                space_id: Set(space.id),
                last_indexed: Set(if error.is_none() {
                    Some(chrono::Utc::now().fixed_offset())
                } else {
                    None
                }),
                indexing_in_progress: Set(Some(false)),
                files_indexed: Set(Some(metrics.files_loaded.load(Ordering::Relaxed) as i32)),
                chunks_stored: Set(Some(metrics.chunks_stored.load(Ordering::Relaxed) as i32)),
                files_failed: Set(Some(metrics.files_failed.load(Ordering::Relaxed) as i32)),
                last_error: Set(error.map(|e| e.to_string())),
                ..Default::default()
            };

            new_status
                .insert(&self.db)
                .await
                .map_err(|e| AppError::Storage(Box::new(e)))?;
        }

        Ok(())
    }
}
