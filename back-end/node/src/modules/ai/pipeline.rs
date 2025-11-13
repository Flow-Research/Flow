use std::{sync::atomic::Ordering, time::Duration};

use super::metrics::PipelineMetrics;
use anyhow::Result;
use swiftide::{
    indexing::{
        self,
        transformers::{Embed, MetadataKeywords, MetadataQAText, MetadataSummary},
    },
    integrations,
    query::{
        self, answers, query_transformers, search_strategies::SimilaritySingleEmbedding,
        states::Answered,
    },
};
use swiftide_integrations::fastembed;

use super::config::IndexingConfig;
use tracing::{debug, error, info, warn};

use crate::modules::ai::metrics::ThresholdStatus;
use crate::modules::ai::smart_chunker::SmartChunker;

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
