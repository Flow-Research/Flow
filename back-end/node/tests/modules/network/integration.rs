use libp2p::Multiaddr;
use node::bootstrap::init::NodeData;
use node::modules::network::config::{
    BootstrapConfig, ConnectionLimits, MdnsConfig, NetworkConfig,
};
use node::modules::network::gossipsub::{GossipSubConfig, Message, MessagePayload};
use node::modules::network::manager::NetworkManager;
use serial_test::serial;
use std::collections::HashSet;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tokio::time::sleep;
use tracing::info;

use crate::bootstrap::init::{NETWORK_MANAGER_LOCK, create_test_node_data, setup_test_env};

// ============================================================================
// Test Helpers
// ============================================================================

/// Creates a network config with TCP (for reliable local testing)
fn create_integration_config(port: u16, mdns_enabled: bool) -> NetworkConfig {
    NetworkConfig {
        enable_quic: false, // Use TCP for deterministic local testing
        listen_port: port,
        bootstrap_peers: vec![],
        mdns: MdnsConfig {
            enabled: mdns_enabled,
            service_name: "_flow-test._udp.local".to_string(),
            query_interval_secs: 2,
        },
        connection_limits: ConnectionLimits {
            max_connections: 50,
            ..Default::default()
        },
        bootstrap: BootstrapConfig {
            auto_dial: true,
            auto_bootstrap: true,
            startup_delay_ms: 100,
            max_retries: 3,
            retry_delay_base_ms: 500,
        },
        gossipsub: GossipSubConfig {
            enabled: true,
            heartbeat_interval: Duration::from_millis(300),
            ..Default::default()
        },
    }
}

/// Creates a bootstrap config that connects to a specific peer
fn create_config_with_bootstrap(port: u16, bootstrap_addr: Multiaddr) -> NetworkConfig {
    let mut config = create_integration_config(port, false);
    config.bootstrap_peers = vec![bootstrap_addr];
    config
}

/// Context for a single test node
struct TestNode {
    manager: NetworkManager,
    _node_data: NodeData,
    temp_dir: TempDir,
    name: String,
}

impl TestNode {
    async fn new(name: &str) -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        // Set env vars for NetworkManager::new() (reads GOSSIP_MESSAGE_DB_PATH)
        setup_test_env(&temp_dir, name);

        let node_data = create_test_node_data();
        let manager = NetworkManager::new(&node_data)
            .await
            .expect("Failed to create NetworkManager");

        TestNode {
            manager,
            _node_data: node_data,
            temp_dir,
            name: name.to_string(),
        }
    }

    fn peer_id(&self) -> &libp2p::PeerId {
        self.manager.local_peer_id()
    }

    async fn start(&self, config: &NetworkConfig) {
        // IMPORTANT: Re-set env vars before start() because they may have been
        // overwritten by other TestNode::new() calls
        setup_test_env(&self.temp_dir, &self.name);

        self.manager.start(config).await.expect("Failed to start");
    }

    async fn stop(&self) {
        self.manager.stop().await.expect("Failed to stop");
    }

    async fn peer_count(&self) -> usize {
        self.manager.peer_count().await.unwrap_or(0)
    }

    async fn connected_peer_ids(&self) -> HashSet<libp2p::PeerId> {
        self.manager
            .connected_peers()
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|p| p.peer_id)
            .collect()
    }

    fn listen_addr(&self, port: u16) -> Multiaddr {
        format!("/ip4/127.0.0.1/tcp/{}/p2p/{}", port, self.peer_id())
            .parse()
            .expect("Invalid multiaddr")
    }
}

/// Wait for nodes to discover each other with polling
async fn wait_for_discovery(
    nodes: &[&TestNode],
    expected_peers_per_node: usize,
    timeout: Duration,
) -> bool {
    let start = Instant::now();
    let poll_interval = Duration::from_millis(500);

    while start.elapsed() < timeout {
        let mut all_connected = true;

        for node in nodes {
            let peer_count = node.peer_count().await;
            if peer_count < expected_peers_per_node {
                all_connected = false;
                break;
            }
        }

        if all_connected {
            info!(
                elapsed_ms = start.elapsed().as_millis(),
                "All nodes connected"
            );
            return true;
        }

        sleep(poll_interval).await;
    }

    false
}

/// Wait for star topology: one hub node with multiple peers, spokes with at least one
async fn wait_for_star_topology(hub: &TestNode, spokes: &[&TestNode], timeout: Duration) -> bool {
    let start = Instant::now();
    let poll_interval = Duration::from_millis(500);
    let expected_hub_peers = spokes.len();

    while start.elapsed() < timeout {
        let hub_peers = hub.peer_count().await;

        // Check if hub has all spokes connected
        if hub_peers >= expected_hub_peers {
            // Check if all spokes have at least the hub
            let mut all_spokes_connected = true;
            for spoke in spokes {
                if spoke.peer_count().await < 1 {
                    all_spokes_connected = false;
                    break;
                }
            }

            if all_spokes_connected {
                info!(
                    elapsed_ms = start.elapsed().as_millis(),
                    hub_peers = hub_peers,
                    "Star topology formed"
                );
                return true;
            }
        }

        sleep(poll_interval).await;
    }

    false
}

// ============================================================================
// 3-Node Discovery Tests
// ============================================================================

#[tokio::test]
#[serial]
async fn test_three_node_discovery_via_mdns() {
    // This test verifies that 3 nodes can discover each other via mDNS

    let _guard = NETWORK_MANAGER_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());

    // Create three nodes
    let node1 = TestNode::new("mdns_3node_1").await;
    let node2 = TestNode::new("mdns_3node_2").await;
    let node3 = TestNode::new("mdns_3node_3").await;

    let config1 = create_integration_config(0, true);
    let config2 = create_integration_config(0, true);
    let config3 = create_integration_config(0, true);

    // Start all nodes
    node1.start(&config1).await;
    info!(peer_id = %node1.peer_id(), "Node 1 started");

    node2.start(&config2).await;
    info!(peer_id = %node2.peer_id(), "Node 2 started");

    node3.start(&config3).await;
    info!(peer_id = %node3.peer_id(), "Node 3 started");

    // Wait for full mesh (each node should see 2 peers)
    let discovered = wait_for_discovery(
        &[&node1, &node2, &node3],
        2, // Each node should have 2 peers
        Duration::from_secs(30),
    )
    .await;

    assert!(
        discovered,
        "All 3 nodes should discover each other within 30 seconds"
    );

    // Verify the mesh topology
    let peers1 = node1.connected_peer_ids().await;
    let peers2 = node2.connected_peer_ids().await;
    let peers3 = node3.connected_peer_ids().await;

    assert!(peers1.contains(node2.peer_id()), "Node 1 should see Node 2");
    assert!(peers1.contains(node3.peer_id()), "Node 1 should see Node 3");
    assert!(peers2.contains(node1.peer_id()), "Node 2 should see Node 1");
    assert!(peers2.contains(node3.peer_id()), "Node 2 should see Node 3");
    assert!(peers3.contains(node1.peer_id()), "Node 3 should see Node 1");
    assert!(peers3.contains(node2.peer_id()), "Node 3 should see Node 2");

    info!("✓ 3-node mDNS discovery verified: full mesh established");

    // Cleanup
    node1.stop().await;
    node2.stop().await;
    node3.stop().await;
}

#[tokio::test]
#[serial]
async fn test_three_node_discovery_via_bootstrap() {
    // This test verifies that 3 nodes connect via bootstrap in a STAR topology
    // Node 1 is the bootstrap node (hub), Node 2 and 3 connect to it (spokes)
    // Note: Bootstrap does NOT create a full mesh - spokes only connect to the hub

    let _guard = NETWORK_MANAGER_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());

    // Create bootstrap node (Node 1 - the hub)
    let node1 = TestNode::new("bootstrap_3node_1").await;
    let mut config1 = create_integration_config(19001, false);
    config1.bootstrap_peers = vec![];

    node1.start(&config1).await;
    info!(peer_id = %node1.peer_id(), "Bootstrap node (hub) started");

    sleep(Duration::from_millis(500)).await;

    // Create bootstrap address for Node 1
    let bootstrap_addr = node1.listen_addr(19001);
    info!(bootstrap_addr = %bootstrap_addr, "Bootstrap address");

    // Create Node 2 connecting to Node 1 (spoke)
    let node2 = TestNode::new("bootstrap_3node_2").await;
    let config2 = create_config_with_bootstrap(19002, bootstrap_addr.clone());

    node2.start(&config2).await;
    info!(peer_id = %node2.peer_id(), "Node 2 (spoke) started");

    // Create Node 3 connecting to Node 1 (spoke)
    let node3 = TestNode::new("bootstrap_3node_3").await;
    let config3 = create_config_with_bootstrap(19003, bootstrap_addr);

    node3.start(&config3).await;
    info!(peer_id = %node3.peer_id(), "Node 3 (spoke) started");

    // Wait for star topology: hub has 2 peers, spokes have at least 1
    let star_formed = wait_for_star_topology(
        &node1,            // hub
        &[&node2, &node3], // spokes
        Duration::from_secs(15),
    )
    .await;

    assert!(star_formed, "Star topology should form via bootstrap");

    // Verify Node 1 (bootstrap/hub) sees both spokes
    let peers1 = node1.connected_peer_ids().await;
    assert!(
        peers1.contains(node2.peer_id()),
        "Bootstrap node should see Node 2"
    );
    assert!(
        peers1.contains(node3.peer_id()),
        "Bootstrap node should see Node 3"
    );

    // Verify Node 2 and Node 3 (spokes) see the bootstrap node
    let peers2 = node2.connected_peer_ids().await;
    let peers3 = node3.connected_peer_ids().await;
    assert!(
        peers2.contains(node1.peer_id()),
        "Node 2 should see bootstrap node"
    );
    assert!(
        peers3.contains(node1.peer_id()),
        "Node 3 should see bootstrap node"
    );

    info!("✓ 3-node bootstrap star topology verified");

    // Cleanup
    node1.stop().await;
    node2.stop().await;
    node3.stop().await;
}

#[tokio::test]
#[serial]
async fn test_three_node_message_propagation() {
    // Test that messages propagate across all 3 nodes via GossipSub

    let _guard = NETWORK_MANAGER_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());

    // Setup 3 nodes with mDNS
    let node1 = TestNode::new("gossip_3node_1").await;
    let node2 = TestNode::new("gossip_3node_2").await;
    let node3 = TestNode::new("gossip_3node_3").await;

    let config1 = create_integration_config(0, true);
    let config2 = create_integration_config(0, true);
    let config3 = create_integration_config(0, true);

    node1.start(&config1).await;
    node2.start(&config2).await;
    node3.start(&config3).await;

    // Wait for mesh to form
    let discovered =
        wait_for_discovery(&[&node1, &node2, &node3], 2, Duration::from_secs(30)).await;

    assert!(discovered, "Nodes should form mesh before messaging");

    // All nodes subscribe to the same topic
    let test_topic = "/flow/v1/test/propagation";
    node1.manager.subscribe(test_topic).await.unwrap();
    node2.manager.subscribe(test_topic).await.unwrap();
    node3.manager.subscribe(test_topic).await.unwrap();

    // Wait for GossipSub mesh to stabilize
    sleep(Duration::from_secs(2)).await;

    // Get subscription handles for receiving
    let mut sub2 = node2
        .manager
        .subscription_manager
        .subscribe(test_topic)
        .await;

    let mut sub3 = node3
        .manager
        .subscription_manager
        .subscribe(test_topic)
        .await;

    // Node 1 publishes a message
    let test_payload = MessagePayload::Custom {
        kind: "test".to_string(),
        data: serde_json::json!({"test": "propagation_3node"}),
    };

    let message = Message::new(*node1.peer_id(), test_topic, test_payload);
    node1.manager.publish(message).await.unwrap();

    info!("Message published from Node 1");

    // Wait for message to propagate
    let timeout = Duration::from_secs(5);

    let received2 = tokio::time::timeout(timeout, sub2.recv()).await;
    let received3 = tokio::time::timeout(timeout, sub3.recv()).await;

    assert!(
        received2.is_ok(),
        "Node 2 should receive message from Node 1"
    );
    assert!(
        received3.is_ok(),
        "Node 3 should receive message from Node 1"
    );

    info!("✓ 3-node message propagation verified");

    // Cleanup
    node1.stop().await;
    node2.stop().await;
    node3.stop().await;
}

#[tokio::test]
#[serial]
async fn test_node_join_existing_mesh() {
    // Test that a new node can join an existing 2-node mesh

    let _guard = NETWORK_MANAGER_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());

    // Start 2 nodes first with a small delay between to prevent race conditions
    let node1 = TestNode::new("join_mesh_1").await;
    let config1 = create_integration_config(0, true);
    node1.start(&config1).await;
    info!(peer_id = %node1.peer_id(), "Node 1 started");

    // Small delay to let node 1 start advertising via mDNS
    sleep(Duration::from_millis(500)).await;

    let node2 = TestNode::new("join_mesh_2").await;
    let config2 = create_integration_config(0, true);
    node2.start(&config2).await;
    info!(peer_id = %node2.peer_id(), "Node 2 started");

    // Wait for initial mesh (increase timeout for reliability)
    let mesh_formed = wait_for_discovery(&[&node1, &node2], 1, Duration::from_secs(20)).await;
    assert!(mesh_formed, "Initial 2-node mesh should form");

    info!("Initial 2-node mesh established");

    // Now join a third node
    let node3 = TestNode::new("join_mesh_3").await;
    let config3 = create_integration_config(0, true);

    node3.start(&config3).await;
    info!(peer_id = %node3.peer_id(), "Node 3 joining existing mesh");

    // Wait for full mesh
    let full_mesh = wait_for_discovery(&[&node1, &node2, &node3], 2, Duration::from_secs(25)).await;

    assert!(full_mesh, "Node 3 should join existing mesh");

    info!("✓ Node successfully joined existing mesh");

    // Cleanup
    node1.stop().await;
    node2.stop().await;
    node3.stop().await;
}

#[tokio::test]
#[serial]
async fn test_node_departure_mesh_remains() {
    // Test that when a node leaves, remaining nodes stay connected

    let _guard = NETWORK_MANAGER_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());

    // Start nodes with staggered delays to prevent race conditions
    let node1 = TestNode::new("depart_mesh_1").await;
    let config1 = create_integration_config(0, true);
    node1.start(&config1).await;
    info!(peer_id = %node1.peer_id(), "Node 1 started");

    // Small delay to let node 1 start advertising via mDNS
    sleep(Duration::from_millis(300)).await;

    let node2 = TestNode::new("depart_mesh_2").await;
    let config2 = create_integration_config(0, true);
    node2.start(&config2).await;
    info!(peer_id = %node2.peer_id(), "Node 2 started");

    sleep(Duration::from_millis(300)).await;

    let node3 = TestNode::new("depart_mesh_3").await;
    let config3 = create_integration_config(0, true);
    node3.start(&config3).await;
    info!(peer_id = %node3.peer_id(), "Node 3 started");

    // Wait for full mesh (increase timeout for reliability)
    let mesh_formed =
        wait_for_discovery(&[&node1, &node2, &node3], 2, Duration::from_secs(30)).await;
    assert!(mesh_formed, "Initial mesh should form");

    info!("Initial 3-node mesh established, stopping Node 3");

    // Stop Node 3
    node3.stop().await;

    // Wait a moment for disconnect to propagate
    sleep(Duration::from_secs(2)).await;

    // Verify Node 1 and Node 2 are still connected
    let peers1 = node1.connected_peer_ids().await;
    let peers2 = node2.connected_peer_ids().await;

    assert!(
        peers1.contains(node2.peer_id()),
        "Node 1 should still be connected to Node 2"
    );
    assert!(
        peers2.contains(node1.peer_id()),
        "Node 2 should still be connected to Node 1"
    );
    assert!(
        !peers1.contains(node3.peer_id()),
        "Node 1 should not see departed Node 3"
    );

    info!("✓ Mesh remains stable after node departure");

    // Cleanup
    node1.stop().await;
    node2.stop().await;
}

#[tokio::test]
#[serial]
async fn test_five_node_mesh_scalability() {
    // Test that 5 nodes can form a mesh (stress test)

    let _guard = NETWORK_MANAGER_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());

    let mut nodes = Vec::new();

    // Create and start 5 nodes
    for i in 0..5 {
        let node = TestNode::new(&format!("scale_5node_{}", i)).await;
        let config = create_integration_config(0, true);
        node.start(&config).await;
        info!(peer_id = %node.peer_id(), node = i, "Node started");
        nodes.push(node);
        sleep(Duration::from_millis(300)).await;
    }

    // Wait for full mesh (each node should see 4 peers)
    let refs: Vec<&TestNode> = nodes.iter().collect();
    let mesh_formed = wait_for_discovery(&refs, 4, Duration::from_secs(60)).await;

    assert!(mesh_formed, "5-node mesh should form within 60 seconds");

    // Verify each node sees 4 peers
    for (i, node) in nodes.iter().enumerate() {
        let peer_count = node.peer_count().await;
        info!(node = i, peers = peer_count, "Node peer count");
        assert!(
            peer_count >= 4,
            "Node {} should have at least 4 peers, has {}",
            i,
            peer_count
        );
    }

    info!("✓ 5-node mesh scalability verified");

    // Cleanup
    for node in nodes {
        node.stop().await;
    }
}
