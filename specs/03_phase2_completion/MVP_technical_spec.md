# MVP Technical Specification: Phase 2 Completion

> **Version**: 1.0.0
> **Status**: Ready for Implementation
> **Created**: 2026-01-10
> **Source**: technical_spec.md v1.1.0

---

## 1. MVP Overview

### 1.1 Objective

Deliver the minimum viable implementation of Phase 2 completion that enables:
1. **Publishing tracking** - Record what content has been published
2. **Discovery** - Receive and store content announcements from network
3. **Remote indexing** - Index discovered content for search
4. **Federated search** - Query both local and remote content
5. **Content API** - REST endpoints for all operations

### 1.2 Success Criteria

A user can:
- Publish content via API and see it tracked
- Discover content announced by other nodes
- Trigger indexing of discovered content
- Search across both local and remote content
- Retrieve content by CID (local or network)

### 1.3 MVP vs Full Spec

| Feature | MVP | Full Spec | Migration Path |
|---------|-----|-----------|----------------|
| Database schema | Full | Full | None needed |
| Rate limiting | Simple counter | Per-peer DashMap | Replace implementation |
| Indexing | Synchronous | Background queue | Add queue wrapper |
| Trusted DIDs | Empty list | Config-based | Add env parsing |
| Signature validation | Skip | Optional verify | Add validation logic |
| Content type filter | All allowed | Configurable | Add filter check |

---

## 2. MVP Architecture

### 2.1 Components

```
┌─────────────────────────────────────────────────────────────┐
│                      REST API Layer                          │
│  /content/publish  /content/{cid}  /search                  │
└─────────────────────────────┬───────────────────────────────┘
                              │
┌─────────────────────────────▼───────────────────────────────┐
│                      Service Layer                           │
│  ┌─────────────────┐  ┌───────────────┐  ┌───────────────┐  │
│  │ ContentService  │  │ RemoteIndexer │  │FederatedSearch│  │
│  │ (+ DB tracking) │  │ (sync mode)   │  │               │  │
│  └─────────────────┘  └───────────────┘  └───────────────┘  │
└─────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────▼───────────────────────────────┐
│  PostgreSQL          │  Qdrant           │  GossipSub       │
│  - published_content │  - space-*-idx    │  - announcements │
│  - remote_content    │  - remote-content │                  │
└─────────────────────────────────────────────────────────────┘
```

### 2.2 Simplified Components

**RemoteIndexer (MVP):**
- Synchronous indexing (no background queue)
- No content type filtering
- No trusted DID auto-indexing
- Manual trigger only via API

**Rate Limiter (MVP):**
- Simple atomic counter per peer
- Reset on node restart (no persistence)
- 100/minute limit hardcoded

**DiscoveryHandler (MVP):**
- Parse and validate announcements
- Insert to database
- Log but don't auto-index

---

## 3. Work Items

### 3.1 Summary

| Priority | Count | Effort |
|----------|-------|--------|
| P0 (Must Have) | 12 | ~M |
| P1 (Should Have) | 4 | ~S |
| P2 (Nice to Have) | 2 | ~XS |
| **Total** | **18** | **~L** |

**Effort Key:** XS (<2h), S (2-4h), M (4-8h), L (1-2d), XL (2-3d)

### 3.2 P0 - Must Have (MVP Blockers)

---

#### MVP-1: Create published_content Entity

**Priority:** P0
**Effort:** S (2-4h)
**Dependencies:** None
**Assignee:** Backend

**Description:**
Create Sea-ORM entity for tracking published content.

**File:** `back-end/entity/src/published_content.rs`

**Implementation:**
```rust
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "published_content")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    #[sea_orm(unique)]
    pub cid: String,
    pub path: Option<String>,
    pub size: i64,
    pub block_count: i32,
    pub content_type: Option<String>,
    pub title: Option<String>,
    pub qdrant_collection: Option<String>,
    #[sea_orm(column_type = "Json", nullable)]
    pub qdrant_point_ids: Option<Json>,
    pub published_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
```

**Acceptance Criteria:**
- [ ] Entity compiles without errors
- [ ] Entity exported from `back-end/entity/src/mod.rs`
- [ ] All fields match technical spec

---

#### MVP-2: Create remote_content Entity

**Priority:** P0
**Effort:** S (2-4h)
**Dependencies:** None
**Assignee:** Backend

**Description:**
Create Sea-ORM entity for tracking discovered remote content.

**File:** `back-end/entity/src/remote_content.rs`

**Implementation:**
```rust
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "remote_content")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    #[sea_orm(unique)]
    pub cid: String,
    pub publisher_peer_id: String,
    pub publisher_did: Option<String>,
    #[sea_orm(column_type = "Json", nullable)]
    pub metadata: Option<Json>,
    pub content_type: Option<String>,
    pub title: Option<String>,
    pub size: i64,
    pub block_count: i32,
    pub discovered_at: DateTimeWithTimeZone,
    pub cached: bool,
    pub indexed: bool,
    pub index_error: Option<String>,
    pub qdrant_collection: Option<String>,
    pub embedding_count: Option<i32>,
    pub indexed_at: Option<DateTimeWithTimeZone>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
```

**Acceptance Criteria:**
- [ ] Entity compiles without errors
- [ ] Entity exported from mod.rs
- [ ] All fields match technical spec

---

#### MVP-3: Create Database Migration

**Priority:** P0
**Effort:** M (4-8h)
**Dependencies:** MVP-1, MVP-2
**Assignee:** Backend

**Description:**
Create Sea-ORM migration for both content tables with indexes.

**File:** `back-end/migration/src/m20260110_000001_create_content_tables.rs`

**Acceptance Criteria:**
- [ ] Migration creates published_content table
- [ ] Migration creates remote_content table
- [ ] All indexes created (cid unique, published_at, discovered_at, indexed, publisher_*)
- [ ] Migration runs successfully: `sea-orm-cli migrate up`
- [ ] Migration rolls back cleanly: `sea-orm-cli migrate down`

---

#### MVP-4: Create ContentAnnouncement Schema

**Priority:** P0
**Effort:** S (2-4h)
**Dependencies:** None
**Assignee:** Backend

**Description:**
Define the ContentAnnouncement message structure for GossipSub.

**File:** `back-end/node/src/modules/network/discovery/announcement.rs`

**Implementation:**
```rust
use serde::{Deserialize, Serialize};

pub const CONTENT_ANNOUNCE_TOPIC: &str = "/flow/content/announce/1.0.0";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentAnnouncement {
    pub version: u8,
    pub cid: String,
    pub publisher_peer_id: String,
    pub publisher_did: String,
    pub content_type: String,
    pub size: u64,
    pub block_count: u32,
    pub title: String,
    pub timestamp: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub signature: Vec<u8>,
}

impl ContentAnnouncement {
    pub fn to_cbor(&self) -> Result<Vec<u8>, serde_ipld_dagcbor::EncodeError> {
        serde_ipld_dagcbor::to_vec(self)
    }

    pub fn from_cbor(data: &[u8]) -> Result<Self, serde_ipld_dagcbor::DecodeError> {
        serde_ipld_dagcbor::from_slice(data)
    }
}
```

**Acceptance Criteria:**
- [ ] Struct serializes/deserializes with DAG-CBOR
- [ ] Topic constant defined
- [ ] Unit tests for serialization round-trip

---

#### MVP-5: Implement Announcement Validation

**Priority:** P0
**Effort:** S (2-4h)
**Dependencies:** MVP-4
**Assignee:** Backend

**Description:**
Validate incoming ContentAnnouncement messages.

**File:** `back-end/node/src/modules/network/discovery/validation.rs`

**Implementation:**
```rust
impl ContentAnnouncement {
    pub fn validate(&self, sender_peer_id: &str) -> Result<(), ValidationError> {
        // Version check
        if self.version != 1 {
            return Err(ValidationError::UnsupportedVersion(self.version));
        }

        // CID format validation
        if ContentId::try_from(self.cid.as_str()).is_err() {
            return Err(ValidationError::InvalidCid);
        }

        // Sender verification
        if self.publisher_peer_id != sender_peer_id {
            return Err(ValidationError::PeerIdMismatch);
        }

        // Size limits (max 1GB)
        if self.size == 0 || self.size > 1_073_741_824 {
            return Err(ValidationError::InvalidSize(self.size));
        }

        // Timestamp freshness (±5 minutes)
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let drift = (now as i64 - self.timestamp as i64).abs();
        if drift > 300 {
            return Err(ValidationError::TimestampDrift(drift));
        }

        Ok(())
    }
}
```

**Acceptance Criteria:**
- [ ] Validates version == 1
- [ ] Validates CID format
- [ ] Validates peer_id matches sender
- [ ] Validates size bounds
- [ ] Validates timestamp freshness
- [ ] Unit tests for each validation rule

---

#### MVP-6: Implement DiscoveryHandler

**Priority:** P0
**Effort:** M (4-8h)
**Dependencies:** MVP-3, MVP-4, MVP-5
**Assignee:** Backend

**Description:**
Handle incoming content announcements from GossipSub.

**File:** `back-end/node/src/modules/network/discovery/handler.rs`

**Key Logic:**
1. Parse announcement from CBOR
2. Validate announcement
3. Check rate limit (simple counter)
4. Check if CID already known
5. Insert remote_content record
6. Log discovery

**Acceptance Criteria:**
- [ ] Subscribes to `/flow/content/announce/1.0.0` topic
- [ ] Parses and validates announcements
- [ ] Rejects malformed announcements (log, don't crash)
- [ ] Rate limits: 100/peer/minute
- [ ] Inserts new records to remote_content
- [ ] Skips duplicate CIDs
- [ ] Integration test with mock GossipSub message

---

#### MVP-7: Implement RemoteIndexer (Sync Mode)

**Priority:** P0
**Effort:** M (4-8h)
**Dependencies:** MVP-3
**Assignee:** Backend

**Description:**
Synchronous remote content indexer (no background queue for MVP).

**File:** `back-end/node/src/modules/ai/remote_indexer.rs`

**Key Methods:**
```rust
impl RemoteIndexer {
    pub async fn index_content(&self, cid: &str) -> Result<IndexResult, IndexError> {
        // 1. Fetch content via NetworkProvider
        // 2. Chunk content
        // 3. Generate embeddings
        // 4. Store in remote-content-idx collection
        // 5. Update remote_content record
    }
}
```

**Acceptance Criteria:**
- [ ] Fetches content by CID from network
- [ ] Creates embeddings via existing pipeline
- [ ] Stores in `remote-content-idx` Qdrant collection
- [ ] Updates remote_content record (indexed=true, embedding_count)
- [ ] Handles fetch failures (sets index_error)
- [ ] Retry logic: 3 attempts with backoff

---

#### MVP-8: Implement FederatedSearch

**Priority:** P0
**Effort:** M (4-8h)
**Dependencies:** MVP-7
**Assignee:** Backend

**Description:**
Search across local space collections and remote-content-idx.

**File:** `back-end/node/src/modules/ai/federated_search.rs`

**Key Methods:**
```rust
impl FederatedSearch {
    pub async fn search(&self, query: &str, scope: SearchScope, limit: usize)
        -> Result<FederatedResults, SearchError> {
        // 1. Generate query embedding
        // 2. Query local collections (if scope includes local)
        // 3. Query remote-content-idx (if scope includes remote)
        // 4. Merge results with 0.9x penalty for remote
        // 5. Sort by adjusted score
    }
}

pub enum SearchScope {
    Local,
    Remote,
    All,
}
```

**Acceptance Criteria:**
- [ ] Queries local space collections
- [ ] Queries remote-content-idx collection
- [ ] Applies 0.9x score penalty to remote results
- [ ] Merges and sorts by adjusted score
- [ ] Respects scope parameter
- [ ] Returns source (local/remote) per result

---

#### MVP-9: POST /api/v1/content/publish Endpoint

**Priority:** P0
**Effort:** M (4-8h)
**Dependencies:** MVP-3
**Assignee:** Backend

**Description:**
REST endpoint for publishing content.

**File:** `back-end/node/src/api/rest/content.rs`

**Acceptance Criteria:**
- [ ] Accepts multipart file upload
- [ ] Calls ContentService.publish()
- [ ] Inserts published_content record
- [ ] Announces to GossipSub
- [ ] Returns: cid, size, block_count, announcement status
- [ ] Error handling: 400 (no file), 413 (too large), 500 (failure)

---

#### MVP-10: GET /api/v1/content/{cid} Endpoint

**Priority:** P0
**Effort:** S (2-4h)
**Dependencies:** None
**Assignee:** Backend

**Description:**
REST endpoint for retrieving content by CID.

**File:** `back-end/node/src/api/rest/content.rs`

**Acceptance Criteria:**
- [ ] Parses CID from path
- [ ] Attempts local fetch first
- [ ] Falls back to network fetch
- [ ] Returns binary content with appropriate headers
- [ ] X-Flow-Source header indicates local/network
- [ ] Error handling: 400 (invalid CID), 404 (not found), 504 (timeout)

---

#### MVP-11: GET /api/v1/content/published Endpoint

**Priority:** P0
**Effort:** S (2-4h)
**Dependencies:** MVP-3
**Assignee:** Backend

**Description:**
REST endpoint for listing published content.

**File:** `back-end/node/src/api/rest/content.rs`

**Acceptance Criteria:**
- [ ] Queries published_content table
- [ ] Supports pagination (limit, offset)
- [ ] Returns: items array, total count
- [ ] Each item: cid, path, size, block_count, content_type, title, published_at

---

#### MVP-12: GET /api/v1/content/discovered Endpoint

**Priority:** P0
**Effort:** S (2-4h)
**Dependencies:** MVP-3
**Assignee:** Backend

**Description:**
REST endpoint for listing discovered remote content.

**File:** `back-end/node/src/api/rest/content.rs`

**Acceptance Criteria:**
- [ ] Queries remote_content table
- [ ] Supports pagination (limit, offset)
- [ ] Supports filter by indexed status
- [ ] Returns: items array, total count
- [ ] Each item: cid, publisher_did, content_type, title, size, discovered_at, indexed

---

### 3.3 P1 - Should Have

---

#### MVP-13: POST /api/v1/content/{cid}/index Endpoint

**Priority:** P1
**Effort:** S (2-4h)
**Dependencies:** MVP-7
**Assignee:** Backend

**Description:**
REST endpoint to trigger indexing of remote content.

**Acceptance Criteria:**
- [ ] Validates CID exists in remote_content
- [ ] Calls RemoteIndexer.index_content()
- [ ] Returns status: success, already_indexed, not_found
- [ ] Updates remote_content record on success/failure

---

#### MVP-14: GET /api/v1/content/{cid}/providers Endpoint

**Priority:** P1
**Effort:** S (2-4h)
**Dependencies:** None
**Assignee:** Backend

**Description:**
REST endpoint to find providers for content.

**Acceptance Criteria:**
- [ ] Queries Kademlia DHT via NetworkManager
- [ ] Returns list of peer_id + multiaddresses
- [ ] Indicates if content is available locally

---

#### MVP-15: POST /api/v1/search Endpoint

**Priority:** P1
**Effort:** S (2-4h)
**Dependencies:** MVP-8
**Assignee:** Backend

**Description:**
REST endpoint for federated search.

**Acceptance Criteria:**
- [ ] Accepts JSON body: query, scope, limit, min_score
- [ ] Calls FederatedSearch.search()
- [ ] Returns results with source indication
- [ ] Returns query_time_ms metric

---

#### MVP-16: Integration Tests

**Priority:** P1
**Effort:** M (4-8h)
**Dependencies:** All P0 items
**Assignee:** Backend

**Description:**
End-to-end integration tests for content workflow.

**Test Cases:**
- [ ] Publish content → verify in published_content table
- [ ] Receive announcement → verify in remote_content table
- [ ] Index remote content → verify in Qdrant
- [ ] Search local → returns local results
- [ ] Search remote → returns remote results
- [ ] Search all → returns merged results

---

### 3.4 P2 - Nice to Have

---

#### MVP-17: Date Range Filtering for Published

**Priority:** P2
**Effort:** XS (<2h)
**Dependencies:** MVP-11
**Assignee:** Backend

**Description:**
Add after/before query params to GET /content/published.

---

#### MVP-18: Publisher Filtering for Discovered

**Priority:** P2
**Effort:** XS (<2h)
**Dependencies:** MVP-12
**Assignee:** Backend

**Description:**
Add publisher query param to GET /content/discovered.

---

## 4. Implementation Sequence

```
Week 1: Foundation
├── Day 1-2: MVP-1, MVP-2, MVP-3 (Entities + Migration)
├── Day 3: MVP-4, MVP-5 (Announcement schema + validation)
└── Day 4-5: MVP-6 (DiscoveryHandler)

Week 2: Core Features
├── Day 1-2: MVP-7 (RemoteIndexer)
├── Day 3: MVP-8 (FederatedSearch)
└── Day 4-5: MVP-9, MVP-10 (Publish + Retrieve APIs)

Week 3: Complete + Test
├── Day 1: MVP-11, MVP-12 (List APIs)
├── Day 2: MVP-13, MVP-14, MVP-15 (Remaining APIs)
├── Day 3-4: MVP-16 (Integration tests)
└── Day 5: Buffer / fixes
```

## 5. Deferred to Post-MVP

| Feature | Reason | When to Add |
|---------|--------|-------------|
| Background indexing queue | Complexity; sync is adequate for MVP scale | When discovery volume > 100/hour |
| Trusted DID auto-indexing | Needs trust infrastructure | Phase 3 |
| Content type filtering | All content useful for MVP | When storage becomes concern |
| Signature verification | Optional in v1 spec | Phase 5 (Security) |
| WebSocket discovery events | Real-time not critical | Phase 3 |
| Search pagination | MVP limit of 100 sufficient | When result sets grow |

## 6. Risk Mitigation

| Risk | Mitigation |
|------|------------|
| Qdrant collection doesn't exist | Create in MVP-7 if missing |
| NetworkProvider fetch failures | Retry with backoff, record error |
| Large announcement floods | Rate limiter caps at 100/peer/min |
| Migration conflicts | Test rollback before deploy |

## 7. Definition of Done

MVP is complete when:
- [ ] All P0 work items implemented
- [ ] All P0 acceptance criteria pass
- [ ] All existing tests pass (486+)
- [ ] New test coverage > 80%
- [ ] Can publish content and see in published list
- [ ] Can receive announcement and see in discovered list
- [ ] Can trigger indexing and search finds remote content
- [ ] Can search across local and remote sources

---

## Revision History

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 1.0.0 | 2026-01-10 | Claude | Initial MVP specification |
