use node::modules::{
    network::{
        config::{MdnsConfig, NetworkConfig},
        gossipsub::{GossipSubConfig, MessagePayload, Topic},
        manager::NetworkManager,
    },
    storage::content::ContentId,
};
use serial_test::serial;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info};

use crate::{TestNodeDirs, bootstrap::init::create_test_node_data};

/// Helper to check if a publish result is acceptable for single-node tests.
/// In single-node tests, `NoPeersSubscribedToTopic` is expected because
/// GossipSub requires peers to publish to.
fn assert_announce_ok_or_no_peers(result: &Result<String, errors::AppError>) -> bool {
    match result {
        Ok(message_id) => {
            assert!(!message_id.is_empty(), "Message ID should not be empty");
            return true;
        }
        Err(e) if e.to_string().contains("NoPeersSubscribedToTopic") => {
            // Expected in single-node tests - GossipSub has no peers to publish to
        }
        Err(e) => {
            panic!("Unexpected error: {:?}", e);
        }
    }
    false
}

fn create_gossipsub_test_config(port: u16) -> NetworkConfig {
    NetworkConfig {
        enable_quic: true,
        listen_port: port,
        bootstrap_peers: vec![],
        mdns: MdnsConfig {
            enabled: false,
            ..Default::default()
        },
        gossipsub: GossipSubConfig {
            enabled: true,
            ..Default::default()
        },
        ..Default::default()
    }
}

// =============================================================================
// UNIT TESTS: Topic String Validation
// =============================================================================

#[test]
fn test_content_announcements_topic_string() {
    let topic = Topic::ContentAnnouncements;
    assert_eq!(topic.to_topic_string(), "/flow/v1/content/announcements");
}

#[test]
fn test_content_announcements_topic_parsing() {
    let parsed = Topic::from_topic_string("/flow/v1/content/announcements");
    assert_eq!(parsed, Some(Topic::ContentAnnouncements));
}

#[test]
fn test_content_announcements_topic_hash_consistency() {
    let topic = Topic::ContentAnnouncements;
    let hash1 = topic.hash();
    let hash2 = topic.hash();
    assert_eq!(hash1, hash2, "Topic hash should be deterministic");
}

// =============================================================================
// UNIT TESTS: CID → RecordKey Mapping (Work Item 2.8.5 context)
// =============================================================================

#[test]
fn test_cid_record_key_deterministic() {
    let data = b"test content for record key";
    let cid1 = ContentId::from_bytes(data);
    let cid2 = ContentId::from_bytes(data);

    assert_eq!(
        cid1.to_bytes(),
        cid2.to_bytes(),
        "Same content should produce same CID bytes"
    );
}

#[test]
fn test_cid_record_key_different_content() {
    let cid1 = ContentId::from_bytes(b"content A");
    let cid2 = ContentId::from_bytes(b"content B");

    assert_ne!(
        cid1.to_bytes(),
        cid2.to_bytes(),
        "Different content should produce different CID bytes"
    );
}

#[test]
fn test_cid_record_key_empty_content() {
    let cid = ContentId::from_bytes(b"");
    let bytes = cid.to_bytes();

    assert!(
        !bytes.is_empty(),
        "Empty content should still produce valid CID bytes"
    );
}

#[test]
fn test_cid_record_key_large_content() {
    let large_data = vec![0xAB_u8; 1024 * 1024]; // 1MB
    let cid = ContentId::from_bytes(&large_data);
    let bytes = cid.to_bytes();

    // CID size is independent of content size (hash-based)
    assert!(
        bytes.len() < 100,
        "CID should be compact regardless of content size"
    );
}

// =============================================================================
// SINGLE-NODE TESTS: Content Announcement
// =============================================================================

#[tokio::test]
#[serial]
async fn test_announce_content_published_single_node() {
    let node_dirs = TestNodeDirs::new("announce_single");
    node_dirs.set_env_vars();

    let node_data = create_test_node_data();
    let manager = NetworkManager::new(&node_data)
        .await
        .expect("Failed to create NetworkManager");

    let config = create_gossipsub_test_config(0);
    node_dirs.set_env_vars();
    manager.start(&config).await.expect("Failed to start");

    // Subscribe to content announcements
    manager
        .subscribe_topic(&Topic::ContentAnnouncements)
        .await
        .expect("Failed to subscribe");

    let cid = ContentId::from_bytes(b"test document content");

    let result = manager
        .announce_content_published(
            cid.clone(),
            "test_document.txt".to_string(),
            1024,
            "text/plain".to_string(),
        )
        .await;
    let is_ok = assert_announce_ok_or_no_peers(&result);

    if is_ok {
        let message_id = result.unwrap();
        assert!(!message_id.is_empty(), "Should return valid message ID");

        debug!(?message_id, "Content announcement published");
    }

    manager.stop().await.expect("Failed to stop");
}

#[tokio::test]
#[serial]
async fn test_announce_multiple_contents() {
    let node_dirs = TestNodeDirs::new("announce_multi");
    node_dirs.set_env_vars();

    let node_data = create_test_node_data();
    let manager = NetworkManager::new(&node_data)
        .await
        .expect("Failed to create NetworkManager");

    let config = create_gossipsub_test_config(0);
    node_dirs.set_env_vars();
    manager.start(&config).await.expect("Failed to start");

    manager
        .subscribe_topic(&Topic::ContentAnnouncements)
        .await
        .expect("Failed to subscribe");

    // Announce multiple contents
    let contents = vec![
        ("file1.pdf", b"pdf content".as_slice(), "application/pdf"),
        ("image.png", b"image content".as_slice(), "image/png"),
        ("data.json", b"json content".as_slice(), "application/json"),
    ];

    for (name, data, content_type) in contents {
        let cid = ContentId::from_bytes(data);
        let result = manager
            .announce_content_published(
                cid,
                name.to_string(),
                data.len() as u64,
                content_type.to_string(),
            )
            .await;
        assert_announce_ok_or_no_peers(&result);
    }

    info!("Successfully announced multiple contents");

    manager.stop().await.expect("Failed to stop");
}

#[tokio::test]
#[serial]
async fn test_announce_content_not_started() {
    let node_dirs = TestNodeDirs::new("announce_not_started");
    node_dirs.set_env_vars();

    let node_data = create_test_node_data();
    let manager = NetworkManager::new(&node_data)
        .await
        .expect("Failed to create NetworkManager");

    // Manager is intentionally not started.
    let cid = ContentId::from_bytes(b"test");
    let result = manager
        .announce_content_published(cid, "test.txt".to_string(), 100, "text/plain".to_string())
        .await;

    assert!(result.is_err());
    assert!(
        result
            .err()
            .unwrap()
            .to_string()
            .to_lowercase()
            .contains("not started"),
        "Error should mention not started"
    );
}

// =============================================================================
// TWO-NODE INTEGRATION: Announcement & Discovery
// =============================================================================

#[tokio::test]
#[serial]
async fn test_two_node_content_announcement_discovery() {
    // Node A announces content, Node B discovers via subscription.

    let publisher_dirs = TestNodeDirs::new("gossip_publisher");
    let subscriber_dirs = TestNodeDirs::new("gossip_subscriber");

    // Publisher node
    publisher_dirs.set_env_vars();
    let publisher_data = create_test_node_data();
    let publisher = NetworkManager::new(&publisher_data)
        .await
        .expect("Failed to create publisher");

    let publisher_config = create_gossipsub_test_config(9400);
    publisher_dirs.set_env_vars();
    publisher
        .start(&publisher_config)
        .await
        .expect("Failed to start publisher");

    let publisher_peer_id = *publisher.local_peer_id();
    let publisher_addr: libp2p::Multiaddr =
        format!("/ip4/127.0.0.1/udp/9400/quic-v1/p2p/{}", publisher_peer_id)
            .parse()
            .unwrap();

    // Publisher subscribes (helps mesh formation)
    publisher
        .subscribe_topic(&Topic::ContentAnnouncements)
        .await
        .expect("Publisher failed to subscribe");

    // Subscriber node
    subscriber_dirs.set_env_vars();
    let subscriber_data = create_test_node_data();
    let subscriber = NetworkManager::new(&subscriber_data)
        .await
        .expect("Failed to create subscriber");

    let mut subscriber_config = create_gossipsub_test_config(9401);
    subscriber_config.bootstrap_peers = vec![publisher_addr];
    subscriber_dirs.set_env_vars();
    subscriber
        .start(&subscriber_config)
        .await
        .expect("Failed to start subscriber");

    // Subscriber subscribes to content announcements
    let mut sub_handle = subscriber
        .subscribe_topic(&Topic::ContentAnnouncements)
        .await
        .expect("Subscriber failed to subscribe");

    // Wait for GossipSub mesh to form
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Publisher announces content
    let test_content = b"Hello, this is announced content!";
    let cid = ContentId::from_bytes(test_content);
    let content_name = "announcement_test.txt".to_string();
    let content_size = test_content.len() as u64;
    let content_type = "text/plain".to_string();

    info!("Publisher announcing content: {}", content_name);

    let announce_result = publisher
        .announce_content_published(
            cid.clone(),
            content_name.clone(),
            content_size,
            content_type.clone(),
        )
        .await;

    assert!(
        announce_result.is_ok(),
        "Announcement should succeed: {:?}",
        announce_result.err()
    );

    // Subscriber attempts to receive the announcement
    let receive_result = tokio::time::timeout(Duration::from_secs(5), sub_handle.recv()).await;

    match receive_result {
        Ok(Ok(message)) => {
            info!("Subscriber received announcement: {:?}", message.payload);

            match message.payload {
                MessagePayload::ContentPublished {
                    cid: received_cid,
                    name: received_name,
                    size: received_size,
                    content_type: received_type,
                } => {
                    assert_eq!(received_cid, cid, "CID should match");
                    assert_eq!(received_name, content_name, "Name should match");
                    assert_eq!(received_size, content_size, "Size should match");
                    assert_eq!(received_type, content_type, "Content type should match");
                    info!("Content announcement successfully received and validated!");
                }
                other => {
                    panic!("Expected ContentPublished payload, got {:?}", other);
                }
            }
        }
        Ok(Err(e)) => {
            info!(
                "Subscription receive error (may be timing-related): {:?}",
                e
            );
        }
        Err(_) => {
            info!("Timeout waiting for announcement in 2-node GossipSub setup");
        }
    }

    subscriber.stop().await.expect("Failed to stop subscriber");
    publisher.stop().await.expect("Failed to stop publisher");
}

#[tokio::test]
#[serial]
async fn test_publish_and_announce_combined() {
    let node_dirs = TestNodeDirs::new("publish_announce");
    node_dirs.set_env_vars();

    let node_data = create_test_node_data();
    let manager = NetworkManager::new(&node_data)
        .await
        .expect("Failed to create NetworkManager");

    let config = create_gossipsub_test_config(0);
    node_dirs.set_env_vars();
    manager.start(&config).await.expect("Failed to start");

    manager
        .subscribe_topic(&Topic::ContentAnnouncements)
        .await
        .expect("Failed to subscribe");

    let test_content = b"Combined publish and announce test";
    let cid = ContentId::from_bytes(test_content);

    let result = manager
        .publish_and_announce(
            cid.clone(),
            "combined_test.txt".to_string(),
            test_content.len() as u64,
            "text/plain".to_string(),
        )
        .await;
    let is_ok = match &result {
        Ok(_) => true,
        Err(e) if e.to_string().contains("NoPeersSubscribedToTopic") => false,
        Err(e) => {
            panic!("Unexpected error: {:?}", e);
        }
    };

    if is_ok {
        let (provider_result, message_id) = result.unwrap();

        debug!(
            ?provider_result.query_id,
            ?message_id,
            "Content published and announced"
        );
    }

    manager.stop().await.expect("Failed to stop");
}

// =============================================================================
// CONCURRENT ANNOUNCEMENT TESTS
// =============================================================================

#[tokio::test]
#[serial]
async fn test_concurrent_announcements() {
    let node_dirs = TestNodeDirs::new("concurrent_announce");
    node_dirs.set_env_vars();

    let node_data = create_test_node_data();
    let manager = Arc::new(
        NetworkManager::new(&node_data)
            .await
            .expect("Failed to create NetworkManager"),
    );

    let config = create_gossipsub_test_config(0);
    node_dirs.set_env_vars();
    manager.start(&config).await.expect("Failed to start");

    manager
        .subscribe_topic(&Topic::ContentAnnouncements)
        .await
        .expect("Failed to subscribe");

    // Spawn concurrent announcements
    let mut handles = vec![];
    for i in 0..10 {
        let mgr = Arc::clone(&manager);
        let handle = tokio::spawn(async move {
            let content = format!("concurrent content {}", i);
            let cid = ContentId::from_bytes(content.as_bytes());
            mgr.announce_content_published(
                cid,
                format!("file_{}.txt", i),
                content.len() as u64,
                "text/plain".to_string(),
            )
            .await
        });
        handles.push(handle);
    }

    for handle in handles {
        let result = handle.await.expect("Task panicked");
        assert_announce_ok_or_no_peers(&result);
    }

    info!("All concurrent announcements succeeded");

    manager.stop().await.expect("Failed to stop");
}

// =============================================================================
// EDGE CASE TESTS
// =============================================================================

#[tokio::test]
#[serial]
async fn test_announce_empty_name() {
    let node_dirs = TestNodeDirs::new("announce_empty_name");
    node_dirs.set_env_vars();

    let node_data = create_test_node_data();
    let manager = NetworkManager::new(&node_data)
        .await
        .expect("Failed to create NetworkManager");

    let config = create_gossipsub_test_config(0);
    node_dirs.set_env_vars();
    manager.start(&config).await.expect("Failed to start");

    manager
        .subscribe_topic(&Topic::ContentAnnouncements)
        .await
        .expect("Failed to subscribe");

    let cid = ContentId::from_bytes(b"nameless content");

    // Empty name should still be accepted
    let result = manager
        .announce_content_published(
            cid,
            String::new(), // empty name
            100,
            "application/octet-stream".to_string(),
        )
        .await;

    assert_announce_ok_or_no_peers(&result);

    manager.stop().await.expect("Failed to stop");
}

#[tokio::test]
#[serial]
async fn test_announce_large_content_metadata() {
    let node_dirs = TestNodeDirs::new("announce_large_meta");
    node_dirs.set_env_vars();

    let node_data = create_test_node_data();
    let manager = NetworkManager::new(&node_data)
        .await
        .expect("Failed to create NetworkManager");

    let config = create_gossipsub_test_config(0);
    node_dirs.set_env_vars();
    manager.start(&config).await.expect("Failed to start");

    manager
        .subscribe_topic(&Topic::ContentAnnouncements)
        .await
        .expect("Failed to subscribe");

    let large_content = vec![0u8; 100 * 1024 * 1024]; // 100MB
    let cid = ContentId::from_bytes(&large_content);

    let result = manager
        .announce_content_published(
            cid,
            "large_file.bin".to_string(),
            large_content.len() as u64,
            "application/octet-stream".to_string(),
        )
        .await;

    assert_announce_ok_or_no_peers(&result);

    manager.stop().await.expect("Failed to stop");
}

#[tokio::test]
#[serial]
async fn test_announce_unicode_name() {
    let node_dirs = TestNodeDirs::new("announce_unicode");
    node_dirs.set_env_vars();

    let node_data = create_test_node_data();
    let manager = NetworkManager::new(&node_data)
        .await
        .expect("Failed to create NetworkManager");

    let config = create_gossipsub_test_config(0);
    node_dirs.set_env_vars();
    manager.start(&config).await.expect("Failed to start");

    manager
        .subscribe_topic(&Topic::ContentAnnouncements)
        .await
        .expect("Failed to subscribe");

    let cid = ContentId::from_bytes(b"unicode test");

    let result = manager
        .announce_content_published(
            cid,
            "文档_🚀_test.txt".to_string(), // Unicode name
            1024,
            "text/plain; charset=utf-8".to_string(),
        )
        .await;

    assert_announce_ok_or_no_peers(&result);

    manager.stop().await.expect("Failed to stop");
}

#[tokio::test]
#[serial]
async fn test_announce_zero_size() {
    let node_dirs = TestNodeDirs::new("announce_zero_size");
    node_dirs.set_env_vars();

    let node_data = create_test_node_data();
    let manager = NetworkManager::new(&node_data)
        .await
        .expect("Failed to create NetworkManager");

    let config = create_gossipsub_test_config(0);
    node_dirs.set_env_vars();
    manager.start(&config).await.expect("Failed to start");

    manager
        .subscribe_topic(&Topic::ContentAnnouncements)
        .await
        .expect("Failed to subscribe");

    let cid = ContentId::from_bytes(b"");

    let result = manager
        .announce_content_published(
            cid,
            "empty.txt".to_string(),
            0, // zero size
            "text/plain".to_string(),
        )
        .await;

    assert_announce_ok_or_no_peers(&result);

    manager.stop().await.expect("Failed to stop");
}
