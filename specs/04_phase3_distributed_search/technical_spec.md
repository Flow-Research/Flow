# Technical Specification: Phase 3 - Distributed Search

> **Version**: 1.0
> **Created**: 2026-01-21
> **Status**: Draft
> **Author**: Flow Network Engineering

---

## 1. Overview

### 1.1 Purpose

This document specifies the technical implementation of Distributed Search for Flow Network, enabling users to search the entire connected network with a single query while maintaining sub-2-second response times.

### 1.2 Scope

**In Scope:**
- SearchRouter orchestration layer
- Local search adapter wrapping FederatedSearch
- Live network query via GossipSub
- Result ranking and merging engine
- Extended search API endpoint
- Opt-in configuration for query responses
- Result caching layer

**Out of Scope:**
- Real-time query subscriptions
- Peer reputation tracking
- Query routing by topic
- Semantic query expansion

### 1.3 References

| Document | Location |
|----------|----------|
| Product Specification | `specs/04_phase3_distributed_search/product_spec.md` |
| Brainstorm | `specs/04_phase3_distributed_search/brainstorm.md` |
| ROADMAP | `specs/ROADMAP.md` (items 3.1-3.6) |
| Existing FederatedSearch | `src/modules/query/federated_search.rs` |
| GossipSub Infrastructure | `src/modules/network/gossipsub/` |

---

## 2. System Architecture

### 2.1 High-Level Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           REST API Layer                                 │
│                      POST /api/v1/search                                 │
└────────────────────────────────┬────────────────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                          SearchRouter                                    │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │                    SearchOrchestrator                            │   │
│  │    ┌───────────────┬───────────────┬───────────────────────┐    │   │
│  │    │ LocalAdapter  │ PrefetchAdapter│   LiveQueryEngine     │    │   │
│  │    │  (Qdrant)     │  (Qdrant)     │    (GossipSub)        │    │   │
│  │    └───────┬───────┴───────┬───────┴───────────┬───────────┘    │   │
│  │            │               │                   │                 │   │
│  │            └───────────────┼───────────────────┘                 │   │
│  │                            ▼                                     │   │
│  │                     ResultAggregator                             │   │
│  │            (normalize, penalty, boost, dedup)                    │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                                                                         │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │                      ResultCache (TTL: 15s)                      │   │
│  └─────────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                         Network Layer                                    │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │                GossipSub /flow/v1/search                         │   │
│  │        SearchQueryMessage ←→ SearchResponseMessage               │   │
│  └─────────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────┘
```

### 2.2 Component Overview

| Component | Responsibility | Location |
|-----------|---------------|----------|
| `SearchRouter` | Entry point, scope routing, caching | `src/modules/query/search_router.rs` |
| `SearchOrchestrator` | Parallel task spawning, timeout management | `src/modules/query/orchestrator.rs` |
| `LocalSearchAdapter` | Wraps FederatedSearch for local queries | `src/modules/query/adapters/local.rs` |
| `PrefetchSearchAdapter` | Queries remote-content-idx collection | `src/modules/query/adapters/prefetch.rs` |
| `LiveQueryEngine` | GossipSub broadcast, response aggregation | `src/modules/query/live_query.rs` |
| `ResultAggregator` | Score normalization, ranking, deduplication | `src/modules/query/ranking.rs` |
| `ResultCache` | LRU cache with 15s TTL | `src/modules/query/cache.rs` |
| `SearchQueryHandler` | Handles incoming search queries from peers | `src/modules/query/query_handler.rs` |

### 2.3 Module Structure

```
src/modules/query/
├── mod.rs                    # Module exports
├── search_router.rs          # SearchRouter entry point
├── orchestrator.rs           # SearchOrchestrator
├── live_query.rs             # LiveQueryEngine
├── ranking.rs                # ResultAggregator
├── cache.rs                  # ResultCache
├── query_handler.rs          # Incoming query handler
├── config.rs                 # SearchConfig
├── types.rs                  # Shared types
├── adapters/
│   ├── mod.rs
│   ├── local.rs              # LocalSearchAdapter
│   └── prefetch.rs           # PrefetchSearchAdapter
└── messages/
    ├── mod.rs
    ├── query.rs              # SearchQueryMessage
    └── response.rs           # SearchResponseMessage
```

### 2.4 Data Flow

```
┌──────────┐
│  User    │
│  Query   │
└────┬─────┘
     │ POST /api/v1/search
     ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                         SearchRouter                                     │
│  1. Parse request (query, scope, limit)                                 │
│  2. Check cache (cache_key = hash(query, scope))                        │
│  3. If cache hit → return cached response                               │
│  4. If cache miss → invoke SearchOrchestrator                           │
└────┬────────────────────────────────────────────────────────────────────┘
     │
     ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                      SearchOrchestrator                                  │
│  Based on scope, spawn tasks in parallel:                               │
│                                                                         │
│  scope=Local:   [LocalAdapter]                                          │
│  scope=Network: [PrefetchAdapter, LiveQueryEngine]                      │
│  scope=All:     [LocalAdapter, PrefetchAdapter, LiveQueryEngine]        │
│                                                                         │
│  tokio::select! with 500ms timeout                                      │
└────┬────────────────────────────────────────────────────────────────────┘
     │
     ├─────────────────────────────┬──────────────────────────────────────┐
     ▼                             ▼                                      ▼
┌─────────────┐           ┌─────────────┐                    ┌─────────────────┐
│LocalAdapter │           │PrefetchAdapter                   │LiveQueryEngine  │
│             │           │             │                    │                 │
│Query Qdrant │           │Query Qdrant │                    │1. Build message │
│space-*-idx  │           │remote-      │                    │2. Broadcast     │
│collections  │           │content-idx  │                    │3. Collect (500ms│
│             │           │             │                    │4. Parse results │
└──────┬──────┘           └──────┬──────┘                    └────────┬────────┘
       │                         │                                     │
       └─────────────────────────┼─────────────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                        ResultAggregator                                  │
│  1. Collect all SourcedResult items                                     │
│  2. Normalize scores to 0-1 per source                                  │
│  3. Apply source penalty (remote = 0.9x)                                │
│  4. Group by CID                                                        │
│  5. Calculate popularity boost (+0.1 per source, max +0.3)              │
│  6. Deduplicate (keep highest adjusted_score)                           │
│  7. Sort by final_score descending                                      │
│  8. Truncate to limit, track overflow                                   │
└────┬────────────────────────────────────────────────────────────────────┘
     │
     ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                          SearchResponse                                  │
│  - Store in cache (15s TTL)                                             │
│  - Return to caller                                                     │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## 3. Data Architecture

### 3.1 Core Types

#### 3.1.1 SearchScope

```rust
/// Search scope determining which sources to query.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SearchScope {
    /// Only local space collections.
    Local,
    /// Only network sources (prefetch + live query).
    Network,
    /// All sources (default).
    #[default]
    All,
}
```

#### 3.1.2 SearchRequest

```rust
/// Incoming search request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchRequest {
    /// The search query text.
    pub query: String,
    /// Search scope.
    #[serde(default)]
    pub scope: SearchScope,
    /// Maximum results to return.
    #[serde(default = "default_limit")]
    pub limit: u32,
}

fn default_limit() -> u32 { 10 }

impl SearchRequest {
    pub fn validate(&self) -> Result<(), SearchError> {
        if self.query.is_empty() {
            return Err(SearchError::InvalidQuery("Query cannot be empty".into()));
        }
        if self.query.len() > 1000 {
            return Err(SearchError::InvalidQuery("Query too long (max 1000 chars)".into()));
        }
        if self.limit > 100 {
            return Err(SearchError::InvalidQuery("Limit too high (max 100)".into()));
        }
        Ok(())
    }
}
```

#### 3.1.3 SearchResult

```rust
/// A single search result with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// Content identifier.
    pub cid: String,
    /// Original similarity score from source.
    pub score: f32,
    /// Score after normalization, penalty, and boost.
    pub adjusted_score: f32,
    /// Source type.
    pub source: ResultSource,
    /// Number of sources that found this CID.
    pub sources_count: u32,
    /// Document title (if available).
    pub title: Option<String>,
    /// Text snippet with matching context.
    pub snippet: Option<String>,
    /// Publisher peer ID (for network results).
    pub publisher_peer_id: Option<String>,
}

/// Source of a search result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResultSource {
    Local,
    Network,
}
```

#### 3.1.4 SearchResponse

```rust
/// Complete search response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResponse {
    pub success: bool,
    pub query: String,
    pub scope: SearchScope,
    pub results: Vec<SearchResult>,
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
```

#### 3.1.5 Internal Types

```rust
/// Internal result with source tracking before aggregation.
#[derive(Debug, Clone)]
pub(crate) struct SourcedResult {
    pub cid: String,
    pub score: f32,
    pub source: ResultSource,
    pub source_id: String,  // e.g., "local:space-abc-idx" or "network:peer-123"
    pub title: Option<String>,
    pub snippet: Option<String>,
    pub publisher_peer_id: Option<String>,
}

/// Result of score normalization per source.
#[derive(Debug)]
pub(crate) struct NormalizationContext {
    pub source_id: String,
    pub min_score: f32,
    pub max_score: f32,
}
```

### 3.2 Network Messages

#### 3.2.1 SearchQueryMessage

```rust
/// Search query broadcast message.
/// Serialized with DAG-CBOR.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchQueryMessage {
    /// Unique query ID for correlation.
    pub query_id: Uuid,
    /// The search query text.
    pub query: String,
    /// Maximum results requested.
    pub limit: u32,
    /// Requester's peer ID for direct response.
    pub requester_peer_id: String,
    /// Timestamp for TTL validation.
    pub timestamp: u64,
}

impl SearchQueryMessage {
    pub fn new(query: String, limit: u32, requester: PeerId) -> Self {
        Self {
            query_id: Uuid::new_v4(),
            query,
            limit,
            requester_peer_id: requester.to_base58(),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        }
    }

    /// Serialize to DAG-CBOR bytes.
    pub fn to_cbor(&self) -> Result<Vec<u8>, SearchError> {
        serde_ipld_dagcbor::to_vec(self)
            .map_err(|e| SearchError::Serialization(e.to_string()))
    }

    /// Deserialize from DAG-CBOR bytes.
    pub fn from_cbor(bytes: &[u8]) -> Result<Self, SearchError> {
        serde_ipld_dagcbor::from_slice(bytes)
            .map_err(|e| SearchError::Deserialization(e.to_string()))
    }
}
```

#### 3.2.2 SearchResponseMessage

```rust
/// Search response message sent directly to requester.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResponseMessage {
    /// Correlation to original query.
    pub query_id: Uuid,
    /// Responder's peer ID.
    pub responder_peer_id: String,
    /// Search results.
    pub results: Vec<NetworkSearchResult>,
    /// Total matches before limit.
    pub total_matches: u32,
    /// Response processing time.
    pub elapsed_ms: u32,
}

/// Individual result in network response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkSearchResult {
    pub cid: String,
    pub title: Option<String>,
    pub score: f32,
    pub snippet: Option<String>,
}

impl SearchResponseMessage {
    pub fn to_cbor(&self) -> Result<Vec<u8>, SearchError> {
        serde_ipld_dagcbor::to_vec(self)
            .map_err(|e| SearchError::Serialization(e.to_string()))
    }

    pub fn from_cbor(bytes: &[u8]) -> Result<Self, SearchError> {
        serde_ipld_dagcbor::from_slice(bytes)
            .map_err(|e| SearchError::Deserialization(e.to_string()))
    }
}
```

### 3.3 GossipSub Topic

Extend the `Topic` enum in `src/modules/network/gossipsub/topics.rs`:

```rust
/// Pre-defined Flow topics
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Topic {
    // ... existing variants ...

    /// Search queries broadcast
    SearchQueries,
}

impl Topic {
    pub fn to_topic_string(&self) -> String {
        match self {
            // ... existing matches ...
            Topic::SearchQueries => format!("{}/search", TOPIC_PREFIX),
        }
    }

    pub fn from_topic_string(s: &str) -> Option<Self> {
        // ... existing parsing ...
        match suffix {
            // ... existing matches ...
            "/search" => Some(Topic::SearchQueries),
            // ...
        }
    }
}
```

### 3.4 MessagePayload Extension

Extend `MessagePayload` in `src/modules/network/gossipsub/message.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "data")]
pub enum MessagePayload {
    // ... existing variants ...

    /// Search query broadcast
    SearchQuery {
        query_id: String,
        query: String,
        limit: u32,
        requester_peer_id: String,
    },

    /// Search response (sent directly, not broadcast)
    SearchResponse {
        query_id: String,
        results: Vec<serde_json::Value>,
        total_matches: u32,
        elapsed_ms: u32,
    },
}
```

### 3.5 Configuration

#### 3.5.1 SearchConfig

```rust
/// Configuration for distributed search.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchConfig {
    /// Enable responding to network search queries.
    #[serde(default)]
    pub respond_to_queries: bool,

    /// Maximum results to return per query response.
    #[serde(default = "default_max_results_per_query")]
    pub max_results_per_query: u32,

    /// Number of peers to query for live search.
    #[serde(default = "default_peer_count")]
    pub peer_count: u32,

    /// Timeout for network queries in milliseconds.
    #[serde(default = "default_network_timeout")]
    pub network_timeout_ms: u64,

    /// Whether to retry failed peer queries.
    #[serde(default = "default_retry_enabled")]
    pub retry_enabled: bool,

    /// Cache TTL in seconds.
    #[serde(default = "default_cache_ttl")]
    pub cache_ttl_secs: u64,

    /// Maximum cache size in entries.
    #[serde(default = "default_cache_size")]
    pub cache_max_entries: usize,
}

fn default_max_results_per_query() -> u32 { 20 }
fn default_peer_count() -> u32 { 15 }
fn default_network_timeout() -> u64 { 500 }
fn default_retry_enabled() -> bool { true }
fn default_cache_ttl() -> u64 { 15 }
fn default_cache_size() -> usize { 1000 }

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            respond_to_queries: false,  // Privacy-first default
            max_results_per_query: default_max_results_per_query(),
            peer_count: default_peer_count(),
            network_timeout_ms: default_network_timeout(),
            retry_enabled: default_retry_enabled(),
            cache_ttl_secs: default_cache_ttl(),
            cache_max_entries: default_cache_size(),
        }
    }
}
```

#### 3.5.2 Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `FLOW_SEARCH_RESPOND_TO_QUERIES` | Enable query responses | `false` |
| `FLOW_SEARCH_MAX_RESULTS_PER_QUERY` | Max results per response | `20` |
| `FLOW_SEARCH_PEER_COUNT` | Peers to query | `15` |
| `FLOW_SEARCH_NETWORK_TIMEOUT_MS` | Query timeout | `500` |
| `FLOW_SEARCH_CACHE_TTL_SECS` | Cache TTL | `15` |

---

## 4. API Specification

### 4.1 POST /api/v1/search

#### Request

```http
POST /api/v1/search
Content-Type: application/json

{
  "query": "distributed consensus algorithms",
  "scope": "all",
  "limit": 10
}
```

**Parameters:**

| Field | Type | Required | Default | Constraints |
|-------|------|----------|---------|-------------|
| `query` | string | Yes | - | 1-1000 characters |
| `scope` | string | No | `"all"` | `"local"`, `"network"`, `"all"` |
| `limit` | integer | No | `10` | 1-100 |

#### Response (200 OK)

```json
{
  "success": true,
  "query": "distributed consensus algorithms",
  "scope": "all",
  "results": [
    {
      "cid": "bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi",
      "score": 0.92,
      "adjusted_score": 0.88,
      "source": "local",
      "sources_count": 1,
      "title": "Introduction to Distributed Systems",
      "snippet": "...Paxos and Raft are widely-used **consensus algorithms** for...",
      "publisher_peer_id": null
    },
    {
      "cid": "bafybeihkoviema7g3gxyt6la7vd5ho32bsjdxwdqzq...",
      "score": 0.89,
      "adjusted_score": 0.85,
      "source": "network",
      "sources_count": 3,
      "title": "CAP Theorem Explained",
      "snippet": "...understanding **distributed** systems requires...",
      "publisher_peer_id": "12D3KooWRJeGh..."
    }
  ],
  "local_count": 5,
  "network_count": 12,
  "peers_queried": 15,
  "peers_responded": 12,
  "more_available": 23,
  "elapsed_ms": 487
}
```

#### Error Responses

| Status | Code | Description |
|--------|------|-------------|
| 400 | `INVALID_QUERY` | Query empty or exceeds 1000 chars |
| 400 | `INVALID_SCOPE` | Scope not one of: local, network, all |
| 400 | `INVALID_LIMIT` | Limit exceeds 100 |
| 503 | `NETWORK_UNAVAILABLE` | No peers connected (scope includes network) |
| 500 | `INTERNAL_ERROR` | Unexpected error |

**Error Response Format:**

```json
{
  "success": false,
  "error": {
    "code": "INVALID_QUERY",
    "message": "Query cannot be empty"
  }
}
```

### 4.2 API Implementation

```rust
// src/api/servers/rest/search.rs

use axum::{extract::State, Json};
use std::sync::Arc;

use crate::modules::query::{SearchRequest, SearchResponse, SearchRouter};

/// POST /api/v1/search handler
pub async fn search_handler(
    State(router): State<Arc<SearchRouter>>,
    Json(request): Json<SearchRequest>,
) -> Result<Json<SearchResponse>, SearchError> {
    // Validate request
    request.validate()?;

    // Execute search
    let response = router.search(request).await?;

    Ok(Json(response))
}
```

---

## 5. Infrastructure & Deployment

### 5.1 Resource Requirements

| Resource | Requirement | Notes |
|----------|-------------|-------|
| Memory | +10MB for cache | LRU cache with 1000 entries |
| CPU | Minimal | Async I/O bound |
| Network | ~100KB/search | 15 peers × 20 results × ~300B each |

### 5.2 Dependencies

#### New Crate Dependencies

```toml
# Cargo.toml additions
[dependencies]
lru = "0.12"              # LRU cache implementation
uuid = { version = "1.6", features = ["v4", "serde"] }
```

#### Internal Dependencies

| Dependency | Purpose |
|------------|---------|
| `FederatedSearch` | Base for LocalSearchAdapter |
| `GossipSub` | Network messaging |
| `NetworkManager` | Peer selection |
| `Qdrant` | Vector search |

### 5.3 Feature Flags

```toml
[features]
default = ["distributed-search"]
distributed-search = []  # Enable distributed search module
```

### 5.4 Configuration File

```toml
# config.toml

[search]
# Enable responding to network search queries
respond_to_queries = false

# Maximum results to return per query
max_results_per_query = 20

# Number of peers to query
peer_count = 15

# Network timeout in milliseconds
network_timeout_ms = 500

# Enable retry for failed queries
retry_enabled = true

# Result cache TTL in seconds
cache_ttl_secs = 15

# Maximum cache entries
cache_max_entries = 1000
```

---

## 6. Security Architecture

### 6.1 Threat Model

| Threat | Likelihood | Impact | Mitigation |
|--------|------------|--------|------------|
| Query flooding (DoS) | Medium | High | Connection limits, future rate limiting |
| Information leakage | Low | Medium | Opt-in model, no query persistence |
| Result poisoning | Low | Low | Score normalization limits single-peer influence |
| Privacy via queries | Medium | Medium | Don't log query text by default |

### 6.2 Privacy Controls

#### 6.2.1 Opt-in Model

```rust
impl SearchQueryHandler {
    pub async fn handle_query(&self, query: SearchQueryMessage) -> Option<SearchResponseMessage> {
        // Check opt-in configuration
        if !self.config.respond_to_queries {
            tracing::debug!(
                query_id = %query.query_id,
                "Ignoring search query - opt-in disabled"
            );
            return None;
        }

        // Process query...
    }
}
```

#### 6.2.2 Logging Policy

```rust
// Log metadata but NOT query text by default
tracing::info!(
    query_id = %query.query_id,
    requester = %query.requester_peer_id,
    limit = query.limit,
    "Received search query"
);

// Query text only at debug level (off in production)
tracing::debug!(query_text = %query.query, "Query text");
```

### 6.3 Input Validation

```rust
impl SearchQueryMessage {
    pub fn validate(&self) -> Result<(), SearchError> {
        // Query length
        if self.query.len() > 1000 {
            return Err(SearchError::InvalidQuery("Query too long".into()));
        }

        // Limit bounds
        if self.limit > 100 {
            return Err(SearchError::InvalidQuery("Limit too high".into()));
        }

        // Timestamp freshness (prevent replay)
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        if self.timestamp < now.saturating_sub(60_000) {
            return Err(SearchError::InvalidQuery("Query expired".into()));
        }

        Ok(())
    }
}
```

### 6.4 Future: Rate Limiting

```rust
// Planned for Phase 5
pub struct RateLimiter {
    requests: HashMap<PeerId, VecDeque<Instant>>,
    limit_per_minute: u32,
    burst_size: u32,
}

impl RateLimiter {
    pub fn check(&mut self, peer: &PeerId) -> bool {
        let now = Instant::now();
        let window_start = now - Duration::from_secs(60);

        let requests = self.requests.entry(*peer).or_default();

        // Remove old requests
        while requests.front().map_or(false, |&t| t < window_start) {
            requests.pop_front();
        }

        // Check limit
        if requests.len() >= self.limit_per_minute as usize {
            return false;
        }

        requests.push_back(now);
        true
    }
}
```

---

## 7. Integration Architecture

### 7.1 SearchRouter Integration

```rust
// src/modules/query/search_router.rs

use std::sync::Arc;
use tokio::sync::RwLock;

use crate::modules::ai::pipeline_manager::PipelineManager;
use crate::modules::network::manager::NetworkManager;
use crate::modules::query::cache::ResultCache;

pub struct SearchRouter {
    /// Local search via FederatedSearch
    federated_search: Arc<FederatedSearch>,
    /// Network manager for peer selection and messaging
    network_manager: Arc<NetworkManager>,
    /// Result cache
    cache: Arc<RwLock<ResultCache>>,
    /// Configuration
    config: SearchConfig,
}

impl SearchRouter {
    pub fn new(
        federated_search: Arc<FederatedSearch>,
        network_manager: Arc<NetworkManager>,
        config: SearchConfig,
    ) -> Self {
        Self {
            federated_search,
            network_manager,
            cache: Arc::new(RwLock::new(ResultCache::new(
                config.cache_max_entries,
                Duration::from_secs(config.cache_ttl_secs),
            ))),
            config,
        }
    }

    pub async fn search(&self, request: SearchRequest) -> Result<SearchResponse, SearchError> {
        let start = Instant::now();

        // Check cache
        let cache_key = self.cache_key(&request);
        if let Some(cached) = self.cache.read().await.get(&cache_key) {
            tracing::debug!(cache_key = %cache_key, "Cache hit");
            return Ok(cached.clone());
        }

        // Execute search
        let response = self.execute_search(request).await?;

        // Store in cache
        self.cache.write().await.insert(cache_key, response.clone());

        tracing::info!(
            elapsed_ms = start.elapsed().as_millis(),
            results = response.results.len(),
            "Search complete"
        );

        Ok(response)
    }
}
```

### 7.2 Node Integration

```rust
// In Node::new() or Node::start()

// Create SearchRouter
let search_router = Arc::new(SearchRouter::new(
    federated_search.clone(),
    network_manager.clone(),
    search_config,
));

// Register query handler for incoming searches
let query_handler = SearchQueryHandler::new(
    federated_search.clone(),
    network_manager.clone(),
    search_config.clone(),
);

// Subscribe to search topic
network_manager
    .subscribe_to_topic(Topic::SearchQueries, move |msg| {
        let handler = query_handler.clone();
        async move {
            if let MessagePayload::SearchQuery { .. } = msg.payload {
                handler.handle_incoming(msg).await;
            }
        }
    })
    .await?;
```

### 7.3 GossipSub Integration

```rust
// src/modules/query/live_query.rs

impl LiveQueryEngine {
    pub async fn broadcast_query(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<Vec<SourcedResult>, SearchError> {
        let query_id = Uuid::new_v4();
        let message = SearchQueryMessage::new(
            query.to_string(),
            limit,
            self.local_peer_id,
        );

        // Create response channel
        let (tx, mut rx) = mpsc::channel(self.config.peer_count as usize);

        // Register pending query
        self.pending_queries.write().await.insert(query_id, tx);

        // Broadcast query
        let topic = Topic::SearchQueries;
        let payload = MessagePayload::SearchQuery {
            query_id: query_id.to_string(),
            query: query.to_string(),
            limit,
            requester_peer_id: self.local_peer_id.to_base58(),
        };

        self.network_manager
            .publish_message(topic, payload)
            .await
            .map_err(|e| SearchError::Network(e.to_string()))?;

        // Collect responses with timeout
        let timeout = Duration::from_millis(self.config.network_timeout_ms);
        let mut results = Vec::new();
        let mut peers_responded = 0;

        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            match tokio::time::timeout_at(deadline.into(), rx.recv()).await {
                Ok(Some(response)) => {
                    peers_responded += 1;
                    for result in response.results {
                        results.push(SourcedResult {
                            cid: result.cid,
                            score: result.score,
                            source: ResultSource::Network,
                            source_id: format!("network:{}", response.responder_peer_id),
                            title: result.title,
                            snippet: result.snippet,
                            publisher_peer_id: Some(response.responder_peer_id.clone()),
                        });
                    }
                }
                Ok(None) => break,  // Channel closed
                Err(_) => break,     // Timeout
            }
        }

        // Cleanup pending query
        self.pending_queries.write().await.remove(&query_id);

        tracing::info!(
            query_id = %query_id,
            peers_responded = peers_responded,
            results = results.len(),
            "Live query complete"
        );

        Ok(results)
    }
}
```

---

## 8. Performance & Scalability

### 8.1 Performance Budget

| Phase | Target | Implementation |
|-------|--------|----------------|
| API parsing | <5ms | Serde deserialize |
| Cache lookup | <1ms | LRU HashMap |
| Embedding generation | <50ms | FastEmbed |
| Local Qdrant search | <100ms | Per-collection parallel |
| Prefetch Qdrant search | <100ms | Single collection |
| Network broadcast | <10ms | GossipSub publish |
| Network wait | ≤500ms | Timeout-bounded |
| Result aggregation | <50ms | In-memory merge |
| Response serialization | <10ms | Serde serialize |
| **Total (scope=all)** | **<2000ms** | **775ms + variance** |

### 8.2 Concurrency Model

```rust
impl SearchOrchestrator {
    pub async fn execute(
        &self,
        request: &SearchRequest,
    ) -> Result<Vec<SourcedResult>, SearchError> {
        let mut tasks: Vec<JoinHandle<Result<Vec<SourcedResult>, SearchError>>> = Vec::new();

        // Spawn tasks based on scope
        match request.scope {
            SearchScope::Local => {
                tasks.push(tokio::spawn(self.local_search(request.query.clone(), request.limit)));
            }
            SearchScope::Network => {
                tasks.push(tokio::spawn(self.prefetch_search(request.query.clone(), request.limit)));
                tasks.push(tokio::spawn(self.live_query(request.query.clone(), request.limit)));
            }
            SearchScope::All => {
                tasks.push(tokio::spawn(self.local_search(request.query.clone(), request.limit)));
                tasks.push(tokio::spawn(self.prefetch_search(request.query.clone(), request.limit)));
                tasks.push(tokio::spawn(self.live_query(request.query.clone(), request.limit)));
            }
        }

        // Wait for all with timeout
        let timeout = Duration::from_millis(self.config.network_timeout_ms + 200);
        let mut all_results = Vec::new();

        for task in tasks {
            match tokio::time::timeout(timeout, task).await {
                Ok(Ok(Ok(results))) => all_results.extend(results),
                Ok(Ok(Err(e))) => tracing::warn!(error = %e, "Search task failed"),
                Ok(Err(e)) => tracing::warn!(error = %e, "Search task panicked"),
                Err(_) => tracing::warn!("Search task timed out"),
            }
        }

        Ok(all_results)
    }
}
```

### 8.3 Caching Strategy

```rust
// src/modules/query/cache.rs

use lru::LruCache;
use std::num::NonZeroUsize;
use std::time::{Duration, Instant};

pub struct ResultCache {
    cache: LruCache<String, CacheEntry>,
    ttl: Duration,
}

struct CacheEntry {
    response: SearchResponse,
    inserted_at: Instant,
}

impl ResultCache {
    pub fn new(max_entries: usize, ttl: Duration) -> Self {
        Self {
            cache: LruCache::new(NonZeroUsize::new(max_entries).unwrap()),
            ttl,
        }
    }

    pub fn get(&mut self, key: &str) -> Option<SearchResponse> {
        if let Some(entry) = self.cache.get(key) {
            if entry.inserted_at.elapsed() < self.ttl {
                return Some(entry.response.clone());
            }
            // Expired - will be evicted on next insert
        }
        None
    }

    pub fn insert(&mut self, key: String, response: SearchResponse) {
        self.cache.put(key, CacheEntry {
            response,
            inserted_at: Instant::now(),
        });
    }
}
```

### 8.4 Resource Limits

| Resource | Limit | Enforcement |
|----------|-------|-------------|
| Max query length | 1000 chars | Request validation |
| Max results per request | 100 | Request validation |
| Max results per peer | 20 | Config, response truncation |
| Network timeout | 500ms | tokio::time::timeout |
| Cache memory | ~10MB | LRU with 1000 entries |
| Concurrent searches | 100 | Semaphore (future) |

---

## 9. Reliability & Operations

### 9.1 Error Handling

```rust
#[derive(Debug, Error)]
pub enum SearchError {
    #[error("invalid query: {0}")]
    InvalidQuery(String),

    #[error("invalid scope: {0}")]
    InvalidScope(String),

    #[error("embedding error: {0}")]
    Embedding(String),

    #[error("qdrant error: {0}")]
    Qdrant(String),

    #[error("network error: {0}")]
    Network(String),

    #[error("serialization error: {0}")]
    Serialization(String),

    #[error("deserialization error: {0}")]
    Deserialization(String),

    #[error("no peers available")]
    NoPeersAvailable,

    #[error("timeout")]
    Timeout,

    #[error("internal error: {0}")]
    Internal(String),
}

impl SearchError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::InvalidQuery(_) | Self::InvalidScope(_) => StatusCode::BAD_REQUEST,
            Self::NoPeersAvailable => StatusCode::SERVICE_UNAVAILABLE,
            Self::Timeout => StatusCode::GATEWAY_TIMEOUT,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn error_code(&self) -> &'static str {
        match self {
            Self::InvalidQuery(_) => "INVALID_QUERY",
            Self::InvalidScope(_) => "INVALID_SCOPE",
            Self::NoPeersAvailable => "NETWORK_UNAVAILABLE",
            Self::Timeout => "TIMEOUT",
            _ => "INTERNAL_ERROR",
        }
    }
}
```

### 9.2 Graceful Degradation

```rust
impl SearchOrchestrator {
    /// Execute search with graceful degradation.
    pub async fn execute_with_fallback(
        &self,
        request: &SearchRequest,
    ) -> SearchResponse {
        let start = Instant::now();
        let mut results = Vec::new();
        let mut local_count = 0;
        let mut network_count = 0;
        let mut peers_queried = 0;
        let mut peers_responded = 0;

        // Always try local search (fast)
        if request.scope != SearchScope::Network {
            match self.local_search(&request.query, request.limit).await {
                Ok(local_results) => {
                    local_count = local_results.len() as u32;
                    results.extend(local_results);
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Local search failed");
                }
            }
        }

        // Try network search if requested
        if request.scope != SearchScope::Local {
            peers_queried = self.config.peer_count;

            // Prefetch (fast, reliable)
            match self.prefetch_search(&request.query, request.limit).await {
                Ok(prefetch_results) => {
                    network_count += prefetch_results.len() as u32;
                    results.extend(prefetch_results);
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Prefetch search failed");
                }
            }

            // Live query (may timeout, that's ok)
            match self.live_query(&request.query, request.limit).await {
                Ok((live_results, responded)) => {
                    peers_responded = responded;
                    network_count += live_results.len() as u32;
                    results.extend(live_results);
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Live query failed");
                }
            }
        }

        // Always return something, even if partial
        let aggregated = self.aggregator.aggregate(results, request.limit);

        SearchResponse {
            success: true,
            query: request.query.clone(),
            scope: request.scope,
            results: aggregated.results,
            local_count,
            network_count,
            peers_queried,
            peers_responded,
            more_available: aggregated.overflow_count,
            elapsed_ms: start.elapsed().as_millis() as u64,
        }
    }
}
```

### 9.3 Observability

#### 9.3.1 Metrics

```rust
use metrics::{counter, gauge, histogram};

impl SearchRouter {
    fn record_metrics(&self, response: &SearchResponse, scope: SearchScope) {
        // Request count
        counter!("search_requests_total", "scope" => scope.as_str()).increment(1);

        // Latency histogram
        histogram!("search_latency_ms", "scope" => scope.as_str())
            .record(response.elapsed_ms as f64);

        // Result counts
        histogram!("search_results_count", "source" => "local")
            .record(response.local_count as f64);
        histogram!("search_results_count", "source" => "network")
            .record(response.network_count as f64);

        // Peer response rate
        if response.peers_queried > 0 {
            let rate = response.peers_responded as f64 / response.peers_queried as f64;
            gauge!("search_peer_response_rate").set(rate);
        }

        // Cache stats
        gauge!("search_cache_entries").set(self.cache_size() as f64);
    }
}
```

#### 9.3.2 Logging

```rust
// Structured logging for search operations
tracing::info!(
    query_id = %query_id,
    scope = ?request.scope,
    limit = request.limit,
    elapsed_ms = response.elapsed_ms,
    local_count = response.local_count,
    network_count = response.network_count,
    peers_queried = response.peers_queried,
    peers_responded = response.peers_responded,
    cache_hit = cached,
    "Search completed"
);
```

### 9.4 Health Checks

```rust
impl SearchRouter {
    /// Check search system health.
    pub async fn health_check(&self) -> HealthStatus {
        let mut status = HealthStatus::Healthy;
        let mut details = Vec::new();

        // Check Qdrant connectivity
        if let Err(e) = self.federated_search.health_check().await {
            status = HealthStatus::Degraded;
            details.push(format!("Qdrant: {}", e));
        }

        // Check peer connectivity
        let connected_peers = self.network_manager.connected_peers().await;
        if connected_peers < self.config.peer_count / 2 {
            status = HealthStatus::Degraded;
            details.push(format!("Low peer count: {}", connected_peers));
        }

        HealthStatus {
            status,
            details,
            cache_entries: self.cache_size(),
            connected_peers,
        }
    }
}
```

---

## 10. Development Standards

### 10.1 Code Organization

```
src/modules/query/
├── mod.rs                    # Public exports
├── search_router.rs          # Main entry point (~200 lines)
├── orchestrator.rs           # Task orchestration (~150 lines)
├── live_query.rs             # Network queries (~200 lines)
├── ranking.rs                # Result aggregation (~250 lines)
├── cache.rs                  # LRU cache (~100 lines)
├── query_handler.rs          # Incoming query handler (~150 lines)
├── config.rs                 # Configuration (~80 lines)
├── types.rs                  # Shared types (~200 lines)
├── error.rs                  # Error types (~80 lines)
├── adapters/
│   ├── mod.rs                # Adapter trait (~30 lines)
│   ├── local.rs              # Local search (~100 lines)
│   └── prefetch.rs           # Prefetch search (~100 lines)
└── messages/
    ├── mod.rs                # Message exports (~10 lines)
    ├── query.rs              # SearchQueryMessage (~80 lines)
    └── response.rs           # SearchResponseMessage (~60 lines)
```

### 10.2 Testing Strategy

#### 10.2.1 Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_score_normalization() {
        let results = vec![
            SourcedResult { score: 0.5, ..Default::default() },
            SourcedResult { score: 1.0, ..Default::default() },
            SourcedResult { score: 0.75, ..Default::default() },
        ];

        let normalized = ResultAggregator::normalize(&results);

        assert!((normalized[0].score - 0.0).abs() < 0.01);  // min -> 0
        assert!((normalized[1].score - 1.0).abs() < 0.01);  // max -> 1
        assert!((normalized[2].score - 0.5).abs() < 0.01);  // mid -> 0.5
    }

    #[test]
    fn test_source_penalty() {
        let local = SourcedResult {
            score: 0.9,
            source: ResultSource::Local,
            ..Default::default()
        };
        let remote = SourcedResult {
            score: 0.9,
            source: ResultSource::Network,
            ..Default::default()
        };

        let adjusted_local = ResultAggregator::apply_penalty(&local);
        let adjusted_remote = ResultAggregator::apply_penalty(&remote);

        assert_eq!(adjusted_local, 0.9);
        assert!((adjusted_remote - 0.81).abs() < 0.01);  // 0.9 * 0.9
    }

    #[test]
    fn test_popularity_boost() {
        let results = vec![
            SourcedResult { cid: "cid1".into(), source_id: "s1".into(), ..Default::default() },
            SourcedResult { cid: "cid1".into(), source_id: "s2".into(), ..Default::default() },
            SourcedResult { cid: "cid1".into(), source_id: "s3".into(), ..Default::default() },
            SourcedResult { cid: "cid2".into(), source_id: "s1".into(), ..Default::default() },
        ];

        let grouped = ResultAggregator::group_by_cid(results);

        assert_eq!(grouped.get("cid1").unwrap().sources_count, 3);
        assert_eq!(grouped.get("cid2").unwrap().sources_count, 1);

        // Popularity boost: (3-1)*0.1 = 0.2
        assert!((grouped.get("cid1").unwrap().boost - 0.2).abs() < 0.01);
    }

    #[test]
    fn test_deduplication() {
        let results = vec![
            SourcedResult {
                cid: "cid1".into(),
                score: 0.8,
                ..Default::default()
            },
            SourcedResult {
                cid: "cid1".into(),
                score: 0.9,  // Higher score
                ..Default::default()
            },
        ];

        let deduped = ResultAggregator::deduplicate(results);

        assert_eq!(deduped.len(), 1);
        assert!((deduped[0].score - 0.9).abs() < 0.01);  // Kept higher
    }
}
```

#### 10.2.2 Integration Tests

```rust
// tests/modules/query/search_router.rs

#[tokio::test]
async fn test_local_only_search() {
    let router = setup_test_router().await;

    let request = SearchRequest {
        query: "test query".into(),
        scope: SearchScope::Local,
        limit: 10,
    };

    let response = router.search(request).await.unwrap();

    assert!(response.success);
    assert_eq!(response.scope, SearchScope::Local);
    assert_eq!(response.peers_queried, 0);
    assert_eq!(response.network_count, 0);
}

#[tokio::test]
async fn test_network_only_search() {
    let router = setup_test_router_with_peers(5).await;

    let request = SearchRequest {
        query: "test query".into(),
        scope: SearchScope::Network,
        limit: 10,
    };

    let response = router.search(request).await.unwrap();

    assert!(response.success);
    assert_eq!(response.scope, SearchScope::Network);
    assert_eq!(response.local_count, 0);
    assert!(response.peers_queried > 0);
}

#[tokio::test]
async fn test_hybrid_search() {
    let router = setup_test_router_with_peers(5).await;

    let request = SearchRequest {
        query: "test query".into(),
        scope: SearchScope::All,
        limit: 10,
    };

    let response = router.search(request).await.unwrap();

    assert!(response.success);
    assert!(response.elapsed_ms < 2000);
}

#[tokio::test]
async fn test_timeout_handling() {
    let router = setup_test_router_with_slow_peers().await;

    let request = SearchRequest {
        query: "test query".into(),
        scope: SearchScope::All,
        limit: 10,
    };

    let response = router.search(request).await.unwrap();

    // Should complete despite slow peers
    assert!(response.success);
    assert!(response.elapsed_ms < 2500);  // 500ms timeout + buffer
}

#[tokio::test]
async fn test_opt_in_disabled() {
    let handler = setup_query_handler(SearchConfig {
        respond_to_queries: false,
        ..Default::default()
    }).await;

    let query = SearchQueryMessage::new("test".into(), 10, test_peer_id());

    let response = handler.handle_query(query).await;

    assert!(response.is_none());  // No response when opt-in disabled
}
```

#### 10.2.3 Performance Tests

```rust
#[tokio::test]
async fn test_local_search_latency() {
    let router = setup_test_router().await;

    let request = SearchRequest {
        query: "performance test".into(),
        scope: SearchScope::Local,
        limit: 10,
    };

    let start = Instant::now();
    let response = router.search(request).await.unwrap();
    let elapsed = start.elapsed();

    assert!(elapsed.as_millis() < 200, "Local search took {}ms", elapsed.as_millis());
}

#[tokio::test]
async fn test_concurrent_searches() {
    let router = Arc::new(setup_test_router().await);

    let tasks: Vec<_> = (0..100).map(|i| {
        let router = router.clone();
        tokio::spawn(async move {
            let request = SearchRequest {
                query: format!("query {}", i),
                scope: SearchScope::Local,
                limit: 10,
            };
            router.search(request).await
        })
    }).collect();

    let results = futures::future::join_all(tasks).await;

    let success_count = results.iter()
        .filter(|r| r.as_ref().map(|r| r.is_ok()).unwrap_or(false))
        .count();

    assert!(success_count >= 95, "Only {} of 100 searches succeeded", success_count);
}
```

### 10.3 Documentation Requirements

- All public types and functions must have doc comments
- Module-level documentation explaining purpose and usage
- Examples for complex APIs
- CHANGELOG entry for each release

---

## 11. Implementation Roadmap

### 11.1 Work Items

| ID | Item | Priority | Estimate | Dependencies |
|----|------|----------|----------|--------------|
| 3.1.1 | SearchRouter struct | P0 | 1d | - |
| 3.1.2 | SearchScope enum | P0 | 0.5d | - |
| 3.1.3 | SearchConfig | P0 | 0.5d | - |
| 3.1.4 | SearchRequest/Response types | P0 | 0.5d | - |
| 3.2.1 | LocalSearchAdapter | P0 | 1d | 3.1.x |
| 3.2.2 | PrefetchSearchAdapter | P0 | 0.5d | 3.1.x |
| 3.3.1 | SearchQueryMessage | P0 | 0.5d | - |
| 3.3.2 | SearchResponseMessage | P0 | 0.5d | 3.3.1 |
| 3.3.3 | Topic::SearchQueries | P0 | 0.5d | - |
| 3.3.4 | LiveQueryEngine | P0 | 2d | 3.3.1-3.3.3 |
| 3.3.5 | SearchQueryHandler | P0 | 1d | 3.3.1-3.3.3 |
| 3.4.1 | SearchOrchestrator | P0 | 1d | 3.2.x, 3.3.4 |
| 3.5.1 | Score normalization | P0 | 0.5d | - |
| 3.5.2 | Source penalty | P0 | 0.5d | 3.5.1 |
| 3.5.3 | Popularity boost | P0 | 0.5d | 3.5.1 |
| 3.5.4 | Deduplication | P0 | 0.5d | 3.5.1-3.5.3 |
| 3.5.5 | ResultAggregator | P0 | 1d | 3.5.1-3.5.4 |
| 3.6.1 | ResultCache | P1 | 0.5d | - |
| 3.6.2 | API endpoint | P0 | 1d | 3.1.x, 3.4.1, 3.5.5 |
| 3.6.3 | Error handling | P0 | 0.5d | 3.6.2 |
| 3.6.4 | Metrics | P1 | 0.5d | 3.6.2 |
| 3.6.5 | Integration tests | P0 | 2d | All above |

### 11.2 Implementation Phases

#### Phase 1: Foundation (Week 1)
- [ ] 3.1.1-3.1.4: Core types and configuration
- [ ] 3.2.1-3.2.2: Search adapters
- [ ] 3.5.1-3.5.5: Ranking engine
- [ ] Unit tests for ranking

#### Phase 2: Network Protocol (Week 2)
- [ ] 3.3.1-3.3.3: Message types and topic
- [ ] 3.3.4: LiveQueryEngine
- [ ] 3.3.5: SearchQueryHandler
- [ ] 3.4.1: SearchOrchestrator
- [ ] Integration tests

#### Phase 3: API & Polish (Week 3)
- [ ] 3.6.1: Result caching
- [ ] 3.6.2-3.6.3: API endpoint with error handling
- [ ] 3.6.4: Metrics and observability
- [ ] 3.6.5: Full integration tests

#### Phase 4: Testing & Documentation (Week 4)
- [ ] Performance testing
- [ ] End-to-end tests
- [ ] Documentation
- [ ] Bug fixes

### 11.3 Risk Mitigation

| Risk | Mitigation | Contingency |
|------|------------|-------------|
| Network latency > 500ms | Aggressive timeout, partial results | Increase timeout, degrade gracefully |
| Low peer response rate | Retry once, prefetch as fallback | Increase peer count |
| Score normalization edge cases | Extensive unit tests | Manual score caps |
| Cache memory pressure | LRU eviction, size limits | Reduce TTL or max entries |

---

## 12. Appendices

### Appendix A: Algorithm Details

#### A.1 Score Normalization

```
For each source s in sources:
    min_s = min(scores in s)
    max_s = max(scores in s)
    range_s = max_s - min_s

    If range_s == 0:
        normalized = 1.0  # All same score
    Else:
        For each result r in s:
            r.normalized_score = (r.score - min_s) / range_s
```

#### A.2 Final Score Calculation

```
For each result r:
    1. normalized = normalize(r.score, source_min, source_max)
    2. penalized = normalized * (0.9 if r.source == Network else 1.0)
    3. sources_count = count(results with same CID)
    4. boost = min(0.3, (sources_count - 1) * 0.1)
    5. final_score = penalized + boost
```

#### A.3 Deduplication

```
Group results by CID:
    For each CID group:
        Keep result with highest penalized score
        Set sources_count = size of group
        Calculate boost based on sources_count
```

### Appendix B: Message Formats

#### B.1 SearchQueryMessage (DAG-CBOR)

```
{
  "query_id": "550e8400-e29b-41d4-a716-446655440000",
  "query": "distributed consensus",
  "limit": 20,
  "requester_peer_id": "12D3KooWRJeGh...",
  "timestamp": 1705859200000
}
```

#### B.2 SearchResponseMessage (DAG-CBOR)

```
{
  "query_id": "550e8400-e29b-41d4-a716-446655440000",
  "responder_peer_id": "12D3KooWABCdef...",
  "results": [
    {
      "cid": "bafybeigdyrzt...",
      "title": "Consensus Algorithms",
      "score": 0.92,
      "snippet": "...Paxos and Raft are..."
    }
  ],
  "total_matches": 45,
  "elapsed_ms": 87
}
```

### Appendix C: Configuration Reference

| Config Key | Type | Default | Description |
|------------|------|---------|-------------|
| `respond_to_queries` | bool | `false` | Enable query responses |
| `max_results_per_query` | u32 | `20` | Max results per response |
| `peer_count` | u32 | `15` | Peers to query |
| `network_timeout_ms` | u64 | `500` | Query timeout |
| `retry_enabled` | bool | `true` | Retry failed queries |
| `cache_ttl_secs` | u64 | `15` | Cache TTL |
| `cache_max_entries` | usize | `1000` | Max cache entries |

### Appendix D: Glossary

| Term | Definition |
|------|------------|
| CID | Content Identifier - cryptographic hash of content |
| DAG-CBOR | CBOR encoding for IPLD data structures |
| GossipSub | libp2p pub/sub protocol |
| Prefetch | Search of pre-indexed remote content |
| Live Query | Real-time broadcast to connected peers |
| Scope | Search target: local, network, or all |
| Adjusted Score | Score after normalization, penalty, and boost |
