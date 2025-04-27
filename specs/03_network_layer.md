# Flow Network Layer (3.7)

## Design Goals

*   **P2P/Local-first Default:** Prioritize direct peer-to-peer or local communication.
*   **Modular Transport:** Support multiple underlying network protocols.
*   **Content-Addressable & Identity-Bound Msgs:** Messages linked to data CIDs and sender DIDs.
*   **Decentralized Discovery:** Find peers without central directories.
*   **Secure Channels:** Ensure message confidentiality, integrity, and authenticity.
*   **Low-Latency & Resilient:** Optimize for speed and handle network interruptions.
*   **Privacy-Preserving:** Minimize metadata leakage.
*   **Sync Integration:** Support efficient CRDT synchronization.
*   **Extensible Overlays:** Allow higher-level protocols and topologies.
*   **Verifiable Communication:** Bind messages to UCANs and provenance.

## Topology & Peer Roles

*   **Flexible Topology:** Supports various network structures:
    *   Mesh Networks
    *   Relay-Assisted Communication
    *   Star (Client-Server for specific services)
    *   Local-Cluster Communication
    *   Hybrid Models
*   **Peer Roles:** Different node types with potentially different network behaviors:
    *   User Nodes (intermittent connectivity)
    *   Agent Nodes (potentially more stable)
    *   Compute Nodes/Providers
    *   Storage Nodes (including Sync Relays, Archive Nodes)
    *   Coordinator Nodes (if used)
*   **Identity:** Relies on DIDs from the Access & Auth Layer.
*   **Trust Zones:** Peers may form groups based on shared trust or policy.

## Discovery & Routing

*   **Decentralized Discovery Mechanisms:**
    *   Distributed Hash Tables (DHTs, e.g., Kademlia via libp2p)
    *   Rendezvous Points/Servers
    *   Local Network Discovery (mDNS, DNS-SD)
    *   Trusted Registries (optional, potentially community-run)
*   **Routing Models:**
    *   Direct Connection (if discoverable and reachable)
    *   Relayed Connection (via mutually trusted peers or dedicated relays)
    *   Gossip Protocols (for broadcast/multicast information)
    *   Content-Addressed Routing (finding peers holding specific CIDs, e.g., via DHT provider records)
*   **Metadata:** Discovery records are signed, potentially including capabilities or roles.
*   **Decision Logic:** Routing and peer selection can be influenced by trust scores, latency, UCANs, and policy.

## Transport Layer

*   **Modular Design:** Pluggable transport protocols (likely leveraging libp2p's capabilities):
    *   QUIC (preferred for web/modern environments)
    *   WebRTC (for browser-to-browser/agent)
    *   WebTransport
    *   TCP
    *   Bluetooth / Local transports (optional)
*   **Authentication:** Transport connections are authenticated using peer DIDs (via Access & Auth Layer).
*   **Encryption:** End-to-end encryption is standard for connections (e.g., Noise protocol framework, TLS 1.3).
*   **UCAN Binding:** Transport sessions can be bound to specific UCAN capabilities, limiting the scope of communication.
*   **Signed Envelopes:** Messages are wrapped in signed envelopes authenticating the sender.

## Messaging & Framing

*   **Standard Envelope:** Defines a common structure for all messages:
    *   Sender DID
    *   Recipient DID (or topic for pub/sub)
    *   Message Type (e.g., `task-request`, `crdt-delta`, `ucan-invocation`, `presence-update`, `kg-query`)
    *   Payload (actual message content, typically DAG-CBOR encoded)
    *   UCAN (embedded or referenced, authorizing the message/action)
    *   Signature (over the envelope contents)
*   **Framing:** Specifies how messages are delimited on the wire (e.g., length-prefixing).
*   **Serialization:** DAG-CBOR is the likely default for payloads due to IPLD integration.
*   **UCAN Validation:** Receiving peers validate the embedded/referenced UCAN against the message type, sender, and intended action before processing.

## Secure Channels

*   Established using DID-based mutual authentication (e.g., via Noise Handshake Pattern, TLS with client/server certs derived from DIDs).
*   **Session Keys:** Derived using cryptographic key agreement (e.g., ECDH) for symmetric encryption during the session.
*   **Forward Secrecy:** Ensures past sessions remain secure even if long-term keys are compromised.
*   **Scoped Sessions:** Channels can be established for specific purposes or bound by UCANs.
*   **Auditable:** Session establishment and key exchanges can be logged for auditing.

## UCAN Transport Integration

*   **Embedding:** UCANs can be directly included in message envelopes.
*   **Referencing:** Messages can reference UCANs stored elsewhere (e.g., in KG, identified by CID).
*   **Session Scoping:** A UCAN can authorize an entire communication session, avoiding per-message UCAN transmission.
*   **Binding:** Cryptographically binding messages to UCANs ensures the authorization applies specifically to that message/action.
*   **Stateless Authorization:** Enables peers to authorize requests without complex session state management.

## Reliability & Ordering

*   Provides options for different delivery semantics:
    *   Best-Effort Unordered (UDP-like)
    *   Reliable Ordered (TCP-like)
*   Leverages underlying transport reliability mechanisms.
*   Session management for connection state.
*   Application-level ACKs/retries where needed (e.g., for critical messages).
*   Integrates with **CRDT sync protocols** (from Coordination Layer) which handle their own state convergence despite potential out-of-order delivery.
*   **Offline Queuing:** Mechanisms to queue messages for offline peers for later delivery upon reconnection (potentially via relays or sync protocols).

## Optimization Strategies

*   **Delta Sync:** Transmitting only changes (CRDT deltas) instead of full state.
*   **Gossip Optimization:** Techniques like Epidemic Broadcast Trees or Pub/Sub filtering to reduce redundant message propagation.
*   **Compression:** Applying compression to message payloads.
*   **Rate Limiting & Flow Control:** Preventing network overload.
*   **Load Balancing:** Distributing traffic across multiple relays or paths.
*   **Prioritizing Local Routing:** Preferring local network paths when available.
