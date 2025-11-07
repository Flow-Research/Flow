use super::config::ChunkerConfig;
use super::content_detection::ContentDetector;
use super::types::*;

use text_splitter::{ChunkConfig, MarkdownSplitter, TextSplitter};
use tokenizers::Tokenizer;
use tracing::{debug, info, instrument, warn};
use unicode_segmentation::UnicodeSegmentation;

pub struct Chunker {
    config: ChunkerConfig,
    tokenizer: Tokenizer,
}

impl Chunker {
    /// Create a new knowledge graph chunker
    ///
    /// # Errors
    /// Returns error if configuration is invalid or tokenizer fails to load
    #[instrument(skip(config))]
    pub fn new(config: ChunkerConfig) -> Result<Self> {
        config.validate()?;

        info!("Initializing Chunker");

        // Load tokenizer for token counting
        let tokenizer = Tokenizer::from_pretrained("bert-base-uncased", None).map_err(|e| {
            ChunkerError::TokenizationError(format!("Failed to load tokenizer: {}", e))
        })?;

        debug!(
            max_chunk_size = config.max_chunk_size,
            overlap = config.overlap,
            "Chunker initialized"
        );

        Ok(Self { config, tokenizer })
    }

    /// Chunk text into processable segments
    ///
    /// # Arguments
    /// * `text` - The text to chunk
    ///
    /// # Returns
    /// Vector of chunks with metadata
    ///
    /// # Errors
    /// Returns error if text processing fails
    #[instrument(skip(self, text), fields(text_len = text.len()))]
    pub fn chunk(&self, text: &str) -> Result<Vec<Chunk>> {
        self.validate_input(text)?;

        if text.is_empty() {
            debug!("Empty text provided, returning empty chunks");
            return Ok(Vec::new());
        }

        // Detect content type
        let content_type = self
            .config
            .force_content_type
            .unwrap_or_else(|| ContentDetector::detect(text));

        info!(content_type = %content_type, "Detected content type");

        // Select strategy
        let strategy = self
            .config
            .force_strategy
            .unwrap_or_else(|| Self::select_strategy(content_type));

        info!(strategy = %strategy, "Selected chunking strategy");

        // Perform chunking
        let raw_chunks = self.chunk_with_strategy(text, strategy, content_type)?;

        if raw_chunks.is_empty() {
            warn!("Chunking produced no results");
            return Ok(Vec::new());
        }

        // Post-process and enrich
        let processed_chunks = self.post_process(raw_chunks, text, content_type)?;

        info!(chunk_count = processed_chunks.len(), "Chunking complete");

        Ok(processed_chunks)
    }

    /// Select appropriate strategy based on content type
    fn select_strategy(content_type: ContentType) -> ChunkingStrategy {
        match content_type {
            ContentType::Code => ChunkingStrategy::FixedSize,
            ContentType::Markdown => ChunkingStrategy::Recursive,
            ContentType::Scientific => ChunkingStrategy::Paragraph,
            ContentType::PlainText => ChunkingStrategy::Sentence,
            ContentType::Structured => ChunkingStrategy::Recursive,
        }
    }

    /// Chunk text using specified strategy
    fn chunk_with_strategy(
        &self,
        text: &str,
        strategy: ChunkingStrategy,
        content_type: ContentType,
    ) -> Result<Vec<String>> {
        match strategy {
            ChunkingStrategy::FixedSize => self.chunk_fixed_size(text),
            ChunkingStrategy::Sentence => self.chunk_by_sentences(text),
            ChunkingStrategy::Paragraph => self.chunk_by_paragraphs(text),
            ChunkingStrategy::Recursive => {
                if content_type == ContentType::Markdown {
                    self.chunk_markdown(text)
                } else {
                    self.chunk_by_sentences(text)
                }
            }
        }
    }

    /// Fixed-size chunking
    fn chunk_fixed_size(&self, text: &str) -> Result<Vec<String>> {
        let chunk_config = ChunkConfig::new(self.config.max_chunk_size)
            .with_overlap(self.config.overlap)
            .map_err(|e| ChunkerError::SplittingError(format!("Invalid chunk config: {}", e)))?;

        let splitter = TextSplitter::new(chunk_config);

        let chunks: Vec<String> = splitter.chunks(text).map(|s| s.to_string()).collect();

        debug!(chunk_count = chunks.len(), "Fixed-size chunking complete");
        Ok(chunks)
    }

    /// Sentence-aware chunking
    fn chunk_by_sentences(&self, text: &str) -> Result<Vec<String>> {
        let chunk_config = ChunkConfig::new(self.config.max_chunk_size)
            .with_overlap(self.config.overlap)
            .map_err(|e| ChunkerError::SplittingError(format!("Invalid chunk config: {}", e)))?;

        let splitter = TextSplitter::new(chunk_config);

        let chunks: Vec<String> = splitter.chunks(text).map(|s| s.to_string()).collect();

        debug!(
            chunk_count = chunks.len(),
            "Sentence-based chunking complete"
        );
        Ok(chunks)
    }

    /// Paragraph-aware chunking
    fn chunk_by_paragraphs(&self, text: &str) -> Result<Vec<String>> {
        let chunk_config = ChunkConfig::new(self.config.max_chunk_size)
            .with_overlap(self.config.overlap)
            .map_err(|e| ChunkerError::SplittingError(format!("Invalid chunk config: {}", e)))?;

        // Split by double newlines (paragraph boundaries)
        let paragraphs: Vec<&str> = text
            .split("\n\n")
            .map(|p| p.trim())
            .filter(|p| !p.is_empty())
            .collect();

        if paragraphs.is_empty() {
            return Ok(Vec::new());
        }

        let mut all_chunks = Vec::new();
        let splitter = TextSplitter::new(chunk_config);

        for (_, paragraph) in paragraphs.iter().enumerate() {
            let para_len = paragraph.len();

            if para_len <= self.config.max_chunk_size {
                // Paragraph fits in one chunk - keep it whole
                all_chunks.push(paragraph.to_string());
            } else {
                // Paragraph is too large - split it with sentence awareness
                let para_chunks: Vec<String> =
                    splitter.chunks(paragraph).map(|s| s.to_string()).collect();
                all_chunks.extend(para_chunks);
            }
        }

        debug!(
            paragraph_count = paragraphs.len(),
            chunk_count = all_chunks.len(),
            "Paragraph-based chunking complete"
        );

        Ok(all_chunks)
    }

    /// Markdown-aware chunking
    fn chunk_markdown(&self, text: &str) -> Result<Vec<String>> {
        let chunk_config = ChunkConfig::new(self.config.max_chunk_size)
            .with_overlap(self.config.overlap)
            .map_err(|e| ChunkerError::SplittingError(format!("Invalid chunk config: {}", e)))?;

        let splitter = MarkdownSplitter::new(chunk_config);

        let chunks: Vec<String> = splitter.chunks(text).map(|s| s.to_string()).collect();

        debug!(
            chunk_count = chunks.len(),
            "Markdown-aware chunking complete"
        );
        Ok(chunks)
    }

    /// Post-process raw chunks into enriched Chunk structs
    /// Post-process raw chunks into enriched Chunk structs
    fn post_process(
        &self,
        raw_chunks: Vec<String>,
        original_text: &str,
        content_type: ContentType,
    ) -> Result<Vec<Chunk>> {
        let mut chunks = Vec::with_capacity(raw_chunks.len());

        // Detect language once if enabled
        let language = if self.config.detect_language {
            ContentDetector::detect_language(original_text)
        } else {
            None
        };

        // Track search position to handle overlaps and duplicates
        let mut search_from = 0;
        let mut chunk_id = 0;

        for raw_text in raw_chunks {
            // Handle trimming AFTER recording position
            let text_for_search = if self.config.trim {
                raw_text.trim()
            } else {
                &raw_text
            };

            // Skip empty chunks early
            if text_for_search.is_empty() {
                continue;
            }

            // Find position, searching from last known position
            let start_char = original_text[search_from..]
                .find(text_for_search)
                .map(|pos| search_from + pos)
                .unwrap_or_else(|| {
                    // Fallback: fuzzy match if exact match fails
                    tracing::warn!(
                        chunk_id,
                        "Could not find exact chunk position, using approximate"
                    );
                    search_from
                });

            let end_char = start_char + text_for_search.len();

            // Update search position for next chunk
            // Account for overlap by only advancing to the end of non-overlapping portion
            search_from = start_char + (text_for_search.len() / 2).max(1);

            // Count tokens and sentences on the (possibly trimmed) text
            let token_count = self.count_tokens(text_for_search);
            let sentence_count = self.count_sentences(text_for_search);

            let metadata = ChunkMetadata {
                content_type: Some(content_type),
                language: language.clone(),
                embedding: None,
                vector_id: None,
                custom: None,
            };

            chunks.push(Chunk {
                text: text_for_search.to_string(),
                token_count,
                sentence_count,
                start_char,
                end_char,
                chunk_id,
                metadata,
            });

            chunk_id += 1;
        }

        Ok(chunks)
    }

    /// Count tokens in text
    fn count_tokens(&self, text: &str) -> usize {
        self.tokenizer
            .encode(text, false)
            .map(|e| e.len())
            .unwrap_or_else(|_| {
                // Fallback: rough estimate
                text.split_whitespace().count()
            })
    }

    /// Count sentences in text
    fn count_sentences(&self, text: &str) -> usize {
        text.unicode_sentences().count()
    }

    /// Validate input text before processing
    fn validate_input(&self, text: &str) -> Result<()> {
        // Check size limit
        if text.len() > self.config.max_input_size {
            return Err(ChunkerError::ValidationError(format!(
                "Input text too large: {} bytes (max: {} bytes)",
                text.len(),
                self.config.max_input_size
            )));
        }

        // Check for null characters (can cause issues in some contexts)
        if self.config.reject_null_chars && text.contains('\0') {
            return Err(ChunkerError::ValidationError(
                "Input contains null characters".to_string(),
            ));
        }

        // Warn if input is suspiciously small
        if text.len() > 0 && text.len() < 10 {
            tracing::debug!(
                text_len = text.len(),
                "Very short input text, proceeding with caution"
            );
        }

        // Check for excessive repetition
        if Self::has_excessive_repetition(text) {
            tracing::warn!(
                text_len = text.len(),
                "Input text has excessive repetition, may cause performance issues"
            );
            // Don't error, just warn - repetition is valid but suspicious
        }

        Ok(())
    }

    fn has_excessive_repetition(text: &str) -> bool {
        if text.len() < 1000 {
            return false; // Too short to matter
        }

        // Sample first 1000 chars
        let sample = &text[..1000.min(text.len())];
        let mut char_counts = std::collections::HashMap::new();

        for c in sample.chars() {
            *char_counts.entry(c).or_insert(0) += 1;
        }

        // If any single character appears > 80% of the time, flag it
        char_counts.values().any(|&count| count > 800)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_chunker() -> Chunker {
        Chunker::new(ChunkerConfig::default()).unwrap()
    }

    #[test]
    fn test_empty_text() {
        let chunker = setup_chunker();
        let chunks = chunker.chunk("").unwrap();
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_basic_chunking() {
        let chunker = setup_chunker();
        let text = "This is a test sentence. ".repeat(100);
        let chunks = chunker.chunk(&text).unwrap();

        assert!(!chunks.is_empty());
        assert!(chunks.iter().all(|c| !c.text.is_empty()));
        assert!(chunks.iter().all(|c| c.token_count > 0));
    }

    #[test]
    fn test_markdown_chunking() {
        let config = ChunkerConfig::default().with_content_type(ContentType::Markdown);
        let chunker = Chunker::new(config).unwrap();

        let markdown = r#"
# Title

Some content here.

## Section 1

More content.

## Section 2

Final content.
        "#;

        let chunks = chunker.chunk(markdown).unwrap();

        println!("Chunk count {}", chunks.len());

        assert!(chunks.len() >= 1);
        assert!(
            chunks
                .iter()
                .all(|c| c.metadata.content_type == Some(ContentType::Markdown))
        );
    }

    #[test]
    fn test_chunk_metadata() {
        let chunker = setup_chunker();
        let text = "This is a test. Another sentence here.";
        let chunks = chunker.chunk(text).unwrap();

        assert!(!chunks.is_empty());
        let chunk = &chunks[0];

        assert!(chunk.token_count > 0);
        assert!(chunk.sentence_count > 0);
        assert!(chunk.metadata.content_type.is_some());
        assert!(chunk.start_char < chunk.end_char);
    }

    #[test]
    fn test_sequential_chunk_ids() {
        let chunker = setup_chunker();
        let text = "Sentence one. ".repeat(50);
        let chunks = chunker.chunk(&text).unwrap();

        for (idx, chunk) in chunks.iter().enumerate() {
            assert_eq!(chunk.chunk_id, idx);
        }
    }
}
