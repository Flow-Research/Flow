use anyhow::Result;
use tokenizers::Tokenizer;
use unicode_segmentation::UnicodeSegmentation;

pub struct SentenceAwareChunker {
    tokenizer: Tokenizer,
    max_tokens: usize,
    overlap_sentences: usize,
}

impl SentenceAwareChunker {
    pub fn new(tokenizer: Tokenizer, max_tokens: usize, overlap_sentences: usize) -> Self {
        Self {
            tokenizer,
            max_tokens,
            overlap_sentences,
        }
    }

    /// Split text into sentences using Unicode segmentation
    fn split_sentences(&self, text: &str) -> Vec<String> {
        text.unicode_sentences().map(|s| s.to_string()).collect()
    }

    /// Count tokens in text
    fn count_tokens(&self, text: &str) -> usize {
        self.tokenizer
            .encode(text, false)
            .map(|e| e.len())
            .unwrap_or(0)
    }

    /// Chunk text by sentences, respecting token limits
    pub fn chunk(&self, text: &str) -> Result<Vec<SemanticChunk>> {
        let sentences = self.split_sentences(text);
        let mut chunks = Vec::new();
        let mut current_sentences = Vec::new();
        let mut current_token_count = 0;

        for (idx, sentence) in sentences.iter().enumerate() {
            let sentence_tokens = self.count_tokens(sentence);

            // If a single sentence exceeds max_tokens, split it further
            if sentence_tokens > self.max_tokens {
                // Flush current chunk if any
                if !current_sentences.is_empty() {
                    chunks.push(self.create_chunk(current_sentences.clone(), current_token_count));
                    current_sentences.clear();
                    current_token_count = 0;
                }

                // Split long sentence by punctuation or fixed size
                let sub_chunks = self.split_long_sentence(sentence);
                for sub_chunk in sub_chunks {
                    chunks.push(sub_chunk);
                }
                continue;
            }

            // Check if adding this sentence would exceed max_tokens
            if current_token_count + sentence_tokens > self.max_tokens {
                // Create chunk from accumulated sentences
                if !current_sentences.is_empty() {
                    chunks.push(self.create_chunk(current_sentences.clone(), current_token_count));
                }

                // Start new chunk with overlap
                let overlap_start = current_sentences
                    .len()
                    .saturating_sub(self.overlap_sentences);
                current_sentences = current_sentences[overlap_start..].to_vec();

                // Recalculate token count for overlap
                let overlap_text = current_sentences.join(" ");
                current_token_count = self.count_tokens(&overlap_text);
            }

            current_sentences.push(sentence.clone());
            current_token_count += sentence_tokens;
        }

        // Don't forget the last chunk
        if !current_sentences.is_empty() {
            chunks.push(self.create_chunk(current_sentences, current_token_count));
        }

        Ok(chunks)
    }

    fn create_chunk(&self, sentences: Vec<String>, token_count: usize) -> SemanticChunk {
        let text = sentences.join(" ");
        SemanticChunk {
            text,
            token_count,
            sentence_count: sentences.len(),
            sentences,
            metadata: ChunkMetadata::default(),
        }
    }

    /// Split a very long sentence by punctuation marks
    fn split_long_sentence(&self, sentence: &str) -> Vec<SemanticChunk> {
        // Split by commas, semicolons, etc.
        let regex = regex::Regex::new(r"[,;:]").unwrap();
        let parts: Vec<&str> = regex.split(sentence).collect();

        let mut chunks = Vec::new();
        let mut current = String::new();

        for part in parts {
            let part_tokens = self.count_tokens(part);
            let current_tokens = self.count_tokens(&current);

            if current_tokens + part_tokens > self.max_tokens && !current.is_empty() {
                chunks.push(SemanticChunk {
                    text: current.trim().to_string(),
                    token_count: current_tokens,
                    sentence_count: 1,
                    sentences: vec![current.trim().to_string()],
                    metadata: ChunkMetadata::default(),
                });
                current = part.to_string();
            } else {
                if !current.is_empty() {
                    current.push_str(", ");
                }
                current.push_str(part);
            }
        }

        if !current.is_empty() {
            chunks.push(SemanticChunk {
                text: current.trim().to_string(),
                token_count: self.count_tokens(&current),
                sentence_count: 1,
                sentences: vec![current.trim().to_string()],
                metadata: ChunkMetadata::default(),
            });
        }

        chunks
    }
}

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
