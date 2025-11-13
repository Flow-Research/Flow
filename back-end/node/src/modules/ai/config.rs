use anyhow::{Result, anyhow};
use sea_orm::Iden;

#[derive(Clone, Debug)]
pub struct IndexingConfig {
    pub ai_api_key: String,

    /// Redis URL for caching (optional)
    pub redis_url: Option<String>,

    /// Qdrant URL
    pub qdrant_url: String,

    /// Vector size for embeddings
    pub vector_size: usize,

    /// Minimum chunk size (bytes)
    pub min_chunk_size: usize,

    /// Maximum chunk size (bytes)
    pub max_chunk_size: usize,

    /// Maximum file size to process (bytes)
    pub max_file_size: usize,

    /// Batch size for embedding
    pub embed_batch_size: usize,

    /// Batch size for storage
    pub storage_batch_size: usize,

    /// Pipeline concurrency
    pub concurrency: usize,

    /// Rate limit: milliseconds between operations
    pub rate_limit_ms: u64,

    /// Enable metadata generation
    pub enable_metadata_qa: bool,
    pub enable_metadata_summary: bool,
    pub enable_metadata_keywords: bool,

    /// File extensions to index (None = all)
    pub allowed_extensions: Option<Vec<String>>,

    /// File patterns to exclude
    pub exclude_patterns: Vec<String>,

    pub max_failure_rate: f64,

    pub min_sample_size: usize,
}

impl IndexingConfig {
    /// Load configuration from environment variables with sensible defaults
    pub fn from_env() -> Result<Self> {
        let config = Self {
            ai_api_key: "".to_string(),

            redis_url: std::env::var("REDIS_URL")
                .ok()
                .or(Some("redis://localhost:6379".to_string())),

            qdrant_url: std::env::var("QDRANT_URL")
                .unwrap_or_else(|_| "http://localhost:6334".to_string()),

            vector_size: std::env::var("VECTOR_SIZE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(384),

            min_chunk_size: std::env::var("MIN_CHUNK_SIZE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(100),

            max_chunk_size: std::env::var("MAX_CHUNK_SIZE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(2048),

            max_file_size: std::env::var("MAX_FILE_SIZE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(10_000_000), // 10MB

            embed_batch_size: std::env::var("EMBED_BATCH_SIZE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(20),

            storage_batch_size: std::env::var("STORAGE_BATCH_SIZE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(50),

            concurrency: std::env::var("CONCURRENCY")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or_else(|| num_cpus::get().min(4)),

            rate_limit_ms: std::env::var("RATE_LIMIT_MS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(100),

            enable_metadata_qa: std::env::var("ENABLE_METADATA_QA")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(true),

            enable_metadata_summary: std::env::var("ENABLE_METADATA_SUMMARY")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(true),

            enable_metadata_keywords: std::env::var("ENABLE_METADATA_KEYWORDS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(true),

            allowed_extensions: std::env::var("ALLOWED_EXTENSIONS")
                .ok()
                .map(|s| s.split(',').map(String::from).collect()),

            exclude_patterns: std::env::var("EXCLUDE_PATTERNS")
                .ok()
                .map(|s| s.split(',').map(String::from).collect())
                .unwrap_or_else(|| {
                    vec![
                        "node_modules".to_string(),
                        ".git".to_string(),
                        "target".to_string(),
                        "dist".to_string(),
                        "build".to_string(),
                        "__pycache__".to_string(),
                        ".venv".to_string(),
                        "venv".to_string(),
                    ]
                }),

            max_failure_rate: std::env::var("MAX_FAILURE_RATE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.5),

            min_sample_size: std::env::var("MIN_SAMPLE_SIZE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(10),
        };
        config.validate()?;

        Ok(config)
    }

    fn validate(&self) -> Result<()> {
        if self.min_chunk_size >= self.max_chunk_size {
            return Err(anyhow!("min_chunk_size must be less than max_chunk_size"));
        }

        if self.embed_batch_size == 0 || self.storage_batch_size == 0 {
            return Err(anyhow!("Batch sizes must be greater than 0"));
        }

        if self.max_failure_rate <= 0.0 || self.max_failure_rate > 1.0 {
            return Err(anyhow!("MAX_FAILURE_RATE must be between 0 and 1"));
        }

        Ok(())
    }
}
