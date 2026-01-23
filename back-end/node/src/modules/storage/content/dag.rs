//! DAG (Directed Acyclic Graph) builder and reader for content-addressed storage.
//!
//! This module provides high-level APIs for publishing and retrieving content:
//!
//! - `DagBuilder`: Chunks data, stores blocks, creates manifest, returns root CID
//! - `DagReader`: Fetches manifest, retrieves chunks, reconstructs content
//!
//! # Example
//!
//! ```ignore
//! // Publishing content
//! let builder = DagBuilder::new(block_store.clone(), chunker);
//! let result = builder.build_from_bytes(
//!     &data,
//!     DagMetadata::new("did:key:z6Mk...")
//!         .name("document.pdf")
//!         .content_type("application/pdf"),
//! )?;
//! println!("Published: {}", result.root_cid);
//!
//! // Retrieving content
//! let reader = DagReader::new(&block_store);
//! let content = reader.read_all(&result.root_cid)?;
//! ```

use std::collections::HashMap;
use std::io::Read;
use std::ops::Range;
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, info, instrument, warn};

use crate::modules::storage::content::{
    ContentProvider, ContentProviderError, LocalProvider, TreeNode,
};

use super::block::Block;
use super::block_store::BlockStore;
use super::chunking::{ChunkData, Chunker, ChunkingAlgorithm, StreamingChunkError};
use super::cid::ContentId;
use super::error::BlockStoreError;
use super::manifest::{DocumentManifest, TreeReader};

// ============================================================================
// Error Types
// ============================================================================

/// Error type for DAG operations.
#[derive(Debug, Error)]
pub enum DagError {
    /// Block store operation failed.
    #[error("Block store error: {0}")]
    BlockStore(#[from] BlockStoreError),

    /// Chunking operation failed.
    #[error("Chunking error: {0}")]
    Chunking(#[from] StreamingChunkError),

    /// IO error during streaming.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Manifest block not found.
    #[error("Manifest not found: {0}")]
    ManifestNotFound(ContentId),

    /// Chunk block not found.
    #[error("Chunk not found: {0}")]
    ChunkNotFound(ContentId),

    /// Tree node not found during traversal.
    #[error("Tree node not found: {0}")]
    TreeNodeNotFound(ContentId),

    /// Invalid manifest data.
    #[error("Invalid manifest: {0}")]
    InvalidManifest(String),

    /// Range out of bounds.
    #[error("Range {start}..{end} out of bounds for size {size}")]
    RangeOutOfBounds { start: u64, end: u64, size: u64 },

    /// Content provider error.
    #[error("Content provider error: {0}")]
    Provider(#[from] ContentProviderError),
}

/// Result type for DAG operations.
pub type DagResult<T> = Result<T, DagError>;

// ============================================================================
// DagMetadata
// ============================================================================

/// Metadata for DAG creation.
///
/// Contains information about the content being published.
/// The `publisher_did` is required; other fields are optional.
#[derive(Debug, Clone)]
pub struct DagMetadata {
    /// Original filename (optional).
    pub name: Option<String>,
    /// MIME content type (optional).
    pub content_type: Option<String>,
    /// Publisher's DID (required).
    pub publisher_did: String,
    /// Additional key-value metadata.
    pub metadata: HashMap<String, String>,
}

impl DagMetadata {
    /// Create new metadata with the required publisher DID.
    pub fn new(publisher_did: impl Into<String>) -> Self {
        Self {
            name: None,
            content_type: None,
            publisher_did: publisher_did.into(),
            metadata: HashMap::new(),
        }
    }

    /// Set the document name.
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set the content type (MIME type).
    pub fn content_type(mut self, content_type: impl Into<String>) -> Self {
        self.content_type = Some(content_type.into());
        self
    }

    /// Add a metadata key-value pair.
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Add multiple metadata entries.
    pub fn with_metadata_entries(mut self, entries: HashMap<String, String>) -> Self {
        self.metadata.extend(entries);
        self
    }
}

// ============================================================================
// DagBuildResult
// ============================================================================

/// Result of a successful DAG build operation.
#[derive(Debug, Clone)]
pub struct DagBuildResult {
    /// Root CID of the manifest (document identity).
    pub root_cid: ContentId,
    /// The built manifest.
    pub manifest: DocumentManifest,
    /// Number of data chunks stored.
    pub chunks_stored: usize,
    /// Total bytes of original content.
    pub total_bytes: u64,
}

// ============================================================================
// DagBuilder
// ============================================================================

/// Builder for content DAGs.
///
/// Takes raw bytes or a reader, chunks the content using the configured
/// chunker, stores each chunk as a block, builds a manifest (with tree
/// structure for large files), and returns the root CID.
///
/// # Example
///
/// ```ignore
/// let chunker = FastCdcChunker::new(ChunkingConfig::default());
/// let builder = DagBuilder::new(block_store.clone(), Box::new(chunker));
///
/// // From bytes
/// let result = builder.build_from_bytes(&data, metadata)?;
///
/// // From reader (streaming)
/// let file = File::open("large_file.bin")?;
/// let result = builder.build_from_reader(file, metadata)?;
/// ```
pub struct DagBuilder {
    block_store: Arc<BlockStore>,
    chunker: Box<dyn Chunker>,
}

impl DagBuilder {
    /// Create a new DAG builder.
    pub fn new(block_store: Arc<BlockStore>, chunker: Box<dyn Chunker>) -> Self {
        info!(
            chunker = chunker.name(),
            target_size = chunker.target_size(),
            "Created DAG builder"
        );
        Self {
            block_store,
            chunker,
        }
    }

    /// Get the chunking algorithm being used.
    pub fn algorithm(&self) -> ChunkingAlgorithm {
        self.chunker.algorithm()
    }

    /// Build a DAG from in-memory bytes.
    ///
    /// Chunks the entire input, stores each chunk as a block, builds a
    /// manifest (with tree structure if needed), and returns the result.
    ///
    /// # Arguments
    ///
    /// * `data` - The content bytes to publish
    /// * `metadata` - Metadata about the content
    ///
    /// # Returns
    ///
    /// A `DagBuildResult` containing the root CID and build statistics.
    #[instrument(skip(self, data, metadata), fields(size = data.len(), publisher = %metadata.publisher_did))]
    pub fn build_from_bytes(
        &self,
        data: &[u8],
        metadata: DagMetadata,
    ) -> DagResult<DagBuildResult> {
        let total_size = data.len() as u64;

        info!(
            size = total_size,
            name = ?metadata.name,
            content_type = ?metadata.content_type,
            "Building DAG from bytes"
        );

        // Handle empty content
        if data.is_empty() {
            debug!("Building DAG for empty content");
            return self.build_empty_dag(metadata);
        }

        // Chunk the data
        let chunks: Vec<ChunkData> = self.chunker.chunk(data);
        debug!(chunk_count = chunks.len(), "Chunked data");

        // Store chunks and collect CIDs
        let chunk_cids = self.store_chunks(chunks)?;

        // Build and store manifest
        self.build_and_store_manifest(chunk_cids, total_size, metadata)
    }

    /// Build a DAG from a reader (streaming).
    ///
    /// Reads content in a streaming fashion, chunking as it goes.
    /// More memory-efficient for large files in principle, though the
    /// current default implementation reads into memory first.
    ///
    /// # Arguments
    ///
    /// * `reader` - The reader to stream content from
    /// * `metadata` - Metadata about the content
    ///
    /// # Returns
    ///
    /// A `DagBuildResult` containing the root CID and build statistics.
    #[instrument(skip(self, reader, metadata), fields(publisher = %metadata.publisher_did))]
    pub fn build_from_reader<R: Read + Send>(
        &self,
        reader: R,
        metadata: DagMetadata,
    ) -> DagResult<DagBuildResult> {
        info!(
            name = ?metadata.name,
            content_type = ?metadata.content_type,
            "Building DAG from reader (streaming)"
        );

        let mut chunk_cids = Vec::new();
        let total_size: u64;
        let mut chunk_count = 0usize;

        // Current implementation: read all into memory, then chunk.
        // `stream_chunks` on trait objects is limited because it requires `Self: Sized`.
        let mut buffer = Vec::new();
        let mut reader = reader;
        reader.read_to_end(&mut buffer)?;
        total_size = buffer.len() as u64;

        if buffer.is_empty() {
            debug!("Building DAG for empty content from reader");
            return self.build_empty_dag(metadata);
        }

        // Chunk the buffered data
        for chunk_data in self.chunker.chunk(&buffer) {
            let block = Block::new(chunk_data.data);
            let cid = self.block_store.put(&block)?;
            chunk_cids.push(cid);
            chunk_count += 1;

            if chunk_count % 1000 == 0 {
                debug!(chunks_processed = chunk_count, "Streaming progress");
            }
        }

        info!(
            total_size,
            chunks = chunk_cids.len(),
            "Finished streaming content"
        );

        self.build_and_store_manifest(chunk_cids, total_size, metadata)
    }

    /// Build a DAG for empty content.
    fn build_empty_dag(&self, metadata: DagMetadata) -> DagResult<DagBuildResult> {
        let manifest = self.create_manifest(vec![], 0, metadata)?;
        let manifest_block = manifest.to_block()?;
        let root_cid = self.block_store.put(&manifest_block)?;

        info!(root_cid = %root_cid, "Built DAG for empty content");

        Ok(DagBuildResult {
            root_cid,
            manifest,
            chunks_stored: 0,
            total_bytes: 0,
        })
    }

    /// Store chunks and return their CIDs.
    fn store_chunks(&self, chunks: Vec<ChunkData>) -> DagResult<Vec<ContentId>> {
        let mut chunk_cids = Vec::with_capacity(chunks.len());

        for (i, chunk_data) in chunks.into_iter().enumerate() {
            let block = Block::new(chunk_data.data);
            let cid = self.block_store.put(&block)?;
            chunk_cids.push(cid);

            if (i + 1) % 1000 == 0 {
                debug!(chunks_stored = i + 1, "Chunk storage progress");
            }
        }

        debug!(total_chunks = chunk_cids.len(), "All chunks stored");
        Ok(chunk_cids)
    }

    /// Build manifest and store it.
    fn build_and_store_manifest(
        &self,
        chunk_cids: Vec<ContentId>,
        total_size: u64,
        metadata: DagMetadata,
    ) -> DagResult<DagBuildResult> {
        let chunks_stored = chunk_cids.len();

        // Build manifest (with tree if needed)
        let manifest = self.create_manifest(chunk_cids, total_size, metadata)?;

        // Store manifest block
        let manifest_block = manifest.to_block()?;
        let root_cid = self.block_store.put(&manifest_block)?;

        info!(
            root_cid = %root_cid,
            chunks = chunks_stored,
            total_size,
            structure = ?manifest.structure,
            "DAG built successfully"
        );

        Ok(DagBuildResult {
            root_cid,
            manifest,
            chunks_stored,
            total_bytes: total_size,
        })
    }

    /// Create a manifest from chunk CIDs.
    fn create_manifest(
        &self,
        chunk_cids: Vec<ContentId>,
        total_size: u64,
        metadata: DagMetadata,
    ) -> DagResult<DocumentManifest> {
        let mut builder = DocumentManifest::builder()
            .publisher_did(metadata.publisher_did)
            .chunks(chunk_cids)
            .total_size(total_size)
            .chunking_algorithm(self.chunker.algorithm());

        if let Some(name) = metadata.name {
            builder = builder.name(name);
        }

        if let Some(content_type) = metadata.content_type {
            builder = builder.content_type(content_type);
        }

        for (key, value) in metadata.metadata {
            builder = builder.metadata(key, value);
        }

        // Use build_with_store for automatic tree structure on large files
        let manifest = builder.build_with_store(&self.block_store)?;

        Ok(manifest)
    }
}

/// Configuration for DagReader batch operations.
#[derive(Debug, Clone)]
pub struct DagReaderConfig {
    /// Number of blocks to fetch in a single batch.
    /// Higher values = fewer network round-trips but more memory usage per batch.
    pub batch_size: usize,
}

impl Default for DagReaderConfig {
    fn default() -> Self {
        Self { batch_size: 50 }
    }
}

impl DagReaderConfig {
    /// Create config from environment variables.
    ///
    /// - `DAG_READER_BATCH_SIZE`: Batch size for block fetching (default: 50)
    pub fn from_env() -> Self {
        Self {
            batch_size: std::env::var("DAG_READER_BATCH_SIZE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(50),
        }
    }

    /// Create config with custom batch size.
    pub fn with_batch_size(batch_size: usize) -> Self {
        Self {
            batch_size: batch_size.max(1), // Minimum batch size of 1
        }
    }
}

// ============================================================================
// DagReader - Generic over ContentProvider
// ============================================================================

/// Reader for content DAGs.
///
/// Generic over `ContentProvider` to support reading from:
/// - `LocalProvider`: Fast local storage only
/// - `NetworkProvider`: Local-first with network fallback
///
/// # Example
///
/// ```ignore
/// // Local-only reading
/// let reader = DagReader::local(block_store.clone());
/// let content = reader.read_all(&root_cid).await?;
///
/// // Network-aware reading
/// let network_provider = NetworkProvider::new(local, network_manager, config);
/// let reader = DagReader::with_provider(network_provider);
/// let content = reader.read_all(&remote_cid).await?;
/// ```
pub struct DagReader<P: ContentProvider> {
    provider: P,
    config: DagReaderConfig,
}

impl DagReader<LocalProvider> {
    /// Create a DAG reader for local-only access.
    ///
    /// This is a convenience constructor for reading from local BlockStore.
    ///
    /// # Arguments
    /// * `block_store` - The block store to read from
    pub fn local(block_store: Arc<BlockStore>) -> Self {
        debug!("Creating local-only DagReader");
        Self {
            provider: LocalProvider::new(block_store),
            config: DagReaderConfig::default(),
        }
    }
}

impl<P: ContentProvider> DagReader<P> {
    /// Create a DAG reader with a custom provider.
    ///
    /// # Arguments
    /// * `provider` - The content provider to use for block retrieval
    pub fn with_provider(provider: P) -> Self {
        debug!("Creating DagReader with custom provider");
        Self {
            provider,
            config: DagReaderConfig::default(),
        }
    }

    /// Create a DAG reader with custom configuration.
    pub fn with(provider: P, config: DagReaderConfig) -> Self {
        debug!(
            batch_size = config.batch_size,
            "Creating DagReader with config"
        );
        Self { provider, config }
    }

    /// Get a reference to the underlying provider.
    pub fn provider(&self) -> &P {
        &self.provider
    }

    /// Read and parse the manifest from a root CID.
    ///
    /// # Arguments
    /// * `root_cid` - The root CID of the manifest
    ///
    /// # Returns
    /// The parsed `DocumentManifest`.
    #[instrument(skip(self), fields(cid = %root_cid))]
    pub async fn read_manifest(&self, root_cid: &ContentId) -> DagResult<DocumentManifest> {
        debug!("Reading manifest");

        let block = self
            .provider
            .get_block(root_cid)
            .await?
            .ok_or_else(|| DagError::ManifestNotFound(root_cid.clone()))?;

        let manifest = DocumentManifest::from_dag_cbor(block.data())
            .map_err(|e| DagError::InvalidManifest(e.to_string()))?;

        debug!(
            name = ?manifest.name,
            chunks = manifest.chunk_count(),
            size = manifest.total_size,
            structure = ?manifest.structure,
            "Manifest read successfully"
        );

        Ok(manifest)
    }

    /// Fetch blocks in batches, returning them in order.
    ///
    /// This is more efficient than fetching one at a time, especially for
    /// network providers where it enables parallel fetching.
    #[instrument(skip(self, cids), fields(total = cids.len(), batch_size = self.config.batch_size))]
    async fn fetch_blocks_batched(&self, cids: &[ContentId]) -> DagResult<Vec<Block>> {
        if cids.is_empty() {
            return Ok(vec![]);
        }

        let batch_size = self.config.batch_size;
        let total_chunks = cids.len();
        let num_batches = (total_chunks + batch_size - 1) / batch_size;

        debug!(
            total_chunks,
            batch_size, num_batches, "Fetching blocks in batches"
        );

        let mut all_blocks = Vec::with_capacity(total_chunks);

        for (batch_idx, batch) in cids.chunks(batch_size).enumerate() {
            let batch_results = self.provider.get_blocks(batch).await?;

            // Verify all blocks in batch were found
            for (i, result) in batch_results.into_iter().enumerate() {
                let global_idx = batch_idx * batch_size + i;
                match result {
                    Some(block) => all_blocks.push(block),
                    None => {
                        return Err(DagError::ChunkNotFound(cids[global_idx].clone()));
                    }
                }
            }

            if (batch_idx + 1) % 10 == 0 || batch_idx + 1 == num_batches {
                debug!(
                    batch = batch_idx + 1,
                    total_batches = num_batches,
                    blocks_fetched = all_blocks.len(),
                    "Batch fetch progress"
                );
            }
        }

        info!(
            total_blocks = all_blocks.len(),
            batches = num_batches,
            "Batch fetch complete"
        );

        Ok(all_blocks)
    }

    /// Read all content from a DAG.
    ///
    /// Fetches the manifest, retrieves all chunks (traversing tree if needed),
    /// and concatenates them into the original content.
    ///
    /// # Arguments
    /// * `root_cid` - The root CID of the manifest
    ///
    /// # Returns
    /// The reconstructed content as bytes.
    #[instrument(skip(self), fields(cid = %root_cid))]
    pub async fn read_all(&self, root_cid: &ContentId) -> DagResult<Vec<u8>> {
        let manifest = self.read_manifest(root_cid).await?;

        info!(
            name = ?manifest.name,
            size = manifest.total_size,
            chunks = manifest.chunk_count(),
            batch_size = self.config.batch_size,
            "Reading all content with batch fetching"
        );

        // Get all chunk CIDs
        let tree_reader = TreeReader::with(&self.provider, self.config.batch_size);
        let chunk_cids = tree_reader.get_all_chunks(&manifest).await?;

        // Fetch all chunks in batches
        let blocks = self.fetch_blocks_batched(&chunk_cids).await?;

        // Concatenate content
        let mut content = Vec::with_capacity(manifest.total_size as usize);
        for block in blocks {
            content.extend_from_slice(block.data());
        }

        info!(
            bytes = content.len(),
            chunks = chunk_cids.len(),
            "Content read successfully"
        );

        Ok(content)
    }

    /// Read a byte range from a DAG.
    ///
    /// Efficiently fetches only the chunks needed to satisfy the range request.
    ///
    /// # Arguments
    /// * `root_cid` - The root CID of the manifest
    /// * `range` - The byte range to read (start..end)
    ///
    /// # Returns
    /// The bytes within the specified range.
    #[instrument(skip(self), fields(cid = %root_cid, start = range.start, end = range.end))]
    pub async fn read_range(&self, root_cid: &ContentId, range: Range<u64>) -> DagResult<Vec<u8>> {
        let manifest = self.read_manifest(root_cid).await?;

        // Validate range
        if range.start >= manifest.total_size {
            return Err(DagError::RangeOutOfBounds {
                start: range.start,
                end: range.end,
                size: manifest.total_size,
            });
        }

        let end = range.end.min(manifest.total_size);
        let start = range.start;
        let range_size = (end - start) as usize;

        debug!(
            start,
            end,
            range_size,
            total_size = manifest.total_size,
            "Reading byte range"
        );

        // Get all chunk CIDs (we need them to calculate offsets)
        let tree_reader = TreeReader::with(&self.provider, self.config.batch_size);
        let chunk_cids = tree_reader.get_all_chunks(&manifest).await?;

        let mut result = Vec::with_capacity(range_size);
        let mut current_offset = 0u64;

        for cid in chunk_cids.iter() {
            // Fetch chunk to get its size
            let block = self
                .provider
                .get_block(cid)
                .await?
                .ok_or_else(|| DagError::ChunkNotFound(cid.clone()))?;

            let chunk_size = block.data().len() as u64;
            let chunk_end = current_offset + chunk_size;

            // Check if this chunk overlaps with our range
            if chunk_end > start && current_offset < end {
                // Calculate the portion of this chunk we need
                let chunk_start = if current_offset < start {
                    (start - current_offset) as usize
                } else {
                    0
                };

                let chunk_end_offset = if chunk_end > end {
                    (end - current_offset) as usize
                } else {
                    chunk_size as usize
                };

                result.extend_from_slice(&block.data()[chunk_start..chunk_end_offset]);

                // Early exit if we have all the data we need
                if result.len() >= range_size {
                    break;
                }
            }

            // Skip chunks that come after our range
            if current_offset >= end {
                break;
            }

            current_offset = chunk_end;
        }

        debug!(bytes_read = result.len(), "Range read complete");
        Ok(result)
    }

    /// Collect all chunk data asynchronously.
    ///
    /// Fetches all chunks and returns them as a Vec of byte vectors.
    /// Suitable for streaming or processing chunks individually.
    ///
    /// # Arguments
    /// * `root_cid` - The root CID of the manifest
    ///
    /// # Returns
    /// A vector of chunk data (each chunk as Vec<u8>).
    #[instrument(skip(self), fields(cid = %root_cid))]
    pub async fn collect_chunks(&self, root_cid: &ContentId) -> DagResult<Vec<Vec<u8>>> {
        let manifest = self.read_manifest(root_cid).await?;

        debug!(
            chunks = manifest.chunk_count(),
            batch_size = self.config.batch_size,
            "Collecting chunks with batch fetching"
        );

        let tree_reader = TreeReader::with(&self.provider, self.config.batch_size);
        let chunk_cids = tree_reader.get_all_chunks(&manifest).await?;

        // Fetch all chunks in batches
        let blocks = self.fetch_blocks_batched(&chunk_cids).await?;

        // Convert to Vec<Vec<u8>>
        let chunks: Vec<Vec<u8>> = blocks.into_iter().map(|b| b.data().to_vec()).collect();

        info!(
            chunks_collected = chunks.len(),
            "Chunks collected successfully"
        );

        Ok(chunks)
    }

    /// Get content metadata without reading the full content.
    ///
    /// # Arguments
    /// * `root_cid` - The root CID of the manifest
    ///
    /// # Returns
    /// The manifest containing all metadata.
    pub async fn get_metadata(&self, root_cid: &ContentId) -> DagResult<DocumentManifest> {
        self.read_manifest(root_cid).await
    }
}

// ============================================================================
// DagChunkIterator
// ============================================================================

/// Lazy iterator over chunk data from a DAG.
pub struct DagChunkIterator<'a> {
    block_store: &'a BlockStore,
    manifest: Arc<DocumentManifest>,

    // Tree traversal state (for lazy iteration)
    /// Stack of (node_cids, current_index) for tree traversal
    stack: Vec<(Vec<ContentId>, usize)>,
    /// Current leaf node's chunks being yielded
    current_chunks: Vec<ContentId>,
    current_chunk_index: usize,
}

impl<'a> DagChunkIterator<'a> {
    pub fn new(block_store: &'a BlockStore, manifest: Arc<DocumentManifest>) -> Self {
        // Initialize stack with manifest's children
        let initial_children = manifest.children.clone();
        let is_flat = manifest.is_flat();

        let (stack, current_chunks) = if is_flat {
            // Flat: children are chunks, no tree traversal needed
            (vec![], initial_children)
        } else {
            // Tree: start traversal from root children
            (vec![(initial_children, 0)], Vec::new())
        };

        Self {
            block_store,
            manifest,
            stack,
            current_chunks,
            current_chunk_index: 0,
        }
    }

    /// Fetch the next chunk CID, traversing tree lazily
    fn next_chunk_cid(&mut self) -> Option<Result<ContentId, DagError>> {
        // First, yield from current chunks buffer
        if self.current_chunk_index < self.current_chunks.len() {
            let cid = self.current_chunks[self.current_chunk_index].clone();
            self.current_chunk_index += 1;
            return Some(Ok(cid));
        }

        // For flat manifests, we're done
        if self.manifest.is_flat() {
            return None;
        }

        // Need to load more tree nodes
        while let Some((node_cids, index)) = self.stack.last_mut() {
            if *index >= node_cids.len() {
                self.stack.pop();
                continue;
            }

            let cid = node_cids[*index].clone();
            *index += 1;

            // Fetch and parse tree node
            let block = match self.block_store.get(&cid) {
                Ok(Some(b)) => b,
                Ok(None) => return Some(Err(DagError::ChunkNotFound(cid))),
                Err(e) => return Some(Err(DagError::BlockStore(e))),
            };

            let node = match TreeNode::from_dag_cbor(block.data()) {
                Ok(n) => n,
                Err(e) => return Some(Err(DagError::BlockStore(e))),
            };

            if node.is_leaf_level {
                // Leaf node: children are chunk CIDs
                self.current_chunks = node.children;
                self.current_chunk_index = 0;

                if !self.current_chunks.is_empty() {
                    let chunk_cid = self.current_chunks[0].clone();
                    self.current_chunk_index = 1;
                    return Some(Ok(chunk_cid));
                }
            } else {
                // Internal node: push children to traverse
                self.stack.push((node.children, 0));
            }
        }

        None
    }
}

impl<'a> Iterator for DagChunkIterator<'a> {
    type Item = DagResult<Vec<u8>>;

    fn next(&mut self) -> Option<Self::Item> {
        // Get next chunk CID lazily
        let cid = match self.next_chunk_cid()? {
            Ok(cid) => cid,
            Err(e) => return Some(Err(e)),
        };

        // Fetch chunk data
        match self.block_store.get(&cid) {
            Ok(Some(block)) => Some(Ok(block.data().to_vec())),
            Ok(None) => Some(Err(DagError::ChunkNotFound(cid))),
            Err(e) => Some(Err(DagError::BlockStore(e))),
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::storage::content::BlockStoreConfig;
    use crate::modules::storage::content::chunking::{
        ChunkingConfig, FastCdcChunker, FixedChunker,
    };
    use tempfile::TempDir;

    const TEST_DID: &str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";

    fn create_test_store() -> (Arc<BlockStore>, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = BlockStoreConfig {
            db_path: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        let store = Arc::new(BlockStore::new(config).unwrap());
        (store, temp_dir)
    }

    fn create_test_chunker() -> Box<dyn Chunker> {
        let config = ChunkingConfig {
            min_size: 256,
            target_size: 1024,
            max_size: 4096,
        };
        Box::new(FastCdcChunker::new(config))
    }

    fn create_fixed_chunker(chunk_size: usize) -> Box<dyn Chunker> {
        let config = ChunkingConfig {
            min_size: chunk_size,
            target_size: chunk_size,
            max_size: chunk_size,
        };
        Box::new(FixedChunker::new(config))
    }

    // ========================================
    // DagMetadata Tests (unchanged - sync)
    // ========================================

    #[test]
    fn test_dag_metadata_new() {
        let metadata = DagMetadata::new(TEST_DID);
        assert_eq!(metadata.publisher_did, TEST_DID);
        assert!(metadata.name.is_none());
        assert!(metadata.content_type.is_none());
        assert!(metadata.metadata.is_empty());
    }

    #[test]
    fn test_dag_metadata_builder() {
        let metadata = DagMetadata::new(TEST_DID)
            .name("test.pdf")
            .content_type("application/pdf")
            .with_metadata("author", "Alice");

        assert_eq!(metadata.name, Some("test.pdf".into()));
        assert_eq!(metadata.content_type, Some("application/pdf".into()));
        assert_eq!(metadata.metadata.get("author"), Some(&"Alice".to_string()));
    }

    // ========================================
    // DagBuilder Tests - Empty Content
    // ========================================

    #[tokio::test]
    async fn test_build_empty_content() {
        let (store, _dir) = create_test_store();
        let chunker = create_test_chunker();
        let builder = DagBuilder::new(store.clone(), chunker);

        let metadata = DagMetadata::new(TEST_DID).name("empty.txt");
        let result = builder.build_from_bytes(&[], metadata).unwrap();

        assert_eq!(result.chunks_stored, 0);
        assert_eq!(result.total_bytes, 0);
        assert_eq!(result.manifest.total_size, 0);

        // Verify we can read it back
        let reader = DagReader::local(store.clone());
        let content = reader.read_all(&result.root_cid).await.unwrap();
        assert!(content.is_empty());
    }

    // ========================================
    // DagBuilder Tests - Single Chunk
    // ========================================

    #[tokio::test]
    async fn test_build_single_chunk() {
        let (store, _dir) = create_test_store();
        let chunker = create_fixed_chunker(1024);
        let builder = DagBuilder::new(store.clone(), chunker);

        let data = b"Hello, Flow!";
        let metadata = DagMetadata::new(TEST_DID)
            .name("hello.txt")
            .content_type("text/plain");

        let result = builder.build_from_bytes(data, metadata).unwrap();

        assert_eq!(result.chunks_stored, 1);
        assert_eq!(result.total_bytes, data.len() as u64);
        assert_eq!(result.manifest.name, Some("hello.txt".into()));
        assert_eq!(result.manifest.content_type, Some("text/plain".into()));

        // Verify round-trip
        let reader = DagReader::local(store.clone());
        let content = reader.read_all(&result.root_cid).await.unwrap();
        assert_eq!(content, data);
    }

    // ========================================
    // DagBuilder Tests - Multiple Chunks
    // ========================================

    #[tokio::test]
    async fn test_build_multiple_chunks() {
        let (store, _dir) = create_test_store();
        let chunker = create_fixed_chunker(256);
        let builder = DagBuilder::new(store.clone(), chunker);

        // Create data that will span multiple chunks
        let data: Vec<u8> = (0..1024).map(|i| (i % 256) as u8).collect();
        let metadata = DagMetadata::new(TEST_DID).name("multi.bin");

        let result = builder.build_from_bytes(&data, metadata).unwrap();

        assert_eq!(result.chunks_stored, 4); // 1024 / 256 = 4 chunks
        assert_eq!(result.total_bytes, 1024);

        // Verify round-trip
        let reader = DagReader::local(store.clone());
        let content = reader.read_all(&result.root_cid).await.unwrap();
        assert_eq!(content, data);
    }

    // ========================================
    // DagBuilder Tests - Large File
    // ========================================

    #[tokio::test]
    async fn test_build_large_file() {
        let (store, _dir) = create_test_store();
        let chunker = create_test_chunker();
        let builder = DagBuilder::new(store.clone(), chunker);

        // Create larger data (100KB)
        let data: Vec<u8> = (0..102400).map(|i| (i % 256) as u8).collect();
        let metadata = DagMetadata::new(TEST_DID)
            .name("large.bin")
            .content_type("application/octet-stream");

        let result = builder.build_from_bytes(&data, metadata).unwrap();

        assert!(result.chunks_stored > 1);
        assert_eq!(result.total_bytes, 102400);

        // Verify round-trip
        let reader = DagReader::local(store.clone());
        let content = reader.read_all(&result.root_cid).await.unwrap();
        assert_eq!(content, data);
    }

    // ========================================
    // DagBuilder Tests - Streaming
    // ========================================

    #[tokio::test]
    async fn test_build_from_reader() {
        let (store, _dir) = create_test_store();
        let chunker = create_fixed_chunker(256);
        let builder = DagBuilder::new(store.clone(), chunker);

        let data: Vec<u8> = (0..1024).map(|i| (i % 256) as u8).collect();
        let cursor = std::io::Cursor::new(data.clone());
        let metadata = DagMetadata::new(TEST_DID).name("streamed.bin");

        let result = builder.build_from_reader(cursor, metadata).unwrap();

        assert_eq!(result.chunks_stored, 4);
        assert_eq!(result.total_bytes, 1024);

        // Verify round-trip
        let reader = DagReader::local(store.clone());
        let content = reader.read_all(&result.root_cid).await.unwrap();
        assert_eq!(content, data);
    }

    // ========================================
    // DagReader Tests - Read Manifest
    // ========================================

    #[tokio::test]
    async fn test_read_manifest() {
        let (store, _dir) = create_test_store();
        let chunker = create_test_chunker();
        let builder = DagBuilder::new(store.clone(), chunker);

        let data = b"Test content";
        let metadata = DagMetadata::new(TEST_DID)
            .name("manifest-test.txt")
            .with_metadata("key", "value");

        let result = builder.build_from_bytes(data, metadata).unwrap();

        let reader = DagReader::local(store.clone());
        let manifest = reader.read_manifest(&result.root_cid).await.unwrap();

        assert_eq!(manifest.name, Some("manifest-test.txt".into()));
        assert_eq!(manifest.publisher_did, TEST_DID);
        assert_eq!(manifest.get_metadata("key"), Some("value"));
    }

    #[tokio::test]
    async fn test_read_manifest_not_found() {
        let (store, _dir) = create_test_store();
        let reader = DagReader::local(store.clone());

        let fake_cid = ContentId::from_bytes(b"nonexistent");
        let result = reader.read_manifest(&fake_cid).await;

        assert!(matches!(result, Err(DagError::ManifestNotFound(_))));
    }

    // ========================================
    // DagReader Tests - Read Range
    // ========================================

    #[tokio::test]
    async fn test_read_range_full() {
        let (store, _dir) = create_test_store();
        let chunker = create_fixed_chunker(256);
        let builder = DagBuilder::new(store.clone(), chunker);

        let data: Vec<u8> = (0..1024).map(|i| (i % 256) as u8).collect();
        let metadata = DagMetadata::new(TEST_DID);
        let result = builder.build_from_bytes(&data, metadata).unwrap();

        let reader = DagReader::local(store.clone());
        let content = reader.read_range(&result.root_cid, 0..1024).await.unwrap();
        assert_eq!(content, data);
    }

    #[tokio::test]
    async fn test_read_range_partial_within_chunk() {
        let (store, _dir) = create_test_store();
        let chunker = create_fixed_chunker(256);
        let builder = DagBuilder::new(store.clone(), chunker);

        let data: Vec<u8> = (0..1024).map(|i| (i % 256) as u8).collect();
        let metadata = DagMetadata::new(TEST_DID);
        let result = builder.build_from_bytes(&data, metadata).unwrap();

        let reader = DagReader::local(store.clone());
        let content = reader.read_range(&result.root_cid, 10..100).await.unwrap();
        assert_eq!(content, &data[10..100]);
    }

    #[tokio::test]
    async fn test_read_range_across_chunks() {
        let (store, _dir) = create_test_store();
        let chunker = create_fixed_chunker(256);
        let builder = DagBuilder::new(store.clone(), chunker);

        let data: Vec<u8> = (0..1024).map(|i| (i % 256) as u8).collect();
        let metadata = DagMetadata::new(TEST_DID);
        let result = builder.build_from_bytes(&data, metadata).unwrap();

        let reader = DagReader::local(store.clone());
        // Range that spans chunks 1 and 2 (256-byte chunks)
        let content = reader.read_range(&result.root_cid, 200..400).await.unwrap();
        assert_eq!(content, &data[200..400]);
    }

    #[tokio::test]
    async fn test_read_range_from_middle_to_end() {
        let (store, _dir) = create_test_store();
        let chunker = create_fixed_chunker(256);
        let builder = DagBuilder::new(store.clone(), chunker);

        let data: Vec<u8> = (0..1024).map(|i| (i % 256) as u8).collect();
        let metadata = DagMetadata::new(TEST_DID);
        let result = builder.build_from_bytes(&data, metadata).unwrap();

        let reader = DagReader::local(store.clone());
        let content = reader
            .read_range(&result.root_cid, 512..1024)
            .await
            .unwrap();
        assert_eq!(content, &data[512..1024]);
    }

    #[tokio::test]
    async fn test_read_range_clamps_end() {
        let (store, _dir) = create_test_store();
        let chunker = create_fixed_chunker(256);
        let builder = DagBuilder::new(store.clone(), chunker);

        let data: Vec<u8> = (0..1024).map(|i| (i % 256) as u8).collect();
        let metadata = DagMetadata::new(TEST_DID);
        let result = builder.build_from_bytes(&data, metadata).unwrap();

        let reader = DagReader::local(store.clone());
        // End beyond file size should be clamped
        let content = reader
            .read_range(&result.root_cid, 900..2000)
            .await
            .unwrap();
        assert_eq!(content, &data[900..1024]);
    }

    #[tokio::test]
    async fn test_read_range_out_of_bounds() {
        let (store, _dir) = create_test_store();
        let chunker = create_fixed_chunker(256);
        let builder = DagBuilder::new(store.clone(), chunker);

        let data = b"Small content";
        let metadata = DagMetadata::new(TEST_DID);
        let result = builder.build_from_bytes(data, metadata).unwrap();

        let reader = DagReader::local(store.clone());
        let err = reader
            .read_range(&result.root_cid, 1000..2000)
            .await
            .unwrap_err();
        assert!(matches!(err, DagError::RangeOutOfBounds { .. }));
    }

    // ========================================
    // DagReader Tests - Collect Chunks (replaces iter_chunks)
    // ========================================

    #[tokio::test]
    async fn test_collect_chunks() {
        let (store, _dir) = create_test_store();
        let chunker = create_fixed_chunker(256);
        let builder = DagBuilder::new(store.clone(), chunker);

        let data: Vec<u8> = (0..1024).map(|i| (i % 256) as u8).collect();
        let metadata = DagMetadata::new(TEST_DID);
        let result = builder.build_from_bytes(&data, metadata).unwrap();

        let reader = DagReader::local(store.clone());
        let chunks = reader.collect_chunks(&result.root_cid).await.unwrap();

        assert_eq!(chunks.len(), 4); // 1024 / 256 = 4 chunks

        // Verify concatenation matches original
        let reconstructed: Vec<u8> = chunks.into_iter().flatten().collect();
        assert_eq!(reconstructed, data);
    }

    #[tokio::test]
    async fn test_collect_chunks_single() {
        let (store, _dir) = create_test_store();
        let chunker = create_fixed_chunker(1024);
        let builder = DagBuilder::new(store.clone(), chunker);

        let data = b"Small data";
        let metadata = DagMetadata::new(TEST_DID);
        let result = builder.build_from_bytes(data, metadata).unwrap();

        let reader = DagReader::local(store.clone());
        let chunks = reader.collect_chunks(&result.root_cid).await.unwrap();

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], data.to_vec());
    }

    // ========================================
    // DagReader Tests - Get Metadata
    // ========================================

    #[tokio::test]
    async fn test_get_metadata() {
        let (store, _dir) = create_test_store();
        let chunker = create_test_chunker();
        let builder = DagBuilder::new(store.clone(), chunker);

        let data = b"Content for metadata test";
        let metadata = DagMetadata::new(TEST_DID)
            .name("metadata.txt")
            .content_type("text/plain")
            .with_metadata("version", "1.0");

        let result = builder.build_from_bytes(data, metadata).unwrap();

        let reader = DagReader::local(store.clone());
        let manifest = reader.get_metadata(&result.root_cid).await.unwrap();

        assert_eq!(manifest.name, Some("metadata.txt".into()));
        assert_eq!(manifest.content_type, Some("text/plain".into()));
        assert_eq!(manifest.total_size, data.len() as u64);
        assert_eq!(manifest.get_metadata("version"), Some("1.0"));
    }

    // ========================================
    // DagReader Tests - With Custom Provider
    // ========================================

    #[tokio::test]
    async fn test_with_custom_provider() {
        let (store, _dir) = create_test_store();
        let chunker = create_test_chunker();
        let builder = DagBuilder::new(store.clone(), chunker);

        let data = b"Custom provider test";
        let metadata = DagMetadata::new(TEST_DID);
        let result = builder.build_from_bytes(data, metadata).unwrap();

        // Create reader with explicit LocalProvider
        let local_provider = LocalProvider::new(store.clone());
        let reader = DagReader::with_provider(local_provider);

        let content = reader.read_all(&result.root_cid).await.unwrap();
        assert_eq!(content, data);
    }

    #[tokio::test]
    async fn test_provider_accessor() {
        let (store, _dir) = create_test_store();
        let reader = DagReader::local(store.clone());

        // Just verify we can access the provider
        let _provider = reader.provider();
    }

    // ========================================
    // Round-Trip Tests
    // ========================================

    #[tokio::test]
    async fn test_round_trip_deterministic() {
        let (store, _dir) = create_test_store();
        let chunker = create_test_chunker();
        let builder = DagBuilder::new(store.clone(), chunker);

        let data = b"Deterministic content test";
        let metadata = DagMetadata::new(TEST_DID).name("deterministic.txt");

        let result1 = builder.build_from_bytes(data, metadata.clone()).unwrap();

        // Build again with same content
        let chunker2 = create_test_chunker();
        let builder2 = DagBuilder::new(store.clone(), chunker2);
        let result2 = builder2.build_from_bytes(data, metadata).unwrap();

        // Root CIDs should be identical
        assert_eq!(result1.root_cid, result2.root_cid);
    }

    #[tokio::test]
    async fn test_round_trip_binary_data() {
        let (store, _dir) = create_test_store();
        let chunker = create_test_chunker();
        let builder = DagBuilder::new(store.clone(), chunker);

        // Binary data with all byte values
        let data: Vec<u8> = (0..=255).collect();
        let metadata = DagMetadata::new(TEST_DID)
            .name("binary.bin")
            .content_type("application/octet-stream");

        let result = builder.build_from_bytes(&data, metadata).unwrap();

        let reader = DagReader::local(store.clone());
        let content = reader.read_all(&result.root_cid).await.unwrap();
        assert_eq!(content, data);
    }

    // ========================================
    // DagChunkIterator Tests (sync, local-only)
    // ========================================

    #[test]
    fn test_dag_chunk_iterator_sync() {
        let (store, _dir) = create_test_store();
        let chunker = create_fixed_chunker(256);
        let builder = DagBuilder::new(store.clone(), chunker);

        let data: Vec<u8> = (0..1024).map(|i| (i % 256) as u8).collect();
        let metadata = DagMetadata::new(TEST_DID);
        let result = builder.build_from_bytes(&data, metadata).unwrap();

        // Use the sync DagChunkIterator directly with BlockStore
        let iter = DagChunkIterator::new(&store, Arc::new(result.manifest));

        let mut reconstructed = Vec::new();
        let mut chunk_count = 0;

        for chunk_result in iter {
            let chunk_data = chunk_result.unwrap();
            reconstructed.extend_from_slice(&chunk_data);
            chunk_count += 1;
        }

        assert_eq!(chunk_count, 4);
        assert_eq!(reconstructed, data);
    }

    // ========================================
    // Error Handling Tests
    // ========================================

    #[test]
    fn test_error_display() {
        let cid = ContentId::from_bytes(b"test");

        let err = DagError::ManifestNotFound(cid.clone());
        assert!(err.to_string().contains("Manifest not found"));

        let err = DagError::ChunkNotFound(cid.clone());
        assert!(err.to_string().contains("Chunk not found"));

        let err = DagError::RangeOutOfBounds {
            start: 100,
            end: 200,
            size: 50,
        };
        assert!(err.to_string().contains("out of bounds"));

        // Test provider error variant
        let provider_err = ContentProviderError::NotFound(cid);
        let err = DagError::Provider(provider_err);
        assert!(err.to_string().contains("Content provider error"));
    }

    #[test]
    fn test_error_from_content_provider_error() {
        let cid = ContentId::from_bytes(b"test");
        let provider_err = ContentProviderError::NotFound(cid);
        let dag_err: DagError = provider_err.into();

        assert!(matches!(dag_err, DagError::Provider(_)));
    }

    // ========================================================================
    // Batch Fetching Tests
    // ========================================================================

    #[tokio::test]
    async fn test_read_all_uses_batch_fetching() {
        let (store, _dir) = create_test_store();

        // Create content large enough to need multiple chunks
        let content: Vec<u8> = (0..100_000).map(|i| (i % 256) as u8).collect();

        let chunker = create_test_chunker();
        let builder = DagBuilder::new(store.clone(), chunker);

        let metadata = DagMetadata::new("did:key:test")
            .name("test.txt")
            .content_type("text/plain");

        let result = builder.build_from_bytes(&content, metadata).unwrap();

        // Read with small batch size to test batching logic
        let config = DagReaderConfig::with_batch_size(5);
        let reader = DagReader::with(LocalProvider::new(store.clone()), config);

        let read_content = reader.read_all(&result.root_cid).await.unwrap();
        assert_eq!(read_content, content);
    }

    #[tokio::test]
    async fn test_collect_chunks_uses_batch_fetching() {
        let (store, _dir) = create_test_store();

        let content: Vec<u8> = (0..50_000).map(|i| (i % 256) as u8).collect();

        let chunker = create_test_chunker();
        let builder = DagBuilder::new(store.clone(), chunker);

        let metadata = DagMetadata::new("did:key:test")
            .name("test.txt")
            .content_type("text/plain");

        let result = builder.build_from_bytes(&content, metadata).unwrap();

        // Small batch size
        let config = DagReaderConfig::with_batch_size(3);
        let reader = DagReader::with(LocalProvider::new(store.clone()), config);

        let chunks = reader.collect_chunks(&result.root_cid).await.unwrap();

        // Verify chunks reconstruct content
        let reconstructed: Vec<u8> = chunks.into_iter().flatten().collect();
        assert_eq!(reconstructed, content);
    }

    #[tokio::test]
    async fn test_read_range_with_batch_fetching() {
        let (store, _dir) = create_test_store();

        let content: Vec<u8> = (0..100_000).map(|i| (i % 256) as u8).collect();

        let chunker = create_test_chunker();
        let builder = DagBuilder::new(store.clone(), chunker);

        let metadata = DagMetadata::new("did:key:test")
            .name("test.txt")
            .content_type("text/plain");

        let result = builder.build_from_bytes(&content, metadata).unwrap();

        let config = DagReaderConfig::with_batch_size(5);
        let reader = DagReader::with(LocalProvider::new(store.clone()), config);

        // Read a range in the middle
        let range_start = 10_000;
        let range_end = 20_000;
        let range_content = reader
            .read_range(&result.root_cid, range_start..range_end)
            .await
            .unwrap();

        assert_eq!(
            range_content,
            &content[range_start as usize..range_end as usize]
        );
    }

    #[tokio::test]
    async fn test_batch_size_one_works() {
        let (store, _dir) = create_test_store();

        let content = b"test batch size one".to_vec();

        let chunker = create_test_chunker();
        let builder = DagBuilder::new(store.clone(), chunker);

        let metadata = DagMetadata::new("did:key:test")
            .name("test.txt")
            .content_type("text/plain");

        let result = builder.build_from_bytes(&content, metadata).unwrap();

        // Batch size of 1 (sequential)
        let config = DagReaderConfig::with_batch_size(1);
        let reader = DagReader::with(LocalProvider::new(store.clone()), config);

        let read_content = reader.read_all(&result.root_cid).await.unwrap();
        assert_eq!(read_content, content);
    }

    #[tokio::test]
    async fn test_large_batch_size() {
        let (store, _dir) = create_test_store();

        let content: Vec<u8> = (0..10_000).map(|i| (i % 256) as u8).collect();

        let chunker = create_test_chunker();
        let builder = DagBuilder::new(store.clone(), chunker);

        let metadata = DagMetadata::new("did:key:test")
            .name("test.txt")
            .content_type("text/plain");

        let result = builder.build_from_bytes(&content, metadata).unwrap();

        // Batch size larger than chunk count
        let config = DagReaderConfig::with_batch_size(1000);
        let reader = DagReader::with(LocalProvider::new(store.clone()), config);

        let read_content = reader.read_all(&result.root_cid).await.unwrap();
        assert_eq!(read_content, content);
    }
}
