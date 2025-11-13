use std::{path::Path, time::Duration};

use swiftide::{
    indexing::{
        IndexingStream, Node,
        transformers::{ChunkMarkdown, ChunkText},
    },
    integrations,
    traits::ChunkerTransformer,
};
use tracing::{debug, warn};

#[derive(Clone)]
pub struct SmartChunker {
    chunk_range: std::ops::Range<usize>,
}

impl SmartChunker {
    pub fn new(chunk_range: std::ops::Range<usize>) -> Self {
        Self { chunk_range }
    }

    /// Detect language from file extension
    fn detect_language(path: &Path) -> Option<&'static str> {
        path.extension()
            .and_then(|ext| ext.to_str())
            .and_then(|ext| {
                let ext_lower = ext.to_lowercase();
                match ext_lower.as_str() {
                    "rs" => Some("rust"),
                    "py" => Some("python"),
                    "js" | "jsx" | "mjs" => Some("javascript"),
                    "ts" | "tsx" => Some("typescript"),
                    "java" => Some("java"),
                    "go" => Some("go"),
                    "rb" => Some("ruby"),
                    "cs" => Some("csharp"),
                    "c" | "h" => Some("c"),
                    "cpp" | "cc" | "cxx" | "hpp" | "hxx" => Some("cpp"),
                    "ex" | "exs" => Some("elixir"),
                    "html" | "htm" => Some("html"),
                    "php" => Some("php"),
                    "sol" => Some("solidity"),
                    _ => None,
                }
            })
    }

    /// Check if file is markdown
    fn is_markdown(path: &Path) -> bool {
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| {
                let ext_lower = ext.to_lowercase();
                matches!(ext_lower.as_str(), "md" | "markdown" | "mdx")
            })
            .unwrap_or(false)
    }
}

impl std::fmt::Debug for SmartChunker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SmartChunker")
            .field("chunk_range", &self.chunk_range)
            .finish()
    }
}

#[async_trait::async_trait]
impl ChunkerTransformer for SmartChunker {
    type Input = String;
    type Output = String;

    async fn transform_node(&self, node: Node<String>) -> IndexingStream<String> {
        let path = &node.path;

        if path.as_os_str().is_empty() {
            debug!("No path for node, using text chunker");
            return ChunkText::from_chunk_range(self.chunk_range.clone())
                .transform_node(node)
                .await;
        }

        // Try code chunking first
        if let Some(language) = Self::detect_language(&path) {
            debug!("Attempting to chunk {:?} as {} code", path, language);

            match integrations::treesitter::transformers::ChunkCode::try_for_language_and_chunk_size(
                language,
                self.chunk_range.clone(),
            ) {
                Ok(chunker) => {
                    match tokio::time::timeout(
                        Duration::from_secs(30),
                        chunker.transform_node(node.clone()),
                    )
                    .await
                    {
                        Ok(stream) => {
                            debug!("Successfully chunked {:?} as {}", path, language);
                            return stream;
                        }
                        Err(_) => {
                            warn!(
                                "Timeout chunking {:?} as {}, falling back to text",
                                path, language
                            );
                        }
                    }
                }
                Err(e) => {
                    warn!(
                        "Failed to create code chunker for {} on {:?}: {}",
                        language, path, e
                    );
                }
            }
        }

        // Try markdown chunking
        if Self::is_markdown(&path) {
            debug!("Chunking {:?} as markdown", path);
            return ChunkMarkdown::from_chunk_range(self.chunk_range.clone())
                .transform_node(node)
                .await;
        }

        // Fall back to text chunking
        debug!("Chunking {:?} as plain text", path);
        ChunkText::from_chunk_range(self.chunk_range.clone())
            .transform_node(node)
            .await
    }
}
