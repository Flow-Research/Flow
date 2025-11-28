# Flow Architecture Overview

Flow is a decentralized automation network that turns goals into verifiable jobs across a peer compute marketplace. Users keep data in private, trusted environments; agents execute with signed events and receipts; state converges via sync; and programmable incentives settle contributions automatically, with provenance and privacy by default.

## Concept Diagram

![Flow concept](https://drive.google.com/uc?export=view&id=11-YO0bh2xKSGA4Zoi85Yx8zoM8vZI2Jy "Flow")

The core idea is that each user or organisation controls a portable, Knowledge Base, anchored to their identity rather than any single app. Different tools and experiences are built on top of this knowledge base.

Users can share specific parts of their Knowledge Base with other users and agents on the network. Access, usage, and changes are enforced and recorded by the network itself, using cryptographic signatures and clear policies.

Flow is not a single app or API. It is a network of protocols, services, and agents that work together to help people automate work, share knowledge, and be rewarded. Personal knowledge bases, agents, and programmable incentives create a market for skills and information that anyone can build on. Permissions, provenance, and payouts are handled automatically by transparent rules.

As users and agents co-create and share knowledge, as contributions are evaluated and rewarded by the network, a new kind of economy can emerge; one where value flows more directly to the people and agents who create it.


## Core Principles

1.  **Ubiquitous, Verifiable Computing:** Compute can run anywhere, but every claim about that compute is verifiable.
2.  **Capability-Based Access:** Authentication and authorization are decentralized. Permissions are based on Verifiable Credentials and Capabilities.
3.  **Knowledge Graphs with Provenance:** Data is stored in schema-aware graphs tracking origin and trust.
4.  **Peer-to-Peer networking:** Agents and users connect and coordinate via a decentralized network.
5.  **Decentralized Execution:** Execution is local-first, edge-native, cloud-optional. Workflows can be distributed and executed in a decentralized manner.
6.  **Agent Explainability (SLRPA):** Agents operate on a Sense → Learn → Reason → Predict → Act cycle.
7.  **Programmable Incentives:** Rewards and reputation are customizable and trackable.


## Layered Architecture

1.  [**Storage Layer**](./specs/01_storage_layer.md): Implements persistent storage over content-addressed systems (like IPFS).
2.  [**Access & Auth Layer**](./specs/02_access_auth_layer.md): Manages identity (DIDs) and permissions using capability-based systems.
3.  [**Network Layer**](./specs/03_network_layer.md): Handles peer-to-peer discovery, communication, data synchronization, and transport, secured by the Auth Layer.
4.  [**Coordination & Sync Layer**](./specs/04_coordination_sync_layer.md): Ensures state consistency across different agents and nodes using the underlying sync mechanisms.
5.  [**Knowledge Graph Layer**](./specs/05_knowledge_graph.md): Provides the semantic context (KG) for data.
6. [**MCP Layer**](./specs/06_mcp.md): Defines how external models/tools (via Model Context Protocol) interact with user content.
7.  [**User Interface / UX Layer**](./specs/07_ui_ux_layer.md): Provides the means for users to interact with the system, manage agents, delegate tasks, etc.
8.  [**Agent Layer**](./specs/08_agent_layer.md): Manages agent lifecycles (Sense→Learn→Reason→Predict→Act) and ensures their actions are explainable.
9.  [**Execution Layer**](./specs/09_execution_layer.md): Handles the definition and execution of workflows as graphs of steps, where each state change is cryptographically signed and verifiable.
10.  [**Compute Layer**](./specs/10_compute_layer.md): Executes the actual computational tasks defined in the Execution Layer, potentially using various backends (local, distributed like Bacalhau) and supporting verifiable computation.
11. [**Incentive Layer**](./specs/11_incentive_layer.md): Defines and manages programmable rewards and contribution tracking based on provenance data in the knowledge graph.


## Flow of Activity

1.  Create and update your objects.
2.  Everything is signed and secure using your digital ID.
3.  Connect to the Flow network and find what you need.
4.  Start Agents that plug into your own knowledge and data.
5.  Agents sense, learn, think, predict, and act on your behalf.
6.  Their actions and results are recorded and checked.
7.  Rewards are given based on clear rules and proven contributions.
8.  Explore and manage all of this through a simple, visual interface.


## Running the Codebase
The project uses [nx](https://nx.dev/) to manage the workspace. You will need to install nx to run the commands below.

### Prerequisites

- Rust and Cargo installed
- Node.js and npm/yarn/pnpm


### Getting Started

To get started, you need to install dependencies for the `back-end` and `user-interface` projects separately.

1.  **Install front-end dependencies:**
    Nx can handle this for you by running the `install-all` command from the root directory.
    ```bash
    nx install-all user-interface
    ```
    This will install the npm packages for both `flow-web` and `flow-app`.

2.  **Build the back-end dependencies:**
    Building the back-end will fetch and compile all the Rust crates.
    ```bash
    nx build back-end
    ```


### Available Commands

You can run commands (called "targets") on specific projects using the `nx` CLI from the root of the repository.

#### Back-End Commands (`back-end`)

-   **Run the node:**
    ```bash
    nx run-node back-end
    ```

-   **Build the node for production:**
    ```bash
    nx build back-end
    ```

-   **Run tests:**
    ```bash
    nx test back-end
    ```


#### User Interface Commands (`user-interface`)

-   **Install all dependencies for user-interface apps**
    ```bash
    nx install-all user-interface
    ```

-   **Run the web app in development mode:**
    ```bash
    nx dev-web user-interface
    ```

-   **Run the mobile app in development mode:**
    ```bash
    nx dev-mobile user-interface
    ```

-   **Run the desktop app in development mode:**
    ```bash
    nx dev-desktop user-interface
    ```

-   **Build all UI applications (web, desktop):**
    ```bash
    nx build-all user-interface
    ```



### Environment Variables

- `LOG_LEVEL` - Controls logging verbosity (default: `info`)
  - Can be set to standard levels: `error`, `warn`, `info`, `debug`, `trace`
  - Supports per-crate configuration: `crate1=level1,crate2=level2`


---

## Contributing
Go over the [Contributing](CONTRIBUTING.md) guide to learn how you can contribute. 


## License
This project is licensed under the Apache License 2.0 - see the [LICENSE](LICENSE) file for details.


## Where to get help?
Join the Discord community and chat with the development team: [here](https://discord.gg/JmkvP6xKFW)
