# Flow Execution Layer (3.2)

## Role & Goals

Orchestrates and executes workflows defined as Directed Acyclic Graphs (DAGs). Aims for:

*   **Verifiable Execution:** Cryptographic proof of computation and data flow.
*   **Resilient Execution:** Handles failures gracefully.
*   **Deterministic Execution:** Reproducible results given the same inputs and context.
*   **Composable Workflows:** DAGs can be nested and reused.
*   **Agent-Centric Control:** Execution is driven by agent intent and capabilities.

## Architecture

Core components:

*   **DAG Engine:** Interprets DAG definitions and manages execution flow.
*   **Task Dispatcher:** Selects appropriate compute resources (via Compute Layer) and sends tasks for execution.
*   **State Manager:** Tracks the status of DAGs and individual tasks using CRDTs (via Coordination/Storage Layers).
*   **Event Bus:** Facilitates internal communication between Execution Layer components.
*   **Audit Logger:** Securely records detailed execution traces for provenance and verification.

Integrates with:

*   Agent Layer (receives execution requests)
*   Knowledge Graph/MCP Layer (accesses task definitions, context, models)
*   Compute Layer (dispatches tasks to runners)
*   Storage Layer (stores/retrieves DAGs, task results, state)
*   Network Layer (communicates status, potentially coordinates distributed execution)
*   Coordination Layer (manages distributed state via CRDTs)

## DAG Structure

*   Workflows are defined as content-addressed (CID) Directed Acyclic Graphs (DAGs).
*   **Nodes:** Represent Tasks.
    *   Task Types: Compute, Data Manipulation, Coordination Logic, Control Flow.
    *   Tasks typically reference MCP manifests detailing their requirements (inputs, model, constraints).
*   **Edges:** Define dependencies between tasks (data flow or control flow).

## Task Lifecycle

Managed by the State Manager, typical states include:

*   Pending
*   Ready (dependencies met)
*   Running
*   Retrying (after transient failure)
*   Completed (successful)
*   Failed (non-recoverable error)
*   Cancelled

State transitions are signed events, logged, and stored using CRDTs for consistency.

## State Management

*   Utilizes CRDTs (via the Coordination & Storage layers) to maintain a consistent, mergeable state for:
    *   Overall DAG execution progress.
    *   Individual task statuses.
*   Enables robust state tracking across potentially distributed agents and nodes.

## Verifiability & Auditability

*   Execution is deeply linked to provenance tracking.
*   Each task execution generates a **signed trace** containing:
    *   Link to the DAG node definition.
    *   Reference to the MCP manifest used.
    *   CIDs of inputs and outputs.
    *   Details of the runner/executor (DID, environment).
    *   Verification proofs (e.g., ZK proof, TEE attestation) if required.
*   Audit logs provide a secure, immutable, and verifiable history of all executions.

## Resilience & Fault Tolerance

Mechanisms include:

*   **Checkpointing:** Periodically saving DAG execution state.
*   **Automatic Retries:** Configurable retries for tasks that fail due to transient issues.
*   **Idempotent Task Design:** Encouraging tasks that can be run multiple times with the same result.
*   **Failover Logic:** Potential for redundant components or state recovery mechanisms.

## Scheduling Interaction

*   The **Task Dispatcher** interacts with **Schedulers** in the Compute Layer.
*   It provides task requirements (from the DAG node and referenced MCP manifest) and constraints (security, budget, location).
*   Schedulers use this information to find and allocate appropriate Runners from the Compute Layer.

## Security

*   DAG operations (creation, execution, modification, inspection) are gated by **UCANs**.
*   Task dispatch ensures secure context and parameter passing according to UCAN capabilities.
*   Provenance graph maintained in the KG serves security audit purposes.

## APIs & SDKs

Provides interfaces (e.g., TS, Python, Rust SDKs, CLI: `flow dag`) for:

*   DAG definition (e.g., YAML, JSON, programmatic builders).
*   Submitting DAGs for execution.
*   Monitoring execution progress and status.
*   Inspecting DAG structure and task details.
*   Retrieving results and execution traces/proofs.
