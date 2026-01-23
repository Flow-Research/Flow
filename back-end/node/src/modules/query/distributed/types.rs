//! Core types for distributed search.
//!
//! These types define the request/response format for the distributed search API
//! and internal result tracking.

use serde::{Deserialize, Serialize};

/// Search scope determining which sources to query.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum DistributedSearchScope {
    /// Only local space collections.
    Local,
    /// Only network sources (prefetch + live query).
    Network,
    /// All sources (default).
    #[default]
    All,
}

impl DistributedSearchScope {
    /// Returns true if this scope includes local search.
    pub fn includes_local(&self) -> bool {
        matches!(self, Self::Local | Self::All)
    }

    /// Returns true if this scope includes network search.
    pub fn includes_network(&self) -> bool {
        matches!(self, Self::Network | Self::All)
    }

    /// Convert to string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Network => "network",
            Self::All => "all",
        }
    }
}

/// Default limit for search results.
fn default_limit() -> u32 {
    10
}

/// Incoming search request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributedSearchRequest {
    /// The search query text.
    pub query: String,

    /// Search scope.
    #[serde(default)]
    pub scope: DistributedSearchScope,

    /// Maximum results to return.
    #[serde(default = "default_limit")]
    pub limit: u32,
}

impl DistributedSearchRequest {
    /// Create a new search request with default scope and limit.
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            scope: DistributedSearchScope::default(),
            limit: default_limit(),
        }
    }

    /// Set the search scope.
    pub fn with_scope(mut self, scope: DistributedSearchScope) -> Self {
        self.scope = scope;
        self
    }

    /// Set the result limit.
    pub fn with_limit(mut self, limit: u32) -> Self {
        self.limit = limit;
        self
    }
}

/// Source of a search result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResultSource {
    /// Result from local space collection.
    Local,
    /// Result from network sources (prefetch or live query).
    Network,
}

impl ResultSource {
    /// Returns true if this is a local result.
    pub fn is_local(&self) -> bool {
        matches!(self, Self::Local)
    }

    /// Returns true if this is a network result.
    pub fn is_network(&self) -> bool {
        matches!(self, Self::Network)
    }
}

impl std::fmt::Display for ResultSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Local => write!(f, "local"),
            Self::Network => write!(f, "network"),
        }
    }
}

/// A single search result with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributedSearchResult {
    /// Content identifier.
    pub cid: String,

    /// Original similarity score from source.
    pub score: f32,

    /// Score after normalization, penalty, and boost.
    pub adjusted_score: f32,

    /// Source type (local or network).
    pub source: ResultSource,

    /// Number of sources that found this CID.
    pub sources_count: u32,

    /// Document title (if available).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// Text snippet with matching context.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,

    /// Publisher peer ID (for network results).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publisher_peer_id: Option<String>,
}

impl DistributedSearchResult {
    /// Create a new search result.
    pub fn new(cid: impl Into<String>, score: f32, source: ResultSource) -> Self {
        Self {
            cid: cid.into(),
            score,
            adjusted_score: score,
            source,
            sources_count: 1,
            title: None,
            snippet: None,
            publisher_peer_id: None,
        }
    }

    /// Set the title.
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Set the snippet.
    pub fn with_snippet(mut self, snippet: impl Into<String>) -> Self {
        self.snippet = Some(snippet.into());
        self
    }

    /// Set the publisher peer ID.
    pub fn with_publisher(mut self, peer_id: impl Into<String>) -> Self {
        self.publisher_peer_id = Some(peer_id.into());
        self
    }
}

/// Complete search response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributedSearchResponse {
    /// Whether the search was successful.
    pub success: bool,

    /// The original query.
    pub query: String,

    /// The scope used for the search.
    pub scope: DistributedSearchScope,

    /// Search results sorted by adjusted score.
    pub results: Vec<DistributedSearchResult>,

    /// Count of results from local sources.
    pub local_count: u32,

    /// Count of results from network sources.
    pub network_count: u32,

    /// Number of peers queried (for network scope).
    pub peers_queried: u32,

    /// Number of peers that responded.
    pub peers_responded: u32,

    /// Additional results available beyond limit.
    pub more_available: u32,

    /// Total elapsed time in milliseconds.
    pub elapsed_ms: u64,
}

impl DistributedSearchResponse {
    /// Create a successful response.
    pub fn success(
        query: impl Into<String>,
        scope: DistributedSearchScope,
        results: Vec<DistributedSearchResult>,
    ) -> Self {
        let local_count = results.iter().filter(|r| r.source.is_local()).count() as u32;
        let network_count = results.iter().filter(|r| r.source.is_network()).count() as u32;

        Self {
            success: true,
            query: query.into(),
            scope,
            results,
            local_count,
            network_count,
            peers_queried: 0,
            peers_responded: 0,
            more_available: 0,
            elapsed_ms: 0,
        }
    }

    /// Set network metadata.
    pub fn with_network_stats(mut self, queried: u32, responded: u32) -> Self {
        self.peers_queried = queried;
        self.peers_responded = responded;
        self
    }

    /// Set overflow count.
    pub fn with_overflow(mut self, more_available: u32) -> Self {
        self.more_available = more_available;
        self
    }

    /// Set elapsed time.
    pub fn with_elapsed(mut self, elapsed_ms: u64) -> Self {
        self.elapsed_ms = elapsed_ms;
        self
    }
}

/// Internal result with source tracking before aggregation.
#[derive(Debug, Clone)]
pub struct SourcedResult {
    /// Content identifier.
    pub cid: String,

    /// Raw score from the source.
    pub score: f32,

    /// Source type (local or network).
    pub source: ResultSource,

    /// Unique source identifier (e.g., "local:space-abc-idx" or "network:peer-123").
    pub source_id: String,

    /// Document title (if available).
    pub title: Option<String>,

    /// Text snippet (if available).
    pub snippet: Option<String>,

    /// Publisher peer ID (for network results).
    pub publisher_peer_id: Option<String>,
}

impl SourcedResult {
    /// Create a new sourced result.
    pub fn new(
        cid: impl Into<String>,
        score: f32,
        source: ResultSource,
        source_id: impl Into<String>,
    ) -> Self {
        Self {
            cid: cid.into(),
            score,
            source,
            source_id: source_id.into(),
            title: None,
            snippet: None,
            publisher_peer_id: None,
        }
    }

    /// Create a local result.
    pub fn local(cid: impl Into<String>, score: f32, collection: impl Into<String>) -> Self {
        let collection = collection.into();
        Self::new(cid, score, ResultSource::Local, format!("local:{}", collection))
    }

    /// Create a network result.
    pub fn network(cid: impl Into<String>, score: f32, peer_id: impl Into<String>) -> Self {
        let peer_id = peer_id.into();
        Self {
            cid: cid.into(),
            score,
            source: ResultSource::Network,
            source_id: format!("network:{}", peer_id),
            title: None,
            snippet: None,
            publisher_peer_id: Some(peer_id),
        }
    }

    /// Set the title.
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Set the snippet.
    pub fn with_snippet(mut self, snippet: impl Into<String>) -> Self {
        self.snippet = Some(snippet.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_scope_default() {
        let scope = DistributedSearchScope::default();
        assert_eq!(scope, DistributedSearchScope::All);
    }

    #[test]
    fn test_search_scope_includes() {
        assert!(DistributedSearchScope::Local.includes_local());
        assert!(!DistributedSearchScope::Local.includes_network());

        assert!(!DistributedSearchScope::Network.includes_local());
        assert!(DistributedSearchScope::Network.includes_network());

        assert!(DistributedSearchScope::All.includes_local());
        assert!(DistributedSearchScope::All.includes_network());
    }

    #[test]
    fn test_search_request_builder() {
        let request = DistributedSearchRequest::new("test query")
            .with_scope(DistributedSearchScope::Local)
            .with_limit(20);

        assert_eq!(request.query, "test query");
        assert_eq!(request.scope, DistributedSearchScope::Local);
        assert_eq!(request.limit, 20);
    }

    #[test]
    fn test_search_request_defaults() {
        let request = DistributedSearchRequest::new("test");

        assert_eq!(request.scope, DistributedSearchScope::All);
        assert_eq!(request.limit, 10);
    }

    #[test]
    fn test_result_source() {
        assert!(ResultSource::Local.is_local());
        assert!(!ResultSource::Local.is_network());

        assert!(!ResultSource::Network.is_local());
        assert!(ResultSource::Network.is_network());
    }

    #[test]
    fn test_search_result_builder() {
        let result = DistributedSearchResult::new("bafytest", 0.95, ResultSource::Local)
            .with_title("Test Document")
            .with_snippet("This is a test snippet");

        assert_eq!(result.cid, "bafytest");
        assert_eq!(result.score, 0.95);
        assert_eq!(result.source, ResultSource::Local);
        assert_eq!(result.title.as_deref(), Some("Test Document"));
        assert_eq!(result.snippet.as_deref(), Some("This is a test snippet"));
    }

    #[test]
    fn test_search_response_counts() {
        let results = vec![
            DistributedSearchResult::new("cid1", 0.9, ResultSource::Local),
            DistributedSearchResult::new("cid2", 0.8, ResultSource::Network),
            DistributedSearchResult::new("cid3", 0.7, ResultSource::Local),
            DistributedSearchResult::new("cid4", 0.6, ResultSource::Network),
        ];

        let response =
            DistributedSearchResponse::success("test", DistributedSearchScope::All, results);

        assert_eq!(response.local_count, 2);
        assert_eq!(response.network_count, 2);
        assert_eq!(response.results.len(), 4);
    }

    #[test]
    fn test_sourced_result_local() {
        let result = SourcedResult::local("cid1", 0.9, "space-abc-idx").with_title("Local Doc");

        assert_eq!(result.source, ResultSource::Local);
        assert_eq!(result.source_id, "local:space-abc-idx");
        assert!(result.publisher_peer_id.is_none());
    }

    #[test]
    fn test_sourced_result_network() {
        let result = SourcedResult::network("cid2", 0.85, "12D3KooWTest");

        assert_eq!(result.source, ResultSource::Network);
        assert_eq!(result.source_id, "network:12D3KooWTest");
        assert_eq!(result.publisher_peer_id.as_deref(), Some("12D3KooWTest"));
    }

    #[test]
    fn test_scope_serialization() {
        let scope = DistributedSearchScope::Network;
        let json = serde_json::to_string(&scope).unwrap();
        assert_eq!(json, "\"network\"");

        let parsed: DistributedSearchScope = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, scope);
    }

    #[test]
    fn test_request_serialization() {
        let request = DistributedSearchRequest::new("test query")
            .with_scope(DistributedSearchScope::All)
            .with_limit(15);

        let json = serde_json::to_string(&request).unwrap();
        let parsed: DistributedSearchRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.query, request.query);
        assert_eq!(parsed.scope, request.scope);
        assert_eq!(parsed.limit, request.limit);
    }
}
