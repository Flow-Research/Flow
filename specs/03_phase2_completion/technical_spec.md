# Technical Specification: Phase 2 Completion

> **Version**: 1.1.0
> **Status**: Reviewed
> **Created**: 2026-01-10
> **Source**: product_spec.md v1.1.0

---

## 1. Overview

### 1.1 Purpose

This document provides the technical blueprint for completing Phase 2 of the Flow Network, covering:
- Database schema for content tracking (2.5)
- Remote content indexing pipeline (2.7)
- Content API endpoints (2.8)
- Federated search capability (2.9)

### 1.2 Scope

| In Scope | Out of Scope |
|----------|--------------|
| published_content and remote_content entities | Live network query broadcast |
| GossipSub announcement subscription | Advanced score normalization |
| RemoteIndexer pipeline | Agent-based content sync |
| 7 REST API endpoints | Content encryption |
| FederatedSearch with local+remote | Multi-user permissions |

### 1.3 Architecture Style

**Monolithic** - Extends existing flow-node binary following established patterns:
- Event-driven network layer (mpsc commands)
- Repository pattern for storage
- Service layer for business logic
- REST API via Axum

### 1.4 Tech Stack

| Layer | Technology | Justification |
|-------|------------|---------------|
| Runtime | Rust + Tokio | Existing stack |
| Database | PostgreSQL + Sea-ORM | Existing ORM |
| Vector Store | Qdrant | Existing infrastructure |
| Network | libp2p (GossipSub, Kademlia) | Existing stack |
| API | Axum | Existing REST framework |
| Serialization | serde + DAG-CBOR | Existing content format |

---

## 2. System Architecture

### 2.1 High-Level Architecture

```
+---------------------------------------------------------------------+
|                           REST API Layer                             |
|  POST /content/publish  GET /content/{cid}  POST /search            |
+-----------------------------+---------------------------------------+
                              |
+-----------------------------v---------------------------------------+
|                          Service Layer                               |
|  +----------------+  +-----------------+  +--------------------+     |
|  | ContentService |  |  RemoteIndexer  |  |  FederatedSearch   |     |
|  | (existing)     |  |  (new)          |  |  (new)             |     |
|  +-------+--------+  +--------+--------+  +---------+----------+     |
+----------+--------------------|----------------------+---------------+
           |                    |                      |
+----------v--------------------v----------------------v---------------+
|                         Storage Layer                                |
|  +------------+  +------------------+  +-------------------------+   |
|  | BlockStore |  | PostgreSQL       |  | Qdrant                  |   |
|  | (RocksDB)  |  | (Sea-ORM)        |  | (Vector Store)          |   |
|  |            |  | - published_     |  | - space collections     |   |
|  |            |  |   content        |  | - remote-content-idx    |   |
|  |            |  | - remote_content |  |                         |   |
|  +------------+  +------------------+  +-------------------------+   |
+----------------------------------------------------------------------+
           |
+----------v-----------------------------------------------------------+
|                        Network Layer                                 |
|  +-----------------+  +---------------+  +------------------------+  |
|  | GossipSub       |  | Kademlia DHT  |  | Request-Response       |  |
|  | - announcements |  | - providers   |  | - block transfer       |  |
|  +-----------------+  +---------------+  +------------------------+  |
+----------------------------------------------------------------------+
```

### 2.2 Component Interactions

**Publishing Flow:**
1. Client sends POST /content/publish with file
2. ContentEndpoint receives multipart upload
3. ContentService chunks, builds DAG, stores blocks
4. PublishedContent record inserted in PostgreSQL
5. NetworkManager announces to DHT and GossipSub

**Discovery Flow:**
1. Remote node publishes ContentAnnouncement to GossipSub
2. Local GossipSub handler receives message
3. DiscoveryHandler validates announcement
4. RemoteContent record inserted in PostgreSQL
5. If trusted publisher, RemoteIndexer queues indexing

**Search Flow:**
1. Client sends POST /search with query and scope
2. FederatedSearch generates embedding
3. Parallel queries to local space collections and remote-content-idx
4. Results merged with 0.9x penalty for remote
5. Combined results returned

### 2.3 New Components

| Component | Location | Responsibility |
|-----------|----------|----------------|
| published_content entity | back-end/entity/src/published_content.rs | Track local publications |
| remote_content entity | back-end/entity/src/remote_content.rs | Track discovered content |
| DiscoveryHandler | back-end/node/src/modules/network/discovery.rs | Process announcements |
| RemoteIndexer | back-end/node/src/modules/ai/remote_indexer.rs | Index remote content |
| FederatedSearch | back-end/node/src/modules/ai/federated_search.rs | Multi-source search |
| ContentEndpoint | back-end/node/src/api/rest/content.rs | REST handlers |

---

## 3. Data Architecture

### 3.1 Database Schema

#### 3.1.1 published_content Entity

```rust
// back-end/entity/src/published_content.rs

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "published_content")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,

    /// CID of the root content (unique)
    #[sea_orm(unique)]
    pub cid: String,

    /// Original file path (if file-based publish)
    pub path: Option<String>,

    /// Total size in bytes
    pub size: i64,

    /// Number of blocks in DAG
    pub block_count: i32,

    /// MIME type of content
    pub content_type: Option<String>,

    /// Title/name for display
    pub title: Option<String>,

    /// Qdrant collection where indexed
    pub qdrant_collection: Option<String>,

    /// Qdrant point IDs (JSON array of UUIDs)
    #[sea_orm(column_type = "Json", nullable)]
    pub qdrant_point_ids: Option<Json>,

    /// When content was published
    pub published_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
```

**Indexes:**
- UNIQUE on cid
- INDEX on published_at DESC

#### 3.1.2 remote_content Entity

```rust
// back-end/entity/src/remote_content.rs

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "remote_content")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,

    /// CID of the root content (unique)
    #[sea_orm(unique)]
    pub cid: String,

    /// Peer ID of publisher
    pub publisher_peer_id: String,

    /// DID of publisher (optional)
    pub publisher_did: Option<String>,

    /// Content metadata from announcement
    #[sea_orm(column_type = "Json", nullable)]
    pub metadata: Option<Json>,

    /// Content type from announcement
    pub content_type: Option<String>,

    /// Title from announcement
    pub title: Option<String>,

    /// Size in bytes from announcement
    pub size: i64,

    /// Block count from announcement
    pub block_count: i32,

    /// When discovered via GossipSub
    pub discovered_at: DateTimeWithTimeZone,

    /// Whether blocks are cached locally
    pub cached: bool,

    /// Whether indexed in Qdrant
    pub indexed: bool,

    /// Last indexing error (if failed)
    pub index_error: Option<String>,

    /// Qdrant collection (if indexed)
    pub qdrant_collection: Option<String>,

    /// Number of embeddings created
    pub embedding_count: Option<i32>,

    /// When last indexed (if indexed)
    pub indexed_at: Option<DateTimeWithTimeZone>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
```

**Indexes:**
- UNIQUE on cid
- INDEX on publisher_peer_id
- INDEX on publisher_did
- INDEX on discovered_at DESC
- INDEX on indexed

### 3.2 Migration

Migration file: `m20260110_000001_create_content_tables.rs`

Creates both tables with appropriate columns and indexes.

```rust
// In up() method, after table creation:

// Indexes for published_content
manager.create_index(
    Index::create()
        .name("idx_published_content_published_at")
        .table(PublishedContent::Table)
        .col(PublishedContent::PublishedAt)
        .to_owned()
).await?;

// Indexes for remote_content
manager.create_index(
    Index::create()
        .name("idx_remote_content_publisher_peer_id")
        .table(RemoteContent::Table)
        .col(RemoteContent::PublisherPeerId)
        .to_owned()
).await?;

manager.create_index(
    Index::create()
        .name("idx_remote_content_publisher_did")
        .table(RemoteContent::Table)
        .col(RemoteContent::PublisherDid)
        .to_owned()
).await?;

manager.create_index(
    Index::create()
        .name("idx_remote_content_discovered_at")
        .table(RemoteContent::Table)
        .col(RemoteContent::DiscoveredAt)
        .to_owned()
).await?;

manager.create_index(
    Index::create()
        .name("idx_remote_content_indexed")
        .table(RemoteContent::Table)
        .col(RemoteContent::Indexed)
        .to_owned()
).await?;
```

### 3.3 Qdrant Collections

#### Existing Space Collections
Pattern: `space-{space_key}-idx` for local content per space.

#### New Remote Content Collection
**Name:** `remote-content-idx`
**Vector Size:** 1536 (OpenAI ada-002 compatible)
**Distance:** Cosine

**Payload Schema:**
- cid: String
- chunk_index: i32
- publisher_peer_id: String
- publisher_did: String (optional)
- content_type: String
- title: String
- text: String (chunk content)

---

## 4. API Specification

### 4.1 Content Endpoints

#### 4.1.1 POST /api/v1/content/publish

**Purpose:** Publish content to the network

**Request:** multipart/form-data
- file: binary (required)
- title: string (optional)
- content_type: string (optional, auto-detected)
- announce_dht: boolean (default: true)
- announce_gossip: boolean (default: true)

**Response (200):**
```json
{
  "cid": "bafybeif...",
  "size": 1048576,
  "block_count": 5,
  "content_type": "text/markdown",
  "title": "My Document",
  "qdrant_collection": "space-default-idx",
  "qdrant_point_ids": ["uuid1", "uuid2"],
  "announcement": {
    "dht": "success",
    "gossipsub": "success"
  }
}
```

**Errors:** 400 (no file), 413 (too large), 500 (storage failure)

#### 4.1.2 GET /api/v1/content/{cid}

**Purpose:** Retrieve content by CID

**Response (200):**
- Content-Type: application/octet-stream
- X-Flow-CID: {cid}
- X-Flow-Source: local|network
- Body: binary content

**Errors:** 400 (invalid CID), 404 (not found), 504 (timeout)

#### 4.1.3 GET /api/v1/content/published

**Purpose:** List locally published content

**Query Params:** limit, offset, after, before

**Response (200):**
```json
{
  "items": [...],
  "total": 42,
  "limit": 20,
  "offset": 0
}
```

#### 4.1.4 GET /api/v1/content/discovered

**Purpose:** List discovered remote content

**Query Params:** limit, offset, indexed, publisher

**Response (200):**
```json
{
  "items": [...],
  "total": 150,
  "limit": 20,
  "offset": 0
}
```

#### 4.1.5 POST /api/v1/content/{cid}/index

**Purpose:** Trigger indexing of remote content

**Response (200):**
```json
{
  "status": "success|already_indexed|not_found",
  "cid": "bafybeig...",
  "embedding_count": 12,
  "indexed_at": "2026-01-10T12:30:00Z"
}
```

#### 4.1.6 GET /api/v1/content/{cid}/providers

**Purpose:** Find peers providing specific content

**Response (200):**
```json
{
  "cid": "bafybeif...",
  "providers": [
    {
      "peer_id": "12D3KooW...",
      "addresses": ["/ip4/.../tcp/4001"]
    }
  ],
  "local": true
}
```

### 4.2 Search Endpoint

#### 4.2.1 POST /api/v1/search

**Purpose:** Federated search across local and remote content

**Request:**
```json
{
  "query": "machine learning algorithms",
  "scope": "all",
  "limit": 20,
  "min_score": 0.5
}
```

**Response (200):**
```json
{
  "results": [
    {
      "cid": "bafybeif...",
      "chunk_index": 0,
      "score": 0.92,
      "source": "local",
      "space_key": "my-space",
      "title": "ML Guide",
      "text": "Matching text...",
      "cached": true
    }
  ],
  "total_local": 5,
  "total_remote": 12,
  "query_time_ms": 145
}
```

---

## 5. Infrastructure and Deployment

### 5.1 Configuration

**Environment Variables:**
```bash
# Remote Indexing
REMOTE_INDEXER_ENABLED=true
REMOTE_INDEXER_AUTO_INDEX_TRUSTED=true
REMOTE_INDEXER_MAX_CONTENT_SIZE=1073741824
REMOTE_INDEXER_FETCH_TIMEOUT_SECS=30
REMOTE_INDEXER_FETCH_RETRIES=2
REMOTE_INDEXER_RATE_LIMIT_PER_PEER=100
REMOTE_INDEXER_ALLOWED_CONTENT_TYPES=text/plain,text/markdown,application/pdf,text/html

# Federated Search
FEDERATED_SEARCH_REMOTE_PENALTY=0.9
FEDERATED_SEARCH_DEFAULT_LIMIT=20
FEDERATED_SEARCH_MAX_LIMIT=100

# Qdrant
QDRANT_REMOTE_COLLECTION=remote-content-idx
```

### 5.2 Deployment Steps

1. Run database migration: `sea-orm-cli migrate up`
2. Create Qdrant collection if not exists
3. Deploy updated node binary
4. Verify content announcement subscription

---

## 6. Security Architecture

### 6.1 Announcement Validation

ContentAnnouncement validation rules:
- version must be 1
- cid must be valid CIDv1 format
- publisher_peer_id must match message sender
- size must be > 0 and < 1GB (configurable)
- timestamp must be within +-5 minutes of current time

### 6.2 Rate Limiting

AnnouncementRateLimiter:
- Per-peer tracking with 1-minute sliding window
- Default: 100 announcements per peer per minute
- Exceeded peers are logged and dropped

### 6.3 Content Validation

- CID integrity verification on fetch (existing)
- No execution of fetched content
- Size limits enforced before fetch

---

## 7. Integration Architecture

### 7.1 GossipSub Integration

**Topic:** `/flow/content/announce/1.0.0`

**Handler Registration:**
1. Subscribe to topic on node startup
2. Register ContentAnnouncementHandler
3. Handler validates, rate-limits, stores records

### 7.2 Publishing Integration

ContentService.publish_with_tracking():
1. Existing publish logic (chunk, DAG, store)
2. Insert PublishedContent record
3. Return combined result

### 7.3 ContentAnnouncement Message

```rust
pub struct ContentAnnouncement {
    pub version: u8,           // 1
    pub cid: String,
    pub publisher_peer_id: String,
    pub publisher_did: String,
    pub content_type: String,
    pub size: u64,
    pub block_count: u32,
    pub title: String,
    pub timestamp: u64,
    // Signature is OPTIONAL in v1 - reserved for future use
    // When present, Ed25519 signature over CBOR-encoded fields (excluding signature)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub signature: Vec<u8>,
}
```

Serialized with DAG-CBOR for network transmission.

**Note:** Signature verification is NOT enforced in v1. The field is reserved for future trust verification. Current security relies on peer_id verification and rate limiting.

### 7.4 Selective Indexing Policy

```rust
impl RemoteIndexer {
    /// Check if content should be indexed based on policy
    pub fn is_indexable(&self, announcement: &ContentAnnouncement) -> bool {
        // Size check
        if announcement.size > self.config.max_content_size {
            debug!(cid = %announcement.cid, size = announcement.size, "Content too large");
            return false;
        }

        // Content type check
        if !self.config.allowed_content_types.is_empty() {
            let allowed = self.config.allowed_content_types
                .iter()
                .any(|t| announcement.content_type.starts_with(t));
            if !allowed {
                debug!(cid = %announcement.cid, content_type = %announcement.content_type, "Content type not allowed");
                return false;
            }
        }

        true
    }

    /// Check if publisher is trusted for auto-indexing
    pub fn is_trusted_publisher(&self, publisher_did: &Option<String>) -> bool {
        match publisher_did {
            Some(did) => self.config.trusted_dids.contains(did),
            None => false,
        }
    }
}
```

**Trusted Publishers Configuration:**
```bash
# Comma-separated list of trusted DIDs for auto-indexing
REMOTE_INDEXER_TRUSTED_DIDS=did:key:z6Mk...,did:key:z6Mn...
```

### 7.5 RemoteIndexer Queue

```rust
pub struct RemoteIndexer {
    db: DatabaseConnection,
    network_provider: Arc<NetworkProvider>,
    qdrant_client: Arc<QdrantClient>,
    config: RemoteIndexerConfig,
    index_queue: Arc<Mutex<VecDeque<String>>>,  // CIDs pending indexing
    worker_handle: Option<JoinHandle<()>>,
}

impl RemoteIndexer {
    /// Queue a CID for background indexing
    pub async fn queue_indexing(&self, cid: &str) {
        let mut queue = self.index_queue.lock().await;
        if !queue.contains(&cid.to_string()) {
            queue.push_back(cid.to_string());
        }
    }

    /// Start background worker that processes queue
    pub fn start_worker(&mut self) {
        let queue = self.index_queue.clone();
        let db = self.db.clone();
        let provider = self.network_provider.clone();
        let qdrant = self.qdrant_client.clone();
        let config = self.config.clone();

        self.worker_handle = Some(tokio::spawn(async move {
            loop {
                let cid = {
                    let mut q = queue.lock().await;
                    q.pop_front()
                };

                if let Some(cid) = cid {
                    if let Err(e) = Self::do_index(&db, &provider, &qdrant, &config, &cid).await {
                        error!(cid = %cid, error = %e, "Background indexing failed");
                    }
                } else {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }));
    }
}
```

---

## 8. Performance and Scalability

### 8.1 Performance Targets

| Operation | Target |
|-----------|--------|
| File publish (10MB) | < 5s |
| Content retrieval (1MB, local) | < 100ms |
| Content retrieval (1MB, network) | < 2s |
| Search query | < 1s |
| Announcement processing | < 50ms |

### 8.2 Scalability Targets

| Metric | Target |
|--------|--------|
| Published content records | 10,000+ |
| Discovered content records | 100,000+ |
| Concurrent API requests | 100+ |
| Qdrant remote collection | 1M+ vectors |

### 8.3 Optimization Strategies

- Batch database inserts for bulk announcements
- Connection pooling (existing Sea-ORM)
- Background indexing queue
- Qdrant batched upserts
- LRU cache for metadata

---

## 9. Reliability and Operations

### 9.1 Error Handling

**Remote Fetch Retry:**
- Max attempts: 3
- Initial delay: 100ms
- Backoff factor: 2.0x
- Max delay: 10s

**Index Error Tracking:**
- Failed indexing records index_error in database
- Allows retry via POST /content/{cid}/index

### 9.2 Monitoring

**Metrics (Prometheus):**
- flow_content_published_total
- flow_content_discovered_total
- flow_remote_index_success_total
- flow_remote_index_failure_total
- flow_search_latency_seconds
- flow_announcement_rate_limited_total

**Logging:** Structured tracing with cid, peer_id, operation context

---

## 10. Development Standards

### 10.1 Code Organization

```
back-end/node/src/
  api/rest/content.rs         # New endpoints
  modules/ai/
    remote_indexer.rs         # New
    federated_search.rs       # New
  modules/network/
    discovery.rs              # New handler

back-end/entity/src/
  published_content.rs        # New entity
  remote_content.rs           # New entity

back-end/migration/src/
  m20260110_..._content.rs    # New migration
```

### 10.2 Testing Requirements

| Component | Test Type | Coverage |
|-----------|-----------|----------|
| Database entities | Unit | 100% |
| Announcement validation | Unit | 100% |
| Rate limiter | Unit | 100% |
| API endpoints | Integration | 90% |
| RemoteIndexer | Integration | 80% |
| FederatedSearch | Integration | 80% |

### 10.3 Documentation

- Rustdoc for all public APIs
- README updates for new endpoints
- Architecture diagrams maintained

---

## 11. Implementation Roadmap

### 11.1 Work Items

**Phase 1: Database Foundation**
- WI-1: published_content entity
- WI-2: remote_content entity
- WI-3: Migration

**Phase 2: GossipSub Integration**
- WI-4: ContentAnnouncement schema
- WI-5: Announcement validation
- WI-6: Rate limiter
- WI-7: DiscoveryHandler

**Phase 3: Remote Indexing**
- WI-8: RemoteIndexer struct
- WI-9: Fetch with retry
- WI-10: Qdrant integration

**Phase 4: Content API**
- WI-11: POST /content/publish
- WI-12: GET /content/{cid}
- WI-13: GET /content/published
- WI-14: GET /content/discovered
- WI-15: POST /content/{cid}/index
- WI-16: GET /content/{cid}/providers

**Phase 5: Federated Search**
- WI-17: FederatedSearch struct
- WI-18: POST /search endpoint
- WI-19: Score merging

**Phase 6: Integration and Testing**
- WI-20: Integration tests
- WI-21: E2E tests
- WI-22: Documentation

### 11.2 Dependencies

- WI-3 depends on WI-1, WI-2
- WI-7 depends on WI-4, WI-5, WI-6, WI-3
- WI-10 depends on WI-8, WI-9
- WI-11 depends on WI-3
- WI-15 depends on WI-10
- WI-18 depends on WI-17
- WI-20 depends on all implementation WIs

---

## 12. Appendices

### A. Error Types

```rust
#[derive(Debug, thiserror::Error)]
pub enum ContentTrackingError {
    #[error("Database error: {0}")]
    Database(#[from] sea_orm::DbErr),
    #[error("Content not found: {0}")]
    NotFound(String),
    #[error("Invalid CID: {0}")]
    InvalidCid(String),
    #[error("Validation failed: {0}")]
    ValidationFailed(String),
    #[error("Rate limited")]
    RateLimited,
    #[error("Network error: {0}")]
    Network(String),
    #[error("Indexing error: {0}")]
    Indexing(String),
}
```

### B. API Route Registration

```rust
// In build_router()
.route("/api/v1/content/publish", post(content::publish_content))
.route("/api/v1/content/published", get(content::list_published))
.route("/api/v1/content/discovered", get(content::list_discovered))
.route("/api/v1/content/:cid", get(content::get_content))
.route("/api/v1/content/:cid/index", post(content::index_content))
.route("/api/v1/content/:cid/providers", get(content::get_providers))
.route("/api/v1/search", post(search::federated_search))
```

---

## Revision History

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 1.0.0 | 2026-01-10 | Claude | Initial specification |
| 1.1.0 | 2026-01-10 | Claude | 6-perspective review: Added RemoteIndexer queue, selective indexing policy, migration indexes, clarified signature handling |
