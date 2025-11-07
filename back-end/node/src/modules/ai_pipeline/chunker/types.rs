use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Chunk {
    pub text: String,
    pub token_count: usize,
    pub sentence_count: usize,
    pub start_char: usize,
    pub end_char: usize,
    pub chunk_id: usize,
    pub metadata: ChunkMetadata,
}

/// Metadata associated with a chunk
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ChunkMetadata {
    pub content_type: Option<ContentType>,

    /// Detected language (ISO 639-1 code)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub vector_id: Option<String>,

    /// Custom metadata from application
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom: Option<serde_json::Value>,
}

/// Content type classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentType {
    /// Source code
    Code,
    /// Markdown documentation
    Markdown,
    /// Plain text
    PlainText,
    /// Scientific/academic paper
    Scientific,
    /// Structured data (JSON, XML, etc.)
    Structured,
}

impl std::fmt::Display for ContentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ContentType::Code => write!(f, "code"),
            ContentType::Markdown => write!(f, "markdown"),
            ContentType::PlainText => write!(f, "plain_text"),
            ContentType::Scientific => write!(f, "scientific"),
            ContentType::Structured => write!(f, "structured"),
        }
    }
}

/// Chunking strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChunkingStrategy {
    /// Fixed character/token size
    FixedSize,
    /// Respect sentence boundaries
    Sentence,
    /// Respect paragraph boundaries
    Paragraph,
    /// Recursive splitting (markdown headers, code blocks)
    Recursive,
}

impl std::fmt::Display for ChunkingStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChunkingStrategy::FixedSize => write!(f, "fixed_size"),
            ChunkingStrategy::Sentence => write!(f, "sentence"),
            ChunkingStrategy::Paragraph => write!(f, "paragraph"),
            ChunkingStrategy::Recursive => write!(f, "recursive"),
        }
    }
}

/// Error types
#[derive(Debug, thiserror::Error)]
pub enum ChunkerError {
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Tokenization failed: {0}")]
    TokenizationError(String),

    #[error("Text splitting failed: {0}")]
    SplittingError(String),

    #[error("Embedding generation failed: {0}")]
    EmbeddingError(String),

    #[error("Content detection failed: {0}")]
    ContentDetectionError(String),

    #[error("Input validation failed: {0}")]
    ValidationError(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, ChunkerError>;
