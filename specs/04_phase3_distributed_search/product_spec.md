# Product Specification: Phase 3 - Distributed Search

> **Version**: 1.0
> **Created**: 2026-01-21
> **Status**: Draft
> **Owner**: Flow Network Team

---

## 1. Executive Summary

### Product Vision

Enable Flow users to search the entire decentralized network with a single query, discovering knowledge from peers across the network while maintaining responsive sub-2-second query times.

### Problem Statement

Flow's current search capability (Phase 2) is limited to content users have already encountered:
- Local space indexes (user's own content)
- Pre-indexed remote content (content announced via GossipSub and explicitly indexed)

Users cannot discover content from peers they haven't directly interacted with, creating information silos in a network designed for knowledge sharing.

### Proposed Solution

Implement **Distributed Search** - a hybrid parallel search system that:
1. Queries local indexes for immediate results (~50-100ms)
2. Broadcasts search queries to 15 random connected peers
3. Merges and ranks results from all sources within 500ms
4. Returns comprehensive, deduplicated results in under 2 seconds

### Key Differentiators

| Aspect | Current State | With Distributed Search |
|--------|---------------|------------------------|
| Scope | Local + pre-indexed remote | Entire connected network |
| Discovery | Manual (must encounter content first) | Automatic via live queries |
| Latency | ~200ms (local only) | <2s (network-wide) |
| Privacy | N/A | Opt-in query responses |

---

## 2. Goals and Objectives

### Primary Goals

1. **Network-wide discovery**: Users find relevant content regardless of whether they've encountered the publisher before
2. **Sub-2-second latency**: Maintain responsive UX despite network round-trips
3. **Privacy-preserving**: Node operators control what content is searchable by the network

### Success Metrics

| Metric | Target | Measurement Method |
|--------|--------|-------------------|
| Search latency (scope=all) | <2,000ms p95 | Server-side timing logs |
| Search latency (scope=local) | <200ms p95 | Server-side timing logs |
| Peer response rate | >80% | peers_responded / peers_queried |
| Result relevance | Top-5 match intent | [ASSUMPTION] Manual evaluation |
| Adoption of opt-in | >50% of nodes enable | Configuration telemetry |

### Non-Goals (This Phase)

- Real-time query subscriptions (live updates as new content matches)
- Semantic query expansion or auto-complete
- Peer reputation/reliability tracking (deferred to Phase 5)
- Topic-based query routing (random peer selection for now)
- Search result explanations or provenance tracking

---

## 3. User Personas

### Persona 1: Knowledge Worker (Primary)

**Name**: Alex, Senior Engineer
**Context**: Works in a distributed team across multiple time zones
**Goals**:
- Find technical documentation shared by teammates
- Discover solutions to problems others have solved
- Avoid duplicating work that exists elsewhere in the organization

**Pain Points**:
- Currently must know which peer has relevant content
- Search is limited to personally indexed content
- Information silos between team members

**How Distributed Search Helps**:
- Single query reaches all connected team members' nodes
- Discovers relevant documents without knowing who created them
- Results ranked by relevance, not by source proximity

### Persona 2: Researcher

**Name**: Dr. Sarah, Academic Researcher
**Context**: Studies emerging topics with distributed research community
**Goals**:
- Comprehensive literature discovery
- Find pre-prints and working papers from peers
- Stay current with community knowledge

**Pain Points**:
- Manual process to check each collaborator's shared content
- Misses relevant work from peripheral connections
- No way to search "the network" as a whole

**How Distributed Search Helps**:
- Query reaches 15+ peers per search, recursively expanding reach
- Popularity boost surfaces widely-cited content
- Network results clearly labeled for provenance

### Persona 3: Organization Admin

**Name**: Jordan, IT Administrator
**Context**: Manages Flow deployment for 50-person company
**Goals**:
- Enable knowledge sharing across departments
- Maintain security and privacy controls
- Monitor system health and usage

**Pain Points**:
- Balancing openness with security requirements
- No visibility into cross-team search patterns
- Concern about resource usage from external queries

**How Distributed Search Helps**:
- Opt-in model gives explicit control over query participation
- Per-space configuration for granular sharing policies
- Metrics (peers_queried, elapsed_ms) for monitoring

---

## 4. User Stories and Requirements

### Epic 1: Search Execution

#### US-1.1: Hybrid Parallel Search
**As a** user searching for content,
**I want** my query to search both local indexes and the network simultaneously,
**So that** I get fast local results immediately while network results enrich the response.

**Acceptance Criteria**:
- [ ] Local search begins immediately upon query submission
- [ ] Network broadcast initiates in parallel (not sequential)
- [ ] Results from both sources merge into single response
- [ ] Total latency <2s for scope=all

#### US-1.2: Search Scope Control
**As a** user with specific search needs,
**I want** to control whether my search queries local only, network only, or both,
**So that** I can optimize for speed or comprehensiveness as needed.

**Acceptance Criteria**:
- [ ] API accepts `scope` parameter: `local`, `network`, `all`
- [ ] `scope=local` bypasses network broadcast entirely
- [ ] `scope=network` bypasses local index search
- [ ] `scope=all` is the default behavior

#### US-1.3: Result Limit with Overflow
**As a** user reviewing search results,
**I want** to know if more results are available beyond my requested limit,
**So that** I can request more if the initial results aren't sufficient.

**Acceptance Criteria**:
- [ ] Response includes `more_available` count
- [ ] Results truncated to requested `limit`
- [ ] Overflow count reflects total unique results across all sources

### Epic 2: Network Query Protocol

#### US-2.1: Query Broadcast
**As a** searching node,
**I want** my query to be broadcast to multiple peers,
**So that** I can discover content from across the network.

**Acceptance Criteria**:
- [ ] Query sent to 15 random connected peers
- [ ] Uses GossipSub topic `/flow/search/1.0.0`
- [ ] Message encoded as DAG-CBOR
- [ ] Includes unique query ID for correlation

#### US-2.2: Query Response
**As a** peer receiving a search query,
**I want** to respond with matching results from my local indexes,
**So that** my content can be discovered by the network.

**Acceptance Criteria**:
- [ ] Response includes up to 20 results per peer
- [ ] Each result has: CID, title, score, snippet
- [ ] Response correlates to query via query ID
- [ ] Response sent only if opt-in is enabled

#### US-2.3: Timeout Handling
**As a** searching node,
**I want** queries to timeout after 500ms,
**So that** slow or unresponsive peers don't block my search.

**Acceptance Criteria**:
- [ ] Network query times out at 500ms
- [ ] One retry attempted for failed peers
- [ ] Partial results returned if some peers timeout
- [ ] `peers_responded` reflects actual responses received

### Epic 3: Privacy and Control

#### US-3.1: Opt-in Query Responses
**As a** node operator,
**I want** to explicitly enable or disable responding to network queries,
**So that** I control whether my content is discoverable.

**Acceptance Criteria**:
- [ ] Configuration option to enable/disable query responses
- [ ] Default is disabled (privacy-first)
- [ ] Can be toggled without restart
- [ ] [ASSUMPTION] Per-space granularity in future iteration

#### US-3.2: Query Response Metrics
**As a** node operator,
**I want** visibility into query activity on my node,
**So that** I can monitor usage and detect abuse.

**Acceptance Criteria**:
- [ ] Log incoming queries (query text, source peer, timestamp)
- [ ] Track queries answered vs ignored
- [ ] [ASSUMPTION] Rate limiting in future iteration

### Epic 4: Result Ranking

#### US-4.1: Score Normalization
**As a** user viewing merged results,
**I want** scores from different sources to be comparable,
**So that** ranking reflects true relevance, not source bias.

**Acceptance Criteria**:
- [ ] All scores normalized to 0-1 range
- [ ] Normalization applied per-source before merging
- [ ] Original score preserved in response (`score` field)
- [ ] Adjusted score used for ranking (`adjusted_score` field)

#### US-4.2: Source Penalty
**As a** user preferring local content,
**I want** local results to rank slightly higher than equivalent remote results,
**So that** my own indexed content surfaces first when relevance is similar.

**Acceptance Criteria**:
- [ ] Remote results receive 0.9x score multiplier
- [ ] Penalty applied after normalization
- [ ] Local results unpenalized (1.0x multiplier)

#### US-4.3: Popularity Boost
**As a** user seeking authoritative content,
**I want** content found by multiple peers to rank higher,
**So that** widely-available content surfaces above obscure results.

**Acceptance Criteria**:
- [ ] +0.1 boost per additional source (beyond first)
- [ ] Maximum boost capped at +0.3
- [ ] `sources_count` included in result for transparency

#### US-4.4: Deduplication
**As a** user viewing results,
**I want** duplicate content (same CID) to appear only once,
**So that** results aren't cluttered with redundant entries.

**Acceptance Criteria**:
- [ ] Deduplicate by CID
- [ ] Keep entry with highest adjusted score
- [ ] Aggregate `sources_count` from all occurrences

---

## 5. Feature Specifications

### Feature 1: SearchRouter

**Description**: Central orchestrator for distributed search operations.

**Components**:
- `SearchRouter` struct managing search lifecycle
- `SearchConfig` for timeout, peer count, retry settings
- `SearchScope` enum: `Local`, `Network`, `All`
- Parallel execution engine for local + network queries

**Behavior**:
```
SearchRouter.search(query, scope, limit)
├── If scope includes Local:
│   └── Spawn local_search_task
├── If scope includes Network:
│   ├── Spawn prefetch_search_task (remote-content-idx)
│   └── Spawn live_query_task (GossipSub broadcast)
├── Await all tasks with 500ms timeout
├── Merge results using RankingEngine
└── Return SearchResponse
```

### Feature 2: Live Query Protocol

**Description**: GossipSub-based query broadcast and response handling.

**Message Types**:

```rust
// Broadcast on /flow/search/1.0.0
struct SearchQueryMessage {
    query_id: String,      // UUID for correlation
    query: String,         // Search terms
    limit: u32,            // Max results requested
    requester: PeerId,     // For direct response
}

struct SearchResponseMessage {
    query_id: String,      // Correlation to request
    results: Vec<SearchResult>,
    total_matches: u32,    // Total before limit
}

struct SearchResult {
    cid: String,
    title: Option<String>,
    score: f32,
    snippet: Option<String>,
}
```

**Protocol Flow**:
1. Requester broadcasts `SearchQueryMessage` to topic
2. Peers with opt-in enabled search local indexes
3. Peers send `SearchResponseMessage` directly to requester
4. Requester aggregates responses until timeout

### Feature 3: Result Ranking Engine

**Description**: Merges and ranks results from multiple sources.

**Algorithm**:
```
for each result in all_results:
    1. normalized_score = normalize_to_0_1(result.score, source_min, source_max)
    2. if source == Remote:
           adjusted_score = normalized_score * 0.9
       else:
           adjusted_score = normalized_score
    3. Group by CID

for each CID group:
    1. sources_count = count(unique sources)
    2. popularity_boost = min(0.3, (sources_count - 1) * 0.1)
    3. best_score = max(adjusted_scores in group)
    4. final_score = best_score + popularity_boost

Sort by final_score descending
Truncate to limit
Track overflow count
```

### Feature 4: Opt-in Configuration

**Description**: Node-level control for query response participation.

**Configuration**:
```toml
[search]
# Enable responding to network search queries
respond_to_queries = false  # Default: privacy-first

# Maximum results to return per query
max_results_per_query = 20

# Rate limit (queries per minute) - future
# rate_limit = 60
```

**Runtime Toggle**:
- Configuration reloaded on SIGHUP
- No restart required to enable/disable

---

## 6. API Specifications

### Endpoint: POST /api/v1/search

**Request**:
```json
{
  "query": "string (required)",
  "scope": "local | network | all (default: all)",
  "limit": "number (default: 10, max: 100)"
}
```

**Response**:
```json
{
  "success": true,
  "query": "search terms",
  "scope": "all",
  "results": [
    {
      "cid": "bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi",
      "score": 0.92,
      "adjusted_score": 0.88,
      "source": "local | network",
      "sources_count": 3,
      "title": "Document Title",
      "snippet": "...matching context around search terms...",
      "publisher_peer_id": "12D3KooW..." // Only for network results
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

**Error Responses**:

| Status | Code | Description |
|--------|------|-------------|
| 400 | INVALID_QUERY | Query string empty or too long |
| 400 | INVALID_SCOPE | Scope not one of: local, network, all |
| 503 | NETWORK_UNAVAILABLE | No peers connected (scope includes network) |

---

## 7. UI/UX Specifications

### Search Interface Enhancement

**Current State**: Search box in Space page queries single space only.

**Enhanced State**: Global search accessible from all pages.

#### Global Search Bar

**Location**: Header/navigation area, accessible from any page

**Behavior**:
1. User types query in search bar
2. Dropdown shows scope selector: "My Spaces" / "Network" / "All"
3. Results appear in dedicated search results page
4. Each result shows source badge (Local/Network)

#### Search Results Page

**Layout**:
```
┌─────────────────────────────────────────────────────────────┐
│ Search: "distributed systems"                    [All ▼]   │
├─────────────────────────────────────────────────────────────┤
│ 17 results (5 local, 12 from network) • 487ms              │
│ 23 more available                                           │
├─────────────────────────────────────────────────────────────┤
│ ┌─────────────────────────────────────────────────────────┐│
│ │ [LOCAL] Introduction to Distributed Systems      0.92  ││
│ │ ...consensus algorithms for distributed...              ││
│ │ Found in: My Research Space                             ││
│ └─────────────────────────────────────────────────────────┘│
│ ┌─────────────────────────────────────────────────────────┐│
│ │ [NETWORK ×3] CAP Theorem Explained               0.88  ││
│ │ ...consistency, availability, partition...              ││
│ │ From: 3 peers                                           ││
│ └─────────────────────────────────────────────────────────┘│
│ ...                                                         │
└─────────────────────────────────────────────────────────────┘
```

**Result Card Elements**:
- Source badge: `[LOCAL]` or `[NETWORK ×N]`
- Title (clickable)
- Relevance score
- Snippet with highlighted terms
- Source context (space name for local, peer count for network)

#### Settings: Search Configuration

**Location**: Settings page > Search section

**Options**:
- [ ] Respond to network search queries (opt-in toggle)
- Spaces to include in network responses (multi-select) [FUTURE]

---

## 8. Technical Architecture

### System Components

```
┌─────────────────────────────────────────────────────────────┐
│                      REST API Layer                         │
│                   POST /api/v1/search                       │
└──────────────────────────┬──────────────────────────────────┘
                           │
┌──────────────────────────▼──────────────────────────────────┐
│                      SearchRouter                           │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐ │
│  │ LocalSearch │  │PrefetchSearch│  │   LiveQueryEngine  │ │
│  │  (Qdrant)   │  │(remote-idx) │  │    (GossipSub)     │ │
│  └──────┬──────┘  └──────┬──────┘  └──────────┬──────────┘ │
│         │                │                     │            │
│         └────────────────┼─────────────────────┘            │
│                          ▼                                  │
│                   RankingEngine                             │
│          (normalize, penalty, boost, dedup)                 │
└─────────────────────────────────────────────────────────────┘
                           │
┌──────────────────────────▼──────────────────────────────────┐
│                    Network Layer                            │
│  ┌─────────────────────────────────────────────────────┐   │
│  │              GossipSub /flow/search/1.0.0           │   │
│  │         SearchQueryMessage ←→ SearchResponseMessage  │   │
│  └─────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
```

### Data Flow

```
User Query
    │
    ▼
┌───────────────────┐
│   SearchRouter    │
│                   │
│ spawn_all([       │
│   local_search,   │──────► Qdrant (space-*-idx)
│   prefetch_search,│──────► Qdrant (remote-content-idx)
│   live_query      │──────► GossipSub broadcast
│ ])                │           │
│                   │           ▼
│ await_with_timeout│◄───── Peer responses (500ms)
│                   │
│ merge_results()   │
└────────┬──────────┘
         │
         ▼
    SearchResponse
```

### Performance Budget

| Phase | Target | Budget |
|-------|--------|--------|
| API parsing | <5ms | 5ms |
| Local search | <100ms | 100ms |
| Prefetch search | <100ms | 100ms |
| Network broadcast | <10ms | 10ms |
| Network wait | <500ms | 500ms |
| Result merging | <50ms | 50ms |
| Response serialization | <10ms | 10ms |
| **Total** | **<2000ms** | **775ms + network** |

---

## 9. Data Requirements

### New Data Structures

#### SearchQueryMessage (GossipSub)
```rust
struct SearchQueryMessage {
    query_id: Uuid,
    query: String,
    limit: u32,
    requester_peer_id: PeerId,
    timestamp: DateTime<Utc>,
}
```

#### SearchResponseMessage (GossipSub)
```rust
struct SearchResponseMessage {
    query_id: Uuid,
    responder_peer_id: PeerId,
    results: Vec<NetworkSearchResult>,
    total_matches: u32,
    elapsed_ms: u32,
}

struct NetworkSearchResult {
    cid: String,
    title: Option<String>,
    score: f32,
    snippet: Option<String>,
}
```

### Caching

| Cache | TTL | Eviction | Purpose |
|-------|-----|----------|---------|
| Network results | 15s | LRU | Avoid re-querying for repeated searches |
| Peer responses | 500ms | Query completion | Aggregate during search |

### No New Database Tables

This phase operates on existing infrastructure:
- Qdrant collections: `space-*-idx`, `remote-content-idx`
- GossipSub messaging
- Existing peer connections

---

## 10. Security Considerations

### Threat Model

| Threat | Mitigation |
|--------|------------|
| Query flooding (DoS) | Rate limiting (future), connection limits |
| Information leakage | Opt-in model, per-space controls (future) |
| Result poisoning | Score normalization limits single-peer influence |
| Privacy via queries | Query logs don't persist query text by default |

### Privacy Controls

1. **Opt-in by default**: Nodes don't respond to queries unless explicitly enabled
2. **No query persistence**: Incoming queries are not stored long-term
3. **Source anonymization**: Results show peer count, not specific peer IDs [ASSUMPTION]
4. **Local preference**: 0.9x penalty ensures local content surfaces first

### Rate Limiting [FUTURE]

```toml
[search.rate_limit]
queries_per_minute = 60
burst_size = 10
```

---

## 11. Performance Requirements

### Latency Targets

| Metric | Target | Measurement |
|--------|--------|-------------|
| scope=local p50 | <100ms | API response time |
| scope=local p95 | <200ms | API response time |
| scope=all p50 | <800ms | API response time |
| scope=all p95 | <2000ms | API response time |
| scope=network p95 | <2000ms | API response time |

### Throughput Targets

| Metric | Target |
|--------|--------|
| Concurrent searches | 100 per node |
| Queries handled (opt-in) | 60 per minute |
| Results merged | 300+ per search (15 peers × 20 results) |

### Resource Limits

| Resource | Limit |
|----------|-------|
| Max query length | 1000 characters |
| Max results per request | 100 |
| Max results per peer | 20 |
| Network timeout | 500ms |
| Cache memory | 10MB for result cache |

---

## 12. Testing Strategy

### Unit Tests

| Component | Coverage Target | Key Cases |
|-----------|-----------------|-----------|
| SearchRouter | 90% | Scope handling, timeout, merging |
| RankingEngine | 95% | Normalization, penalty, boost, dedup |
| Message serialization | 95% | DAG-CBOR encode/decode |

### Integration Tests

| Test | Description |
|------|-------------|
| Local-only search | Verify scope=local bypasses network |
| Network-only search | Verify scope=network bypasses local |
| Hybrid search | Verify parallel execution and merging |
| Timeout handling | Verify graceful degradation on slow peers |
| Opt-in disabled | Verify no response when opt-in=false |

### Performance Tests

| Test | Target |
|------|--------|
| Local search latency | <200ms p95 |
| Network search latency | <2000ms p95 |
| Throughput under load | 100 concurrent searches |
| Memory usage | <50MB additional during search |

### End-to-End Tests

| Test | Description |
|------|-------------|
| Multi-node search | 3+ nodes, verify results from all |
| Result deduplication | Same content on multiple nodes |
| Popularity boost | Verify multi-source content ranks higher |

---

## 13. Launch Plan

### Phase 1: Core Implementation (Week 1-2)

- [ ] SearchRouter foundation
- [ ] Local search integration
- [ ] Score normalization and ranking engine
- [ ] Unit tests for core components

### Phase 2: Network Protocol (Week 2-3)

- [ ] SearchQueryMessage / SearchResponseMessage
- [ ] GossipSub topic subscription
- [ ] Request/response correlation
- [ ] Timeout and retry handling
- [ ] Integration tests

### Phase 3: API & Configuration (Week 3)

- [ ] Extended search endpoint
- [ ] Opt-in configuration
- [ ] Response metadata
- [ ] Result caching

### Phase 4: UI Integration (Week 3-4)

- [ ] Global search bar component
- [ ] Search results page
- [ ] Settings toggle for opt-in
- [ ] Source badges and metadata display

### Phase 5: Testing & Polish (Week 4)

- [ ] Performance testing
- [ ] End-to-end tests
- [ ] Documentation
- [ ] Bug fixes

---

## 14. Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Network latency exceeds 2s | Medium | High | Aggressive timeout (500ms), partial results |
| Low opt-in adoption | Medium | Medium | Clear value proposition, easy toggle |
| Result quality varies | Low | Medium | Normalization, popularity boost |
| Query flooding | Low | High | Rate limiting (future), connection limits |
| Complexity in merging | Low | Medium | Extensive unit tests, clear algorithm |

---

## 15. Open Questions

| Question | Status | Resolution |
|----------|--------|------------|
| Per-space opt-in granularity? | Deferred | Future iteration |
| Query text logging policy? | [NEEDS INPUT] | Recommend: log metadata only, not query text |
| Rate limiting parameters? | Deferred | Future iteration based on usage patterns |
| UI placement of global search? | [ASSUMPTION] | Header bar, accessible from all pages |

---

## Appendix A: Glossary

| Term | Definition |
|------|------------|
| CID | Content Identifier - cryptographic hash of content |
| GossipSub | Pub/sub protocol for peer-to-peer messaging |
| Opt-in | Configuration requiring explicit enablement |
| Prefetch | Searching pre-indexed remote content (already discovered) |
| Live Query | Broadcasting query to peers in real-time |
| Scope | Search target: local, network, or all |

## Appendix B: Related Documents

- [Brainstorm](./brainstorm.md) - Initial ideation
- [ROADMAP.md](../ROADMAP.md) - Phase 3 items 3.1-3.6
- [FederatedSearch](../../back-end/node/src/modules/query/federated_search.rs) - Phase 2 implementation
