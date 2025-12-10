use errors::AppError;
use node::modules::network::config::{
    BootstrapConfig, ConnectionLimits, MdnsConfig, NetworkConfig,
};
use node::modules::network::gossipsub::{GossipSubConfig, Message, MessagePayload};
use node::modules::network::manager::NetworkManager;
use serial_test::serial;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tokio::time::sleep;
use tracing::{debug, info, warn};

use crate::bootstrap::init::{
    NETWORK_MANAGER_LOCK, create_test_node_data, set_env, setup_test_env,
};

// ============================================================================
// Performance Metrics
// ============================================================================

#[derive(Debug, Clone)]
pub struct PerformanceMetrics {
    pub connection_latency_ms: f64,
    pub message_latency_ms: f64,
    pub messages_per_second: f64,
    pub messages_sent: u64,
    pub messages_received: u64,
    pub duration_secs: f64,
}

impl PerformanceMetrics {
    pub fn report(&self) {
        info!("=== Performance Report ===");
        info!("Connection latency: {:.2} ms", self.connection_latency_ms);
        info!("Message latency: {:.2} ms", self.message_latency_ms);
        info!("Throughput: {:.2} messages/sec", self.messages_per_second);
        info!(
            "Messages: {} sent, {} received",
            self.messages_sent, self.messages_received
        );
        info!("Test duration: {:.2} sec", self.duration_secs);
        info!("=========================");
    }
}

// ============================================================================
// Test Helpers
// ============================================================================

fn create_performance_config(port: u16) -> NetworkConfig {
    NetworkConfig {
        enable_quic: false,
        listen_port: port,
        bootstrap_peers: vec![],
        mdns: MdnsConfig {
            enabled: true,
            service_name: "_flow-perf._udp.local".to_string(),
            query_interval_secs: 2,
        },
        connection_limits: ConnectionLimits::default(),
        bootstrap: BootstrapConfig::default(),
        gossipsub: GossipSubConfig {
            enabled: true,
            heartbeat_interval: Duration::from_millis(200),
            ..Default::default()
        },
    }
}

struct PerfTestNode {
    manager: NetworkManager,
    temp_dir: TempDir,
    name: String,
}

impl PerfTestNode {
    async fn new(name: &str) -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        setup_test_env(&temp_dir, name);

        set_env("CONTENT_TRANSFER_ENABLED", "false");

        let node_data = create_test_node_data();
        let manager = NetworkManager::new(&node_data)
            .await
            .expect("Failed to create NetworkManager");

        PerfTestNode {
            manager,
            temp_dir: temp_dir,
            name: name.to_string(),
        }
    }

    /// Re-set env vars and start the manager
    async fn start(&self, config: &NetworkConfig) -> Result<(), AppError> {
        // IMPORTANT: Re-set env vars before start() because they may have been
        // overwritten by other PerfTestNode::new() calls
        setup_test_env(&self.temp_dir, &self.name);
        set_env("CONTENT_TRANSFER_ENABLED", "false");
        self.manager.start(config).await
    }

    async fn stop(&self) -> Result<(), AppError> {
        self.manager.stop().await
    }
}

// ============================================================================
// Performance Tests
// ============================================================================

#[tokio::test]
#[serial]
async fn test_connection_establishment_latency() {
    // Measures the time from node start to first peer connection

    let _guard = NETWORK_MANAGER_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());

    // Start first node
    let node1 = PerfTestNode::new("perf_conn_1").await;
    let config1 = create_performance_config(0);
    node1.start(&config1).await.unwrap();

    sleep(Duration::from_secs(1)).await;

    // Start second node and measure connection time
    let node2 = PerfTestNode::new("perf_conn_2").await;
    let config2 = create_performance_config(0);

    let start = Instant::now();
    node2.start(&config2).await.unwrap();

    // Poll for connection
    let mut connected = false;
    let timeout = Duration::from_secs(30);

    while start.elapsed() < timeout {
        if node2.manager.peer_count().await.unwrap() >= 1 {
            connected = true;
            break;
        }
        sleep(Duration::from_millis(50)).await;
    }

    let latency_ms = start.elapsed().as_secs_f64() * 1000.0;

    assert!(connected, "Nodes should connect within timeout");

    info!(latency_ms = latency_ms, "Connection establishment latency");

    // Connection should typically be under 5 seconds for mDNS
    assert!(
        latency_ms < 10_000.0,
        "Connection latency should be under 10 seconds, was {:.2}ms",
        latency_ms
    );

    let metrics = PerformanceMetrics {
        connection_latency_ms: latency_ms,
        message_latency_ms: 0.0,
        messages_per_second: 0.0,
        messages_sent: 0,
        messages_received: 0,
        duration_secs: start.elapsed().as_secs_f64(),
    };

    metrics.report();

    // Cleanup
    node1.manager.stop().await.unwrap();
    node2.manager.stop().await.unwrap();
}

#[tokio::test]
#[serial]
async fn test_message_delivery_latency() {
    // Measures end-to-end message delivery time

    let _guard = NETWORK_MANAGER_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());

    // Create and start node1 BEFORE creating node2
    let node1 = PerfTestNode::new("perf_msg_1").await;
    let config1 = create_performance_config(0);
    node1.start(&config1).await.unwrap();

    // Now create and start node2
    let node2 = PerfTestNode::new("perf_msg_2").await;
    let config2 = create_performance_config(0);
    node2.start(&config2).await.unwrap();

    // Wait for connection
    let connected = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if node1.manager.peer_count().await.unwrap() >= 1
                && node2.manager.peer_count().await.unwrap() >= 1
            {
                break;
            }
            sleep(Duration::from_millis(100)).await;
        }
    })
    .await;

    assert!(connected.is_ok(), "Nodes should connect");

    // Subscribe to topic
    let test_topic = "/flow/v1/perf/latency";
    node1.manager.subscribe(test_topic).await.unwrap();
    node2.manager.subscribe(test_topic).await.unwrap();

    sleep(Duration::from_secs(2)).await;

    // Setup receiver
    let mut sub2 = node2
        .manager
        .subscription_manager
        .subscribe(test_topic)
        .await;

    // Measure round-trip for multiple messages
    let num_messages = 10;
    let mut latencies = Vec::with_capacity(num_messages);

    for i in 0..num_messages {
        let send_time = Instant::now();

        let payload = MessagePayload::Ping { nonce: i as u64 };
        let message = Message::new(*node1.manager.local_peer_id(), test_topic, payload);

        node1.manager.publish(message.clone()).await.unwrap();

        // Wait for receipt
        let result = tokio::time::timeout(Duration::from_secs(5), sub2.recv()).await;

        if let Ok(Ok(_)) = result {
            let latency = send_time.elapsed().as_secs_f64() * 1000.0;
            latencies.push(latency);
            debug!(message = i, latency_ms = latency, "Message delivered");
        } else {
            warn!(message = i, "Message delivery timeout");
        }
    }

    assert!(
        !latencies.is_empty(),
        "At least some messages should be delivered"
    );

    let avg_latency: f64 = latencies.iter().sum::<f64>() / latencies.len() as f64;
    let min_latency = latencies.iter().cloned().fold(f64::INFINITY, f64::min);
    let max_latency = latencies.iter().cloned().fold(0.0, f64::max);

    info!(
        avg_ms = avg_latency,
        min_ms = min_latency,
        max_ms = max_latency,
        delivered = latencies.len(),
        total = num_messages,
        "Message latency statistics"
    );

    // Messages should typically be under 500ms
    assert!(
        avg_latency < 1000.0,
        "Average latency should be under 1 second, was {:.2}ms",
        avg_latency
    );

    // Cleanup
    node1.manager.stop().await.unwrap();
    node2.manager.stop().await.unwrap();
}

#[tokio::test]
#[serial]
async fn test_message_throughput() {
    // Measures messages per second throughput

    let _guard = NETWORK_MANAGER_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());

    // Create and start node1 BEFORE creating node2
    let node1 = PerfTestNode::new("perf_msg_1").await;
    let config1 = create_performance_config(0);
    node1.start(&config1).await.unwrap();

    // Now create and start node2
    let node2 = PerfTestNode::new("perf_msg_2").await;
    let config2 = create_performance_config(0);
    node2.start(&config2).await.unwrap();

    // Wait for connection
    let connected = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if node1.manager.peer_count().await.unwrap() >= 1 {
                break;
            }
            sleep(Duration::from_millis(100)).await;
        }
    })
    .await;

    assert!(connected.is_ok(), "Nodes should connect");

    let test_topic = "/flow/v1/perf/throughput";
    node1.manager.subscribe(test_topic).await.unwrap();
    node2.manager.subscribe(test_topic).await.unwrap();

    sleep(Duration::from_secs(2)).await;

    // Setup counters
    let messages_sent = Arc::new(AtomicU64::new(0));
    let messages_received = Arc::new(AtomicU64::new(0));

    // Start receiver task
    let received_clone = messages_received.clone();
    let mut sub2 = node2
        .manager
        .subscription_manager
        .subscribe(test_topic)
        .await;

    let receiver_handle = tokio::spawn(async move {
        while let Ok(Ok(_)) = tokio::time::timeout(Duration::from_secs(1), sub2.recv()).await {
            received_clone.fetch_add(1, Ordering::Relaxed);
        }
    });

    // Send messages for fixed duration
    let test_duration = Duration::from_secs(5);
    let start = Instant::now();

    let node1_peer_id = *node1.manager.local_peer_id();
    let sent_clone = messages_sent.clone();
    // let manager = node1.manager.clone();

    while start.elapsed() < test_duration {
        let payload = MessagePayload::Ping {
            nonce: sent_clone.load(Ordering::Relaxed),
        };
        let message = Message::new(node1_peer_id, test_topic, payload);

        if node1.manager.publish(message).await.is_ok() {
            sent_clone.fetch_add(1, Ordering::Relaxed);
        }

        // Small delay to prevent overwhelming
        tokio::task::yield_now().await;
    }

    // Wait for receiver to finish
    sleep(Duration::from_secs(2)).await;
    receiver_handle.abort();

    let sent = messages_sent.load(Ordering::Relaxed);
    let received = messages_received.load(Ordering::Relaxed);
    let duration = start.elapsed().as_secs_f64();
    let throughput = received as f64 / duration;

    let metrics = PerformanceMetrics {
        connection_latency_ms: 0.0,
        message_latency_ms: 0.0,
        messages_per_second: throughput,
        messages_sent: sent,
        messages_received: received,
        duration_secs: duration,
    };

    metrics.report();

    // Expect at least some throughput
    assert!(throughput > 0.0, "Should have positive message throughput");

    info!(
        "✓ Throughput test: {:.2} msg/sec ({} sent, {} received)",
        throughput, sent, received
    );

    // Cleanup
    node1.manager.stop().await.unwrap();
    node2.manager.stop().await.unwrap();
}

#[tokio::test]
#[serial]
async fn test_reconnection_time() {
    // Measures time to reconnect after disconnect

    let _guard = NETWORK_MANAGER_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());

    // Create and start node1 BEFORE creating node2
    let node1 = PerfTestNode::new("perf_reconn_1").await;
    let config1 = create_performance_config(0);
    node1.start(&config1).await.unwrap();

    // Now create and start node2
    let node2 = PerfTestNode::new("perf_reconn_2").await;
    let config2 = create_performance_config(0);
    node2.start(&config2).await.unwrap();

    // Wait for initial connection
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if node1.manager.peer_count().await.unwrap() >= 1 {
                break;
            }
            sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("Initial connection should establish");

    info!("Initial connection established");

    // Stop node2 completely and drop it
    node2.manager.stop().await.unwrap();
    drop(node2); // Fully cleanup node2 to release RocksDB locks

    // Wait for disconnect to be detected
    sleep(Duration::from_secs(2)).await;

    assert_eq!(
        node1.manager.peer_count().await.unwrap(),
        0,
        "Node 1 should detect disconnect"
    );

    // Create a NEW node2 and measure reconnection time
    // NetworkManager doesn't support restart after stop, so we need a fresh instance
    let start = Instant::now();

    let node2_new = PerfTestNode::new("perf_reconn_2b").await; // Different name for fresh temp dir
    let config2_new = create_performance_config(0);
    node2_new.start(&config2_new).await.unwrap();

    let reconnected = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if node1.manager.peer_count().await.unwrap() >= 1 {
                break;
            }
            sleep(Duration::from_millis(50)).await;
        }
    })
    .await;

    let reconnection_time_ms = start.elapsed().as_secs_f64() * 1000.0;

    assert!(reconnected.is_ok(), "Should reconnect within timeout");

    info!(reconnection_ms = reconnection_time_ms, "Reconnection time");

    // Cleanup
    node1.manager.stop().await.unwrap();
    node2_new.manager.stop().await.unwrap();
}
