# Flow Architecture Overview

Flow is a decentralized coordination platform enabling agents and users to co-create, manage, and reward knowledge, tasks, and compute in a verifiable, local-first environment.


## Core Principles

1.  **Ubiquitous, Verifiable Computing:** Compute can run anywhere, but every claim about that compute is verifiable.
2.  **Capability-Based Access:** Authentication and authorization is decentralized. Permissions are based on Verifiable Credentials and Capabilities.
3.  **Knowledge Graphs with Provenance:** Data is stored in schema-aware graphs tracking origin and trust.
4.  **Peer to Peer networking:** Agents and users connect and coordinate via a decentralized network.
5.  **Decentralized Execution:** Execution is local-first, workflows can be distributed and executed in a decentralized manner.
6.  **Agent Explainability (SLRPA):** Agents operate on a Sense → Learn → Reason → Predict → Act cycle.
7.  **Programmable Incentives:** Rewards and reputation are customizable and trackable.


## Layered Architecture

1.  [**Storage Layer**](./specs/01_storage_layer.md): Implements the local-first, persistent storage using CRDTs over content-addressed systems (like IPFS/BadgerDB).
2.  [**Access & Auth Layer**](./specs/02_access_auth_layer.md): Manages identity (DIDs) and permissions using capability-based systems.
3.  [**Network Layer**](./specs/03_network_layer.md): Handles peer-to-peer discovery, communication, data synchronization, and transport, secured by the Auth Layer.
4.  [**Coordination & Sync Layer**](./specs/04_coordination_sync_layer.md): Ensures state consistency across different agents and nodes using the underlying CRDT mechanisms.
5.  [**Knowledge Graph Layer**](./specs/05_knowledge_graph.md): Provides the semantic context (KG) for data.
6. [**MCP Layer**](./specs/06_mcp.md): Defines how external models/tools (via Model Context Protocol) interact with user content.
7.  [**User Interface / UX Layer**](./specs/07_ui_ux_layer.md): Provides the means for users to interact with the system, inspect the graph, manage agents, delegate tasks, etc.
8.  [**Agent Layer**](./specs/08_agent_layer.md): Manages agent lifecycles (Sense→Learn→Reason→Predict→Act) and ensures their actions are explainable.
9.  [**Execution Layer**](./specs/09_execution_layer.md): Handles the definition and running of workflows as Directed Acyclic Graphs (DAGs), managing signed state transitions.
10.  [**Compute Layer**](./specs/10_compute_layer.md): Executes the actual computational tasks defined in the Execution Layer, potentially using various backends (local, distributed like Bacalhau) and supporting verifiable computation.
11. [**Incentive Layer**](./specs/11_incentive_layer.md): Defines and manages programmable rewards and contribution tracking based on provenance data in the knowledge graph.
12. **Physical Layer**: Provides verifiable access to decentralized physical infrastructure (compute, network, storage, sensors, actuators), using DePIN protocols and trust attestations.


## Flow of Activity

1.  Create/update objects (CRDT deltas, sync DAG).
2.  Sign with DIDs, governed by capability tokens.
3.  Discover and connect to Flow network.
4.  Grant access control and permissions to objects.
5.  Users can spin up Agents connected to their knowledge graphs. 
6.  Agents execute via SLRPA (Sense → Learn → Reason → Predict → Act).
7.  Results logged, verified.
8.  Rewards triggered by provenance/policy.
9.  Explore via graph UI.



## Running the Codebase
The project uses [nx](https://nx.dev/) to manage the workspace. You will need to install nx to run the commands below.

### Prerequisites

- Rust and Cargo installed
- Node.js and npm/yarn/pnpm

Depending on how you installed `nx`. In the project root, run 

```sh
nx --version

# Output
# Nx Version:
# - Local: Not found
# - Global: v21.6.4
```

you might need to initilise `nx`. If you installed, `nx` with `brew` or the output of the command above returns ` - Local: Not found`

```
nx init --interactive=false --useDotNxInstallation=true
```

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
