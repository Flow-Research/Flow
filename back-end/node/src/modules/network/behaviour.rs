use libp2p::PeerId;
use libp2p::kad::{self, store::MemoryStore};
use libp2p::swarm::NetworkBehaviour;

/// Combined network behaviour for Flow
///
/// Currently includes:
/// - Kademlia DHT for peer discovery and content routing
///
/// Future extensions will add:
/// - GossipSub for pub/sub messaging
/// - mDNS for local network discovery
/// - Request-Response for direct queries
#[derive(NetworkBehaviour)]
#[behaviour(to_swarm = "FlowBehaviourEvent")]
pub struct FlowBehaviour {
    /// Kademlia DHT for distributed peer discovery
    pub kademlia: kad::Behaviour<MemoryStore>,
}

/// Events emitted by the Flow behaviour
///
/// This enum will grow as we add more protocols
#[derive(Debug)]
pub enum FlowBehaviourEvent {
    Kademlia(kad::Event),
}

impl From<kad::Event> for FlowBehaviourEvent {
    fn from(event: kad::Event) -> Self {
        FlowBehaviourEvent::Kademlia(event)
    }
}

impl FlowBehaviour {
    /// Create a new FlowBehaviour with Kademlia DHT
    ///
    /// # Arguments
    /// * `local_peer_id` - The PeerId of this node
    ///
    /// # Returns
    /// A configured FlowBehaviour ready for use in a Swarm
    pub fn new(local_peer_id: PeerId) -> Self {
        // Create Kademlia configuration
        let kad_config = kad::Config::new(libp2p::StreamProtocol::new("/flow/kad/1.0.0"));

        // Create Kademlia behaviour with in-memory store
        let store = MemoryStore::new(local_peer_id);
        let kademlia = kad::Behaviour::with_config(local_peer_id, store, kad_config);

        Self { kademlia }
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use libp2p::identity::Keypair;

    fn generate_peer_id() -> PeerId {
        Keypair::generate_ed25519().public().to_peer_id()
    }

    #[test]
    fn test_behaviour_creation() {
        let peer_id = generate_peer_id();
        let mut behaviour = FlowBehaviour::new(peer_id);

        // Verify Kademlia is initialized with empty routing table
        assert_eq!(
            behaviour.kademlia.kbuckets().count(),
            0,
            "Routing table should be empty on initialization"
        );
    }

    #[test]
    fn test_bootstrap_fails_without_known_peers() {
        let peer_id = generate_peer_id();
        let mut behaviour = FlowBehaviour::new(peer_id);

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
        let local_peer_id = generate_peer_id();
        let mut behaviour = FlowBehaviour::new(local_peer_id);

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
        let local_peer_id = generate_peer_id();
        let mut behaviour = FlowBehaviour::new(local_peer_id);

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
        let local_peer_id = generate_peer_id();
        let mut behaviour = FlowBehaviour::new(local_peer_id);

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
        let local_peer_id = generate_peer_id();
        let mut behaviour = FlowBehaviour::new(local_peer_id);

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
        let local_peer_id = generate_peer_id();
        let mut behaviour = FlowBehaviour::new(local_peer_id);

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
    fn test_multiaddr_parsing_validation() {
        let local_peer_id = generate_peer_id();
        let mut behaviour = FlowBehaviour::new(local_peer_id);
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
    fn test_behaviour_implements_network_behaviour() {
        // Compile-time verification that FlowBehaviour implements NetworkBehaviour
        // This ensures our derive macro worked correctly
        fn _assert_network_behaviour<T: NetworkBehaviour>() {}
        _assert_network_behaviour::<FlowBehaviour>();
    }
}
