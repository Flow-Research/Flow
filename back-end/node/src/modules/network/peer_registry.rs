use libp2p::{Multiaddr, PeerId};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::time::Instant;
use std::{collections::HashMap, fmt};
use tracing::{debug, info, warn};

/// Discovery source for tracking how peer was found
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DiscoverySource {
    /// Discovered via mDNS on local network
    Mdns,
    /// Discovered via Kademlia DHT
    Dht,
    /// Configured as bootstrap peer
    Bootstrap,
    /// Manually dialed by user/API
    Manual,
    /// Unknown or not tracked (for backwards compatibility)
    #[default]
    Unknown,
}

impl DiscoverySource {
    /// Returns a stable string representation for API responses.
    ///
    /// These strings are part of the public API contract and should not change.
    pub const fn as_str(&self) -> &'static str {
        match self {
            DiscoverySource::Mdns => "mdns",
            DiscoverySource::Dht => "dht",
            DiscoverySource::Bootstrap => "bootstrap",
            DiscoverySource::Manual => "manual",
            DiscoverySource::Unknown => "unknown",
        }
    }

    /// Parse a string back to DiscoverySource.
    ///
    /// Accepts both lowercase (API format) and capitalized (serde format).
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "mdns" => Some(DiscoverySource::Mdns),
            "dht" => Some(DiscoverySource::Dht),
            "bootstrap" => Some(DiscoverySource::Bootstrap),
            "manual" => Some(DiscoverySource::Manual),
            "unknown" => Some(DiscoverySource::Unknown),
            _ => None,
        }
    }
}

impl fmt::Display for DiscoverySource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Information about a connected peer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    /// Peer ID
    #[serde(
        serialize_with = "serialize_peer_id",
        deserialize_with = "deserialize_peer_id"
    )]
    pub peer_id: PeerId,

    /// Known addresses for this peer
    #[serde(
        serialize_with = "serialize_multiaddrs",
        deserialize_with = "deserialize_multiaddrs"
    )]
    pub addresses: Vec<Multiaddr>,

    /// When we first connected to this peer
    #[serde(skip, default = "current_instant")]
    pub first_seen: Instant,

    /// When we last saw activity from this peer
    #[serde(skip, default = "current_instant")]
    pub last_seen: Instant,

    /// Number of active connections to this peer
    pub connection_count: usize,

    /// Connection statistics
    pub stats: ConnectionStats,

    /// Whether this is an outbound (dialed) or inbound connection
    pub connection_direction: ConnectionDirection,

    /// How this peer was discovered
    pub discovery_source: DiscoverySource,
}

// Helper serializers for libp2p types
fn serialize_peer_id<S>(peer_id: &PeerId, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&peer_id.to_string())
}

fn deserialize_peer_id<'de, D>(deserializer: D) -> Result<PeerId, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    s.parse::<PeerId>()
        .map_err(|e| serde::de::Error::custom(format!("Invalid PeerId: {}", e)))
}

fn serialize_multiaddrs<S>(addrs: &[Multiaddr], serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    use serde::ser::SerializeSeq;
    let mut seq = serializer.serialize_seq(Some(addrs.len()))?;
    for addr in addrs {
        seq.serialize_element(&addr.to_string())?;
    }
    seq.end()
}

fn deserialize_multiaddrs<'de, D>(deserializer: D) -> Result<Vec<Multiaddr>, D::Error>
where
    D: Deserializer<'de>,
{
    let strings = Vec::<String>::deserialize(deserializer)?;
    strings
        .into_iter()
        .map(|s| {
            s.parse::<Multiaddr>()
                .map_err(|e| serde::de::Error::custom(format!("Invalid Multiaddr: {}", e)))
        })
        .collect()
}

fn current_instant() -> Instant {
    Instant::now()
}

/// Statistics about a peer connection
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConnectionStats {
    /// Total bytes sent to this peer
    pub bytes_sent: u64,

    /// Total bytes received from this peer
    pub bytes_received: u64,

    /// Number of times we've reconnected
    pub reconnection_count: u32,

    /// Average latency (if measured)
    pub avg_latency_ms: Option<f64>,
}

/// Direction of a connection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionDirection {
    Inbound,
    Outbound,
}

impl ConnectionDirection {
    /// Returns a stable string representation for API responses.
    ///
    /// These strings are part of the public API contract and should not change.
    pub const fn as_str(&self) -> &'static str {
        match self {
            ConnectionDirection::Inbound => "inbound",
            ConnectionDirection::Outbound => "outbound",
        }
    }

    /// Parse a string back to ConnectionDirection.
    ///
    /// Accepts both lowercase (API format) and capitalized (serde format).
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "inbound" => Some(ConnectionDirection::Inbound),
            "outbound" => Some(ConnectionDirection::Outbound),
            _ => None,
        }
    }
}

impl fmt::Display for ConnectionDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Overall network statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkStats {
    /// Total peers connected
    pub connected_peers: usize,

    /// Total known peers (including disconnected)
    pub known_peers: usize,

    /// Uptime since network started (in seconds)
    pub uptime_secs: u64,

    /// Total connections established
    pub total_connections: u64,

    /// Total connection failures
    pub total_connection_failures: u64,
}

/// Registry for tracking peer lifecycle and connections
#[derive(Debug)]
pub struct PeerRegistry {
    /// Currently connected peers
    peers: HashMap<PeerId, PeerInfo>,

    /// Peers we've seen but aren't currently connected to
    known_peers: HashMap<PeerId, Vec<Multiaddr>>,

    /// Track reconnection counts across disconnections
    reconnection_counts: HashMap<PeerId, u32>,

    /// When the registry was created
    started_at: Instant,

    /// Total connections established since start
    total_connections: u64,

    /// Total connection failures
    total_failures: u64,
}

impl PeerRegistry {
    pub fn new() -> Self {
        Self {
            peers: HashMap::new(),
            known_peers: HashMap::new(),
            reconnection_counts: HashMap::new(),
            started_at: Instant::now(),
            total_connections: 0,
            total_failures: 0,
        }
    }

    /// Bulk load known peers from persistent storage
    ///
    /// This is used during initialization to restore historical peer data
    /// without going through the connection lifecycle.
    ///
    /// # Arguments
    /// * `peers` - Map of PeerID to their known addresses
    pub fn bulk_load_known_peers(&mut self, peers: HashMap<PeerId, Vec<Multiaddr>>) {
        let count = peers.len();

        debug!(peer_count = count, "Bulk loading known peers into registry");

        for (peer_id, addresses) in peers {
            if addresses.is_empty() {
                warn!(
                    peer_id = %peer_id,
                    "Skipping peer with no addresses during bulk load"
                );
                continue;
            }

            // Directly insert into known_peers, merging with any existing data
            self.known_peers
                .entry(peer_id)
                .and_modify(|existing_addrs| {
                    // Merge addresses, avoiding duplicates
                    for addr in &addresses {
                        if !existing_addrs.contains(addr) {
                            existing_addrs.push(addr.clone());
                        }
                    }
                })
                .or_insert(addresses);
        }

        info!(
            loaded_peers = count,
            total_known_peers = self.known_peers.len(),
            "Bulk loaded known peers into registry"
        );
    }

    /// Bulk load reconnection counts from persistent storage
    ///
    /// This restores the reconnection statistics without going through
    /// the connection lifecycle.
    ///
    /// # Arguments
    /// * `counts` - Map of PeerID to their reconnection count
    pub fn bulk_load_reconnection_counts(&mut self, counts: HashMap<PeerId, u32>) {
        let count = counts.len();

        debug!(
            count = count,
            "Bulk loading reconnection counts into registry"
        );

        for (peer_id, reconnection_count) in counts {
            if reconnection_count == 0 {
                // Skip zero counts to keep HashMap smaller
                continue;
            }

            // Directly insert, taking the maximum if there's already a value
            // (shouldn't happen in normal operation)
            self.reconnection_counts
                .entry(peer_id)
                .and_modify(|existing| {
                    if reconnection_count > *existing {
                        warn!(
                            peer_id = %peer_id,
                            existing = *existing,
                            loaded = reconnection_count,
                            "Reconnection count conflict during bulk load, using maximum"
                        );
                        *existing = reconnection_count;
                    }
                })
                .or_insert(reconnection_count);
        }

        info!(
            loaded_counts = count,
            total_tracked = self.reconnection_counts.len(),
            "Bulk loaded reconnection counts into registry"
        );
    }

    /// Record a new connection
    pub fn on_connection_established(
        &mut self,
        peer_id: PeerId,
        address: Multiaddr,
        direction: ConnectionDirection,
        discovery_source: DiscoverySource,
    ) {
        let now = Instant::now();
        self.total_connections += 1;

        // Check if this is a reconnection
        let is_reconnection =
            !self.peers.contains_key(&peer_id) && self.known_peers.contains_key(&peer_id);

        self.peers
            .entry(peer_id)
            .and_modify(|info| {
                info.connection_count += 1;
                info.last_seen = now;
                info.stats.reconnection_count += 1;
                // Also update the persistent counter
                *self.reconnection_counts.entry(peer_id).or_insert(0) += 1;
                if !info.addresses.contains(&address) {
                    info.addresses.push(address.clone());
                }
            })
            .or_insert_with(|| {
                let mut stats = ConnectionStats::default();

                // If it's a reconnection, increment the persistent counter
                if is_reconnection {
                    let count = self.reconnection_counts.entry(peer_id).or_insert(0);
                    *count += 1;
                    stats.reconnection_count = *count;
                }

                PeerInfo {
                    peer_id,
                    addresses: vec![address.clone()],
                    first_seen: now,
                    last_seen: now,
                    connection_count: 1,
                    stats,
                    connection_direction: direction,
                    discovery_source,
                }
            });

        // Also track in known peers
        self.known_peers
            .entry(peer_id)
            .and_modify(|addrs| {
                if !addrs.contains(&address) {
                    addrs.push(address.clone());
                }
            })
            .or_insert_with(|| vec![address]);
    }

    /// Record a connection closure
    pub fn on_connection_closed(&mut self, peer_id: &PeerId) {
        if let Some(info) = self.peers.get_mut(peer_id) {
            info.connection_count = info.connection_count.saturating_sub(1);

            // If no more connections, remove from active peers
            // But keep reconnection count for future reconnections
            if info.connection_count == 0 {
                if let Some(removed_info) = self.peers.remove(peer_id) {
                    // Keep addresses in known_peers for potential reconnection
                    self.known_peers.insert(*peer_id, removed_info.addresses);
                    // Reconnection count is preserved in reconnection_counts HashMap
                }
            }
        }
    }

    /// Record a connection failure
    pub fn on_connection_failed(&mut self, _peer_id: Option<&PeerId>) {
        self.total_failures += 1;
    }

    /// Get info about a specific peer
    pub fn get_peer(&self, peer_id: &PeerId) -> Option<PeerInfo> {
        self.peers.get(peer_id).cloned()
    }

    /// Get all connected peers
    pub fn connected_peers(&self) -> Vec<PeerInfo> {
        self.peers.values().cloned().collect()
    }

    /// Get count of connected peers
    pub fn peer_count(&self) -> usize {
        self.peers.len()
    }

    /// Get known addresses for a peer (even if not connected)
    pub fn get_known_addresses(&self, peer_id: &PeerId) -> Option<Vec<Multiaddr>> {
        self.known_peers.get(peer_id).cloned()
    }

    /// Update peer activity timestamp
    pub fn on_peer_activity(&mut self, peer_id: &PeerId) {
        if let Some(info) = self.peers.get_mut(peer_id) {
            info.last_seen = Instant::now();
        }
    }

    /// Get network statistics
    pub fn stats(&self) -> NetworkStats {
        NetworkStats {
            connected_peers: self.peers.len(),
            known_peers: self.known_peers.len(),
            uptime_secs: self.started_at.elapsed().as_secs(),
            total_connections: self.total_connections,
            total_connection_failures: self.total_failures,
        }
    }

    /// Check if a peer is currently connected
    pub fn is_connected(&self, peer_id: &PeerId) -> bool {
        self.peers.contains_key(peer_id)
    }

    /// Get reconnection count for a peer
    pub fn get_reconnection_count(&self, peer_id: &PeerId) -> u32 {
        self.reconnection_counts.get(peer_id).copied().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libp2p::identity::Keypair;

    fn create_test_peer_id() -> PeerId {
        Keypair::generate_ed25519().public().to_peer_id()
    }

    fn create_test_addr() -> Multiaddr {
        "/ip4/127.0.0.1/tcp/4001".parse().unwrap()
    }

    fn create_test_addr_with_port(port: u64) -> Multiaddr {
        format!("/ip4/127.0.0.1/tcp/{}", port).parse().unwrap()
    }

    // ==================== BASIC CREATION ====================

    #[test]
    fn test_peer_registry_creation() {
        let registry = PeerRegistry::new();
        assert_eq!(registry.peer_count(), 0);
        assert_eq!(registry.connected_peers().len(), 0);

        let stats = registry.stats();
        assert_eq!(stats.connected_peers, 0);
        assert_eq!(stats.known_peers, 0);
        assert_eq!(stats.total_connections, 0);
        assert_eq!(stats.total_connection_failures, 0);
    }

    // ==================== CONNECTION ESTABLISHMENT ====================

    #[test]
    fn test_connection_established_outbound() {
        let mut registry = PeerRegistry::new();
        let peer_id = create_test_peer_id();
        let addr = create_test_addr();

        registry.on_connection_established(
            peer_id,
            addr.clone(),
            ConnectionDirection::Outbound,
            DiscoverySource::Mdns,
        );

        assert_eq!(registry.peer_count(), 1);

        let peer_info = registry.get_peer(&peer_id).unwrap();

        assert_eq!(peer_info.connection_count, 1);
        assert_eq!(peer_info.addresses.len(), 1);
        assert_eq!(peer_info.addresses[0], addr);
        assert_eq!(
            peer_info.connection_direction,
            ConnectionDirection::Outbound
        );
        assert_eq!(peer_info.stats.reconnection_count, 0);
        assert_eq!(peer_info.discovery_source, DiscoverySource::Mdns);
    }

    #[test]
    fn test_connection_established_inbound() {
        let mut registry = PeerRegistry::new();
        let peer_id = create_test_peer_id();
        let addr = create_test_addr();

        registry.on_connection_established(
            peer_id,
            addr.clone(),
            ConnectionDirection::Inbound,
            DiscoverySource::Manual,
        );

        let peer_info = registry.get_peer(&peer_id).unwrap();
        assert_eq!(peer_info.connection_direction, ConnectionDirection::Inbound);
    }

    #[test]
    fn test_discovery_source_serialization() {
        let source = DiscoverySource::Mdns;
        let json = serde_json::to_string(&source).unwrap();
        assert_eq!(json, r#""Mdns""#);

        let deserialized: DiscoverySource = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, DiscoverySource::Mdns);
    }

    #[test]
    fn test_multiple_connections_same_peer() {
        let mut registry = PeerRegistry::new();
        let peer_id = create_test_peer_id();
        let addr1 = create_test_addr_with_port(4001);
        let addr2 = create_test_addr_with_port(4002);

        registry.on_connection_established(
            peer_id,
            addr1.clone(),
            ConnectionDirection::Outbound,
            DiscoverySource::Mdns,
        );
        registry.on_connection_established(
            peer_id,
            addr2.clone(),
            ConnectionDirection::Outbound,
            DiscoverySource::Mdns,
        );

        assert_eq!(registry.peer_count(), 1);
        let peer_info = registry.get_peer(&peer_id).unwrap();
        assert_eq!(peer_info.connection_count, 2);
        assert_eq!(peer_info.addresses.len(), 2);
        assert!(peer_info.addresses.contains(&addr1));
        assert!(peer_info.addresses.contains(&addr2));
    }

    #[test]
    fn test_duplicate_address_not_added_twice() {
        let mut registry = PeerRegistry::new();
        let peer_id = create_test_peer_id();
        let addr = create_test_addr();

        // Connect twice with same address
        registry.on_connection_established(
            peer_id,
            addr.clone(),
            ConnectionDirection::Outbound,
            DiscoverySource::Manual,
        );
        registry.on_connection_established(
            peer_id,
            addr.clone(),
            ConnectionDirection::Outbound,
            DiscoverySource::Manual,
        );

        let peer_info = registry.get_peer(&peer_id).unwrap();
        assert_eq!(peer_info.connection_count, 2);
        assert_eq!(
            peer_info.addresses.len(),
            1,
            "Address should not be duplicated"
        );
    }

    #[test]
    fn test_multiple_different_peers() {
        let mut registry = PeerRegistry::new();

        let peer1 = create_test_peer_id();
        let peer2 = create_test_peer_id();
        let peer3 = create_test_peer_id();

        registry.on_connection_established(
            peer1,
            create_test_addr_with_port(4001),
            ConnectionDirection::Outbound,
            DiscoverySource::Manual,
        );
        registry.on_connection_established(
            peer2,
            create_test_addr_with_port(4002),
            ConnectionDirection::Inbound,
            DiscoverySource::Manual,
        );
        registry.on_connection_established(
            peer3,
            create_test_addr_with_port(4003),
            ConnectionDirection::Outbound,
            DiscoverySource::Manual,
        );

        assert_eq!(registry.peer_count(), 3);
        assert_eq!(registry.connected_peers().len(), 3);

        let stats = registry.stats();
        assert_eq!(stats.connected_peers, 3);
        assert_eq!(stats.total_connections, 3);
    }

    // ==================== CONNECTION CLOSURE ====================

    #[test]
    fn test_connection_closed_single() {
        let mut registry = PeerRegistry::new();
        let peer_id = create_test_peer_id();
        let addr = create_test_addr();

        registry.on_connection_established(
            peer_id,
            addr.clone(),
            ConnectionDirection::Outbound,
            DiscoverySource::Manual,
        );
        assert_eq!(registry.peer_count(), 1);

        registry.on_connection_closed(&peer_id);
        assert_eq!(registry.peer_count(), 0);

        // Should still be in known_peers
        let known_addrs = registry.get_known_addresses(&peer_id);
        assert!(known_addrs.is_some());
        assert_eq!(known_addrs.unwrap().len(), 1);
    }

    #[test]
    fn test_connection_closed_multiple_connections() {
        let mut registry = PeerRegistry::new();
        let peer_id = create_test_peer_id();
        let addr1 = create_test_addr_with_port(4001);
        let addr2 = create_test_addr_with_port(4002);

        registry.on_connection_established(
            peer_id,
            addr1,
            ConnectionDirection::Outbound,
            DiscoverySource::Manual,
        );
        registry.on_connection_established(
            peer_id,
            addr2,
            ConnectionDirection::Outbound,
            DiscoverySource::Manual,
        );
        assert_eq!(registry.peer_count(), 1);

        // Close one connection - peer should still be connected
        registry.on_connection_closed(&peer_id);
        assert_eq!(registry.peer_count(), 1);
        assert_eq!(registry.get_peer(&peer_id).unwrap().connection_count, 1);

        // Close second connection - peer should be disconnected
        registry.on_connection_closed(&peer_id);
        assert_eq!(registry.peer_count(), 0);
        assert!(registry.get_peer(&peer_id).is_none());
    }

    #[test]
    fn test_connection_closed_nonexistent_peer() {
        let mut registry = PeerRegistry::new();
        let peer_id = create_test_peer_id();

        // Should not panic or error
        registry.on_connection_closed(&peer_id);
        assert_eq!(registry.peer_count(), 0);
    }

    #[test]
    fn test_connection_closed_below_zero() {
        let mut registry = PeerRegistry::new();
        let peer_id = create_test_peer_id();

        registry.on_connection_established(
            peer_id,
            create_test_addr(),
            ConnectionDirection::Outbound,
            DiscoverySource::Manual,
        );

        // Close connection multiple times (should saturate at 0)
        registry.on_connection_closed(&peer_id);
        registry.on_connection_closed(&peer_id);
        registry.on_connection_closed(&peer_id);

        assert_eq!(registry.peer_count(), 0);
    }

    // ==================== RECONNECTION SCENARIOS ====================

    #[test]
    fn test_reconnection_increments_counter() {
        let mut registry = PeerRegistry::new();
        let peer_id = create_test_peer_id();
        let addr = create_test_addr();

        // First connection
        registry.on_connection_established(
            peer_id,
            addr.clone(),
            ConnectionDirection::Outbound,
            DiscoverySource::Manual,
        );
        assert_eq!(
            registry
                .get_peer(&peer_id)
                .unwrap()
                .stats
                .reconnection_count,
            0
        );

        // Disconnect
        registry.on_connection_closed(&peer_id);
        assert_eq!(registry.peer_count(), 0);

        // Reconnect - should increment reconnection counter
        registry.on_connection_established(
            peer_id,
            addr.clone(),
            ConnectionDirection::Outbound,
            DiscoverySource::Manual,
        );
        assert_eq!(registry.peer_count(), 1);
        assert_eq!(
            registry
                .get_peer(&peer_id)
                .unwrap()
                .stats
                .reconnection_count,
            1
        );

        // Disconnect again
        registry.on_connection_closed(&peer_id);

        // Reconnect again
        registry.on_connection_established(
            peer_id,
            addr.clone(),
            ConnectionDirection::Outbound,
            DiscoverySource::Manual,
        );
        assert_eq!(
            registry
                .get_peer(&peer_id)
                .unwrap()
                .stats
                .reconnection_count,
            2
        );
    }

    #[test]
    fn test_known_addresses_persist_after_disconnect() {
        let mut registry = PeerRegistry::new();
        let peer_id = create_test_peer_id();
        let addr1 = create_test_addr_with_port(4001);
        let addr2 = create_test_addr_with_port(4002);

        registry.on_connection_established(
            peer_id,
            addr1.clone(),
            ConnectionDirection::Outbound,
            DiscoverySource::Manual,
        );
        registry.on_connection_established(
            peer_id,
            addr2.clone(),
            ConnectionDirection::Outbound,
            DiscoverySource::Manual,
        );

        // Disconnect
        registry.on_connection_closed(&peer_id);
        registry.on_connection_closed(&peer_id);

        // Both addresses should still be known
        let known_addrs = registry.get_known_addresses(&peer_id).unwrap();
        assert_eq!(known_addrs.len(), 2);
        assert!(known_addrs.contains(&addr1));
        assert!(known_addrs.contains(&addr2));
    }

    // ==================== PEER QUERIES ====================

    #[test]
    fn test_get_peer_nonexistent() {
        let registry = PeerRegistry::new();
        let fake_peer_id = create_test_peer_id();

        assert!(registry.get_peer(&fake_peer_id).is_none());
    }

    #[test]
    fn test_get_known_addresses_nonexistent() {
        let registry = PeerRegistry::new();
        let fake_peer_id = create_test_peer_id();

        assert!(registry.get_known_addresses(&fake_peer_id).is_none());
    }

    #[test]
    fn test_connected_peers_returns_all() {
        let mut registry = PeerRegistry::new();
        let peer1 = create_test_peer_id();
        let peer2 = create_test_peer_id();
        let peer3 = create_test_peer_id();

        registry.on_connection_established(
            peer1,
            create_test_addr_with_port(4001),
            ConnectionDirection::Outbound,
            DiscoverySource::Manual,
        );
        registry.on_connection_established(
            peer2,
            create_test_addr_with_port(4002),
            ConnectionDirection::Inbound,
            DiscoverySource::Manual,
        );
        registry.on_connection_established(
            peer3,
            create_test_addr_with_port(4003),
            ConnectionDirection::Outbound,
            DiscoverySource::Manual,
        );

        let peers = registry.connected_peers();
        assert_eq!(peers.len(), 3);

        let peer_ids: Vec<PeerId> = peers.iter().map(|p| p.peer_id).collect();
        assert!(peer_ids.contains(&peer1));
        assert!(peer_ids.contains(&peer2));
        assert!(peer_ids.contains(&peer3));
    }

    // ==================== PEER ACTIVITY ====================

    #[test]
    fn test_peer_activity_update() {
        let mut registry = PeerRegistry::new();
        let peer_id = create_test_peer_id();

        registry.on_connection_established(
            peer_id,
            create_test_addr(),
            ConnectionDirection::Outbound,
            DiscoverySource::Manual,
        );

        let first_last_seen = registry.get_peer(&peer_id).unwrap().last_seen;

        std::thread::sleep(std::time::Duration::from_millis(10));
        registry.on_peer_activity(&peer_id);

        let updated_last_seen = registry.get_peer(&peer_id).unwrap().last_seen;

        assert!(updated_last_seen > first_last_seen);
    }

    #[test]
    fn test_peer_activity_nonexistent_peer() {
        let mut registry = PeerRegistry::new();
        let fake_peer_id = create_test_peer_id();

        // Should not panic
        registry.on_peer_activity(&fake_peer_id);
    }

    #[test]
    fn test_first_seen_vs_last_seen() {
        let mut registry = PeerRegistry::new();
        let peer_id = create_test_peer_id();

        registry.on_connection_established(
            peer_id,
            create_test_addr(),
            ConnectionDirection::Outbound,
            DiscoverySource::Manual,
        );

        let peer_info = registry.get_peer(&peer_id).unwrap();
        let first_seen = peer_info.first_seen;
        let initial_last_seen = peer_info.last_seen;

        // Initially they should be equal (or very close)
        assert!(initial_last_seen >= first_seen);

        std::thread::sleep(std::time::Duration::from_millis(10));
        registry.on_peer_activity(&peer_id);

        let updated_peer_info = registry.get_peer(&peer_id).unwrap();

        // first_seen should never change
        assert_eq!(updated_peer_info.first_seen, first_seen);

        // last_seen should be updated
        assert!(updated_peer_info.last_seen > initial_last_seen);
    }

    // ==================== CONNECTION FAILURES ====================

    #[test]
    fn test_connection_failed_tracking() {
        let mut registry = PeerRegistry::new();

        let stats_before = registry.stats();
        assert_eq!(stats_before.total_connection_failures, 0);

        registry.on_connection_failed(None);
        registry.on_connection_failed(None);
        registry.on_connection_failed(Some(&create_test_peer_id()));

        let stats_after = registry.stats();
        assert_eq!(stats_after.total_connection_failures, 3);
    }

    // ==================== NETWORK STATISTICS ====================

    #[test]
    fn test_network_stats_comprehensive() {
        let mut registry = PeerRegistry::new();
        let peer1 = create_test_peer_id();
        let peer2 = create_test_peer_id();
        let peer3 = create_test_peer_id();

        // Connect 3 peers
        registry.on_connection_established(
            peer1,
            create_test_addr_with_port(4001),
            ConnectionDirection::Outbound,
            DiscoverySource::Manual,
        );
        registry.on_connection_established(
            peer2,
            create_test_addr_with_port(4002),
            ConnectionDirection::Inbound,
            DiscoverySource::Manual,
        );
        registry.on_connection_established(
            peer3,
            create_test_addr_with_port(4003),
            ConnectionDirection::Outbound,
            DiscoverySource::Manual,
        );

        // Disconnect one
        registry.on_connection_closed(&peer3);

        // Add some failures
        registry.on_connection_failed(None);
        registry.on_connection_failed(None);

        let stats = registry.stats();
        assert_eq!(stats.connected_peers, 2, "Should have 2 active connections");
        assert_eq!(stats.known_peers, 3, "Should know about 3 peers total");
        assert_eq!(
            stats.total_connections, 3,
            "Should have had 3 total connections"
        );
        assert_eq!(stats.total_connection_failures, 2);
    }

    #[test]
    fn test_stats_uptime_increases() {
        let registry = PeerRegistry::new();

        let stats1 = registry.stats();
        let uptime1 = stats1.uptime_secs;

        std::thread::sleep(std::time::Duration::from_millis(100));

        let stats2 = registry.stats();
        let uptime2 = stats2.uptime_secs;

        assert!(uptime2 >= uptime1, "Uptime should increase or stay same");
    }

    // ==================== CONNECTION STATS ====================

    #[test]
    fn test_connection_stats_defaults() {
        let stats = ConnectionStats::default();
        assert_eq!(stats.bytes_sent, 0);
        assert_eq!(stats.bytes_received, 0);
        assert_eq!(stats.reconnection_count, 0);
        assert_eq!(stats.avg_latency_ms, None);
    }

    #[test]
    fn test_peer_info_has_default_stats() {
        let mut registry = PeerRegistry::new();
        let peer_id = create_test_peer_id();

        registry.on_connection_established(
            peer_id,
            create_test_addr(),
            ConnectionDirection::Outbound,
            DiscoverySource::Manual,
        );

        let peer_info = registry.get_peer(&peer_id).unwrap();
        assert_eq!(peer_info.stats.bytes_sent, 0);
        assert_eq!(peer_info.stats.bytes_received, 0);
        assert_eq!(peer_info.stats.reconnection_count, 0);
    }

    // ==================== SERIALIZATION ====================

    #[test]
    fn test_peer_info_serialization() {
        let mut registry = PeerRegistry::new();
        let peer_id = create_test_peer_id();
        let addr = create_test_addr();

        registry.on_connection_established(
            peer_id,
            addr,
            ConnectionDirection::Outbound,
            DiscoverySource::Manual,
        );
        let peer_info = registry.get_peer(&peer_id).unwrap();

        // Serialize to JSON
        let json = serde_json::to_string(&peer_info).expect("Should serialize to JSON");
        assert!(!json.is_empty());
        assert!(json.contains(&peer_id.to_string()));
    }

    #[test]
    fn test_peer_info_deserialization() {
        let peer_id = create_test_peer_id();
        let peer_id_str = peer_id.to_string();

        let json = format!(
            r#"{{
                "peer_id": "{}",
                "addresses": ["/ip4/127.0.0.1/tcp/4001"],
                "connection_count": 1,
                "stats": {{
                    "bytes_sent": 0,
                    "bytes_received": 0,
                    "reconnection_count": 0,
                    "avg_latency_ms": null
                }},
                "connection_direction": "Outbound",
                "discovery_source": "Unknown"
            }}"#,
            peer_id_str
        );

        let peer_info: PeerInfo =
            serde_json::from_str(&json).expect("Should deserialize from JSON");
        assert_eq!(peer_info.peer_id.to_string(), peer_id_str);
        assert_eq!(peer_info.addresses.len(), 1);
        assert_eq!(peer_info.connection_count, 1);
        assert_eq!(
            peer_info.connection_direction,
            ConnectionDirection::Outbound
        );
    }

    #[test]
    fn test_network_stats_serialization() {
        let mut registry = PeerRegistry::new();
        registry.on_connection_established(
            create_test_peer_id(),
            create_test_addr(),
            ConnectionDirection::Outbound,
            DiscoverySource::Manual,
        );

        let stats = registry.stats();
        let json = serde_json::to_string(&stats).expect("Should serialize");
        assert!(json.contains("connected_peers"));
        assert!(json.contains("uptime_secs"));
    }

    #[test]
    fn test_connection_direction_serialization() {
        let outbound = ConnectionDirection::Outbound;
        let inbound = ConnectionDirection::Inbound;

        let outbound_json = serde_json::to_string(&outbound).unwrap();
        let inbound_json = serde_json::to_string(&inbound).unwrap();

        assert_eq!(outbound_json, r#""Outbound""#);
        assert_eq!(inbound_json, r#""Inbound""#);
    }

    // ==================== EDGE CASES ====================

    #[test]
    fn test_empty_registry_operations() {
        let mut registry = PeerRegistry::new();
        let fake_peer = create_test_peer_id();

        // All operations should be safe on empty registry
        assert_eq!(registry.peer_count(), 0);
        assert_eq!(registry.connected_peers().len(), 0);
        assert!(registry.get_peer(&fake_peer).is_none());
        assert!(registry.get_known_addresses(&fake_peer).is_none());

        registry.on_connection_closed(&fake_peer);
        registry.on_peer_activity(&fake_peer);
        registry.on_connection_failed(Some(&fake_peer));

        // Registry should still be valid
        assert_eq!(registry.peer_count(), 0);
    }

    #[test]
    fn test_large_number_of_peers() {
        let mut registry = PeerRegistry::new();
        let num_peers: u64 = 100;

        let mut peer_ids = Vec::new();
        for i in 0..num_peers {
            let peer_id = create_test_peer_id();
            let addr = create_test_addr_with_port(4000 + i);
            registry.on_connection_established(
                peer_id,
                addr,
                ConnectionDirection::Outbound,
                DiscoverySource::Manual,
            );
            peer_ids.push(peer_id);
        }

        assert_eq!(registry.peer_count(), num_peers as usize);
        assert_eq!(registry.connected_peers().len(), num_peers as usize);

        let stats = registry.stats();
        assert_eq!(stats.connected_peers, num_peers as usize);
        assert_eq!(stats.total_connections, num_peers);
    }

    #[test]
    fn test_bulk_load_known_peers_empty() {
        let mut registry = PeerRegistry::new();
        let peers = HashMap::new();

        registry.bulk_load_known_peers(peers);

        assert_eq!(registry.known_peers.len(), 0);
    }

    #[test]
    fn test_bulk_load_known_peers_single() {
        let mut registry = PeerRegistry::new();
        let peer_id = PeerId::random();
        let addr: Multiaddr = "/ip4/127.0.0.1/tcp/4001".parse().unwrap();

        let mut peers = HashMap::new();
        peers.insert(peer_id, vec![addr.clone()]);

        registry.bulk_load_known_peers(peers);

        assert_eq!(registry.known_peers.len(), 1);
        assert!(registry.known_peers.contains_key(&peer_id));
        assert_eq!(registry.known_peers.get(&peer_id).unwrap().len(), 1);
        assert_eq!(registry.known_peers.get(&peer_id).unwrap()[0], addr);
    }

    #[test]
    fn test_bulk_load_known_peers_multiple() {
        let mut registry = PeerRegistry::new();
        let peer1 = PeerId::random();
        let peer2 = PeerId::random();
        let peer3 = PeerId::random();

        let addr1: Multiaddr = "/ip4/127.0.0.1/tcp/4001".parse().unwrap();
        let addr2: Multiaddr = "/ip4/127.0.0.1/tcp/4002".parse().unwrap();
        let addr3: Multiaddr = "/ip4/127.0.0.1/tcp/4003".parse().unwrap();

        let mut peers = HashMap::new();
        peers.insert(peer1, vec![addr1.clone()]);
        peers.insert(peer2, vec![addr2.clone(), addr3.clone()]);
        peers.insert(peer3, vec![]);

        registry.bulk_load_known_peers(peers);

        // peer3 should be skipped due to empty addresses
        assert_eq!(registry.known_peers.len(), 2);
        assert!(registry.known_peers.contains_key(&peer1));
        assert!(registry.known_peers.contains_key(&peer2));
        assert!(!registry.known_peers.contains_key(&peer3));

        assert_eq!(registry.known_peers.get(&peer1).unwrap().len(), 1);
        assert_eq!(registry.known_peers.get(&peer2).unwrap().len(), 2);
    }

    #[test]
    fn test_bulk_load_merges_with_existing() {
        let mut registry = PeerRegistry::new();
        let peer_id = PeerId::random();
        let addr1: Multiaddr = "/ip4/127.0.0.1/tcp/4001".parse().unwrap();
        let addr2: Multiaddr = "/ip4/127.0.0.1/tcp/4002".parse().unwrap();

        // Manually add one address
        registry.known_peers.insert(peer_id, vec![addr1.clone()]);

        // Bulk load with another address
        let mut peers = HashMap::new();
        peers.insert(peer_id, vec![addr2.clone()]);

        registry.bulk_load_known_peers(peers);

        // Should have both addresses
        assert_eq!(registry.known_peers.get(&peer_id).unwrap().len(), 2);
        assert!(registry.known_peers.get(&peer_id).unwrap().contains(&addr1));
        assert!(registry.known_peers.get(&peer_id).unwrap().contains(&addr2));
    }

    #[test]
    fn test_bulk_load_avoids_duplicate_addresses() {
        let mut registry = PeerRegistry::new();
        let peer_id = PeerId::random();
        let addr: Multiaddr = "/ip4/127.0.0.1/tcp/4001".parse().unwrap();

        // Manually add address
        registry.known_peers.insert(peer_id, vec![addr.clone()]);

        // Bulk load with same address
        let mut peers = HashMap::new();
        peers.insert(peer_id, vec![addr.clone()]);

        registry.bulk_load_known_peers(peers);

        // Should still have only one address (no duplicate)
        assert_eq!(registry.known_peers.get(&peer_id).unwrap().len(), 1);
    }

    #[test]
    fn test_bulk_load_reconnection_counts_empty() {
        let mut registry = PeerRegistry::new();
        let counts = HashMap::new();

        registry.bulk_load_reconnection_counts(counts);

        assert_eq!(registry.reconnection_counts.len(), 0);
    }

    #[test]
    fn test_bulk_load_reconnection_counts_single() {
        let mut registry = PeerRegistry::new();
        let peer_id = PeerId::random();

        let mut counts = HashMap::new();
        counts.insert(peer_id, 5);

        registry.bulk_load_reconnection_counts(counts);

        assert_eq!(registry.reconnection_counts.len(), 1);
        assert_eq!(*registry.reconnection_counts.get(&peer_id).unwrap(), 5);
    }

    #[test]
    fn test_bulk_load_reconnection_counts_skips_zero() {
        let mut registry = PeerRegistry::new();
        let peer1 = PeerId::random();
        let peer2 = PeerId::random();
        let peer3 = PeerId::random();

        let mut counts = HashMap::new();
        counts.insert(peer1, 3);
        counts.insert(peer2, 10);
        counts.insert(peer3, 0); // Should be skipped

        registry.bulk_load_reconnection_counts(counts);

        assert_eq!(registry.reconnection_counts.len(), 2);
        assert_eq!(*registry.reconnection_counts.get(&peer1).unwrap(), 3);
        assert_eq!(*registry.reconnection_counts.get(&peer2).unwrap(), 10);
        assert!(!registry.reconnection_counts.contains_key(&peer3));
    }

    #[test]
    fn test_bulk_load_uses_maximum_on_conflict() {
        let mut registry = PeerRegistry::new();
        let peer_id = PeerId::random();

        // Manually set count to 5
        registry.reconnection_counts.insert(peer_id, 5);

        // Bulk load with higher count
        let mut counts = HashMap::new();
        counts.insert(peer_id, 10);

        registry.bulk_load_reconnection_counts(counts);

        // Should use the maximum (10)
        assert_eq!(*registry.reconnection_counts.get(&peer_id).unwrap(), 10);
    }

    #[test]
    fn test_bulk_loaded_peer_reconnection_tracking() {
        let mut registry = PeerRegistry::new();
        let peer_id = PeerId::random();
        let addr: Multiaddr = "/ip4/127.0.0.1/tcp/4001".parse().unwrap();

        // Bulk load known peer and reconnection count
        let mut peers = HashMap::new();
        peers.insert(peer_id, vec![addr.clone()]);
        registry.bulk_load_known_peers(peers);

        let mut counts = HashMap::new();
        counts.insert(peer_id, 5);
        registry.bulk_load_reconnection_counts(counts);

        // Verify bulk loaded data
        assert_eq!(registry.known_peers.len(), 1);
        assert_eq!(*registry.reconnection_counts.get(&peer_id).unwrap(), 5);

        // Now simulate a connection
        registry.on_connection_established(
            peer_id,
            addr.clone(),
            ConnectionDirection::Outbound,
            DiscoverySource::Dht,
        );

        // Reconnection count should have been incremented
        assert_eq!(*registry.reconnection_counts.get(&peer_id).unwrap(), 6);

        // Peer info should have the reconnection count
        let peer_info = registry.get_peer(&peer_id).unwrap();
        assert_eq!(peer_info.stats.reconnection_count, 6);
    }
}
