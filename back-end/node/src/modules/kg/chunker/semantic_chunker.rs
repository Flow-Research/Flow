use anyhow::{Context, Result};
use candle_core::{Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config, DTYPE};
use hf_hub::{Repo, RepoType, api::tokio::Api};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokenizers::Tokenizer;
use tracing::{debug, info, instrument, warn};
use unicode_segmentation::UnicodeSegmentation;

// ============================================================================
// Configuration
// ============================================================================

#[derive(Debug, Clone)]
pub struct EmbedderConfig {
    pub model_id: String,
    pub batch_size: usize,
    pub use_gpu: bool,
    pub download_timeout: Duration,
}

impl Default for EmbedderConfig {
    fn default() -> Self {
        Self {
            model_id: "sentence-transformers/all-MiniLM-L6-v2".to_string(),
            batch_size: 8,
            use_gpu: false,
            download_timeout: Duration::from_secs(300), // 5 minutes
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChunkerConfig {
    pub similarity_threshold: f32,
    pub max_tokens: usize,
    pub min_tokens: usize,
}

impl ChunkerConfig {
    pub fn new(similarity_threshold: f32, max_tokens: usize, min_tokens: usize) -> Result<Self> {
        Self::validate(similarity_threshold, max_tokens, min_tokens)?;
        Ok(Self {
            similarity_threshold,
            max_tokens,
            min_tokens,
        })
    }

    fn validate(similarity_threshold: f32, max_tokens: usize, min_tokens: usize) -> Result<()> {
        if !similarity_threshold.is_finite() || !(0.0..=1.0).contains(&similarity_threshold) {
            return Err(anyhow::anyhow!(
                "similarity_threshold must be finite and in range [0.0, 1.0], got: {}",
                similarity_threshold
            ));
        }
        if min_tokens == 0 {
            return Err(anyhow::anyhow!("min_tokens must be greater than 0"));
        }
        if min_tokens >= max_tokens {
            return Err(anyhow::anyhow!(
                "min_tokens ({}) must be less than max_tokens ({})",
                min_tokens,
                max_tokens
            ));
        }
        Ok(())
    }
}

impl Default for ChunkerConfig {
    fn default() -> Self {
        Self {
            similarity_threshold: 0.5,
            max_tokens: 512,
            min_tokens: 50,
        }
    }
}

// ============================================================================
// Core Types
// ============================================================================

#[derive(Debug, Clone)]
pub struct SemanticChunk {
    pub text: String,
    pub token_count: usize,
    pub sentence_count: usize,
    pub sentences: Vec<String>,
    pub metadata: ChunkMetadata,
}

#[derive(Debug, Clone, Default)]
pub struct ChunkMetadata {
    pub embedding: Option<Vec<f32>>,
    pub topic_label: Option<String>,
    pub coherence_score: Option<f32>,
}

// ============================================================================
// Embedder Trait & Implementation
// ============================================================================

/// Trait for sentence embedding models
///
/// Implementations must be Send + Sync for concurrent use.
pub trait SentenceEmbedder: Send + Sync {
    fn embed(&self, sentences: &[String]) -> Result<Vec<Vec<f32>>>;
}

/// Candle-based sentence embedder using BERT models
///
/// Thread Safety: This struct is Send + Sync. The underlying BertModel
/// does not use interior mutability, so concurrent reads are safe.
/// For concurrent embedding operations, wrap in Arc.
pub struct CandleEmbedder {
    model: BertModel,
    tokenizer: Tokenizer,
    device: Device,
    config: EmbedderConfig,
}

impl CandleEmbedder {
    /// Create a new embedder with custom configuration
    ///
    /// This is an async operation that may take several minutes on first run
    /// as it downloads the model from HuggingFace.
    ///
    /// # Common Models
    /// - "sentence-transformers/all-MiniLM-L6-v2" (lightweight, 384 dims, 80MB)
    /// - "sentence-transformers/all-MiniLM-L12-v2" (better quality, 384 dims, 120MB)
    /// - "sentence-transformers/all-mpnet-base-v2" (best quality, 768 dims, 420MB)
    #[instrument(skip(config), fields(model_id = %config.model_id))]
    pub async fn new(config: EmbedderConfig) -> Result<Self> {
        info!(
            "Initializing CandleEmbedder with model: {}",
            config.model_id
        );

        let device = if config.use_gpu && candle_core::utils::cuda_is_available() {
            info!("Using CUDA device");
            Device::new_cuda(0).context("Failed to initialize CUDA device")?
        } else {
            debug!("Using CPU device");
            Device::Cpu
        };

        // Download model files with timeout
        let download_task = Self::download_model(&config.model_id);
        let (config_path, tokenizer_path, weights_path) =
            tokio::time::timeout(config.download_timeout, download_task)
                .await
                .context("Model download timed out")?
                .context("Failed to download model")?;

        info!("Model files downloaded successfully");

        // Load configuration
        let model_config: Config = serde_json::from_str(&std::fs::read_to_string(&config_path)?)
            .context("Failed to parse model config")?;

        // Load model weights
        let vb = if weights_path
            .extension()
            .map_or(false, |e| e == "safetensors")
        {
            debug!("Loading safetensors weights");
            // SAFETY: We trust the safetensors format from HuggingFace Hub.
            // Memory-mapped loading is safe as we only read from the file and
            // do not expose raw pointers to user code.
            unsafe {
                VarBuilder::from_mmaped_safetensors(&[weights_path], DTYPE, &device)
                    .context("Failed to load safetensors")?
            }
        } else {
            debug!("Loading PyTorch weights");
            VarBuilder::from_pth(&weights_path, DTYPE, &device)
                .context("Failed to load PyTorch weights")?
        };

        let model =
            BertModel::load(vb, &model_config).context("Failed to initialize BERT model")?;

        // Load tokenizer
        let tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| anyhow::anyhow!("Failed to load tokenizer: {}", e))?;

        info!("CandleEmbedder initialized successfully");

        Ok(Self {
            model,
            tokenizer,
            device,
            config,
        })
    }

    /// Create embedder with default configuration
    pub async fn default() -> Result<Self> {
        Self::new(EmbedderConfig::default()).await
    }

    async fn download_model(model_id: &str) -> Result<(PathBuf, PathBuf, PathBuf)> {
        let api = Api::new().context("Failed to initialize HuggingFace API")?;
        let repo = api.repo(Repo::new(model_id.to_string(), RepoType::Model));

        info!("Downloading model files for {}", model_id);

        let config_path = repo
            .get("config.json")
            .await
            .context("Failed to download config.json")?;

        let tokenizer_path = repo
            .get("tokenizer.json")
            .await
            .context("Failed to download tokenizer.json")?;

        let weights_path = match repo.get("model.safetensors").await {
            Ok(path) => path,
            Err(_) => repo.get("pytorch_model.bin").await.context(
                "Failed to download model weights (tried model.safetensors and pytorch_model.bin)",
            )?,
        };

        Ok((config_path, tokenizer_path, weights_path))
    }

    #[instrument(skip(self, token_embeddings, attention_mask))]
    fn mean_pooling(&self, token_embeddings: &Tensor, attention_mask: &Tensor) -> Result<Tensor> {
        // Expand attention mask to match embedding dimensions
        let expanded_mask = attention_mask
            .unsqueeze(2)
            .context("Failed to unsqueeze attention mask")?
            .expand(token_embeddings.shape())
            .context("Failed to expand attention mask")?
            .to_dtype(token_embeddings.dtype())
            .context("Failed to convert attention mask dtype")?;

        // Sum embeddings weighted by mask
        let sum_embeddings = (token_embeddings * &expanded_mask)
            .context("Failed to multiply embeddings by mask")?
            .sum(1)
            .context("Failed to sum embeddings")?;

        // Sum attention mask to get valid token counts
        let sum_mask = expanded_mask
            .sum(1)
            .context("Failed to sum attention mask")?;

        // Avoid division by zero
        let sum_mask = sum_mask
            .clamp(1e-9, f32::INFINITY)
            .context("Failed to clamp mask values")?;

        // Divide to get mean
        sum_embeddings
            .broadcast_div(&sum_mask)
            .context("Failed to compute mean pooling")
    }

    fn normalize(&self, embeddings: &Tensor) -> Result<Tensor> {
        let norm = embeddings
            .sqr()
            .context("Failed to square embeddings")?
            .sum_keepdim(1)
            .context("Failed to sum squared embeddings")?
            .sqrt()
            .context("Failed to compute norm")?;

        embeddings
            .broadcast_div(&norm)
            .context("Failed to normalize embeddings")
    }
}

impl SentenceEmbedder for CandleEmbedder {
    #[instrument(skip(self, sentences), fields(sentence_count = sentences.len()))]
    fn embed(&self, sentences: &[String]) -> Result<Vec<Vec<f32>>> {
        if sentences.is_empty() {
            debug!("Empty sentences array, returning empty embeddings");
            return Ok(Vec::new());
        }

        if sentences.len() > 1000 {
            warn!(
                "Embedding large batch of {} sentences, this may be slow",
                sentences.len()
            );
        }

        let mut all_embeddings = Vec::with_capacity(sentences.len());
        let batch_size = self.config.batch_size;

        for (batch_idx, batch) in sentences.chunks(batch_size).enumerate() {
            debug!("Processing batch {}, size: {}", batch_idx, batch.len());
            let batch_strs: Vec<&str> = batch.iter().map(|s| s.as_str()).collect();
            let batch_embeddings = self
                .embed_batch(&batch_strs)
                .with_context(|| format!("Failed to embed batch {}", batch_idx))?;
            all_embeddings.extend(batch_embeddings);
        }

        debug!("Embedded {} sentences successfully", sentences.len());
        Ok(all_embeddings)
    }
}

impl CandleEmbedder {
    #[instrument(skip(self, sentences), fields(batch_size = sentences.len()))]
    fn embed_batch(&self, sentences: &[&str]) -> Result<Vec<Vec<f32>>> {
        // Tokenize sentences
        let encodings = self
            .tokenizer
            .encode_batch(sentences.to_vec(), true)
            .map_err(|e| {
                anyhow::anyhow!(
                    "Tokenization failed for {} sentences: {}",
                    sentences.len(),
                    e
                )
            })?;

        if encodings.is_empty() {
            return Ok(Vec::new());
        }

        // Extract input IDs and attention masks
        let input_ids: Vec<Vec<u32>> = encodings.iter().map(|e| e.get_ids().to_vec()).collect();
        let attention_masks: Vec<Vec<u32>> = encodings
            .iter()
            .map(|e| e.get_attention_mask().to_vec())
            .collect();

        let max_len = input_ids.iter().map(|ids| ids.len()).max().unwrap_or(0);

        if max_len == 0 {
            return Err(anyhow::anyhow!(
                "All sequences have zero length after tokenization"
            ));
        }

        // Pad sequences efficiently
        let mut padded_ids = Vec::with_capacity(input_ids.len());
        let mut padded_masks = Vec::with_capacity(attention_masks.len());

        for (ids, mask) in input_ids.iter().zip(attention_masks.iter()) {
            let mut padded = ids.clone();
            let mut mask_padded = mask.clone();

            // Pre-allocate to avoid repeated allocations
            padded.resize(max_len, 0);
            mask_padded.resize(max_len, 0);

            padded_ids.push(padded);
            padded_masks.push(mask_padded);
        }

        // Convert to i64 for BERT
        let input_ids_flat: Vec<i64> = padded_ids
            .iter()
            .flat_map(|ids| ids.iter().map(|&id| id as i64))
            .collect();

        let attention_mask_flat: Vec<i64> = padded_masks
            .iter()
            .flat_map(|mask| mask.iter().map(|&m| m as i64))
            .collect();

        let batch_size = padded_ids.len();
        let input_tensor = Tensor::from_slice(&input_ids_flat, (batch_size, max_len), &self.device)
            .context("Failed to create input tensor")?;

        let attention_tensor =
            Tensor::from_slice(&attention_mask_flat, (batch_size, max_len), &self.device)
                .context("Failed to create attention tensor")?;

        let token_type_ids =
            Tensor::zeros_like(&input_tensor).context("Failed to create token type IDs")?;

        // Run through model
        let model_output = self
            .model
            .forward(&input_tensor, &token_type_ids, Some(&attention_tensor))
            .context("Model forward pass failed")?;

        // Apply mean pooling
        let pooled = self.mean_pooling(&model_output, &attention_tensor)?;

        // Normalize embeddings
        let normalized = self.normalize(&pooled)?;

        // Convert to Vec<Vec<f32>>
        let embeddings = normalized
            .to_vec2::<f32>()
            .context("Failed to convert embeddings to Vec<Vec<f32>>")?;

        if embeddings.len() != sentences.len() {
            return Err(anyhow::anyhow!(
                "Embedding count mismatch: expected {}, got {}",
                sentences.len(),
                embeddings.len()
            ));
        }

        Ok(embeddings)
    }
}

// ============================================================================
// Semantic Chunker
// ============================================================================

/// Chunks text based on semantic similarity between sentences
///
/// Thread Safety: This struct is Send + Sync when the embedder is wrapped in Arc.
pub struct SemanticChunker {
    embedder: Arc<dyn SentenceEmbedder>,
    config: ChunkerConfig,
}

impl SemanticChunker {
    pub fn new(embedder: Arc<dyn SentenceEmbedder>, config: ChunkerConfig) -> Self {
        Self { embedder, config }
    }

    /// Create with default Candle embedder
    pub async fn with_default_embedder(config: ChunkerConfig) -> Result<Self> {
        let embedder = Arc::new(CandleEmbedder::default().await?);
        Ok(Self::new(embedder, config))
    }

    /// Chunk text based on semantic similarity between sentences
    #[instrument(skip(self, text), fields(text_length = text.len()))]
    pub fn chunk(&self, text: &str) -> Result<Vec<SemanticChunk>> {
        if text.is_empty() {
            debug!("Empty text provided, returning empty chunks");
            return Ok(Vec::new());
        }

        info!("Starting semantic chunking");

        // 1. Split into sentences
        let sentences: Vec<String> = text.unicode_sentences().map(|s| s.to_string()).collect();

        if sentences.is_empty() {
            debug!("No sentences found in text");
            return Ok(Vec::new());
        }

        debug!("Split text into {} sentences", sentences.len());

        // 2. Generate embeddings for all sentences
        let embeddings = self
            .embedder
            .embed(&sentences)
            .context("Failed to generate sentence embeddings")?;

        if embeddings.len() != sentences.len() {
            return Err(anyhow::anyhow!(
                "Embedding count mismatch: {} sentences but {} embeddings",
                sentences.len(),
                embeddings.len()
            ));
        }

        // 3. Calculate similarity between consecutive sentences
        let similarities = self.calculate_consecutive_similarities(&embeddings)?;

        // 4. Find semantic breakpoints (low similarity)
        let breakpoints = self.find_breakpoints(&similarities);

        debug!("Found {} breakpoints", breakpoints.len());

        // 5. Create chunks based on breakpoints
        let chunks = self.create_semantic_chunks(sentences, embeddings, breakpoints)?;

        info!("Created {} semantic chunks", chunks.len());
        Ok(chunks)
    }

    fn calculate_consecutive_similarities(&self, embeddings: &[Vec<f32>]) -> Result<Vec<f32>> {
        embeddings
            .windows(2)
            .map(|window| cosine_similarity(&window[0], &window[1]))
            .collect::<Result<Vec<f32>>>()
    }

    fn find_breakpoints(&self, similarities: &[f32]) -> Vec<usize> {
        let mut breakpoints = vec![0];

        for (idx, &similarity) in similarities.iter().enumerate() {
            if similarity < self.config.similarity_threshold {
                breakpoints.push(idx + 1);
            }
        }

        breakpoints.push(similarities.len() + 1);
        breakpoints
    }

    fn create_semantic_chunks(
        &self,
        sentences: Vec<String>,
        embeddings: Vec<Vec<f32>>,
        breakpoints: Vec<usize>,
    ) -> Result<Vec<SemanticChunk>> {
        let mut chunks = Vec::new();

        for window in breakpoints.windows(2) {
            let start = window[0];
            let end = window[1];

            if start >= sentences.len() {
                continue;
            }

            let actual_end = end.min(sentences.len());
            if start >= actual_end {
                continue;
            }

            let chunk_sentences = sentences[start..actual_end].to_vec();
            let chunk_embeddings = embeddings[start..actual_end].to_vec();

            let avg_embedding = average_embeddings(&chunk_embeddings)?;
            let text = chunk_sentences.join(" ");

            chunks.push(SemanticChunk {
                text,
                token_count: self.estimate_tokens(&chunk_sentences),
                sentence_count: chunk_sentences.len(),
                sentences: chunk_sentences,
                metadata: ChunkMetadata {
                    embedding: Some(avg_embedding),
                    topic_label: None,
                    coherence_score: Some(self.calculate_coherence(&chunk_embeddings)?),
                },
            });
        }

        let processed_chunks = self.post_process_chunks(chunks)?;
        Ok(processed_chunks)
    }

    fn calculate_coherence(&self, embeddings: &[Vec<f32>]) -> Result<f32> {
        if embeddings.len() < 2 {
            return Ok(1.0);
        }

        let mut total_similarity = 0.0;
        let mut count = 0;

        for i in 0..embeddings.len() {
            for j in (i + 1)..embeddings.len() {
                total_similarity += cosine_similarity(&embeddings[i], &embeddings[j])?;
                count += 1;
            }
        }

        Ok(if count > 0 {
            total_similarity / count as f32
        } else {
            1.0
        })
    }

    fn post_process_chunks(&self, chunks: Vec<SemanticChunk>) -> Result<Vec<SemanticChunk>> {
        let mut processed = Vec::new();
        let mut pending: Option<SemanticChunk> = None;

        for chunk in chunks {
            let token_count = chunk.token_count;

            // If chunk is too large, split it
            if token_count > self.config.max_tokens {
                if let Some(p) = pending.take() {
                    processed.push(p);
                }
                let split_chunks = self.split_large_chunk(chunk)?;
                processed.extend(split_chunks);
                continue;
            }

            // If chunk is too small, try to merge with previous
            if token_count < self.config.min_tokens {
                pending = match pending {
                    Some(p) => {
                        // NEW: Check if semantically similar before merging
                        if self.should_merge(&p, &chunk)? {
                            Some(self.merge_chunks(p, chunk))
                        } else {
                            // Not similar enough - keep separate
                            processed.push(p);
                            Some(chunk)
                        }
                    }
                    None => Some(chunk),
                };
                continue;
            }

            // Chunk is just right
            if let Some(p) = pending.take() {
                processed.push(p);
            }
            processed.push(chunk);
        }

        if let Some(p) = pending {
            processed.push(p);
        }

        Ok(processed)
    }

    fn should_merge(&self, chunk_a: &SemanticChunk, chunk_b: &SemanticChunk) -> Result<bool> {
        match (&chunk_a.metadata.embedding, &chunk_b.metadata.embedding) {
            (Some(emb_a), Some(emb_b)) => {
                let similarity = cosine_similarity(emb_a, emb_b)?;
                // Use same threshold as breakpoint detection
                Ok(similarity >= self.config.similarity_threshold)
            }
            _ => Ok(true), // No embeddings, merge by default
        }
    }

    fn merge_chunks(&self, mut a: SemanticChunk, b: SemanticChunk) -> SemanticChunk {
        a.text.push(' ');
        a.text.push_str(&b.text);
        a.token_count += b.token_count;
        a.sentence_count += b.sentence_count;
        a.sentences.extend(b.sentences);

        let emb_a = a.metadata.embedding.take();
        let emb_b = b.metadata.embedding;

        if let (Some(ea), Some(eb)) = (emb_a, emb_b) {
            // Ignore error in merge - worst case we lose embedding
            a.metadata.embedding = average_embeddings(&[ea, eb]).ok();
        }

        a
    }

    fn split_large_chunk(&self, chunk: SemanticChunk) -> Result<Vec<SemanticChunk>> {
        let mut result = Vec::new();
        let mut current_sentences = Vec::new();
        let mut current_tokens = 0;

        for sentence in chunk.sentences {
            let sentence_tokens = self.estimate_tokens(&[sentence.clone()]);

            if current_tokens + sentence_tokens > self.config.max_tokens
                && !current_sentences.is_empty()
            {
                let text = current_sentences.join(" ");
                result.push(SemanticChunk {
                    text,
                    token_count: current_tokens,
                    sentence_count: current_sentences.len(),
                    sentences: current_sentences.clone(),
                    metadata: ChunkMetadata::default(),
                });
                current_sentences.clear();
                current_tokens = 0;
            }

            current_sentences.push(sentence);
            current_tokens += sentence_tokens;
        }

        if !current_sentences.is_empty() {
            let text = current_sentences.join(" ");
            result.push(SemanticChunk {
                text,
                token_count: current_tokens,
                sentence_count: current_sentences.len(),
                sentences: current_sentences,
                metadata: ChunkMetadata::default(),
            });
        }

        Ok(result)
    }

    fn estimate_tokens(&self, sentences: &[String]) -> usize {
        // Improved heuristic: average of 5 chars per token for English
        // This is still approximate but better than 4
        let total_chars: usize = sentences.iter().map(|s| s.len()).sum();
        (total_chars / 5).max(1)
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

pub fn cosine_similarity(a: &[f32], b: &[f32]) -> Result<f32> {
    if a.len() != b.len() {
        return Err(anyhow::anyhow!(
            "Vector dimension mismatch: {} vs {}",
            a.len(),
            b.len()
        ));
    }

    if a.is_empty() {
        return Err(anyhow::anyhow!(
            "Cannot compute similarity of empty vectors"
        ));
    }

    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let magnitude_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let magnitude_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if magnitude_a == 0.0 || magnitude_b == 0.0 {
        return Ok(0.0);
    }

    let similarity = dot_product / (magnitude_a * magnitude_b);

    // Clamp to valid range due to floating point errors
    Ok(similarity.clamp(-1.0, 1.0))
}

pub fn average_embeddings(embeddings: &[Vec<f32>]) -> Result<Vec<f32>> {
    if embeddings.is_empty() {
        return Err(anyhow::anyhow!("Cannot average empty embeddings"));
    }

    let dim = embeddings[0].len();

    // Validate all embeddings have same dimension
    for (i, emb) in embeddings.iter().enumerate() {
        if emb.len() != dim {
            return Err(anyhow::anyhow!(
                "Embedding dimension mismatch at index {}: expected {}, got {}",
                i,
                dim,
                emb.len()
            ));
        }
    }

    let mut avg = vec![0.0; dim];

    for embedding in embeddings {
        for (i, &val) in embedding.iter().enumerate() {
            avg[i] += val;
        }
    }

    let count = embeddings.len() as f32;
    for val in &mut avg {
        *val /= count;
    }

    Ok(avg)
}
