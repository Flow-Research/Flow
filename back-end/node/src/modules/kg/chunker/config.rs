use super::types::{ChunkerError, ChunkingStrategy, ContentType, Result};
use serde::{Deserialize, Serialize};

/// Main configuration for chunking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkerConfig {
    /// Maximum characters per chunk (approximate)
    pub max_chunk_size: usize,

    /// Overlap size in characters
    pub overlap: usize,

    /// Trim whitespace from chunks
    pub trim: bool,
    pub force_content_type: Option<ContentType>,
    pub force_strategy: Option<ChunkingStrategy>,
    pub detect_language: bool,
    pub generate_embeddings: bool,

    pub embedding_config: Option<EmbeddingConfig>,

    pub max_input_size: usize,
    pub reject_null_chars: bool,
}

impl Default for ChunkerConfig {
    fn default() -> Self {
        Self {
            max_chunk_size: 2000,
            overlap: 200,
            trim: true,
            force_content_type: None,
            force_strategy: None,
            detect_language: false,
            generate_embeddings: false,
            embedding_config: None,
            max_input_size: 10 * 1024 * 1024, // 10MB
            reject_null_chars: true,
        }
    }
}

impl ChunkerConfig {
    /// Create with defaults
    pub fn new() -> Self {
        Self::default()
    }

    /// Builder pattern methods
    pub fn with_max_size(mut self, size: usize) -> Self {
        self.max_chunk_size = size;
        self
    }

    pub fn with_overlap(mut self, overlap: usize) -> Self {
        self.overlap = overlap;
        self
    }

    pub fn with_content_type(mut self, content_type: ContentType) -> Self {
        self.force_content_type = Some(content_type);
        self
    }

    pub fn with_strategy(mut self, strategy: ChunkingStrategy) -> Self {
        self.force_strategy = Some(strategy);
        self
    }

    pub fn with_language_detection(mut self) -> Self {
        self.detect_language = true;
        self
    }

    pub fn with_embeddings(mut self, config: EmbeddingConfig) -> Self {
        self.generate_embeddings = true;
        self.embedding_config = Some(config);
        self
    }

    pub fn with_max_input_size(mut self, size: usize) -> Self {
        self.max_input_size = size;
        self
    }

    pub fn with_null_char_rejection(mut self, reject: bool) -> Self {
        self.reject_null_chars = reject;
        self
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<()> {
        if self.max_chunk_size == 0 {
            return Err(ChunkerError::InvalidConfig(
                "max_chunk_size must be greater than 0".to_string(),
            ));
        }

        if self.overlap >= self.max_chunk_size {
            return Err(ChunkerError::InvalidConfig(format!(
                "overlap ({}) must be less than max_chunk_size ({})",
                self.overlap, self.max_chunk_size
            )));
        }

        if self.overlap > self.max_chunk_size / 2 {
            tracing::warn!(
                overlap = self.overlap,
                max_chunk_size = self.max_chunk_size,
                "Overlap is greater than 50% of chunk size, may cause excessive duplication"
            );
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    /// HuggingFace model ID
    pub model_id: String,

    /// Batch size for embedding generation
    pub batch_size: usize,

    /// Use GPU if available
    pub use_gpu: bool,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            model_id: "sentence-transformers/all-MiniLM-L6-v2".to_string(),
            batch_size: 32,
            use_gpu: false,
        }
    }
}
