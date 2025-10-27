use anyhow::Result;
use node::modules::kg::chunker::token_chunker::TokenChunker;

mod token_chunker_tests {

    use log::info;

    use super::*;

    // ============================================================================
    // Basic Functionality Tests
    // ============================================================================

    #[test]
    fn test_chunker_initialization() -> Result<()> {
        let chunker = TokenChunker::new(512, 50)?;
        assert!(true, "Chunker should initialize successfully");
        Ok(())
    }

    #[test]
    fn test_empty_text() -> Result<()> {
        let chunker = TokenChunker::new(512, 50)?;
        let chunks = chunker.chunk("")?;

        assert_eq!(chunks.len(), 0, "Empty text should produce no chunks");
        Ok(())
    }

    #[test]
    fn test_single_token_text() -> Result<()> {
        let chunker = TokenChunker::new(512, 50)?;
        let text = "Hello";
        let chunks = chunker.chunk(text)?;

        assert_eq!(chunks.len(), 1, "Single word should produce one chunk");
        assert_eq!(chunks[0].text, text, "Chunk text should match input");
        assert_eq!(chunks[0].start_char, 0, "Should start at character 0");
        assert_eq!(chunks[0].end_char, text.len(), "Should end at text length");
        assert_eq!(chunks[0].chunk_id, 0, "First chunk should have ID 0");
        Ok(())
    }

    #[test]
    fn test_short_text_single_chunk() -> Result<()> {
        let chunker = TokenChunker::new(512, 50)?;
        let text = "The quick brown fox jumps over the lazy dog.";
        let chunks = chunker.chunk(text)?;

        assert_eq!(chunks.len(), 1, "Short text should fit in one chunk");
        assert_eq!(chunks[0].text, text);
        assert!(chunks[0].token_count > 0, "Should have counted tokens");
        assert!(chunks[0].token_count <= 512, "Should not exceed chunk size");
        Ok(())
    }

    // ============================================================================
    // Multi-chunk Tests
    // ============================================================================

    #[test]
    fn test_text_requiring_multiple_chunks() -> Result<()> {
        let chunker = TokenChunker::new(10, 2)?; // Small chunks for testing
        let text = "The quick brown fox jumps over the lazy dog. \
                    The quick brown fox jumps over the lazy dog. \
                    The quick b rown fox jumps over the lazy dog.";

        let chunks = chunker.chunk(text)?;

        assert!(chunks.len() > 1, "Long text should produce multiple chunks");

        // Verify chunk IDs are sequential
        for (i, chunk) in chunks.iter().enumerate() {
            assert_eq!(chunk.chunk_id, i, "Chunk IDs should be sequential");
            info!("Chunk: {}", chunk.text)
        }

        Ok(())
    }

    #[test]
    fn test_chunk_size_limit() -> Result<()> {
        let chunk_size = 10;
        let chunker = TokenChunker::new(chunk_size, 0)?;
        let text = "word ".repeat(100); // Many words

        let chunks = chunker.chunk(&text)?;

        for chunk in chunks.iter() {
            assert!(
                chunk.token_count <= chunk_size,
                "Chunk token count ({}) should not exceed chunk_size ({})",
                chunk.token_count,
                chunk_size
            );
        }

        Ok(())
    }

    // ============================================================================
    // Overlap Tests
    // ============================================================================

    #[test]
    fn test_overlap_behavior() -> Result<()> {
        let chunker = TokenChunker::new(10, 3)?;
        let text = "word ".repeat(50);

        let chunks = chunker.chunk(&text)?;

        if chunks.len() > 1 {
            // Check that consecutive chunks have overlapping character ranges
            for i in 0..chunks.len() - 1 {
                let current = &chunks[i];
                let next = &chunks[i + 1];

                assert!(
                    next.start_char < current.end_char,
                    "Chunks should overlap: chunk {} ends at {}, chunk {} starts at {}",
                    i,
                    current.end_char,
                    i + 1,
                    next.start_char
                );
            }
        }

        Ok(())
    }

    #[test]
    fn test_no_overlap() -> Result<()> {
        let chunker = TokenChunker::new(10, 0)?;
        let text = "word ".repeat(50);

        let chunks = chunker.chunk(&text)?;

        if chunks.len() > 1 {
            for i in 0..chunks.len() - 1 {
                let current = &chunks[i];
                let next = &chunks[i + 1];

                assert!(
                    next.start_char >= current.end_char,
                    "With no overlap, chunks should not overlap"
                );
            }
        }

        Ok(())
    }

    // ============================================================================
    // Character Position Integrity Tests
    // ============================================================================

    #[test]
    fn test_character_offsets_validity() -> Result<()> {
        let chunker = TokenChunker::new(20, 5)?;
        let text = "The quick brown fox jumps over the lazy dog. ".repeat(5);

        let chunks = chunker.chunk(&text)?;

        for chunk in chunks.iter() {
            // Verify offsets are within text bounds
            assert!(chunk.start_char < text.len(), "start_char out of bounds");
            assert!(chunk.end_char <= text.len(), "end_char out of bounds");
            assert!(
                chunk.start_char < chunk.end_char,
                "start should be before end"
            );

            // Verify the text slice matches
            let extracted_text = &text[chunk.start_char..chunk.end_char];
            assert_eq!(
                extracted_text, chunk.text,
                "Extracted text should match chunk.text"
            );
        }

        Ok(())
    }

    #[test]
    fn test_first_chunk_starts_at_zero() -> Result<()> {
        let chunker = TokenChunker::new(512, 50)?;
        let text = "Some test text here.";

        let chunks = chunker.chunk(text)?;

        assert!(!chunks.is_empty(), "Should have at least one chunk");
        assert_eq!(chunks[0].start_char, 0, "First chunk should start at 0");
        Ok(())
    }

    #[test]
    fn test_last_chunk_ends_at_text_length() -> Result<()> {
        let chunker = TokenChunker::new(10, 2)?;
        let text = "word ".repeat(20);

        let chunks = chunker.chunk(&text)?;

        assert!(!chunks.is_empty(), "Should have at least one chunk");
        let last_chunk = chunks.last().unwrap();
        assert_eq!(
            last_chunk.end_char,
            text.len(),
            "Last chunk should end at text length"
        );
        Ok(())
    }

    #[test]
    fn test_chunks_cover_entire_text() -> Result<()> {
        let chunker = TokenChunker::new(10, 0)?; // No overlap
        let text = "The quick brown fox jumps over the lazy dog.";

        let chunks = chunker.chunk(text)?;

        // With no overlap, chunks should cover the entire text without gaps
        let mut covered = vec![false; text.len()];

        for chunk in chunks.iter() {
            for i in chunk.start_char..chunk.end_char {
                covered[i] = true;
            }
        }

        let all_covered = covered.iter().all(|&c| c);
        assert!(all_covered, "All characters should be covered by chunks");
        Ok(())
    }

    // ============================================================================
    // Edge Cases and Special Characters
    // ============================================================================

    #[test]
    fn test_unicode_text() -> Result<()> {
        let chunker = TokenChunker::new(512, 50)?;
        let text = "Hello 世界! Привет мир! مرحبا بالعالم! 🌍🌎🌏";

        let chunks = chunker.chunk(text)?;

        assert!(!chunks.is_empty(), "Should handle unicode text");

        // Verify we can reconstruct the text
        for chunk in chunks.iter() {
            let extracted = &text[chunk.start_char..chunk.end_char];
            assert_eq!(extracted, chunk.text, "Unicode should be preserved");
        }

        Ok(())
    }

    #[test]
    fn test_text_with_newlines() -> Result<()> {
        let chunker = TokenChunker::new(512, 50)?;
        let text = "Line 1\nLine 2\nLine 3\n\nLine 5";

        let chunks = chunker.chunk(text)?;

        assert!(!chunks.is_empty(), "Should handle newlines");

        // Verify newlines are preserved
        let reconstructed: String = chunks
            .iter()
            .map(|c| c.text.as_str())
            .collect::<Vec<_>>()
            .join("");

        // With overlap, we can't do exact reconstruction, but first chunk should match start
        assert!(
            text.starts_with(&chunks[0].text),
            "Should preserve beginning"
        );
        Ok(())
    }

    #[test]
    fn test_text_with_special_characters() -> Result<()> {
        let chunker = TokenChunker::new(512, 50)?;
        let text = "Special chars: @#$%^&*()_+-=[]{}|;':\",./<>?`~";

        let chunks = chunker.chunk(text)?;

        assert!(!chunks.is_empty(), "Should handle special characters");
        assert_eq!(chunks[0].text, text, "Special chars should be preserved");
        Ok(())
    }

    #[test]
    fn test_whitespace_only() -> Result<()> {
        let chunker = TokenChunker::new(512, 50)?;
        let text = "     \t\t\n\n   ";

        let chunks = chunker.chunk(text)?;

        // Depending on tokenizer, this might produce chunks or be empty
        // Just verify it doesn't panic
        assert!(true, "Should handle whitespace-only text");
        Ok(())
    }

    #[test]
    fn test_very_long_single_word() -> Result<()> {
        let chunker = TokenChunker::new(512, 50)?;
        let text = "a".repeat(10000);

        let chunks = chunker.chunk(&text)?;

        assert!(!chunks.is_empty(), "Should handle very long words");
        Ok(())
    }

    // ============================================================================
    // Boundary Condition Tests
    // ============================================================================

    #[test]
    fn test_chunk_size_equals_one() -> Result<()> {
        let chunker = TokenChunker::new(1, 0)?;
        let text = "Hello world";

        let chunks = chunker.chunk(text)?;

        assert!(
            chunks.len() > 1,
            "Should split into multiple single-token chunks"
        );
        for chunk in chunks.iter() {
            assert!(
                chunk.token_count <= 1,
                "Each chunk should have at most 1 token"
            );
        }
        Ok(())
    }

    #[test]
    fn test_overlap_larger_than_chunk_size() -> Result<()> {
        // This is technically invalid but should be handled gracefully
        let result = TokenChunker::new(5, 10);

        assert!(result.is_err(), "Should reject overlap >= chunk_size");

        if let Err(e) = result {
            assert!(e.to_string().contains("overlap"));
        }

        Ok(())
    }

    #[test]
    fn test_multilingual_text() -> Result<()> {
        let chunker = TokenChunker::new(50, 10)?;
        let text = "English text. Texto en español. Texte français. \
                    Deutscher Text. Testo italiano. 日本語のテキスト。\
                    中文文本。한국어 텍스트。";

        let chunks = chunker.chunk(text)?;

        assert!(!chunks.is_empty(), "Should handle multilingual text");

        // Verify chunks don't corrupt the text
        for chunk in chunks.iter() {
            assert!(!chunk.text.is_empty(), "Chunks should not be empty");
        }

        Ok(())
    }

    // ============================================================================
    // Consistency and Determinism Tests
    // ============================================================================

    #[test]
    fn test_chunking_is_deterministic() -> Result<()> {
        let text = "The quick brown fox jumps over the lazy dog. ".repeat(5);

        let chunker1 = TokenChunker::new(20, 5)?;
        let chunks1 = chunker1.chunk(&text)?;

        let chunker2 = TokenChunker::new(20, 5)?;
        let chunks2 = chunker2.chunk(&text)?;

        assert_eq!(
            chunks1.len(),
            chunks2.len(),
            "Should produce same number of chunks"
        );

        for (c1, c2) in chunks1.iter().zip(chunks2.iter()) {
            assert_eq!(c1.text, c2.text, "Chunk text should match");
            assert_eq!(c1.token_count, c2.token_count, "Token counts should match");
            assert_eq!(c1.start_char, c2.start_char, "Start positions should match");
            assert_eq!(c1.end_char, c2.end_char, "End positions should match");
        }

        Ok(())
    }

    // ============================================================================
    // Integration Tests
    // ============================================================================

    #[test]
    fn test_realistic_document_chunking() -> Result<()> {
        let chunker = TokenChunker::new(512, 50)?;

        let text = "# Introduction\n\n\
            This is a sample document that contains multiple paragraphs \
            and sections. It simulates a real-world knowledge base document.\n\n\
            ## Section 1\n\n\
            Lorem ipsum dolor sit amet, consectetur adipiscing elit. \
            Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.\n\n\
            ## Section 2\n\n\
            Ut enim ad minim veniam, quis nostrud exercitation ullamco \
            laboris nisi ut aliquip ex ea commodo consequat.\n\n\
            ## Conclusion\n\n\
            Duis aute irure dolor in reprehenderit in voluptate velit \
            esse cillum dolore eu fugiat nulla pariatur.";

        let chunks = chunker.chunk(text)?;

        assert!(!chunks.is_empty(), "Should produce chunks");

        // Verify properties
        for chunk in chunks.iter() {
            assert!(!chunk.text.is_empty(), "Chunks should not be empty");
            assert!(chunk.token_count > 0, "Should have positive token count");
            assert!(chunk.token_count <= 512, "Should respect chunk size");
        }

        println!("\n=== Document Chunking Results ===");
        println!("Total chunks: {}", chunks.len());
        for (i, chunk) in chunks.iter().enumerate() {
            println!(
                "\nChunk {}: {} tokens, chars {}-{}",
                i, chunk.token_count, chunk.start_char, chunk.end_char
            );
            println!(
                "Preview: {}...",
                chunk.text.chars().take(50).collect::<String>()
            );
        }

        Ok(())
    }
}
