# Changelog

All notable changes to Flow will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- MVP Phase 3: Web UI with routing, authentication, and core pages
- MVP Phase 2: Query module with RAG pipeline and semantic search
- MVP Phase 1: Knowledge Graph entity/edge stores, vector store, entity extraction
- MVP Phase 0: KG migrations, JWT auth, CORS configuration
- REST API endpoints for spaces and KG entities
- MVP API reference documentation

### Changed
- Standardized API error responses with ApiError type
- Aligned frontend API types with backend responses

---

## [0.3.0] - 2025-11-25

> **Version Bump:** Minor version for new Bootstrap Node Support feature - prerequisite for GossipSub message routing.

### Added

#### Bootstrap Node Support
- **BootstrapConfig** struct for fine-grained control over bootstrap behavior with configurable options:
  - `auto_dial` - Automatically dial bootstrap peers on startup (default: true)
  - `auto_bootstrap` - Run Kademlia DHT bootstrap query on startup (default: true)
  - `startup_delay_ms` - Delay before bootstrap allowing listener initialization (default: 100ms)
  - `max_retries` - Maximum retry attempts per bootstrap peer (default: 3)
  - `retry_delay_base_ms` - Base delay for exponential backoff (default: 1000ms)
- **Bootstrap peer tracking** via `HashSet<PeerId>` identifying which peers are bootstrap nodes
- **Automatic retry with exponential backoff** for failed bootstrap peer connections
- **Event-loop-driven bootstrap** with scheduled initial trigger after configurable startup delay
- **Manual bootstrap trigger API** via `NetworkManager::trigger_bootstrap()`
- **BootstrapResult** struct providing bootstrap operation outcomes

#### Testing
- 8 comprehensive bootstrap tests covering configuration, peer parsing, retry logic, and integration

### Changed
- Extended **NetworkConfig** with `bootstrap: BootstrapConfig` field
- Extended **NetworkEventLoop** with bootstrap peer state tracking
- Extended **PersistentPeerRegistry** with forwarding methods

---

## [0.2.0] - 2025-11-21

### Added

#### Networking Layer (libp2p Foundation - Phase 1)
- **libp2p P2P networking stack** with full Kademlia DHT implementation
- **NetworkManager** abstraction providing clean API for network operations
- **Ed25519 to libp2p keypair conversion** for existing node keys
- **Multi-transport support** with QUIC (primary) and TCP fallback
- **Noise protocol encryption** with Yamux stream multiplexing
- **Persistent DHT store** using RocksDB
- **Peer registry** with comprehensive connection tracking
- **Network statistics API** exposing peer count and connection stats

#### Storage Layer
- **RocksDB-based KV store** replacing Sled with production-grade storage
- **RocksDB event store** with complete migration from Sled
- **Peer registry persistence** using RocksDB with column families
- **Unified storage configuration** with platform-specific data directory support

#### AI Module (Major Refactoring)
- **Modular AI architecture** splitting monolithic module into 7 focused components
- **Smart language-aware chunking** with tree-sitter support for 15+ languages
- **Failure tracking system** with database persistence
- **Real-time validation framework** with catastrophic failure detection
- **Background indexing** with non-blocking pipeline execution
- **13 comprehensive space API tests**

### Changed
- Migrated Event Store and KV store from Sled to RocksDB
- Refactored AI module from `ai_pipeline` to `ai`
- Improved space creation flow (indexing failures non-fatal)
- Enhanced runner initialization with clearer phase separation

### Fixed
- Race condition in Event Store append operations
- Incorrect embedding model usage (switched to FastEmbed)
- NetworkEventLoop shutdown hang
- PeerInfo serialization issues
- RocksDB lock conflicts in parallel tests

### Security
- Noise protocol implementation for encrypted P2P connections
- Capability-based network access foundation

### Deprecated
- Sled database completely removed in favor of RocksDB

---

## [0.1.0] - 2025-10-14

### Added

#### Identity & Authentication (WebAuthn/DID/SSI)
- **WebAuthn authentication system** with complete registration and authentication flows
- **Passkey management** with database persistence and secure credential storage
- **DID generation** from passkeys supporting did:key and did:peer methods
- **DID document creation** with JWK public keys
- **Challenge-based authentication** with replay protection
- **Multi-device support**

#### Event Store
- **Persistent append-only event store** with Sled backend
- **Hash chain integrity** with SHA-256
- **Causal ordering** with Dotted Version Vectors
- **JSON Schema validation** with versioning
- **Downstream processing** with persistent subscriptions
- **Idempotency guarantees** via atomic claim mechanism
- **108+ tests** with property-based testing

#### REST API
- Authentication endpoints (WebAuthn registration/authentication)
- Space management endpoints
- Health check endpoint

#### Database Layer
- SeaORM integration with SQLite backend
- Migration system for schema versioning
- Entity models for User, PassKey, Space

#### Initial Modules
- SSI module with WebAuthn state management
- Space module for local directory indexing
- AI pipeline module for document indexing

---

## Migration Notes

### From 0.2.x to 0.3.x
No breaking changes. New bootstrap configuration is optional.

### From 0.1.x to 0.2.x

#### Breaking Changes
- **Storage backend change**: Sled databases will not be automatically migrated
- **Import paths changed**: `ai_pipeline` module renamed to `ai`
- **KV Store API**: `sled::Db` replaced with `Arc<dyn KvStore>`

#### New Environment Variables
```bash
# Network Configuration
DHT_DB_PATH=/path/to/dht/db
PEER_REGISTRY_DB_PATH=/path/to/peer/registry
ENABLE_QUIC=true

# KV Store Configuration  
KV_STORE_PATH=/path/to/kv/store
KV_ENABLE_COMPRESSION=true

# AI Module (Failure Thresholds)
MAX_FAILURE_RATE=0.3
CATASTROPHIC_FAILURE_RATE=0.7
```

#### Database Migrations
```bash
cd back-end
sea-orm-cli migrate up
```
