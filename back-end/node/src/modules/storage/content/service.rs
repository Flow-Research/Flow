//! High-level content service for publishing and fetching content-addressed data.
//!
//! `ContentService` provides a unified API that orchestrates chunking, DAG building,
//! storage, and network announcements.
//!
//! # Example
//!
//! ```ignore
//! let service = ContentService::new(block_store, network_manager, config);
//!
//! // Publish content
//! let result = service.publish(b"Hello, world!", metadata).await?;
//! println!("Published: {}", result.root_cid);
//!
//! // Fetch content
//! let content = service.fetch(&result.root_cid).await?;
//! ```

use std::path::Path;
use std::sync::Arc;

use thiserror::Error;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tracing::{debug, error, info, instrument, warn};

use crate::modules::network::manager::NetworkManager;

use super::{
    BlockStore, BlockStoreError, ChunkingAlgorithm, ChunkingConfig, ContentId,
    ContentProviderError, DagBuilder, DagError, DagMetadata, DagReader, DagReaderConfig,
    DocumentManifest, LocalProvider, NetworkProvider,
};

// ============================================================================
// Announcement Types
// ============================================================================

/// Result of an individual announcement attempt.
#[derive(Debug, Clone)]
pub enum AnnouncementOutcome {
    /// Announcement succeeded, contains identifier/reference
    Success(String),
    /// Announcement failed, contains error message
    Failed(String),
    /// Announcement was disabled in config
    Skipped,
    /// Could not attempt (e.g., network not running)
    NotAttempted,
}

impl Default for AnnouncementOutcome {
    fn default() -> Self {
        AnnouncementOutcome::NotAttempted
    }
}

impl AnnouncementOutcome {
    /// Check if this outcome represents success.
    pub fn is_success(&self) -> bool {
        matches!(self, AnnouncementOutcome::Success(_))
    }

    /// Check if this outcome represents failure.
    pub fn is_failed(&self) -> bool {
        matches!(self, AnnouncementOutcome::Failed(_))
    }

    /// Get the success identifier, if any.
    pub fn success_id(&self) -> Option<&str> {
        match self {
            AnnouncementOutcome::Success(id) => Some(id),
            _ => None,
        }
    }

    /// Get the error message, if any.
    pub fn error_message(&self) -> Option<&str> {
        match self {
            AnnouncementOutcome::Failed(msg) => Some(msg),
            _ => None,
        }
    }
}

/// Results of all announcement attempts.
///
/// This struct is extensible - new announcement methods can be added as fields
/// without breaking existing code.
#[derive(Debug, Clone, Default)]
pub struct AnnouncementResult {
    /// DHT provider announcement result
    pub dht: AnnouncementOutcome,
    /// GossipSub broadcast result
    pub gossipsub: AnnouncementOutcome,
    // Future extensibility:
    // pub relay: AnnouncementOutcome,
    // pub webhook: AnnouncementOutcome,
}

impl AnnouncementResult {
    /// Check if at least one announcement succeeded.
    pub fn any_succeeded(&self) -> bool {
        self.dht.is_success() || self.gossipsub.is_success()
    }

    /// Check if all attempted announcements succeeded (ignores skipped/not attempted).
    pub fn all_attempted_succeeded(&self) -> bool {
        !self.dht.is_failed() && !self.gossipsub.is_failed()
    }

    /// Check if everything was skipped or not attempted.
    pub fn is_empty(&self) -> bool {
        matches!(
            self.dht,
            AnnouncementOutcome::Skipped | AnnouncementOutcome::NotAttempted
        ) && matches!(
            self.gossipsub,
            AnnouncementOutcome::Skipped | AnnouncementOutcome::NotAttempted
        )
    }

    /// Get count of successful announcements.
    pub fn success_count(&self) -> usize {
        [self.dht.is_success(), self.gossipsub.is_success()]
            .iter()
            .filter(|&&v| v)
            .count()
    }

    /// Get count of failed announcements.
    pub fn failure_count(&self) -> usize {
        [self.dht.is_failed(), self.gossipsub.is_failed()]
            .iter()
            .filter(|&&v| v)
            .count()
    }
}

// ============================================================================
// Configuration
// ============================================================================

/// Configuration for ContentService behavior.
#[derive(Debug, Clone)]
pub struct ContentServiceConfig {
    /// Chunking algorithm to use (default: FastCDC)
    pub chunking_algorithm: ChunkingAlgorithm,
    /// Chunk size configuration
    pub chunking_config: ChunkingConfig,
    /// Whether to announce to DHT after publishing (default: true)
    pub announce_dht: bool,
    /// Whether to broadcast via GossipSub after publishing (default: true)
    pub announce_gossip: bool,
    /// Batch size for DagReader operations (default: 50)
    pub reader_batch_size: usize,
}

impl Default for ContentServiceConfig {
    fn default() -> Self {
        Self {
            chunking_algorithm: ChunkingAlgorithm::FastCdc,
            chunking_config: ChunkingConfig::default(),
            announce_dht: true,
            announce_gossip: true,
            reader_batch_size: 50,
        }
    }
}

impl ContentServiceConfig {
    /// Load configuration from environment variables.
    ///
    /// Environment variables:
    /// - `CONTENT_SERVICE_CHUNKING_ALGORITHM`: "fastcdc", "rabin", or "fixed"
    /// - `CONTENT_SERVICE_ANNOUNCE_DHT`: "true" or "false"
    /// - `CONTENT_SERVICE_ANNOUNCE_GOSSIP`: "true" or "false"
    /// - `CONTENT_SERVICE_READER_BATCH_SIZE`: batch size for reads
    /// - Plus all `CHUNKING_*` variables for chunk sizes
    pub fn from_env() -> Self {
        use std::env;

        let algorithm = env::var("CONTENT_SERVICE_CHUNKING_ALGORITHM")
            .ok()
            .and_then(|s| match s.to_lowercase().as_str() {
                "fastcdc" => Some(ChunkingAlgorithm::FastCdc),
                "rabin" => Some(ChunkingAlgorithm::Rabin),
                "fixed" => Some(ChunkingAlgorithm::Fixed),
                _ => None,
            })
            .unwrap_or(ChunkingAlgorithm::FastCdc);

        Self {
            chunking_algorithm: algorithm,
            chunking_config: ChunkingConfig::from_env(),
            announce_dht: env::var("CONTENT_SERVICE_ANNOUNCE_DHT")
                .map(|s| s != "false" && s != "0")
                .unwrap_or(true),
            announce_gossip: env::var("CONTENT_SERVICE_ANNOUNCE_GOSSIP")
                .map(|s| s != "false" && s != "0")
                .unwrap_or(true),
            reader_batch_size: env::var("CONTENT_SERVICE_READER_BATCH_SIZE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(50),
        }
    }

    /// Create config with announcements disabled (for testing/private content).
    pub fn local_only() -> Self {
        Self {
            announce_dht: false,
            announce_gossip: false,
            ..Default::default()
        }
    }

    /// Builder method to set chunking algorithm.
    pub fn with_algorithm(mut self, algorithm: ChunkingAlgorithm) -> Self {
        self.chunking_algorithm = algorithm;
        self
    }

    /// Builder method to disable DHT announcements.
    pub fn without_dht(mut self) -> Self {
        self.announce_dht = false;
        self
    }

    /// Builder method to disable GossipSub announcements.
    pub fn without_gossip(mut self) -> Self {
        self.announce_gossip = false;
        self
    }
}

// ============================================================================
// Result Types
// ============================================================================

/// Result of a successful publish operation.
#[derive(Debug, Clone)]
pub struct PublishResult {
    /// Root CID of the published content
    pub root_cid: ContentId,
    /// Document manifest with metadata
    pub manifest: DocumentManifest,
    /// Publishing statistics
    pub stats: PublishStats,
    /// Announcement outcomes
    pub announcement: AnnouncementResult,
}

/// Statistics from a publish operation.
#[derive(Debug, Clone, Default)]
pub struct PublishStats {
    /// Number of data chunks created
    pub chunk_count: usize,
    /// Total bytes of original content
    pub content_size: u64,
}

// ============================================================================
// Errors
// ============================================================================

/// Errors that can occur in ContentService operations.
#[derive(Debug, Error)]
pub enum ContentServiceError {
    /// Storage layer error
    #[error("Storage error: {0}")]
    Storage(#[from] BlockStoreError),

    /// DAG building/reading error
    #[error("DAG error: {0}")]
    Dag(#[from] DagError),

    /// Content provider error
    #[error("Provider error: {0}")]
    Provider(#[from] ContentProviderError),

    /// Network operation error
    #[error("Network error: {0}")]
    Network(String),

    /// Content not found locally or on network
    #[error("Content not found: {0}")]
    NotFound(ContentId),

    /// File I/O error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Invalid argument provided
    #[error("Invalid argument: {0}")]
    InvalidArgument(String),
}

impl From<errors::AppError> for ContentServiceError {
    fn from(err: errors::AppError) -> Self {
        ContentServiceError::Network(err.to_string())
    }
}

/// Result type alias for ContentService operations.
pub type ContentServiceResult<T> = Result<T, ContentServiceError>;

// ============================================================================
// ContentService
// ============================================================================

/// High-level service for publishing and fetching content-addressed data.
///
/// Orchestrates chunking, DAG building, local storage, and network operations
/// through a unified API.
pub struct ContentService {
    block_store: Arc<BlockStore>,
    network: Arc<NetworkManager>,
    config: ContentServiceConfig,
}

impl std::fmt::Debug for ContentService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContentService")
            .field("block_store", &"<BlockStore>")
            .field(
                "network",
                &format!("NetworkManager({})", self.network.local_peer_id()),
            )
            .field("config", &self.config)
            .finish()
    }
}

impl ContentService {
    /// Create a new ContentService.
    ///
    /// # Arguments
    /// * `block_store` - Local block storage
    /// * `network` - Network manager for DHT/GossipSub operations
    /// * `config` - Service configuration
    pub fn new(
        block_store: Arc<BlockStore>,
        network: Arc<NetworkManager>,
        config: ContentServiceConfig,
    ) -> Self {
        info!(
            algorithm = ?config.chunking_algorithm,
            announce_dht = config.announce_dht,
            announce_gossip = config.announce_gossip,
            "Creating ContentService"
        );

        Self {
            block_store,
            network,
            config,
        }
    }

    /// Create a ContentService with default configuration.
    pub fn with_defaults(block_store: Arc<BlockStore>, network: Arc<NetworkManager>) -> Self {
        Self::new(block_store, network, ContentServiceConfig::default())
    }

    /// Get reference to the block store.
    pub fn block_store(&self) -> &Arc<BlockStore> {
        &self.block_store
    }

    /// Get reference to the network manager.
    pub fn network(&self) -> &Arc<NetworkManager> {
        &self.network
    }

    /// Get reference to the configuration.
    pub fn config(&self) -> &ContentServiceConfig {
        &self.config
    }

    // ========================================================================
    // Publish Operations
    // ========================================================================

    /// Publish content from bytes.
    ///
    /// Chunks the data, builds a DAG, stores blocks locally, and announces
    /// to the network (DHT + GossipSub) based on configuration.
    ///
    /// # Arguments
    /// * `data` - Content bytes to publish
    /// * `metadata` - Metadata about the content (name, type, publisher DID)
    ///
    /// # Returns
    /// `PublishResult` containing the root CID, manifest, stats, and announcement outcomes.
    #[instrument(skip(self, data), fields(size = data.len(), publisher = %metadata.publisher_did))]
    pub async fn publish(
        &self,
        data: &[u8],
        metadata: DagMetadata,
    ) -> ContentServiceResult<PublishResult> {
        info!(
            size = data.len(),
            name = ?metadata.name,
            "Publishing content"
        );

        // Step 1: Build DAG (chunk + store)
        let chunker = self
            .config
            .chunking_algorithm
            .create_chunker(self.config.chunking_config);
        let dag_builder = DagBuilder::new(self.block_store.clone(), chunker);

        let build_result = dag_builder.build_from_bytes(data, metadata.clone())?;

        debug!(
            root_cid = %build_result.root_cid,
            chunks = build_result.chunks_stored,
            "DAG built successfully"
        );

        // Step 2: Announce to network
        let announcement = self
            .announce(&build_result.root_cid, &build_result.manifest)
            .await;

        let stats = PublishStats {
            chunk_count: build_result.chunks_stored,
            content_size: build_result.total_bytes,
        };

        info!(
            root_cid = %build_result.root_cid,
            chunks = stats.chunk_count,
            dht_success = announcement.dht.is_success(),
            gossip_success = announcement.gossipsub.is_success(),
            "Content published"
        );

        Ok(PublishResult {
            root_cid: build_result.root_cid,
            manifest: build_result.manifest,
            stats,
            announcement,
        })
    }

    /// Publish content from a file.
    ///
    /// Reads the file and publishes its contents. The filename is used as
    /// the default name if not specified in metadata.
    ///
    /// # Arguments
    /// * `path` - Path to the file to publish
    /// * `metadata` - Metadata (name will be inferred from path if not set)
    #[instrument(skip(self), fields(path = %path.as_ref().display()))]
    pub async fn publish_file(
        &self,
        path: impl AsRef<Path>,
        metadata: DagMetadata,
    ) -> ContentServiceResult<PublishResult> {
        let path = path.as_ref();

        if !path.exists() {
            return Err(ContentServiceError::InvalidArgument(format!(
                "File not found: {}",
                path.display()
            )));
        }

        info!(path = %path.display(), "Publishing file");

        let data = fs::read(path).await?;

        // Use filename as name if not specified
        let metadata = if metadata.name.is_none() {
            metadata.name(
                path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unnamed"),
            )
        } else {
            metadata
        };

        self.publish(&data, metadata).await
    }

    /// Announce content to the network.
    async fn announce(&self, cid: &ContentId, manifest: &DocumentManifest) -> AnnouncementResult {
        let mut result = AnnouncementResult::default();

        // Check if network is running
        if !self.network.is_running().await {
            warn!("Network not running, skipping announcements");
            result.dht = AnnouncementOutcome::NotAttempted;
            result.gossipsub = AnnouncementOutcome::NotAttempted;
            return result;
        }

        // DHT announcement
        if self.config.announce_dht {
            match self.network.start_providing(cid.clone()).await {
                Ok(provider_result) => {
                    let query_id = format!("{:?}", provider_result.query_id);
                    debug!(query_id = %query_id, "DHT announcement initiated");
                    result.dht = AnnouncementOutcome::Success(query_id);
                }
                Err(e) => {
                    warn!(error = %e, "DHT announcement failed");
                    result.dht = AnnouncementOutcome::Failed(e.to_string());
                }
            }
        } else {
            result.dht = AnnouncementOutcome::Skipped;
        }

        // GossipSub announcement
        if self.config.announce_gossip {
            let name = manifest
                .name
                .clone()
                .unwrap_or_else(|| "unnamed".to_string());
            let content_type = manifest
                .content_type
                .clone()
                .unwrap_or_else(|| "application/octet-stream".to_string());

            match self
                .network
                .announce_content_published(cid.clone(), name, manifest.total_size, content_type)
                .await
            {
                Ok(msg_id) => {
                    debug!(msg_id = %msg_id, "GossipSub announcement sent");
                    result.gossipsub = AnnouncementOutcome::Success(msg_id);
                }
                Err(e) => {
                    warn!(error = %e, "GossipSub announcement failed");
                    result.gossipsub = AnnouncementOutcome::Failed(e.to_string());
                }
            }
        } else {
            result.gossipsub = AnnouncementOutcome::Skipped;
        }

        result
    }

    // ========================================================================
    // Fetch Operations
    // ========================================================================

    /// Fetch content by CID.
    ///
    /// First checks local storage, then queries the network if not found locally.
    /// Uses batch fetching for efficient multi-chunk retrieval.
    ///
    /// # Arguments
    /// * `cid` - Content identifier to fetch
    ///
    /// # Returns
    /// The complete content bytes.
    #[instrument(skip(self), fields(cid = %cid))]
    pub async fn fetch(&self, cid: &ContentId) -> ContentServiceResult<Vec<u8>> {
        info!("Fetching content");

        let provider = self.create_network_provider();
        let reader_config = DagReaderConfig::with_batch_size(self.config.reader_batch_size);
        let reader = DagReader::with(provider, reader_config);

        let content = reader.read_all(cid).await?;

        info!(size = content.len(), "Content fetched");
        Ok(content)
    }

    /// Fetch content and write to a file.
    ///
    /// # Arguments
    /// * `cid` - Content identifier to fetch
    /// * `path` - Destination file path
    #[instrument(skip(self), fields(cid = %cid, path = %path.as_ref().display()))]
    pub async fn fetch_to_file(
        &self,
        cid: &ContentId,
        path: impl AsRef<Path>,
    ) -> ContentServiceResult<()> {
        let path = path.as_ref();
        info!("Fetching content to file");

        let content = self.fetch(cid).await?;

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }

        let mut file = fs::File::create(path).await?;
        file.write_all(&content).await?;
        file.flush().await?;

        info!(bytes = content.len(), "Content written to file");
        Ok(())
    }

    // ========================================================================
    // Info Operations
    // ========================================================================

    /// Get document manifest/metadata for a CID.
    ///
    /// # Arguments
    /// * `cid` - Content identifier
    ///
    /// # Returns
    /// The `DocumentManifest` containing metadata and chunk structure.
    #[instrument(skip(self), fields(cid = %cid))]
    pub async fn get_info(&self, cid: &ContentId) -> ContentServiceResult<DocumentManifest> {
        debug!("Getting content info");

        let provider = self.create_network_provider();
        let reader = DagReader::with_provider(provider);

        let manifest = reader.read_manifest(cid).await?;

        debug!(
            name = ?manifest.name,
            size = manifest.total_size,
            chunks = manifest.chunk_count(),
            "Content info retrieved"
        );

        Ok(manifest)
    }

    /// Check if content is available locally.
    ///
    /// # Arguments
    /// * `cid` - Content identifier to check
    ///
    /// # Returns
    /// `true` if the manifest block exists locally.
    #[instrument(skip(self), fields(cid = %cid))]
    pub fn is_local(&self, cid: &ContentId) -> bool {
        self.block_store.has(cid).unwrap_or(false)
    }

    /// List all locally stored content root CIDs.
    ///
    /// Returns CIDs of blocks that appear to be document manifests
    /// (DAG-CBOR codec that parses successfully as DocumentManifest).
    ///
    /// # Note
    /// This iterates through all blocks and filters for manifests,
    /// which may be slow for very large stores.
    #[instrument(skip(self))]
    pub fn list_local(&self) -> ContentServiceResult<Vec<ContentId>> {
        debug!("Listing local content");

        let mut manifests = Vec::new();

        let iter = self.block_store.iter()?;
        for result in iter {
            let block = result?;

            // Check if this looks like a manifest (DAG-CBOR codec)
            if block.cid().is_dag_cbor() {
                // Try to parse as manifest
                if DocumentManifest::from_dag_cbor(block.data()).is_ok() {
                    manifests.push(block.cid().clone());
                }
            }
        }

        info!(count = manifests.len(), "Found local manifests");
        Ok(manifests)
    }

    // ========================================================================
    // Internal Helpers
    // ========================================================================

    /// Create a NetworkProvider for fetch operations.
    fn create_network_provider(&self) -> NetworkProvider {
        let local = LocalProvider::new(self.block_store.clone());
        NetworkProvider::with_defaults(local, self.network.clone())
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // AnnouncementOutcome Tests
    // ========================================================================

    #[test]
    fn test_announcement_outcome_success() {
        let outcome = AnnouncementOutcome::Success("query-123".to_string());
        assert!(outcome.is_success());
        assert!(!outcome.is_failed());
        assert_eq!(outcome.success_id(), Some("query-123"));
        assert!(outcome.error_message().is_none());
    }

    #[test]
    fn test_announcement_outcome_failed() {
        let outcome = AnnouncementOutcome::Failed("connection timeout".to_string());
        assert!(!outcome.is_success());
        assert!(outcome.is_failed());
        assert!(outcome.success_id().is_none());
        assert_eq!(outcome.error_message(), Some("connection timeout"));
    }

    #[test]
    fn test_announcement_outcome_skipped() {
        let outcome = AnnouncementOutcome::Skipped;
        assert!(!outcome.is_success());
        assert!(!outcome.is_failed());
    }

    #[test]
    fn test_announcement_outcome_default() {
        let outcome = AnnouncementOutcome::default();
        assert!(matches!(outcome, AnnouncementOutcome::NotAttempted));
    }

    // ========================================================================
    // AnnouncementResult Tests
    // ========================================================================

    #[test]
    fn test_announcement_result_any_succeeded() {
        let result = AnnouncementResult {
            dht: AnnouncementOutcome::Success("q1".into()),
            gossipsub: AnnouncementOutcome::Failed("err".into()),
        };
        assert!(result.any_succeeded());

        let result = AnnouncementResult {
            dht: AnnouncementOutcome::Failed("err".into()),
            gossipsub: AnnouncementOutcome::Success("m1".into()),
        };
        assert!(result.any_succeeded());

        let result = AnnouncementResult {
            dht: AnnouncementOutcome::Failed("e1".into()),
            gossipsub: AnnouncementOutcome::Failed("e2".into()),
        };
        assert!(!result.any_succeeded());
    }

    #[test]
    fn test_announcement_result_all_attempted_succeeded() {
        let result = AnnouncementResult {
            dht: AnnouncementOutcome::Success("q1".into()),
            gossipsub: AnnouncementOutcome::Success("m1".into()),
        };
        assert!(result.all_attempted_succeeded());

        let result = AnnouncementResult {
            dht: AnnouncementOutcome::Success("q1".into()),
            gossipsub: AnnouncementOutcome::Skipped,
        };
        assert!(result.all_attempted_succeeded());

        let result = AnnouncementResult {
            dht: AnnouncementOutcome::Success("q1".into()),
            gossipsub: AnnouncementOutcome::Failed("err".into()),
        };
        assert!(!result.all_attempted_succeeded());
    }

    #[test]
    fn test_announcement_result_is_empty() {
        let result = AnnouncementResult::default();
        assert!(result.is_empty());

        let result = AnnouncementResult {
            dht: AnnouncementOutcome::Skipped,
            gossipsub: AnnouncementOutcome::Skipped,
        };
        assert!(result.is_empty());

        let result = AnnouncementResult {
            dht: AnnouncementOutcome::Success("q1".into()),
            gossipsub: AnnouncementOutcome::Skipped,
        };
        assert!(!result.is_empty());
    }

    #[test]
    fn test_announcement_result_counts() {
        let result = AnnouncementResult {
            dht: AnnouncementOutcome::Success("q1".into()),
            gossipsub: AnnouncementOutcome::Failed("err".into()),
        };
        assert_eq!(result.success_count(), 1);
        assert_eq!(result.failure_count(), 1);

        let result = AnnouncementResult {
            dht: AnnouncementOutcome::Success("q1".into()),
            gossipsub: AnnouncementOutcome::Success("m1".into()),
        };
        assert_eq!(result.success_count(), 2);
        assert_eq!(result.failure_count(), 0);
    }

    // ========================================================================
    // Config Tests
    // ========================================================================

    #[test]
    fn test_content_service_config_default() {
        let config = ContentServiceConfig::default();
        assert!(matches!(
            config.chunking_algorithm,
            ChunkingAlgorithm::FastCdc
        ));
        assert!(config.announce_dht);
        assert!(config.announce_gossip);
        assert_eq!(config.reader_batch_size, 50);
    }

    #[test]
    fn test_content_service_config_local_only() {
        let config = ContentServiceConfig::local_only();
        assert!(!config.announce_dht);
        assert!(!config.announce_gossip);
    }

    #[test]
    fn test_content_service_config_builders() {
        let config = ContentServiceConfig::default()
            .with_algorithm(ChunkingAlgorithm::Rabin)
            .without_dht()
            .without_gossip();

        assert!(matches!(
            config.chunking_algorithm,
            ChunkingAlgorithm::Rabin
        ));
        assert!(!config.announce_dht);
        assert!(!config.announce_gossip);
    }

    // ========================================================================
    // PublishStats Tests
    // ========================================================================

    #[test]
    fn test_publish_stats_default() {
        let stats = PublishStats::default();
        assert_eq!(stats.chunk_count, 0);
        assert_eq!(stats.content_size, 0);
    }

    // ========================================================================
    // Error Tests
    // ========================================================================

    #[test]
    fn test_content_service_error_display() {
        let err = ContentServiceError::NotFound(ContentId::from_bytes(b"test"));
        assert!(err.to_string().contains("not found"));

        let err = ContentServiceError::InvalidArgument("bad input".into());
        assert!(err.to_string().contains("bad input"));

        let err = ContentServiceError::Network("connection failed".into());
        assert!(err.to_string().contains("connection failed"));
    }
}
