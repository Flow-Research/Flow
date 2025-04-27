# Flow Agent Layer (3.1)

## Role & Goals

Hosts autonomous agents, providing the primary interface for users/developers. Focuses on:

*   **Explainability (SLRPA):** Agents operate transparently.
*   **Autonomy:** Agents act independently based on goals and context.
*   **Coordination:** Agents collaborate effectively.
*   **Modularity:** Agent components are reusable.
*   **Security:** Agents operate within defined permissions.

## Architecture

*   Built around the **SLRPA (Sense, Learn, Reason, Predict, Act)** lifecycle.
*   Integrates deeply with:
    *   Knowledge Graph/MCP Layer (context, reasoning, model use)
    *   Execution Layer (running DAGs)
    *   Network Layer (Agent-to-Agent communication)
    *   Storage Layer (CRDT-backed agent state)
    *   Access & Auth Layer (Agent DIDs, UCAN enforcement)

## SLRPA Lifecycle

Defines the core agent loop:

1.  **Sense:** Observe the environment (KG updates, messages, external events).
2.  **Learn:** Update internal models, beliefs, and the Knowledge Graph based on observations.
3.  **Reason:** Plan actions and make decisions using KG context, MCP-defined tools/models, and integrated solvers.
4.  **Predict:** Evaluate potential outcomes of planned actions, assess model confidence.
5.  **Act:** Execute actions (trigger DAGs, send messages, update KG state, interact with external systems).

## Agent Types & Roles

Defines specialized agents that collaborate within the ecosystem, potentially including:

*   Planner Agents
*   Executor Agents
*   Sensor Agents
*   Monitor Agents
*   Coordinator Agents
*   User Proxy Agents

## State Management

*   Agent state (context, beliefs, plans, goals) is stored in its local, CRDT-backed slice of the Knowledge Graph.
*   Ensures state is versioned, verifiable, and syncable with other authorized agents/users.

## Agent Communication (A2A)

*   Uses secure, DID-authenticated protocols (built on the Network Layer).
*   Supports discovery, negotiation, task delegation, and knowledge sharing between agents.
*   UCANs strictly gate all interactions.

## Security & Identity

*   Each agent possesses a Decentralized Identifier (DID).
*   Manages cryptographic keys securely.
*   Operates based on granted UCAN capabilities.
*   Secure state storage and communication channels are enforced.

## APIs & SDKs

Provides tools (likely TS, Python, Rust SDKs, CLI: `flow agent`) for:

*   Agent creation and configuration
*   Deployment and management
*   Monitoring and debugging
*   User-agent interaction and delegation
