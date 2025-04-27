# Flow Access & Auth Layer (3.6)

## Design Goals

*   **DID-Native:** Use Decentralized Identifiers as the foundation for all actors.
*   **Capability-Based:** Employ UCANs (User Controlled Authorization Networks) for granular permissions.
*   **Delegation-First:** Natively support secure delegation of capabilities.
*   **Verifiable Consent & Auditability:** Ensure actions are explicitly authorized and trackable.
*   **Interoperable:** Align with W3C standards (DIDs) and emerging patterns (UCANs).

## Identity Model (DIDs)

*   **Universal Identifiers:** Assign DIDs to all actors: users, agents, tools, compute runtimes, storage nodes, etc.
*   **Supported Methods:** Flexible support for various DID methods (e.g., `did:key`, `did:peer`, `did:web`, `did:ion`).
*   **DID Documents:** Standard mechanism for discovering public keys, service endpoints, and other metadata associated with a DID.
*   **Resolution:** Mechanisms to resolve DIDs to their corresponding DID documents.
*   **Linking:** Methods for linking DIDs across different methods or associating them with traditional identifiers if needed.

## Capability Authorization (UCANs)

*   **Core Mechanism:** UCANs are the primary way permissions are granted and verified.
*   **Structure:** Signed tokens (typically JWTs) containing:
    *   `iss`: Issuer DID (who granted the capability).
    *   `aud`: Audience DID (who received the capability).
    *   `att`: Attenuations - List of specific capabilities granted (e.g., `{ can: "crud/write", with: "kg://did:../object-cid" }`).
    *   `prf`: Proofs - Chain of parent UCANs justifying the grant.
    *   `exp`: Expiration time.
    *   `nbf`: Not-before time.
    *   `nnc`: Nonce (optional, for replay prevention).
    *   `fct`: Facts (optional, contextual information).
*   **Capabilities (`can`):** Define actions (e.g., `kg/read`, `compute/execute`, `storage/sync`, `ucan/delegate`).
*   **Resource (`with`):** Specifies the target resource (e.g., KG object CID, storage namespace, agent DID, task type).
*   **Chaining (`prf`):** Enables delegation by linking UCANs. A UCAN is valid only if its entire proof chain is valid and unexpired.
*   **Runtime Enforcement:** Integrated into all other layers (Storage, Compute, Network, KG) to check UCAN validity before allowing actions.

## Consent & Delegation

*   **Explicit Consent:** Mechanisms for users/agents to grant specific, often time-bound or single-use, permissions, potentially represented as specialized UCANs or signed consent tokens.
*   **Verifiable Delegation:** UCAN chaining provides a verifiable audit trail of how capabilities were delegated.
*   **Human-in-the-Loop:** UI/UX Layer facilitates clear consent prompts and management of delegations.
*   **Revocation:** Mechanisms for invalidating UCANs or entire delegation chains (e.g., via revocation lists, time expiry, explicit revocation messages).

## Trust Scoring & Reputation

*   **Contextual Trust:** Compute trust scores for DIDs based on factors like:
    *   Provenance of past actions (KG).
    *   Attestations from other trusted DIDs.
    *   Observed behavior (e.g., successful task completions vs failures).
    *   Reputation data from the Incentive Layer.
*   **Reputation Graphs:** Store trust relationships and scores within the Knowledge Graph.
*   **Trust-Aware Decisions:** Agents can use trust scores to inform decisions (e.g., selecting peers for sync, choosing compute providers, validating information).

## Key Management

*   **Key Roles:** Differentiate between keys used for signing (authentication, integrity) and encryption (confidentiality).
*   **Key Types:** Support for various key storage mechanisms:
    *   Hardware Security Modules (HSMs)
    *   Secure Enclaves
    *   Software Keystores (password protected)
    *   Session Keys (ephemeral)
*   **Management per Actor:** Secure key generation, storage, backup, and rotation strategies tailored for users, agents, and system components.
*   **Rotation/Revocation:** Procedures for updating keys associated with a DID and handling compromised keys.

## Agent-to-Agent Trust & Communication

*   Relies on the Network Layer for secure channel establishment (e.g., Noise protocol, TLS) using DID authentication.
*   A2A protocols (potentially DIDComm-like) for structured message exchange.
*   Mutual authentication using DIDs.
*   Session negotiation and establishment.
*   Exchange of intent and capabilities using UCANs within messages or sessions.
*   Trust evaluation based on peer DID, presented UCANs, and reputation scores.

## Fine-Grained Policies

*   Ability to define access control policies beyond basic UCAN grants:
    *   Object-level permissions within the KG.
    *   Schema-based access rules.
    *   Programmable policies (e.g., using JSON, DSL, or potentially WASM modules).
*   Policy evaluation engine integrates with UCAN verification.
*   Enforcement points distributed across layers.

## APIs & SDKs

Provides core modules/libraries via SDKs (TS, Python, Rust, Go) and potentially a CLI (`flow auth`):

*   `identity`: DID creation, resolution, management.
*   `ucan`: UCAN creation, validation, parsing, delegation.
*   `consent`: Managing user consent flows and tokens.
*   `access`: High-level functions for checking permissions (`canAct(actorDID, action, resourceDID)`).
*   `trust`: Querying/updating reputation and trust scores.
*   `audit`: Accessing logs related to authorization decisions and UCAN usage.
*   `keyring`: Secure key management interfaces.
*   `identityBridge`: Optional module for linking DIDs to existing identity systems.
