# Flow: Complete Technical Specification

> **Version**: 1.0.0  
> **Status**: Executable Specification  
> **Last Updated**: January 2026

This document serves as the authoritative specification for building Flow, a decentralized automation network. It is designed to be executed by engineering teams and AI coding agents.

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Architecture Overview](#2-architecture-overview)
3. [Implementation Priorities](#3-implementation-priorities)
4. [Layer Specifications](#4-layer-specifications)
5. [Data Models](#5-data-models)
6. [API Specifications](#6-api-specifications)
7. [Security Model](#7-security-model)
8. [Performance Requirements](#8-performance-requirements)
9. [UI/UX Specifications](#9-uiux-specifications)
10. [Deployment & Operations](#10-deployment--operations)
11. [Testing Requirements](#11-testing-requirements)
12. [Implementation Task Breakdown](#12-implementation-task-breakdown)

---

## 1. Executive Summary

### 1.1 Vision

Flow is a **decentralized automation network** that transforms goals into verifiable jobs across a peer compute marketplace. The platform enables:

- **Personal Knowledge Management**: Users control their own Knowledge Base with AI-powered indexing and retrieval
- **Collaborative Workspaces**: Real-time sync and collaboration with cryptographic verification
- **Autonomous Agents**: AI agents that act on behalf of users with delegated capabilities
- **Decentralized Infrastructure**: Local-first, peer-to-peer, with optional cloud integration

### 1.2 Core Principles

| # | Principle | Implementation |
|---|-----------|----------------|
| 1 | Ubiquitous, Verifiable Computing | All operations produce verifiable proofs |
| 2 | Capability-Based Access | DIDs + Verifiable Credentials for authorization |
| 3 | Knowledge Graphs with Provenance | Schema-aware graphs tracking origin and trust |
| 4 | P2P Networking | Decentralized discovery, routing, and sync |
| 5 | Decentralized Execution | Local-first, edge-native, cloud-optional |
| 6 | Agent Explainability (SLRPA) | Sense -> Learn -> Reason -> Predict -> Act cycle |
| 7 | Programmable Incentives | Customizable rewards and reputation |

### 1.3 MVP Scope

The first production release includes:

- **Storage Layer**: Content-addressed persistence with IPLD, CRDTs, RocksDB
- **Access & Auth Layer**: DIDs, VCs, capability delegation, WebAuthn
- **Network Layer**: libp2p (Kademlia, GossipSub, QUIC/TCP), peer registry
- **Knowledge Graph**: Full semantic memory with reasoning capabilities
- **Agent Layer (Basic)**: RAG queries and predefined workflow execution

### 1.4 Target Users

Flow serves three user types simultaneously:

| User Type | Primary Interface | Key Features |
|-----------|-------------------|--------------|
| End Users | Web/Mobile Apps | Space management, AI queries, collaboration |
| Developers | SDK/API | Integration, custom workflows, extensions |
| AI Agents | MCP/API | Tool access, capability delegation, inter-agent communication |

### 1.5 Deployment Models

| Model | Database | Use Case |
|-------|----------|----------|
| Self-hosted Node | SQLite | Personal use, privacy-focused |
| Managed Cloud | PostgreSQL | Teams, enterprises |
| Embedded Library | SQLite | Third-party app integration |

---

## 2. Architecture Overview

### 2.1 Layered Architecture

```
+------------------------------------------------------------------+
|                        UI/UX Layer                                |
|    Web (React/Vite/Electron) | Mobile (Expo/React Native)        |
+------------------------------------------------------------------+
|                         API Layer                                 |
|         REST | WebSocket | GraphQL | MCP Server/Client           |
+------------------------------------------------------------------+
|                        Agent Layer                                |
|    Workflow Engine | SLRPA Lifecycle | Tool Registry             |
+------------------------------------------------------------------+
|                    Knowledge Graph Layer                          |
|    Entity Store | Relationship Index | Semantic Memory           |
+------------------------------------------------------------------+
|                   Coordination & Sync Layer                       |
|         CRDTs (Yrs) | Event Sourcing | Conflict Resolution       |
+------------------------------------------------------------------+
|                       Network Layer                               |
|    libp2p | Kademlia DHT | GossipSub | Content Transfer          |
+------------------------------------------------------------------+
|                    Access & Auth Layer                            |
|         DIDs | VCs | Capability Delegation | WebAuthn            |
+------------------------------------------------------------------+
|                       Storage Layer                               |
|    IPLD Blocks | RocksDB KV | SQLite/Postgres | Content Addressing|
+------------------------------------------------------------------+
```

### 2.2 Module Structure (Backend)

```
back-end/node/src/
├── main.rs                 # Entry point
├── runner.rs               # Application bootstrap
├── bootstrap/              # Config, DID generation
├── api/
│   ├── servers/           # REST, WebSocket, GraphQL
│   ├── dto/               # Data transfer objects
│   └── node.rs            # API facade
└── modules/
    ├── storage/           # KV store, content-addressed blocks
    ├── ssi/               # DIDs, WebAuthn, capabilities
    ├── network/           # libp2p, gossipsub, peer management
    ├── knowledge/         # Knowledge Graph (NEW)
    ├── agent/             # Workflow engine, WASM sandbox (NEW)
    └── space.rs           # Space management
```

### 2.3 Data Flow

```
User Action
    │
    ▼
┌─────────────────┐
│   API Layer     │──────────────────────────────────────┐
└────────┬────────┘                                      │
         │                                               │
         ▼                                               ▼
┌─────────────────┐                            ┌─────────────────┐
│  Auth Check     │                            │  Event Created  │
│  (Capability)   │                            │  (Signed, CID)  │
└────────┬────────┘                            └────────┬────────┘
         │                                               │
         ▼                                               ▼
┌─────────────────┐                            ┌─────────────────┐
│  Knowledge      │◄──────────────────────────►│  Event Store    │
│  Graph Update   │                            │  (Append-only)  │
└────────┬────────┘                            └────────┬────────┘
         │                                               │
         ▼                                               ▼
┌─────────────────┐                            ┌─────────────────┐
│  CRDT Merge     │                            │  Network Sync   │
│  (if needed)    │                            │  (GossipSub)    │
└─────────────────┘                            └─────────────────┘
```

---

## 3. Implementation Priorities

### 3.1 Priority Order

| Priority | Feature Area | Description | Dependencies |
|----------|--------------|-------------|--------------|
| **P0** | Solo User | Create Space -> Index docs -> Query via AI | Storage, Auth, KG |
| **P1** | Multi-User | Share Space -> Real-time sync -> Collaborate | P0 + Sync, Network |
| **P2** | Cross-Node Networking | Discovery, content routing, pub/sub | P1 + Network |
| **P3** | Agent Autonomy | Delegated capabilities, workflow execution | P2 + Agent |
| **P4** | Developer SDK | Clean APIs, documentation, examples | P3 + API |

### 3.2 Release Phases

#### Phase 1: Solo User MVP (P0)

**Goal**: Single user can create a Space, index documents, and query via AI.

**Deliverables**:
- [ ] Space creation and management
- [ ] Document indexing pipeline (text, code, PDF, Office)
- [ ] Knowledge Graph with entity extraction
- [ ] RAG-based query system
- [ ] Web UI for Space browser and chat
- [ ] Passkey authentication

#### Phase 2: Collaboration (P1)

**Goal**: Multiple users can share and collaborate on Spaces.

**Deliverables**:
- [ ] Space sharing with capability delegation
- [ ] CRDT-based real-time sync (Yrs for text)
- [ ] Conflict resolution UI for non-CRDT data
- [ ] Multi-device support
- [ ] Sync status indicators

#### Phase 3: Network (P2)

**Goal**: Nodes discover each other and exchange content.

**Deliverables**:
- [ ] Bootstrap node infrastructure
- [ ] Peer discovery via Kademlia DHT
- [ ] Content routing and transfer
- [ ] GossipSub topic subscription
- [ ] NAT traversal (relay + hole punching)

#### Phase 4: Agents (P3)

**Goal**: AI agents execute workflows on behalf of users.

**Deliverables**:
- [ ] Workflow definition system (DSL, DAG, MCP)
- [ ] WASM sandbox execution (Wasmtime)
- [ ] Agent DID and capability delegation
- [ ] Checkpoint and resume for long-running workflows
- [ ] Audit logging for agent actions

#### Phase 5: SDK (P4)

**Goal**: Developers can integrate Flow into their applications.

**Deliverables**:
- [ ] TypeScript SDK
- [ ] Python SDK
- [ ] Rust crate documentation
- [ ] API reference documentation
- [ ] Example applications

---

## 4. Layer Specifications

### 4.1 Storage Layer

#### 4.1.1 Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                      Storage Layer                          │
├─────────────────┬─────────────────┬─────────────────────────┤
│   Block Store   │    KV Store     │     SQL Database        │
│   (IPLD/CID)    │   (RocksDB)     │   (SQLite/Postgres)     │
├─────────────────┼─────────────────┼─────────────────────────┤
│ Content blocks  │ Events          │ Entities (User, Space)  │
│ Manifests       │ Indexes         │ Relationships           │
│ DAG nodes       │ Cache           │ Metadata                │
└─────────────────┴─────────────────┴─────────────────────────┘
```

#### 4.1.2 Content Addressing

All content is addressed by CID (Content Identifier):

```rust
pub struct ContentId {
    pub version: u64,           // CID version (1)
    pub codec: u64,             // Multicodec (dag-cbor, raw, etc.)
    pub hash: Multihash,        // SHA2-256 hash
}

impl ContentId {
    pub fn from_bytes(data: &[u8]) -> Self;
    pub fn to_string(&self) -> String;  // Base32 encoding
}
```

#### 4.1.3 Block Store

```rust
pub trait BlockStore: Send + Sync {
    async fn get(&self, cid: &ContentId) -> Result<Option<Block>, BlockError>;
    async fn put(&self, block: Block) -> Result<ContentId, BlockError>;
    async fn has(&self, cid: &ContentId) -> Result<bool, BlockError>;
    async fn delete(&self, cid: &ContentId) -> Result<(), BlockError>;
    async fn list(&self, prefix: Option<&str>) -> Result<Vec<ContentId>, BlockError>;
}

pub struct Block {
    pub cid: ContentId,
    pub data: Vec<u8>,
    pub links: Vec<ContentId>,  // IPLD links
}
```

#### 4.1.4 Event Store

Events are append-only, signed, and causally ordered:

```rust
pub struct Event {
    pub id: Ulid,                       // Unique identifier
    pub event_type: String,             // e.g., "document.created"
    pub version: u32,                   // Schema version
    pub payload: serde_json::Value,     // Event data
    pub timestamp: DateTime<Utc>,
    pub causality: DottedVersionVector, // Causal ordering
    pub prev_hash: Option<ContentId>,   // Hash chain
    pub signer: Did,                    // Who signed this
    pub signature: Signature,           // Cryptographic signature
}

pub trait EventStore: Send + Sync {
    async fn append(&self, event: Event) -> Result<ContentId, EventError>;
    async fn get(&self, id: &Ulid) -> Result<Option<Event>, EventError>;
    async fn iter_from(&self, offset: u64) -> Result<EventIterator, EventError>;
    async fn verify_chain(&self) -> Result<bool, EventError>;
}
```

#### 4.1.5 Database Schema

```sql
-- Users table
CREATE TABLE users (
    id SERIAL PRIMARY KEY,
    did VARCHAR(255) UNIQUE NOT NULL,
    username VARCHAR(100),
    display_name VARCHAR(255),
    public_key_jwk JSONB,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    last_login TIMESTAMPTZ
);

-- Spaces table
CREATE TABLE spaces (
    id SERIAL PRIMARY KEY,
    key VARCHAR(64) UNIQUE NOT NULL,  -- SHA256 of canonical path
    location TEXT NOT NULL,
    owner_id INTEGER REFERENCES users(id),
    encryption_key_id VARCHAR(255),    -- Reference to key in secure storage
    availability VARCHAR(20) DEFAULT 'local',  -- local, pinned, public
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Space sharing
CREATE TABLE space_shares (
    id SERIAL PRIMARY KEY,
    space_id INTEGER REFERENCES spaces(id),
    user_id INTEGER REFERENCES users(id),
    capability_vc TEXT NOT NULL,       -- Verifiable Credential
    granted_at TIMESTAMPTZ DEFAULT NOW(),
    expires_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ
);

-- Index status
CREATE TABLE space_index_status (
    id SERIAL PRIMARY KEY,
    space_id INTEGER REFERENCES spaces(id),
    last_indexed TIMESTAMPTZ,
    indexing_in_progress BOOLEAN DEFAULT FALSE,
    files_indexed INTEGER DEFAULT 0,
    chunks_stored INTEGER DEFAULT 0,
    files_failed INTEGER DEFAULT 0,
    last_error TEXT
);
```

#### 4.1.6 Encryption at Rest

```rust
pub struct SpaceEncryption {
    /// Per-Space encryption key (stored in Secure Enclave/TPM)
    pub key_id: String,
    
    /// Algorithm: AES-256-GCM
    pub algorithm: EncryptionAlgorithm,
    
    /// Key derivation for devices without hardware security
    pub fallback_kdf: Option<Argon2Params>,
}

impl SpaceEncryption {
    /// Encrypt data for storage
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, CryptoError>;
    
    /// Decrypt data from storage
    pub fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, CryptoError>;
    
    /// Rotate key (for access revocation)
    pub async fn rotate_key(&mut self) -> Result<(), CryptoError>;
}
```

---

### 4.2 Access & Auth Layer

#### 4.2.1 DID (Decentralized Identifier)

```rust
/// DID format: did:key:<multibase-encoded-public-key>
pub struct Did {
    pub method: String,     // "key"
    pub id: String,         // Multibase-encoded public key
}

impl Did {
    /// Generate new DID from Ed25519 keypair
    pub fn generate() -> (Did, SigningKey);
    
    /// Parse DID string
    pub fn parse(s: &str) -> Result<Did, DidError>;
    
    /// Get public key from DID
    pub fn public_key(&self) -> Result<PublicKey, DidError>;
}
```

#### 4.2.2 Capability Delegation

```rust
pub struct Capability {
    pub id: String,                     // Unique capability ID
    pub issuer: Did,                    // Who granted this
    pub subject: Did,                   // Who receives this
    pub resource: ResourceIdentifier,   // What it applies to
    pub actions: Vec<Action>,           // What actions are allowed
    pub constraints: Constraints,       // Limitations
    pub proof_chain: Vec<ContentId>,    // Delegation chain
}

pub enum Action {
    ReadSpace,
    WriteSpace,
    QueryKnowledgeGraph,
    ExecuteWorkflow(WorkflowId),
    DelegateCapability,
    NetworkPublish,
    NetworkSubscribe,
}

pub struct Constraints {
    pub expires_at: Option<DateTime<Utc>>,
    pub max_uses: Option<u32>,
    pub allow_subdelegation: bool,      // Default: false
}

impl Capability {
    /// Verify capability chain is valid
    pub fn verify(&self) -> Result<bool, CapabilityError>;
    
    /// Check if action is permitted
    pub fn permits(&self, action: &Action) -> bool;
    
    /// Revoke this capability (cascades to subdelegations)
    pub async fn revoke(&self, store: &impl CapabilityStore) -> Result<(), CapabilityError>;
}
```

#### 4.2.3 WebAuthn Integration

```rust
pub struct AuthState {
    pub webauthn: Webauthn,
    pub challenges: DashMap<String, Challenge>,  // Pending challenges
}

impl AuthState {
    /// Start passkey registration
    pub async fn start_registration(&self, user: &User) 
        -> Result<(CreationChallengeResponse, String), AuthError>;
    
    /// Complete passkey registration
    pub async fn finish_registration(
        &self,
        challenge_id: &str,
        credential: RegisterPublicKeyCredential,
    ) -> Result<PassKey, AuthError>;
    
    /// Start authentication
    pub async fn start_authentication(&self) 
        -> Result<(RequestChallengeResponse, String), AuthError>;
    
    /// Complete authentication
    pub async fn finish_authentication(
        &self,
        challenge_id: &str,
        credential: PublicKeyCredential,
    ) -> Result<AuthResult, AuthError>;
}
```

#### 4.2.4 Credential Storage

Credentials are stored locally and replicated to trusted peers:

```rust
pub struct CredentialStore {
    /// Local encrypted storage
    local: EncryptedStore,
    
    /// Trusted peers for replication
    trusted_peers: Vec<PeerId>,
}

impl CredentialStore {
    /// Store credential locally and replicate
    pub async fn store(&self, vc: &VerifiableCredential) -> Result<(), StoreError>;
    
    /// Retrieve credential
    pub async fn get(&self, id: &str) -> Result<Option<VerifiableCredential>, StoreError>;
    
    /// Sync with trusted peers
    pub async fn sync(&self) -> Result<SyncResult, StoreError>;
}
```

---

### 4.3 Network Layer

#### 4.3.1 Network Manager

```rust
pub struct NetworkManager {
    local_peer_id: PeerId,
    command_tx: mpsc::UnboundedSender<NetworkCommand>,
    message_router: Arc<MessageRouter>,
    subscription_manager: Arc<TopicSubscriptionManager>,
    block_store: Option<Arc<BlockStore>>,
}

impl NetworkManager {
    /// Start the network with configuration
    pub async fn start(&self, config: &NetworkConfig) -> Result<(), AppError>;
    
    /// Stop the network gracefully
    pub async fn stop(&self) -> Result<(), AppError>;
    
    /// Connect to a peer
    pub async fn dial_peer(&self, address: Multiaddr) -> Result<(), AppError>;
    
    /// Subscribe to a topic
    pub async fn subscribe(&self, topic: impl Into<String>) 
        -> Result<SubscriptionHandle, AppError>;
    
    /// Publish a message
    pub async fn publish(&self, message: Message) -> Result<String, AppError>;
    
    /// Find content providers
    pub async fn get_providers(&self, cid: ContentId) 
        -> Result<ProviderDiscoveryResult, AppError>;
    
    /// Announce as content provider
    pub async fn start_providing(&self, cid: ContentId) 
        -> Result<ProviderAnnounceResult, AppError>;
    
    /// Fetch block from network
    pub async fn fetch_block(&self, cid: ContentId) 
        -> Result<Block, ContentTransferError>;
}
```

#### 4.3.2 Network Configuration

```rust
pub struct NetworkConfig {
    /// Port to listen on
    pub listen_port: u16,               // Default: 4001
    
    /// Enable QUIC transport
    pub enable_quic: bool,              // Default: true
    
    /// Bootstrap peers
    pub bootstrap_peers: Vec<Multiaddr>,
    
    /// Bootstrap behavior
    pub bootstrap: BootstrapConfig,
    
    /// mDNS for local discovery
    pub mdns: MdnsConfig,
    
    /// GossipSub settings
    pub gossipsub: GossipSubConfig,
    
    /// Connection limits
    pub connection_limits: ConnectionLimits,
    
    /// Content transfer settings
    pub content_transfer: ContentTransferConfig,
}

pub struct BootstrapConfig {
    pub auto_dial: bool,                // Default: true
    pub auto_bootstrap: bool,           // Default: true
    pub startup_delay_ms: u64,          // Default: 100
    pub max_retries: u32,               // Default: 3
    pub retry_delay_base_ms: u64,       // Default: 1000
}

pub struct ConnectionLimits {
    pub max_inbound: u32,               // Default: 100
    pub max_outbound: u32,              // Default: 100
    pub max_pending_incoming: u32,      // Default: 50
    pub max_pending_outgoing: u32,      // Default: 50
}
```

#### 4.3.3 GossipSub Topics

```rust
pub enum Topic {
    // Global topics (everyone can subscribe)
    ContentAnnouncements,       // New content published
    NetworkEvents,              // Network-wide events
    PeerDiscovery,              // Peer announcements
    
    // Scoped topics (privacy-sensitive)
    Space(SpaceId),             // Per-space updates
    User(Did),                  // Per-user notifications
    
    // Ephemeral topics (coordination)
    Workflow(WorkflowId),       // Workflow execution
    Session(SessionId),         // Collaboration session
    
    // Agent topics
    AgentBroadcast,             // Agent announcements
    AgentDirect(Did),           // Direct agent messages
}

impl Topic {
    pub fn to_topic_string(&self) -> String;
    pub fn from_string(s: &str) -> Result<Topic, TopicError>;
}
```

#### 4.3.4 NAT Traversal

```rust
pub struct NatConfig {
    /// Enable relay (Circuit Relay v2)
    pub enable_relay: bool,             // Default: true
    
    /// Flow-operated relay nodes
    pub relay_nodes: Vec<Multiaddr>,
    
    /// Enable hole punching
    pub enable_hole_punching: bool,     // Default: true
    
    /// Fallback to relay if hole punching fails
    pub fallback_to_relay: bool,        // Default: true
}
```

---

### 4.4 Knowledge Graph Layer

#### 4.4.1 Architecture

```
┌────────────────────────────────────────────────────────────────┐
│                    Knowledge Graph Layer                        │
├──────────────────┬──────────────────┬──────────────────────────┤
│   Entity Store   │  Relationship    │    Semantic Memory       │
│   (IPLD nodes)   │  Index           │    (Embeddings + LLM)    │
├──────────────────┼──────────────────┼──────────────────────────┤
│ Typed entities   │ Edge traversal   │ Vector search            │
│ Schema validation│ Path queries     │ Entity extraction        │
│ Version history  │ Federation       │ Inference                │
└──────────────────┴──────────────────┴──────────────────────────┘
```

#### 4.4.2 Core Entity Types (Base Schema)

```rust
pub enum EntityType {
    Document,       // Indexed file/content
    Person,         // Human identity
    Agent,          // AI agent identity
    Concept,        // Abstract idea/topic
    Event,          // Time-bound occurrence
    Task,           // Actionable item
    Place,          // Location
    Organization,   // Group/company
}

pub struct Entity {
    pub id: ContentId,              // IPLD CID
    pub entity_type: EntityType,
    pub space_id: SpaceId,          // Owning space
    pub properties: HashMap<String, Value>,
    pub created_at: DateTime<Utc>,
    pub created_by: Did,
    pub modified_at: DateTime<Utc>,
    pub prev_version: Option<ContentId>,  // Version chain
}
```

#### 4.4.3 Edge Types (Relationship Schema)

```rust
pub enum EdgeType {
    // Provenance
    CreatedBy,          // Any -> Person/Agent
    ModifiedBy,         // Any -> Person/Agent
    DerivedFrom,        // Any -> Any (lineage)
    Supersedes,         // Any -> Any (versioning)
    
    // Structure
    Contains,           // Collection -> Any
    References,         // Any -> Any (citation)
    Mentions,           // Document -> Any
    
    // Workflow
    AssignedTo,         // Task -> Person/Agent
    DependsOn,          // Task -> Task
    InputOf,            // Any -> Workflow
    OutputOf,           // Workflow -> Any
    
    // Agent
    DelegatedTo,        // Person -> Agent
    ActedOnBehalfOf,    // Agent -> Person
    
    // Social
    Knows,              // Person -> Person
    
    // Semantic
    RelatedTo,          // Any -> Any (generic, use sparingly)
    SameAs,             // Any -> Any (identity across spaces)
    InstanceOf,         // Any -> Concept (classification)
}

pub struct Edge {
    pub id: ContentId,
    pub edge_type: EdgeType,
    pub source: ContentId,          // From entity
    pub target: ContentId,          // To entity
    pub properties: HashMap<String, Value>,
    pub created_at: DateTime<Utc>,
    pub created_by: Did,
    pub valid_from: Option<DateTime<Utc>>,
    pub valid_until: Option<DateTime<Utc>>,
}
```

#### 4.4.4 Knowledge Graph Store

```rust
pub trait KnowledgeGraphStore: Send + Sync {
    // Entity operations
    async fn create_entity(&self, entity: Entity) -> Result<ContentId, KgError>;
    async fn get_entity(&self, id: &ContentId) -> Result<Option<Entity>, KgError>;
    async fn update_entity(&self, entity: Entity) -> Result<ContentId, KgError>;
    async fn delete_entity(&self, id: &ContentId) -> Result<(), KgError>;
    
    // Edge operations
    async fn create_edge(&self, edge: Edge) -> Result<ContentId, KgError>;
    async fn get_edges(&self, entity_id: &ContentId, direction: Direction) 
        -> Result<Vec<Edge>, KgError>;
    async fn delete_edge(&self, id: &ContentId) -> Result<(), KgError>;
    
    // Queries
    async fn query(&self, query: GraphQuery) -> Result<QueryResult, KgError>;
    async fn traverse(&self, start: &ContentId, pattern: TraversalPattern) 
        -> Result<Vec<Path>, KgError>;
    
    // Cross-space federation
    async fn federated_query(&self, spaces: &[SpaceId], query: GraphQuery) 
        -> Result<QueryResult, KgError>;
}
```

#### 4.4.5 Semantic Memory

```rust
pub struct SemanticMemory {
    /// Embedding model
    embedding_model: EmbeddingModel,
    
    /// Vector index
    vector_store: VectorStore,
    
    /// LLM for reasoning
    llm: LanguageModel,
}

impl SemanticMemory {
    /// Extract entities from document
    pub async fn extract_entities(&self, document: &Document) 
        -> Result<Vec<ExtractedEntity>, ExtractionError>;
    
    /// Find similar entities
    pub async fn find_similar(&self, query: &str, limit: usize) 
        -> Result<Vec<(Entity, f32)>, SearchError>;
    
    /// Temporal query ("What did I know about X last month?")
    pub async fn temporal_query(&self, query: &str, time_range: TimeRange) 
        -> Result<Vec<Entity>, QueryError>;
    
    /// Inference (derive new facts)
    pub async fn infer(&self, premises: &[Entity]) 
        -> Result<Vec<InferredFact>, InferenceError>;
}
```

#### 4.4.6 AI Model Configuration

```rust
pub struct ModelConfig {
    /// Embedding model
    pub embedding: EmbeddingModelConfig,
    
    /// Chat/RAG model
    pub chat: ChatModelConfig,
    
    /// Device-specific fallbacks
    pub fallbacks: FallbackConfig,
}

pub struct EmbeddingModelConfig {
    /// Primary model
    pub model: String,              // Default: "nomic-embed-text"
    pub dimensions: usize,          // Default: 768
    
    /// Lightweight fallback
    pub fallback_model: String,     // "all-MiniLM-L6-v2"
    pub fallback_dimensions: usize, // 384
}

pub struct ChatModelConfig {
    /// Primary model
    pub model: String,              // Default: "llama3.1:8b"
    
    /// Lightweight fallback
    pub fallback_model: String,     // "phi-3-mini" or "mistral:7b"
    
    /// Cloud fallback (opt-in)
    pub cloud_provider: Option<CloudProvider>,
}

pub struct FallbackConfig {
    /// RAM threshold for fallback (MB)
    pub ram_threshold: usize,       // Default: 8192 (8GB)
    
    /// Auto-detect and switch
    pub auto_fallback: bool,        // Default: true
}
```

---

### 4.5 Coordination & Sync Layer

#### 4.5.1 CRDT Strategy

| Data Type | CRDT Type | Implementation |
|-----------|-----------|----------------|
| Document text | Y.Text | Yrs (Yjs Rust port) |
| File metadata | LWW-Register | Custom |
| User preferences | LWW-Map | Custom |
| KG edges | OR-Set | Custom |
| Agent state | Event sourcing | Not CRDT (manual) |
| KG properties | Event sourcing | Not CRDT (manual) |

#### 4.5.2 Sync Protocol

```rust
pub struct SyncManager {
    /// Local event store
    event_store: Arc<EventStore>,
    
    /// CRDT documents
    yrs_docs: HashMap<SpaceId, Doc>,
    
    /// Network manager
    network: Arc<NetworkManager>,
}

impl SyncManager {
    /// Sync with a peer
    pub async fn sync_with_peer(&self, peer: PeerId, space: SpaceId) 
        -> Result<SyncResult, SyncError>;
    
    /// Apply remote changes
    pub async fn apply_remote(&self, changes: Vec<Change>) 
        -> Result<ApplyResult, SyncError>;
    
    /// Get local changes since timestamp
    pub async fn get_changes_since(&self, since: DateTime<Utc>) 
        -> Result<Vec<Change>, SyncError>;
    
    /// Resolve conflicts (for non-CRDT data)
    pub async fn resolve_conflict(&self, conflict: Conflict, resolution: Resolution) 
        -> Result<(), SyncError>;
}
```

#### 4.5.3 Conflict Resolution

```rust
pub enum ConflictResolution {
    /// Automatic (CRDTs)
    Auto,
    
    /// Last-write-wins
    LastWriteWins,
    
    /// Manual (user decides)
    Manual(ManualResolution),
}

pub struct Conflict {
    pub entity_id: ContentId,
    pub local_version: ContentId,
    pub remote_version: ContentId,
    pub conflict_type: ConflictType,
}

pub enum ConflictType {
    ConcurrentEdit,
    DeletedRemotely,
    SchemaConflict,
}
```

---

### 4.6 Agent Layer

#### 4.6.1 Workflow Definition

Workflows can be defined in multiple formats:

```rust
pub enum WorkflowDefinition {
    /// Rust code (compiled into node)
    Native(fn(Context) -> Result<Output, Error>),
    
    /// DSL/YAML
    Dsl(DslWorkflow),
    
    /// DAG of steps
    Dag(DagWorkflow),
    
    /// MCP tool definition
    Mcp(McpTool),
}

pub struct DslWorkflow {
    pub name: String,
    pub description: String,
    pub inputs: Vec<InputSchema>,
    pub outputs: Vec<OutputSchema>,
    pub steps: Vec<DslStep>,
}

pub struct DagWorkflow {
    pub name: String,
    pub nodes: Vec<DagNode>,
    pub edges: Vec<DagEdge>,
}

pub struct McpTool {
    pub name: String,
    pub description: String,
    pub input_schema: JsonSchema,
    pub output_schema: JsonSchema,
}
```

#### 4.6.2 Execution Context

```rust
pub enum ExecutionMode {
    /// In-process (trusted, Flow-shipped only)
    InProcess,
    
    /// WASM sandbox (default for user/third-party)
    Wasm(WasmConfig),
    
    /// Remote execution (distributed compute)
    Remote(RemoteConfig),
}

pub struct WasmConfig {
    /// Fuel limit (computation budget)
    pub fuel_limit: u64,
    
    /// Memory limit (bytes)
    pub memory_limit: usize,
    
    /// Allowed capabilities
    pub capabilities: Vec<WasmCapability>,
}

pub struct ExecutionContext {
    /// Workflow being executed
    pub workflow_id: WorkflowId,
    
    /// Agent executing
    pub agent_did: Did,
    
    /// Delegated capabilities
    pub capabilities: Vec<Capability>,
    
    /// Execution mode
    pub mode: ExecutionMode,
    
    /// Checkpoint store (for resume)
    pub checkpoints: Arc<CheckpointStore>,
}
```

#### 4.6.3 Agent Identity

```rust
pub struct Agent {
    /// Agent's own DID
    pub did: Did,
    
    /// Owner (delegating user)
    pub owner: Did,
    
    /// Delegated capabilities
    pub capabilities: Vec<Capability>,
    
    /// Agent metadata
    pub metadata: AgentMetadata,
}

impl Agent {
    /// Check if agent can perform action
    pub fn can_perform(&self, action: &Action) -> Result<bool, CapabilityError>;
    
    /// Execute action with audit logging
    pub async fn execute(&self, action: Action, context: &ExecutionContext) 
        -> Result<ActionResult, ExecutionError>;
    
    /// Delegate capability to another agent
    pub async fn delegate(&self, to: &Did, capability: Capability) 
        -> Result<Capability, DelegationError>;
}
```

#### 4.6.4 Workflow Persistence

```rust
pub struct CheckpointStore {
    /// RocksDB backend
    store: Arc<RocksDbKvStore>,
}

impl CheckpointStore {
    /// Save checkpoint
    pub async fn save(&self, workflow_id: &WorkflowId, checkpoint: Checkpoint) 
        -> Result<(), StoreError>;
    
    /// Load latest checkpoint
    pub async fn load(&self, workflow_id: &WorkflowId) 
        -> Result<Option<Checkpoint>, StoreError>;
    
    /// Resume from checkpoint
    pub async fn resume(&self, workflow_id: &WorkflowId) 
        -> Result<ExecutionState, ResumeError>;
}

pub struct Checkpoint {
    pub workflow_id: WorkflowId,
    pub step_index: usize,
    pub state: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub event_ids: Vec<Ulid>,  // Events that led to this state
}
```

#### 4.6.5 MVP Workflows

```rust
// Example workflow definitions for MVP

/// Summarize all documents in a folder
pub struct SummarizeFolderWorkflow {
    pub folder_path: String,
    pub max_length: Option<usize>,
}

/// Extract action items from meeting notes
pub struct ExtractActionItemsWorkflow {
    pub document_ids: Vec<ContentId>,
    pub assignees: Option<Vec<Did>>,
}

/// Crypto trading agent (example of delegated autonomy)
pub struct TradingAgentWorkflow {
    pub wallet_capability: Capability,  // Delegated wallet access
    pub duration: Duration,
    pub strategy: TradingStrategy,
    pub risk_limits: RiskLimits,
}
```

---

## 5. Data Models

### 5.1 Document Chunking

```rust
pub struct ChunkingConfig {
    /// Strategy per document type
    pub strategies: HashMap<DocumentType, ChunkingStrategy>,
    
    /// Default strategy
    pub default: ChunkingStrategy,
}

pub enum ChunkingStrategy {
    /// Fixed size chunks
    Fixed { size: usize, overlap: usize },
    
    /// Semantic chunking (paragraphs)
    Semantic { max_size: usize },
    
    /// AST-aware (for code)
    AstAware { granularity: AstGranularity },
    
    /// Hierarchical (headers -> sections)
    Hierarchical { levels: Vec<String> },
    
    /// Smart (auto-detect)
    Smart,
}

pub enum AstGranularity {
    Function,
    Class,
    Module,
}

// Default strategies per document type
impl Default for ChunkingConfig {
    fn default() -> Self {
        let mut strategies = HashMap::new();
        strategies.insert(DocumentType::Code, ChunkingStrategy::AstAware { 
            granularity: AstGranularity::Function 
        });
        strategies.insert(DocumentType::Markdown, ChunkingStrategy::Hierarchical { 
            levels: vec!["#", "##", "###"].into_iter().map(String::from).collect() 
        });
        strategies.insert(DocumentType::Pdf, ChunkingStrategy::Smart);
        strategies.insert(DocumentType::PlainText, ChunkingStrategy::Semantic { 
            max_size: 1000 
        });
        
        Self {
            strategies,
            default: ChunkingStrategy::Smart,
        }
    }
}
```

### 5.2 Chunk Hierarchy

```rust
pub struct DocumentChunks {
    pub document_id: ContentId,
    pub chunks: Vec<Chunk>,
    pub hierarchy: ChunkTree,
}

pub struct Chunk {
    pub id: ContentId,
    pub content: String,
    pub embedding: Vec<f32>,
    pub position: ChunkPosition,
    pub parent_id: Option<ContentId>,
    pub metadata: ChunkMetadata,
}

pub struct ChunkPosition {
    pub start_offset: usize,
    pub end_offset: usize,
    pub line_start: usize,
    pub line_end: usize,
}

pub struct ChunkMetadata {
    pub document_type: DocumentType,
    pub language: Option<String>,
    pub heading: Option<String>,
    pub section_path: Vec<String>,
}
```

### 5.3 Temporal Modeling

```rust
pub struct TemporalEntity {
    /// Current version
    pub current: Entity,
    
    /// Version chain (via prev_version CIDs)
    pub history: Vec<ContentId>,
}

pub struct TemporalEdge {
    /// Edge data
    pub edge: Edge,
    
    /// When this relationship is/was valid
    pub valid_from: Option<DateTime<Utc>>,
    pub valid_until: Option<DateTime<Utc>>,
    
    /// When this edge was recorded
    pub created_at: DateTime<Utc>,
}

impl TemporalEntity {
    /// Get entity state at a specific time
    pub async fn at_time(&self, time: DateTime<Utc>, store: &impl KnowledgeGraphStore) 
        -> Result<Option<Entity>, KgError>;
    
    /// Get all versions
    pub async fn versions(&self, store: &impl KnowledgeGraphStore) 
        -> Result<Vec<Entity>, KgError>;
}
```

---

## 6. API Specifications

### 6.1 REST API

Base URL: `/api/v1`

#### Authentication

```
POST /auth/webauthn/start-registration
Response: { challenge: CreationChallengeResponse, challenge_id: string }

POST /auth/webauthn/finish-registration
Body: { challenge_id: string, credential: RegisterPublicKeyCredential }
Response: { user_id: string, did: string }

POST /auth/webauthn/start-authentication
Response: { challenge: RequestChallengeResponse, challenge_id: string }

POST /auth/webauthn/finish-authentication
Body: { challenge_id: string, credential: PublicKeyCredential }
Response: { token: string, user: User }
```

#### Spaces

```
GET /spaces
Response: { spaces: Space[] }

POST /spaces
Body: { path: string, name?: string }
Response: { space: Space }

GET /spaces/:key
Response: { space: Space, index_status: IndexStatus }

DELETE /spaces/:key
Response: { success: boolean }

POST /spaces/:key/share
Body: { user_did: string, capabilities: Action[], expires_at?: string }
Response: { capability: Capability }

POST /spaces/:key/query
Body: { query: string }
Response: { answer: string, sources: Source[] }
```

#### Knowledge Graph

```
GET /kg/entities
Query: { space?: string, type?: EntityType, limit?: number, offset?: number }
Response: { entities: Entity[], total: number }

POST /kg/entities
Body: { entity: CreateEntity }
Response: { entity: Entity }

GET /kg/entities/:id
Response: { entity: Entity, edges: Edge[] }

PUT /kg/entities/:id
Body: { properties: Record<string, any> }
Response: { entity: Entity }

DELETE /kg/entities/:id
Response: { success: boolean }

GET /kg/entities/:id/edges
Query: { direction?: "in" | "out" | "both", type?: EdgeType }
Response: { edges: Edge[] }

POST /kg/edges
Body: { edge: CreateEdge }
Response: { edge: Edge }

DELETE /kg/edges/:id
Response: { success: boolean }
```

#### Network

```
GET /network/status
Response: { 
  running: boolean, 
  peer_id: string, 
  peer_count: number,
  topics: string[] 
}

GET /network/peers
Response: { peers: PeerInfo[] }

POST /network/dial
Body: { address: string }
Response: { success: boolean }

POST /network/start
Response: { success: boolean }

POST /network/stop
Response: { success: boolean }
```

#### Workflows

```
GET /workflows
Response: { workflows: WorkflowDefinition[] }

POST /workflows
Body: { definition: WorkflowDefinition }
Response: { workflow: Workflow }

POST /workflows/:id/execute
Body: { inputs: Record<string, any> }
Response: { execution_id: string, status: "pending" }

GET /workflows/executions/:id
Response: { execution: WorkflowExecution }

POST /workflows/executions/:id/cancel
Response: { success: boolean }
```

### 6.2 WebSocket API

Connection: `ws://localhost:8081/ws`

#### Messages (Client -> Server)

```typescript
interface ClientMessage {
  type: "subscribe" | "unsubscribe" | "query" | "sync";
  id: string;  // Request ID for correlation
  payload: any;
}

// Subscribe to space updates
{ type: "subscribe", id: "1", payload: { topic: "space:<key>" } }

// Query knowledge graph
{ type: "query", id: "2", payload: { query: "..." } }

// Sync request
{ type: "sync", id: "3", payload: { space: "<key>", since: "<timestamp>" } }
```

#### Messages (Server -> Client)

```typescript
interface ServerMessage {
  type: "response" | "event" | "error";
  id?: string;  // Correlates with request
  payload: any;
}

// Response to query
{ type: "response", id: "2", payload: { answer: "...", sources: [...] } }

// Event notification
{ type: "event", payload: { topic: "space:abc", event: {...} } }

// Error
{ type: "error", id: "1", payload: { code: "NOT_FOUND", message: "..." } }
```

### 6.3 GraphQL API

```graphql
type Query {
  # Spaces
  spaces: [Space!]!
  space(key: ID!): Space
  
  # Knowledge Graph
  entities(
    spaceKey: ID
    type: EntityType
    first: Int
    after: String
  ): EntityConnection!
  
  entity(id: ID!): Entity
  
  # Traversal query
  traverse(
    from: ID!
    pattern: String!  # e.g., "(p:Person)-[:created*1..3]->(d:Document)"
    limit: Int
  ): [Path!]!
  
  # Federated query across spaces
  federatedQuery(
    spaces: [ID!]!
    query: String!
  ): QueryResult!
  
  # Semantic search
  search(
    query: String!
    spaceKey: ID
    limit: Int
  ): [SearchResult!]!
}

type Mutation {
  # Spaces
  createSpace(input: CreateSpaceInput!): Space!
  deleteSpace(key: ID!): Boolean!
  shareSpace(input: ShareSpaceInput!): Capability!
  
  # Entities
  createEntity(input: CreateEntityInput!): Entity!
  updateEntity(id: ID!, input: UpdateEntityInput!): Entity!
  deleteEntity(id: ID!): Boolean!
  
  # Edges
  createEdge(input: CreateEdgeInput!): Edge!
  deleteEdge(id: ID!): Boolean!
  
  # Workflows
  executeWorkflow(id: ID!, inputs: JSON): WorkflowExecution!
}

type Subscription {
  # Space updates
  spaceUpdated(key: ID!): SpaceEvent!
  
  # Entity changes
  entityChanged(spaceKey: ID!): EntityEvent!
  
  # Sync status
  syncStatus(spaceKey: ID!): SyncStatus!
  
  # Workflow progress
  workflowProgress(executionId: ID!): WorkflowProgress!
}
```

### 6.4 MCP Integration

Flow acts as both MCP server (exposing tools) and client (using external tools).

#### Flow as MCP Server

```json
{
  "name": "flow",
  "version": "1.0.0",
  "tools": [
    {
      "name": "query_space",
      "description": "Query a knowledge space using natural language",
      "inputSchema": {
        "type": "object",
        "properties": {
          "space_key": { "type": "string" },
          "query": { "type": "string" }
        },
        "required": ["space_key", "query"]
      }
    },
    {
      "name": "search_entities",
      "description": "Search for entities in the knowledge graph",
      "inputSchema": {
        "type": "object",
        "properties": {
          "query": { "type": "string" },
          "entity_type": { "type": "string" },
          "limit": { "type": "integer" }
        },
        "required": ["query"]
      }
    },
    {
      "name": "create_entity",
      "description": "Create a new entity in the knowledge graph",
      "inputSchema": {
        "type": "object",
        "properties": {
          "space_key": { "type": "string" },
          "entity_type": { "type": "string" },
          "properties": { "type": "object" }
        },
        "required": ["space_key", "entity_type", "properties"]
      }
    }
  ]
}
```

---

## 7. Security Model

### 7.1 Threat Model

| Threat | Mitigation |
|--------|------------|
| Stolen device | Per-Space encryption, Secure Enclave keys |
| Malicious peer | Reputation scoring, escalating response |
| Capability abuse | Time/usage limits, cascading revocation |
| Man-in-the-middle | E2E encryption, Noise protocol |
| Replay attacks | Nonces, timestamps, sequence numbers |
| Sybil attacks | Distributed reputation, weighted trust |

### 7.2 Encryption Summary

| Context | Encryption | Key Storage |
|---------|------------|-------------|
| At rest (Space data) | AES-256-GCM | Secure Enclave/TPM |
| In transit (P2P) | E2E + Noise | Per-Space key |
| Event signatures | Ed25519 | Node keystore |
| Capability credentials | Ed25519 | Secure Enclave |

### 7.3 Audit Logging

```rust
pub struct AuditLog {
    pub event_id: Ulid,
    pub timestamp: DateTime<Utc>,
    pub actor: Did,              // Who performed action
    pub action: AuditAction,
    pub resource: ResourceId,
    pub result: AuditResult,
    pub metadata: serde_json::Value,
}

pub enum AuditAction {
    // Always logged
    Authenticate,
    DelegateCapability,
    RevokeCapability,
    CreateEntity,
    UpdateEntity,
    DeleteEntity,
    
    // Opt-in per Space
    ReadEntity,
    QueryKnowledgeGraph,
}

pub enum AuditResult {
    Success,
    Failure(String),
    Denied(String),
}
```

### 7.4 Malicious Peer Response

```rust
pub struct PeerReputation {
    pub peer_id: PeerId,
    pub score: f32,                 // 0.0 - 1.0
    pub violations: Vec<Violation>,
    pub last_seen: DateTime<Utc>,
}

pub enum ResponseLevel {
    /// Reduce reputation score
    ScoreReduction(f32),
    
    /// Temporary disconnect
    TempDisconnect(Duration),
    
    /// Local blacklist
    Blacklist,
    
    /// Network-wide report
    NetworkReport,
}

impl PeerReputation {
    /// Determine response based on violation severity
    pub fn respond_to_violation(&mut self, violation: &Violation) -> ResponseLevel {
        match violation.severity {
            Severity::Minor => {
                self.score -= 0.1;
                ResponseLevel::ScoreReduction(0.1)
            }
            Severity::Moderate => {
                let backoff = self.exponential_backoff();
                ResponseLevel::TempDisconnect(backoff)
            }
            Severity::Severe => {
                ResponseLevel::Blacklist
            }
        }
    }
    
    fn exponential_backoff(&self) -> Duration {
        let minutes = 2u64.pow(self.violations.len().min(10) as u32);
        Duration::from_secs(minutes * 60)
    }
}
```

---

## 8. Performance Requirements

### 8.1 Latency Targets (P95)

| Operation | Target | Notes |
|-----------|--------|-------|
| Simple KG query | <50ms | Local index lookup |
| RAG query (with LLM) | <3000ms | <500ms to first token |
| Full-text search | <100ms | Index lookup |
| Sync (small delta) | <500ms | Network RTT + apply |
| Entity creation | <100ms | Write + index update |
| Workflow step | <1000ms | Varies by step type |

### 8.2 Throughput Targets

| Metric | Target |
|--------|--------|
| Concurrent WebSocket connections | 1,000+ |
| API requests/second (single node) | 500+ |
| Documents indexed/minute | 100+ |
| Sync operations/second | 50+ |

### 8.3 Scale Targets

| Metric | Personal Node | Server Node |
|--------|---------------|-------------|
| Documents | 100,000+ | 1,000,000+ |
| Spaces | 100+ | 1,000+ |
| Entities (KG) | 1,000,000+ | 10,000,000+ |
| Concurrent editors/Space | 10 | 100+ |
| Storage | 100GB+ | 1TB+ |

### 8.4 Resource Limits

```rust
pub struct ResourceLimits {
    // Memory
    pub max_memory_mb: usize,           // Default: 2048
    pub embedding_cache_mb: usize,      // Default: 512
    
    // CPU
    pub max_indexing_threads: usize,    // Default: num_cpus / 2
    pub max_query_threads: usize,       // Default: 4
    
    // Storage
    pub warning_threshold_percent: u8,  // Default: 80
    pub critical_threshold_percent: u8, // Default: 95
    
    // Network
    pub max_connections: u32,           // Default: 200
    pub max_bandwidth_mbps: Option<u32>,// Default: None (unlimited)
    
    // WASM
    pub wasm_fuel_limit: u64,           // Default: 10_000_000
    pub wasm_memory_mb: usize,          // Default: 256
}
```

---

## 9. UI/UX Specifications

### 9.1 Web Application

#### 9.1.1 Information Architecture

```
Flow Web
├── Space Browser (Default View)
│   ├── Space List
│   ├── Document Tree
│   └── Preview Panel
├── Search
│   ├── Full-text Search
│   ├── Semantic Search
│   └── Filters (type, date, space)
├── Graph View (On-demand)
│   ├── Entity Graph
│   ├── Relationship Explorer
│   └── Path Visualization
├── Chat Sidebar (Persistent)
│   ├── Query Input
│   ├── Response Display
│   └── Source Citations
├── Activity Feed
│   ├── Recent Changes
│   ├── Sync Status
│   └── Notifications
└── Settings
    ├── Profile
    ├── Spaces
    ├── Security
    ├── AI Models
    └── Network
```

#### 9.1.2 Component Hierarchy

```
App
├── Layout
│   ├── Sidebar
│   │   ├── SpaceList
│   │   └── Navigation
│   ├── MainContent
│   │   ├── SpaceBrowser
│   │   ├── SearchResults
│   │   ├── GraphView
│   │   └── Settings
│   └── ChatPanel
│       ├── MessageList
│       └── QueryInput
├── Modals
│   ├── CreateSpace
│   ├── ShareSpace
│   ├── ConflictResolution
│   └── WorkflowBuilder
└── Notifications
    ├── Toast
    └── SyncStatus
```

### 9.2 Mobile Application

#### 9.2.1 Navigation Structure

```
Flow Mobile (Tab-based)
├── Home (Tab 1)
│   ├── Recent Spaces
│   ├── Quick Actions
│   └── Activity Feed
├── Spaces (Tab 2)
│   ├── Space List
│   └── Space Detail
│       ├── Documents
│       └── Graph
├── Capture (Tab 3)
│   ├── Photo Capture
│   ├── Voice Note
│   └── Quick Note
├── Chat (Tab 4)
│   ├── Query Interface
│   └── History
└── Settings (Tab 5)
    ├── Account
    ├── Offline
    └── Sync
```

#### 9.2.2 Offline Configuration

```rust
pub struct OfflineConfig {
    /// Maximum storage for offline content
    pub max_storage_mb: usize,          // Default: 2048 (2GB)
    
    /// Pinned spaces (always available offline)
    pub pinned_spaces: Vec<SpaceId>,
    
    /// LRU cache for recent content
    pub cache_recent: bool,             // Default: true
    
    /// Auto-eviction when storage full
    pub auto_evict: bool,               // Default: true
}
```

### 9.3 Desktop Application (Electron)

#### 9.3.1 Additional Features

```rust
pub struct DesktopFeatures {
    /// System tray integration
    pub system_tray: SystemTrayConfig,
    
    /// File system watcher
    pub file_watcher: FileWatcherConfig,
    
    /// Native notifications
    pub notifications: NotificationConfig,
    
    /// Global keyboard shortcuts
    pub shortcuts: ShortcutConfig,
}

pub struct FileWatcherConfig {
    /// Directories to watch
    pub watch_dirs: Vec<PathBuf>,
    
    /// Use native APIs (inotify/FSEvents)
    pub use_native: bool,               // Default: true
    
    /// Poll interval (fallback)
    pub poll_interval_ms: u64,          // Default: 5000
    
    /// Auto-index new files
    pub auto_index: bool,               // Default: true
}
```

### 9.4 Onboarding Flow

```
┌─────────────────────────────────────────────────────────────┐
│                     Welcome to Flow                          │
│                                                              │
│  ┌───────────────────┐    ┌───────────────────┐            │
│  │   New to Flow?    │    │  Have an Account? │            │
│  │                   │    │                   │            │
│  │  [Create Account] │    │  [Sign In]        │            │
│  └───────────────────┘    └───────────────────┘            │
└─────────────────────────────────────────────────────────────┘
                              │
              ┌───────────────┴───────────────┐
              ▼                               ▼
┌─────────────────────────┐    ┌─────────────────────────────┐
│    Create Account       │    │        Sign In              │
│                         │    │                             │
│  1. Create Passkey      │    │  1. Authenticate with       │
│     (Biometric prompt)  │    │     existing passkey        │
│                         │    │                             │
│  2. Backup codes shown  │    │  2. Sync spaces from        │
│     (Save these!)       │    │     other devices           │
│                         │    │                             │
│  3. Create first Space  │    │  3. Ready!                  │
│     (optional)          │    │                             │
│                         │    └─────────────────────────────┘
│  4. Tutorial (optional) │
│                         │
│  5. Ready!              │
└─────────────────────────┘
```

---

## 10. Deployment & Operations

### 10.1 Distribution

| Channel | Format | Auto-Update |
|---------|--------|-------------|
| GitHub Releases | Binary | Manual |
| Homebrew | Formula | brew upgrade |
| apt/yum | Package | apt upgrade |
| Docker Hub | Image | Pull new tag |
| App Stores | App bundle | Store update |
| crates.io | Rust crate | cargo update |
| npm | WASM package | npm update |

### 10.2 Configuration

Configuration precedence: Environment > Config File > UI Settings > Defaults

```toml
# flow.toml

[database]
url = "sqlite:///path/to/flow.db"  # or postgres://...
max_connections = 100
min_connections = 5

[storage]
data_dir = "~/.flow/data"
kv_path = "~/.flow/kv"

[network]
listen_port = 4001
enable_quic = true
bootstrap_peers = [
  "/dns4/bootstrap1.flow.network/tcp/4001/p2p/12D3...",
  "/dns4/bootstrap2.flow.network/tcp/4001/p2p/12D3..."
]

[network.mdns]
enabled = true

[ai]
embedding_model = "nomic-embed-text"
chat_model = "llama3.1:8b"
local_only = true

[server]
rest_port = 8080
websocket_port = 8081
graphql_port = 8082
host = "0.0.0.0"

[security]
encryption_at_rest = true
audit_logging = true
```

### 10.3 Observability

```rust
pub struct ObservabilityConfig {
    /// Structured logging
    pub logging: LoggingConfig,
    
    /// Prometheus metrics
    pub metrics: MetricsConfig,
    
    /// OpenTelemetry tracing
    pub tracing: TracingConfig,
    
    /// Health checks
    pub health: HealthConfig,
}

pub struct MetricsConfig {
    pub enabled: bool,                  // Default: true
    pub endpoint: String,               // Default: "/metrics"
    pub port: u16,                      // Default: 9090
}

pub struct TracingConfig {
    pub enabled: bool,                  // Default: false
    pub otlp_endpoint: Option<String>,
    pub sample_rate: f64,               // Default: 0.1
}
```

#### Key Metrics

```
# Node metrics
flow_node_uptime_seconds
flow_node_memory_bytes
flow_node_storage_bytes

# API metrics
flow_api_requests_total{method, path, status}
flow_api_request_duration_seconds{method, path}

# Network metrics
flow_network_peers_connected
flow_network_messages_sent_total{topic}
flow_network_messages_received_total{topic}
flow_network_bandwidth_bytes{direction}

# Knowledge Graph metrics
flow_kg_entities_total{type}
flow_kg_edges_total{type}
flow_kg_queries_total
flow_kg_query_duration_seconds

# Sync metrics
flow_sync_operations_total{status}
flow_sync_conflicts_total
flow_sync_lag_seconds

# Agent metrics
flow_agent_workflows_total{status}
flow_agent_workflow_duration_seconds
flow_agent_wasm_fuel_used
```

### 10.4 Crash Recovery

```rust
pub enum DeploymentMode {
    /// Standalone binary
    Standalone,
    
    /// Supervised (systemd, launchd)
    Supervised,
    
    /// Container (Docker, K8s)
    Container,
}

impl Node {
    /// Graceful shutdown
    pub async fn shutdown(&self) -> Result<(), ShutdownError> {
        // 1. Stop accepting new requests
        self.api.stop_accepting().await?;
        
        // 2. Complete in-flight requests (with timeout)
        self.api.drain(Duration::from_secs(30)).await?;
        
        // 3. Flush pending writes
        self.kv.flush()?;
        self.event_store.flush().await?;
        
        // 4. Stop network
        self.network.stop().await?;
        
        // 5. Close database connections
        self.db.close().await?;
        
        Ok(())
    }
    
    /// Recovery on startup
    pub async fn recover(&self) -> Result<RecoveryResult, RecoveryError> {
        // 1. Verify event store integrity
        let events_ok = self.event_store.verify_chain().await?;
        
        // 2. Replay uncommitted events
        let replayed = self.replay_pending_events().await?;
        
        // 3. Rebuild indexes if needed
        if self.indexes_stale() {
            self.rebuild_indexes().await?;
        }
        
        // 4. Resume interrupted workflows
        let workflows = self.resume_workflows().await?;
        
        Ok(RecoveryResult { events_ok, replayed, workflows })
    }
}
```

### 10.5 Update Strategy

```rust
pub enum UpdateStrategy {
    /// Check only, user initiates
    Manual,
    
    /// Notify, one-click install
    NotifyAndInstall,             // Default
    
    /// Download and install automatically
    Automatic,
}

pub struct UpdateConfig {
    pub strategy: UpdateStrategy,
    pub check_interval: Duration,       // Default: 24 hours
    pub update_channel: UpdateChannel,  // stable, beta, nightly
    pub auto_restart: bool,             // Default: false
}
```

---

## 11. Testing Requirements

### 11.1 Coverage Targets

| Layer | Unit | Integration | E2E |
|-------|------|-------------|-----|
| Storage | 90% | 90% | - |
| Auth | 90% | 90% | - |
| Network | 90% | 90% | - |
| Knowledge Graph | 90% | 90% | - |
| Agent | 90% | 90% | - |
| API | - | 90% | 80% |
| UI | - | - | 80% |

### 11.2 Test Categories

```rust
// Unit tests - per module
#[cfg(test)]
mod tests {
    #[test]
    fn test_content_id_roundtrip() { ... }
    
    #[tokio::test]
    async fn test_event_append() { ... }
}

// Integration tests - cross-module
// tests/integration/
#[tokio::test]
async fn test_space_creation_indexes_documents() { ... }

#[tokio::test]
async fn test_capability_delegation_chain() { ... }

// E2E tests - full stack
// tests/e2e/
#[tokio::test]
async fn test_user_creates_space_and_queries() { ... }

#[tokio::test]
async fn test_two_users_collaborate_with_sync() { ... }
```

### 11.3 CI/CD Pipeline

```yaml
# .github/workflows/ci.yml
name: CI

on: [push, pull_request]

jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      
      # Rust checks
      - name: Clippy
        run: cargo clippy --all-targets -- -D warnings
      
      - name: Format
        run: cargo fmt --check
      
      - name: Type check
        run: cargo check --all-targets
      
      # JS/TS checks
      - name: ESLint
        run: nx lint user-interface
      
      - name: TypeScript
        run: nx typecheck user-interface

  test:
    runs-on: ubuntu-latest
    steps:
      - name: Unit tests
        run: cargo test --lib
      
      - name: Integration tests
        run: cargo test --test '*'
      
      - name: Frontend tests
        run: nx test user-interface

  security:
    runs-on: ubuntu-latest
    steps:
      - name: Cargo audit
        run: cargo audit
      
      - name: npm audit
        run: npm audit --prefix user-interface/flow-web

  benchmark:
    runs-on: ubuntu-latest
    steps:
      - name: Run benchmarks
        run: cargo bench --no-run  # Compile only, compare on merge
      
      - name: Compare with main
        run: ./scripts/compare-benchmarks.sh

  e2e:
    runs-on: ubuntu-latest
    steps:
      - name: Start services
        run: docker-compose up -d
      
      - name: Run E2E tests
        run: cargo test --test e2e
      
      - name: Stop services
        run: docker-compose down
```

### 11.4 Property-Based Tests

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn content_id_deterministic(data: Vec<u8>) {
        let cid1 = ContentId::from_bytes(&data);
        let cid2 = ContentId::from_bytes(&data);
        prop_assert_eq!(cid1, cid2);
    }
    
    #[test]
    fn crdt_merge_commutative(ops1: Vec<Op>, ops2: Vec<Op>) {
        let result1 = merge(apply(ops1.clone()), apply(ops2.clone()));
        let result2 = merge(apply(ops2), apply(ops1));
        prop_assert_eq!(result1, result2);
    }
    
    #[test]
    fn capability_chain_validates(chain: Vec<Capability>) {
        if chain.is_valid() {
            // Valid chains must trace back to a root authority
            prop_assert!(chain.first().unwrap().issuer.is_root());
        }
    }
}
```

---

## 12. Implementation Task Breakdown

### 12.1 Phase 1: Solo User MVP

#### Milestone 1.1: Core Storage (Week 1-2)

| Task | Priority | Estimate | Dependencies |
|------|----------|----------|--------------|
| Implement IPLD block store | P0 | 3d | - |
| Add per-Space encryption | P0 | 2d | Block store |
| Event store with hash chains | P0 | 3d | Block store |
| SQLite migrations for KG schema | P0 | 2d | - |

#### Milestone 1.2: Knowledge Graph (Week 3-4)

| Task | Priority | Estimate | Dependencies |
|------|----------|----------|--------------|
| Entity CRUD operations | P0 | 2d | Storage |
| Edge CRUD operations | P0 | 2d | Entities |
| Basic graph traversal | P0 | 3d | Edges |
| Entity extraction pipeline | P0 | 4d | Traversal |
| Embedding generation | P0 | 2d | - |
| Vector search integration | P0 | 3d | Embeddings |

#### Milestone 1.3: Indexing Pipeline (Week 5-6)

| Task | Priority | Estimate | Dependencies |
|------|----------|----------|--------------|
| Document type detection | P0 | 1d | - |
| Smart chunking implementation | P0 | 3d | Type detection |
| Chunk hierarchy storage | P0 | 2d | Chunking |
| PDF parsing | P0 | 2d | Chunking |
| Office doc parsing | P0 | 2d | Chunking |
| Code AST chunking | P0 | 3d | Chunking |
| Background indexing queue | P0 | 2d | All parsers |

#### Milestone 1.4: Query System (Week 7-8)

| Task | Priority | Estimate | Dependencies |
|------|----------|----------|--------------|
| RAG pipeline setup | P0 | 3d | KG, Indexing |
| Query reformulation | P0 | 2d | RAG |
| Source citation | P0 | 2d | RAG |
| Temporal queries | P1 | 3d | RAG |
| Inference engine (basic) | P1 | 4d | KG |

#### Milestone 1.5: Web UI (Week 9-10)

| Task | Priority | Estimate | Dependencies |
|------|----------|----------|--------------|
| Space browser component | P0 | 3d | API |
| Document preview | P0 | 2d | Browser |
| Chat interface | P0 | 3d | Query API |
| Search UI | P0 | 2d | Search API |
| Settings panel | P1 | 2d | - |
| Passkey auth flow | P0 | 2d | Auth API |

### 12.2 Phase 2: Collaboration

#### Milestone 2.1: Sync Infrastructure (Week 11-12)

| Task | Priority | Estimate | Dependencies |
|------|----------|----------|--------------|
| Yrs integration | P0 | 3d | - |
| Document CRDT wrapper | P0 | 2d | Yrs |
| Sync protocol design | P0 | 2d | CRDT |
| Delta generation | P0 | 2d | Protocol |
| Delta application | P0 | 2d | Protocol |
| Conflict detection | P0 | 2d | Deltas |

#### Milestone 2.2: Sharing & Permissions (Week 13-14)

| Task | Priority | Estimate | Dependencies |
|------|----------|----------|--------------|
| Capability creation | P0 | 2d | Auth |
| Capability verification | P0 | 2d | Creation |
| Delegation chain validation | P0 | 3d | Verification |
| Revocation cascade | P0 | 2d | Chain |
| Share UI | P0 | 2d | API |
| Permissions display | P0 | 1d | UI |

#### Milestone 2.3: Real-time Sync (Week 15-16)

| Task | Priority | Estimate | Dependencies |
|------|----------|----------|--------------|
| WebSocket sync channel | P0 | 2d | Sync infra |
| Presence indicators | P1 | 2d | WebSocket |
| Cursor sharing | P1 | 2d | WebSocket |
| Conflict resolution UI | P0 | 3d | Sync |
| Sync status indicators | P0 | 2d | UI |

### 12.3 Phase 3: Network

#### Milestone 3.1: Bootstrap & Discovery (Week 17-18)

| Task | Priority | Estimate | Dependencies |
|------|----------|----------|--------------|
| Bootstrap node setup | P0 | 2d | Network |
| Kademlia tuning | P0 | 2d | Bootstrap |
| Peer discovery optimization | P0 | 3d | Kademlia |
| NAT detection | P0 | 2d | - |
| Relay integration | P0 | 3d | NAT |
| Hole punching | P1 | 3d | NAT |

#### Milestone 3.2: Content Routing (Week 19-20)

| Task | Priority | Estimate | Dependencies |
|------|----------|----------|--------------|
| Provider announcements | P0 | 2d | Network |
| Provider discovery | P0 | 2d | Announcements |
| Content fetch with routing | P0 | 3d | Discovery |
| Caching layer | P1 | 2d | Fetch |
| Pinning service interface | P1 | 3d | Routing |

#### Milestone 3.3: GossipSub Topics (Week 21-22)

| Task | Priority | Estimate | Dependencies |
|------|----------|----------|--------------|
| Topic schema implementation | P0 | 2d | Network |
| Scoped topic subscriptions | P0 | 2d | Schema |
| Ephemeral topics | P1 | 2d | Schema |
| Message persistence | P0 | 2d | Topics |
| Topic authorization | P0 | 3d | Auth |

### 12.4 Phase 4: Agents

#### Milestone 4.1: Workflow Engine (Week 23-24)

| Task | Priority | Estimate | Dependencies |
|------|----------|----------|--------------|
| Workflow definition parser | P0 | 3d | - |
| DSL implementation | P0 | 3d | Parser |
| DAG executor | P0 | 4d | Parser |
| MCP tool adapter | P0 | 2d | Executor |

#### Milestone 4.2: WASM Sandbox (Week 25-26)

| Task | Priority | Estimate | Dependencies |
|------|----------|----------|--------------|
| Wasmtime integration | P0 | 3d | - |
| Fuel metering | P0 | 2d | Wasmtime |
| Capability enforcement | P0 | 3d | Wasmtime |
| Host function bindings | P0 | 3d | Sandbox |

#### Milestone 4.3: Agent Identity (Week 27-28)

| Task | Priority | Estimate | Dependencies |
|------|----------|----------|--------------|
| Agent DID generation | P0 | 1d | Auth |
| Capability delegation to agents | P0 | 2d | DID |
| Audit logging for agents | P0 | 2d | Delegation |
| Agent revocation | P0 | 2d | Audit |

#### Milestone 4.4: Persistence & Resume (Week 29-30)

| Task | Priority | Estimate | Dependencies |
|------|----------|----------|--------------|
| Checkpoint store | P0 | 2d | Storage |
| Checkpoint serialization | P0 | 2d | Store |
| Resume logic | P0 | 3d | Checkpoints |
| Event replay for state | P0 | 2d | Events |

### 12.5 Phase 5: SDK

#### Milestone 5.1: TypeScript SDK (Week 31-32)

| Task | Priority | Estimate | Dependencies |
|------|----------|----------|--------------|
| API client generation | P0 | 2d | API |
| Type definitions | P0 | 2d | Client |
| React hooks | P1 | 3d | Types |
| Documentation | P0 | 3d | All |
| Examples | P0 | 2d | Docs |

#### Milestone 5.2: Python SDK (Week 33-34)

| Task | Priority | Estimate | Dependencies |
|------|----------|----------|--------------|
| API client | P0 | 2d | API |
| Type hints | P0 | 2d | Client |
| Async support | P0 | 2d | Client |
| Documentation | P0 | 2d | All |
| Examples | P0 | 2d | Docs |

#### Milestone 5.3: Rust Crate (Week 35-36)

| Task | Priority | Estimate | Dependencies |
|------|----------|----------|--------------|
| Public API cleanup | P0 | 3d | All modules |
| Documentation | P0 | 3d | API |
| Examples | P0 | 2d | Docs |
| crates.io publish | P0 | 1d | All |

---

## Appendix A: Glossary

| Term | Definition |
|------|------------|
| **CID** | Content Identifier - hash-based address for content |
| **CRDT** | Conflict-free Replicated Data Type |
| **DID** | Decentralized Identifier |
| **IPLD** | InterPlanetary Linked Data |
| **KG** | Knowledge Graph |
| **MCP** | Model Context Protocol |
| **RAG** | Retrieval-Augmented Generation |
| **SLRPA** | Sense-Learn-Reason-Predict-Act agent cycle |
| **Space** | User-owned knowledge workspace |
| **VC** | Verifiable Credential |
| **WASM** | WebAssembly |

## Appendix B: Environment Variables

```bash
# Database
DATABASE_URL=sqlite:///path/to/flow.db
DB_MAX_CONNECTIONS=100
DB_MIN_CONNECTIONS=5

# Storage
FLOW_DATA_HOME=~/.flow/data
KV_PATH=~/.flow/kv
BLOCK_STORE_PATH=~/.flow/blocks

# Network
NETWORK_LISTEN_PORT=4001
NETWORK_ENABLE_QUIC=true
BOOTSTRAP_PEERS=/dns4/bootstrap1.flow.network/tcp/4001/p2p/...

# API
REST_PORT=8080
WEBSOCKET_PORT=8081
GRAPHQL_PORT=8082
HOST=0.0.0.0

# AI
EMBEDDING_MODEL=nomic-embed-text
CHAT_MODEL=llama3.1:8b
OLLAMA_HOST=http://localhost:11434
AI_LOCAL_ONLY=true

# Security
WEBAUTHN_RP_ID=localhost
WEBAUTHN_RP_ORIGIN=http://localhost:3000

# Observability
LOG_LEVEL=info
METRICS_ENABLED=true
METRICS_PORT=9090
TRACING_ENABLED=false
```

## Appendix C: File Type Support

### MVP (Phase 1)

| Type | Extensions | Parser | Chunking |
|------|------------|--------|----------|
| Plain text | .txt | Native | Semantic |
| Markdown | .md | Native | Hierarchical |
| Code | .rs, .ts, .py, .js, .go, etc. | tree-sitter | AST-aware |
| PDF | .pdf | pdf-extract | Smart |
| Office | .docx, .xlsx, .pptx | Apache Tika | Smart |

### v1.1 (Phase 2+)

| Type | Extensions | Parser | Processing |
|------|------------|--------|------------|
| Images | .png, .jpg, .webp | OCR + CLIP | Text extraction, embeddings |
| Audio | .mp3, .wav, .m4a | Whisper | Transcription |

### Future

| Type | Extensions | Parser | Processing |
|------|------------|--------|------------|
| Video | .mp4, .mov | Whisper + CLIP | Transcription, frame extraction |

---

## Appendix D: Decision Log

| Date | Decision | Rationale | Alternatives Considered |
|------|----------|-----------|------------------------|
| 2026-01 | IPLD for KG storage | Content-addressed, aligns with storage layer | Neo4j, PostgreSQL |
| 2026-01 | Yrs for text CRDTs | Rust-native, Yjs-compatible | Automerge, custom |
| 2026-01 | Wasmtime for sandbox | Best security, fuel metering | Wasmer, wasm3 |
| 2026-01 | GraphQL for KG queries | Better than REST for graphs | Cypher, SPARQL |
| 2026-01 | Open core licensing | Sustainability + community | MIT, proprietary |
| 2026-01 | Hub-and-spoke -> decentralized | Pragmatic bootstrap | Pure decentralized |
| 2026-01 | Per-Space encryption | Granular sharing, revocation | Per-node encryption |
| 2026-01 | Cascading revocation | Security, legal alignment | Independent subdelegations |

---

*End of Specification*
