//! Document manifest for published content.
//!
//! The `DocumentManifest` represents the root of a published document,
//! containing metadata and references to all chunks. For small files,
//! chunks are stored directly. For large files, a Merkle tree structure
//! is used with intermediate `TreeNode` blocks.
//!
//! The serialized manifest produces a CID that becomes the document's
//! network identity.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, info, warn};

use super::block::Block;
use super::block_store::BlockStore;
use super::chunking::ChunkingAlgorithm;
use super::cid::ContentId;
use super::error::BlockStoreError;

// ============================================================================
// Constants
// ============================================================================

/// Current manifest schema version.
pub const MANIFEST_VERSION: u32 = 1;

/// Default fanout for tree nodes (max children per node).
/// 174 links × ~40 bytes = ~7KB per node (well under 1MB block limit).
pub const DEFAULT_TREE_FANOUT: usize = 174;

/// Threshold for switching from flat to tree structure.
/// ~40,000 chunks × 40 bytes = ~1.6MB manifest (reasonable).
pub const FLAT_CHUNK_THRESHOLD: usize = 40_000;

// ============================================================================
// ManifestStructure
// ============================================================================

/// Structure type for chunk references in a manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ManifestStructure {
    /// Direct chunk list (for small files).
    /// The `children` field in DocumentManifest contains chunk CIDs directly.
    Flat,

    /// Merkle tree structure (for large files).
    /// The `children` field contains CIDs of TreeNode blocks.
    Tree {
        /// Tree depth (1 = one level of TreeNodes above chunks).
        depth: u8,
        /// Maximum children per node.
        fanout: u16,
        /// Total chunk count (for verification).
        chunk_count: u64,
    },
}

impl Default for ManifestStructure {
    fn default() -> Self {
        ManifestStructure::Flat
    }
}

// ============================================================================
// DocumentManifest
// ============================================================================

/// Document manifest containing metadata and chunk references.
///
/// This is the "root" of a published document - its CID becomes the
/// document's identity in the Flow network. The manifest is serialized
/// to DAG-CBOR and stored as a block in the BlockStore.
///
/// # Structure
///
/// - **Flat**: For small files (<40K chunks), `children` contains chunk CIDs directly.
/// - **Tree**: For large files, `children` contains TreeNode CIDs forming a Merkle tree.
///
/// # Example
///
/// ```ignore
/// // Small file (flat structure)
/// let manifest = DocumentManifest::builder()
///     .publisher_did("did:key:z6Mk...")
///     .name("report.pdf")
///     .content_type("application/pdf")
///     .chunks(chunk_cids)
///     .total_size(file_size)
///     .build_flat()?;
///
/// // Large file (tree structure, requires BlockStore)
/// let manifest = DocumentManifest::builder()
///     .publisher_did("did:key:z6Mk...")
///     .chunks(many_chunk_cids)
///     .total_size(file_size)
///     .build_with_store(&block_store)?;
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentManifest {
    /// Schema version for forward compatibility.
    pub version: u32,

    /// Original filename (if known).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub name: Option<String>,

    /// MIME type (e.g., "application/pdf").
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub content_type: Option<String>,

    /// Total size of the original content in bytes.
    pub total_size: u64,

    /// Structure type: Flat (direct) or Tree (intermediate nodes).
    #[serde(default)]
    pub structure: ManifestStructure,

    /// Child CIDs: chunk CIDs (flat) or TreeNode CIDs (tree).
    pub children: Vec<ContentId>,

    /// Algorithm used for chunking.
    pub chunking_algorithm: ChunkingAlgorithm,

    /// Publisher's DID
    pub publisher_did: String,

    /// Creation timestamp (Unix epoch seconds).
    pub created_at: u64,

    /// Extensible key-value metadata.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, String>,
}

impl DocumentManifest {
    /// Create a builder for constructing a DocumentManifest.
    pub fn builder() -> DocumentManifestBuilder {
        DocumentManifestBuilder::new()
    }

    /// Serialize the manifest to DAG-CBOR bytes.
    ///
    /// DAG-CBOR provides canonical serialization, ensuring the same
    /// manifest always produces the same bytes and CID.
    pub fn to_dag_cbor(&self) -> Result<Vec<u8>, BlockStoreError> {
        serde_ipld_dagcbor::to_vec(self).map_err(|e| {
            BlockStoreError::Serialization(format!("Failed to serialize manifest: {}", e))
        })
    }

    /// Deserialize a manifest from DAG-CBOR bytes.
    pub fn from_dag_cbor(bytes: &[u8]) -> Result<Self, BlockStoreError> {
        serde_ipld_dagcbor::from_slice(bytes).map_err(|e| {
            BlockStoreError::Serialization(format!("Failed to deserialize manifest: {}", e))
        })
    }

    /// Compute the root CID of this manifest.
    ///
    /// The CID is computed from the DAG-CBOR serialized form using
    /// SHA2-256 with the DAG-CBOR codec (0x71).
    ///
    /// This CID becomes the document's identity in the network.
    pub fn root_cid(&self) -> Result<ContentId, BlockStoreError> {
        let bytes = self.to_dag_cbor()?;
        let cid = ContentId::from_dag_cbor(&bytes);

        info!(
            cid = %cid,
            name = ?self.name,
            structure = ?self.structure,
            children = self.children.len(),
            size = self.total_size,
            "Computed manifest root CID"
        );

        Ok(cid)
    }

    /// Convert the manifest to a Block for storage.
    ///
    /// The block contains the DAG-CBOR serialized manifest and
    /// links to all child CIDs (chunks or tree nodes).
    pub fn to_block(&self) -> Result<Block, BlockStoreError> {
        let bytes = self.to_dag_cbor()?;
        let links = self.children.clone();

        debug!(
            links = links.len(),
            size = bytes.len(),
            "Creating manifest block"
        );

        Ok(Block::from_dag_cbor(bytes, links))
    }

    /// Check if this manifest uses tree structure.
    pub fn is_tree(&self) -> bool {
        matches!(self.structure, ManifestStructure::Tree { .. })
    }

    /// Check if this manifest uses flat structure.
    pub fn is_flat(&self) -> bool {
        matches!(self.structure, ManifestStructure::Flat)
    }

    /// Get total chunk count.
    ///
    /// For flat manifests, this is the number of children.
    /// For tree manifests, this is the stored chunk_count.
    pub fn chunk_count(&self) -> usize {
        match &self.structure {
            ManifestStructure::Flat => self.children.len(),
            ManifestStructure::Tree { chunk_count, .. } => *chunk_count as usize,
        }
    }

    /// Get the tree depth (0 for flat manifests).
    pub fn tree_depth(&self) -> u8 {
        match &self.structure {
            ManifestStructure::Flat => 0,
            ManifestStructure::Tree { depth, .. } => *depth,
        }
    }

    /// Check if this manifest has a name.
    pub fn has_name(&self) -> bool {
        self.name.is_some()
    }

    /// Check if this manifest has a content type.
    pub fn has_content_type(&self) -> bool {
        self.content_type.is_some()
    }

    /// Get a metadata value by key.
    pub fn get_metadata(&self, key: &str) -> Option<&str> {
        self.metadata.get(key).map(|s| s.as_str())
    }

    /// Add or update a metadata key-value pair.
    pub fn set_metadata(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.metadata.insert(key.into(), value.into());
    }
}

// ============================================================================
// TreeNode
// ============================================================================

/// Intermediate tree node for large file manifests.
///
/// Each TreeNode contains links to either:
/// - Other TreeNode CIDs (internal nodes), or
/// - Chunk CIDs (leaf level)
///
/// TreeNodes are stored as blocks in the BlockStore.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreeNode {
    /// Child CIDs (TreeNodes or chunks depending on level).
    pub children: Vec<ContentId>,

    /// Whether children are chunks (true) or more TreeNodes (false).
    pub is_leaf_level: bool,
}

impl TreeNode {
    /// Create a new tree node.
    pub fn new(children: Vec<ContentId>, is_leaf_level: bool) -> Self {
        Self {
            children,
            is_leaf_level,
        }
    }

    /// Serialize to DAG-CBOR.
    pub fn to_dag_cbor(&self) -> Result<Vec<u8>, BlockStoreError> {
        serde_ipld_dagcbor::to_vec(self).map_err(|e| {
            BlockStoreError::Serialization(format!("Failed to serialize tree node: {}", e))
        })
    }

    /// Deserialize from DAG-CBOR.
    pub fn from_dag_cbor(bytes: &[u8]) -> Result<Self, BlockStoreError> {
        serde_ipld_dagcbor::from_slice(bytes).map_err(|e| {
            BlockStoreError::Serialization(format!("Failed to deserialize tree node: {}", e))
        })
    }

    /// Convert to a Block for storage.
    pub fn to_block(&self) -> Result<Block, BlockStoreError> {
        let bytes = self.to_dag_cbor()?;
        let links = self.children.clone();
        Ok(Block::from_dag_cbor(bytes, links))
    }

    /// Compute the CID of this node.
    pub fn cid(&self) -> Result<ContentId, BlockStoreError> {
        let bytes = self.to_dag_cbor()?;
        Ok(ContentId::from_dag_cbor(&bytes))
    }
}

// ============================================================================
// TreeConfig
// ============================================================================

/// Configuration for tree building.
#[derive(Debug, Clone)]
pub struct TreeConfig {
    /// Maximum children per node.
    pub fanout: usize,
    /// Chunk count threshold for switching to tree structure.
    pub flat_threshold: usize,
}

impl Default for TreeConfig {
    fn default() -> Self {
        Self {
            fanout: DEFAULT_TREE_FANOUT,
            flat_threshold: FLAT_CHUNK_THRESHOLD,
        }
    }
}

// ============================================================================
// TreeBuilder
// ============================================================================

/// Builder for constructing Merkle trees from chunk CIDs.
pub struct TreeBuilder {
    config: TreeConfig,
}

impl Default for TreeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl TreeBuilder {
    /// Create a new tree builder with default config.
    pub fn new() -> Self {
        Self {
            config: TreeConfig::default(),
        }
    }

    /// Create with custom configuration.
    pub fn with_config(config: TreeConfig) -> Self {
        Self { config }
    }

    /// Determine if chunks should use tree structure.
    pub fn should_use_tree(&self, chunk_count: usize) -> bool {
        chunk_count > self.config.flat_threshold
    }

    /// Build tree from chunk CIDs.
    ///
    /// Returns `(root_children, structure, blocks_to_store)`.
    ///
    /// For small files, returns chunks directly with `ManifestStructure::Flat`.
    /// For large files, builds a Merkle tree and returns intermediate node blocks.
    pub fn build(
        &self,
        chunks: Vec<ContentId>,
    ) -> Result<(Vec<ContentId>, ManifestStructure, Vec<Block>), BlockStoreError> {
        let chunk_count = chunks.len();

        if !self.should_use_tree(chunk_count) {
            debug!(chunk_count, "Using flat manifest structure");
            return Ok((chunks, ManifestStructure::Flat, vec![]));
        }

        info!(
            chunk_count,
            fanout = self.config.fanout,
            "Building tree structure for large file"
        );

        let mut blocks_to_store = Vec::new();
        let mut current_level: Vec<ContentId> = chunks;
        let mut depth = 0u8;
        let mut is_leaf_level = true;

        // Build tree bottom-up
        while current_level.len() > self.config.fanout {
            depth += 1;
            let mut next_level = Vec::new();

            for node_children in current_level.chunks(self.config.fanout) {
                let node = TreeNode::new(node_children.to_vec(), is_leaf_level);
                let block = node.to_block()?;
                let cid = block.cid().clone();

                blocks_to_store.push(block);
                next_level.push(cid);
            }

            debug!(
                depth,
                nodes = next_level.len(),
                is_leaf_level,
                "Built tree level"
            );

            current_level = next_level;
            is_leaf_level = false;
        }

        let structure = ManifestStructure::Tree {
            depth,
            fanout: self.config.fanout as u16,
            chunk_count: chunk_count as u64,
        };

        info!(
            depth,
            root_children = current_level.len(),
            intermediate_nodes = blocks_to_store.len(),
            "Tree structure built"
        );

        Ok((current_level, structure, blocks_to_store))
    }
}

// ============================================================================
// TreeReader
// ============================================================================

/// Reader for traversing tree structure to get all chunk CIDs.
pub struct TreeReader<'a> {
    block_store: &'a BlockStore,
}

impl<'a> TreeReader<'a> {
    /// Create a new tree reader.
    pub fn new(block_store: &'a BlockStore) -> Self {
        Self { block_store }
    }

    /// Get all chunk CIDs from a manifest, traversing tree if needed.
    pub fn get_all_chunks(
        &self,
        manifest: &DocumentManifest,
    ) -> Result<Vec<ContentId>, BlockStoreError> {
        match &manifest.structure {
            ManifestStructure::Flat => Ok(manifest.children.clone()),
            ManifestStructure::Tree { chunk_count, .. } => {
                debug!(chunk_count, "Traversing tree structure");

                let mut chunks = Vec::with_capacity(*chunk_count as usize);
                self.collect_chunks(&manifest.children, &mut chunks)?;

                if chunks.len() != *chunk_count as usize {
                    warn!(
                        expected = chunk_count,
                        actual = chunks.len(),
                        "Chunk count mismatch after tree traversal"
                    );
                }

                Ok(chunks)
            }
        }
    }

    /// Recursively collect chunks from tree nodes.
    fn collect_chunks(
        &self,
        node_cids: &[ContentId],
        chunks: &mut Vec<ContentId>,
    ) -> Result<(), BlockStoreError> {
        for cid in node_cids {
            let block = self
                .block_store
                .get(cid)?
                .ok_or_else(|| BlockStoreError::NotFound(cid.clone()))?;

            let node = TreeNode::from_dag_cbor(block.data())?;

            if node.is_leaf_level {
                chunks.extend(node.children.iter().cloned());
            } else {
                self.collect_chunks(&node.children, chunks)?;
            }
        }

        Ok(())
    }

    /// Create an iterator over chunks (lazy traversal).
    pub fn iter_chunks(
        &'a self,
        manifest: &'a DocumentManifest,
    ) -> Box<dyn Iterator<Item = Result<ContentId, BlockStoreError>> + 'a> {
        match &manifest.structure {
            ManifestStructure::Flat => Box::new(manifest.children.iter().cloned().map(Ok)),
            ManifestStructure::Tree { .. } => Box::new(TreeChunkIterator::new(self, manifest)),
        }
    }
}

// ============================================================================
// TreeChunkIterator
// ============================================================================

/// Lazy iterator over chunks in a tree-structured manifest.
pub struct TreeChunkIterator<'a> {
    reader: &'a TreeReader<'a>,
    /// Stack of (node_cids, current_index)
    stack: Vec<(Vec<ContentId>, usize)>,
    /// Current leaf node's chunks being yielded
    current_chunks: Vec<ContentId>,
    current_chunk_index: usize,
}

impl<'a> TreeChunkIterator<'a> {
    fn new(reader: &'a TreeReader<'a>, manifest: &'a DocumentManifest) -> Self {
        Self {
            reader,
            stack: vec![(manifest.children.clone(), 0)],
            current_chunks: Vec::new(),
            current_chunk_index: 0,
        }
    }
}

impl<'a> Iterator for TreeChunkIterator<'a> {
    type Item = Result<ContentId, BlockStoreError>;

    fn next(&mut self) -> Option<Self::Item> {
        // First, yield from current chunks buffer
        if self.current_chunk_index < self.current_chunks.len() {
            let chunk = self.current_chunks[self.current_chunk_index].clone();
            self.current_chunk_index += 1;
            return Some(Ok(chunk));
        }

        // Need to load more nodes
        while let Some((node_cids, index)) = self.stack.last_mut() {
            if *index >= node_cids.len() {
                self.stack.pop();
                continue;
            }

            let cid = node_cids[*index].clone();
            *index += 1;

            // Fetch and parse node
            let block = match self.reader.block_store.get(&cid) {
                Ok(Some(b)) => b,
                Ok(None) => return Some(Err(BlockStoreError::NotFound(cid))),
                Err(e) => return Some(Err(e)),
            };

            let node = match TreeNode::from_dag_cbor(block.data()) {
                Ok(n) => n,
                Err(e) => return Some(Err(e)),
            };

            if node.is_leaf_level {
                self.current_chunks = node.children;
                self.current_chunk_index = 0;

                if !self.current_chunks.is_empty() {
                    let chunk = self.current_chunks[0].clone();
                    self.current_chunk_index = 1;
                    return Some(Ok(chunk));
                }
            } else {
                self.stack.push((node.children, 0));
            }
        }

        None
    }
}

// ============================================================================
// DocumentManifestBuilder
// ============================================================================

/// Builder for constructing DocumentManifest instances.
///
/// Provides a fluent API for setting fields. Use either:
/// - `build_flat()` for small files (no BlockStore needed)
/// - `build_with_store()` for automatic tree structure on large files
///
/// # Example
///
/// ```ignore
/// // Small file
/// let manifest = DocumentManifest::builder()
///     .publisher_did("did:key:z6Mk...")
///     .name("small.txt")
///     .chunks(chunk_cids)
///     .total_size(1024)
///     .build_flat()?;
///
/// // Large file with automatic tree
/// let manifest = DocumentManifest::builder()
///     .publisher_did("did:key:z6Mk...")
///     .chunks(many_chunks)
///     .total_size(10_000_000_000)
///     .build_with_store(&block_store)?;
/// ```
#[derive(Debug, Default)]
pub struct DocumentManifestBuilder {
    name: Option<String>,
    content_type: Option<String>,
    total_size: Option<u64>,
    chunks: Vec<ContentId>,
    chunking_algorithm: ChunkingAlgorithm,
    publisher_did: Option<String>,
    created_at: Option<u64>,
    metadata: HashMap<String, String>,
    tree_config: Option<TreeConfig>,
}

impl DocumentManifestBuilder {
    /// Create a new builder with default values.
    pub fn new() -> Self {
        Self::default()
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

    /// Set the total size in bytes.
    pub fn total_size(mut self, size: u64) -> Self {
        self.total_size = Some(size);
        self
    }

    /// Set the chunk CIDs.
    pub fn chunks(mut self, chunks: Vec<ContentId>) -> Self {
        self.chunks = chunks;
        self
    }

    /// Add a single chunk CID.
    pub fn add_chunk(mut self, chunk: ContentId) -> Self {
        self.chunks.push(chunk);
        self
    }

    /// Set the chunking algorithm.
    pub fn chunking_algorithm(mut self, algorithm: ChunkingAlgorithm) -> Self {
        self.chunking_algorithm = algorithm;
        self
    }

    /// Set the publisher DID.
    pub fn publisher_did(mut self, did: impl Into<String>) -> Self {
        self.publisher_did = Some(did.into());
        self
    }

    /// Set the creation timestamp (Unix epoch seconds).
    pub fn created_at(mut self, timestamp: u64) -> Self {
        self.created_at = Some(timestamp);
        self
    }

    /// Add a metadata key-value pair.
    pub fn metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Add multiple metadata entries.
    pub fn metadata_entries(mut self, entries: HashMap<String, String>) -> Self {
        self.metadata.extend(entries);
        self
    }

    /// Set custom tree configuration.
    pub fn tree_config(mut self, config: TreeConfig) -> Self {
        self.tree_config = Some(config);
        self
    }

    /// Get the current timestamp.
    fn current_timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }

    /// Build a flat manifest (chunks stored directly, no tree).
    ///
    /// Use this for small files or when you don't have a BlockStore.
    /// Logs a warning if chunk count exceeds the flat threshold.
    ///
    /// # Errors
    ///
    /// Returns an error if `publisher_did` or `total_size` is missing.
    pub fn build_flat(self) -> Result<DocumentManifest, BlockStoreError> {
        let publisher_did = self
            .publisher_did
            .ok_or_else(|| BlockStoreError::Config("publisher_did is required".into()))?;

        let total_size = self
            .total_size
            .ok_or_else(|| BlockStoreError::Config("total_size is required".into()))?;

        if self.chunks.len() > FLAT_CHUNK_THRESHOLD {
            warn!(
                chunks = self.chunks.len(),
                threshold = FLAT_CHUNK_THRESHOLD,
                "Building flat manifest for large file; consider using build_with_store()"
            );
        }

        let created_at = self.created_at.unwrap_or_else(Self::current_timestamp);

        debug!(
            publisher = %publisher_did,
            name = ?self.name,
            chunks = self.chunks.len(),
            total_size,
            "Building flat document manifest"
        );

        Ok(DocumentManifest {
            version: MANIFEST_VERSION,
            name: self.name,
            content_type: self.content_type,
            total_size,
            structure: ManifestStructure::Flat,
            children: self.chunks,
            chunking_algorithm: self.chunking_algorithm,
            publisher_did,
            created_at,
            metadata: self.metadata,
        })
    }

    /// Build manifest with automatic tree structure for large files.
    ///
    /// If chunk count exceeds the threshold, builds a Merkle tree and
    /// stores intermediate nodes in the BlockStore.
    ///
    /// # Errors
    ///
    /// Returns an error if `publisher_did` or `total_size` is missing,
    /// or if storing intermediate blocks fails.
    pub fn build_with_store(
        self,
        block_store: &BlockStore,
    ) -> Result<DocumentManifest, BlockStoreError> {
        let publisher_did = self
            .publisher_did
            .ok_or_else(|| BlockStoreError::Config("publisher_did is required".into()))?;

        let total_size = self
            .total_size
            .ok_or_else(|| BlockStoreError::Config("total_size is required".into()))?;

        let tree_config = self.tree_config.unwrap_or_default();
        let tree_builder = TreeBuilder::with_config(tree_config);

        let (children, structure, intermediate_blocks) = tree_builder.build(self.chunks)?;

        // Store intermediate tree nodes
        for block in intermediate_blocks {
            block_store.put(&block)?;
        }

        let created_at = self.created_at.unwrap_or_else(Self::current_timestamp);

        info!(
            publisher = %publisher_did,
            name = ?self.name,
            structure = ?structure,
            children = children.len(),
            total_size,
            "Built document manifest"
        );

        Ok(DocumentManifest {
            version: MANIFEST_VERSION,
            name: self.name,
            content_type: self.content_type,
            total_size,
            structure,
            children,
            chunking_algorithm: self.chunking_algorithm,
            publisher_did,
            created_at,
            metadata: self.metadata,
        })
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
const TEST_DID: &str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::storage::content::BlockStoreConfig;
    use tempfile::TempDir;

    fn sample_chunks() -> Vec<ContentId> {
        vec![
            ContentId::from_bytes(b"chunk1"),
            ContentId::from_bytes(b"chunk2"),
            ContentId::from_bytes(b"chunk3"),
        ]
    }

    fn generate_chunks(count: usize) -> Vec<ContentId> {
        (0..count)
            .map(|i| ContentId::from_bytes(format!("chunk_{:06}", i).as_bytes()))
            .collect()
    }

    fn create_test_store() -> (BlockStore, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = BlockStoreConfig {
            db_path: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        let store = BlockStore::new(config).unwrap();
        (store, temp_dir)
    }

    // ========================================
    // Flat Manifest Tests
    // ========================================

    #[test]
    fn test_manifest_builder_flat_minimal() {
        let chunks = sample_chunks();
        let manifest = DocumentManifest::builder()
            .publisher_did(TEST_DID)
            .chunks(chunks.clone())
            .total_size(2048)
            .build_flat()
            .expect("should build");

        assert_eq!(manifest.publisher_did, TEST_DID);
        assert_eq!(manifest.children, chunks);
        assert_eq!(manifest.total_size, 2048);
        assert!(manifest.is_flat());
        assert_eq!(manifest.chunk_count(), 3);
    }

    #[test]
    fn test_manifest_builder_flat_full() {
        let chunks = sample_chunks();
        let manifest = DocumentManifest::builder()
            .publisher_did(TEST_DID)
            .name("report.pdf")
            .content_type("application/pdf")
            .chunks(chunks.clone())
            .total_size(4096)
            .chunking_algorithm(ChunkingAlgorithm::Rabin)
            .created_at(1702156800)
            .metadata("author", "Alice")
            .build_flat()
            .expect("should build");

        assert_eq!(manifest.name, Some("report.pdf".into()));
        assert_eq!(manifest.content_type, Some("application/pdf".into()));
        assert_eq!(manifest.chunking_algorithm, ChunkingAlgorithm::Rabin);
        assert_eq!(manifest.created_at, 1702156800);
        assert_eq!(manifest.get_metadata("author"), Some("Alice"));
    }

    #[test]
    fn test_manifest_builder_missing_publisher() {
        let result = DocumentManifest::builder()
            .chunks(sample_chunks())
            .total_size(1024)
            .build_flat();

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("publisher_did"));
    }

    #[test]
    fn test_manifest_builder_missing_size() {
        let result = DocumentManifest::builder()
            .publisher_did(TEST_DID)
            .chunks(sample_chunks())
            .build_flat();

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("total_size"));
    }

    // ========================================
    // Serialization Tests
    // ========================================

    #[test]
    fn test_dag_cbor_roundtrip_flat() {
        let manifest = DocumentManifest::builder()
            .publisher_did(TEST_DID)
            .name("test.txt")
            .content_type("text/plain")
            .chunks(sample_chunks())
            .total_size(1024)
            .created_at(1702156800)
            .metadata("key", "value")
            .build_flat()
            .expect("should build");

        let bytes = manifest.to_dag_cbor().expect("should serialize");
        let restored = DocumentManifest::from_dag_cbor(&bytes).expect("should deserialize");

        assert_eq!(manifest, restored);
    }

    #[test]
    fn test_root_cid_deterministic() {
        let manifest = DocumentManifest::builder()
            .publisher_did(TEST_DID)
            .name("test.pdf")
            .chunks(sample_chunks())
            .total_size(1024)
            .created_at(1702156800)
            .build_flat()
            .expect("should build");

        let cid1 = manifest.root_cid().expect("should compute CID");
        let cid2 = manifest.root_cid().expect("should compute CID");

        assert_eq!(cid1, cid2);
        assert!(cid1.is_dag_cbor());
    }

    #[test]
    fn test_to_block() {
        let chunks = sample_chunks();
        let manifest = DocumentManifest::builder()
            .publisher_did(TEST_DID)
            .chunks(chunks.clone())
            .total_size(1024)
            .created_at(1702156800)
            .build_flat()
            .expect("should build");

        let block = manifest.to_block().expect("should create block");
        let root_cid = manifest.root_cid().expect("should compute CID");

        assert_eq!(block.cid(), &root_cid);
        assert_eq!(block.links().len(), chunks.len());
    }

    // ========================================
    // Tree Structure Tests
    // ========================================

    #[test]
    fn test_tree_node_roundtrip() {
        let children = generate_chunks(10);
        let node = TreeNode::new(children.clone(), true);

        let bytes = node.to_dag_cbor().unwrap();
        let restored = TreeNode::from_dag_cbor(&bytes).unwrap();

        assert_eq!(node, restored);
        assert!(restored.is_leaf_level);
    }

    #[test]
    fn test_tree_builder_small_stays_flat() {
        let chunks = generate_chunks(100);
        let builder = TreeBuilder::new();

        let (children, structure, blocks) = builder.build(chunks.clone()).unwrap();

        assert!(matches!(structure, ManifestStructure::Flat));
        assert_eq!(children, chunks);
        assert!(blocks.is_empty());
    }

    #[test]
    fn test_tree_builder_large_creates_tree() {
        let config = TreeConfig {
            fanout: 10,
            flat_threshold: 50,
        };
        let chunks = generate_chunks(100);
        let builder = TreeBuilder::with_config(config);

        let (children, structure, blocks) = builder.build(chunks).unwrap();

        match structure {
            ManifestStructure::Tree {
                depth,
                fanout,
                chunk_count,
            } => {
                assert!(depth >= 1);
                assert_eq!(fanout, 10);
                assert_eq!(chunk_count, 100);
            }
            _ => panic!("Expected tree structure"),
        }

        assert!(!blocks.is_empty());
        assert!(children.len() <= 10);
    }

    #[test]
    fn test_manifest_with_tree_structure() {
        let (store, _temp) = create_test_store();
        let chunks = generate_chunks(100);

        let manifest = DocumentManifest::builder()
            .publisher_did(TEST_DID)
            .name("large.bin")
            .chunks(chunks.clone())
            .total_size(25600)
            .tree_config(TreeConfig {
                fanout: 10,
                flat_threshold: 50,
            })
            .build_with_store(&store)
            .expect("should build");

        assert!(manifest.is_tree());
        assert_eq!(manifest.chunk_count(), 100);
        assert!(manifest.tree_depth() >= 1);
    }

    #[test]
    fn test_tree_reader_get_all_chunks() {
        let (store, _temp) = create_test_store();
        let chunks = generate_chunks(100);

        let manifest = DocumentManifest::builder()
            .publisher_did(TEST_DID)
            .chunks(chunks.clone())
            .total_size(25600)
            .tree_config(TreeConfig {
                fanout: 10,
                flat_threshold: 50,
            })
            .build_with_store(&store)
            .expect("should build");

        let reader = TreeReader::new(&store);
        let result = reader.get_all_chunks(&manifest).unwrap();

        assert_eq!(result.len(), chunks.len());
        assert_eq!(result, chunks);
    }

    #[test]
    fn test_tree_chunk_iterator() {
        let (store, _temp) = create_test_store();
        let chunks = generate_chunks(50);

        let manifest = DocumentManifest::builder()
            .publisher_did(TEST_DID)
            .chunks(chunks.clone())
            .total_size(12800)
            .tree_config(TreeConfig {
                fanout: 5,
                flat_threshold: 10,
            })
            .build_with_store(&store)
            .expect("should build");

        let reader = TreeReader::new(&store);
        let iterated: Vec<ContentId> = reader.iter_chunks(&manifest).map(|r| r.unwrap()).collect();

        assert_eq!(iterated, chunks);
    }

    // ========================================
    // Edge Cases
    // ========================================

    #[test]
    fn test_empty_chunks() {
        let manifest = DocumentManifest::builder()
            .publisher_did(TEST_DID)
            .chunks(vec![])
            .total_size(0)
            .build_flat()
            .expect("should build");

        assert_eq!(manifest.chunk_count(), 0);
        assert!(manifest.is_flat());
    }

    #[test]
    fn test_unicode_name() {
        let manifest = DocumentManifest::builder()
            .publisher_did(TEST_DID)
            .name("文档.pdf")
            .chunks(sample_chunks())
            .total_size(1024)
            .build_flat()
            .expect("should build");

        let bytes = manifest.to_dag_cbor().expect("should serialize");
        let restored = DocumentManifest::from_dag_cbor(&bytes).expect("should deserialize");

        assert_eq!(restored.name, Some("文档.pdf".into()));
    }

    #[test]
    fn test_full_roundtrip_with_tree() {
        let (store, _temp) = create_test_store();
        let original_chunks = generate_chunks(200);

        // Build and store manifest
        let manifest = DocumentManifest::builder()
            .publisher_did(TEST_DID)
            .name("big_file.bin")
            .chunks(original_chunks.clone())
            .total_size(51200)
            .tree_config(TreeConfig {
                fanout: 20,
                flat_threshold: 50,
            })
            .build_with_store(&store)
            .expect("should build");

        // Store manifest block
        let manifest_block = manifest.to_block().unwrap();
        store.put(&manifest_block).unwrap();

        // Retrieve and verify
        let retrieved = store.get(manifest_block.cid()).unwrap().unwrap();
        let restored = DocumentManifest::from_dag_cbor(retrieved.data()).unwrap();

        assert_eq!(restored.name, Some("big_file.bin".into()));
        assert!(restored.is_tree());
        assert_eq!(restored.chunk_count(), 200);

        // Traverse tree to get all chunks
        let reader = TreeReader::new(&store);
        let read_chunks = reader.get_all_chunks(&restored).unwrap();

        assert_eq!(read_chunks, original_chunks);
    }
}

#[cfg(test)]
mod tree_tests {
    use crate::modules::storage::content::BlockStoreConfig;

    use super::*;
    use tempfile::TempDir;

    fn create_test_store() -> (BlockStore, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = BlockStoreConfig {
            db_path: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        let store = BlockStore::new(config).unwrap();
        (store, temp_dir)
    }

    fn generate_chunks(count: usize) -> Vec<ContentId> {
        (0..count)
            .map(|i| ContentId::from_bytes(format!("chunk_{:06}", i).as_bytes()))
            .collect()
    }

    // ========================================
    // TreeNode Tests
    // ========================================

    #[test]
    fn test_tree_node_roundtrip() {
        let children = generate_chunks(10);
        let node = TreeNode::new(children.clone(), true);

        let bytes = node.to_dag_cbor().unwrap();
        let restored = TreeNode::from_dag_cbor(&bytes).unwrap();

        assert_eq!(node, restored);
        assert!(restored.is_leaf_level);
    }

    #[test]
    fn test_tree_node_to_block() {
        let children = generate_chunks(5);
        let node = TreeNode::new(children.clone(), true);

        let block = node.to_block().unwrap();

        assert_eq!(block.links().len(), 5);
        assert!(block.cid().is_dag_cbor());
    }

    // ========================================
    // TreeBuilder Tests
    // ========================================

    #[test]
    fn test_tree_builder_small_file_stays_flat() {
        let chunks = generate_chunks(100);
        let builder = TreeBuilder::new();

        let (children, structure, blocks) = builder.build(chunks.clone()).unwrap();

        assert!(matches!(structure, ManifestStructure::Flat));
        assert_eq!(children, chunks);
        assert!(blocks.is_empty());
    }

    #[test]
    fn test_tree_builder_large_file_creates_tree() {
        // Create more chunks than threshold
        let chunks = generate_chunks(50_000);
        let builder = TreeBuilder::new();

        let (children, structure, blocks) = builder.build(chunks.clone()).unwrap();

        match structure {
            ManifestStructure::Tree {
                depth,
                fanout,
                chunk_count,
            } => {
                assert!(depth >= 1);
                assert_eq!(fanout, DEFAULT_TREE_FANOUT as u16);
                assert_eq!(chunk_count, 50_000);
            }
            _ => panic!("Expected tree structure"),
        }

        // Should have created intermediate nodes
        assert!(!blocks.is_empty());
        // Root children should be fewer than original chunks
        assert!(children.len() < chunks.len());
    }

    #[test]
    fn test_tree_builder_custom_fanout() {
        let config = TreeConfig {
            fanout: 10,
            flat_threshold: 5,
        };
        let builder = TreeBuilder::with_config(config);
        let chunks = generate_chunks(100);

        let (children, structure, blocks) = builder.build(chunks).unwrap();

        match structure {
            ManifestStructure::Tree { fanout, .. } => {
                assert_eq!(fanout, 10);
            }
            _ => panic!("Expected tree structure"),
        }

        assert!(!blocks.is_empty());
        assert!(children.len() <= 10);
    }

    // ========================================
    // TreeReader Tests
    // ========================================

    #[test]
    fn test_tree_reader_flat_manifest() {
        let (store, _temp) = create_test_store();
        let chunks = generate_chunks(10);

        let manifest = DocumentManifest::builder()
            .publisher_did(TEST_DID)
            .chunks(chunks.clone())
            .total_size(1024)
            .build_flat()
            .unwrap();

        let reader = TreeReader::new(&store);
        let result = reader.get_all_chunks(&manifest).unwrap();

        assert_eq!(result, chunks);
    }

    #[test]
    fn test_tree_reader_tree_manifest() {
        let (store, _temp) = create_test_store();

        // Use low threshold to force tree structure
        let chunks = generate_chunks(100);

        let manifest = DocumentManifest::builder()
            .publisher_did(TEST_DID)
            .chunks(chunks.clone())
            .total_size(25600)
            .tree_config(TreeConfig {
                fanout: 10,
                flat_threshold: 5,
            })
            .build_with_store(&store)
            .unwrap();

        assert!(manifest.is_tree());

        let reader = TreeReader::new(&store);
        let result = reader.get_all_chunks(&manifest).unwrap();

        assert_eq!(result.len(), chunks.len());
        assert_eq!(result, chunks);
    }

    #[test]
    fn test_tree_chunk_iterator() {
        let (store, _temp) = create_test_store();
        let chunks = generate_chunks(50);

        let manifest = DocumentManifest::builder()
            .publisher_did(TEST_DID)
            .chunks(chunks.clone())
            .total_size(12800)
            .tree_config(TreeConfig {
                fanout: 5,
                flat_threshold: 3,
            })
            .build_with_store(&store)
            .unwrap();

        let reader = TreeReader::new(&store);
        let iterated: Vec<ContentId> = reader.iter_chunks(&manifest).map(|r| r.unwrap()).collect();

        assert_eq!(iterated, chunks);
    }

    // ========================================
    // Integration Tests
    // ========================================

    #[test]
    fn test_manifest_tree_full_roundtrip() {
        let (store, _temp) = create_test_store();
        let original_chunks = generate_chunks(1000);

        // Build manifest with tree
        let manifest = DocumentManifest::builder()
            .publisher_did(TEST_DID)
            .name("large_file.bin")
            .chunks(original_chunks.clone())
            .total_size(256_000_000)
            .tree_config(TreeConfig {
                fanout: 50,
                flat_threshold: 100,
            })
            .build_with_store(&store)
            .unwrap();

        assert!(manifest.is_tree());
        assert_eq!(manifest.chunk_count(), 1000);

        // Store manifest
        let manifest_block = manifest.to_block().unwrap();
        store.put(&manifest_block).unwrap();

        // Retrieve and verify
        let retrieved = store.get(manifest_block.cid()).unwrap().unwrap();
        let restored = DocumentManifest::from_dag_cbor(retrieved.data()).unwrap();

        assert_eq!(restored.name, Some("large_file.bin".into()));
        assert!(restored.is_tree());

        // Read all chunks through tree
        let reader = TreeReader::new(&store);
        let read_chunks = reader.get_all_chunks(&restored).unwrap();

        assert_eq!(read_chunks, original_chunks);
    }

    #[test]
    fn test_tree_depth_calculation() {
        let config = TreeConfig {
            fanout: 10,
            flat_threshold: 5,
        };
        let builder = TreeBuilder::with_config(config);

        // 100 chunks with fanout 10:
        // Level 0: 100 chunks
        // Level 1: 10 nodes (each holding 10 chunks)
        // Root: 10 children
        // Depth = 1
        let chunks = generate_chunks(100);
        let (children, structure, _) = builder.build(chunks).unwrap();

        match structure {
            ManifestStructure::Tree { depth, .. } => {
                assert_eq!(depth, 1);
                assert_eq!(children.len(), 10);
            }
            _ => panic!("Expected tree"),
        }
    }
}
