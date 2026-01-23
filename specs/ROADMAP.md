# Flow Network Implementation Roadmap

> **Integrated**: 2026-01-10
> **Status**: Active Development
> **Current Phase**: Phase 2 (Data Sync & Publishing) - Final items

## Priority-Ordered Development Plan

### 📋 Overview

| Attribute | Value |
|-----------|-------|
| Total Timeline | 6-9 months (40 weeks) |
| Phases | 5 major phases + 1 foundation phase |
| Team Size | 3-5 engineers recommended |

---

## 📊 Progress Summary

| Phase | Status | Progress |
|-------|--------|----------|
| Phase 0: Foundation | ✅ Complete | 100% |
| Phase 1: Networking | ✅ Complete | 100% |
| Phase 2: Data Sync | 🔨 In Progress | **~75%** |
| Phase 3: Distributed Search | ⏳ Pending | 0% |
| Phase 4: Agent Framework | ⏳ Pending | 0% |
| Phase 5: Security | ⏳ Pending | 0% |

---

## 🎯 Phase 0: Foundation & Setup ✅ COMPLETE

All items implemented. Development environment ready, architecture approved.

---

## 🌐 Phase 1: Networking Foundation ✅ COMPLETE

**Milestone 1**: ✅ Two nodes can discover each other and exchange messages

Implemented:
- libp2p integration with Kademlia DHT, QUIC, Noise
- mDNS local discovery
- GossipSub messaging
- NetworkManager service
- REST API exposure
- Testing infrastructure

---

## 📦 Phase 2: Data Sync & Publishing 🔨 IN PROGRESS (~75%)

### ✅ Completed Items

#### 2.1 Block Store & Content Addressing ✅ COMPLETE
Location: `src/modules/storage/content/`
- [x] IPLD dependencies (cid, multihash, serde_ipld_dagcbor)
- [x] `ContentId` wrapper type (CIDv1 with SHA2-256) → `cid.rs`
- [x] `Block` struct with CID, data, and DAG links → `block.rs`
- [x] `BlockStore` with RocksDB backend → `block_store.rs`
- [x] Column families: blocks, pins, metadata
- [x] put(), get(), has(), delete(), iter() methods
- [x] Unit tests for CID determinism

#### 2.2 IPLD Data Structures ✅ COMPLETE
Location: `src/modules/storage/content/`
- [x] `ChunkRef` struct (CID, offset, size) → `chunking/`
- [x] `ChunkData` with content bytes
- [x] `DocumentManifest` struct → `manifest.rs`
- [x] `DagBuilder`, `DagReader` → `dag.rs`
- [x] DAG-CBOR serialization
- [x] File chunking (FastCDC, Fixed, Rabin) → `chunking/`
- [x] Tree structure for large files → `manifest.rs`

#### 2.3 Request-Response Protocol ✅ COMPLETE
Location: `src/modules/network/content_transfer/`
- [x] `ContentRequest` enum → `messages.rs`
- [x] `ContentResponse` enum → `messages.rs`
- [x] `ContentCodec` for DAG-CBOR → `codec.rs`
- [x] Protocol ID `/flow/content/1.0.0`
- [x] request_response::Behaviour integration
- [x] Request timeout handling

#### 2.4 Publishing Pipeline ✅ MOSTLY COMPLETE
Location: `src/modules/storage/content/service.rs`
- [x] `ContentService` struct
- [x] Publish workflow → chunk → CID → store
- [x] DAG structure linking chunks
- [x] Integration with indexing Pipeline
- [x] `PublishResult` with stats
- [x] `AnnouncementResult` for DHT

#### 2.6 Content Retrieval ✅ COMPLETE
Location: `src/modules/storage/content/provider.rs`
- [x] `ContentProvider` trait
- [x] `LocalProvider` for local BlockStore
- [x] `NetworkProvider` for remote fetch
- [x] `NetworkProviderConfig`
- [x] Provider selection
- [x] CID integrity verification
- [x] Cache retrieved blocks

### 🔨 Remaining Items

#### 2.5 Database Schema ⏳ PENDING
- [ ] `published_content` entity (Sea-ORM)
  - Fields: cid, path, size, block_count, qdrant_collection, qdrant_point_ids, published_at
- [ ] `remote_content` entity (Sea-ORM)
  - Fields: cid, publisher_peer_id, publisher_did, metadata, discovered_at, cached, indexed, qdrant_collection, embedding_count
- [ ] Sea-ORM migrations
- [ ] Indexes for CID and publisher lookups

#### 2.7 Remote Content Indexing ⏳ PENDING
- [ ] Subscribe to GossipSub ContentAnnouncement events
- [ ] Parse and validate incoming announcements
- [ ] Store discovered content in remote_content table
- [ ] `RemoteIndexer` struct
- [ ] `is_indexable(metadata)` filter
- [ ] `index_remote_content(cid)` → fetch → index → embed
- [ ] Separate Qdrant collection (remote-content-idx)
- [ ] Selective indexing policies
- [ ] Cache management with LRU eviction

#### 2.8 Content API Endpoints ⏳ PENDING
- [ ] `Node::publish_file(path, options)` method
- [ ] `Node::retrieve_content(cid, options)` method
- [ ] `Node::index_remote_content(cid)` method
- [ ] POST /api/v1/content/publish (multipart file upload)
- [ ] GET /api/v1/content/{cid}
- [ ] GET /api/v1/content/{cid}/providers
- [ ] GET /api/v1/content/published
- [ ] GET /api/v1/content/discovered
- [ ] POST /api/v1/content/{cid}/index
- [ ] End-to-end integration tests

#### 2.9 Federated Search ⏳ PENDING
- [ ] `FederatedSearch` struct
- [ ] `search_local(query)` → local space collections
- [ ] `search_remote(query)` → remote-content-idx
- [ ] Merge and rank results
- [ ] Lazy fetch for uncached results
- [ ] POST /api/v1/search with scope option

**Milestone 2**: ⏳ Content can be published and retrieved across nodes

---

## 🔍 Phase 3: Distributed Search (Week 17-22) ⏳ PENDING

### 3.1 SearchRouter Foundation
- [ ] SearchRouter struct
- [ ] SearchScope and SearchResult types
- [ ] Result merging logic
- [ ] Deduplication algorithm
- [ ] Unified scoring system

### 3.2 Local Search Integration
- [ ] Wrap PipelineManager.query_space()
- [ ] Parse Swiftide responses
- [ ] Multi-space query support
- [ ] Query batching optimization
- [ ] Caching for repeated queries

### 3.3 Network Search - Prefetch Model
- [ ] "network-idx" Qdrant collection
- [ ] Store prefetched embeddings
- [ ] Embedding-based search
- [ ] Payload filtering

### 3.4 Network Search - Live Query
- [ ] Query broadcast via GossipSub
- [ ] Query response handler
- [ ] Timeout mechanism (2s)
- [ ] Aggregate responses
- [ ] Handle partial failures

### 3.5 Result Ranking & Merging
- [ ] Cross-source score normalization
- [ ] Relevance penalties (remote = 0.9x local)
- [ ] Deduplicate by content similarity
- [ ] Configurable ranking weights
- [ ] Pagination support

### 3.6 API & Performance
- [ ] Update GET /api/v1/search
- [ ] network=true/false parameter
- [ ] Source breakdown in response
- [ ] Execution time metrics

**Milestone 3**: ⏳ Search returns results from both local and network sources in <2s

---

## 🤖 Phase 4: Agent Framework (Week 23-34) ⏳ PENDING

### 4.1 Core Agent Structure
- [ ] Agent struct with SLRPA components
- [ ] AgentState with goals, beliefs, plans
- [ ] Agent lifecycle (start/stop)
- [ ] State persistence

### 4.2-4.3 SLRPA Framework & Loop
- [ ] Sense, Learn, Reason, Predict, Act traits
- [ ] AgentContext for coordination
- [ ] run_cycle() implementation
- [ ] Error handling and recovery

### 4.4-4.7 Agent Implementations
- [ ] FileWatcher Agent
- [ ] SearchAgent
- [ ] SyncAgent
- [ ] Agent-to-Agent messaging

### 4.8 Agent Management API
- [ ] CRUD endpoints for agents
- [ ] Start/stop controls

**Milestone 4**: ⏳ Agents autonomously manage file indexing and respond to queries

---

## 🔐 Phase 5: Security & Privacy (Week 35-40) ⏳ PENDING

### 5.1-5.3 Security Features
- [ ] Chunk encryption (ChaCha20-Poly1305)
- [ ] UCAN integration
- [ ] Rate limiting
- [ ] Peer reputation
- [ ] Security audit

**Milestone 5**: ⏳ System is production-ready with encryption and access control

---

## 🎯 Recommended Next Build-E2E Cycle

Based on current progress, the highest-impact next cycle is:

### **Option A: Phase 2 Completion** (Recommended)
**Scope**: Items 2.5, 2.7, 2.8, 2.9
**Effort**: 2-3 weeks
**Value**: Completes data sync foundation for all future phases

Work items:
1. Database schema for published/remote content
2. Remote content indexing pipeline
3. Content API endpoints
4. Federated search foundation

### **Option B: Phase 3 Quick Start**
**Scope**: Items 3.1-3.2 (SearchRouter + Local Integration)
**Effort**: 1-2 weeks
**Value**: Improves existing search without network dependency

### **Option C: Agent Foundation**
**Scope**: Items 4.1-4.3 (Core Agent Structure)
**Effort**: 2-3 weeks
**Value**: Enables autonomous workflows

---

## 🔄 Dependencies

```
Phase 0 (Foundation) ✅
    ↓
Phase 1 (Network) ✅
    ↓
Phase 2 (Publishing) ← 75% COMPLETE
    ↓
    ├─→ 2.5 Database Schema
    ├─→ 2.7 Remote Indexing
    ├─→ 2.8 Content API
    └─→ 2.9 Federated Search
    ↓
Phase 3 (Search)
    ↓
Phase 4 (Agents)
    ↓
Phase 5 (Security)
```

---

## 📈 Success Metrics

### Phase 2 Success Criteria
- [ ] File publish time < 5s for 10MB file
- [ ] Content retrieval time < 2s for 1MB
- [ ] DHT provider record propagation < 10s
- [ ] Zero data corruption (CID verification 100%)
