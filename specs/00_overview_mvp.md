# Flow MVP: Executable Technical Specification

> **Version**: 0.1.0 (MVP)  
> **Status**: Executable Specification  
> **Last Updated**: January 2026  
> **Realistic Timeline**: 16-20 weeks from current state

This document defines the **Minimum Viable Product** for Flow. It incorporates lessons from the extended specification and explicitly defers ambitious features to future versions.

---

## Table of Contents

1. [MVP Definition](#1-mvp-definition)
2. [What's In vs What's Out](#2-whats-in-vs-whats-out)
3. [Architecture (MVP Scope)](#3-architecture-mvp-scope)
4. [Storage Layer](#4-storage-layer)
5. [Access & Auth Layer](#5-access--auth-layer)
6. [Network Layer](#6-network-layer)
7. [Knowledge Graph Layer](#7-knowledge-graph-layer)
8. [Indexing Pipeline](#8-indexing-pipeline)
9. [Query System](#9-query-system)
10. [API Layer](#10-api-layer)
11. [Web UI (MVP)](#11-web-ui-mvp)
12. [Security Model (MVP)](#12-security-model-mvp)
13. [Performance Targets (Realistic)](#13-performance-targets-realistic)
14. [Testing Requirements](#14-testing-requirements)
15. [Implementation Plan](#15-implementation-plan)
16. [Deferred Features (v1.1+)](#16-deferred-features-v11)

---

## 1. MVP Definition

### 1.1 The One-Liner

**MVP Goal**: A single user can create a Space, index documents, build a Knowledge Graph, and query it via AI.

### 1.2 Success Criteria

The MVP is complete when a user can:

1. **Authenticate** with a passkey (WebAuthn)
2. **Create a Space** pointing to a local directory
3. **Index documents** (text, markdown, code, PDF)
4. **See extracted entities** in a Knowledge Graph
5. **Query the Space** using natural language
6. **Get answers** with source citations

### 1.3 What MVP is NOT

- Multi-user collaboration (v1.1)
- Real-time sync between devices (v1.1)
- Autonomous agent workflows (v1.2)
- Cross-node content sharing (v1.1)
- Mobile application (v1.1)
- SDK for developers (v1.2)

---

## 2. What's In vs What's Out

### 2.1 In Scope (MVP)

| Component | Status | Notes |
|-----------|--------|-------|
| Passkey authentication | ✅ Implemented | Polish needed |
| Space creation | ✅ Implemented | Polish needed |
| Document indexing | ✅ Implemented | Extend parsers |
| Content-addressed storage | ✅ Implemented | Stable |
| Event sourcing | ✅ Implemented | Stable |
| P2P networking (local) | ✅ Implemented | mDNS works |
| Knowledge Graph (basic) | 🔨 Build | New module |
| Entity extraction | 🔨 Build | LLM-based |
| Vector search | 🔨 Build | Integration needed |
| RAG query system | 🔨 Build | Pipeline exists |
| Web UI (Space browser) | 🔨 Build | React frontend |
| Web UI (Chat interface) | 🔨 Build | React frontend |

### 2.2 Explicitly Out of Scope (MVP)

| Feature | Reason | Target Version |
|---------|--------|----------------|
| Multi-user collaboration | Requires CRDT integration | v1.1 |
| Real-time sync | Requires collaboration | v1.1 |
| Cross-node networking | Bootstrap infra needed | v1.1 |
| WASM agent sandbox | Complex, no toolchain | v1.2 |
| Federated queries | Privacy/security concerns | v2.0 |
| Key rotation | Re-encryption complexity | v1.2 |
| Mobile app | Focus on web first | v1.1 |
| TypeScript SDK | API must stabilize first | v1.2 |
| Python SDK | After TypeScript | v1.2 |
| ZK proofs | Not production ready | v2.0+ |
| Distributed reputation | Requires economic stake | v2.0+ |
| Trading/crypto workflows | Out of scope entirely | Never (MVP) |

### 2.3 V1 Constraints (Explicit Limitations)

These are **intentional limitations** for MVP:

1. **Single-user only**: No sharing, no collaboration
2. **Single-device only**: No sync between devices
3. **Local network only**: mDNS discovery, no internet P2P
4. **No key rotation**: Delete Space to revoke access
5. **No delegation chains**: Direct capabilities only
6. **English UI only**: Content in any language
7. **Local LLM only**: No cloud AI by default
8. **SQLite only**: No Postgres in MVP

---

## 3. Architecture (MVP Scope)

### 3.1 Simplified Stack

```
┌─────────────────────────────────────────────────────────────┐
│                     Web UI (React)                          │
│         Space Browser | Chat | Search | Settings            │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                      API Layer                              │
│              REST (Axum) | WebSocket                        │
└─────────────────────────────────────────────────────────────┘
                              │
        ┌─────────────────────┼─────────────────────┐
        ▼                     ▼                     ▼
┌───────────────┐   ┌─────────────────┐   ┌───────────────────┐
│   Auth        │   │  Knowledge      │   │   Query           │
│   (WebAuthn)  │   │  Graph          │   │   System          │
└───────────────┘   └─────────────────┘   └───────────────────┘
        │                     │                     │
        └─────────────────────┼─────────────────────┘
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                    Storage Layer                            │
│     SQLite (Entities) | RocksDB (KV) | Vector Store         │
└─────────────────────────────────────────────────────────────┘
```

### 3.2 Module Structure (MVP)

```
back-end/node/src/
├── main.rs                 # Entry point
├── runner.rs               # Bootstrap
├── bootstrap/              # Config, DID generation (EXISTS)
├── api/
│   ├── servers/           # REST, WebSocket (EXISTS)
│   ├── dto/               # DTOs (EXISTS)
│   └── node.rs            # API facade (EXISTS)
└── modules/
    ├── storage/           # KV, blocks (EXISTS)
    ├── ssi/               # WebAuthn (EXISTS)
    ├── network/           # libp2p (EXISTS - disable for MVP UI)
    ├── ai/                # Indexing pipeline (EXISTS)
    ├── knowledge/         # NEW: Knowledge Graph
    │   ├── mod.rs
    │   ├── entity.rs      # Entity CRUD
    │   ├── edge.rs        # Edge CRUD
    │   ├── extraction.rs  # Entity extraction
    │   └── vector.rs      # Vector search
    ├── query/             # NEW: Query system
    │   ├── mod.rs
    │   ├── rag.rs         # RAG pipeline
    │   └── search.rs      # Full-text + semantic
    └── space.rs           # Space management (EXISTS)
```

---

## 4. Storage Layer

### 4.1 Current State (Keep As-Is)

The storage layer is **implemented and stable**:

| Component | Implementation | Status |
|-----------|----------------|--------|
| Block Store | RocksDB | ✅ Stable |
| KV Store | RocksDB | ✅ Stable |
| Event Store | RocksDB + hash chains | ✅ Stable |
| SQL Database | SQLite via SeaORM | ✅ Stable |
| Content Addressing | CID/Multihash | ✅ Stable |

### 4.2 MVP Additions

#### 4.2.1 Knowledge Graph Tables (SQLite)

```sql
-- Entity storage (simple relational, not graph DB)
-- NOTE: Entity embeddings stay in SQLite (unlike chunks) because:
--   - Entity count is small (~100K vs 1.5M chunks)
--   - Entity embeddings are optional (for similarity search)
--   - Can optimize later if needed
CREATE TABLE kg_entities (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    cid TEXT UNIQUE NOT NULL,           -- Content ID
    space_id INTEGER NOT NULL REFERENCES spaces(id),
    entity_type TEXT NOT NULL,          -- 'Document', 'Person', 'Concept', etc.
    name TEXT NOT NULL,                 -- Display name
    properties JSON,                    -- Flexible properties
    embedding BLOB,                     -- Optional: for entity similarity search
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    created_by TEXT NOT NULL,           -- DID
    source_chunk_id TEXT,               -- Where this was extracted from
    FOREIGN KEY (space_id) REFERENCES spaces(id)
);

CREATE INDEX idx_entities_space ON kg_entities(space_id);
CREATE INDEX idx_entities_type ON kg_entities(entity_type);
CREATE INDEX idx_entities_name ON kg_entities(name);

-- Edge storage (adjacency list)
CREATE TABLE kg_edges (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    cid TEXT UNIQUE NOT NULL,
    space_id INTEGER NOT NULL,
    edge_type TEXT NOT NULL,            -- 'mentions', 'created_by', etc.
    source_id INTEGER NOT NULL REFERENCES kg_entities(id),
    target_id INTEGER NOT NULL REFERENCES kg_entities(id),
    properties JSON,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    created_by TEXT NOT NULL,
    FOREIGN KEY (space_id) REFERENCES spaces(id),
    FOREIGN KEY (source_id) REFERENCES kg_entities(id),
    FOREIGN KEY (target_id) REFERENCES kg_entities(id)
);

CREATE INDEX idx_edges_source ON kg_edges(source_id);
CREATE INDEX idx_edges_target ON kg_edges(target_id);
CREATE INDEX idx_edges_type ON kg_edges(edge_type);

-- Document chunks - METADATA ONLY (content stored in RocksDB)
CREATE TABLE chunks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    cid TEXT UNIQUE NOT NULL,           -- Reference to content in RocksDB
    document_id INTEGER NOT NULL,       -- Reference to kg_entities
    space_id INTEGER NOT NULL,
    position_start INTEGER,
    position_end INTEGER,
    char_count INTEGER,                 -- For display without fetching content
    metadata JSON,                      -- Small structured data only
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (document_id) REFERENCES kg_entities(id),
    FOREIGN KEY (space_id) REFERENCES spaces(id)
);

-- NOTE: content and embeddings are NOT in SQLite
-- They are stored in RocksDB, keyed by CID

CREATE INDEX idx_chunks_document ON chunks(document_id);
CREATE INDEX idx_chunks_space ON chunks(space_id);
```

#### 4.2.2 Chunk Storage Architecture

**Problem**: Storing 1.5M chunks with embeddings in SQLite doesn't scale.

| Data | Size per chunk | Total (1.5M chunks) |
|------|----------------|---------------------|
| Content (text) | ~2KB avg | 3GB |
| Embedding (768 floats) | 3KB | 4.5GB |
| Metadata | ~200 bytes | 300MB |

**Solution**: Split storage by data type:

```
┌─────────────────────────────────────────────────────────────────┐
│                        Chunk Storage                            │
├─────────────────────┬─────────────────────┬─────────────────────┤
│      SQLite         │      RocksDB        │    Disk + Memory    │
│   (Metadata only)   │   (Content + Emb)   │   (HNSW Index)      │
├─────────────────────┼─────────────────────┼─────────────────────┤
│ - chunk_id          │ - Full text content │ - Serialized index  │
│ - document_id       │ - Embedding vector  │ - Loaded to memory  │
│ - space_id          │ - Keyed by CID      │ - Fast ANN search   │
│ - position          │                     │                     │
│ - cid (reference)   │                     │                     │
└─────────────────────┴─────────────────────┴─────────────────────┘
```

#### 4.2.3 RocksDB Chunk Content Store

```rust
/// Chunk content stored in RocksDB (not SQLite)
#[derive(Serialize, Deserialize)]
pub struct ChunkContent {
    pub text: String,           // Full text content
    pub embedding: Vec<f32>,    // 768 floats
}

impl KvStore {
    /// Store chunk content by CID
    pub fn store_chunk(&self, cid: &ContentId, content: &ChunkContent) -> Result<(), KvError> {
        let key = format!("chunk:{}", cid);
        let value = serde_cbor::to_vec(content)?;  // CBOR: compact binary, already a dependency
        self.put(key.as_bytes(), &value)
    }
    
    /// Get chunk content by CID
    pub fn get_chunk(&self, cid: &ContentId) -> Result<Option<ChunkContent>, KvError> {
        let key = format!("chunk:{}", cid);
        match self.get(key.as_bytes())? {
            Some(bytes) => Ok(Some(serde_cbor::from_slice(&bytes)?)),
            None => Ok(None),
        }
    }
    
    /// Batch get chunks (for search results)
    pub fn get_chunks(&self, cids: &[ContentId]) -> Result<Vec<ChunkContent>, KvError> {
        cids.iter()
            .filter_map(|cid| self.get_chunk(cid).ok().flatten())
            .collect()
    }
}
```

#### 4.2.4 Vector Index (HNSW)

```rust
pub struct VectorIndex {
    /// In-memory HNSW index
    index: HnswIndex<f32>,
    
    /// Maps HNSW internal ID -> chunk CID
    id_to_cid: HashMap<usize, ContentId>,
    
    /// Maps chunk CID -> HNSW internal ID
    cid_to_id: HashMap<ContentId, usize>,
    
    /// Path to serialized index
    index_path: PathBuf,
    
    /// Embedding dimensions
    dimensions: usize,  // 768 for nomic-embed-text
}

impl VectorIndex {
    /// Load index from disk (fast startup)
    pub fn load(path: &Path) -> Result<Self, VectorError> {
        if path.exists() {
            let bytes = std::fs::read(path)?;
            let (index, id_to_cid) = serde_cbor::from_slice(&bytes)?;
            let cid_to_id = id_to_cid.iter()
                .map(|(k, v)| (v.clone(), *k))
                .collect();
            Ok(Self { index, id_to_cid, cid_to_id, index_path: path.to_owned(), dimensions: 768 })
        } else {
            Ok(Self::new(path))
        }
    }
    
    /// Rebuild index from RocksDB (slow, only when index missing/corrupt)
    pub async fn rebuild_from_store(&mut self, kv: &impl KvStore) -> Result<(), VectorError> {
        tracing::info!("Rebuilding vector index from RocksDB...");
        
        let chunks = kv.scan_prefix(b"chunk:")?;
        for (key, value) in chunks {
            let cid = ContentId::from_key(&key)?;
            let content: ChunkContent = serde_cbor::from_slice(&value)?;
            self.add_internal(&cid, &content.embedding)?;
        }
        
        self.save()?;
        tracing::info!("Vector index rebuilt with {} vectors", self.id_to_cid.len());
        Ok(())
    }
    
    /// Add embedding to index
    pub fn add(&mut self, cid: &ContentId, embedding: &[f32]) -> Result<(), VectorError> {
        self.add_internal(cid, embedding)?;
        // Periodic save (every 1000 additions)
        if self.id_to_cid.len() % 1000 == 0 {
            self.save()?;
        }
        Ok(())
    }
    
    /// Search k nearest neighbors
    pub fn search(&self, query: &[f32], k: usize) -> Vec<(ContentId, f32)> {
        self.index
            .search(query, k)
            .into_iter()
            .filter_map(|(id, score)| {
                self.id_to_cid.get(&id).map(|cid| (cid.clone(), score))
            })
            .collect()
    }
    
    /// Save index to disk
    pub fn save(&self) -> Result<(), VectorError> {
        let bytes = serde_cbor::to_vec(&(&self.index, &self.id_to_cid))?;
        std::fs::write(&self.index_path, bytes)?;
        Ok(())
    }
}
```

#### 4.2.5 Query Flow (Putting It Together)

```rust
pub async fn search_chunks(
    &self,
    space_id: SpaceId,
    query_embedding: &[f32],
    limit: usize,
) -> Result<Vec<ChunkSearchResult>, SearchError> {
    // 1. Vector search in memory (fast: <10ms)
    let similar = self.vector_index.search(query_embedding, limit * 2);
    
    // 2. Get CIDs that belong to this space (filter)
    let cids: Vec<ContentId> = similar.iter().map(|(cid, _)| cid.clone()).collect();
    
    // 3. Fetch metadata from SQLite to filter by space
    let metadata: Vec<ChunkMetadata> = sqlx::query_as(
        "SELECT * FROM chunks WHERE cid IN (?) AND space_id = ?"
    )
    .bind(&cids)
    .bind(space_id)
    .fetch_all(&self.db)
    .await?;
    
    // 4. Fetch content from RocksDB (only for results we'll return)
    let result_cids: Vec<_> = metadata.iter().take(limit).map(|m| m.cid.clone()).collect();
    let contents = self.kv.get_chunks(&result_cids)?;
    
    // 5. Combine metadata + content + scores
    Ok(metadata.into_iter()
        .take(limit)
        .zip(contents)
        .zip(similar.iter().map(|(_, score)| *score))
        .map(|((meta, content), score)| ChunkSearchResult {
            cid: meta.cid,
            document_id: meta.document_id,
            content: content.text,
            score,
            position: (meta.position_start, meta.position_end),
        })
        .collect())
}
```

**Why this architecture?**
- SQLite stays small and fast (~300MB for metadata)
- RocksDB handles large binary data efficiently (~8GB)
- HNSW index serialized to disk = fast startup (load, don't rebuild)
- Memory usage bounded (index only, not all content)

**Library choice**: Use `hnsw_rs` or `instant-distance` crate (pure Rust, serializable).

---

## 5. Access & Auth Layer

### 5.1 Current State (Keep As-Is)

| Component | Implementation | Status |
|-----------|----------------|--------|
| DID generation | did:key (Ed25519) | ✅ Stable |
| Passkey registration | webauthn-rs | ✅ Stable |
| Passkey authentication | webauthn-rs | ✅ Stable |
| Challenge management | In-memory DashMap | ✅ Stable |

### 5.2 MVP Simplifications

#### 5.2.1 No Capability Delegation

MVP has **no delegation chains**. Permissions are direct:

```rust
// MVP: Simple permission check
pub struct Permission {
    pub user_did: Did,
    pub space_id: SpaceId,
    pub actions: Vec<Action>,
}

pub enum Action {
    Read,
    Write,
    Query,
    Delete,
}

// NO delegation chains, NO capabilities, NO VCs in MVP
// Just: "Does this user own this space?"
impl Node {
    pub fn check_permission(&self, user: &Did, space: &SpaceId, action: Action) -> bool {
        // MVP: Only owner has access
        self.is_space_owner(user, space)
    }
}
```

#### 5.2.2 No Encryption at Rest (MVP)

**MVP Decision**: Defer per-Space encryption to v1.1.

Rationale:
- Secure Enclave integration is platform-specific
- Key management adds significant complexity
- OS-level encryption (FileVault, BitLocker) provides baseline

**v1.1 Plan**: Add optional encryption with passkey-derived keys.

---

## 6. Network Layer

### 6.1 Current State

The network layer is **implemented** but not needed for MVP:

| Component | Implementation | Status |
|-----------|----------------|--------|
| libp2p Swarm | TCP + QUIC | ✅ Implemented |
| Kademlia DHT | Peer discovery | ✅ Implemented |
| GossipSub | Pub/sub messaging | ✅ Implemented |
| mDNS | Local discovery | ✅ Implemented |
| Content Transfer | Block exchange | ✅ Implemented |

### 6.2 MVP Strategy

**Decision**: Network layer is **already implemented and stays as-is**.

The existing network implementation works. No changes needed for MVP:
- Local peer discovery via mDNS works
- Bootstrap peers can be configured
- GossipSub messaging works
- Content transfer protocol works

**MVP Usage**: 
- Network starts automatically (existing behavior)
- Single-user won't have peers to connect to, which is fine
- Foundation is ready for v1.1 multi-device sync

**No new config flags or feature toggles** - existing `NetworkConfig` from `bootstrap/config.rs` is sufficient.

---

## 7. Knowledge Graph Layer

### 7.1 Overview

The Knowledge Graph is **new for MVP**. It has three components:

1. **Entity Store**: SQLite-backed entity/edge storage
2. **Chunk Store**: SQLite (metadata) + RocksDB (content/embeddings) + HNSW (vector index)
3. **Extraction Pipeline**: LLM-based entity extraction

### 7.2 Entity Types (MVP Schema)

```rust
/// MVP: Fixed set of entity types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EntityType {
    Document,   // Indexed file
    Person,     // Named person
    Concept,    // Abstract concept/topic
    Date,       // Date/time reference
    Location,   // Place name
}

impl EntityType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "document" => Some(Self::Document),
            "person" => Some(Self::Person),
            "concept" => Some(Self::Concept),
            "date" => Some(Self::Date),
            "location" => Some(Self::Location),
            _ => None,
        }
    }
}
```

**Why limited types?**
- Simpler extraction prompts
- More reliable LLM output
- Extensible in v1.1

### 7.3 Edge Types (MVP Schema)

```rust
/// MVP: Fixed set of edge types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EdgeType {
    Mentions,       // Document -> Any (extracted reference)
    CreatedBy,      // Document -> Person (authorship)
    RelatedTo,      // Any -> Any (generic relationship)
    PartOf,         // Any -> Any (hierarchy)
    OccurredOn,     // Any -> Date (temporal)
    LocatedIn,      // Any -> Location (spatial)
}
```

### 7.4 Entity Store Implementation

```rust
pub struct EntityStore {
    db: DatabaseConnection,
}

impl EntityStore {
    /// Create a new entity
    pub async fn create(&self, entity: CreateEntity) -> Result<Entity, KgError> {
        // 1. Generate CID from content
        let cid = ContentId::from_json(&entity)?;
        
        // 2. Insert into SQLite
        let model = kg_entity::ActiveModel {
            cid: Set(cid.to_string()),
            space_id: Set(entity.space_id),
            entity_type: Set(entity.entity_type.to_string()),
            name: Set(entity.name),
            properties: Set(Some(serde_json::to_value(&entity.properties)?)),
            embedding: Set(None),  // Added later
            created_by: Set(entity.created_by.to_string()),
            source_chunk_id: Set(entity.source_chunk_id),
            ..Default::default()
        };
        
        let result = model.insert(&self.db).await?;
        Ok(result.into())
    }
    
    /// Get entity by ID
    pub async fn get(&self, id: i64) -> Result<Option<Entity>, KgError>;
    
    /// Get entity by CID
    pub async fn get_by_cid(&self, cid: &ContentId) -> Result<Option<Entity>, KgError>;
    
    /// List entities in a space
    pub async fn list(&self, space_id: SpaceId, filter: EntityFilter) 
        -> Result<Vec<Entity>, KgError>;
    
    /// Get edges for an entity
    pub async fn get_edges(&self, entity_id: i64, direction: Direction) 
        -> Result<Vec<Edge>, KgError> {
        match direction {
            Direction::Outgoing => {
                kg_edge::Entity::find()
                    .filter(kg_edge::Column::SourceId.eq(entity_id))
                    .all(&self.db)
                    .await
                    .map(|edges| edges.into_iter().map(Into::into).collect())
            }
            Direction::Incoming => {
                kg_edge::Entity::find()
                    .filter(kg_edge::Column::TargetId.eq(entity_id))
                    .all(&self.db)
                    .await
                    .map(|edges| edges.into_iter().map(Into::into).collect())
            }
            Direction::Both => {
                let out = self.get_edges(entity_id, Direction::Outgoing).await?;
                let in_ = self.get_edges(entity_id, Direction::Incoming).await?;
                Ok([out, in_].concat())
            }
        }
    }
    
    /// Simple traversal (1-2 hops only in MVP)
    pub async fn traverse(
        &self,
        start: i64,
        edge_type: Option<EdgeType>,
        max_depth: usize,  // MVP: max 2
    ) -> Result<Vec<TraversalResult>, KgError> {
        if max_depth > 2 {
            return Err(KgError::MaxDepthExceeded);
        }
        // BFS traversal
        // ...
    }
}
```

### 7.5 Entity Extraction

```rust
pub struct EntityExtractor {
    llm: LanguageModel,
}

impl EntityExtractor {
    /// Extract entities from a chunk of text
    pub async fn extract(&self, chunk: &Chunk) -> Result<Vec<ExtractedEntity>, ExtractionError> {
        let prompt = self.build_extraction_prompt(chunk);
        let response = self.llm.generate(&prompt).await?;
        self.parse_extraction_response(&response)
    }
    
    fn build_extraction_prompt(&self, chunk: &Chunk) -> String {
        format!(r#"
Extract entities from the following text. Return JSON array.

Entity types: Person, Concept, Date, Location
For each entity, provide: type, name, confidence (0-1)

Text:
{}

Response format:
[
  {{"type": "Person", "name": "John Smith", "confidence": 0.9}},
  {{"type": "Concept", "name": "machine learning", "confidence": 0.8}}
]

Entities:
"#, chunk.content)
    }
    
    fn parse_extraction_response(&self, response: &str) -> Result<Vec<ExtractedEntity>, ExtractionError> {
        // Parse JSON, filter by confidence threshold (0.7)
        let entities: Vec<ExtractedEntity> = serde_json::from_str(response)?;
        Ok(entities.into_iter().filter(|e| e.confidence >= 0.7).collect())
    }
}
```

**Extraction flow**:
1. Document is chunked
2. Each chunk is processed by LLM
3. Extracted entities are deduplicated (by name + type)
4. Entities are stored with source_chunk_id
5. Edges are created: Document --mentions--> Entity

---

## 8. Indexing Pipeline

### 8.1 Current State

The indexing pipeline **exists** via Swiftide:

```rust
// Existing in modules/ai/pipeline.rs
pub struct Pipeline {
    pub space_id: i32,
    pub space_key: String,
    pub location: String,
    pub config: IndexingConfig,
    // ...
}
```

### 8.2 MVP Enhancements

#### 8.2.1 Document Type Support

| Type | Parser | Chunking | Status |
|------|--------|----------|--------|
| Plain text (.txt) | Native | Semantic | ✅ Works |
| Markdown (.md) | Native | Hierarchical | ✅ Works |
| Code (.rs, .ts, .py, etc.) | tree-sitter | AST-aware | 🔨 Enhance |
| PDF (.pdf) | pdf-extract | Smart | ✅ Works |
| Office (.docx) | Apache Tika (external) | Smart | 🔨 Add |

#### 8.2.2 Enhanced Pipeline

```rust
impl Pipeline {
    pub async fn build_indexing_pipeline(&self) -> Result<IndexingPipeline, PipelineError> {
        // Existing Swiftide pipeline + KG integration
        
        let pipeline = swiftide::indexing::Pipeline::new()
            // 1. Load files from directory
            .with_loader(FileSystemLoader::new(&self.location))
            
            // 2. Parse by document type
            .with_transformer(DocumentTypeRouter::new())
            
            // 3. Chunk documents
            .with_transformer(SmartChunker::new(&self.config))
            
            // 4. Generate embeddings
            .with_transformer(EmbeddingGenerator::new(&self.config))
            
            // 5. NEW: Extract entities
            .with_transformer(EntityExtractor::new(&self.config))
            
            // 6. Store chunks
            .with_storage(ChunkStorage::new(&self.db))
            
            // 7. NEW: Store entities and edges
            .with_storage(KnowledgeGraphStorage::new(&self.entity_store));
        
        Ok(pipeline)
    }
}
```

#### 8.2.3 Realistic Performance

| Document Type | Chunks/Doc | Time/Chunk | Docs/Minute |
|---------------|------------|------------|-------------|
| Text (1 page) | 2-3 | 150ms | 20-30 |
| Markdown (10 pages) | 20-30 | 150ms | 2-3 |
| Code (500 lines) | 10-15 | 150ms | 4-6 |
| PDF (10 pages) | 30-50 | 200ms | 1-2 |

**Realistic throughput**: 10-20 complex documents/minute.

**Why?**
- Embedding generation: ~100ms/chunk (local model)
- Entity extraction: ~50ms/chunk (cached LLM)
- Storage: ~10ms/chunk
- Overhead: ~40ms/chunk

---

## 9. Query System

### 9.1 RAG Pipeline

```rust
pub struct QueryPipeline {
    /// Vector index for semantic search (in-memory HNSW)
    vector_index: Arc<VectorIndex>,
    
    /// Chunk content store (RocksDB)
    kv_store: Arc<dyn KvStore>,
    
    /// Chunk metadata store (SQLite)
    db: DatabaseConnection,
    
    /// Entity store for KG queries
    entity_store: Arc<EntityStore>,
    
    /// LLM for answer generation
    llm: LanguageModel,
}

impl QueryPipeline {
    pub async fn query(&self, space_id: SpaceId, query: &str) -> Result<QueryResult, QueryError> {
        // 1. Generate query embedding
        let query_embedding = self.embed(query).await?;
        
        // 2. Semantic search for relevant chunks (uses architecture from 4.2.5)
        let relevant_chunks = self
            .search_chunks(space_id, &query_embedding, 10)
            .await?;
        
        // 3. Get entities mentioned in those chunks
        let entities = self.get_entities_from_chunks(&relevant_chunks).await?;
        
        // 4. Expand context with related entities (1 hop)
        let expanded_entities = self.expand_entities(&entities, 1).await?;
        
        // 5. Build context
        let context = self.build_context(&relevant_chunks, &expanded_entities);
        
        // 6. Generate answer
        let answer = self.generate_answer(query, &context).await?;
        
        // 7. Return with sources
        Ok(QueryResult {
            answer,
            sources: relevant_chunks.iter().map(|c| c.into()).collect(),
            entities: entities.iter().map(|e| e.into()).collect(),
        })
    }
    
    async fn generate_answer(&self, query: &str, context: &str) -> Result<String, QueryError> {
        let prompt = format!(r#"
Answer the question based on the provided context. 
If the answer is not in the context, say "I don't have enough information."
Cite sources when possible.

Context:
{}

Question: {}

Answer:
"#, context, query);
        
        self.llm.generate(&prompt).await
    }
}
```

### 9.2 Search Types

#### 9.2.1 Semantic Search (Primary)

```rust
/// Vector similarity search
/// Uses the chunk storage architecture from Section 4.2.5
pub async fn semantic_search(
    &self,
    space_id: SpaceId,
    query: &str,
    limit: usize,
) -> Result<Vec<SearchResult>, SearchError> {
    // 1. Generate query embedding
    let embedding = self.embed(query).await?;
    
    // 2. Search vector index (in-memory HNSW)
    let similar_cids = self.vector_index.search(&embedding, limit * 2);
    
    // 3. Filter by space and get metadata from SQLite
    let metadata: Vec<ChunkMetadata> = sqlx::query_as(
        "SELECT * FROM chunks WHERE cid IN (?) AND space_id = ? LIMIT ?"
    )
    .bind(&similar_cids.iter().map(|(cid, _)| cid.to_string()).collect::<Vec<_>>())
    .bind(space_id)
    .bind(limit)
    .fetch_all(&self.db)
    .await?;
    
    // 4. Fetch content from RocksDB
    let cids: Vec<_> = metadata.iter().map(|m| m.cid.clone()).collect();
    let contents = self.kv_store.get_chunks(&cids)?;
    
    // 5. Combine results
    Ok(metadata.into_iter()
        .zip(contents)
        .zip(similar_cids.iter().map(|(_, score)| *score))
        .map(|((meta, content), score)| SearchResult {
            cid: meta.cid,
            content: content.text,
            score,
            document_id: meta.document_id,
        })
        .collect())
}
```

#### 9.2.2 Full-Text Search (Deferred to v1.1)

**MVP Decision**: Full-text search is **deferred to v1.1**.

**Why?**
- Chunk content is now in RocksDB, not SQLite
- FTS5 requires content in SQLite (or complex external content tables)
- Semantic search covers most use cases better
- Adding tantivy or similar is non-trivial scope

**v1.1 Plan**: Add full-text search using tantivy:
- Index chunk content on write
- Hybrid search: combine FTS + semantic scores
- Support exact phrase matching

**MVP Workaround**: For exact phrase searches, the UI can:
1. Run semantic search first
2. Filter results client-side for exact matches
3. Display "No full-text search in MVP" if needed

#### 9.2.3 Entity Search

```rust
/// Search entities by name
pub async fn entity_search(
    &self,
    space_id: SpaceId,
    query: &str,
    entity_type: Option<EntityType>,
) -> Result<Vec<Entity>, SearchError> {
    let mut sql = String::from(
        "SELECT * FROM kg_entities WHERE space_id = ? AND name LIKE ?"
    );
    
    if entity_type.is_some() {
        sql.push_str(" AND entity_type = ?");
    }
    
    // Execute query...
}
```

---

## 10. API Layer

### 10.1 REST API (MVP Endpoints)

Base URL: `/api/v1`

#### Authentication

```
POST /auth/webauthn/start-registration
POST /auth/webauthn/finish-registration
POST /auth/webauthn/start-authentication
POST /auth/webauthn/finish-authentication
```
**Status**: ✅ Implemented

#### Spaces

```
GET    /spaces                    # List user's spaces
POST   /spaces                    # Create space
GET    /spaces/:key               # Get space details
DELETE /spaces/:key               # Delete space
POST   /spaces/:key/reindex       # Trigger re-indexing
GET    /spaces/:key/status        # Get indexing status
```
**Status**: 🔨 Partially implemented (add reindex, status)

#### Knowledge Graph

```
GET    /spaces/:key/entities      # List entities
GET    /spaces/:key/entities/:id  # Get entity with edges
POST   /spaces/:key/entities      # Create entity (manual)
DELETE /spaces/:key/entities/:id  # Delete entity

GET    /spaces/:key/edges         # List edges
POST   /spaces/:key/edges         # Create edge (manual)
DELETE /spaces/:key/edges/:id     # Delete edge
```
**Status**: 🔨 New

#### Query

```
POST   /spaces/:key/query         # Natural language query
POST   /spaces/:key/search        # Search (semantic + fulltext)
```
**Status**: 🔨 New

#### Health

```
GET    /health                    # Health check
```
**Status**: ✅ Implemented

### 10.2 WebSocket API (MVP)

Connection: `ws://localhost:8081/ws`

#### Messages

```typescript
// Client -> Server
{ "type": "query", "id": "1", "payload": { "space_key": "...", "query": "..." } }
{ "type": "subscribe", "id": "2", "payload": { "space_key": "..." } }

// Server -> Client
{ "type": "response", "id": "1", "payload": { "answer": "...", "sources": [...] } }
{ "type": "event", "payload": { "type": "indexing_progress", "progress": 0.5 } }
{ "type": "error", "id": "1", "payload": { "code": "NOT_FOUND", "message": "..." } }
```

**Status**: ✅ Partially implemented (add query support)

### 10.3 API Response Format

```typescript
// Success response
{
  "success": true,
  "data": { ... }
}

// Error response
{
  "success": false,
  "error": {
    "code": "NOT_FOUND",
    "message": "Space not found"
  }
}

// Paginated response
{
  "success": true,
  "data": [...],
  "pagination": {
    "total": 100,
    "offset": 0,
    "limit": 20
  }
}
```

---

## 11. Web UI (MVP)

### 11.1 Pages

| Page | Route | Purpose |
|------|-------|---------|
| Auth | `/auth` | Passkey registration/login |
| Home | `/` | Space list, recent activity |
| Space | `/spaces/:key` | Document browser, entity list |
| Chat | `/spaces/:key/chat` | Query interface |
| Settings | `/settings` | User settings |

### 11.2 Components

```
src/
├── components/
│   ├── layout/
│   │   ├── Sidebar.tsx
│   │   ├── Header.tsx
│   │   └── MainContent.tsx
│   ├── auth/
│   │   ├── PasskeyRegister.tsx
│   │   └── PasskeyLogin.tsx
│   ├── spaces/
│   │   ├── SpaceList.tsx
│   │   ├── SpaceCard.tsx
│   │   ├── CreateSpaceModal.tsx
│   │   └── DocumentTree.tsx
│   ├── knowledge/
│   │   ├── EntityList.tsx
│   │   ├── EntityCard.tsx
│   │   └── EntityGraph.tsx      # Simple visualization
│   ├── chat/
│   │   ├── ChatInterface.tsx
│   │   ├── MessageList.tsx
│   │   ├── MessageInput.tsx
│   │   └── SourceCitation.tsx
│   └── common/
│       ├── Button.tsx
│       ├── Modal.tsx
│       ├── Loading.tsx
│       └── ErrorBoundary.tsx
├── hooks/
│   ├── useAuth.ts
│   ├── useSpaces.ts
│   ├── useQuery.ts
│   └── useWebSocket.ts
├── services/
│   ├── api.ts
│   └── websocket.ts
└── pages/
    ├── Auth.tsx
    ├── Home.tsx
    ├── Space.tsx
    ├── Chat.tsx
    └── Settings.tsx
```

### 11.3 Key User Flows

#### Flow 1: First-Time Setup

```
1. User visits /auth
2. Click "Create Account"
3. Browser prompts for passkey (biometric)
4. Passkey created, user logged in
5. Redirect to / (empty state)
6. Prompt: "Create your first Space"
```

#### Flow 2: Create Space

```
1. Click "Create Space" button
2. Modal: Enter local directory path
3. Validation: Directory exists
4. Create space, start indexing
5. Show progress indicator
6. Redirect to /spaces/:key when done
```

#### Flow 3: Query Space

```
1. Navigate to /spaces/:key/chat
2. Type natural language question
3. Show loading indicator
4. Display answer with source citations
5. Click citation to see original chunk
```

### 11.4 State Management

**MVP Decision**: React hooks + Context API (no Redux/Zustand).

```typescript
// AuthContext
const AuthContext = createContext<AuthState>({
  user: null,
  isAuthenticated: false,
  login: async () => {},
  logout: () => {},
});

// SpacesContext
const SpacesContext = createContext<SpacesState>({
  spaces: [],
  loading: false,
  createSpace: async () => {},
  deleteSpace: async () => {},
});
```

**Why no external state library?**
- MVP is simple enough
- Fewer dependencies
- Can add Zustand in v1.1 if needed

---

## 12. Security Model (MVP)

### 12.1 Authentication

| Mechanism | Implementation | Status |
|-----------|----------------|--------|
| Passkey (WebAuthn) | webauthn-rs | ✅ Implemented |
| Session tokens | JWT (short-lived) | 🔨 Add |

### 12.2 Authorization (Simplified)

**MVP Rule**: Owner has full access, no one else has any access.

```rust
pub fn authorize(user_did: &Did, space: &Space, action: Action) -> bool {
    // MVP: Only owner can access
    space.owner_did == *user_did
}
```

### 12.3 What's NOT in MVP

| Feature | Reason | Target |
|---------|--------|--------|
| Capability delegation | Complexity | v1.1 |
| Per-Space encryption | Key management | v1.1 |
| Audit logging | Not needed for single-user | v1.1 |
| Rate limiting | Single-user local | v1.1 |

### 12.4 Secure Defaults

```rust
pub struct SecurityConfig {
    /// JWT expiration (short for security)
    pub jwt_expiry: Duration,           // Default: 1 hour
    
    /// CORS allowed origins
    pub cors_origins: Vec<String>,      // Default: ["http://localhost:3000"]
    
    /// Max request body size
    pub max_body_size: usize,           // Default: 10MB
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            jwt_expiry: Duration::from_secs(3600),
            cors_origins: vec!["http://localhost:3000".to_string()],
            max_body_size: 10 * 1024 * 1024,
        }
    }
}
```

---

## 13. Performance Targets (Realistic)

### 13.1 Latency (P95)

| Operation | Target | Notes |
|-----------|--------|-------|
| Auth (passkey) | <500ms | Browser biometric |
| Space list | <100ms | SQLite query |
| Document tree | <200ms | File system + DB |
| Semantic search | <300ms | Vector search + DB |
| RAG query | <5000ms | LLM generation dominates |
| Entity list | <100ms | SQLite query |

### 13.2 Throughput

| Metric | Target | Notes |
|--------|--------|-------|
| Indexing | 10-20 docs/min | Complex documents |
| Queries | 10/min | Limited by LLM |
| API requests | 100/sec | Non-LLM endpoints |

### 13.3 Scale (MVP)

| Metric | Target | Notes |
|--------|--------|-------|
| Documents/Space | 10,000 | Practical limit |
| Total documents | 50,000 | SQLite performs well |
| Entities | 100,000 | SQLite + HNSW |
| Chunks | 500,000 | May need optimization |

### 13.4 Resource Usage

| Resource | Target | Notes |
|----------|--------|-------|
| RAM (idle) | <500MB | Without LLM loaded |
| RAM (HNSW index) | ~4 bytes × vectors × 100 | ~600MB for 1.5M chunks |
| RAM (indexing) | <2GB | With LLM for extraction |
| RAM (query) | <4GB | With LLM loaded |
| Disk (SQLite) | <500MB | Metadata only |
| Disk (RocksDB) | ~8GB | Chunks + embeddings (for 1.5M) |
| Disk (HNSW file) | ~6GB | Serialized vector index |
| Disk (per 1K docs) | ~50MB | All storage combined |

---

## 14. Testing Requirements

### 14.1 Coverage Targets (MVP)

| Layer | Unit | Integration |
|-------|------|-------------|
| Storage | 80% | 70% |
| Auth | 80% | 80% |
| Knowledge Graph | 80% | 70% |
| Query | 70% | 70% |
| API | 60% | 80% |
| UI | - | 50% (E2E) |

**Note**: Lower than extended spec because MVP scope is smaller.

### 14.2 Critical Test Cases

#### Authentication
- [ ] Passkey registration succeeds
- [ ] Passkey login succeeds
- [ ] Invalid credential rejected
- [ ] Session expires correctly

#### Spaces
- [ ] Create space with valid directory
- [ ] Create space with invalid directory fails
- [ ] Delete space removes all data
- [ ] Indexing completes without errors

#### Knowledge Graph
- [ ] Entity creation with valid data
- [ ] Edge creation between entities
- [ ] Entity deletion cascades to edges
- [ ] Search returns relevant results

#### Query
- [ ] RAG query returns answer
- [ ] Answer includes source citations
- [ ] Empty space returns appropriate message
- [ ] Long query times out gracefully

### 14.3 CI Pipeline (MVP)

```yaml
name: MVP CI

on: [push, pull_request]

jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Clippy
        run: cargo clippy -- -D warnings
      - name: Format
        run: cargo fmt --check
      - name: Type check
        run: cargo check

  test:
    runs-on: ubuntu-latest
    steps:
      - name: Unit tests
        run: cargo test --lib
      - name: Integration tests
        run: cargo test --test '*'

  frontend:
    runs-on: ubuntu-latest
    steps:
      - name: ESLint
        run: npm run lint --prefix user-interface/flow-web
      - name: TypeScript
        run: npm run typecheck --prefix user-interface/flow-web
      - name: Build
        run: npm run build --prefix user-interface/flow-web
```

---

## 15. Implementation Plan

### 15.1 Phase Overview

| Phase | Focus | Duration | Status |
|-------|-------|----------|--------|
| **Phase 0** | Foundation fixes | 2 weeks | 🔨 Current |
| **Phase 1** | Knowledge Graph | 4 weeks | Pending |
| **Phase 2** | Query System | 3 weeks | Pending |
| **Phase 3** | Web UI | 4 weeks | Pending |
| **Phase 4** | Polish & Testing | 3 weeks | Pending |
| **Total** | | **16 weeks** | |

### 15.2 Phase 0: Foundation Fixes (Weeks 1-2)

**Goal**: Stabilize existing code, add missing pieces.

| Task | Estimate | Priority |
|------|----------|----------|
| Add JWT session tokens | 2d | P0 |
| Fix WebAuthn CORS issues | 1d | P0 |
| Add database migrations for KG | 2d | P0 |
| Disable network layer by default | 1d | P0 |
| Update config system | 2d | P1 |
| Add health check improvements | 1d | P1 |

**Deliverables**:
- [ ] User can register and login with passkey
- [ ] Database schema ready for KG
- [ ] Config file support working

### 15.3 Phase 1: Knowledge Graph (Weeks 3-6)

**Goal**: Implement entity/edge storage and extraction.

| Task | Estimate | Priority |
|------|----------|----------|
| Entity store (CRUD) | 3d | P0 |
| Edge store (CRUD) | 2d | P0 |
| Vector store (HNSW) | 3d | P0 |
| Entity extraction (LLM) | 4d | P0 |
| Integration with indexing | 3d | P0 |
| Basic traversal queries | 2d | P1 |
| Entity deduplication | 2d | P1 |

**Deliverables**:
- [ ] Entities extracted from documents
- [ ] Edges created (Document -> Entity)
- [ ] Entities searchable by name
- [ ] Vector similarity search working

### 15.4 Phase 2: Query System (Weeks 7-9)

**Goal**: Implement RAG and search.

| Task | Estimate | Priority |
|------|----------|----------|
| RAG pipeline | 4d | P0 |
| Semantic search (HNSW) | 3d | P0 |
| Chunk storage (RocksDB integration) | 2d | P0 |
| Source citation | 2d | P0 |
| Context expansion (KG) | 2d | P1 |
| Query API endpoint | 1d | P0 |
| WebSocket query support | 2d | P1 |

**Deliverables**:
- [ ] User can ask natural language questions
- [ ] Answers include source citations
- [ ] Semantic search returns relevant chunks
- [ ] Entity context improves answers

**Note**: Full-text search deferred to v1.1 (see Section 9.2.2).

### 15.5 Phase 3: Web UI (Weeks 10-13)

**Goal**: Build functional web interface.

| Task | Estimate | Priority |
|------|----------|----------|
| Project setup (Vite, routing) | 2d | P0 |
| Auth pages (register, login) | 3d | P0 |
| Space list page | 2d | P0 |
| Space detail page | 3d | P0 |
| Document tree component | 2d | P0 |
| Entity list component | 2d | P0 |
| Chat interface | 4d | P0 |
| Source citation display | 2d | P0 |
| Settings page | 1d | P1 |
| Error handling | 2d | P0 |

**Deliverables**:
- [ ] User can register/login via UI
- [ ] User can create/view/delete spaces
- [ ] User can browse documents
- [ ] User can see extracted entities
- [ ] User can query via chat interface

### 15.6 Phase 4: Polish & Testing (Weeks 14-16)

**Goal**: Production readiness.

| Task | Estimate | Priority |
|------|----------|----------|
| Unit test coverage (80%) | 4d | P0 |
| Integration tests | 3d | P0 |
| E2E tests (critical paths) | 3d | P0 |
| Performance profiling | 2d | P1 |
| Error message improvements | 2d | P1 |
| Documentation (README, API) | 2d | P1 |
| Bug fixes (buffer) | 3d | P0 |

**Deliverables**:
- [ ] All tests passing
- [ ] No critical bugs
- [ ] Documentation complete
- [ ] Ready for beta users

---

## 16. Deferred Features (v1.1+)

### 16.1 Version 1.1 (Collaboration)

**Target**: 12-16 weeks after MVP

| Feature | Notes |
|---------|-------|
| Full-text search | Tantivy integration for exact matching |
| Multi-user spaces | Space sharing with permissions |
| CRDT text sync | Yrs integration for documents |
| Multi-device | Sync between user's devices |
| Cross-node networking | Enable libp2p for sync |
| Mobile app | React Native / Expo |
| Capability delegation | VC-based permissions |
| Per-Space encryption | With passkey-derived keys |

### 16.2 Version 1.2 (Agents)

**Target**: 16-24 weeks after v1.1

| Feature | Notes |
|---------|-------|
| Workflow DSL | YAML-based workflow definitions |
| WASM sandbox | Wasmtime integration |
| Agent DIDs | Separate identity for agents |
| Predefined workflows | Summarize, extract, monitor |
| Workflow checkpoints | Resume after crash |

### 16.3 Version 2.0 (Scale)

**Target**: Future

| Feature | Notes |
|---------|-------|
| Federated queries | Cross-user queries with consent |
| Key rotation | Background re-encryption |
| Distributed reputation | Economic stake model |
| ZK proofs | Verifiable computation |
| Incentive layer | Token-based rewards |

---

## Appendix A: Environment Variables (MVP)

```bash
# Required
DATABASE_URL=sqlite:///path/to/flow.db

# Optional (with defaults)
FLOW_DATA_HOME=~/.flow/data           # Data directory
KV_PATH=~/.flow/kv                    # RocksDB path
REST_PORT=8080                        # REST API port
WEBSOCKET_PORT=8081                   # WebSocket port
HOST=0.0.0.0                          # Bind address

# AI (with defaults)
EMBEDDING_MODEL=nomic-embed-text      # Embedding model
CHAT_MODEL=llama3.1:8b                # Chat/RAG model
OLLAMA_HOST=http://localhost:11434    # Ollama endpoint

# Auth
WEBAUTHN_RP_ID=localhost              # Relying Party ID
WEBAUTHN_RP_ORIGIN=http://localhost:3000

# Feature flags
ENABLE_NETWORK=false                  # P2P networking (disabled in MVP)
LOG_LEVEL=info                        # Logging level
```

---

## Appendix B: File Type Support (MVP)

| Extension | Parser | Chunking | Priority |
|-----------|--------|----------|----------|
| .txt | Native | Semantic | P0 |
| .md | Native | Hierarchical | P0 |
| .rs, .ts, .py, .js, .go | tree-sitter | AST | P0 |
| .pdf | pdf-extract | Smart | P0 |
| .docx | pandoc (external) | Smart | P1 |
| .html | scraper | Semantic | P1 |
| .json, .yaml | Native | None | P1 |

**Not in MVP**:
- Images (.png, .jpg) - requires OCR
- Audio (.mp3, .wav) - requires transcription
- Video (.mp4) - too complex

---

## Appendix C: Error Codes

| Code | HTTP | Description |
|------|------|-------------|
| `AUTH_FAILED` | 401 | Authentication failed |
| `NOT_FOUND` | 404 | Resource not found |
| `VALIDATION_ERROR` | 400 | Invalid input |
| `SPACE_EXISTS` | 409 | Space already exists |
| `INDEXING_IN_PROGRESS` | 409 | Cannot modify during indexing |
| `LLM_ERROR` | 503 | LLM service unavailable |
| `INTERNAL_ERROR` | 500 | Unexpected error |

---

## Appendix D: Glossary

| Term | Definition |
|------|------------|
| **Space** | A user-owned directory indexed by Flow |
| **Entity** | A node in the Knowledge Graph (Person, Concept, etc.) |
| **Edge** | A relationship between entities |
| **Chunk** | A segment of a document stored for retrieval |
| **RAG** | Retrieval-Augmented Generation |
| **CID** | Content Identifier (hash-based address) |
| **DID** | Decentralized Identifier |
| **HNSW** | Hierarchical Navigable Small World (vector index) |

---

*End of MVP Specification*
