//! Content provider abstraction for block retrieval.
//!
//! This module defines the `ContentProvider` trait which abstracts over
//! different sources of content-addressed blocks (local storage, network, etc.).
//!
//! # Example
//!
//! ```ignore
//! use std::sync::Arc;
//!
//! // Create a local provider
//! let provider = LocalProvider::new(Arc::new(block_store));
//!
//! // Fetch a block
//! if let Some(block) = provider.get_block(&cid).await? {
//!     println!("Got block: {} bytes", block.size());
//! }
//! ```

use async_trait::async_trait;
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, instrument, warn};

use super::block::Block;
use super::block_store::BlockStore;
use super::cid::ContentId;
use super::error::BlockStoreError;

// ============================================================================
// Error Type
// ============================================================================

/// Errors that can occur during content provider operations.
#[derive(Debug, Error)]
pub enum ContentProviderError {
    /// Block not found in any provider
    #[error("Block not found: {0}")]
    NotFound(ContentId),

    /// Local storage error
    #[error("Storage error: {0}")]
    Storage(String),

    /// Network error (for remote providers)
    #[error("Network error: {0}")]
    Network(String),

    /// Provider is unavailable or not started
    #[error("Provider unavailable: {0}")]
    Unavailable(String),

    /// Internal error
    #[error("Internal error: {0}")]
    Internal(String),
}

impl From<BlockStoreError> for ContentProviderError {
    fn from(err: BlockStoreError) -> Self {
        match err {
            BlockStoreError::NotFound(cid) => ContentProviderError::NotFound(cid),
            other => ContentProviderError::Storage(other.to_string()),
        }
    }
}

impl From<ContentProviderError> for errors::AppError {
    fn from(err: ContentProviderError) -> Self {
        errors::AppError::Storage(Box::new(err) as Box<dyn std::error::Error + Send + Sync>)
    }
}

// ============================================================================
// ContentProvider Trait
// ============================================================================

/// Trait for content providers that can retrieve blocks by CID.
///
/// This abstraction allows components like `DagReader` to work transparently
/// with different block sources:
/// - `LocalProvider`: Local BlockStore (RocksDB)
/// - `NetworkProvider`: Remote peers via libp2p (future)
/// - `CachingProvider`: Combination with caching (future)
///
/// All methods are async to accommodate network operations in remote providers.
#[async_trait]
pub trait ContentProvider: Send + Sync {
    /// Retrieve a block by its content identifier.
    ///
    /// # Arguments
    /// * `cid` - The content identifier of the block to retrieve
    ///
    /// # Returns
    /// * `Ok(Some(block))` - Block was found and retrieved
    /// * `Ok(None)` - Block does not exist
    /// * `Err(_)` - An error occurred during retrieval
    async fn get_block(&self, cid: &ContentId) -> Result<Option<Block>, ContentProviderError>;

    /// Check if a block exists without retrieving it.
    ///
    /// This may be more efficient than `get_block` for existence checks,
    /// especially for network providers.
    ///
    /// # Arguments
    /// * `cid` - The content identifier to check
    ///
    /// # Returns
    /// * `Ok(true)` - Block exists
    /// * `Ok(false)` - Block does not exist
    /// * `Err(_)` - An error occurred during the check
    async fn has_block(&self, cid: &ContentId) -> Result<bool, ContentProviderError>;
}

// ============================================================================
// LocalProvider
// ============================================================================

/// Local content provider backed by BlockStore.
///
/// This provider retrieves blocks from the local RocksDB-backed block store.
/// It wraps the synchronous `BlockStore` operations and executes them on
/// a blocking thread pool to avoid blocking the async runtime.
///
/// # Example
///
/// ```ignore
/// let block_store = Arc::new(BlockStore::new(config)?);
/// let provider = LocalProvider::new(block_store);
///
/// // Use as ContentProvider
/// let block = provider.get_block(&cid).await?;
/// ```
#[derive(Debug, Clone)]
pub struct LocalProvider {
    block_store: Arc<BlockStore>,
}

impl LocalProvider {
    /// Create a new local provider wrapping a BlockStore.
    ///
    /// # Arguments
    /// * `block_store` - The block store to use for retrieving blocks
    pub fn new(block_store: Arc<BlockStore>) -> Self {
        debug!("Creating LocalProvider");
        Self { block_store }
    }

    /// Get a reference to the underlying BlockStore.
    ///
    /// This is useful when you need direct access to BlockStore-specific
    /// functionality like pinning or iteration.
    pub fn block_store(&self) -> &Arc<BlockStore> {
        &self.block_store
    }
}

#[async_trait]
impl ContentProvider for LocalProvider {
    #[instrument(skip(self), fields(cid = %cid))]
    async fn get_block(&self, cid: &ContentId) -> Result<Option<Block>, ContentProviderError> {
        let store = self.block_store.clone();
        let cid = cid.clone();

        // Run the synchronous RocksDB operation on a blocking thread
        let result = tokio::task::spawn_blocking(move || store.get(&cid))
            .await
            .map_err(|e| {
                warn!(error = %e, "spawn_blocking failed for get_block");
                ContentProviderError::Internal(format!("Task join error: {}", e))
            })?;

        match result {
            Ok(block) => {
                if block.is_some() {
                    debug!("Block found in local store");
                } else {
                    debug!("Block not found in local store");
                }
                Ok(block)
            }
            Err(e) => {
                warn!(error = %e, "BlockStore error");
                Err(ContentProviderError::from(e))
            }
        }
    }

    #[instrument(skip(self), fields(cid = %cid))]
    async fn has_block(&self, cid: &ContentId) -> Result<bool, ContentProviderError> {
        let store = self.block_store.clone();
        let cid = cid.clone();

        // Run the synchronous RocksDB operation on a blocking thread
        let result = tokio::task::spawn_blocking(move || store.has(&cid))
            .await
            .map_err(|e| {
                warn!(error = %e, "spawn_blocking failed for has_block");
                ContentProviderError::Internal(format!("Task join error: {}", e))
            })?;

        match result {
            Ok(exists) => {
                debug!(exists, "Checked block existence");
                Ok(exists)
            }
            Err(e) => {
                warn!(error = %e, "BlockStore error");
                Err(ContentProviderError::from(e))
            }
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_block_store() -> (BlockStore, TempDir) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config = super::super::block_store::BlockStoreConfig {
            db_path: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        let store = BlockStore::new(config).expect("Failed to create block store");
        (store, temp_dir)
    }

    #[tokio::test]
    async fn test_local_provider_get_nonexistent_block() {
        let (store, _temp) = create_test_block_store();
        let provider = LocalProvider::new(Arc::new(store));

        let cid = ContentId::from_bytes(b"nonexistent content");
        let result = provider.get_block(&cid).await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_local_provider_put_and_get_block() {
        let (store, _temp) = create_test_block_store();
        let store = Arc::new(store);

        // Put a block directly into the store
        let data = b"hello world";
        let block = Block::new(data.to_vec());
        let cid = store.put(&block).expect("Failed to put block");

        // Now retrieve via provider
        let provider = LocalProvider::new(store);
        let result = provider.get_block(&cid).await;

        assert!(result.is_ok());
        let retrieved = result.unwrap().expect("Block should exist");
        assert_eq!(retrieved.data(), data);
    }

    #[tokio::test]
    async fn test_local_provider_has_block_false() {
        let (store, _temp) = create_test_block_store();
        let provider = LocalProvider::new(Arc::new(store));

        let cid = ContentId::from_bytes(b"nonexistent");
        let result = provider.has_block(&cid).await;

        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[tokio::test]
    async fn test_local_provider_has_block_true() {
        let (store, _temp) = create_test_block_store();
        let store = Arc::new(store);

        // Put a block
        let block = Block::new(b"test data".to_vec());
        let cid = store.put(&block).expect("Failed to put block");

        // Check via provider
        let provider = LocalProvider::new(store);
        let result = provider.has_block(&cid).await;

        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn test_local_provider_clone() {
        let (store, _temp) = create_test_block_store();
        let store = Arc::new(store);

        let provider1 = LocalProvider::new(store.clone());
        let provider2 = provider1.clone();

        // Both should work with the same underlying store
        let block = Block::new(b"shared data".to_vec());
        let cid = store.put(&block).expect("Failed to put block");

        let result1 = provider1.get_block(&cid).await;
        let result2 = provider2.get_block(&cid).await;

        assert!(result1.is_ok());
        assert!(result2.is_ok());
        assert_eq!(
            result1.unwrap().unwrap().data(),
            result2.unwrap().unwrap().data()
        );
    }

    #[tokio::test]
    async fn test_local_provider_block_store_accessor() {
        let (store, _temp) = create_test_block_store();
        let store = Arc::new(store);

        let provider = LocalProvider::new(store.clone());

        // Should be able to access underlying store
        assert!(Arc::ptr_eq(&store, provider.block_store()));
    }

    #[tokio::test]
    async fn test_local_provider_multiple_blocks() {
        let (store, _temp) = create_test_block_store();
        let store = Arc::new(store);

        // Put multiple blocks
        let blocks: Vec<_> = (0..10)
            .map(|i| {
                let data = format!("block data {}", i);
                let block = Block::new(data.into_bytes());
                let cid = store.put(&block).expect("Failed to put block");
                (cid, block)
            })
            .collect();

        let provider = LocalProvider::new(store);

        // Retrieve all blocks
        for (cid, original_block) in blocks {
            let result = provider.get_block(&cid).await;
            assert!(result.is_ok());
            let retrieved = result.unwrap().expect("Block should exist");
            assert_eq!(retrieved.data(), original_block.data());
        }
    }

    #[tokio::test]
    async fn test_content_provider_error_from_block_store_error() {
        // Test error conversion
        let cid = ContentId::from_bytes(b"test");
        let block_store_err = BlockStoreError::NotFound(cid.clone());
        let provider_err: ContentProviderError = block_store_err.into();

        match provider_err {
            ContentProviderError::NotFound(c) => assert_eq!(c, cid),
            _ => panic!("Expected NotFound error"),
        }

        // Test storage error conversion
        let storage_err = BlockStoreError::Storage("test error".to_string());
        let provider_err: ContentProviderError = storage_err.into();

        match provider_err {
            ContentProviderError::Storage(msg) => assert!(msg.contains("test error")),
            _ => panic!("Expected Storage error"),
        }
    }
}
