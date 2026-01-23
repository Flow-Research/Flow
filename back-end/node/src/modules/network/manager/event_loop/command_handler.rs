//! Command handler - processes NetworkCommands from the API layer

use super::super::commands::NetworkCommand;
use super::super::types::ConnectionCapacityStatus;
use super::NetworkEventLoop;
use libp2p::{PeerId, gossipsub::IdentTopic};
use tracing::{debug, info, warn};

impl NetworkEventLoop {
    pub(crate) async fn handle_command(&mut self, command: NetworkCommand) -> bool {
        match command {
            NetworkCommand::Shutdown => {
                info!("Received shutdown command");
                return true; // only shutdown command should return true
            }

            NetworkCommand::GetPeerCount(response_tx) => {
                let count = self.peer_registry.peer_count();
                let _ = response_tx.send(count);
            }

            NetworkCommand::GetConnectedPeers(response_tx) => {
                let peers = self.peer_registry.connected_peers();
                let _ = response_tx.send(peers);
            }

            NetworkCommand::GetPeerInfo { peer_id, response } => {
                let info = self.peer_registry.get_peer(&peer_id);
                let _ = response.send(info);
            }

            NetworkCommand::DialPeer { address, response } => {
                match self.swarm.dial(address.clone()) {
                    Ok(_) => {
                        info!("Dialing peer at {}", address);
                        let _ = response.send(Ok(()));
                    }
                    Err(e) => {
                        warn!("Failed to dial peer at {}: {:?}", address, e);
                        let _ = response.send(Err(format!("Dial failed: {:?}", e)));
                    }
                }
            }

            NetworkCommand::DisconnectPeer { peer_id, response } => {
                let result = self.swarm.disconnect_peer_id(peer_id);
                match result {
                    Ok(_) => {
                        info!("Disconnected from peer {}", peer_id);
                        let _ = response.send(Ok(()));
                    }
                    Err(e) => {
                        warn!("Failed to disconnect from peer {}: {:?}", peer_id, e);
                        let _ = response.send(Err(format!("Disconnect failed: {:?}", e)));
                    }
                }
            }

            NetworkCommand::GetNetworkStats(response_tx) => {
                let stats = self.peer_registry.stats();
                let _ = response_tx.send(stats);
            }

            NetworkCommand::GetConnectionCapacity(response) => {
                let status = ConnectionCapacityStatus {
                    active_connections: self.active_connections,
                    max_connections: self.connection_limits.max_connections,
                    available_slots: self
                        .connection_limits
                        .max_connections
                        .saturating_sub(self.active_connections),
                    can_accept_inbound: self
                        .connection_limits
                        .can_accept_inbound(self.active_connections),
                    can_dial_outbound: self
                        .connection_limits
                        .can_dial_outbound(self.active_connections),
                };

                let _ = response.send(status);
            }

            NetworkCommand::TriggerBootstrap { response } => {
                let result = self.execute_bootstrap();
                let _ = response.send(result);
            }

            NetworkCommand::GetBootstrapPeerIds(response) => {
                let peer_ids: Vec<PeerId> = self.bootstrap_peer_ids.iter().copied().collect();
                let _ = response.send(peer_ids);
            }

            NetworkCommand::Subscribe { topic, response } => {
                let ident_topic = IdentTopic::new(&topic);
                let result = match self.swarm.behaviour_mut().subscribe(&ident_topic) {
                    Ok(true) => {
                        info!(topic = %topic, "Subscribed to topic");
                        Ok(())
                    }
                    Ok(false) => Err("GossipSub is disabled".to_string()),
                    Err(e) => Err(format!("Subscription failed: {:?}", e)),
                };
                let _ = response.send(result);
            }

            NetworkCommand::Unsubscribe { topic, response } => {
                let ident_topic = IdentTopic::new(&topic);
                let unsubscribed = self.swarm.behaviour_mut().unsubscribe(&ident_topic);
                if unsubscribed {
                    info!(topic = %topic, "Unsubscribed from topic");
                } else {
                    debug!(topic = %topic, "Was not subscribed to topic");
                }
                let _ = response.send(Ok(()));
            }

            NetworkCommand::Publish { message, response } => {
                let topic = message.topic.clone();
                let ident_topic = IdentTopic::new(&topic);

                match message.serialize() {
                    Ok(data) => match self.swarm.behaviour_mut().publish(ident_topic, data) {
                        Ok(msg_id) => {
                            info!(
                                topic = %topic,
                                message_id = %msg_id,
                                flow_message_id = %message.id,
                                "Message published"
                            );
                            let _ = response.send(Ok(msg_id.to_string()));
                        }
                        Err(e) => {
                            warn!(topic = %topic, error = ?e, "Publish failed");
                            let _ = response.send(Err(format!("Publish failed: {:?}", e)));
                        }
                    },
                    Err(e) => {
                        let _ = response.send(Err(format!("Serialization failed: {}", e)));
                    }
                }
            }

            NetworkCommand::GetSubscriptions(response_tx) => {
                let topics: Vec<String> = self
                    .swarm
                    .behaviour()
                    .subscribed_topics()
                    .iter()
                    .map(|h| h.to_string())
                    .collect();
                let _ = response_tx.send(topics);
            }

            NetworkCommand::GetMeshPeers { topic, response } => {
                let ident_topic = IdentTopic::new(&topic);
                let peers = self.swarm.behaviour().mesh_peers(&ident_topic.hash());
                let _ = response.send(peers);
            }

            NetworkCommand::RequestBlock {
                peer_id,
                cid,
                response,
            } => {
                self.handle_request_block(peer_id, cid, response);
            }

            NetworkCommand::RequestManifest {
                peer_id,
                cid,
                response,
            } => {
                self.handle_request_manifest(peer_id, cid, response);
            }

            NetworkCommand::StartProviding { cid, response } => {
                let result = self.handle_start_providing(&cid);
                let _ = response.send(result);
            }

            NetworkCommand::StartProvidingWithCallback {
                cid,
                callback,
                response,
            } => {
                let result = self.handle_start_providing_impl(&cid, callback);
                let _ = response.send(result);
            }

            NetworkCommand::StopProviding { cid, response } => {
                let result = self.handle_stop_providing(&cid);
                let _ = response.send(result);
            }

            NetworkCommand::GetProviders { cid, response } => {
                self.handle_get_providers(cid, response);
            }

            NetworkCommand::GetLocalBlock { cid, response } => {
                let block = self
                    .block_store
                    .as_ref()
                    .and_then(|store| store.get(&cid).ok().flatten());
                let _ = response.send(block);
            }
        }

        false
    }
}
