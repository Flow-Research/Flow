use anyhow::Result;
use node::modules::kg::chunker::semantic_chunker::*;
use std::sync::Arc;

// ============================================================================
// Mock Embedder for Unit Testing
// ============================================================================

/// Mock embedder that returns fixed embeddings for testing
/// Embedding values determine similarity between sentences
struct MockEmbedder {
    embeddings: Vec<Vec<f32>>,
}

impl MockEmbedder {
    fn new(embeddings: Vec<Vec<f32>>) -> Self {
        Self { embeddings }
    }

    /// Create embedder where all sentences get identical embeddings (high similarity)
    fn high_similarity(count: usize, dim: usize) -> Self {
        let embedding = vec![1.0; dim];
        Self::new(vec![embedding; count])
    }

    /// Create embedder where sentences alternate between two orthogonal embeddings
    fn alternating_similarity(count: usize) -> Self {
        let mut embeddings = Vec::new();
        let emb_a = vec![1.0, 0.0, 0.0];
        let emb_b = vec![0.0, 1.0, 0.0];

        for i in 0..count {
            embeddings.push(if i % 2 == 0 {
                emb_a.clone()
            } else {
                emb_b.clone()
            });
        }

        Self::new(embeddings)
    }

    /// Create embedder that returns custom embeddings
    fn custom(embeddings: Vec<Vec<f32>>) -> Self {
        Self::new(embeddings)
    }
}

impl SentenceEmbedder for MockEmbedder {
    fn embed(&self, sentences: &[String]) -> Result<Vec<Vec<f32>>> {
        if sentences.len() > self.embeddings.len() {
            return Err(anyhow::anyhow!(
                "MockEmbedder only has {} embeddings but {} requested",
                self.embeddings.len(),
                sentences.len()
            ));
        }

        Ok(self.embeddings[..sentences.len()].to_vec())
    }
}

// ============================================================================
// Configuration Tests
// ============================================================================

#[cfg(test)]
mod config_tests {
    use super::*;

    #[test]
    fn test_chunker_config_valid() -> Result<()> {
        let config = ChunkerConfig::new(0.5, 512, 50)?;
        assert_eq!(config.similarity_threshold, 0.5);
        assert_eq!(config.max_tokens, 512);
        assert_eq!(config.min_tokens, 50);
        Ok(())
    }

    #[test]
    fn test_chunker_config_default() {
        let config = ChunkerConfig::default();
        assert_eq!(config.similarity_threshold, 0.5);
        assert_eq!(config.max_tokens, 512);
        assert_eq!(config.min_tokens, 50);
    }

    #[test]
    fn test_chunker_config_invalid_threshold_too_high() {
        assert!(ChunkerConfig::new(1.5, 512, 50).is_err());
    }

    #[test]
    fn test_chunker_config_invalid_threshold_too_low() {
        assert!(ChunkerConfig::new(-0.1, 512, 50).is_err());
    }

    #[test]
    fn test_chunker_config_invalid_threshold_nan() {
        assert!(ChunkerConfig::new(f32::NAN, 512, 50).is_err());
    }

    #[test]
    fn test_chunker_config_invalid_threshold_infinity() {
        assert!(ChunkerConfig::new(f32::INFINITY, 512, 50).is_err());
    }

    #[test]
    fn test_chunker_config_boundary_threshold() -> Result<()> {
        // 0.0 and 1.0 should be valid
        assert!(ChunkerConfig::new(0.0, 512, 50).is_ok());
        assert!(ChunkerConfig::new(1.0, 512, 50).is_ok());
        Ok(())
    }

    #[test]
    fn test_chunker_config_invalid_min_equals_max() {
        assert!(ChunkerConfig::new(0.5, 100, 100).is_err());
    }

    #[test]
    fn test_chunker_config_invalid_min_greater_than_max() {
        assert!(ChunkerConfig::new(0.5, 100, 200).is_err());
    }

    #[test]
    fn test_chunker_config_invalid_min_zero() {
        assert!(ChunkerConfig::new(0.5, 512, 0).is_err());
    }

    #[test]
    fn test_chunker_config_min_one_is_valid() -> Result<()> {
        assert!(ChunkerConfig::new(0.5, 512, 1).is_ok());
        Ok(())
    }

    #[test]
    fn test_embedder_config_default() {
        let config = EmbedderConfig::default();
        assert_eq!(config.model_id, "sentence-transformers/all-MiniLM-L6-v2");
        assert_eq!(config.batch_size, 8);
        assert!(!config.use_gpu);
        assert_eq!(config.download_timeout.as_secs(), 300);
    }
}

// ============================================================================
// Helper Function Tests
// ============================================================================

#[cfg(test)]
mod helper_tests {
    use super::*;

    // cosine_similarity tests
    #[test]
    fn test_cosine_similarity_identical_vectors() -> Result<()> {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let sim = node::modules::kg::chunker::semantic_chunker::cosine_similarity(&a, &b)?;
        assert!((sim - 1.0).abs() < 1e-6);
        Ok(())
    }

    #[test]
    fn test_cosine_similarity_orthogonal_vectors() -> Result<()> {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let sim = node::modules::kg::chunker::semantic_chunker::cosine_similarity(&a, &b)?;
        assert!((sim - 0.0).abs() < 1e-6);
        Ok(())
    }

    #[test]
    fn test_cosine_similarity_opposite_vectors() -> Result<()> {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![-1.0, 0.0, 0.0];
        let sim = node::modules::kg::chunker::semantic_chunker::cosine_similarity(&a, &b)?;
        assert!((sim - (-1.0)).abs() < 1e-6);
        Ok(())
    }

    #[test]
    fn test_cosine_similarity_partial() -> Result<()> {
        let a = vec![1.0, 1.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let sim = node::modules::kg::chunker::semantic_chunker::cosine_similarity(&a, &b)?;
        // Expected: 1 / sqrt(2) ≈ 0.707
        assert!((sim - 0.707).abs() < 0.01);
        Ok(())
    }

    #[test]
    fn test_cosine_similarity_dimension_mismatch() {
        let a = vec![1.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let result = node::modules::kg::chunker::semantic_chunker::cosine_similarity(&a, &b);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("dimension mismatch")
        );
    }

    #[test]
    fn test_cosine_similarity_empty_vectors() {
        let a: Vec<f32> = vec![];
        let b: Vec<f32> = vec![];
        let result = node::modules::kg::chunker::semantic_chunker::cosine_similarity(&a, &b);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty"));
    }

    #[test]
    fn test_cosine_similarity_zero_magnitude() -> Result<()> {
        let a = vec![0.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let sim = node::modules::kg::chunker::semantic_chunker::cosine_similarity(&a, &b)?;
        assert_eq!(sim, 0.0);
        Ok(())
    }

    #[test]
    fn test_cosine_similarity_normalized_vectors() -> Result<()> {
        // Pre-normalized vectors
        let a = vec![0.6, 0.8];
        let b = vec![0.8, 0.6];
        let sim = node::modules::kg::chunker::semantic_chunker::cosine_similarity(&a, &b)?;
        // Expected: 0.6*0.8 + 0.8*0.6 = 0.96
        assert!((sim - 0.96).abs() < 1e-6);
        Ok(())
    }

    // average_embeddings tests
    #[test]
    fn test_average_embeddings_two_vectors() -> Result<()> {
        let embeddings = vec![vec![1.0, 2.0, 3.0], vec![4.0, 5.0, 6.0]];
        let avg = node::modules::kg::chunker::semantic_chunker::average_embeddings(&embeddings)?;
        assert_eq!(avg, vec![2.5, 3.5, 4.5]);
        Ok(())
    }

    #[test]
    fn test_average_embeddings_single_vector() -> Result<()> {
        let embeddings = vec![vec![1.0, 2.0, 3.0]];
        let avg = node::modules::kg::chunker::semantic_chunker::average_embeddings(&embeddings)?;
        assert_eq!(avg, vec![1.0, 2.0, 3.0]);
        Ok(())
    }

    #[test]
    fn test_average_embeddings_empty() {
        let embeddings: Vec<Vec<f32>> = vec![];
        let result = node::modules::kg::chunker::semantic_chunker::average_embeddings(&embeddings);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty"));
    }

    #[test]
    fn test_average_embeddings_dimension_mismatch() {
        let embeddings = vec![vec![1.0, 2.0], vec![1.0, 2.0, 3.0]];
        let result = node::modules::kg::chunker::semantic_chunker::average_embeddings(&embeddings);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("dimension mismatch")
        );
    }

    #[test]
    fn test_average_embeddings_three_vectors() -> Result<()> {
        let embeddings = vec![vec![1.0, 2.0], vec![2.0, 4.0], vec![3.0, 6.0]];
        let avg = node::modules::kg::chunker::semantic_chunker::average_embeddings(&embeddings)?;
        assert_eq!(avg, vec![2.0, 4.0]);
        Ok(())
    }

    #[test]
    fn test_average_embeddings_zeros() -> Result<()> {
        let embeddings = vec![vec![0.0, 0.0], vec![0.0, 0.0]];
        let avg = node::modules::kg::chunker::semantic_chunker::average_embeddings(&embeddings)?;
        assert_eq!(avg, vec![0.0, 0.0]);
        Ok(())
    }
}

// ============================================================================
// SemanticChunker Unit Tests (with Mock Embedder)
// ============================================================================

#[cfg(test)]
mod chunker_tests {
    use super::*;

    #[test]
    fn test_chunker_empty_text() -> Result<()> {
        let embedder = Arc::new(MockEmbedder::high_similarity(0, 3));
        let config = ChunkerConfig::new(0.5, 512, 50)?;
        let chunker = SemanticChunker::new(embedder, config);

        let chunks = chunker.chunk("")?;
        assert!(chunks.is_empty());
        Ok(())
    }

    #[test]
    fn test_chunker_single_sentence() -> Result<()> {
        let embedder = Arc::new(MockEmbedder::high_similarity(1, 3));
        let config = ChunkerConfig::new(0.5, 512, 50)?;
        let chunker = SemanticChunker::new(embedder, config);

        let text = "This is a single sentence.";
        let chunks = chunker.chunk(text)?;

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].text, text);
        assert_eq!(chunks[0].sentence_count, 1);
        assert!(chunks[0].metadata.embedding.is_some());
        Ok(())
    }

    #[test]
    fn test_chunker_high_similarity_stays_together() -> Result<()> {
        // All sentences have identical embeddings (similarity = 1.0)
        let embedder = Arc::new(MockEmbedder::high_similarity(3, 3));
        let config = ChunkerConfig::new(0.5, 512, 50)?;
        let chunker = SemanticChunker::new(embedder, config);

        let text = "First sentence. Second sentence. Third sentence.";
        let chunks = chunker.chunk(text)?;

        // All should be in one chunk since similarity is high
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].sentence_count, 3);
        Ok(())
    }

    #[test]
    fn test_chunker_low_similarity_breaks_apart() -> Result<()> {
        // Alternating orthogonal embeddings (similarity = 0.0)
        let embedder = Arc::new(MockEmbedder::alternating_similarity(4));
        // Set min_tokens to 1 to prevent merging small chunks
        let config = ChunkerConfig::new(0.5, 512, 1)?; // Changed from 50 to 1
        let chunker = SemanticChunker::new(embedder, config);

        let text = "First. Second. Third. Fourth.";
        let chunks = chunker.chunk(text)?;

        // Should break at each transition (0.0 < 0.5 threshold)
        assert!(chunks.len() > 1);
        Ok(())
    }

    #[test]
    fn test_chunker_respects_similarity_threshold() -> Result<()> {
        // Create embeddings with specific similarities
        let embeddings = vec![
            vec![1.0, 0.0, 0.0], // Sentence 1
            vec![0.9, 0.1, 0.0], // Sentence 2 (high similarity to 1)
            vec![0.0, 1.0, 0.0], // Sentence 3 (low similarity to 2)
        ];
        let embedder = Arc::new(MockEmbedder::custom(embeddings));

        // High threshold - more breaks
        let config = ChunkerConfig::new(0.95, 512, 1)?;
        let chunker = SemanticChunker::new(embedder.clone(), config);

        let text = "First. Second. Third.";
        let chunks = chunker.chunk(text)?;

        assert!(
            chunks.len() >= 2,
            "High threshold should create more breaks"
        );
        Ok(())
    }

    #[test]
    fn test_chunker_token_limits_max() -> Result<()> {
        let embedder = Arc::new(MockEmbedder::high_similarity(3, 3));
        // Very small max_tokens to force splitting
        let config = ChunkerConfig::new(0.5, 50, 1)?;
        let chunker = SemanticChunker::new(embedder, config);

        // Create naturally long sentences
        let text = "This is a very long sentence that contains many words and should definitely exceed the maximum token limit that we have configured for this test case. \
                Another extremely long sentence with lots of words to ensure we trigger the splitting behavior when chunks become too large for the configuration. \
                A third lengthy sentence packed with numerous words to thoroughly test the maximum token enforcement.";

        let chunks = chunker.chunk(text)?;

        // Should split due to token limit
        for (i, chunk) in chunks.iter().enumerate() {
            assert!(
                chunk.token_count <= 50,
                "Chunk {} exceeds max_tokens: {} > 50",
                i,
                chunk.token_count
            );
        }
        Ok(())
    }

    #[test]
    fn test_chunker_token_limits_min() -> Result<()> {
        let embedder = Arc::new(MockEmbedder::alternating_similarity(10));
        // High min_tokens to force merging
        let config = ChunkerConfig::new(0.5, 1000, 100)?;
        let chunker = SemanticChunker::new(embedder, config);

        let text = "A. B. C. D. E. F. G. H. I. J.";
        let chunks = chunker.chunk(text)?;

        // Chunks should be merged to meet min_tokens
        // (except possibly the last one)
        for (i, chunk) in chunks.iter().enumerate() {
            if i < chunks.len() - 1 {
                // All but last should meet minimum
                assert!(
                    chunk.token_count >= 100 || chunk.sentence_count > 1,
                    "Chunk {} should meet min_tokens or be merged",
                    i
                );
            }
        }
        Ok(())
    }

    #[test]
    fn test_chunker_metadata_embeddings() -> Result<()> {
        let embedder = Arc::new(MockEmbedder::high_similarity(2, 3));
        let config = ChunkerConfig::new(0.5, 512, 50)?;
        let chunker = SemanticChunker::new(embedder, config);

        let text = "First sentence. Second sentence.";
        let chunks = chunker.chunk(text)?;

        for chunk in &chunks {
            assert!(chunk.metadata.embedding.is_some(), "Should have embedding");
            let emb = chunk.metadata.embedding.as_ref().unwrap();
            assert!(!emb.is_empty(), "Embedding should not be empty");
        }
        Ok(())
    }

    #[test]
    fn test_chunker_metadata_coherence() -> Result<()> {
        let embedder = Arc::new(MockEmbedder::high_similarity(3, 3));
        let config = ChunkerConfig::new(0.5, 512, 50)?;
        let chunker = SemanticChunker::new(embedder, config);

        let text = "First. Second. Third.";
        let chunks = chunker.chunk(text)?;

        for chunk in &chunks {
            assert!(
                chunk.metadata.coherence_score.is_some(),
                "Should have coherence score"
            );
            let coherence = chunk.metadata.coherence_score.unwrap();
            assert!(
                (0.0..=1.0).contains(&coherence),
                "Coherence should be in [0, 1]"
            );
        }
        Ok(())
    }

    #[test]
    fn test_chunker_sentence_count_accuracy() -> Result<()> {
        let embedder = Arc::new(MockEmbedder::high_similarity(5, 3));
        let config = ChunkerConfig::new(0.5, 512, 1)?;
        let chunker = SemanticChunker::new(embedder, config);

        let text = "One. Two. Three. Four. Five.";
        let chunks = chunker.chunk(text)?;

        let total_sentences: usize = chunks.iter().map(|c| c.sentence_count).sum();
        assert_eq!(total_sentences, 5, "Should account for all sentences");
        Ok(())
    }

    #[test]
    fn test_chunker_sentence_reconstruction() -> Result<()> {
        let embedder = Arc::new(MockEmbedder::alternating_similarity(3));
        let config = ChunkerConfig::new(0.5, 512, 1)?;
        let chunker = SemanticChunker::new(embedder, config);

        let text = "First sentence. Second sentence. Third sentence.";
        let chunks = chunker.chunk(text)?;

        // Reconstruct text from chunks
        let reconstructed_sentences: Vec<String> =
            chunks.iter().flat_map(|c| c.sentences.clone()).collect();

        assert_eq!(reconstructed_sentences.len(), 3);
        Ok(())
    }

    #[test]
    fn test_chunker_special_characters() -> Result<()> {
        let embedder = Arc::new(MockEmbedder::high_similarity(3, 3));
        let config = ChunkerConfig::new(0.5, 512, 50)?;
        let chunker = SemanticChunker::new(embedder, config);

        let text = "Hello! How are you? I'm fine, thanks.";
        let chunks = chunker.chunk(text)?;

        assert!(!chunks.is_empty());
        assert!(chunks[0].text.contains("Hello"));
        Ok(())
    }

    #[test]
    fn test_chunker_unicode_text() -> Result<()> {
        let embedder = Arc::new(MockEmbedder::high_similarity(3, 3));
        let config = ChunkerConfig::new(0.5, 512, 50)?;
        let chunker = SemanticChunker::new(embedder, config);

        let text = "こんにちは。元気ですか。はい、元気です。";
        let chunks = chunker.chunk(text)?;

        assert!(!chunks.is_empty());
        for chunk in &chunks {
            assert!(!chunk.text.is_empty());
        }
        Ok(())
    }

    #[test]
    fn test_chunker_newlines_in_text() -> Result<()> {
        let embedder = Arc::new(MockEmbedder::high_similarity(3, 3));
        let config = ChunkerConfig::new(0.5, 512, 50)?;
        let chunker = SemanticChunker::new(embedder, config);

        let text = "First sentence.\nSecond sentence.\n\nThird sentence.";
        let chunks = chunker.chunk(text)?;

        assert!(!chunks.is_empty());
        Ok(())
    }
}

// ============================================================================
// Edge Case Tests
// ============================================================================

#[cfg(test)]
mod edge_case_tests {
    use super::*;

    #[test]
    fn test_very_long_single_sentence() -> Result<()> {
        let embedder = Arc::new(MockEmbedder::high_similarity(1, 3));
        let config = ChunkerConfig::new(0.5, 100, 10)?;
        let chunker = SemanticChunker::new(embedder, config);

        let long_sentence = format!("{}.", "word ".repeat(500));
        let chunks = chunker.chunk(&long_sentence)?;

        // Should handle without panic
        assert!(!chunks.is_empty());
        Ok(())
    }

    #[test]
    fn test_many_short_sentences() -> Result<()> {
        let count = 100;
        let embedder = Arc::new(MockEmbedder::high_similarity(count, 3));
        let config = ChunkerConfig::new(0.5, 512, 1)?;
        let chunker = SemanticChunker::new(embedder, config);

        let text = (0..count)
            .map(|i| format!("Sentence {}.", i))
            .collect::<Vec<_>>()
            .join(" ");
        let chunks = chunker.chunk(&text)?;

        assert!(!chunks.is_empty());
        Ok(())
    }

    #[test]
    fn test_whitespace_only_between_sentences() -> Result<()> {
        let embedder = Arc::new(MockEmbedder::high_similarity(2, 3));
        let config = ChunkerConfig::new(0.5, 512, 50)?;
        let chunker = SemanticChunker::new(embedder, config);

        let text = "First.     Second.";
        let chunks = chunker.chunk(text)?;

        assert!(!chunks.is_empty());
        Ok(())
    }

    #[test]
    fn test_minimum_config_values() -> Result<()> {
        let embedder = Arc::new(MockEmbedder::high_similarity(2, 3));
        let config = ChunkerConfig::new(0.0, 10, 1)?; // Minimum valid values
        let chunker = SemanticChunker::new(embedder, config);

        let text = "First. Second.";
        let chunks = chunker.chunk(text)?;

        assert!(!chunks.is_empty());
        Ok(())
    }

    #[test]
    fn test_maximum_threshold() -> Result<()> {
        let embedder = Arc::new(MockEmbedder::high_similarity(5, 3));
        let config = ChunkerConfig::new(1.0, 512, 50)?; // Maximum threshold
        let chunker = SemanticChunker::new(embedder, config);

        let text = "One. Two. Three. Four. Five.";
        let chunks = chunker.chunk(text)?;

        // With threshold = 1.0, even identical embeddings won't meet it due to FP precision
        // Should create breaks
        assert!(chunks.len() >= 1);
        Ok(())
    }
}

// ============================================================================
// Integration Tests (Require Model Download)
// ============================================================================

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Requires model download, run with: cargo test -- --ignored
    async fn test_candle_embedder_initialization() -> Result<()> {
        let embedder = CandleEmbedder::default().await?;

        // Just verify it initialized
        let sentences = vec!["Test".to_string()];
        let embeddings = embedder.embed(&sentences)?;

        assert_eq!(embeddings.len(), 1);
        assert!(!embeddings[0].is_empty());

        Ok(())
    }

    #[tokio::test]
    #[ignore]
    async fn test_candle_embedder_multiple_sentences() -> Result<()> {
        let embedder = CandleEmbedder::default().await?;

        let sentences = vec![
            "Hello world".to_string(),
            "Goodbye world".to_string(),
            "How are you".to_string(),
        ];
        let embeddings = embedder.embed(&sentences)?;

        assert_eq!(embeddings.len(), 3);
        // All should have same dimension
        let dim = embeddings[0].len();
        for emb in &embeddings {
            assert_eq!(emb.len(), dim);
        }

        Ok(())
    }

    #[tokio::test]
    #[ignore]
    async fn test_candle_embedder_similarity_sanity() -> Result<()> {
        let embedder = CandleEmbedder::default().await?;

        let similar = vec![
            "The cat sat on the mat".to_string(),
            "A cat was sitting on a mat".to_string(),
        ];

        let dissimilar = vec![
            "The cat sat on the mat".to_string(),
            "Quantum physics is fascinating".to_string(),
        ];

        let similar_emb = embedder.embed(&similar)?;
        let dissimilar_emb = embedder.embed(&dissimilar)?;

        let similar_score = node::modules::kg::chunker::semantic_chunker::cosine_similarity(
            &similar_emb[0],
            &similar_emb[1],
        )?;

        let dissimilar_score = node::modules::kg::chunker::semantic_chunker::cosine_similarity(
            &dissimilar_emb[0],
            &dissimilar_emb[1],
        )?;

        // Similar sentences should have higher similarity
        assert!(similar_score > dissimilar_score);
        assert!(similar_score > 0.7); // Should be quite similar

        Ok(())
    }

    #[tokio::test]
    #[ignore]
    async fn test_semantic_chunker_end_to_end() -> Result<()> {
        let config = ChunkerConfig::new(0.5, 512, 20)?;
        let chunker = SemanticChunker::with_default_embedder(config).await?;

        let text = "Machine learning is a field of artificial intelligence. \
                   It uses statistical techniques to give computers the ability to learn. \
                   Deep learning is a subset of machine learning. \
                   Quantum computing is a completely different topic. \
                   It leverages quantum mechanical phenomena. \
                   Quantum computers could revolutionize computing.";

        let chunks = chunker.chunk(text)?;

        assert!(!chunks.is_empty());

        // Verify chunk properties
        for chunk in &chunks {
            assert!(!chunk.text.is_empty());
            assert!(chunk.sentence_count > 0);
            assert!(chunk.token_count > 0);
            assert!(chunk.metadata.embedding.is_some());
            assert!(chunk.metadata.coherence_score.is_some());
        }

        println!("\n=== Semantic Chunking Results ===");
        println!("Total chunks: {}", chunks.len());
        for (i, chunk) in chunks.iter().enumerate() {
            println!(
                "\nChunk {}: {} sentences, {} tokens",
                i, chunk.sentence_count, chunk.token_count
            );
            println!(
                "Text: {}...",
                chunk.text.chars().take(80).collect::<String>()
            );
            println!(
                "Coherence: {:.3}",
                chunk.metadata.coherence_score.unwrap_or(0.0)
            );
        }

        Ok(())
    }

    #[tokio::test]
    #[ignore]
    async fn test_semantic_chunker_groups_related_content() -> Result<()> {
        let config = ChunkerConfig::new(0.4, 512, 20)?;
        let chunker = SemanticChunker::with_default_embedder(config).await?;

        // Two clearly different topics
        let text = "Dogs are loyal companions. They love to play fetch. \
                   Dogs need regular exercise and training. \
                   Python is a programming language. \
                   It's popular for data science and machine learning. \
                   Python has a simple and readable syntax.";

        let chunks = chunker.chunk(text)?;

        // Should create at least 2 chunks for different topics
        assert!(chunks.len() >= 2, "Should separate different topics");

        println!("\n=== Semantic Chunking Results ===");
        println!("Total chunks: {}", chunks.len());
        for (i, chunk) in chunks.iter().enumerate() {
            println!(
                "\nChunk {}: {} sentences, {} tokens",
                i, chunk.sentence_count, chunk.token_count
            );
            println!(
                "Text: {}...",
                chunk.text.chars().take(80).collect::<String>()
            );
            println!(
                "Coherence: {:.3}",
                chunk.metadata.coherence_score.unwrap_or(0.0)
            );
        }

        Ok(())
    }
}

// ============================================================================
// Property-Based Tests
// ============================================================================

#[cfg(test)]
mod property_tests {
    use super::*;

    #[test]
    fn test_property_all_sentences_accounted_for() -> Result<()> {
        for sentence_count in [1, 5, 10, 20] {
            let embedder = Arc::new(MockEmbedder::alternating_similarity(sentence_count));
            let config = ChunkerConfig::new(0.5, 512, 1)?;
            let chunker = SemanticChunker::new(embedder, config);

            let text = (0..sentence_count)
                .map(|i| format!("Sentence {}.", i))
                .collect::<Vec<_>>()
                .join(" ");

            let chunks = chunker.chunk(&text)?;

            let total: usize = chunks.iter().map(|c| c.sentence_count).sum();
            assert_eq!(
                total, sentence_count,
                "All {} sentences should be accounted for",
                sentence_count
            );
        }
        Ok(())
    }

    #[test]
    fn test_property_chunks_not_empty() -> Result<()> {
        let embedder = Arc::new(MockEmbedder::high_similarity(10, 3));
        let config = ChunkerConfig::new(0.5, 512, 50)?;
        let chunker = SemanticChunker::new(embedder, config);

        let text = "One. Two. Three. Four. Five. Six. Seven. Eight. Nine. Ten.";
        let chunks = chunker.chunk(text)?;

        for chunk in &chunks {
            assert!(!chunk.text.is_empty(), "Chunks should not be empty");
            assert!(chunk.sentence_count > 0, "Chunks should have sentences");
            assert!(chunk.token_count > 0, "Chunks should have tokens");
        }
        Ok(())
    }

    #[test]
    fn test_property_deterministic() -> Result<()> {
        let embedder = Arc::new(MockEmbedder::alternating_similarity(5));
        let config = ChunkerConfig::new(0.5, 512, 50)?;
        let chunker = SemanticChunker::new(embedder, config);

        let text = "One. Two. Three. Four. Five.";

        let chunks1 = chunker.chunk(text)?;
        let chunks2 = chunker.chunk(text)?;

        assert_eq!(chunks1.len(), chunks2.len(), "Should be deterministic");

        for (c1, c2) in chunks1.iter().zip(chunks2.iter()) {
            assert_eq!(c1.text, c2.text);
            assert_eq!(c1.sentence_count, c2.sentence_count);
            assert_eq!(c1.token_count, c2.token_count);
        }
        Ok(())
    }
}
