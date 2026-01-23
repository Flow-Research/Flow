//! Distributed Search Module
//!
//! Enables network-wide search by broadcasting queries to connected peers
//! and merging results from local indexes, pre-indexed remote content,
//! and live network responses.
//!
//! # Architecture
//!
//! ```text
//! SearchRouter
//! ├── LocalSearchAdapter (Qdrant space-*-idx)
//! ├── PrefetchSearchAdapter (Qdrant remote-content-idx)
//! └── LiveQueryEngine (GossipSub broadcast)
//!         ↓
//!     ResultAggregator (normalize, penalty, boost, dedup)
//!         ↓
//!     SearchResponse
//! ```

pub mod adapters;
pub mod cache;
pub mod config;
pub mod error;
pub mod live_query;
pub mod messages;
pub mod orchestrator;
pub mod query_handler;
pub mod ranking;
pub mod router;
pub mod types;

// Re-exports for convenient access
pub use config::SearchConfig;
pub use error::DistributedSearchError;
pub use router::SearchRouter;
pub use types::{
    DistributedSearchRequest, DistributedSearchResponse, DistributedSearchResult,
    DistributedSearchScope,
};
