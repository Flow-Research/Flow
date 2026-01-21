# Brainstorm: Phase 2 Completion - Data Sync & Publishing

> **Session Date**: 2026-01-10
> **Status**: Finalized
> **Source**: ROADMAP.md items 2.5, 2.7, 2.8, 2.9

---

## 1. Core Concept

### The One-Liner
Complete the remaining 25% of Phase 2 to enable cross-node content publishing, discovery, and federated search.

### Problem Statement
The Flow network has implemented the core content-addressing infrastructure (BlockStore, IPLD, DAG, ContentService, NetworkProvider) but lacks:
1. **Persistence layer** for tracking published and discovered content
2. **Remote indexing** to make discovered content searchable
3. **API endpoints** to expose content operations to clients
4. **Federated search** to query both local and remote content

Without these pieces, nodes cannot:
- Track what content they've published
- Discover and index remote content
- Search across the network
- Build on top of content sync for agents/collaboration

### Inspiration
This completes the data layer foundation that enables all future features:
- Agent-based content sync (Phase 4)
- Distributed search (Phase 3)
- Collaborative workflows (future)

---

## 2. Target & Context

### Who Benefits
- **Single-node users**: Can track their published content
- **Multi-node deployments**: Can discover and sync content across nodes
- **Future agents**: Will use these APIs for autonomous content management
- **Search users**: Can find content across local and remote sources

### Environment
- Rust backend (existing node crate)
- Sea-ORM for database entities
- Qdrant for vector search
- libp2p/GossipSub for content announcements
- Existing ContentService, BlockStore, NetworkProvider

### Existing Solutions Being Extended
- `ContentService` → Add publishing tracking
- `NetworkProvider` → Add remote indexing
- Qdrant collections → Add remote-content-idx
- REST API → Add content endpoints
- Search pipeline → Add federated capability

---

## 3. Scope & Constraints

### In Scope (This Cycle)

#### 2.5 Database Schema
- `published_content` entity tracking local publications
- `remote_content` entity tracking discovered remote content
- Sea-ORM migrations
- Proper indexes for CID and publisher lookups

#### 2.7 Remote Content Indexing
- GossipSub ContentAnnouncement subscription
- RemoteIndexer struct with fetch → index → embed pipeline
- Separate Qdrant collection (remote-content-idx)
- Selective indexing policies (auto-index trusted, manual others)
- Cache management with configurable limits

#### 2.8 Content API Endpoints
- Node facade methods (publish_file, retrieve_content, index_remote_content)
- REST endpoints:
  - POST /api/v1/content/publish
  - GET /api/v1/content/{cid}
  - GET /api/v1/content/{cid}/providers
  - GET /api/v1/content/published
  - GET /api/v1/content/discovered
  - POST /api/v1/content/{cid}/index

#### 2.9 Federated Search
- FederatedSearch struct
- search_local() for space collections
- search_remote() for remote-content-idx
- Result merging and ranking
- POST /api/v1/search with scope option (local/remote/all)

### Out of Scope (Deferred)
- Live network query broadcast (Phase 3.4)
- Cross-source score normalization (Phase 3.5)
- Agent-based content sync (Phase 4)
- Encryption (Phase 5)

### Constraints
- Must integrate with existing ContentService
- Must use existing Qdrant infrastructure
- Must follow existing API patterns (REST + DTOs)
- Must maintain test coverage (currently 486 tests)
- Branch: `ipld` (current)

---

## 4. Success Criteria

### Milestone: Phase 2 Complete
When a user can:
1. **Publish content** and see it tracked in database
2. **Discover remote content** announced via GossipSub
3. **Index remote content** into searchable collection
4. **Search federated** across local and remote sources
5. **Retrieve content** by CID from local or network

### Technical Metrics
- File publish time < 5s for 10MB file
- Content retrieval time < 2s for 1MB
- DHT provider record propagation < 10s
- Zero data corruption (CID verification 100%)
- All existing tests still pass (486+)

---

## 5. Key Decisions

### Database Design
- Use Sea-ORM entities (consistent with existing codebase)
- Store Qdrant point IDs in database for cross-reference
- Index on CID (primary lookup) and publisher_did (filtering)

### Remote Indexing Strategy
- Separate Qdrant collection to isolate remote content
- Configurable auto-indexing (trust-based policy)
- LRU cache eviction for storage limits

### API Design
- Follow existing REST patterns in `api/rest/`
- Use existing DTO patterns
- Add WebSocket events for discovery notifications

### Search Architecture
- FederatedSearch wraps existing PipelineManager
- Parallel queries to local + remote collections
- Simple score-based merging (remote penalty 0.9x)

---

## 6. References

### Existing Code
- `src/modules/storage/content/` - BlockStore, ContentService, etc.
- `src/modules/network/content_transfer/` - Request-Response protocol
- `src/modules/network/gossipsub/` - GossipSub messaging
- `src/api/rest/` - REST endpoint patterns
- `src/modules/ai/pipeline/` - Indexing pipeline

### Specifications
- `specs/ROADMAP.md` - Full roadmap with item details
- `specs/00_overview_mvp.md` - MVP architecture
- `specs/01_storage_layer.md` - Storage design

---

## 7. Open Questions (Resolved)

| Question | Resolution |
|----------|------------|
| Where to store remote content metadata? | New `remote_content` Sea-ORM entity |
| Separate Qdrant collection for remote? | Yes, `remote-content-idx` collection |
| Auto-index all remote or selective? | Selective with trust-based policy |
| How to handle malformed announcements? | Log warning, drop, don't crash |

---

## 8. Next Steps

1. → Generate Product Specification (product_spec.md)
2. → Generate Technical Specification (technical_spec.md)
3. → Generate MVP Specification with work items
4. → Implement and test
5. → QA validation
