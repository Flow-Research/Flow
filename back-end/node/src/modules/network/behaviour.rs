use crate::modules::network::config::MdnsConfig;
use crate::modules::network::content_transfer::{
    ContentCodec, ContentProtocol, ContentRequest, ContentResponse, ContentTransferConfig,
};
use crate::modules::network::gossipsub::GossipSubConfig;
use crate::modules::network::storage::RocksDbStore;
use libp2p::identity::Keypair;
use libp2p::kad::RecordKey;
use libp2p::request_response::{self, ProtocolSupport};
use libp2p::swarm::NetworkBehaviour;
use libp2p::swarm::behaviour::toggle::Toggle;
use libp2p::{PeerId, gossipsub, kad, mdns};
use std::num::NonZeroUsize;
use std::time::Duration;
use tracing::{debug, info, warn};

/// Combined network behaviour for Flow
///
/// Currently includes:
/// - Kademlia DHT for peer discovery and content routing
/// - mDNS for local network discovery
/// - GossipSub for pub/sub messaging
/// - Request-Response for direct request response between nodes
#[derive(NetworkBehaviour)]
#[behaviour(to_swarm = "FlowBehaviourEvent")]
pub struct FlowBehaviour {
    /// Kademlia DHT for distributed peer discovery
    pub kademlia: kad::Behaviour<RocksDbStore>,

    /// mDNS for local network discovery
    pub mdns: Toggle<mdns::tokio::Behaviour>,

    /// GossipSub for pub/sub messaging
    pub gossipsub: Toggle<gossipsub::Behaviour>,

    /// Request-Response for content transfer
    pub content_transfer: Toggle<request_response::Behaviour<ContentCodec>>,
}

/// Events emitted by the Flow behaviour
///
/// This enum will grow as we add more protocols
#[derive(Debug)]
pub enum FlowBehaviourEvent {
    Kademlia(kad::Event),
    Mdns(mdns::Event),
    Gossipsub(gossipsub::Event),
    ContentTransfer(request_response::Event<ContentRequest, ContentResponse>),
}

impl From<kad::Event> for FlowBehaviourEvent {
    fn from(event: kad::Event) -> Self {
        FlowBehaviourEvent::Kademlia(event)
    }
}

impl From<mdns::Event> for FlowBehaviourEvent {
    fn from(event: mdns::Event) -> Self {
        FlowBehaviourEvent::Mdns(event)
    }
}

impl From<gossipsub::Event> for FlowBehaviourEvent {
    fn from(event: gossipsub::Event) -> Self {
        FlowBehaviourEvent::Gossipsub(event)
    }
}

impl From<request_response::Event<ContentRequest, ContentResponse>> for FlowBehaviourEvent {
    fn from(event: request_response::Event<ContentRequest, ContentResponse>) -> Self {
        FlowBehaviourEvent::ContentTransfer(event)
    }
}

impl FlowBehaviour {
    /// Create a new FlowBehaviour with all protocols
    ///
    /// # Arguments
    /// * `local_peer_id` - The PeerId of this node
    /// * `keypair` - The node's keypair (needed for GossipSub signing)
    /// * `store` - Persistent RocksDB store for DHT records
    /// * `mdns_config` - mDNS configuration (None to disable)
    /// * `gossipsub_config` - GossipSub configuration (None to disable)
    pub fn new(
        local_peer_id: PeerId,
        keypair: &Keypair,
        store: RocksDbStore,
        mdns_config: Option<&MdnsConfig>,
        gossipsub_config: Option<&GossipSubConfig>,
        content_transfer_config: Option<&ContentTransferConfig>,
    ) -> Self {
        Self {
            kademlia: Self::create_kademlia(local_peer_id, store),
            mdns: Self::create_mdns(local_peer_id, mdns_config),
            gossipsub: Self::create_gossipsub(keypair, gossipsub_config),
            content_transfer: Self::create_content_transfer(content_transfer_config),
        }
    }

    /// Create Kademlia DHT behaviour
    fn create_kademlia(local_peer_id: PeerId, store: RocksDbStore) -> kad::Behaviour<RocksDbStore> {
        let mut config = kad::Config::new(libp2p::StreamProtocol::new("/flow/kad/1.0.0"));
        config.set_kbucket_size(NonZeroUsize::new(1).unwrap());
        kad::Behaviour::with_config(local_peer_id, store, config)
    }

    /// Create mDNS behaviour if enabled
    fn create_mdns(
        local_peer_id: PeerId,
        config: Option<&MdnsConfig>,
    ) -> Toggle<mdns::tokio::Behaviour> {
        let Some(config) = config else {
            debug!("mDNS disabled (no config)");
            return Toggle::from(None);
        };

        if !config.enabled {
            debug!("mDNS disabled");
            return Toggle::from(None);
        }

        match mdns::tokio::Behaviour::new(
            mdns::Config {
                ttl: Duration::from_secs(6 * 60),
                query_interval: Duration::from_secs(config.query_interval_secs),
                enable_ipv6: false,
            },
            local_peer_id,
        ) {
            Ok(behaviour) => {
                info!(
                    query_interval_secs = config.query_interval_secs,
                    "mDNS enabled for local network discovery"
                );
                Toggle::from(Some(behaviour))
            }
            Err(e) => {
                warn!(
                    "Failed to create mDNS behaviour: {}. Continuing without mDNS.",
                    e
                );
                Toggle::from(None)
            }
        }
    }

    /// Create GossipSub behaviour if configured
    fn create_gossipsub(
        keypair: &Keypair,
        config: Option<&GossipSubConfig>,
    ) -> Toggle<gossipsub::Behaviour> {
        let Some(config) = config else {
            debug!("GossipSub disabled (no config)");
            return Toggle::from(None);
        };

        if !config.enabled {
            debug!("GossipSub disabled");
            return Toggle::from(None);
        }

        match config.build_behaviour(keypair) {
            Ok(behaviour) => {
                info!(
                    mesh_n = config.mesh_n,
                    validate_signatures = config.validate_signatures,
                    "GossipSub enabled"
                );
                Toggle::from(Some(behaviour))
            }
            Err(e) => {
                warn!(
                    "Failed to create GossipSub behaviour: {:?}. Continuing without GossipSub.",
                    e
                );
                Toggle::from(None)
            }
        }
    }

    /// Create content transfer request-response behaviour if enabled
    fn create_content_transfer(
        config: Option<&ContentTransferConfig>,
    ) -> Toggle<request_response::Behaviour<ContentCodec>> {
        let Some(config) = config else {
            debug!("Content transfer disabled (no config)");
            return Toggle::from(None);
        };

        if !config.enabled {
            debug!("Content transfer disabled");
            return Toggle::from(None);
        }

        let protocol = [(ContentProtocol::as_stream_protocol(), ProtocolSupport::Full)];

        let req_resp_config =
            request_response::Config::default().with_request_timeout(config.request_timeout());

        let behaviour = request_response::Behaviour::new(protocol, req_resp_config);

        info!(
            timeout_secs = config.request_timeout_secs,
            max_concurrent = config.max_concurrent_requests,
            "Content transfer enabled"
        );

        Toggle::from(Some(behaviour))
    }

    /// Add a bootstrap peer to the Kademlia routing table
    ///
    /// This should be called for each known bootstrap node before starting
    /// the Swarm event loop.
    pub fn add_bootstrap_peer(&mut self, peer_id: PeerId, addr: libp2p::Multiaddr) {
        self.kademlia.add_address(&peer_id, addr);
    }

    /// Initiate bootstrap process
    ///
    /// This queries the DHT to populate the routing table with peers
    pub fn bootstrap(&mut self) -> Result<kad::QueryId, kad::NoKnownPeers> {
        self.kademlia.bootstrap()
    }

    /// Subscribe to a GossipSub topic
    ///
    /// Returns true if subscription succeeded, false if GossipSub is disabled
    pub fn subscribe(
        &mut self,
        topic: &gossipsub::IdentTopic,
    ) -> Result<bool, gossipsub::SubscriptionError> {
        if let Some(gs) = self.gossipsub.as_mut() {
            gs.subscribe(topic)?;
            info!(topic = %topic, "Subscribed to topic");
            Ok(true)
        } else {
            warn!("Cannot subscribe: GossipSub is disabled");
            Ok(false)
        }
    }

    /// Unsubscribe from a GossipSub topic
    pub fn unsubscribe(&mut self, topic: &gossipsub::IdentTopic) -> bool {
        if let Some(gs) = self.gossipsub.as_mut() {
            let success = gs.unsubscribe(topic);
            if success {
                info!(topic = %topic, "Unsubscribed from topic");
            } else {
                debug!(topic = %topic, "Was not subscribed to topic");
            }
            success
        } else {
            false
        }
    }

    /// Publish a message to a GossipSub topic
    pub fn publish(
        &mut self,
        topic: gossipsub::IdentTopic,
        data: Vec<u8>,
    ) -> Result<gossipsub::MessageId, gossipsub::PublishError> {
        if let Some(gs) = self.gossipsub.as_mut() {
            let msg_id = gs.publish(topic.clone(), data)?;
            debug!(topic = %topic, message_id = %msg_id, "Published message");
            Ok(msg_id)
        } else {
            Err(gossipsub::PublishError::NoPeersSubscribedToTopic)
        }
    }

    /// Get list of peers subscribed to a topic
    pub fn mesh_peers(&self, topic: &gossipsub::TopicHash) -> Vec<PeerId> {
        if let Some(gs) = self.gossipsub.as_ref() {
            gs.mesh_peers(topic).cloned().collect()
        } else {
            Vec::new()
        }
    }

    /// Get all subscribed topics
    pub fn subscribed_topics(&self) -> Vec<gossipsub::TopicHash> {
        if let Some(gs) = self.gossipsub.as_ref() {
            gs.topics().cloned().collect()
        } else {
            Vec::new()
        }
    }

    /// Add peer to GossipSub mesh
    pub fn add_explicit_peer(&mut self, peer_id: &PeerId) {
        if let Some(gs) = self.gossipsub.as_mut() {
            gs.add_explicit_peer(peer_id);
        }
    }

    /// Check if GossipSub is enabled
    pub fn is_gossipsub_enabled(&self) -> bool {
        self.gossipsub.is_enabled()
    }

    /// Send a content request to a peer
    pub fn send_content_request(
        &mut self,
        peer_id: &PeerId,
        request: ContentRequest,
    ) -> Option<request_response::OutboundRequestId> {
        if let Some(ct) = self.content_transfer.as_mut() {
            let request_id = ct.send_request(peer_id, request);
            debug!(peer = %peer_id, request_id = ?request_id, "Sent content request");
            Some(request_id)
        } else {
            warn!("Content transfer is disabled, cannot send request");
            None
        }
    }

    /// Send a content response
    pub fn send_content_response(
        &mut self,
        channel: request_response::ResponseChannel<ContentResponse>,
        response: ContentResponse,
    ) -> Result<(), ContentResponse> {
        if let Some(ct) = self.content_transfer.as_mut() {
            ct.send_response(channel, response)
        } else {
            Err(response)
        }
    }

    /// Check if content transfer is enabled
    pub fn is_content_transfer_enabled(&self) -> bool {
        self.content_transfer.is_enabled()
    }

    /// Start providing content in the DHT.
    ///
    /// Announces this node as a provider for the given key.
    /// Other peers can then use `get_providers` to discover us.
    pub fn start_providing(&mut self, key: RecordKey) -> Result<kad::QueryId, kad::store::Error> {
        self.kademlia.start_providing(key)
    }

    /// Stop providing content.
    ///
    /// Removes the local provider record for the given key.
    /// Note: remote records expire based on their TTL.
    pub fn stop_providing(&mut self, key: &RecordKey) {
        self.kademlia.stop_providing(key);
    }

    /// Get providers for a given key from the DHT.
    ///
    /// Queries the DHT for peers that have announced themselves as providers
    /// for the given content.
    pub fn get_providers(&mut self, key: RecordKey) -> kad::QueryId {
        self.kademlia.get_providers(key)
    }
}

#[cfg(test)]
mod tests {
    use crate::modules::network::storage::StorageConfig;

    use super::*;
    use libp2p::identity::Keypair;
    use tempfile::TempDir;

    fn generate_keypair() -> Keypair {
        Keypair::generate_ed25519()
    }

    fn generate_peer_id() -> PeerId {
        generate_keypair().public().to_peer_id()
    }

    // Helper function to create a temporary RocksDB store for testing
    fn create_test_store(peer_id: PeerId) -> (RocksDbStore, TempDir) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config = StorageConfig {
            db_path: temp_dir.path().to_path_buf(),
            max_records: 1000,
            max_providers_per_key: 20,
            max_value_bytes: 65536,
            enable_compression: false, // Disable for faster tests
        };

        let store = RocksDbStore::new(temp_dir.path(), peer_id, config)
            .expect("Failed to create test store");

        (store, temp_dir)
    }

    #[test]
    fn test_behaviour_creation() {
        let keypair = generate_keypair();
        let peer_id = keypair.public().to_peer_id();
        let gs_config = GossipSubConfig::default();
        let content_transfer_config = ContentTransferConfig::default();
        let (store, _temp_dir) = create_test_store(peer_id);

        let mut behaviour = FlowBehaviour::new(
            peer_id,
            &keypair,
            store,
            None,
            Some(&gs_config),
            Some(&content_transfer_config),
        );

        // Verify Kademlia is initialized with empty routing table
        assert_eq!(
            behaviour.kademlia.kbuckets().count(),
            0,
            "Routing table should be empty on initialization"
        );
    }

    #[test]
    fn test_bootstrap_fails_without_known_peers() {
        let keypair = generate_keypair();
        let peer_id = keypair.public().to_peer_id();
        let gs_config = GossipSubConfig::default();
        let content_transfer_config = ContentTransferConfig::default();
        let (store, _temp_dir) = create_test_store(peer_id);

        let mut behaviour = FlowBehaviour::new(
            peer_id,
            &keypair,
            store,
            None,
            Some(&gs_config),
            Some(&content_transfer_config),
        );

        // Attempt to bootstrap without any known peers
        let result = behaviour.bootstrap();

        assert!(
            result.is_err(),
            "Bootstrap should fail when no peers are known"
        );

        // Verify it's the specific error we expect
        assert!(
            matches!(result.unwrap_err(), kad::NoKnownPeers()),
            "Should return NoKnownPeers error"
        );
    }

    #[test]
    fn test_bootstrap_succeeds_with_known_peers() {
        let keypair = generate_keypair();
        let peer_id = keypair.public().to_peer_id();
        let gs_config = GossipSubConfig::default();
        let content_transfer_config = ContentTransferConfig::default();
        let (store, _temp_dir) = create_test_store(peer_id);

        let mut behaviour = FlowBehaviour::new(
            peer_id,
            &keypair,
            store,
            None,
            Some(&gs_config),
            Some(&content_transfer_config),
        );

        // Add a bootstrap peer
        let bootstrap_peer = generate_peer_id();
        let addr: libp2p::Multiaddr = "/ip4/127.0.0.1/tcp/4001".parse().unwrap();
        behaviour.add_bootstrap_peer(bootstrap_peer, addr);

        // Now bootstrap should succeed
        let result = behaviour.bootstrap();

        assert!(
            result.is_ok(),
            "Bootstrap should succeed when peers are known"
        );

        // Verify we got a QueryId back
        let _query_id = result.unwrap();
        // Note: We can't easily test the query execution without a full Swarm,
        // but we've verified the bootstrap process initiated successfully
    }

    #[test]
    fn test_add_single_bootstrap_peer() {
        let keypair = generate_keypair();
        let peer_id = keypair.public().to_peer_id();
        let gs_config = GossipSubConfig::default();
        let content_transfer_config = ContentTransferConfig::default();
        let (store, _temp_dir) = create_test_store(peer_id);

        let mut behaviour = FlowBehaviour::new(
            peer_id,
            &keypair,
            store,
            None,
            Some(&gs_config),
            Some(&content_transfer_config),
        );

        let bootstrap_peer = generate_peer_id();
        let addr: libp2p::Multiaddr = "/ip4/192.168.1.100/tcp/4001".parse().unwrap();

        // Initially, routing table should be empty
        assert_eq!(behaviour.kademlia.kbuckets().count(), 0);

        behaviour.add_bootstrap_peer(bootstrap_peer, addr.clone());

        // After adding, we can verify the address was stored by checking
        // if bootstrap now succeeds (it requires at least one known peer)
        let bootstrap_result = behaviour.bootstrap();
        assert!(
            bootstrap_result.is_ok(),
            "Bootstrap should work after adding peer"
        );
    }

    #[test]
    fn test_add_multiple_bootstrap_peers() {
        let keypair = generate_keypair();
        let peer_id = keypair.public().to_peer_id();
        let gs_config = GossipSubConfig::default();
        let content_transfer_config = ContentTransferConfig::default();
        let (store, _temp_dir) = create_test_store(peer_id);

        let mut behaviour = FlowBehaviour::new(
            peer_id,
            &keypair,
            store,
            None,
            Some(&gs_config),
            Some(&content_transfer_config),
        );

        // Add multiple bootstrap peers
        let peer1 = generate_peer_id();
        let peer2 = generate_peer_id();
        let peer3 = generate_peer_id();

        behaviour.add_bootstrap_peer(peer1, "/ip4/10.0.0.1/tcp/4001".parse().unwrap());
        behaviour.add_bootstrap_peer(peer2, "/ip4/10.0.0.2/tcp/4001".parse().unwrap());
        behaviour.add_bootstrap_peer(peer3, "/ip4/10.0.0.3/tcp/4001".parse().unwrap());

        // Bootstrap should succeed with multiple peers
        assert!(behaviour.bootstrap().is_ok());
    }

    #[test]
    fn test_add_multiple_addresses_for_same_peer() {
        let keypair = generate_keypair();
        let peer_id = keypair.public().to_peer_id();
        let gs_config = GossipSubConfig::default();
        let content_transfer_config = ContentTransferConfig::default();
        let (store, _temp_dir) = create_test_store(peer_id);

        let mut behaviour = FlowBehaviour::new(
            peer_id,
            &keypair,
            store,
            None,
            Some(&gs_config),
            Some(&content_transfer_config),
        );

        let bootstrap_peer = generate_peer_id();

        // Add multiple addresses for the same peer
        let addr1: libp2p::Multiaddr = "/ip4/127.0.0.1/tcp/4001".parse().unwrap();
        let addr2: libp2p::Multiaddr = "/ip6/::1/tcp/4001".parse().unwrap();

        behaviour.add_bootstrap_peer(bootstrap_peer, addr1);
        behaviour.add_bootstrap_peer(bootstrap_peer, addr2);

        // Should still be able to bootstrap
        assert!(
            behaviour.bootstrap().is_ok(),
            "Bootstrap should work with multiple addresses for same peer"
        );
    }

    #[test]
    fn test_bootstrap_query_id_uniqueness() {
        let keypair = generate_keypair();
        let peer_id = keypair.public().to_peer_id();
        let gs_config = GossipSubConfig::default();
        let content_transfer_config = ContentTransferConfig::default();
        let (store, _temp_dir) = create_test_store(peer_id);

        let mut behaviour = FlowBehaviour::new(
            peer_id,
            &keypair,
            store,
            None,
            Some(&gs_config),
            Some(&content_transfer_config),
        );

        // Add a peer
        let peer = generate_peer_id();
        behaviour.add_bootstrap_peer(peer, "/ip4/127.0.0.1/tcp/4001".parse().unwrap());

        // Bootstrap twice and verify we get different QueryIds
        let query1 = behaviour.bootstrap().unwrap();
        let query2 = behaviour.bootstrap().unwrap();

        assert_ne!(
            query1, query2,
            "Each bootstrap call should return a unique QueryId"
        );
    }

    #[test]
    fn test_behaviour_with_gossipsub() {
        let keypair = generate_keypair();
        let peer_id = keypair.public().to_peer_id();
        let (store, _temp_dir) = create_test_store(peer_id);
        let content_transfer_config = ContentTransferConfig::default();
        let gs_config = GossipSubConfig::default();

        let behaviour = FlowBehaviour::new(
            peer_id,
            &keypair,
            store,
            None,
            Some(&gs_config),
            Some(&content_transfer_config),
        );

        assert!(behaviour.is_gossipsub_enabled());
    }

    #[test]
    fn test_behaviour_without_gossipsub() {
        let keypair = generate_keypair();
        let peer_id = keypair.public().to_peer_id();
        let (store, _temp_dir) = create_test_store(peer_id);

        let behaviour = FlowBehaviour::new(peer_id, &keypair, store, None, None, None);

        assert!(!behaviour.is_gossipsub_enabled());
    }

    #[test]
    fn test_topic_subscription() {
        let keypair = generate_keypair();
        let peer_id = keypair.public().to_peer_id();
        let (store, _temp_dir) = create_test_store(peer_id);
        let gs_config = GossipSubConfig::default();
        let content_transfer_config = ContentTransferConfig::default();

        let mut behaviour = FlowBehaviour::new(
            peer_id,
            &keypair,
            store,
            None,
            Some(&gs_config),
            Some(&content_transfer_config),
        );

        let topic = gossipsub::IdentTopic::new("/flow/v1/test");
        let result = behaviour.subscribe(&topic);
        assert!(result.is_ok());
        assert!(result.unwrap());

        let topics = behaviour.subscribed_topics();
        assert!(topics.contains(&topic.hash()));
    }

    #[test]
    fn test_multiaddr_parsing_validation() {
        let keypair = generate_keypair();
        let peer_id = keypair.public().to_peer_id();
        let gs_config = GossipSubConfig::default();
        let content_transfer_config = ContentTransferConfig::default();
        let (store, _temp_dir) = create_test_store(peer_id);

        let mut behaviour = FlowBehaviour::new(
            peer_id,
            &keypair,
            store,
            None,
            Some(&gs_config),
            Some(&content_transfer_config),
        );
        let peer = generate_peer_id();

        // Test various valid multiaddr formats
        let valid_addrs = vec![
            "/ip4/127.0.0.1/tcp/4001",
            "/ip4/192.168.1.1/tcp/8080",
            "/ip6/::1/tcp/4001",
            "/ip4/8.8.8.8/udp/4001/quic-v1",
        ];

        for addr_str in valid_addrs {
            let addr: libp2p::Multiaddr = addr_str
                .parse()
                .expect(&format!("Should parse valid multiaddr: {}", addr_str));
            behaviour.add_bootstrap_peer(peer, addr);
        }

        // Should be able to bootstrap after adding valid addresses
        assert!(behaviour.bootstrap().is_ok());
    }

    #[test]
    fn test_event_conversion_from_kad_event() {
        // Test that we can convert Kademlia events to FlowBehaviourEvent
        // This verifies the From implementation works correctly

        // We can't easily create a real kad::Event without a full Swarm,
        // but we can verify the type system is set up correctly

        // This is more of a compile-time test, but it ensures
        // our event handling structure is correct
        let _assert_event_convertible = |event: kad::Event| -> FlowBehaviourEvent { event.into() };
    }

    #[test]
    fn test_event_conversion_from_mdns_event() {
        // Compile-time verification that mdns::Event can be converted
        let _assert_event_convertible = |event: mdns::Event| -> FlowBehaviourEvent { event.into() };
    }

    #[test]
    fn test_behaviour_implements_network_behaviour() {
        // Compile-time verification that FlowBehaviour implements NetworkBehaviour
        // This ensures our derive macro worked correctly
        fn _assert_network_behaviour<T: NetworkBehaviour>() {}
        _assert_network_behaviour::<FlowBehaviour>();
    }
}
