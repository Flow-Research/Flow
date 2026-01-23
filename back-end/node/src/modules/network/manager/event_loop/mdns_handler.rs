//! mDNS event handler - local peer discovery and auto-dialing

use super::NetworkEventLoop;
use crate::modules::network::peer_registry::DiscoverySource;
use std::time::Instant as StdInstant;
use tracing::{debug, info, warn};

impl NetworkEventLoop {
    /// Handle mDNS-specific events
    pub(crate) async fn handle_mdns_event(&mut self, event: libp2p::mdns::Event) {
        match event {
            libp2p::mdns::Event::Discovered(list) => {
                for (peer_id, multiaddr) in list {
                    info!(
                        source = "mdns",
                        %peer_id,
                        %multiaddr,
                        "Peer discovered"
                    );

                    // Track that this peer was discovered via mDNS
                    self.peer_discovery_source
                        .insert(peer_id, DiscoverySource::Mdns);

                    self.swarm
                        .behaviour_mut()
                        .kademlia
                        .add_address(&peer_id, multiaddr.clone());

                    // Check if we should dial this peer
                    if self.should_dial_peer(&peer_id) {
                        info!("Auto-dialing mDNS-discovered peer: {}", peer_id);

                        match self.swarm.dial(multiaddr.clone()) {
                            Ok(_) => {
                                self.dialing_peers.insert(peer_id);
                                self.last_dial_attempt.insert(peer_id, StdInstant::now());
                                debug!("Successfully initiated dial to {}", peer_id);
                            }
                            Err(e) => {
                                warn!("Failed to dial mDNS peer {}: {:?}", peer_id, e);
                                self.peer_discovery_source.remove(&peer_id);
                            }
                        }
                    } else {
                        debug!(
                            "Skipping dial to {} (already connected or recently attempted)",
                            peer_id
                        );

                        self.peer_discovery_source.remove(&peer_id);
                    }
                }
            }

            libp2p::mdns::Event::Expired(list) => {
                for (peer_id, multiaddr) in list {
                    info!(
                        "mDNS peer expired: {} at {} (no longer advertising)",
                        peer_id, multiaddr
                    );
                    // Note: We don't disconnect here, as the peer might still be reachable
                    // Connections will time out naturally if peer is truly gone
                }
            }
        }
    }
}
