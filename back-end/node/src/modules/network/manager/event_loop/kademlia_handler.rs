//! Kademlia event handler - DHT routing, provider queries

use super::NetworkEventLoop;
use crate::modules::network::peer_registry::DiscoverySource;
use libp2p::kad::QueryResult;
use tracing::{debug, info, warn};

impl NetworkEventLoop {
    /// Handle Kademlia-specific events
    pub(crate) async fn handle_kad_event(&mut self, event: libp2p::kad::Event) {
        use libp2p::kad::Event;

        match event {
            Event::RoutingUpdated {
                peer, addresses, ..
            } => {
                debug!(
                    source = "dht",
                    %peer,
                    address_count = addresses.len(),
                    "Routing table updated"
                );

                // Track that we discovered this peer via DHT
                self.peer_discovery_source
                    .insert(peer, DiscoverySource::Dht);

                // Note: Kademlia automatically dials these peers in the background
                // The connection will use the discovery source we just set
            }

            Event::OutboundQueryProgressed { id, result, .. } => match result {
                QueryResult::GetClosestPeers(Ok(closest)) => {
                    debug!(
                        ?id,
                        peers = closest.peers.len(),
                        "GetClosestPeers completed"
                    );
                    for peer_info in &closest.peers {
                        self.peer_discovery_source
                            .insert(peer_info.peer_id, DiscoverySource::Dht);
                    }
                }

                QueryResult::StartProviding(Ok(add_provider_ok)) => {
                    info!(
                        query_id = ?id,
                        key = ?add_provider_ok.key,
                        "Successfully announced as provider"
                    );

                    // Invoke callback and update metadata if tracked
                    if let Some(pending) = self.pending_provider_announcements.remove(&id) {
                        let elapsed = pending.created_at.elapsed();
                        info!(
                            %pending.cid,
                            elapsed_ms = elapsed.as_millis(),
                            "Provider announcement confirmed"
                        );

                        // Update announcement metadata
                        if let Err(e) = self.provider_registry.record_announcement(&pending.cid) {
                            warn!(%pending.cid, error = %e, "Failed to record announcement");
                        }

                        // Invoke callback
                        if let Some(callback) = pending.callback {
                            callback(pending.cid, Ok(()));
                        }
                    }
                }

                QueryResult::StartProviding(Err(add_provider_err)) => {
                    warn!(
                        query_id = ?id,
                        key = ?add_provider_err.key(),
                        "Failed to announce as provider"
                    );

                    if let Some(pending) = self.pending_provider_announcements.remove(&id) {
                        if let Some(callback) = pending.callback {
                            callback(
                                pending.cid,
                                Err(format!(
                                    "Provider announcement failed for key {:?}",
                                    add_provider_err.key()
                                )),
                            );
                        }
                    }
                }

                QueryResult::GetProviders(Ok(get_providers_ok)) => match get_providers_ok {
                    libp2p::kad::GetProvidersOk::FoundProviders { key, providers } => {
                        debug!(
                            query_id = ?id,
                            key = ?key,
                            provider_count = providers.len(),
                            "GetProviders found providers"
                        );

                        // Accumulate providers in pending query
                        if let Some(pending) = self.pending_provider_queries.get_mut(&id) {
                            pending.providers.extend(providers);
                            debug!(
                                query_id = ?id,
                                total_providers = pending.providers.len(),
                                "Accumulated providers for query"
                            );
                        }
                    }
                    libp2p::kad::GetProvidersOk::FinishedWithNoAdditionalRecord {
                        closest_peers,
                    } => {
                        debug!(
                            query_id = ?id,
                            closest_peers_count = closest_peers.len(),
                            "GetProviders finished with no additional records"
                        );

                        // Complete the pending query
                        if let Some(pending) = self.pending_provider_queries.remove(&id) {
                            let provider_count = pending.providers.len();
                            let elapsed = pending.created_at.elapsed();

                            info!(
                                %pending.cid,
                                provider_count,
                                elapsed_ms = elapsed.as_millis(),
                                "Provider discovery completed"
                            );

                            let _ = pending.response_tx.send(Ok(pending.providers));
                        }
                    }
                },

                QueryResult::GetProviders(Err(get_providers_err)) => match get_providers_err {
                    libp2p::kad::GetProvidersError::Timeout { key, closest_peers } => {
                        debug!(
                            query_id = ?id,
                            key = ?key,
                            closest_peers_count = closest_peers.len(),
                            "GetProviders timed out"
                        );

                        // Complete with whatever providers we found (may be empty)
                        if let Some(pending) = self.pending_provider_queries.remove(&id) {
                            let provider_count = pending.providers.len();

                            if provider_count > 0 {
                                info!(
                                    %pending.cid,
                                    provider_count,
                                    "GetProviders timed out but found some providers"
                                );
                                let _ = pending.response_tx.send(Ok(pending.providers));
                            } else {
                                warn!(%pending.cid, "GetProviders timed out with no providers");
                                let _ = pending.response_tx.send(Err(format!(
                                    "Provider query timed out for {}",
                                    pending.cid
                                )));
                            }
                        }
                    }
                },

                QueryResult::Bootstrap(Ok(bootstrap_ok)) => {
                    debug!(
                        query_id = ?id,
                        peer = %bootstrap_ok.peer,
                        remaining = bootstrap_ok.num_remaining,
                        "Bootstrap query progressed"
                    );
                }

                QueryResult::Bootstrap(Err(e)) => {
                    warn!(query_id = ?id, error = ?e, "Bootstrap query failed");
                }

                other => {
                    debug!(query_id = ?id, result = ?other, "Kademlia query progressed");
                }
            },

            Event::InboundRequest { request } => match request {
                libp2p::kad::InboundRequest::AddProvider { record } => {
                    if let Some(r) = record {
                        info!(
                            provider = ?r.provider,
                            key = ?r.key,
                            "Received ADD_PROVIDER request - storing provider record"
                        );
                    }
                }
                libp2p::kad::InboundRequest::GetProvider { .. } => {
                    info!("Received GET_PROVIDER request");
                }
                _ => {
                    debug!(?request, "Received inbound Kademlia request");
                }
            },

            event => {
                debug!("Kademlia event: {:?}", event);
            }
        }
    }
}
