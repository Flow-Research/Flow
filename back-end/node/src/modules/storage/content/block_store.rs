use rocksdb::{ColumnFamilyDescriptor, DB, DBIteratorWithThreadMode, IteratorMode, Options};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, error, info, instrument, warn};

use super::block::Block;
use super::cid::ContentId;
use super::error::BlockStoreError;

// Column family names
const CF_BLOCKS: &str = "blocks";
const CF_PINS: &str = "pins";
const CF_METADATA: &str = "block_metadata";

// Metadata keys
const KEY_BLOCK_COUNT: &[u8] = b"block_count";
const KEY_TOTAL_SIZE: &[u8] = b"total_size";

/// Configuration for the block store.
#[derive(Debug, Clone)]
pub struct BlockStoreConfig {
    /// Path to the RocksDB database directory
    pub db_path: PathBuf,
    /// Maximum size for a single block in bytes (default: 1MB)
    pub max_block_size: usize,
    /// Maximum total storage size in bytes (default: 10GB, 0 = unlimited)
    pub max_total_size_bytes: u64,
    /// Enable LZ4 compression (default: true)
    pub enable_compression: bool,
    /// Maximum open files for RocksDB (default: 512)
    pub max_open_files: i32,
}

impl Default for BlockStoreConfig {
    fn default() -> Self {
        Self {
            db_path: PathBuf::from("/tmp/flow/blocks"),
            max_block_size: 1024 * 1024,                   // 1MB
            max_total_size_bytes: 10 * 1024 * 1024 * 1024, // 10GB
            enable_compression: true,
            max_open_files: 512,
        }
    }
}

impl BlockStoreConfig {
    /// Load configuration from environment variables.
    ///
    /// Environment variables:
    /// - `BLOCK_STORE_PATH`: Database path
    /// - `BLOCK_STORE_MAX_BLOCK_SIZE`: Max block size in bytes
    /// - `BLOCK_STORE_MAX_TOTAL_SIZE`: Max total storage in bytes
    /// - `BLOCK_STORE_COMPRESSION`: Enable compression (true/false)
    /// - `BLOCK_STORE_MAX_OPEN_FILES`: Max open files
    pub fn from_env() -> Self {
        use std::env;

        let db_path = env::var("BLOCK_STORE_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                directories::ProjectDirs::from("network", "flow", "node")
                    .map(|dirs| dirs.data_dir().join("blocks"))
                    .unwrap_or_else(|| PathBuf::from("/tmp/flow/blocks"))
            });

        let max_block_size = env::var("BLOCK_STORE_MAX_BLOCK_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1024 * 1024);

        let max_total_size_bytes = env::var("BLOCK_STORE_MAX_TOTAL_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(10 * 1024 * 1024 * 1024);

        let enable_compression = env::var("BLOCK_STORE_COMPRESSION")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(true);

        let max_open_files = env::var("BLOCK_STORE_MAX_OPEN_FILES")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(512);

        Self {
            db_path,
            max_block_size,
            max_total_size_bytes,
            enable_compression,
            max_open_files,
        }
    }
}

/// RocksDB-backed block store.
///
/// Provides content-addressed storage with support for:
/// - Block storage and retrieval by CID
/// - Pinning to prevent garbage collection
/// - Metadata tracking (block count, total size)
/// - Iteration over all blocks
pub struct BlockStore {
    db: Arc<DB>,
    config: BlockStoreConfig,
}

impl BlockStore {
    /// Create or open a block store at the configured path.
    #[instrument(skip(config), fields(path = %config.db_path.display()))]
    pub fn new(config: BlockStoreConfig) -> Result<Self, BlockStoreError> {
        info!(
            path = %config.db_path.display(),
            max_block_size = config.max_block_size,
            max_total_size = config.max_total_size_bytes,
            compression = config.enable_compression,
            "Opening block store"
        );

        // Ensure parent directory exists
        if let Some(parent) = config.db_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                error!(error = %e, "Failed to create block store directory");
                BlockStoreError::Config(format!("Failed to create directory: {}", e))
            })?;
        }

        // Configure RocksDB options
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);
        opts.set_max_open_files(config.max_open_files);
        opts.set_keep_log_file_num(5);

        if config.enable_compression {
            opts.set_compression_type(rocksdb::DBCompressionType::Lz4);
        }

        // Define column families
        let cf_blocks = ColumnFamilyDescriptor::new(CF_BLOCKS, Options::default());
        let cf_pins = ColumnFamilyDescriptor::new(CF_PINS, Options::default());
        let cf_metadata = ColumnFamilyDescriptor::new(CF_METADATA, Options::default());

        // Open database
        let db = DB::open_cf_descriptors(
            &opts,
            &config.db_path,
            vec![cf_blocks, cf_pins, cf_metadata],
        )
        .map_err(|e| {
            error!(error = %e, "Failed to open block store database");
            BlockStoreError::Database(e.to_string())
        })?;

        let store = Self {
            db: Arc::new(db),
            config,
        };

        // Log initial stats
        let stats = store.stats()?;
        info!(
            block_count = stats.block_count,
            total_size = stats.total_size_bytes,
            "Block store opened"
        );

        Ok(store)
    }

    /// Store a block.
    ///
    /// Returns the CID of the stored block.
    /// If the block already exists, this is a no-op.
    #[instrument(skip(self, block), fields(cid = %block.cid(), size = block.size()))]
    pub fn put(&self, block: &Block) -> Result<ContentId, BlockStoreError> {
        let cid = block.cid();

        // Check block size limit
        if block.size() > self.config.max_block_size {
            warn!(
                size = block.size(),
                max = self.config.max_block_size,
                "Block exceeds maximum size"
            );
            return Err(BlockStoreError::BlockTooLarge {
                size: block.size(),
                max: self.config.max_block_size,
            });
        }

        // Check if already exists (deduplication)
        if self.has(cid)? {
            debug!(cid = %cid, "Block already exists, skipping");
            return Ok(cid.clone());
        }

        // Check total size limit
        if self.config.max_total_size_bytes > 0 {
            let stats = self.stats()?;
            if stats.total_size_bytes + block.size() as u64 > self.config.max_total_size_bytes {
                warn!(
                    current = stats.total_size_bytes,
                    block_size = block.size(),
                    max = self.config.max_total_size_bytes,
                    "Block store capacity exceeded"
                );
                return Err(BlockStoreError::Storage(
                    "Block store capacity exceeded".to_string(),
                ));
            }
        }

        // Serialize block
        let bytes = block.to_stored_bytes()?;
        let key = cid.to_bytes();

        // Store block
        let cf = self.db.cf_handle(CF_BLOCKS).ok_or_else(|| {
            BlockStoreError::Database("Blocks column family not found".to_string())
        })?;

        self.db.put_cf(&cf, &key, &bytes)?;

        // Update metadata
        self.increment_stats(block.size())?;

        info!(cid = %cid, size = block.size(), "Block stored");

        Ok(cid.clone())
    }

    /// Retrieve a block by CID.
    ///
    /// Returns `None` if the block doesn't exist.
    /// Verifies block integrity on retrieval.
    #[instrument(skip(self), fields(cid = %cid))]
    pub fn get(&self, cid: &ContentId) -> Result<Option<Block>, BlockStoreError> {
        let cf = self.db.cf_handle(CF_BLOCKS).ok_or_else(|| {
            BlockStoreError::Database("Blocks column family not found".to_string())
        })?;

        let key = cid.to_bytes();

        match self.db.get_cf(&cf, &key)? {
            Some(bytes) => {
                let block = Block::from_stored_bytes(cid.clone(), &bytes)?;

                // Verify integrity
                if !block.verify() {
                    error!(cid = %cid, "Block integrity verification failed");
                    return Err(BlockStoreError::IntegrityError {
                        expected: cid.clone(),
                        actual: block.cid().clone(),
                    });
                }

                debug!(cid = %cid, size = block.size(), "Block retrieved");
                Ok(Some(block))
            }
            None => {
                debug!(cid = %cid, "Block not found");
                Ok(None)
            }
        }
    }

    /// Check if a block exists.
    #[instrument(skip(self), fields(cid = %cid))]
    pub fn has(&self, cid: &ContentId) -> Result<bool, BlockStoreError> {
        let cf = self.db.cf_handle(CF_BLOCKS).ok_or_else(|| {
            BlockStoreError::Database("Blocks column family not found".to_string())
        })?;

        let key = cid.to_bytes();
        let exists = self.db.get_cf(&cf, &key)?.is_some();

        debug!(cid = %cid, exists = exists, "Block existence check");
        Ok(exists)
    }

    /// Delete a block.
    ///
    /// Does nothing if the block doesn't exist or is pinned.
    /// Returns `true` if the block was deleted.
    #[instrument(skip(self), fields(cid = %cid))]
    pub fn delete(&self, cid: &ContentId) -> Result<bool, BlockStoreError> {
        // Check if pinned
        if self.is_pinned(cid)? {
            warn!(cid = %cid, "Cannot delete pinned block");
            return Ok(false);
        }

        let cf = self.db.cf_handle(CF_BLOCKS).ok_or_else(|| {
            BlockStoreError::Database("Blocks column family not found".to_string())
        })?;

        let key = cid.to_bytes();

        // Get block size for stats update
        let size = match self.db.get_cf(&cf, &key)? {
            Some(bytes) => {
                let block = Block::from_stored_bytes(cid.clone(), &bytes)?;
                block.size()
            }
            None => {
                debug!(cid = %cid, "Block not found for deletion");
                return Ok(false);
            }
        };

        // Delete block
        self.db.delete_cf(&cf, &key)?;

        // Update metadata
        self.decrement_stats(size)?;

        info!(cid = %cid, "Block deleted");
        Ok(true)
    }

    /// Pin a block to prevent garbage collection.
    #[instrument(skip(self), fields(cid = %cid))]
    pub fn pin(&self, cid: &ContentId) -> Result<(), BlockStoreError> {
        if !self.has(cid)? {
            return Err(BlockStoreError::NotFound(cid.clone()));
        }

        let cf = self
            .db
            .cf_handle(CF_PINS)
            .ok_or_else(|| BlockStoreError::Database("Pins column family not found".to_string()))?;

        let key = cid.to_bytes();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        self.db.put_cf(&cf, &key, timestamp.to_le_bytes())?;

        info!(cid = %cid, "Block pinned");
        Ok(())
    }

    /// Unpin a block.
    #[instrument(skip(self), fields(cid = %cid))]
    pub fn unpin(&self, cid: &ContentId) -> Result<(), BlockStoreError> {
        let cf = self
            .db
            .cf_handle(CF_PINS)
            .ok_or_else(|| BlockStoreError::Database("Pins column family not found".to_string()))?;

        let key = cid.to_bytes();
        self.db.delete_cf(&cf, &key)?;

        info!(cid = %cid, "Block unpinned");
        Ok(())
    }

    /// Check if a block is pinned.
    pub fn is_pinned(&self, cid: &ContentId) -> Result<bool, BlockStoreError> {
        let cf = self
            .db
            .cf_handle(CF_PINS)
            .ok_or_else(|| BlockStoreError::Database("Pins column family not found".to_string()))?;

        let key = cid.to_bytes();
        Ok(self.db.get_cf(&cf, &key)?.is_some())
    }

    /// Iterate over all blocks.
    pub fn iter(&self) -> Result<BlockIterator<'_>, BlockStoreError> {
        let cf = self.db.cf_handle(CF_BLOCKS).ok_or_else(|| {
            BlockStoreError::Database("Blocks column family not found".to_string())
        })?;

        let iter = self.db.iterator_cf(&cf, IteratorMode::Start);

        Ok(BlockIterator {
            inner: iter,
            _db: Arc::clone(&self.db),
        })
    }

    /// Get storage statistics.
    pub fn stats(&self) -> Result<BlockStoreStats, BlockStoreError> {
        let cf = self.db.cf_handle(CF_METADATA).ok_or_else(|| {
            BlockStoreError::Database("Metadata column family not found".to_string())
        })?;

        let block_count = self
            .db
            .get_cf(&cf, KEY_BLOCK_COUNT)?
            .map(|b| u64::from_le_bytes(b.try_into().unwrap_or([0; 8])))
            .unwrap_or(0);

        let total_size_bytes = self
            .db
            .get_cf(&cf, KEY_TOTAL_SIZE)?
            .map(|b| u64::from_le_bytes(b.try_into().unwrap_or([0; 8])))
            .unwrap_or(0);

        Ok(BlockStoreStats {
            block_count,
            total_size_bytes,
            max_block_size: self.config.max_block_size,
            max_total_size_bytes: self.config.max_total_size_bytes,
        })
    }

    /// Flush all pending writes to disk.
    pub fn flush(&self) -> Result<(), BlockStoreError> {
        if let Some(cf) = self.db.cf_handle(CF_BLOCKS) {
            self.db.flush_cf(&cf)?;
        }
        if let Some(cf) = self.db.cf_handle(CF_PINS) {
            self.db.flush_cf(&cf)?;
        }
        if let Some(cf) = self.db.cf_handle(CF_METADATA) {
            self.db.flush_cf(&cf)?;
        }
        info!("Block store flushed");
        Ok(())
    }

    /// Get the database path.
    pub fn path(&self) -> &Path {
        &self.config.db_path
    }

    // Internal: increment stats after storing a block
    fn increment_stats(&self, size: usize) -> Result<(), BlockStoreError> {
        let cf = self.db.cf_handle(CF_METADATA).ok_or_else(|| {
            BlockStoreError::Database("Metadata column family not found".to_string())
        })?;

        // Update block count
        let count = self
            .db
            .get_cf(&cf, KEY_BLOCK_COUNT)?
            .map(|b| u64::from_le_bytes(b.try_into().unwrap_or([0; 8])))
            .unwrap_or(0)
            + 1;
        self.db.put_cf(&cf, KEY_BLOCK_COUNT, count.to_le_bytes())?;

        // Update total size
        let total = self
            .db
            .get_cf(&cf, KEY_TOTAL_SIZE)?
            .map(|b| u64::from_le_bytes(b.try_into().unwrap_or([0; 8])))
            .unwrap_or(0)
            + size as u64;
        self.db.put_cf(&cf, KEY_TOTAL_SIZE, total.to_le_bytes())?;

        Ok(())
    }

    // Internal: decrement stats after deleting a block
    fn decrement_stats(&self, size: usize) -> Result<(), BlockStoreError> {
        let cf = self.db.cf_handle(CF_METADATA).ok_or_else(|| {
            BlockStoreError::Database("Metadata column family not found".to_string())
        })?;

        // Update block count
        let count = self
            .db
            .get_cf(&cf, KEY_BLOCK_COUNT)?
            .map(|b| u64::from_le_bytes(b.try_into().unwrap_or([0; 8])))
            .unwrap_or(0)
            .saturating_sub(1);
        self.db.put_cf(&cf, KEY_BLOCK_COUNT, count.to_le_bytes())?;

        // Update total size
        let total = self
            .db
            .get_cf(&cf, KEY_TOTAL_SIZE)?
            .map(|b| u64::from_le_bytes(b.try_into().unwrap_or([0; 8])))
            .unwrap_or(0)
            .saturating_sub(size as u64);
        self.db.put_cf(&cf, KEY_TOTAL_SIZE, total.to_le_bytes())?;

        Ok(())
    }
}

impl Drop for BlockStore {
    fn drop(&mut self) {
        if let Err(e) = self.flush() {
            error!(error = %e, "Failed to flush block store on drop");
        }
        info!(path = %self.config.db_path.display(), "Block store closed");
    }
}

/// Statistics about the block store.
#[derive(Debug, Clone)]
pub struct BlockStoreStats {
    /// Number of blocks stored
    pub block_count: u64,
    /// Total size of all blocks in bytes
    pub total_size_bytes: u64,
    /// Maximum allowed block size
    pub max_block_size: usize,
    /// Maximum total storage size (0 = unlimited)
    pub max_total_size_bytes: u64,
}

/// Iterator over blocks in the store.
pub struct BlockIterator<'a> {
    inner: DBIteratorWithThreadMode<'a, DB>,
    _db: Arc<DB>,
}

impl<'a> Iterator for BlockIterator<'a> {
    type Item = Result<Block, BlockStoreError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|result| {
            let (key, value) = result.map_err(BlockStoreError::from)?;

            let cid = ContentId::from_raw_bytes(&key)?;
            Block::from_stored_bytes(cid, &value)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_store() -> (BlockStore, TempDir) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config = BlockStoreConfig {
            db_path: temp_dir.path().join("blocks"),
            max_block_size: 1024 * 1024,
            max_total_size_bytes: 0, // Unlimited for tests
            enable_compression: false,
            max_open_files: 128,
        };
        let store = BlockStore::new(config).expect("Failed to create store");
        (store, temp_dir)
    }

    #[test]
    fn test_store_creation() {
        let (store, _temp_dir) = create_test_store();
        let stats = store.stats().unwrap();
        assert_eq!(stats.block_count, 0);
        assert_eq!(stats.total_size_bytes, 0);
    }

    #[test]
    fn test_put_and_get() {
        let (store, _temp_dir) = create_test_store();
        let block = Block::new(b"hello world".to_vec());
        let cid = block.cid().clone();

        store.put(&block).unwrap();

        let retrieved = store.get(&cid).unwrap().unwrap();
        assert_eq!(retrieved.data(), b"hello world");
        assert_eq!(retrieved.cid(), &cid);
    }

    #[test]
    fn test_has() {
        let (store, _temp_dir) = create_test_store();
        let block = Block::new(b"test data".to_vec());
        let cid = block.cid().clone();

        assert!(!store.has(&cid).unwrap());

        store.put(&block).unwrap();

        assert!(store.has(&cid).unwrap());
    }

    #[test]
    fn test_delete() {
        let (store, _temp_dir) = create_test_store();
        let block = Block::new(b"delete me".to_vec());
        let cid = block.cid().clone();

        store.put(&block).unwrap();
        assert!(store.has(&cid).unwrap());

        let deleted = store.delete(&cid).unwrap();
        assert!(deleted);
        assert!(!store.has(&cid).unwrap());
    }

    #[test]
    fn test_delete_nonexistent() {
        let (store, _temp_dir) = create_test_store();
        let cid = ContentId::from_bytes(b"nonexistent");

        let deleted = store.delete(&cid).unwrap();
        assert!(!deleted);
    }

    #[test]
    fn test_pin_prevents_delete() {
        let (store, _temp_dir) = create_test_store();
        let block = Block::new(b"pinned data".to_vec());
        let cid = block.cid().clone();

        store.put(&block).unwrap();
        store.pin(&cid).unwrap();

        let deleted = store.delete(&cid).unwrap();
        assert!(!deleted);
        assert!(store.has(&cid).unwrap());

        store.unpin(&cid).unwrap();

        let deleted = store.delete(&cid).unwrap();
        assert!(deleted);
    }

    #[test]
    fn test_stats_update() {
        let (store, _temp_dir) = create_test_store();

        let block1 = Block::new(b"block one".to_vec());
        let block2 = Block::new(b"block two".to_vec());

        store.put(&block1).unwrap();

        let stats = store.stats().unwrap();
        assert_eq!(stats.block_count, 1);

        store.put(&block2).unwrap();

        let stats = store.stats().unwrap();
        assert_eq!(stats.block_count, 2);

        store.delete(block1.cid()).unwrap();

        let stats = store.stats().unwrap();
        assert_eq!(stats.block_count, 1);
    }

    #[test]
    fn test_deduplication() {
        let (store, _temp_dir) = create_test_store();
        let data = b"duplicate data".to_vec();

        let block1 = Block::new(data.clone());
        let block2 = Block::new(data);

        store.put(&block1).unwrap();
        store.put(&block2).unwrap();

        let stats = store.stats().unwrap();
        assert_eq!(stats.block_count, 1);
    }

    #[test]
    fn test_block_too_large() {
        let temp_dir = TempDir::new().unwrap();
        let config = BlockStoreConfig {
            db_path: temp_dir.path().join("blocks"),
            max_block_size: 100, // Very small limit
            ..Default::default()
        };
        let store = BlockStore::new(config).unwrap();

        let large_block = Block::new(vec![0u8; 200]);
        let result = store.put(&large_block);

        assert!(matches!(result, Err(BlockStoreError::BlockTooLarge { .. })));
    }

    #[test]
    fn test_iteration() {
        let (store, _temp_dir) = create_test_store();

        let block1 = Block::new(b"first".to_vec());
        let block2 = Block::new(b"second".to_vec());
        let block3 = Block::new(b"third".to_vec());

        store.put(&block1).unwrap();
        store.put(&block2).unwrap();
        store.put(&block3).unwrap();

        let blocks: Vec<_> = store.iter().unwrap().collect();
        assert_eq!(blocks.len(), 3);

        for result in blocks {
            assert!(result.is_ok());
        }
    }

    #[test]
    fn test_block_with_links() {
        let (store, _temp_dir) = create_test_store();

        let child1 = Block::new(b"child 1".to_vec());
        let child2 = Block::new(b"child 2".to_vec());

        store.put(&child1).unwrap();
        store.put(&child2).unwrap();

        let parent = Block::with_links(
            b"parent".to_vec(),
            vec![child1.cid().clone(), child2.cid().clone()],
        );

        store.put(&parent).unwrap();

        let retrieved = store.get(parent.cid()).unwrap().unwrap();
        assert_eq!(retrieved.links().len(), 2);
        assert!(retrieved.links().contains(child1.cid()));
        assert!(retrieved.links().contains(child2.cid()));
    }

    #[test]
    fn test_persistence() {
        let temp_dir = TempDir::new().unwrap();
        let config = BlockStoreConfig {
            db_path: temp_dir.path().join("blocks"),
            ..Default::default()
        };

        let cid;

        {
            let store = BlockStore::new(config.clone()).unwrap();
            let block = Block::new(b"persistent data".to_vec());
            cid = block.cid().clone();
            store.put(&block).unwrap();
        }

        {
            let store = BlockStore::new(config).unwrap();
            let retrieved = store.get(&cid).unwrap();
            assert!(retrieved.is_some());
            assert_eq!(retrieved.unwrap().data(), b"persistent data");
        }
    }

    #[test]
    fn test_get_nonexistent() {
        let (store, _temp_dir) = create_test_store();
        let cid = ContentId::from_bytes(b"nonexistent");

        let result = store.get(&cid).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_empty_block() {
        let (store, _temp_dir) = create_test_store();
        let block = Block::new(vec![]);

        store.put(&block).unwrap();

        let retrieved = store.get(block.cid()).unwrap().unwrap();
        assert_eq!(retrieved.size(), 0);
    }

    #[test]
    fn test_binary_data() {
        let (store, _temp_dir) = create_test_store();
        let data: Vec<u8> = (0..=255).collect();
        let block = Block::new(data.clone());

        store.put(&block).unwrap();

        let retrieved = store.get(block.cid()).unwrap().unwrap();
        assert_eq!(retrieved.data(), &data);
    }

    #[test]
    fn test_config_from_env() {
        let _config = BlockStoreConfig::from_env();
    }

    #[test]
    fn test_pin_nonexistent() {
        let (store, _temp_dir) = create_test_store();
        let cid = ContentId::from_bytes(b"nonexistent");

        let result = store.pin(&cid);
        assert!(matches!(result, Err(BlockStoreError::NotFound(_))));
    }

    #[test]
    fn test_is_pinned() {
        let (store, _temp_dir) = create_test_store();
        let block = Block::new(b"pin test".to_vec());

        store.put(&block).unwrap();
        assert!(!store.is_pinned(block.cid()).unwrap());

        store.pin(block.cid()).unwrap();
        assert!(store.is_pinned(block.cid()).unwrap());

        store.unpin(block.cid()).unwrap();
        assert!(!store.is_pinned(block.cid()).unwrap());
    }

    #[test]
    fn test_total_size_limit() {
        let temp_dir = TempDir::new().unwrap();
        let config = BlockStoreConfig {
            db_path: temp_dir.path().join("blocks"),
            max_block_size: 1000,
            max_total_size_bytes: 500, // Small limit
            ..Default::default()
        };
        let store = BlockStore::new(config).unwrap();

        let block1 = Block::new(vec![0u8; 200]);
        store.put(&block1).unwrap();

        let block2 = Block::new(vec![1u8; 200]);
        store.put(&block2).unwrap();

        // This should fail - would exceed 500 bytes
        let block3 = Block::new(vec![2u8; 200]);
        let result = store.put(&block3);

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("capacity exceeded")
        );
    }

    #[test]
    fn test_stats_total_size_bytes() {
        let (store, _temp_dir) = create_test_store();

        let block1 = Block::new(vec![0u8; 100]);
        let block2 = Block::new(vec![1u8; 200]);

        store.put(&block1).unwrap();
        let stats = store.stats().unwrap();
        // Note: stored size includes serialization overhead
        assert!(stats.total_size_bytes > 0);

        store.put(&block2).unwrap();
        let stats2 = store.stats().unwrap();
        assert!(stats2.total_size_bytes > stats.total_size_bytes);

        store.delete(block1.cid()).unwrap();
        let stats3 = store.stats().unwrap();
        assert!(stats3.total_size_bytes < stats2.total_size_bytes);
    }

    #[test]
    fn test_iteration_empty() {
        let (store, _temp_dir) = create_test_store();

        let blocks: Vec<_> = store.iter().unwrap().collect();
        assert!(blocks.is_empty());
    }

    #[test]
    fn test_flush() {
        let (store, _temp_dir) = create_test_store();
        let block = Block::new(b"flush test".to_vec());
        store.put(&block).unwrap();

        // Should not error
        store.flush().unwrap();
    }

    #[test]
    fn test_path() {
        let temp_dir = TempDir::new().unwrap();
        let expected_path = temp_dir.path().join("blocks");
        let config = BlockStoreConfig {
            db_path: expected_path.clone(),
            ..Default::default()
        };
        let store = BlockStore::new(config).unwrap();

        assert_eq!(store.path(), expected_path);
    }

    #[test]
    fn test_compression_enabled() {
        let temp_dir = TempDir::new().unwrap();
        let config = BlockStoreConfig {
            db_path: temp_dir.path().join("blocks"),
            enable_compression: true,
            ..Default::default()
        };
        let store = BlockStore::new(config).unwrap();

        // Store highly compressible data
        let block = Block::new(vec![0u8; 10000]);
        store.put(&block).unwrap();

        let retrieved = store.get(block.cid()).unwrap().unwrap();
        assert_eq!(retrieved.data(), &vec![0u8; 10000]);
    }

    #[test]
    fn test_pin_persistence() {
        let temp_dir = TempDir::new().unwrap();
        let config = BlockStoreConfig {
            db_path: temp_dir.path().join("blocks"),
            ..Default::default()
        };

        let cid;
        {
            let store = BlockStore::new(config.clone()).unwrap();
            let block = Block::new(b"pinned persistent".to_vec());
            cid = block.cid().clone();
            store.put(&block).unwrap();
            store.pin(&cid).unwrap();
        }

        {
            let store = BlockStore::new(config).unwrap();
            assert!(store.is_pinned(&cid).unwrap());
        }
    }

    #[test]
    fn test_stats_persistence() {
        let temp_dir = TempDir::new().unwrap();
        let config = BlockStoreConfig {
            db_path: temp_dir.path().join("blocks"),
            ..Default::default()
        };

        {
            let store = BlockStore::new(config.clone()).unwrap();
            store.put(&Block::new(b"one".to_vec())).unwrap();
            store.put(&Block::new(b"two".to_vec())).unwrap();
        }

        {
            let store = BlockStore::new(config).unwrap();
            let stats = store.stats().unwrap();
            assert_eq!(stats.block_count, 2);
        }
    }
}
