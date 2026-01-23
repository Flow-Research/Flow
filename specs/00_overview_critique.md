Critical Analysis of 00_overview_extended.md
Overall Verdict: Sound architecture, aggressive timeline, some practical concerns

The spec is well-structured and technically coherent. Most elements are implementable. However, there are areas that are impractical, underspecified, or overly optimistic.

---
✅ SOUND AND EXECUTABLE
1. Storage Layer (Section 4.1)
- Content-addressed blocks with IPLD/CID — well-established pattern (IPFS)
- RocksDB for KV — proven, your implementation already works
- Event store with hash chains — standard event sourcing, already implemented
- SeaORM with SQLite/Postgres — already working

2. Network Layer (Section 4.3)  
- libp2p with Kademlia/GossipSub — already implemented and working
- mDNS for local discovery — standard, implemented
- Content transfer protocol — implemented
- Topic structure — sensible design

3. Access & Auth Layer (Section 4.2)
- did:key generation — implemented
- WebAuthn passkeys — implemented
- Basic capability structure — well-defined

4. API Layer (Section 6)
- REST API — standard, partially implemented
- WebSocket — implemented
- MCP integration — straightforward

5. Document Chunking (Section 5.1)
- Strategy per document type — sensible
- AST-aware for code — achievable with tree-sitter
- Hierarchical for markdown — already using text-splitter

---
⚠️ CHALLENGING BUT ACHIEVABLE
1. Knowledge Graph Layer (Section 4.4)
Concern: The spec conflates three distinct systems:
1. Entity/Edge storage (graph database)
2. Vector search (semantic memory)
3. LLM inference

Issue: No concrete implementation path for:
- How entities are stored (SQL? RocksDB? IPLD blocks?)
- How vector indices are persisted
- How inference chains are tracked

Recommendation: Split KG spec into:
- Entity/Edge store (can use SQLite with adjacency lists)
- Vector store (use Qdrant, LanceDB, or in-memory HNSW)
- LLM orchestration (separate module)

2. CRDT Strategy (Section 4.5)
Concern: Mixed strategy creates complexity:
| Document text | Y.Text    | Yrs           |
| KG edges      | OR-Set    | Custom        |
| Agent state   | Event     | Not CRDT      |
Issue: Yrs (Yjs Rust port) integration is non-trivial. Custom CRDTs require significant effort and testing.
Recommendation: Start with event sourcing everywhere, add CRDTs selectively after P0.

3. Capability Delegation Chain (Section 4.2.2)
Concern: 
pub proof_chain: Vec<ContentId>,  // Delegation chain
Issue: Verifying arbitrary-length delegation chains is O(n) and requires fetching all parent capabilities. If a chain is 10 levels deep, you need 10 lookups.

Recommendation: 
- Add chain length limit (e.g., max 5 hops)
- Cache verified chains
- Consider Merkle proofs for efficiency
---

🔴 IMPRACTICAL OR PROBLEMATIC
1. Performance Targets (Section 8.1)
| Target | Reality Check |
|--------|---------------|
| RAG query < 3000ms | ✅ Achievable with local LLM |
| Simple KG query < 50ms | ⚠️ Only with hot cache |
| 1M+ entities (Personal Node) | ❌ Unrealistic for SQLite |
| 100+ documents/minute indexing | ❌ Unrealistic with embedding generation |

Issue: Embedding generation is ~100ms/chunk with local models. A 10-page PDF = ~50 chunks = 5 seconds minimum.

Recommendation: 
- Clarify that "100 docs/min" means text files, not PDFs
- Add queue-based async indexing (which you have)
- Set realistic expectations: 10-20 complex documents/minute

2. Agent WASM Sandbox (Section 4.6.2)
Spec says:
pub struct WasmConfig {
    pub fuel_limit: u64,
    pub memory_limit: usize,
    pub capabilities: Vec<WasmCapability>,
}
Issue: 
- No WASM toolchain for workflow authoring defined
- How do users write WASM workflows? Compile from what language?
- No standard library for WASM to interact with Flow (KG queries, network, etc.)

Recommendation:
- Start with DSL/YAML workflows (no WASM) for MVP
- Add WASM later with a Rust/AssemblyScript SDK
- Define host function bindings before implementing sandbox

3. Trading Agent Example (Section 4.6.5)
pub struct TradingAgentWorkflow {
    pub wallet_capability: Capability,
    pub duration: Duration,
    pub strategy: TradingStrategy,
}
Issue: This implies:
- Wallet integration (which chain? How?)
- Financial risk management
- Regulatory compliance
Reality: This is a multi-year project in itself.
Recommendation: Remove from MVP spec. Replace with simpler examples:
- Summarize folder
- Extract action items
- Monitor directory for changes

4. Federated Queries (Section 4.4.4)
async fn federated_query(&self, spaces: &[SpaceId], query: GraphQuery) 
    -> Result<QueryResult, KgError>;
Issue: 
- How do you query across spaces owned by different users?
- Who runs the query? Where is data aggregated?
- Privacy implications are severe
Reality: True federation requires:
- Distributed query planning
- Secure aggregation (MPC or TEE)
- Trust model for remote spaces

Recommendation: 
- MVP: Only federate across your own spaces (local-only)
- P2: Add peer-to-peer query forwarding with explicit consent
- Remove "federated" from MVP scope

5. Encryption at Rest with Key Rotation (Section 4.1.6)
pub async fn rotate_key(&mut self) -> Result<(), CryptoError>;
Issue: Key rotation means:
1. Generate new key
2. Re-encrypt ALL data in the Space
3. Update all sync peers atomically
4. Revoke old key access
For a 10GB Space, this is hours of work and complex coordination.
Recommendation:
- MVP: No key rotation (revoke by deleting Space)
- P2: Add rotation with background re-encryption queue
---

🚫 POTENTIALLY IMPOSSIBLE OR CONTRADICTORY
1. "All operations produce verifiable proofs" (Principle 1)
Spec claims: 
> Ubiquitous, Verifiable Computing: All operations produce verifiable proofs
Reality: 
- ZK proofs for general computation are not production-ready
- TEE attestation requires specific hardware
- Current implementation has no proof generation
Contradiction: The spec later says "signatures + hashes" is acceptable:
// Supports multiple proof types:
// Signatures + Hashes: Basic integrity check.

Recommendation: 
- Rephrase to "All operations are signed and auditable"
- Defer ZK proofs to future version
- Be explicit that v1 uses hash chains + signatures

2. 36-Week Implementation Timeline (Section 12)
Spec proposes:
- Phase 1 (Solo MVP): 10 weeks
- Phase 2 (Collaboration): 6 weeks  
- Phase 3 (Network): 6 weeks
- Phase 4 (Agents): 8 weeks
- Phase 5 (SDK): 6 weeks
Reality check:
- You already have ~3 months of implementation
- Network layer is mostly done
- But: KG, Agents, SDKs are 0%
Realistic estimate:
| Phase | Spec Estimate | Realistic |
|-------|---------------|-----------|
| Solo MVP | 10 weeks | 12-16 weeks |
| Collaboration | 6 weeks | 10-14 weeks |
| Network | 6 weeks | ✅ Mostly done |
| Agents | 8 weeks | 16-20 weeks |
| SDK | 6 weeks | 8-10 weeks |
Total: 36 weeks (spec) vs 50-70 weeks (realistic)

3. Sybil Attack Mitigation (Section 7.1)
Spec says:
> Sybil attacks: Distributed reputation, weighted trust
Issue: Distributed reputation without a blockchain/consensus is solvable only locally. Each node has its own view.
Reality: Without economic stake (tokens), Sybil resistance in pure P2P is an unsolved problem.
Recommendation:
- Accept that v1 has limited Sybil resistance
- Rely on bootstrap nodes as trust anchors
- Plan for economic stake in Incentive Layer
---
📋 SUMMARY
| Category | Assessment |
|----------|------------|
| Storage Layer | ✅ Sound, mostly implemented |
| Network Layer | ✅ Sound, mostly implemented |
| Auth Layer | ✅ Sound, basic implementation done |
| Knowledge Graph | ⚠️ Underspecified, needs design decisions |
| Sync/CRDTs | ⚠️ Complex, defer CRDTs to P2 |
| Agent Layer | ⚠️ Ambitious, simplify for MVP |
| Federation | 🔴 Impractical for MVP |
| ZK Proofs | 🔴 Not achievable in v1 |
| Timeline | 🔴 ~2x underestimated |
---

Recommendations
1. Reduce MVP scope: Remove federation, ZK proofs, WASM sandbox from P0
2. Sequence dependencies: KG before Agents, Agents before SDK
3. Add concrete KG design: Specify SQLite schema, vector store choice
4. Simplify capability model: Max chain length, cached verification
5. Realistic performance: 10-20 docs/min, not 100
6. Remove trading agent example: Replace with achievable workflows

Bottom line: The spec is a solid vision document but needs a "v1 constraints" section that explicitly defers features to later releases.