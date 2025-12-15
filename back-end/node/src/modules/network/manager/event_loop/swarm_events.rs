//! Swarm event handling - connections, errors, dispatching

use super::NetworkEventLoop;
use crate::modules::network::behaviour::FlowBehaviourEvent;
use crate::modules::network::config::ConnectionLimitPolicy;
use crate::modules::network::peer_registry::{ConnectionDirection, DiscoverySource};
use tracing::{debug, info, warn};

impl NetworkEventLoop {
    /// Handle individual Swarm events
    pub(crate) async fn handle_swarm_event(
        &mut self,
        event: libp2p::swarm::SwarmEvent<FlowBehaviourEvent>,
    ) {
        use libp2p::swarm::SwarmEvent;

        match event {
            SwarmEvent::NewListenAddr { address, .. } => {
                info!("Listening on: {}", address);
            }

            SwarmEvent::ConnectionEstablished {
                peer_id,
                endpoint,
                established_in,
                ..
            } => {
                let direction = if endpoint.is_dialer() {
                    ConnectionDirection::Outbound
                } else {
                    ConnectionDirection::Inbound
                };

                let should_disconnect = if self.connection_limits.max_connections == 0 {
                    false // Unlimited
                } else {
                    match self.connection_limits.policy {
                        ConnectionLimitPolicy::Strict => {
                            // Strict: reject ANY connection over max
                            self.active_connections >= self.connection_limits.max_connections
                        }
                        ConnectionLimitPolicy::PreferOutbound => {
                            // PreferOutbound: only reject INBOUND over inbound_limit
                            // Outbound can go up to max_connections
                            if direction == ConnectionDirection::Inbound {
                                let inbound_limit = self
                                    .connection_limits
                                    .max_connections
                                    .saturating_sub(self.connection_limits.reserved_outbound);
                                self.active_connections >= inbound_limit
                            } else {
                                // Outbound: only check against total max
                                self.active_connections >= self.connection_limits.max_connections
                            }
                        }
                    }
                };

                if should_disconnect {
                    self.rejected_connections.insert(peer_id);

                    warn!(
                        active_connections = self.active_connections,
                        max_connections = self.connection_limits.max_connections,
                        policy = ?self.connection_limits.policy,
                        direction = ?direction,
                        peer_id = %peer_id,
                        "Connection limit exceeded, disconnecting peer"
                    );

                    // Disconnect immediately
                    let _ = self.swarm.disconnect_peer_id(peer_id);

                    // Don't process further
                    return;
                }

                // Increment connection counter
                self.active_connections += 1;

                // Determine discovery source
                // For outbound connections, we look up how we discovered them
                // For inbound connections, we don't know how they found us
                let discovery_source = if direction == ConnectionDirection::Outbound {
                    self.peer_discovery_source
                        .remove(&peer_id) // Remove and return
                        .unwrap_or(DiscoverySource::Unknown)
                } else {
                    DiscoverySource::Unknown // We don't know how they discovered us
                };

                info!(
                    active_connections = self.active_connections,
                    "Connection established with {} via {} (direction: {:?}, source: {:?}, time: {:?})",
                    peer_id,
                    endpoint.get_remote_address(),
                    direction,
                    discovery_source,
                    established_in
                );

                // Remove from dialing set if we were dialing
                self.dialing_peers.remove(&peer_id);
                self.peer_registry.on_connection_established(
                    peer_id,
                    endpoint.get_remote_address().clone(),
                    direction,
                    discovery_source,
                );
            }

            SwarmEvent::ConnectionClosed { peer_id, cause, .. } => {
                // Only decrement if this wasn't a rejected connection
                if self.rejected_connections.remove(&peer_id) {
                    debug!(peer_id = %peer_id, "Ignoring close for rejected connection");
                } else {
                    self.active_connections = self.active_connections.saturating_sub(1);
                }

                debug!(
                    active_connections = self.active_connections,
                    peer_id = %peer_id,
                    cause = ?cause,
                    "Connection closed"
                );

                self.peer_registry.on_connection_closed(&peer_id);
                self.dialing_peers.remove(&peer_id);
            }

            SwarmEvent::IncomingConnection { send_back_addr, .. } => {
                debug!(
                    active_connections = self.active_connections,
                    max_connections = self.connection_limits.max_connections,
                    from = %send_back_addr,
                    "Incoming connection attempt"
                );
            }

            SwarmEvent::IncomingConnectionError {
                send_back_addr,
                error,
                ..
            } => {
                warn!(
                    "Incoming connection error from {}: {}",
                    send_back_addr, error
                );
                self.peer_registry.on_connection_failed(None);
            }

            SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
                warn!("Outgoing connection error to {:?}: {}", peer_id, error);
                if let Some(peer) = peer_id.as_ref() {
                    self.dialing_peers.remove(peer);
                }
                self.peer_registry.on_connection_failed(peer_id.as_ref());
            }

            SwarmEvent::Dialing {
                peer_id,
                connection_id,
            } => {
                debug!(
                    "Dialing peer {:?} (connection: {:?})",
                    peer_id, connection_id
                );
            }

            SwarmEvent::Behaviour(FlowBehaviourEvent::Kademlia(kad_event)) => {
                self.handle_kad_event(kad_event).await;
            }

            SwarmEvent::Behaviour(FlowBehaviourEvent::Mdns(mdns_event)) => {
                self.handle_mdns_event(mdns_event).await;
            }

            SwarmEvent::Behaviour(FlowBehaviourEvent::Gossipsub(event)) => {
                self.handle_gossipsub_event(event).await;
            }

            SwarmEvent::Behaviour(FlowBehaviourEvent::ContentTransfer(event)) => {
                self.handle_content_transfer_event(event).await;
            }

            event => {
                debug!("Unhandled swarm event: {:?}", event);
            }
        }
    }
}
