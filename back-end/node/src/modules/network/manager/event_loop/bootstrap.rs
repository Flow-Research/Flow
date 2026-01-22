//! Bootstrap logic for network initialization

use super::super::types::BootstrapResult;
use super::NetworkEventLoop;
use crate::modules::network::peer_registry::DiscoverySource;
use libp2p::PeerId;
use std::time::{Duration, Instant as StdInstant};
use tracing::{debug, info, warn};

impl NetworkEventLoop {
    // ==================== Bootstrap Logic ====================

    /// Execute the full bootstrap process
    pub(crate) fn execute_bootstrap(&mut self) -> BootstrapResult {
        if self.bootstrap_initiated {
            return BootstrapResult {
                connected_count: 0,
                failed_count: 0,
                query_initiated: false,
            };
        }

        self.bootstrap_initiated = true;

        // Run maintenance to purge stale peers from previous sessions
        let (failed_purged, expired_purged) = self.peer_registry.run_maintenance();
        if failed_purged > 0 || expired_purged > 0 {
            info!(
                failed_purged = failed_purged,
                expired_purged = expired_purged,
                "Purged stale peers during bootstrap"
            );
        }

        info!(
            peer_count = self.bootstrap_peer_ids.len(),
            auto_dial = self.bootstrap_config.auto_dial,
            auto_bootstrap = self.bootstrap_config.auto_bootstrap,
            "Executing bootstrap"
        );

        let (connected, failed) = self.dial_bootstrap_peers();
        let query_initiated = self.initiate_kademlia_bootstrap();

        BootstrapResult {
            connected_count: connected,
            failed_count: failed,
            query_initiated,
        }
    }

    /// Dial all bootstrap peers using addresses from peer registry
    fn dial_bootstrap_peers(&mut self) -> (usize, usize) {
        if !self.bootstrap_config.auto_dial {
            return (0, 0);
        }

        let mut attempts = 0;
        let mut failures = 0;

        let bootstrap_ids: Vec<PeerId> = self.bootstrap_peer_ids.iter().copied().collect();

        for peer_id in bootstrap_ids {
            if self.peer_registry.is_connected(&peer_id) {
                debug!(peer_id = %peer_id, "Bootstrap peer already connected");
                attempts += 1; // Count as successful
                continue;
            }

            if self.dialing_peers.contains(&peer_id) {
                debug!(peer_id = %peer_id, "Bootstrap peer already dialing");
                continue;
            }

            if let Some(addrs) = self.peer_registry.get_known_addresses(&peer_id) {
                if let Some(addr) = addrs.first() {
                    self.peer_discovery_source
                        .insert(peer_id, DiscoverySource::Bootstrap);
                    self.last_dial_attempt.insert(peer_id, StdInstant::now());

                    info!(peer_id = %peer_id, address = %addr, "Dialing bootstrap peer");

                    match self.swarm.dial(addr.clone()) {
                        Ok(_) => {
                            self.dialing_peers.insert(peer_id);
                            attempts += 1;
                        }
                        Err(e) => {
                            warn!(peer_id = %peer_id, error = %e, "Failed to dial bootstrap peer");
                            failures += 1;
                        }
                    }
                }
            } else {
                warn!(peer_id = %peer_id, "No known addresses for bootstrap peer");
                failures += 1;
            }
        }

        info!(
            dial_attempts = attempts,
            failures = failures,
            "Bootstrap peer dialing complete"
        );

        (attempts, failures)
    }

    /// Initiate Kademlia DHT bootstrap query
    fn initiate_kademlia_bootstrap(&mut self) -> bool {
        if !self.bootstrap_config.auto_bootstrap {
            return false;
        }

        match self.swarm.behaviour_mut().bootstrap() {
            Ok(query_id) => {
                info!(query_id = ?query_id, "Kademlia bootstrap query initiated");
                true
            }
            Err(e) => {
                warn!(error = ?e, "Kademlia bootstrap failed - no known peers in routing table");
                false
            }
        }
    }

    // ==================== Bootstrap Retry Logic ====================

    /// Check if a peer is a bootstrap peer
    fn _is_bootstrap_peer(&self, peer_id: &PeerId) -> bool {
        self.bootstrap_peer_ids.contains(peer_id)
    }

    /// Retry disconnected bootstrap peers with exponential backoff
    pub(crate) fn retry_bootstrap_peers(&mut self) {
        let now = StdInstant::now();
        let bootstrap_ids: Vec<PeerId> = self.bootstrap_peer_ids.iter().copied().collect();

        for peer_id in bootstrap_ids {
            if self.should_retry_bootstrap_peer(&peer_id, now) {
                self.retry_single_bootstrap_peer(peer_id, now);
            }
        }
    }

    /// Check if we should retry connecting to a bootstrap peer
    fn should_retry_bootstrap_peer(&self, peer_id: &PeerId, now: StdInstant) -> bool {
        // Skip if connected or dialing
        if self.peer_registry.is_connected(peer_id) || self.dialing_peers.contains(peer_id) {
            return false;
        }

        // Check retry count against max
        let attempts = self.peer_registry.get_reconnection_count(peer_id);
        if attempts >= self.bootstrap_config.max_retries {
            return false;
        }

        // Check backoff timing
        let backoff = self.calculate_backoff(attempts);
        self.last_dial_attempt
            .get(peer_id)
            .map(|last| now.duration_since(*last) >= backoff)
            .unwrap_or(true)
    }

    /// Calculate exponential backoff duration: base_delay * 2^attempts
    fn calculate_backoff(&self, attempts: u32) -> Duration {
        let exponent = attempts.min(10); // Cap to prevent overflow
        let multiplier = 1u64 << exponent;
        let ms = self
            .bootstrap_config
            .retry_delay_base_ms
            .saturating_mul(multiplier);
        Duration::from_millis(ms)
    }

    /// Retry connecting to a single bootstrap peer
    fn retry_single_bootstrap_peer(&mut self, peer_id: PeerId, now: StdInstant) {
        let Some(addrs) = self.peer_registry.get_known_addresses(&peer_id) else {
            return;
        };

        let Some(addr) = addrs.first() else {
            return;
        };

        let attempts = self.peer_registry.get_reconnection_count(&peer_id);
        let next_backoff = self.calculate_backoff(attempts + 1);

        info!(
            peer_id = %peer_id,
            attempt = attempts + 1,
            max_retries = self.bootstrap_config.max_retries,
            next_backoff_ms = next_backoff.as_millis(),
            "Retrying bootstrap peer connection"
        );

        self.last_dial_attempt.insert(peer_id, now);
        self.peer_discovery_source
            .insert(peer_id, DiscoverySource::Bootstrap);

        if self.swarm.dial(addr.clone()).is_ok() {
            self.dialing_peers.insert(peer_id);
        }
    }
}
