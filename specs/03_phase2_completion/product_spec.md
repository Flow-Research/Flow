# Product Specification: Phase 2 Completion

> **Version**: 1.1.0
> **Status**: Reviewed
> **Created**: 2026-01-10
> **Source**: brainstorm.md, ROADMAP.md

---

## 1. Executive Summary

### Product Name
**Flow Phase 2: Data Sync & Publishing Completion**

### Vision Statement
Complete the remaining 25% of Phase 2 to enable cross-node content publishing, discovery, and federated search, establishing the foundation for distributed knowledge sharing.

### Target Release
Q1 2026 (2-3 weeks from start)

### Success Metric
A user can publish content to the network, discover content from other nodes, and search across both local and remote sources.

---

## 2. Problem Statement

### Current State
The Flow network has implemented core content-addressing infrastructure:
- ✅ BlockStore with RocksDB backend
- ✅ IPLD/CID content addressing
- ✅ DAG building and manifest creation
- ✅ ContentService for publishing workflow
- ✅ NetworkProvider for content retrieval
- ✅ Request-Response protocol for block transfer

### Gap Analysis
However, the system lacks critical capabilities:

| Gap | Impact |
|-----|--------|
| No persistence of published content | Cannot track what's been published |
| No tracking of discovered content | Cannot manage remote content |
| No remote content indexing | Discovered content isn't searchable |
| No content API endpoints | Clients cannot interact with content |
| No federated search | Search limited to local content |

### User Pain Points
1. **Cannot track publications**: After publishing, no record exists
2. **Cannot discover content**: GossipSub announcements are ignored
3. **Cannot search network**: Only local content is searchable
4. **No programmatic access**: No API for content operations

---

## 3. User Personas

### Persona 1: Solo Node Operator
**Name**: Alex (Developer)
**Goals**:
- Index personal documents
- Track what content has been published
- Search across all indexed content

**Pain Points**:
- No visibility into published content status
- Cannot see publishing history

### Persona 2: Multi-Node Admin
**Name**: Jordan (DevOps)
**Goals**:
- Run multiple Flow nodes
- Sync content between nodes
- Monitor content discovery

**Pain Points**:
- Cannot see what remote content is available
- No way to selectively index discovered content

### Persona 3: Application Developer
**Name**: Sam (Integration Developer)
**Goals**:
- Build applications on top of Flow
- Use REST API for content operations
- Implement search across network

**Pain Points**:
- No API endpoints for content operations
- Cannot search beyond local node

---

## 4. User Stories & Requirements

### Epic 1: Content Publishing Tracking (2.5)

#### US-1.1: Track Published Content
**As a** node operator,
**I want** to see a list of all content I've published,
**So that** I can manage my publications and verify they were successful.

**Acceptance Criteria**:
- [ ] Published content is recorded in database
- [ ] Each record includes: CID, path, size, block count, publish timestamp
- [ ] Records include Qdrant collection and point IDs for cross-reference
- [ ] Can query publications by CID or timestamp

#### US-1.2: Track Discovered Content
**As a** node operator,
**I want** to see remote content discovered from the network,
**So that** I can decide what to index locally.

**Acceptance Criteria**:
- [ ] Discovered content is recorded when announced via GossipSub
- [ ] Each record includes: CID, publisher peer ID, publisher DID, metadata
- [ ] Records track: discovered_at, cached status, indexed status
- [ ] Can query discoveries by publisher or timestamp

### Epic 2: Remote Content Indexing (2.7)

#### US-2.1: Subscribe to Content Announcements
**As a** node,
**I want** to receive and process content announcements from the network,
**So that** I can discover available content.

**Acceptance Criteria**:
- [ ] Node subscribes to `/flow/content/announce/1.0.0` GossipSub topic
- [ ] Incoming announcements are parsed per ContentAnnouncement schema (see Appendix C)
- [ ] Valid announcements create remote_content records
- [ ] Malformed announcements are logged with peer ID and dropped (no crash)
- [ ] Rate limiting: max 100 announcements per peer per minute

#### US-2.2: Index Remote Content
**As a** node operator,
**I want** to index discovered content into a searchable collection,
**So that** I can search across network content.

**Acceptance Criteria**:
- [ ] RemoteIndexer uses NetworkProvider to fetch content by CID
- [ ] Fetched content is chunked and embedded via existing PipelineManager
- [ ] Embeddings stored in separate Qdrant collection (`remote-content-idx`)
- [ ] remote_content record updated with indexed=true, embedding_count
- [ ] Fetch timeout: 30 seconds with 2 retries (exponential backoff)
- [ ] On permanent failure: record marked with index_error, retry available via API

#### US-2.3: Selective Indexing Policy
**As a** node operator,
**I want** to control which remote content gets indexed,
**So that** I don't waste resources on unwanted content.

**Acceptance Criteria**:
- [ ] is_indexable(metadata) filter based on content type, size
- [ ] Trust-based policy: auto-index from trusted publishers
- [ ] Manual trigger available for non-trusted content
- [ ] Configurable cache limits with LRU eviction

### Epic 3: Content API (2.8)

#### US-3.1: Publish Content via API
**As an** application developer,
**I want** to publish content via REST API,
**So that** I can integrate publishing into my applications.

**Acceptance Criteria**:
- [ ] POST /api/v1/content/publish accepts multipart file upload
- [ ] Returns: root CID, block count, size, Qdrant point IDs
- [ ] Creates published_content database record
- [ ] Announces to network via GossipSub

#### US-3.2: Retrieve Content via API
**As an** application developer,
**I want** to retrieve content by CID via REST API,
**So that** I can access content programmatically.

**Acceptance Criteria**:
- [ ] GET /api/v1/content/{cid} returns content bytes
- [ ] Attempts local BlockStore first
- [ ] Falls back to network retrieval if not local
- [ ] Returns appropriate error if not found anywhere

#### US-3.3: List Published Content
**As a** node operator,
**I want** to list my published content via API,
**So that** I can see my publication history.

**Acceptance Criteria**:
- [ ] GET /api/v1/content/published returns list
- [ ] Includes: cid, path, size, block_count, published_at
- [ ] Supports pagination (limit, offset)
- [ ] Supports filtering by date range

#### US-3.4: List Discovered Content
**As a** node operator,
**I want** to list discovered content via API,
**So that** I can see what's available on the network.

**Acceptance Criteria**:
- [ ] GET /api/v1/content/discovered returns list
- [ ] Includes: cid, publisher_did, metadata, discovered_at, indexed
- [ ] Supports pagination
- [ ] Supports filtering by indexed status

#### US-3.5: Trigger Remote Indexing
**As a** node operator,
**I want** to trigger indexing of specific remote content,
**So that** I can make it searchable.

**Acceptance Criteria**:
- [ ] POST /api/v1/content/{cid}/index triggers indexing
- [ ] Returns indexing status (success, already_indexed, not_found)
- [ ] Updates remote_content record

#### US-3.6: Find Content Providers
**As an** application developer,
**I want** to find which peers have specific content,
**So that** I can understand content availability.

**Acceptance Criteria**:
- [ ] GET /api/v1/content/{cid}/providers returns provider list
- [ ] Queries Kademlia DHT for providers
- [ ] Returns peer IDs and multiaddresses

### Epic 4: Federated Search (2.9)

#### US-4.1: Search Local Content
**As a** user,
**I want** to search my local content,
**So that** I can find relevant documents.

**Acceptance Criteria**:
- [ ] search_local(query) queries space collections
- [ ] Returns ranked results with scores
- [ ] Includes source document metadata

#### US-4.2: Search Remote Content
**As a** user,
**I want** to search indexed remote content,
**So that** I can find content from the network.

**Acceptance Criteria**:
- [ ] search_remote(query) queries remote-content-idx
- [ ] Returns ranked results with scores
- [ ] Includes publisher information

#### US-4.3: Federated Search
**As a** user,
**I want** to search across both local and remote content,
**So that** I get comprehensive results.

**Acceptance Criteria**:
- [ ] POST /api/v1/search with scope parameter (local/remote/all)
- [ ] Parallel queries to local and remote collections
- [ ] Results merged and ranked (remote penalty 0.9x)
- [ ] Response indicates source (local vs remote)

#### US-4.4: Lazy Content Fetch
**As a** user,
**I want** to access remote search results,
**So that** I can view the actual content.

**Acceptance Criteria**:
- [ ] Search results include cached flag
- [ ] Uncached results can be fetched on demand
- [ ] Fetched content is cached locally

---

## 5. Feature Prioritization

### P0 - Must Have (MVP)

| Feature | User Story | Rationale |
|---------|------------|-----------|
| Database entities | US-1.1, US-1.2 | Foundation for all tracking |
| Content announcement subscription | US-2.1 | Enables discovery |
| Basic remote indexing | US-2.2 | Makes discovery useful |
| POST /api/v1/content/publish | US-3.1 | Core publishing API |
| GET /api/v1/content/{cid} | US-3.2 | Core retrieval API |
| GET /api/v1/content/published | US-3.3 | Publication visibility |
| GET /api/v1/content/discovered | US-3.4 | Discovery visibility |
| POST /api/v1/search (scope=all) | US-4.3 | Federated search |

### P1 - Should Have

| Feature | User Story | Rationale |
|---------|------------|-----------|
| Selective indexing policy | US-2.3 | Resource management |
| POST /api/v1/content/{cid}/index | US-3.5 | Manual indexing control |
| GET /api/v1/content/{cid}/providers | US-3.6 | Provider discovery |
| Lazy content fetch | US-4.4 | UX improvement |

### P2 - Nice to Have

| Feature | User Story | Rationale |
|---------|------------|-----------|
| WebSocket discovery events | - | Real-time updates |
| Advanced cache eviction | US-2.3 | Storage optimization |
| Search result pagination | US-4.3 | Large result sets |

---

## 6. Non-Functional Requirements

### Performance
| Metric | Target | Rationale |
|--------|--------|-----------|
| File publish time (10MB) | < 5 seconds | User experience |
| Content retrieval (1MB) | < 2 seconds | User experience |
| DHT propagation | < 10 seconds | Discovery latency |
| Search latency | < 1 second | Interactive UX |

### Reliability
- Zero data corruption (CID verification 100%)
- All existing tests pass (486+)
- Graceful handling of malformed announcements
- Retry logic for network failures

### Scalability
- Support 10,000+ published content records
- Support 100,000+ discovered content records
- Handle 100+ concurrent API requests

### Security
- Validate all incoming announcements
- Rate limit discovery processing
- No execution of untrusted content

---

## 7. Technical Constraints

### Must Use
- Sea-ORM for database entities (consistency)
- Qdrant for vector storage (existing infrastructure)
- Existing ContentService (integration)
- Existing API patterns (consistency)

### Must Avoid
- Breaking existing tests
- Changing existing API contracts
- Modifying BlockStore interface
- New external dependencies (minimize)

### Branch
- All work on `ipld` branch

---

## 8. Dependencies

### Internal Dependencies
| Component | Required For | Status |
|-----------|--------------|--------|
| ContentService | Publishing pipeline | ✅ Ready |
| BlockStore | Content storage | ✅ Ready |
| NetworkProvider | Content retrieval | ✅ Ready |
| GossipSub | Announcements | ✅ Ready |
| PipelineManager | Indexing | ✅ Ready |
| Qdrant client | Vector search | ✅ Ready |

### External Dependencies
None required (all dependencies already in place)

---

## 9. Success Metrics

### Functional Metrics
| Metric | Target |
|--------|--------|
| Published content tracking | 100% of publications recorded |
| Discovery processing | 100% of valid announcements stored |
| Remote indexing success | 95%+ (excluding network failures) |
| API endpoint coverage | All P0 endpoints functional |
| Federated search | Returns results from both sources |

### Quality Metrics
| Metric | Target |
|--------|--------|
| Test pass rate | 100% |
| New test coverage | 80%+ for new code |
| Zero regressions | All 486 existing tests pass |
| Zero critical bugs | Before completion |

---

## 10. Risks & Mitigations

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| Database migration issues | Medium | High | Test migrations thoroughly |
| Qdrant collection conflicts | Low | Medium | Use unique collection names |
| Network timeout handling | Medium | Medium | Implement proper retry logic |
| Large announcement floods | Low | High | Rate limiting on processing |
| Search result inconsistency | Medium | Low | Clear source labeling |

---

## 11. Out of Scope

Explicitly excluded from this cycle:
- Live network query broadcast (Phase 3.4)
- Advanced score normalization (Phase 3.5)
- Agent-based sync (Phase 4)
- Content encryption (Phase 5)
- Multi-user permissions
- Cross-node collaboration

---

## 12. Glossary

| Term | Definition |
|------|------------|
| CID | Content Identifier - hash-based address for content |
| ContentAnnouncement | GossipSub message announcing newly published content |
| DAG | Directed Acyclic Graph - structure linking content chunks |
| DHT | Distributed Hash Table - for provider discovery |
| GossipSub | Pub/sub protocol for content announcements |
| IPLD | InterPlanetary Linked Data - data model for content |
| Multiaddress | libp2p address format encoding protocol + address (e.g., /ip4/127.0.0.1/tcp/4001) |
| Qdrant | Vector database for semantic search |
| RemoteIndexer | Component that fetches and indexes discovered remote content |
| Sea-ORM | Rust ORM for database operations |
| Space Collection | Qdrant collection for a user's local indexed content |

---

## 13. Appendix

### A. API Endpoint Summary

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | /api/v1/content/publish | Publish content |
| GET | /api/v1/content/{cid} | Retrieve content |
| GET | /api/v1/content/{cid}/providers | Find providers |
| GET | /api/v1/content/published | List publications |
| GET | /api/v1/content/discovered | List discoveries |
| POST | /api/v1/content/{cid}/index | Trigger indexing |
| POST | /api/v1/search | Federated search |

### B. Database Entity Summary

**published_content**
- cid (PK), path, size, block_count
- qdrant_collection, qdrant_point_ids
- published_at

**remote_content**
- cid (PK), publisher_peer_id, publisher_did
- metadata (JSON), discovered_at
- cached, indexed, index_error (nullable)
- qdrant_collection, embedding_count

### C. ContentAnnouncement Schema

```
ContentAnnouncement {
  version: u8,              // Protocol version (1)
  cid: String,              // Root CID of published content
  publisher_peer_id: String,// libp2p peer ID
  publisher_did: String,    // DID of publisher (optional, may be empty)
  content_type: String,     // MIME type (e.g., "text/markdown")
  size: u64,                // Total content size in bytes
  block_count: u32,         // Number of blocks in DAG
  title: String,            // Human-readable title (optional)
  timestamp: u64,           // Unix timestamp of publication
  signature: Vec<u8>,       // Ed25519 signature over fields (optional)
}
```

**Validation Rules:**
- `version` must be 1
- `cid` must be valid CIDv1
- `publisher_peer_id` must match message sender
- `size` must be > 0 and < 1GB (configurable)
- `timestamp` must be within ±5 minutes of current time
