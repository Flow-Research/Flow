# Flow Storage Layer

## Design Goals

*   **Local-first, edge-native, cloud-optional:** Data resides on the user's/agent's local device, and in trusted environments.
*   **Sovereign:** Users/agents control their data.
*   **Private:** Encryption and access control are fundamental.
*   **Verifiable:** Data integrity is ensured via content addressing and signatures.
*   **Interoperable:** Uses standard formats like IPLD.
*   **Agent-centric:** Optimized for agent state and operational data.
*   **Resilient:** Data persists despite network issues or failures.

## Architecture

Layered approach to storage management:

1.  **Ephemeral Cache:** In-memory storage for fast access (optional).
2.  **Persistent Local CRDT Store:** The primary storage engine.
    *   Likely based on **Any-Sync** or a similar CRDT framework.
    *   Uses a local key-value store (RocksDB) as the underlying persistence.
3.  **Hot Sync Storage (Optional):** Relays or dedicated nodes (e.g., Storacha) for faster sync and availability among online peers.
4.  **Cold Archive Storage (Optional):** Long-term, high-latency storage using content-addressed networks like IPFS/Filecoin for backup and large data blobs.

Node Roles:

*   **Local Node:** Standard user/agent instance.
*   **Sync Node:** Facilitates CRDT delta exchange.
*   **File Node:** Interfaces with Cold Archive Storage.
*   **Coordinator Node:** May manage cross-node operations (optional).
*   **Verifier Node:** Checks proofs or data integrity (optional).

## Data Types Managed

*   Knowledge Graph Nodes & Edges
*   CRDT Delta Logs / Operation Logs
*   DAG Definitions & Execution Artifacts (inputs, outputs, traces)
*   MCP Manifests
*   ML Models or other large assets referenced by MCP
*   Verification Proofs (ZK proofs, TEE attestations)
*   State Snapshots
*   User Documents / Files
*   Capability Based Access Control (capabilities)

Data Representation:

*   Primarily **IPLD** (DAG-JSON, DAG-CBOR).
*   Uses **CAR (Content Addressable aRchive)** files for bundling related IPLD blocks.

## CRDTs & Synchronization

*   Leverages CRDTs for conflict-free state management.
*   Core CRDT types:
    *   LWW-Register/Map (Last-Writer-Wins)
    *   OR-Set (Observed-Remove Set)
    *   RGA (Replicated Growable Array) or similar sequence types.
    *   Custom CRDTs for specific needs (e.g., causal DAGs).
*   **Delta-based Sync:** Only changes (signed deltas) are typically synced.
*   **Causal Consistency:** Sync respects the causal history of changes.
*   **Mergeable State:** CRDTs ensure deterministic merging of concurrent changes.
*   **Scoped Sync:** Using Capability Based Access Control controls which parts of the state graph can be synced with which peers.

## Security & Encryption

*   **Encryption-at-Rest:** Local database files are encrypted.
*   **Field-Level Encryption:** Sensitive data within objects can be selectively encrypted.
*   Uses standard cryptographic primitives (e.g., Sealed Box, Envelope Encryption).
*   **Signatures:** All objects, deltas, and operations are typically signed by the originating DID.
*   **Access Control:** Capability gate all storage operations (see Access & Auth Layer).

## Access Control

*   Relies heavily on the **Capability based access Control** framework (see Access & Auth Layer).
*   Defines fine-grained capabilities:
    *   `read`, `write`, `delete`
    *   `sync` (permission to exchange deltas for an object/scope)
    *   `share` (permission to delegate capabilities to others)
    *   `verify` (permission to access data needed for verification)
    *   `execute` (related to compute, but may involve storage access)
    *   `redact`
    *   `pin` (requesting persistent storage)
*   Enforcement occurs at the storage API level, checking VC validity and scope.

## APIs & Interfaces

Provides access via:

*   **SDKs:** Libraries for TypeScript, Python, Rust, Go.
*   **CLI:** Command-line tool (`flow-store` or similar).
*   **Endpoints:** Potentially local HTTP/gRPC endpoints for inter-process communication.

Supports operations like:

*   Get/Put/Delete objects (based on CID/Key)
*   Querying/Filtering (leveraging KG layer capabilities)
*   Initiating Sync with peers
*   Sharing data
*   Managing encryption keys
*   Triggering pinning/archival

## Retention & Garbage Collection (GC)

*   **Policy-Driven:** Rules define data lifecycle (TTL, expiry, compliance needs like GDPR).
*   Policies can be stored in the KG or attached via capability tokens.
*   **GC Modes:**
    *   Soft Delete (mark as deleted, retain for recovery)
    *   Hard Delete (permanently remove)
    *   Redact (remove specific fields)
    *   Proof-Preserving Trim (remove data but keep hashes/proofs)
    *   Snapshot-Based GC (keep only data reachable from recent snapshots).
*   **Compliance-Aware:** GC respects legal/regulatory requirements.
*   **Dependency Resolution:** Avoids deleting data still referenced by other objects or required for provenance.

## Redundancy & Recovery

*   **Multi-Tier Redundancy:**
    *   Local CRDT logs and periodic snapshots.
    *   Replication via peer sync (Hot Sync).
    *   Optional backup to Cold Archive (IPFS/Filecoin).
*   **Availability Models:** Configurable based on needs (local-only, multi-peer availability, archival).
*   **Recovery:** Restore state from:
    *   Local logs/snapshots.
    *   Syncing with peers.
    *   Retrieving from Cold Archive.
*   **Verifiable Restoration:** Ensures recovered data matches original CIDs/signatures.
