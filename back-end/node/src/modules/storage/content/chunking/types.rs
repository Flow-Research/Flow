use serde::{Deserialize, Serialize};

use crate::modules::storage::content::ContentId;

/// Reference to a chunk within a larger document.
///
/// Contains the CID of the chunk block plus byte-range information
/// for efficient random access and reconstruction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkRef {
    /// Content identifier of this chunk's block.
    pub cid: ContentId,
    /// Byte offset in the original file where this chunk starts.
    pub offset: u64,
    /// Size of this chunk in bytes.
    pub size: u32,
}

impl ChunkRef {
    /// Create a new chunk reference.
    pub fn new(cid: ContentId, offset: u64, size: u32) -> Self {
        Self { cid, offset, size }
    }

    /// Get the byte range this chunk covers (start..end).
    pub fn byte_range(&self) -> std::ops::Range<u64> {
        self.offset..self.offset + self.size as u64
    }

    /// Check if this chunk contains the given byte offset.
    pub fn contains_offset(&self, offset: u64) -> bool {
        offset >= self.offset && offset < self.offset + self.size as u64
    }
}

/// Output of the chunking process: raw bytes with offset metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkData {
    /// The raw chunk bytes.
    pub data: Vec<u8>,
    /// Byte offset in the original input where this chunk starts.
    pub offset: u64,
}

impl ChunkData {
    /// Create new chunk data.
    pub fn new(data: Vec<u8>, offset: u64) -> Self {
        Self { data, offset }
    }

    /// Get the size of this chunk.
    pub fn size(&self) -> usize {
        self.data.len()
    }

    /// Convert to a ChunkRef by computing the CID.
    pub fn to_chunk_ref(&self) -> ChunkRef {
        let cid = ContentId::from_bytes(&self.data);
        ChunkRef::new(cid, self.offset, self.data.len() as u32)
    }
}

/// Trait for content chunking algorithms.
///
/// All chunkers implement this trait for unified usage.
/// Implementations must be thread-safe (Send + Sync).
pub trait Chunker: Send + Sync {
    /// Chunk the entire input data and return all chunks.
    ///
    /// This is the simplest API, loading all chunks into memory.
    /// For large files, consider using `chunk_iter` instead.
    fn chunk(&self, data: &[u8]) -> Vec<ChunkData>;

    /// Create an iterator over chunks.
    ///
    /// This is more memory-efficient for large inputs as it
    /// yields chunks one at a time.
    fn chunk_iter<'a>(&self, data: &'a [u8]) -> Box<dyn Iterator<Item = ChunkData> + 'a>;

    /// Get the algorithm name.
    fn name(&self) -> &'static str;

    /// Get the configuration.
    fn config(&self) -> &super::ChunkingConfig;

    /// Get the target chunk size.
    fn target_size(&self) -> usize {
        self.config().target_size
    }
}
