# Flow Architecture Overview

Flow is a decentralized coordination platform enabling agents and users to co-create, manage, and reward knowledge, tasks, and compute in a verifiable, local-first environment.


## Core Principles

1.  **Ubiquitous, Verifiable Computing:** Compute can run anywhere, but every claim about that compute is verifiable.
2.  **Capability-Based Access:** Authentication and authorization is decentralized. Permissions are based on Verifiable Credentials and Capabilities.
3.  **Knowledge Graphs with Provenance:** Data is stored in schema-aware graphs tracking origin and trust.
4.  **Peer-to-Peer networking:** Agents and users connect and coordinate via a decentralized network.
5.  **Decentralized Execution:** Execution is local-first, workflows can be distributed and executed in a decentralized manner.
6.  **Agent Explainability (SLRPA):** Agents operate on a Sense → Learn → Reason → Predict → Act cycle.
7.  **Programmable Incentives:** Rewards and reputation are customizable and trackable.


## Layered Architecture

1.  [**Storage Layer**](./01_storage_layer.md): Implements persistent storage over content-addressed systems (like IPFS).
2.  [**Access & Auth Layer**](./02_access_auth_layer.md): Manages identity (DIDs) and permissions using capability-based systems.
3.  [**Network Layer**](./03_network_layer.md): Handles peer-to-peer discovery, communication, data synchronization, and transport, secured by the Auth Layer.
4.  [**Coordination & Sync Layer**](./04_coordination_sync_layer.md): Ensures state consistency across different agents and nodes using the underlying sync mechanisms.
5.  [**Knowledge Graph Layer**](./05_knowledge_graph.md): Provides the semantic context (KG) for data. 
6. [**MCP Layer**](./06_mcp.md): Defines how external models/tools (via Model Context Protocol) interact with user content.
7.  [**User Interface / UX Layer**](./07_ui_ux_layer.md): Provides the means for users to interact with the system, inspect the graph, manage agents, delegate tasks, etc.
8.  [**Agent Layer**](./08_agent_layer.md): Manages agent lifecycles (Sense→Learn→Reason→Predict→Act) and ensures their actions are explainable.
9.  [**Execution Layer**](./09_execution_layer.md): Handles the definition and running of workflows as Directed Acyclic Graphs (DAGs), managing signed state transitions.
10.  [**Compute Layer**](./10_compute_layer.md): Executes the actual computational tasks defined in the Execution Layer, potentially using various backends (local, distributed like Bacalhau) and supporting verifiable computation.
11. [**Incentive Layer**](./11_incentive_layer.md): Defines and manages programmable rewards and contribution tracking based on provenance data in the knowledge graph.


## Flow of Activity

1.  Create/update objects.
2.  Sign with DIDs, governed by capability tokens.
3.  Discover and connect to the Flow network.
4.  Grant access control and permissions to objects.
5.  Users can spin up Agents connected to their knowledge base. 
6.  Agents execute via SLRPA (Sense → Learn → Reason → Predict → Act).
7.  Results logged, verified.
8.  Rewards triggered by provenance/policy.
9.  Explore the knowledge base via an intuitive UI.

