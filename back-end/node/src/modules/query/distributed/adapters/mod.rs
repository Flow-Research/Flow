//! Search adapters for different data sources.
//!
//! Adapters wrap underlying search implementations to provide a uniform
//! interface for the SearchOrchestrator.

pub mod local;
pub mod prefetch;

use async_trait::async_trait;

use super::error::SearchResult;
use super::types::SourcedResult;

/// Trait for search adapters.
///
/// Adapters implement this trait to provide search capability over a specific
/// data source (local Qdrant collections, remote-content-idx, etc.).
#[async_trait]
pub trait SearchAdapter: Send + Sync {
    /// Execute a search query and return sourced results.
    ///
    /// # Arguments
    ///
    /// * `query` - The search query text.
    /// * `limit` - Maximum number of results to return.
    ///
    /// # Returns
    ///
    /// Vector of `SourcedResult` items with source tracking information.
    async fn search(&self, query: &str, limit: u32) -> SearchResult<Vec<SourcedResult>>;

    /// Get the adapter name for logging and metrics.
    fn name(&self) -> &'static str;

    /// Check if the adapter is healthy and ready to serve queries.
    async fn health_check(&self) -> SearchResult<()>;
}

pub use local::LocalSearchAdapter;
pub use prefetch::PrefetchSearchAdapter;
