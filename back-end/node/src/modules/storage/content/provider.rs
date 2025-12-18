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
use futures::{StreamExt, stream};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tracing::{debug, info, instrument, warn};

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

    /// All provider attempts exhausted
    #[error("All {attempts} provider attempts exhausted for {cid}")]
    ProvidersExhausted { cid: ContentId, attempts: usize },
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
/// - `NetworkProvider`: Remote peers via libp2p
/// - `CachingProvider`: Combination with caching
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

    /// Retrieve multiple blocks by their CIDs in a batch operation.
    ///
    /// This is more efficient than calling `get_block` multiple times,
    /// especially for network providers where it enables parallel fetching,
    /// and for local providers where it enables batched disk I/O.
    ///
    /// # Arguments
    /// * `cids` - Slice of content identifiers to retrieve
    ///
    /// # Returns
    /// A vector of `Option<Block>` in the same order as the input CIDs.
    /// Each element is `Some(block)` if found, `None` if not found.
    ///
    /// # Default Implementation
    /// Calls `get_block` sequentially for each CID. Implementors should
    /// override this for better performance.
    async fn get_blocks(
        &self,
        cids: &[ContentId],
    ) -> Result<Vec<Option<Block>>, ContentProviderError> {
        debug!(
            count = cids.len(),
            "get_blocks: using default sequential implementation"
        );

        let mut results = Vec::with_capacity(cids.len());
        for cid in cids {
            results.push(self.get_block(cid).await?);
        }
        Ok(results)
    }
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

    /// Optimized batch retrieval using RocksDB's multi_get.
    ///
    /// Uses a single `spawn_blocking` call for all reads, leveraging
    /// RocksDB's `multi_get_cf` for efficient batch disk I/O.
    #[instrument(skip(self), fields(count = cids.len()))]
    async fn get_blocks(
        &self,
        cids: &[ContentId],
    ) -> Result<Vec<Option<Block>>, ContentProviderError> {
        if cids.is_empty() {
            debug!("Empty batch request");
            return Ok(vec![]);
        }

        let store = self.block_store.clone();
        let cids = cids.to_vec();
        let cids_length = cids.len();

        debug!(batch_size = cids.len(), "Executing batch block retrieval");

        // Run the batch RocksDB operation on a blocking thread
        let result = tokio::task::spawn_blocking(move || store.get_batch(&cids))
            .await
            .map_err(|e| {
                warn!(error = %e, "spawn_blocking failed for get_blocks");
                ContentProviderError::Internal(format!("Task join error: {}", e))
            })?;

        match result {
            Ok(blocks) => {
                let found = blocks.iter().filter(|b| b.is_some()).count();
                debug!(
                    batch_size = cids_length,
                    found,
                    missing = cids_length - found,
                    "Batch retrieval complete"
                );
                Ok(blocks)
            }
            Err(e) => {
                warn!(error = %e, "BlockStore batch error");
                Err(ContentProviderError::from(e))
            }
        }
    }
}

// ============================================================================
// NetworkProviderConfig
// ============================================================================

/// Configuration for NetworkProvider behavior.
#[derive(Debug, Clone)]
pub struct NetworkProviderConfig {
    /// Maximum number of providers to try before giving up (default: 3).
    pub max_provider_attempts: usize,

    /// Whether to cache fetched blocks locally (default: true).
    pub cache_remote_blocks: bool,

    /// Whether has_block should check network if not found locally (default: false).
    /// Setting this to true can be slow as it requires DHT queries.
    pub check_network_for_existence: bool,

    /// Request timeout for individual peer requests (default: 30s).
    pub request_timeout: Duration,

    /// Retry delay between provider attempts (default: 100ms).
    pub retry_delay: Duration,

    /// Maximum number of parallel network fetch operations.
    /// This limits concurrent DHT queries and block requests to avoid
    /// overwhelming the network.
    pub max_parallel_fetch: usize,
}

impl Default for NetworkProviderConfig {
    fn default() -> Self {
        Self {
            max_provider_attempts: 3,
            cache_remote_blocks: true,
            check_network_for_existence: false,
            request_timeout: Duration::from_secs(30),
            retry_delay: Duration::from_millis(100),
            max_parallel_fetch: 10,
        }
    }
}

impl NetworkProviderConfig {
    /// Load configuration from environment variables.
    ///
    /// Environment variables:
    /// - `NETWORK_PROVIDER_MAX_ATTEMPTS`: Max provider attempts
    /// - `NETWORK_PROVIDER_CACHE_REMOTE`: Cache remote blocks (true/false)
    /// - `NETWORK_PROVIDER_CHECK_NETWORK_EXISTENCE`: Check network for has_block
    /// - `NETWORK_PROVIDER_TIMEOUT_SECS`: Request timeout in seconds
    /// - `NETWORK_PROVIDER_RETRY_DELAY_MS`: Retry delay in milliseconds
    /// - `NETWORK_PROVIDER_MAX_PARALLEL_FETCHES`: Max parallel fetch operations
    pub fn from_env() -> Self {
        use std::env;

        Self {
            max_provider_attempts: env::var("NETWORK_PROVIDER_MAX_ATTEMPTS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(3),
            cache_remote_blocks: env::var("NETWORK_PROVIDER_CACHE_REMOTE")
                .map(|s| s != "false" && s != "0")
                .unwrap_or(true),
            check_network_for_existence: env::var("NETWORK_PROVIDER_CHECK_NETWORK_EXISTENCE")
                .map(|s| s == "true" || s == "1")
                .unwrap_or(false),
            request_timeout: Duration::from_secs(
                env::var("NETWORK_PROVIDER_TIMEOUT_SECS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(30),
            ),
            retry_delay: Duration::from_millis(
                env::var("NETWORK_PROVIDER_RETRY_DELAY_MS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(100),
            ),
            max_parallel_fetch: env::var("NETWORK_PROVIDER_MAX_PARALLEL_FETCH")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(10),
        }
    }

    /// Create config with caching disabled (for testing).
    pub fn without_caching() -> Self {
        Self {
            cache_remote_blocks: false,
            ..Default::default()
        }
    }
}

// ============================================================================
// NetworkProvider
// ============================================================================

use crate::modules::network::manager::NetworkManager;

/// Network-aware content provider.
///
/// Combines local storage with network retrieval for transparent
/// access to content-addressed blocks. Implements a local-first
/// strategy with automatic DHT discovery and P2P content transfer.
///
/// # Retrieval Strategy
///
/// 1. **Local Check**: First checks the local `BlockStore` via `LocalProvider`
/// 2. **DHT Discovery**: If not found locally, queries the Kademlia DHT for providers
/// 3. **P2P Fetch**: Attempts to fetch from discovered providers
/// 4. **Local Caching**: Caches successfully fetched blocks locally
///
/// # Example
///
/// ```ignore
/// let block_store = Arc::new(BlockStore::new(config)?);
/// let local_provider = LocalProvider::new(block_store.clone());
/// let network_manager = Arc::new(NetworkManager::new(&node_data).await?);
///
/// let provider = NetworkProvider::new(
///     local_provider,
///     network_manager,
///     NetworkProviderConfig::default(),
/// );
///
/// // Transparently fetches from local or network
/// if let Some(block) = provider.get_block(&cid).await? {
///     println!("Got block: {} bytes", block.size());
/// }
/// ```
#[derive(Clone)]
pub struct NetworkProvider {
    /// Local provider for BlockStore access
    local: LocalProvider,
    /// Network manager for DHT and P2P operations
    network: Arc<NetworkManager>,
    /// Configuration
    config: NetworkProviderConfig,
}

impl std::fmt::Debug for NetworkProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NetworkProvider")
            .field("local", &self.local)
            .field(
                "network",
                &format!("NetworkManager({})", self.network.local_peer_id()),
            )
            .field("config", &self.config)
            .finish()
    }
}

impl NetworkProvider {
    /// Create a new NetworkProvider.
    ///
    /// # Arguments
    /// * `local` - LocalProvider for block store access
    /// * `network` - NetworkManager for DHT and P2P operations
    /// * `config` - Configuration options
    pub fn new(
        local: LocalProvider,
        network: Arc<NetworkManager>,
        config: NetworkProviderConfig,
    ) -> Self {
        info!(
            peer_id = %network.local_peer_id(),
            max_attempts = config.max_provider_attempts,
            cache_remote = config.cache_remote_blocks,
            "Creating NetworkProvider"
        );
        Self {
            local,
            network,
            config,
        }
    }

    /// Create a NetworkProvider with default configuration.
    pub fn with_defaults(local: LocalProvider, network: Arc<NetworkManager>) -> Self {
        Self::new(local, network, NetworkProviderConfig::default())
    }

    /// Get reference to the underlying LocalProvider.
    pub fn local_provider(&self) -> &LocalProvider {
        &self.local
    }

    /// Get reference to the underlying NetworkManager.
    pub fn network_manager(&self) -> &Arc<NetworkManager> {
        &self.network
    }

    /// Get reference to the configuration.
    pub fn config(&self) -> &NetworkProviderConfig {
        &self.config
    }

    /// Get reference to the underlying BlockStore.
    pub fn block_store(&self) -> &Arc<BlockStore> {
        self.local.block_store()
    }

    /// Fetch block from network and optionally cache it locally.
    #[instrument(skip(self), fields(cid = %cid))]
    async fn fetch_from_network(
        &self,
        cid: &ContentId,
    ) -> Result<Option<Block>, ContentProviderError> {
        // Step 1: Query DHT for providers
        debug!("Querying DHT for providers");

        let discovery_result = self.network.get_providers(cid.clone()).await.map_err(|e| {
            warn!(error = %e, "DHT provider discovery failed");
            ContentProviderError::Network(format!("Provider discovery failed: {}", e))
        })?;

        if discovery_result.providers.is_empty() {
            debug!("No providers found in DHT");
            return Ok(None);
        }

        info!(
            provider_count = discovery_result.providers.len(),
            "Found providers, attempting fetch"
        );

        // Step 2: Try each provider until success
        let max_attempts = self
            .config
            .max_provider_attempts
            .min(discovery_result.providers.len());
        let mut last_error = None;

        for (attempt, peer_id) in discovery_result
            .providers
            .iter()
            .take(max_attempts)
            .enumerate()
        {
            debug!(
                %peer_id,
                attempt = attempt + 1,
                max_attempts,
                "Requesting block from provider"
            );

            match self.network.request_block(*peer_id, cid.clone()).await {
                Ok(block) => {
                    info!(%peer_id, "Successfully fetched block from provider");

                    // Step 3: Cache locally if enabled
                    if self.config.cache_remote_blocks {
                        self.cache_block(&block).await;
                    }

                    return Ok(Some(block));
                }
                Err(e) => {
                    warn!(
                        %peer_id,
                        error = %e,
                        attempt = attempt + 1,
                        "Failed to fetch from provider"
                    );
                    last_error = Some(e);

                    // Brief delay before trying next provider
                    if attempt + 1 < max_attempts {
                        tokio::time::sleep(self.config.retry_delay).await;
                    }
                }
            }
        }

        // All attempts failed
        if let Some(e) = last_error {
            // Check if it was a definitive "not found" from all providers
            if e.to_string().contains("not found") {
                return Ok(None);
            }
            Err(ContentProviderError::ProvidersExhausted {
                cid: cid.clone(),
                attempts: max_attempts,
            })
        } else {
            Ok(None)
        }
    }

    /// Cache a block in local storage.
    #[instrument(skip(self, block), fields(cid = %block.cid()))]
    async fn cache_block(&self, block: &Block) {
        let store = self.local.block_store().clone();
        let block = block.clone();

        let result = tokio::task::spawn_blocking(move || store.put(&block)).await;

        match result {
            Ok(Ok(_)) => {
                debug!("Block cached locally");
            }
            Ok(Err(e)) => {
                // Log but don't fail - caching is best-effort
                warn!(error = %e, "Failed to cache block locally");
            }
            Err(e) => {
                warn!(error = %e, "spawn_blocking failed for cache operation");
            }
        }
    }

    /// Check if any providers exist for a CID in the DHT.
    #[instrument(skip(self), fields(cid = %cid))]
    async fn has_providers(&self, cid: &ContentId) -> Result<bool, ContentProviderError> {
        let discovery_result = self.network.get_providers(cid.clone()).await.map_err(|e| {
            ContentProviderError::Network(format!("Provider discovery failed: {}", e))
        })?;

        Ok(!discovery_result.providers.is_empty())
    }

    /// Fetch a single block from the network (standalone helper for parallel fetching).
    ///
    /// Returns `None` if the block couldn't be fetched (no providers, all attempts failed, etc.)
    /// instead of returning an error, to allow partial success in batch operations.
    async fn fetch_single_with_config(
        network: &Arc<NetworkManager>,
        cid: &ContentId,
        config: &NetworkProviderConfig,
    ) -> Option<Block> {
        // Query DHT for providers
        let discovery_result = match network.get_providers(cid.clone()).await {
            Ok(result) => result,
            Err(e) => {
                debug!(%cid, error = %e, "DHT provider discovery failed");
                return None;
            }
        };

        if discovery_result.providers.is_empty() {
            debug!(%cid, "No providers found");
            return None;
        }

        debug!(
            %cid,
            provider_count = discovery_result.providers.len(),
            "Found providers, attempting fetch"
        );

        // Try providers
        let max_attempts = config
            .max_provider_attempts
            .min(discovery_result.providers.len());

        for (attempt, peer_id) in discovery_result
            .providers
            .iter()
            .take(max_attempts)
            .enumerate()
        {
            match network.request_block(*peer_id, cid.clone()).await {
                Ok(block) => {
                    debug!(%cid, %peer_id, "Successfully fetched block");
                    return Some(block);
                }
                Err(e) => {
                    debug!(
                        %cid,
                        %peer_id,
                        error = %e,
                        attempt = attempt + 1,
                        max_attempts,
                        "Failed to fetch from provider"
                    );

                    // Brief delay before next attempt
                    if attempt + 1 < max_attempts {
                        tokio::time::sleep(config.retry_delay).await;
                    }
                }
            }
        }

        debug!(%cid, "All provider attempts exhausted");
        None
    }

    /// Cache multiple blocks in local storage.
    #[instrument(skip(self, blocks), fields(count = blocks.len()))]
    async fn cache_blocks(&self, blocks: &[Block]) {
        if blocks.is_empty() {
            return;
        }

        let store = self.local.block_store().clone();
        let blocks = blocks.to_vec();
        let blocks_length = blocks.len();

        let result = tokio::task::spawn_blocking(move || {
            let mut success_count = 0;
            for block in &blocks {
                match store.put(block) {
                    Ok(_) => success_count += 1,
                    Err(e) => {
                        warn!(cid = %block.cid(), error = %e, "Failed to cache block");
                    }
                }
            }
            success_count
        })
        .await;

        match result {
            Ok(count) => {
                debug!(
                    cached = count,
                    total = blocks_length,
                    "Batch cache complete"
                );
            }
            Err(e) => {
                warn!(error = %e, "spawn_blocking failed for batch cache");
            }
        }
    }
}

#[async_trait]
impl ContentProvider for NetworkProvider {
    /// Retrieve a block by CID, checking local storage first, then network.
    ///
    /// # Strategy
    /// 1. Check local BlockStore
    /// 2. If not found, query DHT for providers
    /// 3. Fetch from providers, trying up to `max_provider_attempts`
    /// 4. Cache fetched block locally if `cache_remote_blocks` is enabled
    #[instrument(skip(self), fields(cid = %cid))]
    async fn get_block(&self, cid: &ContentId) -> Result<Option<Block>, ContentProviderError> {
        // Step 1: Try local first
        debug!("Checking local store");
        match self.local.get_block(cid).await? {
            Some(block) => {
                debug!("Block found locally");
                return Ok(Some(block));
            }
            None => {
                debug!("Block not found locally, trying network");
            }
        }

        // Step 2: Check if network manager is running
        if !self.network.is_running().await {
            debug!("Network manager not running, cannot fetch from network");
            return Ok(None);
        }

        // Step 3: Fetch from network
        self.fetch_from_network(cid).await
    }

    /// Check if a block exists locally, optionally checking network.
    ///
    /// By default, only checks local storage. Set `check_network_for_existence`
    /// in config to also query DHT (slower but more comprehensive).
    #[instrument(skip(self), fields(cid = %cid))]
    async fn has_block(&self, cid: &ContentId) -> Result<bool, ContentProviderError> {
        // Always check local first
        if self.local.has_block(cid).await? {
            return Ok(true);
        }

        // Optionally check network
        if self.config.check_network_for_existence {
            if !self.network.is_running().await {
                return Ok(false);
            }

            debug!("Block not found locally, checking network for providers");
            return self.has_providers(cid).await;
        }

        Ok(false)
    }

    /// Retrieve multiple blocks, checking local storage first then fetching
    /// missing blocks from the network in parallel.
    ///
    /// # Strategy
    /// 1. Batch-check local BlockStore for all CIDs
    /// 2. For CIDs not found locally, fetch from network in parallel
    /// 3. Cache all fetched blocks locally if caching is enabled
    /// 4. Return results in same order as input CIDs
    #[instrument(skip(self), fields(count = cids.len()))]
    async fn get_blocks(
        &self,
        cids: &[ContentId],
    ) -> Result<Vec<Option<Block>>, ContentProviderError> {
        if cids.is_empty() {
            debug!("Empty batch request");
            return Ok(vec![]);
        }

        // Step 1: Batch check local storage
        debug!(batch_size = cids.len(), "Checking local store for batch");
        let mut results = self.local.get_blocks(cids).await?;

        // Collect indices and CIDs of missing blocks
        let missing: Vec<(usize, ContentId)> = results
            .iter()
            .enumerate()
            .filter_map(|(idx, opt)| {
                if opt.is_none() {
                    Some((idx, cids[idx].clone()))
                } else {
                    None
                }
            })
            .collect();
        let missing_length = missing.len();

        let found_locally = cids.len() - missing_length;
        debug!(
            batch_size = cids.len(),
            found_locally,
            missing = missing_length,
            "Local batch check complete"
        );

        // Return early if all blocks found locally
        if missing.is_empty() {
            info!(batch_size = cids.len(), "All blocks found locally");
            return Ok(results);
        }

        // Step 2: Check if network manager is running
        if !self.network.is_running().await {
            debug!("Network manager not running, cannot fetch missing blocks");
            return Ok(results);
        }

        // Step 3: Fetch missing blocks from network in parallel
        info!(
            missing_count = missing.len(),
            max_parallel = self.config.max_parallel_fetch,
            "Fetching missing blocks from network"
        );

        let fetch_results: Vec<(usize, Option<Block>)> = stream::iter(missing)
            .map(|(idx, cid)| {
                let network = self.network.clone();
                let config = self.config.clone();
                async move {
                    let block = Self::fetch_single_with_config(&network, &cid, &config).await;
                    (idx, block)
                }
            })
            .buffer_unordered(self.config.max_parallel_fetch)
            .collect()
            .await;

        // Step 4: Process results and cache fetched blocks
        let mut fetched_count = 0;
        let mut blocks_to_cache = Vec::new();

        for (idx, block_opt) in fetch_results {
            if let Some(block) = block_opt {
                fetched_count += 1;
                if self.config.cache_remote_blocks {
                    blocks_to_cache.push(block.clone());
                }
                results[idx] = Some(block);
            }
        }

        // Batch cache all fetched blocks
        if !blocks_to_cache.is_empty() {
            debug!(
                cache_count = blocks_to_cache.len(),
                "Caching fetched blocks"
            );
            self.cache_blocks(&blocks_to_cache).await;
        }

        info!(
            batch_size = cids.len(),
            found_locally,
            fetched_from_network = fetched_count,
            still_missing = missing_length - fetched_count,
            "Batch retrieval complete"
        );

        Ok(results)
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

    // ========================================================================
    // NetworkProviderConfig Tests
    // ========================================================================

    #[test]
    fn test_network_provider_config_default() {
        let config = NetworkProviderConfig::default();
        assert_eq!(config.max_provider_attempts, 3);
        assert!(config.cache_remote_blocks);
        assert!(!config.check_network_for_existence);
        assert_eq!(config.request_timeout, Duration::from_secs(30));
        assert_eq!(config.retry_delay, Duration::from_millis(100));
    }

    #[test]
    fn test_network_provider_config_without_caching() {
        let config = NetworkProviderConfig::without_caching();
        assert!(!config.cache_remote_blocks);
    }

    #[test]
    fn test_network_provider_config_from_env() {
        // Just test that it doesn't panic
        let _config = NetworkProviderConfig::from_env();
    }

    // ========================================================================
    // NetworkProvider Unit Tests (without actual network)
    // ========================================================================

    // Note: Full NetworkProvider tests require a running NetworkManager,
    // which is tested in integration tests. These tests verify the
    // local-only path and configuration.

    #[tokio::test]
    async fn test_content_provider_error_variants() {
        let cid = ContentId::from_bytes(b"test");

        let not_found = ContentProviderError::NotFound(cid.clone());
        assert!(not_found.to_string().contains("not found"));

        let storage = ContentProviderError::Storage("disk full".to_string());
        assert!(storage.to_string().contains("disk full"));

        let network = ContentProviderError::Network("timeout".to_string());
        assert!(network.to_string().contains("timeout"));

        let exhausted = ContentProviderError::ProvidersExhausted {
            cid: cid.clone(),
            attempts: 3,
        };
        assert!(exhausted.to_string().contains("3"));
    }

    #[tokio::test]
    async fn test_local_provider_get_blocks_empty() {
        let (store, _dir) = create_test_block_store();
        let provider = LocalProvider::new(Arc::new(store));

        let results = provider.get_blocks(&[]).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_local_provider_get_blocks_all_found() {
        let (store, _dir) = create_test_block_store();

        // Store blocks
        let block1 = Block::new(b"data one".to_vec());
        let block2 = Block::new(b"data two".to_vec());
        let cid1 = store.put(&block1).unwrap();
        let cid2 = store.put(&block2).unwrap();

        let provider = LocalProvider::new(Arc::new(store));
        let results = provider.get_blocks(&[cid1, cid2]).await.unwrap();

        assert_eq!(results.len(), 2);
        assert!(results[0].is_some());
        assert!(results[1].is_some());
        assert_eq!(results[0].as_ref().unwrap().data(), b"data one");
        assert_eq!(results[1].as_ref().unwrap().data(), b"data two");
    }

    #[tokio::test]
    async fn test_local_provider_get_blocks_mixed() {
        let (store, _dir) = create_test_block_store();

        let block = Block::new(b"exists".to_vec());
        let existing_cid = store.put(&block).unwrap();
        let missing_cid = ContentId::from_bytes(b"missing");

        let provider = LocalProvider::new(Arc::new(store));
        let results = provider
            .get_blocks(&[existing_cid, missing_cid])
            .await
            .unwrap();

        assert_eq!(results.len(), 2);
        assert!(results[0].is_some());
        assert!(results[1].is_none());
    }

    #[tokio::test]
    async fn test_local_provider_get_blocks_single() {
        let (store, _dir) = create_test_block_store();

        let block = Block::new(b"single block".to_vec());
        let cid = store.put(&block).unwrap();

        let provider = LocalProvider::new(Arc::new(store));
        let results = provider.get_blocks(&[cid]).await.unwrap();

        assert_eq!(results.len(), 1);
        assert!(results[0].is_some());
        assert_eq!(results[0].as_ref().unwrap().data(), b"single block");
    }

    #[tokio::test]
    async fn test_local_provider_get_blocks_large_batch() {
        let (store, _dir) = create_test_block_store();

        // Create 100 blocks
        let blocks: Vec<Block> = (0..100)
            .map(|i| Block::new(format!("block_{:03}", i).into_bytes()))
            .collect();

        let cids: Vec<ContentId> = blocks.iter().map(|b| store.put(b).unwrap()).collect();

        let provider = LocalProvider::new(Arc::new(store));
        let results = provider.get_blocks(&cids).await.unwrap();

        assert_eq!(results.len(), 100);
        for (i, result) in results.iter().enumerate() {
            let expected = format!("block_{:03}", i);
            assert_eq!(result.as_ref().unwrap().data(), expected.as_bytes());
        }
    }

    // Test default trait implementation
    #[tokio::test]
    async fn test_default_get_blocks_implementation() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        // Create a mock provider that tracks call count
        struct MockProvider {
            call_count: AtomicUsize,
        }

        #[async_trait]
        impl ContentProvider for MockProvider {
            async fn get_block(
                &self,
                _cid: &ContentId,
            ) -> Result<Option<Block>, ContentProviderError> {
                self.call_count.fetch_add(1, Ordering::SeqCst);
                Ok(Some(Block::new(b"mock".to_vec())))
            }

            async fn has_block(&self, _cid: &ContentId) -> Result<bool, ContentProviderError> {
                Ok(true)
            }
            // Uses default get_blocks implementation
        }

        let provider = MockProvider {
            call_count: AtomicUsize::new(0),
        };

        let cids = vec![
            ContentId::from_bytes(b"a"),
            ContentId::from_bytes(b"b"),
            ContentId::from_bytes(b"c"),
        ];

        let results = provider.get_blocks(&cids).await.unwrap();

        // Default implementation calls get_block for each CID
        assert_eq!(provider.call_count.load(Ordering::SeqCst), 3);
        assert_eq!(results.len(), 3);
    }
}
