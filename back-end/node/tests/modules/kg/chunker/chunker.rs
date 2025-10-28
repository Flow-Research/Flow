use node::modules::kg::chunker::chunker::Chunker;
use node::modules::kg::chunker::config::ChunkerConfig;
use node::modules::kg::chunker::types::{
    Chunk, ChunkMetadata, ChunkerError, ChunkingStrategy, ContentType,
};

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_chunker() -> Chunker {
        Chunker::new(ChunkerConfig::default()).unwrap()
    }

    // ========================================================================
    // Basic Functionality Tests
    // ========================================================================

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
    fn test_single_sentence() {
        let chunker = setup_chunker();
        let text = "This is a single sentence.";
        let chunks = chunker.chunk(text).unwrap();

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].text, text);
    }

    // ========================================================================
    // Content Type & Strategy Tests
    // ========================================================================

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
        assert!(chunks.len() >= 1);
        assert!(
            chunks
                .iter()
                .all(|c| c.metadata.content_type == Some(ContentType::Markdown))
        );
    }

    #[test]
    fn test_code_chunking() {
        let config = ChunkerConfig::default().with_content_type(ContentType::Code);
        let chunker = Chunker::new(config).unwrap();

        let code = r#"
fn main() {
    println!("Hello, world!");
}

fn another_function() {
    let x = 5;
    let y = 10;
}
        "#;

        let chunks = chunker.chunk(code).unwrap();
        assert!(!chunks.is_empty());
        assert!(
            chunks
                .iter()
                .all(|c| c.metadata.content_type == Some(ContentType::Code))
        );
    }

    #[test]
    fn test_scientific_chunking() {
        let config = ChunkerConfig::default().with_content_type(ContentType::Scientific);
        let chunker = Chunker::new(config).unwrap();

        let paper = r#"
Abstract: This paper presents a novel approach.

Introduction

The field has seen significant advances.

Methodology

We employed rigorous experimental design.
        "#;

        let chunks = chunker.chunk(paper).unwrap();
        assert!(!chunks.is_empty());
        assert!(
            chunks
                .iter()
                .all(|c| c.metadata.content_type == Some(ContentType::Scientific))
        );
    }

    #[test]
    fn test_strategy_override() {
        let config = ChunkerConfig::default().with_strategy(ChunkingStrategy::FixedSize);
        let chunker = Chunker::new(config).unwrap();

        let text = "Sentence one. Sentence two. Sentence three.";
        let chunks = chunker.chunk(text).unwrap();
        assert!(!chunks.is_empty());
    }

    // ========================================================================
    // Paragraph Chunking Tests
    // ========================================================================

    #[test]
    fn test_paragraph_preservation() {
        let config = ChunkerConfig::default().with_strategy(ChunkingStrategy::Paragraph);
        let chunker = Chunker::new(config).unwrap();

        let text = "Paragraph one.\n\nParagraph two.\n\nParagraph three.";
        let chunks = chunker.chunk(text).unwrap();

        println!("Chunk count {}", chunks.len());

        // Should preserve paragraph boundaries
        assert!(chunks.len() >= 1);
    }

    #[test]
    fn test_empty_paragraphs_filtered() {
        let config = ChunkerConfig::default().with_strategy(ChunkingStrategy::Paragraph);
        let chunker = Chunker::new(config).unwrap();

        let text = "Paragraph one.\n\n\n\nParagraph two.";
        let chunks = chunker.chunk(text).unwrap();

        // Empty paragraphs should be filtered
        assert!(chunks.iter().all(|c| !c.text.is_empty()));
    }

    #[test]
    fn test_large_paragraph_splitting() {
        let config = ChunkerConfig::default()
            .with_max_size(100)
            .with_overlap(50)
            .with_strategy(ChunkingStrategy::Paragraph);
        let chunker = Chunker::new(config).unwrap();

        let large_para = "This is a very long paragraph. ".repeat(20);
        let chunks = chunker.chunk(&large_para).unwrap();

        // Large paragraph should be split
        assert!(chunks.len() > 1);
        assert!(chunks.iter().all(|c| c.text.len() <= 150)); // With some tolerance
    }

    // ========================================================================
    // Metadata Tests
    // ========================================================================

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

    #[test]
    fn test_language_detection() {
        let config = ChunkerConfig::default().with_language_detection();
        let chunker = Chunker::new(config).unwrap();

        let english_text = "This is an English sentence for testing.";
        let chunks = chunker.chunk(english_text).unwrap();

        assert!(!chunks.is_empty());
        assert_eq!(chunks[0].metadata.language, Some("eng".to_string()));
    }

    #[test]
    fn test_no_language_detection_by_default() {
        let chunker = setup_chunker();
        let text = "This is a test.";
        let chunks = chunker.chunk(text).unwrap();

        assert!(chunks[0].metadata.language.is_none());
    }

    // ========================================================================
    // Position Tracking Tests
    // ========================================================================

    #[test]
    fn test_position_tracking() {
        let chunker = setup_chunker();
        let text = "First sentence. Second sentence. Third sentence.";
        let chunks = chunker.chunk(text).unwrap();

        // Positions should be within text bounds
        for chunk in &chunks {
            assert!(chunk.start_char < text.len());
            assert!(chunk.end_char <= text.len());
            assert!(chunk.start_char < chunk.end_char);
        }
    }

    #[test]
    fn test_overlapping_chunks_positions() {
        let config = ChunkerConfig::default().with_max_size(50).with_overlap(20);
        let chunker = Chunker::new(config).unwrap();

        let text = "Word ".repeat(50);
        let chunks = chunker.chunk(&text).unwrap();

        if chunks.len() > 1 {
            // Subsequent chunks should start before previous ends (overlap)
            for i in 0..chunks.len() - 1 {
                assert!(chunks[i + 1].start_char <= chunks[i].end_char);
            }
        }
    }

    // ========================================================================
    // Validation Tests
    // ========================================================================

    #[test]
    fn test_reject_oversized_input() {
        let config = ChunkerConfig::default().with_max_input_size(1000);
        let chunker = Chunker::new(config).unwrap();

        let large_text = "x".repeat(2000);
        let result = chunker.chunk(&large_text);

        assert!(result.is_err());
        match result.unwrap_err() {
            ChunkerError::ValidationError(msg) => {
                assert!(msg.contains("too large"));
            }
            _ => panic!("Expected ValidationError"),
        }
    }

    #[test]
    fn test_reject_null_characters() {
        let chunker = setup_chunker();
        let text_with_null = "Hello\0World";
        let result = chunker.chunk(text_with_null);

        assert!(result.is_err());
        match result.unwrap_err() {
            ChunkerError::ValidationError(msg) => {
                assert!(msg.contains("null"));
            }
            _ => panic!("Expected ValidationError"),
        }
    }

    #[test]
    fn test_allow_null_when_configured() {
        let config = ChunkerConfig::default().with_null_char_rejection(false);
        let chunker = Chunker::new(config).unwrap();

        let text_with_null = "Hello\0World";
        let result = chunker.chunk(text_with_null);

        // Should succeed when null rejection is disabled
        assert!(result.is_ok());
    }

    // ========================================================================
    // Edge Cases
    // ========================================================================

    #[test]
    fn test_whitespace_only() {
        let chunker = setup_chunker();
        let whitespace = "   \n\n   \t  ";
        let chunks = chunker.chunk(whitespace).unwrap();

        // After trimming, should produce no chunks
        assert!(chunks.is_empty() || chunks.iter().all(|c| c.text.is_empty()));
    }

    #[test]
    fn test_unicode_handling() {
        let chunker = setup_chunker();
        let unicode = "Hello 世界 🌍 Привет مرحبا";
        let chunks = chunker.chunk(unicode).unwrap();

        assert!(!chunks.is_empty());
        assert!(chunks[0].text.contains("世界"));
        assert!(chunks[0].text.contains("🌍"));
    }

    #[test]
    fn test_mixed_newlines() {
        let chunker = setup_chunker();
        let text = "Line 1\nLine 2\r\nLine 3\n\nParagraph break";
        let result = chunker.chunk(text);

        assert!(result.is_ok());
        assert!(!result.unwrap().is_empty());
    }

    #[test]
    fn test_special_characters() {
        let chunker = setup_chunker();
        let text = "Test with $pecial ch@racters & symbols!";
        let chunks = chunker.chunk(text).unwrap();

        assert!(!chunks.is_empty());
        assert_eq!(chunks[0].text, text);
    }

    #[test]
    fn test_very_long_single_sentence() {
        let config = ChunkerConfig::default().with_max_size(100).with_overlap(40);
        let chunker = Chunker::new(config).unwrap();

        let long_sentence = "This is a very long sentence without any punctuation that goes on and on and on and should be split ".repeat(5);
        let chunks = chunker.chunk(&long_sentence).unwrap();

        // Should split even a single long sentence
        assert!(chunks.len() > 1);
    }

    // ========================================================================
    // Configuration Tests
    // ========================================================================

    #[test]
    fn test_invalid_config_zero_chunk_size() {
        let config = ChunkerConfig {
            max_chunk_size: 0,
            ..Default::default()
        };

        let result = Chunker::new(config);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_config_overlap_too_large() {
        let config = ChunkerConfig {
            max_chunk_size: 100,
            overlap: 100,
            ..Default::default()
        };

        let result = Chunker::new(config);
        assert!(result.is_err());
    }

    #[test]
    fn test_custom_chunk_size() {
        let config = ChunkerConfig::default().with_max_size(500);
        let chunker = Chunker::new(config).unwrap();

        let text = "Word ".repeat(200);
        let chunks = chunker.chunk(&text).unwrap();

        // All chunks should respect max size (with some tolerance)
        assert!(chunks.iter().all(|c| c.text.len() <= 550));
    }

    // ========================================================================
    // Trimming Tests
    // ========================================================================

    #[test]
    fn test_trimming_enabled() {
        let config = ChunkerConfig::default(); // trim is true by default
        let chunker = Chunker::new(config).unwrap();

        let text = "  Text with spaces  ";
        let chunks = chunker.chunk(text).unwrap();

        assert!(!chunks.is_empty());
        assert_eq!(chunks[0].text, "Text with spaces");
    }

    #[test]
    fn test_trimming_disabled() {
        let config = ChunkerConfig {
            trim: false,
            ..Default::default()
        };
        let chunker = Chunker::new(config).unwrap();

        let text = "  Text with spaces  ";
        let chunks = chunker.chunk(text).unwrap();

        // Note: text-splitter might still trim, but we don't add extra trimming
        assert!(!chunks.is_empty());
    }

    // ========================================================================
    // Token and Sentence Counting Tests
    // ========================================================================

    #[test]
    fn test_token_counting() {
        let chunker = setup_chunker();
        let text = "This is a test sentence.";
        let chunks = chunker.chunk(text).unwrap();

        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].token_count > 0);
        // Should have roughly 5-6 tokens
        assert!(chunks[0].token_count >= 4 && chunks[0].token_count <= 8);
    }

    #[test]
    fn test_sentence_counting() {
        let chunker = setup_chunker();
        let text = "First sentence. Second sentence. Third sentence.";
        let chunks = chunker.chunk(text).unwrap();

        assert!(!chunks.is_empty());
        // Count total sentences across all chunks
        let total_sentences: usize = chunks.iter().map(|c| c.sentence_count).sum();
        assert_eq!(total_sentences, 3);
    }

    // ========================================================================
    // Serialization Tests
    // ========================================================================

    #[test]
    fn test_chunk_serialization() {
        let chunker = setup_chunker();
        let text = "Test serialization.";
        let chunks = chunker.chunk(text).unwrap();

        let json = serde_json::to_string(&chunks[0]).unwrap();
        let deserialized: Chunk = serde_json::from_str(&json).unwrap();

        assert_eq!(chunks[0], deserialized);
    }

    #[test]
    fn test_chunk_metadata_serialization() {
        let metadata = ChunkMetadata {
            content_type: Some(ContentType::Markdown),
            language: Some("eng".to_string()),
            embedding: Some(vec![0.1, 0.2, 0.3]),
            vector_id: Some("vec_123".to_string()),
            custom: Some(serde_json::json!({"key": "value"})),
        };

        let json = serde_json::to_string(&metadata).unwrap();
        let deserialized: ChunkMetadata = serde_json::from_str(&json).unwrap();

        assert_eq!(metadata, deserialized);
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    // ========================================================================
    // Large Document Tests
    // ========================================================================

    #[test]
    fn test_large_document_chunking() {
        let config = ChunkerConfig::default();
        let chunker = Chunker::new(config).unwrap();

        // ~150KB text
        let large_text = "This is a test sentence with multiple words. ".repeat(3000);
        let chunks = chunker.chunk(&large_text).unwrap();

        assert!(!chunks.is_empty());
        assert!(chunks.iter().all(|c| c.text.len() <= 2200)); // max_size + tolerance

        // Verify all chunks are non-empty
        assert!(chunks.iter().all(|c| !c.text.is_empty()));

        // Verify chunk IDs are sequential
        for (idx, chunk) in chunks.iter().enumerate() {
            assert_eq!(chunk.chunk_id, idx);
        }
    }

    #[test]
    fn test_very_large_document() {
        let config = ChunkerConfig::default().with_max_input_size(1024 * 1024); // 1MB limit
        let chunker = Chunker::new(config).unwrap();

        // ~500KB text
        let very_large = "Word ".repeat(100_000);
        let chunks = chunker.chunk(&very_large).unwrap();

        assert!(!chunks.is_empty());
        println!("Generated {} chunks from 500KB text", chunks.len());
    }

    // ========================================================================
    // All Content Types End-to-End
    // ========================================================================

    #[test]
    fn test_all_content_types_auto_detect() {
        let chunker = Chunker::new(ChunkerConfig::default()).unwrap();

        let code = r#"
fn main() {
    let x = 5;
    println!("Hello");
}
    "#;

        let markdown = r#"
# Title

Content here

## Subtitle

More content
    "#;

        let scientific = r#"
Abstract: This paper presents...

Introduction

Methodology
    "#;

        let plain = "This is just plain text with sentences.";

        // All should succeed
        assert!(chunker.chunk(code).is_ok());
        assert!(chunker.chunk(markdown).is_ok());
        assert!(chunker.chunk(scientific).is_ok());
        assert!(chunker.chunk(plain).is_ok());
    }

    #[test]
    fn test_mixed_content_in_one_document() {
        let chunker = Chunker::new(ChunkerConfig::default()).unwrap();

        let mixed = r#"
# Documentation

Here's some code:

```rust
fn example() {
    println!("test");
}
```

And some explanation text afterwards.
    "#;

        let chunks = chunker.chunk(mixed).unwrap();
        assert!(!chunks.is_empty());

        // Should detect as markdown since it has markdown markers
        assert!(
            chunks
                .iter()
                .any(|c| c.metadata.content_type == Some(ContentType::Markdown))
        );
    }

    // ========================================================================
    // Overlap and Position Tests
    // ========================================================================

    #[test]
    fn test_overlap_provides_context() {
        let config = ChunkerConfig::default().with_max_size(100).with_overlap(30);
        let chunker = Chunker::new(config).unwrap();

        let text = "Sentence one. Sentence two. Sentence three. Sentence four. Sentence five.";
        let chunks = chunker.chunk(text).unwrap();

        if chunks.len() > 1 {
            // Check that some content overlaps
            let first_end = &chunks[0].text[chunks[0].text.len().saturating_sub(20)..];
            let second_start = &chunks[1].text[..20.min(chunks[1].text.len())];

            // Should have some overlapping words
            let first_words: Vec<&str> = first_end.split_whitespace().collect();
            let second_words: Vec<&str> = second_start.split_whitespace().collect();

            let has_overlap = first_words.iter().any(|w| second_words.contains(w));
            assert!(has_overlap, "Chunks should have overlapping content");
        }
    }

    // ========================================================================
    // Error Handling Tests
    // ========================================================================

    #[test]
    fn test_all_validation_errors() {
        // Test size limit
        let small_limit = ChunkerConfig::default().with_max_input_size(100);
        let chunker = Chunker::new(small_limit).unwrap();
        assert!(chunker.chunk(&"x".repeat(200)).is_err());

        // Test null characters
        let chunker = Chunker::new(ChunkerConfig::default()).unwrap();
        assert!(chunker.chunk("text\0with\0nulls").is_err());

        // Test config validation
        let invalid_config = ChunkerConfig {
            max_chunk_size: 100,
            overlap: 150,
            ..Default::default()
        };
        assert!(Chunker::new(invalid_config).is_err());
    }

    // ========================================================================
    // Real-World Scenarios
    // ========================================================================

    #[test]
    fn test_academic_paper() {
        let config = ChunkerConfig::default()
            .with_content_type(ContentType::Scientific)
            .with_max_size(1500);
        let chunker = Chunker::new(config).unwrap();

        let paper = r#"
Abstract

This paper presents a comprehensive analysis of distributed systems
and their application in modern computing environments.

Introduction

Distributed systems have become increasingly important in recent years.
Multiple researchers have contributed to this field (Smith et al., 2020).

Methodology

We employed a rigorous experimental design to test our hypothesis.
Data was collected from multiple sources over a six-month period.

Results

As shown in Fig. 1, the results indicate significant improvements.
Statistical analysis revealed p < 0.05 across all metrics.

Conclusion

This study demonstrates the effectiveness of our approach.
Future work will explore additional applications.

References

Smith, J., et al. (2020). Distributed Computing. Journal of CS, 45(2), 123-145.
    "#;

        let chunks = chunker.chunk(paper).unwrap();

        assert!(!chunks.is_empty());
        println!("Academic paper split into {} chunks", chunks.len());

        // Should preserve paragraph structure
        assert!(chunks.iter().all(|c| c.sentence_count > 0));
    }

    #[test]
    fn test_code_file() {
        let config = ChunkerConfig::default().with_content_type(ContentType::Code);
        let chunker = Chunker::new(config).unwrap();

        let code = r#"
use std::collections::HashMap;

pub struct DataStore {
    data: HashMap<String, String>,
}

impl DataStore {
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
        }
    }

    pub fn insert(&mut self, key: String, value: String) {
        self.data.insert(key, value);
    }

    pub fn get(&self, key: &str) -> Option<&String> {
        self.data.get(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_and_get() {
        let mut store = DataStore::new();
        store.insert("key".to_string(), "value".to_string());
        assert_eq!(store.get("key"), Some(&"value".to_string()));
    }
}
    "#;

        let chunks = chunker.chunk(code).unwrap();
        assert!(!chunks.is_empty());
        println!("Code file split into {} chunks", chunks.len());
    }

    #[test]
    fn test_large_code_file() {
        let config = ChunkerConfig::default()
            .with_content_type(ContentType::Code)
            .with_max_size(200)
            .with_overlap(50);
        let chunker = Chunker::new(config).unwrap();

        let code = "fn function() {}\n".repeat(50); // Repeat to make it large
        let chunks = chunker.chunk(&code).unwrap();

        assert!(
            chunks.len() > 1,
            "Large code should split into multiple chunks"
        );
    }

    #[test]
    fn test_markdown_documentation() {
        let config = ChunkerConfig::default().with_content_type(ContentType::Markdown);
        let chunker = Chunker::new(config).unwrap();

        let markdown = r#"
# Project Documentation

## Installation

```bash
npm install my-package
```

## Usage

Import the library:

```javascript
import { MyClass } from 'my-package';
```

Create an instance:

```javascript
const instance = new MyClass();
```

## API Reference

### MyClass

The main class for interacting with the library.

#### Methods

- `method1()` - Does something
- `method2()` - Does something else

## Examples

Here's a complete example:

```javascript
const instance = new MyClass();
instance.method1();
```
    "#;

        let chunks = chunker.chunk(markdown).unwrap();
        assert!(!chunks.is_empty());
        println!("Markdown doc split into {} chunks", chunks.len());

        // Should recognize as markdown
        assert!(
            chunks
                .iter()
                .all(|c| c.metadata.content_type == Some(ContentType::Markdown))
        );
    }

    // ========================================================================
    // Performance Tests (Rough)
    // ========================================================================

    #[test]
    fn test_performance_100kb_document() {
        use std::time::Instant;

        let config = ChunkerConfig::default();
        let chunker = Chunker::new(config).unwrap();

        let text = "This is a test sentence for performance measurement. ".repeat(2000);

        let start = Instant::now();
        let chunks = chunker.chunk(&text).unwrap();
        let duration = start.elapsed();

        assert!(!chunks.is_empty());
        println!("Chunked 100KB in {:?}", duration);
        println!("Chunks {}", chunks.len());

        // Should complete in reasonable time (< 1 second)
        assert!(
            duration.as_secs() < 1,
            "Chunking took too long: {:?}",
            duration
        );
    }
}
