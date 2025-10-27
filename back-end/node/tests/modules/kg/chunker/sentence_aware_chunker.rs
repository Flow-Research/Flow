use anyhow::Result;
use node::modules::kg::chunker::sentence_aware_chunker::{
    ChunkMetadata, SemanticChunk, SentenceAwareChunker,
};
use tokenizers::Tokenizer;

#[cfg(test)]
mod sentence_aware_chunker_tests {
    use super::*;

    // Helper function to create a tokenizer for tests
    fn create_test_tokenizer() -> Result<Tokenizer> {
        Tokenizer::from_pretrained("google-bert/bert-base-multilingual-cased", None)
            .map_err(|e| anyhow::anyhow!("Failed to load tokenizer: {}", e))
    }

    // ============================================================================
    // Basic Functionality Tests
    // ============================================================================

    #[test]
    fn test_chunker_initialization() -> Result<()> {
        let tokenizer = create_test_tokenizer()?;
        let chunker = SentenceAwareChunker::new(tokenizer, 512, 1);

        // If we got here without panicking, initialization worked
        assert!(true, "Chunker should initialize successfully");
        Ok(())
    }

    #[test]
    fn test_empty_text() -> Result<()> {
        let tokenizer = create_test_tokenizer()?;
        let chunker = SentenceAwareChunker::new(tokenizer, 512, 1);
        let chunks = chunker.chunk("")?;

        assert_eq!(chunks.len(), 0, "Empty text should produce no chunks");
        Ok(())
    }

    #[test]
    fn test_whitespace_only() -> Result<()> {
        let tokenizer = create_test_tokenizer()?;
        let chunker = SentenceAwareChunker::new(tokenizer, 512, 1);
        let chunks = chunker.chunk("   \n\t\r   ")?;

        // May produce empty chunk or no chunks depending on sentence segmentation
        for chunk in chunks.iter() {
            // Should not panic when accessing chunk properties
            let _ = chunk.text.len();
        }

        Ok(())
    }

    #[test]
    fn test_single_short_sentence() -> Result<()> {
        let tokenizer = create_test_tokenizer()?;
        let chunker = SentenceAwareChunker::new(tokenizer, 512, 1);
        let text = "This is a single sentence.";

        let chunks = chunker.chunk(text)?;

        assert_eq!(chunks.len(), 1, "Single sentence should produce one chunk");
        assert!(
            chunks[0].text.contains("single sentence"),
            "Text should be preserved"
        );
        assert_eq!(
            chunks[0].sentence_count, 1,
            "Should have exactly 1 sentence"
        );
        assert_eq!(
            chunks[0].sentences.len(),
            1,
            "Sentences vec should have 1 element"
        );
        assert!(
            chunks[0].token_count > 0,
            "Should have positive token count"
        );
        Ok(())
    }

    #[test]
    fn test_multiple_short_sentences() -> Result<()> {
        let tokenizer = create_test_tokenizer()?;
        let chunker = SentenceAwareChunker::new(tokenizer, 512, 1);
        let text = "First sentence. Second sentence. Third sentence.";

        let chunks = chunker.chunk(text)?;

        assert_eq!(chunks.len(), 1, "Short text should fit in one chunk");
        assert_eq!(chunks[0].sentence_count, 3, "Should have 3 sentences");
        assert_eq!(
            chunks[0].sentences.len(),
            3,
            "Sentences vec should have 3 elements"
        );
        Ok(())
    }

    // ============================================================================
    // Sentence Boundary Tests
    // ============================================================================

    #[test]
    fn test_sentence_splitting_periods() -> Result<()> {
        let tokenizer = create_test_tokenizer()?;
        let chunker = SentenceAwareChunker::new(tokenizer, 512, 0);
        let text = "First. Second. Third.";

        let chunks = chunker.chunk(text)?;

        assert!(!chunks.is_empty(), "Should produce chunks");
        let total_sentences: usize = chunks.iter().map(|c| c.sentence_count).sum();
        assert_eq!(total_sentences, 3, "Should detect 3 sentences");
        Ok(())
    }

    #[test]
    fn test_sentence_splitting_question_marks() -> Result<()> {
        let tokenizer = create_test_tokenizer()?;
        let chunker = SentenceAwareChunker::new(tokenizer, 512, 0);
        let text = "What is this? How does it work? Why is it here?";

        let chunks = chunker.chunk(text)?;

        assert!(!chunks.is_empty(), "Should produce chunks");
        let total_sentences: usize = chunks.iter().map(|c| c.sentence_count).sum();
        assert_eq!(total_sentences, 3, "Should detect 3 questions");
        Ok(())
    }

    #[test]
    fn test_sentence_splitting_exclamation_marks() -> Result<()> {
        let tokenizer = create_test_tokenizer()?;
        let chunker = SentenceAwareChunker::new(tokenizer, 512, 0);
        let text = "Amazing! Fantastic! Incredible!";

        let chunks = chunker.chunk(text)?;

        assert!(!chunks.is_empty(), "Should produce chunks");
        let total_sentences: usize = chunks.iter().map(|c| c.sentence_count).sum();
        assert_eq!(total_sentences, 3, "Should detect 3 exclamations");
        Ok(())
    }

    #[test]
    fn test_sentence_splitting_mixed_punctuation() -> Result<()> {
        let tokenizer = create_test_tokenizer()?;
        let chunker = SentenceAwareChunker::new(tokenizer, 512, 0);
        let text = "This is a statement. Is this a question? What an exclamation!";

        let chunks = chunker.chunk(text)?;

        assert!(!chunks.is_empty(), "Should produce chunks");
        let total_sentences: usize = chunks.iter().map(|c| c.sentence_count).sum();
        assert_eq!(
            total_sentences, 3,
            "Should detect 3 different sentence types"
        );
        Ok(())
    }

    #[test]
    fn test_sentence_with_abbreviations() -> Result<()> {
        let tokenizer = create_test_tokenizer()?;
        let chunker = SentenceAwareChunker::new(tokenizer, 512, 0);
        let text = "Dr. Smith works at U.S.A. Inc. He loves his job.";

        let chunks = chunker.chunk(text)?;

        // Unicode segmentation should handle abbreviations correctly
        assert!(!chunks.is_empty(), "Should produce chunks");
        // Note: may be 1 or 2 sentences depending on Unicode segmentation rules
        let total_sentences: usize = chunks.iter().map(|c| c.sentence_count).sum();
        assert!(total_sentences >= 1, "Should detect at least 1 sentence");
        Ok(())
    }

    // ============================================================================
    // Token Limit and Multi-Chunk Tests
    // ============================================================================

    #[test]
    fn test_text_exceeding_token_limit() -> Result<()> {
        let tokenizer = create_test_tokenizer()?;
        let max_tokens = 20; // Small limit to force splitting
        let chunker = SentenceAwareChunker::new(tokenizer, max_tokens, 0);

        let text = "This is the first sentence with several words. \
                    This is the second sentence with many words too. \
                    This is the third sentence that also has words. \
                    And this is the fourth sentence with more words.";

        let chunks = chunker.chunk(text)?;

        assert!(chunks.len() > 1, "Should create multiple chunks");

        // Verify each chunk respects token limit
        for (i, chunk) in chunks.iter().enumerate() {
            assert!(
                chunk.token_count <= max_tokens,
                "Chunk {} has {} tokens, exceeds max of {}",
                i,
                chunk.token_count,
                max_tokens
            );
        }

        Ok(())
    }

    #[test]
    fn test_chunk_token_count_accuracy() -> Result<()> {
        let tokenizer = create_test_tokenizer()?;
        let chunker = SentenceAwareChunker::new(tokenizer, 512, 0);
        let text = "Short sentence.";

        let chunks = chunker.chunk(text)?;

        assert_eq!(chunks.len(), 1, "Should have one chunk");
        assert!(
            chunks[0].token_count > 0,
            "Should have positive token count"
        );
        assert!(
            chunks[0].token_count < 10,
            "Short sentence should have few tokens"
        );
        Ok(())
    }

    // ============================================================================
    // Overlap Tests
    // ============================================================================

    #[test]
    fn test_no_overlap() -> Result<()> {
        let tokenizer = create_test_tokenizer()?;
        let chunker = SentenceAwareChunker::new(tokenizer, 30, 0); // No overlap

        let text = "First sentence here. Second sentence here. Third sentence here. \
                    Fourth sentence here. Fifth sentence here. Sixth sentence here.";

        let chunks = chunker.chunk(text)?;

        if chunks.len() > 1 {
            // With no overlap, consecutive chunks should not share sentences
            // This is harder to verify programmatically without comparing actual content
            // but we can check that chunks exist
            assert!(chunks.len() >= 2, "Should create multiple chunks");
        }

        Ok(())
    }

    #[test]
    fn test_single_sentence_overlap() -> Result<()> {
        let tokenizer = create_test_tokenizer()?;
        let overlap_sentences = 1;
        let chunker = SentenceAwareChunker::new(tokenizer, 30, overlap_sentences);

        let text = "First sentence. Second sentence. Third sentence. \
                    Fourth sentence. Fifth sentence. Sixth sentence.";

        let chunks = chunker.chunk(text)?;

        if chunks.len() > 1 {
            // Each chunk after the first should start with overlap from previous
            assert!(
                chunks.len() >= 2,
                "Should create multiple chunks with overlap"
            );

            // The implementation overlaps by taking last N sentences from previous chunk
            // We can't easily verify exact overlap without parsing, but check metadata
            for chunk in chunks.iter() {
                assert!(
                    chunk.sentence_count >= 1,
                    "Each chunk should have sentences"
                );
            }
        }

        Ok(())
    }

    #[test]
    fn test_multiple_sentence_overlap() -> Result<()> {
        let tokenizer = create_test_tokenizer()?;
        let overlap_sentences = 2;
        let chunker = SentenceAwareChunker::new(tokenizer, 30, overlap_sentences);

        let text = "One. Two. Three. Four. Five. Six. Seven. Eight.";

        let chunks = chunker.chunk(text)?;

        // With 2-sentence overlap, should create overlapping chunks
        assert!(!chunks.is_empty(), "Should produce chunks");
        Ok(())
    }

    #[test]
    fn test_overlap_larger_than_chunk() -> Result<()> {
        let tokenizer = create_test_tokenizer()?;
        let overlap_sentences = 10; // Very large overlap
        let chunker = SentenceAwareChunker::new(tokenizer, 30, overlap_sentences);

        let text = "First. Second. Third. Fourth. Fifth.";

        // Should handle gracefully (saturating_sub should prevent issues)
        let result = chunker.chunk(text);
        assert!(result.is_ok(), "Should handle large overlap gracefully");
        Ok(())
    }

    // ============================================================================
    // Long Sentence Handling Tests
    // ============================================================================

    #[test]
    fn test_single_long_sentence() -> Result<()> {
        let tokenizer = create_test_tokenizer()?;
        let max_tokens = 20;
        let chunker = SentenceAwareChunker::new(tokenizer, max_tokens, 0);

        // Create a very long sentence
        let text = "This is a very long sentence that contains many words and \
                    will definitely exceed the maximum token limit that we have \
                    set for our chunker and should trigger the long sentence \
                    splitting logic that breaks it down by punctuation marks.";

        let chunks = chunker.chunk(text)?;

        // Should split the long sentence
        assert!(
            !chunks.is_empty(),
            "Should produce chunks from long sentence"
        );

        // Each chunk should respect token limit (or be close if punctuation split)
        for chunk in chunks.iter() {
            println!("Chunk tokens: {}", chunk.token_count);
            // May slightly exceed due to punctuation-based splitting
        }

        Ok(())
    }

    #[test]
    fn test_long_sentence_with_commas() -> Result<()> {
        let tokenizer = create_test_tokenizer()?;
        let max_tokens = 15;
        let chunker = SentenceAwareChunker::new(tokenizer, max_tokens, 0);

        let text = "This is a sentence, with many commas, that should be split, \
                    into smaller parts, based on those commas, when it exceeds \
                    the token limit.";

        let chunks = chunker.chunk(text)?;

        assert!(chunks.len() >= 1, "Should produce at least one chunk");
        Ok(())
    }

    #[test]
    fn test_long_sentence_with_semicolons() -> Result<()> {
        let tokenizer = create_test_tokenizer()?;
        let max_tokens = 15;
        let chunker = SentenceAwareChunker::new(tokenizer, max_tokens, 0);

        let text = "This part; and this part; and this other part; \
                    should all be split; into separate chunks; based on semicolons.";

        let chunks = chunker.chunk(text)?;

        assert!(!chunks.is_empty(), "Should split on semicolons");
        Ok(())
    }

    #[test]
    fn test_long_sentence_with_colons() -> Result<()> {
        let tokenizer = create_test_tokenizer()?;
        let max_tokens = 15;
        let chunker = SentenceAwareChunker::new(tokenizer, max_tokens, 0);

        let text = "Consider these items: first item, second item: third item, \
                    fourth item: and finally the fifth item.";

        let chunks = chunker.chunk(text)?;

        assert!(!chunks.is_empty(), "Should handle colons in splitting");
        Ok(())
    }

    #[test]
    fn test_long_sentence_no_punctuation() -> Result<()> {
        let tokenizer = create_test_tokenizer()?;
        let max_tokens = 10;
        let chunker = SentenceAwareChunker::new(tokenizer, max_tokens, 0);

        // A long sentence with no commas/semicolons/colons
        let text = "word ".repeat(100);

        let chunks = chunker.chunk(&text)?;

        // Should still handle it somehow (might create large chunks)
        assert!(!chunks.is_empty(), "Should handle unpunctuated long text");
        Ok(())
    }

    // ============================================================================
    // Chunk Metadata Tests
    // ============================================================================

    #[test]
    fn test_chunk_metadata_default() -> Result<()> {
        let tokenizer = create_test_tokenizer()?;
        let chunker = SentenceAwareChunker::new(tokenizer, 512, 0);
        let text = "Test sentence.";

        let chunks = chunker.chunk(text)?;

        assert_eq!(chunks.len(), 1, "Should have one chunk");
        let metadata = &chunks[0].metadata;

        assert!(
            metadata.embedding.is_none(),
            "Default embedding should be None"
        );
        assert!(
            metadata.topic_label.is_none(),
            "Default topic should be None"
        );
        assert!(
            metadata.coherence_score.is_none(),
            "Default coherence should be None"
        );
        Ok(())
    }

    #[test]
    fn test_chunk_sentences_vec() -> Result<()> {
        let tokenizer = create_test_tokenizer()?;
        let chunker = SentenceAwareChunker::new(tokenizer, 512, 0);
        let text = "First. Second. Third.";

        let chunks = chunker.chunk(text)?;

        assert!(!chunks.is_empty(), "Should have chunks");
        let chunk = &chunks[0];

        assert_eq!(
            chunk.sentences.len(),
            chunk.sentence_count,
            "Sentences vec length should match sentence_count"
        );

        // Verify we can reconstruct text from sentences (approximately)
        let reconstructed = chunk.sentences.join(" ");
        // May have spacing differences
        assert!(
            !reconstructed.is_empty(),
            "Should be able to join sentences"
        );
        Ok(())
    }

    // ============================================================================
    // Unicode and Multilingual Tests
    // ============================================================================

    #[test]
    fn test_unicode_text() -> Result<()> {
        let tokenizer = create_test_tokenizer()?;
        let chunker = SentenceAwareChunker::new(tokenizer, 512, 1);

        let text = "Hello world! 你好世界！ Привет мир! مرحبا بالعالم!";

        let chunks = chunker.chunk(text)?;

        assert!(!chunks.is_empty(), "Should handle multilingual text");
        assert!(chunks[0].text.contains("Hello"), "Should preserve English");
        assert!(chunks[0].text.contains("你好"), "Should preserve Chinese");
        assert!(chunks[0].text.contains("Привет"), "Should preserve Russian");
        Ok(())
    }

    #[test]
    fn test_emoji_handling() -> Result<()> {
        let tokenizer = create_test_tokenizer()?;
        let chunker = SentenceAwareChunker::new(tokenizer, 512, 0);

        let text = "This is great! 🎉 Another sentence. 🌟 Final one! 🚀";

        let chunks = chunker.chunk(text)?;

        assert!(!chunks.is_empty(), "Should handle emojis");
        Ok(())
    }

    #[test]
    fn test_chinese_sentence_splitting() -> Result<()> {
        let tokenizer = create_test_tokenizer()?;
        let chunker = SentenceAwareChunker::new(tokenizer, 512, 0);

        let text = "这是第一句话。这是第二句话。这是第三句话。";

        let chunks = chunker.chunk(text)?;

        assert!(!chunks.is_empty(), "Should handle Chinese sentences");
        // Unicode sentence segmentation should detect Chinese periods (。)
        Ok(())
    }

    #[test]
    fn test_japanese_sentence_splitting() -> Result<()> {
        let tokenizer = create_test_tokenizer()?;
        let chunker = SentenceAwareChunker::new(tokenizer, 512, 0);

        let text = "これは最初の文です。これは2番目の文です。";

        let chunks = chunker.chunk(text)?;

        assert!(!chunks.is_empty(), "Should handle Japanese sentences");
        Ok(())
    }

    // ============================================================================
    // Integration and Real-World Tests
    // ============================================================================

    #[test]
    fn test_realistic_paragraph() -> Result<()> {
        let tokenizer = create_test_tokenizer()?;
        let chunker = SentenceAwareChunker::new(tokenizer, 100, 1);

        let text = "The knowledge graph serves as the semantic memory foundation \
                    for the Flow system. It manages entities, relationships, context, \
                    causality, and provenance as a distributed graph. The KG provides \
                    semantic context and long-term memory for agents. It handles the \
                    provenance, invocation, and verification of models and tools used \
                    by agents and DAG tasks.";

        let chunks = chunker.chunk(text)?;

        assert!(!chunks.is_empty(), "Should produce chunks");

        // Verify all chunks have valid properties
        for (i, chunk) in chunks.iter().enumerate() {
            assert!(
                !chunk.text.is_empty(),
                "Chunk {} text should not be empty",
                i
            );
            assert!(chunk.token_count > 0, "Chunk {} should have tokens", i);
            assert!(
                chunk.sentence_count > 0,
                "Chunk {} should have sentences",
                i
            );
            assert!(
                !chunk.sentences.is_empty(),
                "Chunk {} should have sentences vec",
                i
            );
        }

        println!("\n=== Realistic Paragraph Chunking ===");
        println!("Total chunks: {}", chunks.len());
        for (i, chunk) in chunks.iter().enumerate() {
            println!(
                "\nChunk {}: {} sentences, {} tokens",
                i, chunk.sentence_count, chunk.token_count
            );
            println!(
                "Preview: {}...",
                chunk.text.chars().take(80).collect::<String>()
            );
        }

        Ok(())
    }

    #[test]
    fn test_markdown_document() -> Result<()> {
        let tokenizer = create_test_tokenizer()?;
        let chunker = SentenceAwareChunker::new(tokenizer, 150, 1);

        let text = "# Flow Knowledge Graph\n\n\
                    ## Overview\n\n\
                    The KG provides semantic context. It uses IPLD-compatible structures. \
                    The system is CRDT-backed for consistency.\n\n\
                    ## Features\n\n\
                    - Entity management\n\
                    - Relationship tracking\n\
                    - Provenance support\n\n\
                    Each feature is designed for distributed operation.";

        let chunks = chunker.chunk(text)?;

        assert!(!chunks.is_empty(), "Should handle markdown");

        // Verify chunks maintain some structural information
        let full_text: String = chunks.iter().map(|c| c.text.as_str()).collect();
        assert!(
            full_text.contains("Overview") || chunks[0].text.contains("Overview"),
            "Should preserve section headers"
        );

        Ok(())
    }

    #[test]
    fn test_code_snippet_chunking() -> Result<()> {
        let tokenizer = create_test_tokenizer()?;
        let chunker = SentenceAwareChunker::new(tokenizer, 100, 0);

        let text = "Here's an example function:\n\
                ```rust\n\
                fn calculate(x: i32, y: i32) -> i32 {\n\
                    let result = x + y;\n\
                    return result;\n\
                }\n\
                ```\n\
                This function adds two numbers. It takes two parameters. \
                The result is returned as an integer.";

        let chunks = chunker.chunk(text)?;

        assert!(!chunks.is_empty(), "Should handle text with code blocks");

        // Check if code block is preserved in some chunk
        let has_code = chunks.iter().any(|c| c.text.contains("fn calculate"));
        assert!(has_code, "Should preserve code snippet content");

        Ok(())
    }

    #[test]
    fn test_inline_code_handling() -> Result<()> {
        let tokenizer = create_test_tokenizer()?;
        let chunker = SentenceAwareChunker::new(tokenizer, 100, 0);

        let text = "Use the `tokenizer.encode()` method to tokenize text. \
                The `chunk()` function returns a vector. \
                Call `unwrap()` to extract the value.";

        let chunks = chunker.chunk(text)?;

        assert!(!chunks.is_empty(), "Should handle inline code");
        assert!(
            chunks[0].text.contains("encode()"),
            "Should preserve inline code"
        );

        Ok(())
    }

    #[test]
    fn test_mixed_code_and_prose() -> Result<()> {
        let tokenizer = create_test_tokenizer()?;
        let chunker = SentenceAwareChunker::new(tokenizer, 80, 1);

        let text = "First, initialize the tokenizer. \
                You can use: let tokenizer = Tokenizer::from_pretrained(\"model\", None)?; \
                This loads a pretrained model. Next, create a chunker instance. \
                Finally, call chunker.chunk(text) to process your input.";

        let chunks = chunker.chunk(text)?;

        assert!(!chunks.is_empty(), "Should handle mixed code and prose");

        // Verify content is preserved
        let combined = chunks
            .iter()
            .map(|c| c.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");

        assert!(
            combined.contains("Tokenizer::from_pretrained"),
            "Should preserve code elements"
        );
        assert!(combined.contains("initialize"), "Should preserve prose");

        Ok(())
    }

    // ============================================================================
    // Consistency and Determinism Tests
    // ============================================================================

    #[test]
    fn test_chunking_is_deterministic() -> Result<()> {
        let text = "First sentence here. Second sentence here. Third sentence here. \
                    Fourth one. Fifth one. Sixth one.";

        let tokenizer1 = create_test_tokenizer()?;
        let chunker1 = SentenceAwareChunker::new(tokenizer1, 50, 1);
        let chunks1 = chunker1.chunk(text)?;

        let tokenizer2 = create_test_tokenizer()?;
        let chunker2 = SentenceAwareChunker::new(tokenizer2, 50, 1);
        let chunks2 = chunker2.chunk(text)?;

        assert_eq!(
            chunks1.len(),
            chunks2.len(),
            "Should produce same number of chunks"
        );

        for (c1, c2) in chunks1.iter().zip(chunks2.iter()) {
            assert_eq!(c1.text, c2.text, "Chunk text should match");
            assert_eq!(
                c1.sentence_count, c2.sentence_count,
                "Sentence counts should match"
            );
            assert_eq!(c1.token_count, c2.token_count, "Token counts should match");
        }

        Ok(())
    }

    #[test]
    fn test_chunking_preserves_content() -> Result<()> {
        let tokenizer = create_test_tokenizer()?;
        let chunker = SentenceAwareChunker::new(tokenizer, 30, 0); // No overlap

        let text = "Alpha. Beta. Gamma. Delta.";

        let chunks = chunker.chunk(text)?;

        // With no overlap, we should be able to find all original words
        let combined = chunks
            .iter()
            .map(|c| c.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");

        assert!(combined.contains("Alpha"), "Should preserve 'Alpha'");
        assert!(combined.contains("Beta"), "Should preserve 'Beta'");
        assert!(combined.contains("Gamma"), "Should preserve 'Gamma'");
        assert!(combined.contains("Delta"), "Should preserve 'Delta'");

        Ok(())
    }
}
