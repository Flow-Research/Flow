use anyhow::Result;
use tokenizers::Tokenizer;

pub struct TokenChunker {
    tokenizer: Tokenizer,
    chunk_size: usize,
    overlap: usize,
}

impl TokenChunker {
    /// Create a new token-based chunker
    ///
    /// # Arguments
    /// * `chunk_size` - Maximum tokens per chunk
    /// * `overlap` - Number of tokens to overlap between chunks
    pub fn new(chunk_size: usize, overlap: usize) -> Result<Self> {
        if overlap >= chunk_size {
            return Err(anyhow::anyhow!(
                "Invalid configuration: overlap ({}) must be less than chunk_size ({})",
                overlap,
                chunk_size
            ));
        }

        let tokenizer =
            Tokenizer::from_pretrained("google-bert/bert-base-multilingual-cased", None)
                .map_err(|e| anyhow::anyhow!("Failed to load tokenizer: {}", e))?;

        Ok(Self {
            tokenizer,
            chunk_size,
            overlap,
        })
    }

    /// Chunk text by token count
    pub fn chunk(&self, text: &str) -> Result<Vec<Chunk>> {
        // Tokenize the entire text
        let encoding = self
            .tokenizer
            .encode(text, false)
            .map_err(|e| anyhow::anyhow!("Tokenization failed: {}", e))?;

        let tokens = encoding.get_tokens();
        let token_count = tokens.len();

        let mut chunks = Vec::new();
        let mut start_idx = 0;

        while start_idx < token_count {
            let end_idx = (start_idx + self.chunk_size).min(token_count);

            // Get the character offsets for this token range
            let start_char = encoding
                .token_to_chars(start_idx)
                .map(|(_, (start, _end))| start)
                .unwrap_or(0);

            let end_char = if end_idx == token_count {
                text.len()
            } else {
                encoding
                    .token_to_chars(end_idx.saturating_sub(1))
                    .map(|(_, (_start, end))| end)
                    .unwrap_or(text.len())
            };

            let chunk_text = &text[start_char..end_char];

            chunks.push(Chunk {
                text: chunk_text.to_string(),
                token_count: end_idx - start_idx,
                start_char,
                end_char,
                chunk_id: chunks.len(),
            });

            // Move forward by chunk_size - overlap
            start_idx += self.chunk_size.saturating_sub(self.overlap);
        }

        Ok(chunks)
    }
}

#[derive(Debug, Clone)]
pub struct Chunk {
    pub text: String,
    pub token_count: usize,
    pub start_char: usize,
    pub end_char: usize,
    pub chunk_id: usize,
}
