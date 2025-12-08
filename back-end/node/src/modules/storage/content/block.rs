use tracing::debug;

use super::cid::ContentId;
use super::error::BlockStoreError;

/// A content-addressed block with optional DAG links.
///
/// Blocks are immutable - once created, their content and CID cannot change.
/// The CID is computed from the data on construction.
#[derive(Clone, Debug)]
pub struct Block {
    /// Content identifier (hash of the data)
    cid: ContentId,
    /// Raw block data
    data: Vec<u8>,
    /// Links to other blocks (forms DAG structure)
    links: Vec<ContentId>,
}

impl Block {
    /// Create a new block from raw bytes.
    ///
    /// The CID is automatically computed using SHA2-256 with RAW codec.
    pub fn new(data: impl Into<Vec<u8>>) -> Self {
        let data = data.into();
        let cid = ContentId::from_bytes(&data);

        debug!(
            cid = %cid,
            size = data.len(),
            "Created new block"
        );

        Self {
            cid,
            data,
            links: Vec::new(),
        }
    }

    /// Create a new block with links to other blocks.
    ///
    /// The CID is computed from the data, not including the links.
    /// Links create a DAG structure for organizing related blocks.
    pub fn with_links(data: impl Into<Vec<u8>>, links: Vec<ContentId>) -> Self {
        let data = data.into();
        let cid = ContentId::from_bytes(&data);

        debug!(
            cid = %cid,
            size = data.len(),
            link_count = links.len(),
            "Created new block with links"
        );

        Self { cid, data, links }
    }

    /// Reconstruct a block from stored components.
    ///
    /// This verifies that the data matches the expected CID.
    pub fn from_parts(
        expected_cid: ContentId,
        data: Vec<u8>,
        links: Vec<ContentId>,
    ) -> Result<Self, BlockStoreError> {
        let computed_cid = ContentId::from_bytes(&data);

        if computed_cid != expected_cid {
            return Err(BlockStoreError::IntegrityError {
                expected: expected_cid,
                actual: computed_cid,
            });
        }

        Ok(Self {
            cid: expected_cid,
            data,
            links,
        })
    }

    /// Get the content identifier.
    pub fn cid(&self) -> &ContentId {
        &self.cid
    }

    /// Get the raw data.
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Get the size of the data in bytes.
    pub fn size(&self) -> usize {
        self.data.len()
    }

    /// Get links to other blocks.
    pub fn links(&self) -> &[ContentId] {
        &self.links
    }

    /// Check if this block has any links.
    pub fn has_links(&self) -> bool {
        !self.links.is_empty()
    }

    /// Verify the block's integrity by recomputing the CID.
    pub fn verify(&self) -> bool {
        self.cid.verify(&self.data)
    }

    /// Serialize for storage (just data + links)
    pub fn to_stored_bytes(&self) -> Result<Vec<u8>, BlockStoreError> {
        // Simple: serialize tuple of (data, link_bytes)
        let link_bytes: Vec<Vec<u8>> = self.links.iter().map(|l| l.to_bytes()).collect();
        serde_ipld_dagcbor::to_vec(&(&self.data, &link_bytes))
            .map_err(|e| BlockStoreError::Serialization(e.to_string()))
    }

    /// Deserialize from stored bytes
    pub fn from_stored_bytes(cid: ContentId, bytes: &[u8]) -> Result<Self, BlockStoreError> {
        let (data, link_bytes): (Vec<u8>, Vec<Vec<u8>>) = serde_ipld_dagcbor::from_slice(bytes)
            .map_err(|e| BlockStoreError::Serialization(e.to_string()))?;

        let links: Result<Vec<ContentId>, _> = link_bytes
            .iter()
            .map(|b| ContentId::from_raw_bytes(b))
            .collect();

        Self::from_parts(cid, data, links?)
    }
}

impl PartialEq for Block {
    fn eq(&self, other: &Self) -> bool {
        self.cid == other.cid
    }
}

impl Eq for Block {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_new() {
        let block = Block::new(b"test data".to_vec());

        assert_eq!(block.data(), b"test data");
        assert_eq!(block.size(), 9);
        assert!(!block.has_links());
    }

    #[test]
    fn test_block_with_links() {
        let link1 = ContentId::from_bytes(b"linked block 1");
        let link2 = ContentId::from_bytes(b"linked block 2");

        let block = Block::with_links(b"parent block".to_vec(), vec![link1.clone(), link2.clone()]);

        assert!(block.has_links());
        assert_eq!(block.links().len(), 2);
        assert!(block.links().contains(&link1));
        assert!(block.links().contains(&link2));
    }

    #[test]
    fn test_block_verify() {
        let block = Block::new(b"verify me".to_vec());
        assert!(block.verify());
    }

    #[test]
    fn test_block_equality() {
        let block1 = Block::new(b"same data".to_vec());
        let block2 = Block::new(b"same data".to_vec());
        let block3 = Block::new(b"different data".to_vec());

        assert_eq!(block1, block2);
        assert_ne!(block1, block3);
    }

    #[test]
    fn test_block_from_parts_valid() {
        let data = b"original data".to_vec();
        let cid = ContentId::from_bytes(&data);

        let block = Block::from_parts(cid.clone(), data, vec![]).unwrap();
        assert_eq!(block.cid(), &cid);
    }

    #[test]
    fn test_block_from_parts_invalid() {
        let data = b"original data".to_vec();
        let wrong_cid = ContentId::from_bytes(b"different data");

        let result = Block::from_parts(wrong_cid, data, vec![]);
        assert!(matches!(
            result,
            Err(BlockStoreError::IntegrityError { .. })
        ));
    }

    #[test]
    fn test_block_stored_bytes_roundtrip() {
        let link = ContentId::from_bytes(b"linked");
        let original = Block::with_links(b"parent data".to_vec(), vec![link]);

        let bytes = original.to_stored_bytes().unwrap();
        let restored = Block::from_stored_bytes(original.cid().clone(), &bytes).unwrap();

        assert_eq!(original, restored);
        assert_eq!(original.links(), restored.links());
    }

    #[test]
    fn test_block_stored_bytes_roundtrip_no_links() {
        let original = Block::new(b"no links".to_vec());

        let bytes = original.to_stored_bytes().unwrap();
        let restored = Block::from_stored_bytes(original.cid().clone(), &bytes).unwrap();

        assert_eq!(original, restored);
        assert!(restored.links().is_empty());
    }

    #[test]
    fn test_block_empty_data() {
        let block = Block::new(vec![]);
        assert_eq!(block.size(), 0);
        assert!(block.verify());
    }

    #[test]
    fn test_block_large_data() {
        let data = vec![42u8; 1_000_000]; // 1MB
        let block = Block::new(data);
        assert_eq!(block.size(), 1_000_000);
        assert!(block.verify());
    }

    #[test]
    fn test_block_from_stored_bytes_corrupted() {
        let cid = ContentId::from_bytes(b"test");
        let garbage = b"not valid cbor";

        let result = Block::from_stored_bytes(cid, garbage);
        assert!(matches!(result, Err(BlockStoreError::Serialization(_))));
    }

    #[test]
    fn test_block_from_stored_bytes_cid_mismatch() {
        let original = Block::new(b"original data".to_vec());
        let bytes = original.to_stored_bytes().unwrap();

        // Try to restore with wrong CID
        let wrong_cid = ContentId::from_bytes(b"different");
        let result = Block::from_stored_bytes(wrong_cid, &bytes);

        assert!(matches!(
            result,
            Err(BlockStoreError::IntegrityError { .. })
        ));
    }

    #[test]
    fn test_block_from_parts_with_links() {
        let data = b"parent".to_vec();
        let cid = ContentId::from_bytes(&data);
        let link = ContentId::from_bytes(b"child");

        let block = Block::from_parts(cid.clone(), data, vec![link.clone()]).unwrap();

        assert_eq!(block.cid(), &cid);
        assert_eq!(block.links(), &[link]);
    }

    #[test]
    fn test_block_deterministic() {
        let block1 = Block::new(b"deterministic".to_vec());
        let block2 = Block::new(b"deterministic".to_vec());

        assert_eq!(block1.cid(), block2.cid());
        assert_eq!(block1.data(), block2.data());
    }

    #[test]
    fn test_block_many_links() {
        let links: Vec<ContentId> = (0..100)
            .map(|i| ContentId::from_bytes(&[i as u8]))
            .collect();

        let block = Block::with_links(b"parent".to_vec(), links.clone());

        assert_eq!(block.links().len(), 100);

        // Roundtrip should preserve all links
        let bytes = block.to_stored_bytes().unwrap();
        let restored = Block::from_stored_bytes(block.cid().clone(), &bytes).unwrap();
        assert_eq!(restored.links(), links.as_slice());
    }

    #[test]
    fn test_block_cid_accessor() {
        let data = b"test";
        let expected_cid = ContentId::from_bytes(data);
        let block = Block::new(data.to_vec());

        assert_eq!(block.cid(), &expected_cid);
    }
}
