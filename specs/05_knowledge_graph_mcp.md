# Flow Knowledge Graph (KG) & Model Context Protocol (MCP) Layer (3.3)

## Role & Goals

Provides the semantic memory and context foundation for the Flow system, and defines how models and tools are integrated and invoked verifiably.

*   **KG:** Manages entities, relationships, context, causality, and provenance as a distributed graph.
*   **MCP:** Standardizes the description, requirements, invocation, and verification of external models, tools, and functions.

## Foundational Role

*   **KG:** Provides semantic context and long-term memory for agents.
*   **MCP:** Handles the provenance, invocation, and verification of models and tools used by agents and DAG tasks.

## KG Architecture

*   **Data Structure:** IPLD-compatible object graph.
*   **Backend:** CRDT-backed persistence (e.g., using Any-Sync) for decentralized consistency.
*   **Nodes:** Represent entities (users, agents, tasks, data, concepts), events, etc. Identified by CIDs or DIDs.
*   **Edges:** Represent semantic or causal links between nodes.
*   **Layered Model:** Conceptually layered for different types of information:
    *   Entity Layer
    *   Context Layer
    *   Semantic Layer (Schemas, Ontologies)
    *   Causal Layer (Execution Traces)
    *   Provenance Layer (Origin, Signatures, Proofs)
*   **Schemas:** Supports JSON-LD, RDFS/OWL-lite for defining object types and relationships.
*   **Access Control:** UCAN-based permissions govern read/write access to graph partitions.

## Contextualization & Binding

*   Links KG nodes to:
    *   Agent SLRPA phases (providing context for Sense, Reason, Learn).
    *   DAG task inputs/outputs.
    *   Agent-to-Agent messages.
*   Supports scoping context by:
    *   Time
    *   Logical relevance
    *   Privacy constraints (UCANs)
*   Enables runtime resolution of context needed by tasks/models.
*   Tracks provenance of context used.

## MCP Integration

*   **Manifests:** Define models/tools.
    *   Signed, content-addressed (CID).
    *   Stored within the KG.
    *   Specify:
        *   Interface (inputs, outputs, function signature).
        *   Context needs (required KG data, schemas).
        *   Constraints (UCAN capabilities needed, resource limits, privacy rules).
        *   Proof requirements (e.g., zkDL proof, TEE attestation needed).
*   **Enables:** Verifiable, composable, and explainable use of external functions/models within Flow DAGs and agent reasoning.

## Reasoning & Constraint Solving

*   Integrates symbolic (rules, policies) and statistical (ML models via MCP) reasoning.
*   May utilize an **MCP-Solver** for constraint satisfaction (CSP, SMT) over graph subgraphs defined by context.
*   Reasoning chains link agent SLRPA phases, solver execution, and DAG task execution.
*   Produces verifiable reasoning traces stored in the KG.

## Federated Composition

*   Agents can merge, fork, and snapshot graph partitions using CRDT mechanisms.
*   UCANs control sharing permissions and redaction rules during federation.
*   Supports multi-agent planning and shared contextual understanding.

## Querying

*   Supports query languages like:
    *   GraphQL
    *   SPARQL-lite (subset)
    *   JSONPath / DAGPath
*   Queries operate over local, federated, or historical graph snapshots.
*   Queries are UCAN-gated for access control.
*   Supports privacy-preserving query mechanisms.
*   Allows live subscriptions to graph changes.

## Explainability & Audit

*   Causal chains within the KG link observations, reasoning steps, predictions, and actions.
*   Temporal queries allow reconstruction of agent behavior and decision-making.
*   Verifiable execution traces (signed MCP invocations, proofs) provide auditability.
*   Supports generation of human-readable views of KG data and provenance.

## Compliance & Redaction

*   Policy-driven redaction (omission, masking) based on compliance tags (GDPR, HIPAA, etc.).
*   Supports reasoning over masked or redacted data where appropriate.
*   Enforces data retention policies defined in the graph or via UCANs.
