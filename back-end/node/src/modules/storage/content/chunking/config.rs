use tracing::debug;

/// Configuration for chunking algorithms.
///
/// Controls the size distribution of chunks produced by any chunker.
/// Content-defined chunkers (FastCDC, Rabin) use these as guidelines,
/// while fixed chunkers use `target_size` directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkingConfig {
    /// Minimum chunk size in bytes. Chunks won't be smaller than this.
    pub min_size: usize,
    /// Target/average chunk size in bytes.
    pub target_size: usize,
    /// Maximum chunk size in bytes. Force split if exceeded.
    pub max_size: usize,
}

impl Default for ChunkingConfig {
    fn default() -> Self {
        Self {
            min_size: 64 * 1024,     // 64 KB
            target_size: 256 * 1024, // 256 KB
            max_size: 1024 * 1024,   // 1 MB
        }
    }
}

impl ChunkingConfig {
    /// Create a new configuration with specified sizes.
    ///
    /// # Panics
    ///
    /// Panics if `min_size > target_size` or `target_size > max_size`.
    pub fn new(min_size: usize, target_size: usize, max_size: usize) -> Self {
        assert!(
            min_size <= target_size,
            "min_size ({}) must be <= target_size ({})",
            min_size,
            target_size
        );
        assert!(
            target_size <= max_size,
            "target_size ({}) must be <= max_size ({})",
            target_size,
            max_size
        );

        Self {
            min_size,
            target_size,
            max_size,
        }
    }

    /// Create configuration for small chunks (good for dedup testing).
    pub fn small() -> Self {
        Self {
            min_size: 1024,        // 1 KB
            target_size: 4 * 1024, // 4 KB
            max_size: 16 * 1024,   // 16 KB
        }
    }

    /// Create configuration for large chunks (good for large files).
    pub fn large() -> Self {
        Self {
            min_size: 256 * 1024,      // 256 KB
            target_size: 1024 * 1024,  // 1 MB
            max_size: 4 * 1024 * 1024, // 4 MB
        }
    }

    /// Load configuration from environment variables.
    ///
    /// - `CHUNKING_MIN_SIZE`: Minimum chunk size (default: 65536)
    /// - `CHUNKING_TARGET_SIZE`: Target chunk size (default: 262144)
    /// - `CHUNKING_MAX_SIZE`: Maximum chunk size (default: 1048576)
    pub fn from_env() -> Self {
        let min_size = std::env::var("CHUNKING_MIN_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(64 * 1024);

        let target_size = std::env::var("CHUNKING_TARGET_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(256 * 1024);

        let max_size = std::env::var("CHUNKING_MAX_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1024 * 1024);

        debug!(
            min_size = min_size,
            target_size = target_size,
            max_size = max_size,
            "Loaded chunking config from environment"
        );

        // Validate and clamp if necessary
        let min_size = min_size.max(1024); // At least 1KB
        let target_size = target_size.max(min_size);
        let max_size = max_size.max(target_size);

        Self {
            min_size,
            target_size,
            max_size,
        }
    }

    /// Validate that the configuration is sensible.
    pub fn validate(&self) -> Result<(), String> {
        if self.min_size == 0 {
            return Err("min_size must be greater than 0".to_string());
        }
        if self.min_size > self.target_size {
            return Err(format!(
                "min_size ({}) must be <= target_size ({})",
                self.min_size, self.target_size
            ));
        }
        if self.target_size > self.max_size {
            return Err(format!(
                "target_size ({}) must be <= max_size ({})",
                self.target_size, self.max_size
            ));
        }
        Ok(())
    }
}
