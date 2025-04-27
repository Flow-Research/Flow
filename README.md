# Flow Architecture Overview

Flow is a decentralized coordination platform enabling agents and users to co-create, manage, and reward knowledge, tasks, and compute in a verifiable, local-first environment.


## Core Principles

1.  **Local-first + Verifiable:** Data and computation start locally, with cryptographic verification.
2.  **Agent Explainability (SLRPA):** Agents operate on a Sense → Learn → Reason → Predict → Act cycle.
3.  **Decentralized Execution:** Workflows run as Directed Acyclic Graphs (DAGs) with off-chain compute.
4.  **Knowledge Graphs with Provenance:** Data is stored in schema-aware graphs tracking origin and trust.
5.  **Capability-Based Access:** Permissions use UCANs (User Controlled Authorization Networks).
6.  **Programmable Incentives:** Rewards and reputation are customizable and trackable.


## Layered Architecture

1.  [**Storage Layer**](./specs/01_storage_layer.md) (Local-first CRDT, Any-Sync, Storacha/IPFS)
2.  [**Access & Auth Layer**](./specs/02_access_auth_layer.md) (DIDs + UCANs, trust graphs)
3.  [**Network Layer**](./specs/03_network_layer.md) (network discovery, P2P messaging, UCAN-auth, transport-agnostic)
4.  [**Coordination & Sync Layer**](./specs/04_coordination_sync_layer.md) (CRDT + DAG state, multi-agent sync)
5.  [**Knowledge Graph & MCP Layer**](./specs/05_knowledge_graph_mcp.md) (Schema-bound DAG nodes, Model Context Protocol)
6.  [**User Interface / UX Layer**](./specs/06_ui_ux_layer.md) (User interface for interacting with the Knowledge graph)
7.  [**Agent Layer**](./specs/07_agent_layer.md) (SLRPA lifecycle, explainable)
8.  [**Execution Layer**](./specs/08_execution_layer.md) (DAG workflows, signed transitions)
9.  [**Compute Layer**](./specs/09_compute_layer.md) (Runners, Bacalhau, zkML/zkPoE)
10. [**Incentive Layer**](./specs/10_incentive_layer.md) (Programmable rewards, contribution tracking)


## Flow of Activity

1.  Create/update objects (CRDT deltas, sync DAG).
2.  Sign with DIDs, governed by UCANs.
3.  Agents execute via SLRPA.
4.  Results logged, optionally verified (ZK).
5.  Rewards triggered by provenance/policy.
6.  Explore via graph UI.


## Security/Verifiability

Signed/versioned objects, no central state, UCAN-based permissions.


## Example Use Cases

Open science research coordination, federated data marketplaces
