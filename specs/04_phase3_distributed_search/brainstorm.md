# Phase 3: Distributed Search - Brainstorm

> **Created**: 2026-01-21
> **Status**: Ready for PRD
> **Roadmap Items**: 3.1, 3.2, 3.3, 3.4, 3.5, 3.6

## Problem Statement

Flow's current search (Phase 2) can query local spaces and pre-indexed remote content, but it cannot discover content across the live network in real-time. Users are limited to content they've already encountered and indexed locally.

**The opportunity**: Enable users to search the entire Flow network with a single query, discovering knowledge from peers they haven't directly interacted with, while maintaining responsive <2s query times.

## Core Concept

**One-sentence summary**: Extend Flow's search to query the entire network in real-time by broadcasting queries to connected peers and merging results with local indexes, all within 2 seconds.

**Key insight**: A hybrid approach that runs local and network searches in parallel provides both speed (local results are instant) and breadth (network results provide discovery), without sacrificing either.

## Target Users

| User Type | Need | Benefit |
|-----------|------|---------|
| Knowledge workers | Find information across distributed teams | Discover relevant content from peers' knowledge bases |
| Researchers | Comprehensive literature search | Access to network-wide indexed documents |
| Organizations | Cross-team knowledge sharing | Break down information silos without centralizing data |

## Architecture Decisions

### Search Execution Model: Hybrid Parallel

**Decision**: Execute local index search AND network broadcast simultaneously, merge results as they arrive.

**Rationale**:
- Local results return in ~50-100ms, providing immediate feedback
- Network results stream in over 500ms window
- User sees fast initial results that enrich as network responds
- No wasted time with waterfall approaches

### Network Query Protocol

| Parameter | Value | Rationale |
|-----------|-------|-----------|
| Peers queried | 15 random connected peers | Balance coverage vs latency variance |
| Timeout | 500ms | Aggressive for <2s total target |
| Retry | Once per peer | Simple resilience without complexity |
| Response format | Full results (CID, title, score, snippet) | One round trip, everything needed to display |

### Query Permissions: Opt-in

**Decision**: Nodes must explicitly enable responding to remote search queries.

**Rationale**:
- Privacy-first default aligns with Flow's principles
- Node operators control what they share
- Prevents abuse of nodes as free search infrastructure
- Can be enabled per-space or globally

### Result Merging Strategy

1. **Normalize scores** to 0-1 range per source
2. **Apply source penalty**: Remote results receive 0.9x multiplier (local preference)
3. **Popularity boost**: +0.1 per additional source finding same CID (max +0.3)
4. **Deduplicate** by CID, keeping highest adjusted score
5. **Sort** by final adjusted score descending
6. **Truncate** to requested limit, track overflow count

### Protocol Design

| Component | Choice |
|-----------|--------|
| Transport | GossipSub broadcast on `/flow/search/1.0.0` topic |
| Encoding | DAG-CBOR (consistent with content protocol) |
| Max results per peer | 20 (prevents single peer from dominating) |
| Cache TTL | 15 seconds for network results |

## API Design

### Endpoint Evolution

Extend existing `POST /api/v1/search`:

```json
{
  "query": "search terms",
  "scope": "local" | "network" | "all",
  "limit": 10
}
```

Where:
- `local` = Only search local space indexes
- `network` = Only broadcast to network peers (skip local)
- `all` = Hybrid parallel (default)

### Response Shape

```json
{
  "success": true,
  "query": "search terms",
  "results": [
    {
      "cid": "bafy...",
      "score": 0.92,
      "adjusted_score": 0.85,
      "source": "local" | "network",
      "sources_count": 3,
      "title": "Document Title",
      "snippet": "...matching text..."
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

## Technical Components

### 3.1 SearchRouter Foundation

- `SearchRouter` struct orchestrating local + network search
- `SearchScope` enum: `Local`, `Network`, `All`
- `SearchConfig` with timeout, peer count, retry settings
- Result aggregation and deduplication logic

### 3.2 Local Search Integration

- Wrap existing `PipelineManager.query_space()` calls
- Multi-space parallel query support
- Score normalization to 0-1 range
- Integration with SearchRouter

### 3.3 Network Search - Prefetch Model

- Leverage existing `remote-content-idx` Qdrant collection
- Search pre-indexed remote content first (fast path)
- Include in parallel execution with live query

### 3.4 Network Search - Live Query

- `SearchQueryMessage` broadcast via GossipSub
- `SearchResponseMessage` with results array
- Request/response correlation via query ID
- Timeout handling and partial result acceptance
- Opt-in configuration for responding to queries

### 3.5 Result Ranking & Merging

- Score normalization per source
- Source penalty application (0.9x remote)
- Popularity boost calculation
- Deduplication by CID
- Pagination with overflow tracking

### 3.6 API & Performance

- Extended search endpoint with network scope
- Response metadata (peers queried/responded, timing)
- Result caching (15s TTL)
- Performance metrics and logging

## Success Criteria

| Metric | Target |
|--------|--------|
| Total search latency | <2s for scope=all |
| Local-only latency | <200ms |
| Network response rate | >80% of queried peers respond |
| Result relevance | Top-5 results match user intent |

## Non-Goals (This Phase)

- Persistent query subscriptions (real-time updates)
- Semantic query expansion
- Peer reputation/reliability tracking (deferred)
- Query routing by topic/interest (use random for now)
- Search result explanations/provenance

## Dependencies

- Phase 2 complete: FederatedSearch, remote-content-idx, content API
- GossipSub messaging infrastructure
- Qdrant vector search

## Open Questions (Resolved)

| Question | Resolution |
|----------|------------|
| Prefetch vs live query? | Hybrid - both in parallel |
| Network query timeout? | 500ms |
| How many peers? | 15 random |
| Score merging? | Normalize + penalty + popularity boost |
| Who can query? | Opt-in per node |

## References

- [ROADMAP.md](../ROADMAP.md) - Phase 3 items 3.1-3.6
- [FederatedSearch](../../back-end/node/src/modules/query/federated_search.rs) - Existing implementation
- [GossipSub](../../back-end/node/src/modules/network/gossipsub/) - Messaging infrastructure
