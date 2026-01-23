# Flow

<<<<<<< HEAD
**A peer-to-peer network for storing, sharing, and searching knowledge.**

Flow lets you store content locally with full ownership, share it selectively with others, and search across the entire network. Every piece of content is tracked with provenance, and contributions can be rewarded automatically.

## What Flow Does

| Capability | Description |
|------------|-------------|
| **Local-first storage** | Your data stays on your machine, content-addressed and cryptographically verified |
| **Selective sharing** | Share specific content with specific people or agents — you control access |
| **Semantic search** | Find content by meaning, not just keywords — works across local and network data |
| **Provenance tracking** | Know where content came from and how it's been used |
| **Programmable rewards** | Define rules for compensating valuable contributions *(coming soon)* |
=======
Flow is a decentralized automation network where user-defined goals run as verifiable jobs across a peer compute and knowledge marketplace.

Users keep their data in private environments they control. Agents execute jobs with signed events and receipts. The network keeps the shared state in sync. Programmable incentives settle contributions automatically based on recorded provenance, while access rules help protect privacy.
>>>>>>> 6402be9391ccb5e8f210c76852ed8dc814865c3d

## Concept Diagram

![Flow concept](https://drive.google.com/uc?export=view&id=11-YO0bh2xKSGA4Zoi85Yx8zoM8vZI2Jy "Flow")

<<<<<<< HEAD
> **Note:** This diagram links to Google Drive. If it doesn't load, the image may need to be moved to the repository.
=======
At the core, each user or organisation controls a portable knowledge base, anchored to their identity. Apps, tools, and agents are built on top of this base to provide experiences for users. Users can share specific parts of their data with other users and agents. Access, usage, and changes are checked and logged using cryptographic signatures and clear policies.

Flow is not a single app or API, but a network of protocols, services, and agents that help people automate work, share knowledge, and earn from their contributions.
>>>>>>> 6402be9391ccb5e8f210c76852ed8dc814865c3d

**The core idea:** Each user controls their own Knowledge Base. They can share parts of it with others on the network. The network enforces access rules and tracks contributions.

## Core Principles

<<<<<<< HEAD
| Principle | Description |
|-----------|-------------|
| **Local-first** | Your data lives on your machine; cloud is optional |
| **Content-addressed** | Data is identified by its hash (CID), not location |
| **Verifiable** | Every claim can be cryptographically verified |
| **Capability-based** | Access controlled by tokens you grant, not central authority |
| **Peer-to-peer** | Nodes connect directly; no central server required |
| **Provenance-tracked** | Origin and history of all content is preserved |
=======
1.  **Ubiquitous, Verifiable Computing:** Compute can run anywhere, but every claim about that compute is verifiable.
2.  **Capability-Based Access:** Authentication and authorization are decentralized. Permissions are based on Verifiable Credentials and Capabilities.
3.  **Knowledge graph and vector indexes with provenance** Flow maintains a shared knowledge graph and vector index across data stored in many systems, and ties every fact and embedding back to its source and history.
4.  **Peer-to-Peer networking:** Agents and users connect and coordinate via a decentralized network.
5.  **Decentralized Execution:** Execution is local-first, edge-native, cloud-optional. Workflows can be distributed and executed in a decentralized manner.
6.  **Agent Explainability (SLRPA):** Agents operate on a Sense → Learn → Reason → Predict → Act cycle.
7.  **Programmable Incentives:** Rewards and reputation are customizable and trackable.
>>>>>>> 6402be9391ccb5e8f210c76852ed8dc814865c3d


## Architecture

Flow is organized into four architectural groups:

### Foundation Layer
Core infrastructure for data and connectivity.

| Component | Purpose | Spec |
|-----------|---------|------|
| **Storage** | Content-addressed storage (IPLD/CIDs), RocksDB block store | [spec](./specs/01_storage_layer.md) |
| **Identity & Auth** | DIDs, verifiable credentials, capability tokens | [spec](./specs/02_access_auth_layer.md) |
| **Network** | libp2p, peer discovery, GossipSub messaging | [spec](./specs/03_network_layer.md) |

### Intelligence Layer
Understanding and context for content.

| Component | Purpose | Spec |
|-----------|---------|------|
| **Knowledge Graph** | Semantic indexing, embeddings, vector search | [spec](./specs/05_knowledge_graph.md) |
| **Agents** | Autonomous actors (Sense→Learn→Reason→Predict→Act) | [spec](./specs/08_agent_layer.md) |
| **MCP** | Model Context Protocol for AI/tool integration | [spec](./specs/06_mcp.md) |

### Execution Layer
Coordination and computation.

| Component | Purpose | Spec |
|-----------|---------|------|
| **Coordination** | State sync across nodes, conflict resolution | [spec](./specs/04_coordination_sync_layer.md) |
| **Workflows** | DAG-based task execution with signed transitions | [spec](./specs/09_execution_layer.md) |
| **Compute** | Distributed execution (local, Bacalhau, etc.) | [spec](./specs/10_compute_layer.md) |

### Experience Layer
User-facing capabilities.

| Component | Purpose | Spec |
|-----------|---------|------|
| **User Interface** | Web/mobile/desktop apps for content management | [spec](./specs/07_ui_ux_layer.md) |
| **Incentives** | Programmable rewards and reputation tracking | [spec](./specs/11_incentive_layer.md) |


## How It Works Today

<<<<<<< HEAD
```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│  Add Files  │ ──► │   Index &   │ ──► │   Search    │
│  to Space   │     │   Embed     │     │  Locally    │
└─────────────┘     └─────────────┘     └─────────────┘
                                               │
                                               ▼
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│   Publish   │ ◄── │  Discover   │ ◄── │   Connect   │
│  to Network │     │    Peers    │     │  to Network │
└─────────────┘     └─────────────┘     └─────────────┘
                                               │
                                               ▼
                                        ┌─────────────┐
                                        │  Federated  │
                                        │   Search    │
                                        └─────────────┘
```

1. **Add content** to a local space (files are chunked and hashed)
2. **Automatic indexing** generates embeddings for semantic search
3. **Search locally** using natural language queries
4. **Connect to network** and discover peers via mDNS
5. **Publish content** to make it available to other nodes
6. **Federated search** queries both local and network content

### Vision (Coming Soon)

- 🔮 Autonomous agents that act on your behalf
- 🔮 DAG-based workflows with verifiable execution
- 🔮 Programmable rewards for contributions
- 🔮 Decentralized compute marketplace
=======
1.  Create and update your objects.
2.  Everything is signed and secure using your digital ID.
3.  Connect to the network and find what you need.
4.  Start Agents that plug into your own knowledge and data.
5.  Agents sense, learn, think, predict, and act on your behalf.
6.  Their actions and results are recorded and checked.
7.  Rewards are given based on clear rules and proven contributions.
8.  Explore and manage all of this through a simple, visual interface.
>>>>>>> 6402be9391ccb5e8f210c76852ed8dc814865c3d


## Implementation Status

> **Note:** Flow is under active development. The table below shows current implementation status.

| Layer | Status | Description | Code Location |
|-------|--------|-------------|---------------|
| **Storage** | ✅ Complete | Content-addressed storage with IPLD/CIDs, RocksDB block store, SQLite metadata | `back-end/node/src/modules/storage/` |
| **Access & Auth** | 🚧 Partial | WebAuthn registration/login implemented; DIDs and VCs planned | `back-end/node/src/modules/identity/` |
| **Network** | ✅ Complete | libp2p with GossipSub messaging, mDNS discovery, peer registry | `back-end/node/src/modules/network/` |
| **Coordination & Sync** | 🚧 Partial | Basic pub/sub sync; CRDT-based convergence planned | `back-end/node/src/modules/network/gossipsub/` |
| **Knowledge Graph** | ✅ Complete | Semantic indexing with FastEmbed, Qdrant vector storage | `back-end/node/src/modules/ai/` |
| **MCP** | 📋 Planned | Model Context Protocol integration not yet started | — |
| **User Interface** | ✅ Complete | React/Vite web app with spaces, search, content management | `user-interface/flow-web/` |
| **Agent** | 📋 Planned | SLRPA agent framework not yet implemented | — |
| **Execution** | 📋 Planned | DAG workflow engine not yet implemented | — |
| **Compute** | 📋 Planned | Bacalhau integration not yet started | — |
| **Incentive** | 📋 Planned | Reward and reputation system not yet implemented | — |

### What You Can Do Today

- ✅ **Store content locally** — Files are chunked, hashed (CID), and stored in RocksDB
- ✅ **Semantic search** — Content is indexed with embeddings for natural language search
- ✅ **Create spaces** — Organize content into named collections
- ✅ **Publish to network** — Share content with other Flow nodes
- ✅ **Federated search** — Search across local and network content
- ✅ **Peer discovery** — Auto-discover other nodes on your local network (mDNS)
- ✅ **Web interface** — Browse, search, and manage content through the UI

### Coming Soon

- 🚧 Decentralized identity (DIDs) and verifiable credentials
- 📋 Autonomous agents with explainable actions
- 📋 DAG-based workflow execution
- 📋 Distributed compute marketplace
- 📋 Programmable incentives and rewards

For detailed roadmap, see [specs/ROADMAP.md](./specs/ROADMAP.md).


---

## Getting Started

### Prerequisites

Before you begin, ensure you have the following installed:

| Tool | Version | Purpose |
|------|---------|---------|
| **Rust** | 1.75+ | Backend development |
| **Cargo** | Latest | Rust package manager |
| **Node.js** | 18+ | Frontend development |
| **npm/yarn/pnpm** | Latest | Node package manager |
| **Docker** | 20+ | Running Qdrant and Redis |
| **nx** | Latest | Monorepo task runner |

Install nx globally:
```bash
npm install -g nx
```

### Quick Start

1. **Clone the repository:**
   ```bash
   git clone https://github.com/your-org/flow.git
   cd flow
   ```

2. **Copy environment file:**
   ```bash
   cp .env.example .env
   ```
   > Note: `.env` is gitignored - never commit files containing secrets

3. **Start Docker services (Qdrant + Redis):**
   ```bash
   nx docker-up back-end
   ```

4. **Install frontend dependencies:**
   ```bash
   nx install-all user-interface
   ```

5. **Build the backend:**
   ```bash
   nx build back-end
   ```

6. **Run the node:**
   ```bash
   nx run-node back-end
   ```

7. **Run the web UI (in another terminal):**
   ```bash
   nx dev-web user-interface
   ```

8. **Open your browser:**
   - Web UI: http://localhost:5173
   - REST API: http://localhost:8080


---

## Environment Variables

Copy `.env.example` to `.env` and configure as needed.

> **Security Warning**
>
> - **Never commit `.env` files** containing secrets to version control
> - The `.env` file is already in `.gitignore` - keep it that way
> - Only `.env.example` (with placeholder values) should be committed
> - If you accidentally commit secrets, rotate them immediately and use `git filter-branch` or BFG to remove from history

### Quick Start Configuration

For most users, these are the only variables you need to set:

```bash
# Required - Database
DATABASE_URL="sqlite:///db/data.sqlite?mode=rwc"

# Required - Vector Database (must have Qdrant running)
QDRANT_URL="http://localhost:6334"
QDRANT_SKIP_API_KEY=true  # For local development

# Optional - Server ports (defaults shown)
REST_PORT=8080
WEBSOCKET_PORT=8081
```

Everything else has sensible defaults. See the full reference below for advanced configuration.

---

<details>
<summary><strong>Full Configuration Reference</strong> (click to expand)</summary>

### Database Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `DATABASE_URL` | `sqlite:///db/data.sqlite?mode=rwc` | SQLite database connection string |
| `DB_MAX_CONNECTIONS` | `100` | Maximum database connections |
| `DB_MIN_CONNECTIONS` | `5` | Minimum database connections |
| `DB_CONNECT_TIMEOUT` | `8` | Connection timeout in seconds |
| `DB_IDLE_TIMEOUT` | `600` | Idle connection timeout in seconds |
| `DB_MAX_LIFETIME` | `1800` | Maximum connection lifetime in seconds |
| `DB_LOGGING_ENABLED` | `false` | Enable SQL query logging |

### Storage Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `KV_STORE_PATH` | `$HOME/.config/flow/kv` | Path to RocksDB key-value store |

### Server Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `HOST` | `0.0.0.0` | Server bind address |
| `REST_PORT` | `8080` | REST API port |
| `WEBSOCKET_PORT` | `8081` | WebSocket port |
| `CORS_ORIGINS` | `http://localhost:3000,http://localhost:5173` | Allowed CORS origins (comma-separated) |

### Vector Database (Qdrant)

| Variable | Default | Description |
|----------|---------|-------------|
| `QDRANT_URL` | `http://localhost:6334` | Qdrant gRPC endpoint |
| `QDRANT_SKIP_API_KEY` | `false` | Skip API key auth (for local/test instances) |

### Cache (Redis)

| Variable | Default | Description |
|----------|---------|-------------|
| `REDIS_URL` | *(optional)* | Redis connection URL for caching indexed content |

### AI Indexing Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `VECTOR_SIZE` | `384` | Embedding vector dimensions (FastEmbed default) |
| `MIN_CHUNK_SIZE` | `10` | Minimum chunk size in characters |
| `MAX_CHUNK_SIZE` | `20000` | Maximum chunk size in characters |
| `MAX_FILE_SIZE` | `10000000` | Maximum file size to index (10MB) |
| `EMBED_BATCH_SIZE` | `20` | Batch size for embedding generation |
| `STORAGE_BATCH_SIZE` | `50` | Batch size for Qdrant storage |
| `CONCURRENCY` | `4` | Pipeline concurrency level |
| `RATE_LIMIT_MS` | `100` | Rate limiting between operations |
| `ALLOWED_EXTENSIONS` | `md,rs,txt,...` | File extensions to index (comma-separated) |
| `EXCLUDE_PATTERNS` | *(empty)* | Patterns to exclude from indexing |
| `MAX_FAILURE_RATE` | `0.5` | Maximum failure rate before stopping |
| `MIN_SAMPLE_SIZE` | `10` | Minimum samples for failure rate calculation |

### AI Metadata Generation (Optional)

These features require [Ollama](https://ollama.ai/) with the `llama3.2:3b` model installed.

| Variable | Default | Description |
|----------|---------|-------------|
| `ENABLE_METADATA_QA` | `false` | Generate Q&A pairs for chunks |
| `ENABLE_METADATA_SUMMARY` | `false` | Generate summaries for chunks |
| `ENABLE_METADATA_KEYWORDS` | `false` | Extract keywords from chunks |

### Distributed Search

| Variable | Default | Description |
|----------|---------|-------------|
| `FLOW_SEARCH_RESPOND_TO_QUERIES` | `true` | Respond to network search queries |
| `FLOW_SEARCH_MAX_RESULTS_PER_QUERY` | `10` | Max results per search query |
| `FLOW_SEARCH_PEER_COUNT` | `5` | Number of peers to query |
| `FLOW_SEARCH_NETWORK_TIMEOUT_MS` | `5000` | Network query timeout |
| `FLOW_SEARCH_RETRY_ENABLED` | `true` | Enable query retries |
| `FLOW_SEARCH_CACHE_TTL_SECS` | `300` | Search cache TTL |
| `FLOW_SEARCH_CACHE_MAX_ENTRIES` | `1000` | Maximum cache entries |

### P2P Network Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `GOSSIPSUB_ENABLED` | `true` | Enable GossipSub protocol |
| `GOSSIPSUB_HEARTBEAT_INTERVAL_MS` | `1000` | Heartbeat interval |
| `GOSSIPSUB_MESH_N` | `6` | Target mesh size |
| `GOSSIPSUB_MESH_N_LOW` | `4` | Minimum mesh size |
| `GOSSIPSUB_MESH_N_HIGH` | `12` | Maximum mesh size |
| `GOSSIPSUB_MAX_MESSAGE_SIZE` | `65536` | Maximum message size |
| `GOSSIPSUB_VALIDATE_SIGNATURES` | `true` | Validate message signatures |
| `GOSSIP_MESSAGE_DB_PATH` | `$HOME/.config/flow/gossip` | Message store path |
| `MESSAGE_STORE_ENABLED` | `true` | Enable message persistence |
| `MESSAGE_STORE_MAX_PER_TOPIC` | `10000` | Max messages per topic |
| `MESSAGE_STORE_MAX_TOTAL` | `100000` | Max total messages |
| `MESSAGE_STORE_CLEANUP_INTERVAL` | `60` | Cleanup interval in seconds |
| `NETWORK_MDNS_QUERY_INTERVAL` | `5` | mDNS discovery interval |

### Peer Registry

| Variable | Default | Description |
|----------|---------|-------------|
| `PEER_REGISTRY_DB_PATH` | `$HOME/.config/flow/peers` | Peer registry database path |
| `PEER_REGISTRY_FLUSH_INTERVAL` | `30` | Flush interval in seconds |
| `PEER_REGISTRY_MAX_FAILURES` | `5` | Max failures before peer removal |
| `PEER_REGISTRY_TTL_SECS` | `86400` | Peer TTL (24 hours) |

### WebAuthn Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `WEBAUTHN_RP_ID` | `localhost` | Relying Party ID |
| `WEBAUTHN_RP_ORIGIN` | `http://localhost:3000` | Relying Party origin |
| `WEBAUTHN_RP_NAME` | `Flow WebAuthn` | Relying Party display name |

### Logging

| Variable | Default | Description |
|----------|---------|-------------|
| `RUST_LOG` | `info` | Log level (`error`, `warn`, `info`, `debug`, `trace`) |
| `LOG_LEVEL` | `info` | Alternative log level variable |

You can also set per-module log levels:
```bash
RUST_LOG=node::modules::network=debug,node::modules::ai=trace
```

</details>


---

## Available Commands

The project uses [nx](https://nx.dev/) to manage the workspace.

### Workspace Commands (`flow`)

| Command | Description |
|---------|-------------|
| `nx start-all flow` | Start all services (backend + docker + frontend) |
| `nx stop-all flow` | Stop all services |
| `nx docker-up flow` | Start Qdrant + Redis containers |
| `nx docker-down flow` | Stop Qdrant + Redis containers |

### Back-End Commands (`back-end`)

| Command | Description |
|---------|-------------|
| `nx run-node back-end` | Run the Flow node |
| `nx stop back-end` | Stop the node |
| `nx build back-end` | Build for production |
| `nx test back-end` | Run tests |
| `nx docker-up back-end` | Start Docker containers |
| `nx docker-down back-end` | Stop Docker containers |
| `nx docker-logs back-end` | Tail container logs |

### User Interface Commands (`user-interface`)

| Command | Description |
|---------|-------------|
| `nx install-all user-interface` | Install all UI dependencies |
| `nx dev-web user-interface` | Run web app in dev mode |
| `nx dev-mobile user-interface` | Run mobile app in dev mode |
| `nx dev-desktop user-interface` | Run desktop app in dev mode |
| `nx build-all user-interface` | Build all UI applications |
| `nx stop user-interface` | Stop all frontend processes |


---

## Docker Services

The backend requires two Docker services:

### Qdrant (Vector Database)
- **Image:** `qdrant/qdrant:latest`
- **Ports:** 6333 (HTTP), 6334 (gRPC)
- **Purpose:** Stores embeddings for semantic search

### Redis (Cache)
- **Image:** `redis:7-alpine`
- **Port:** 6379
- **Purpose:** Caches indexed content to avoid re-processing

Start both with:
```bash
nx docker-up back-end
```

Or manually:
```bash
cd back-end
docker-compose up -d
```


---

## Running Tests

### Backend Tests
```bash
# Run all tests
nx test back-end

# Run specific test
cd back-end/node
cargo test test_name

# Run integration tests (requires Docker)
cargo test indexing_e2e -- --ignored --test-threads=1
```

### Frontend Tests
```bash
cd user-interface/flow-web
npm test
```


---

## Troubleshooting

### Docker containers won't start
```bash
# Check if ports are in use
lsof -i :6333
lsof -i :6334
lsof -i :6379

# Remove old containers
docker rm -f qdrant redis
```

### Qdrant connection errors
1. Ensure Docker is running: `docker ps`
2. Check Qdrant logs: `docker logs qdrant`
3. For local testing without API key, set: `QDRANT_SKIP_API_KEY=true`

### Database errors
1. Ensure the database directory exists
2. Check permissions on the SQLite file
3. Try removing and recreating: `rm -rf db/` then restart

### Indexing not working
1. Check Qdrant is running: `curl http://localhost:6333/health`
2. Check allowed extensions include your file types
3. Review logs: `RUST_LOG=debug nx run-node back-end`

### Frontend can't connect to backend
1. Ensure backend is running on port 8080
2. Check CORS_ORIGINS includes your frontend URL
3. Try: `CORS_ORIGINS="http://localhost:5173" nx run-node back-end`


---

## Contributing
Go over the [Contributing](CONTRIBUTING.md) guide to learn how you can contribute.


## License
This project is licensed under the Apache License 2.0 - see the [LICENSE](LICENSE) file for details.


## Where to get help?
Join the Discord community and chat with the development team: [here](https://discord.gg/JmkvP6xKFW)
