//! Provider handler - DHT provider announcement, discovery, and auto-reprovide

use crate::modules::network::content_transfer::ContentTransferError;
use crate::modules::network::manager::ProviderConfirmationCallback;
use crate::modules::network::manager::event_loop::NetworkEventLoop;
use crate::modules::network::manager::types::{PendingProviderAnnouncement, PendingProviderQuery};
use crate::modules::storage::content::ContentId;
use libp2p::PeerId;
use libp2p::kad::{QueryId, RecordKey};
use libp2p::request_response::OutboundRequestId;
use std::collections::HashSet;
use tokio::sync::oneshot;
use tracing::{debug, error, info, warn};

impl NetworkEventLoop {
    /// Convert a ContentId to a Kademlia RecordKey.
    ///
    /// Uses the raw CID bytes as the key, ensuring content-addressable
    /// lookups work correctly across the DHT.
    pub(crate) fn cid_to_record_key(cid: &ContentId) -> RecordKey {
        RecordKey::new(&cid.to_bytes())
    }

    /// Handle get_providers command - initiate DHT provider query
    pub(crate) fn handle_get_providers(
        &mut self,
        cid: ContentId,
        response: oneshot::Sender<Result<HashSet<PeerId>, String>>,
    ) {
        let key = Self::cid_to_record_key(&cid);

        debug!(%cid, key = ?key, "Starting provider discovery query");

        let query_id = self.swarm.behaviour_mut().get_providers(key);

        info!(
            %cid,
            ?query_id,
            "Provider discovery query initiated"
        );

        self.pending_provider_queries.insert(
            query_id,
            PendingProviderQuery {
                response_tx: response,
                cid,
                providers: HashSet::new(),
                created_at: std::time::Instant::now(),
            },
        );
    }

    pub(crate) fn reprovide_all(&mut self) {
        let cids = self.provider_registry.get_all();
        let cid_count = cids.len();

        if cid_count == 0 {
            debug!("No content to reprovide");
            return;
        }

        info!(cid_count, "Starting auto-reprovide cycle");

        let mut success_count = 0;
        let mut error_count = 0;

        for cid in cids {
            let key = Self::cid_to_record_key(&cid);

            match self.swarm.behaviour_mut().start_providing(key) {
                Ok(query_id) => {
                    debug!(%cid, ?query_id, "Reprovide initiated");
                    success_count += 1;
                }
                Err(e) => {
                    warn!(%cid, error = ?e, "Failed to reprovide");
                    error_count += 1;
                }
            }
        }

        info!(
            cid_count,
            success_count, error_count, "Auto-reprovide cycle completed"
        );
    }

    /// Clean up stale pending queries.
    pub(crate) fn cleanup_stale_queries(&mut self) {
        let timeout = self.provider_config.query_timeout();
        let now = std::time::Instant::now();

        // Cleanup stale get_providers queries
        let stale_provider_queries: Vec<QueryId> = self
            .pending_provider_queries
            .iter()
            .filter(|(_, pending)| now.duration_since(pending.created_at) > timeout)
            .map(|(id, _)| *id)
            .collect();

        for query_id in &stale_provider_queries {
            if let Some(pending) = self.pending_provider_queries.remove(query_id) {
                let elapsed = now.duration_since(pending.created_at);
                warn!(
                    ?query_id,
                    %pending.cid,
                    elapsed_secs = elapsed.as_secs(),
                    "Cleaning up stale get_providers query"
                );
                let _ = pending.response_tx.send(Err(format!(
                    "Provider query timed out for {} after {}s",
                    pending.cid,
                    elapsed.as_secs()
                )));
            }
        }

        // Cleanup stale provider announcement queries
        let stale_announcements: Vec<QueryId> = self
            .pending_provider_announcements
            .iter()
            .filter(|(_, pending)| now.duration_since(pending.created_at) > timeout)
            .map(|(id, _)| *id)
            .collect();

        for query_id in &stale_announcements {
            if let Some(pending) = self.pending_provider_announcements.remove(query_id) {
                let elapsed = now.duration_since(pending.created_at);
                warn!(
                    ?query_id,
                    %pending.cid,
                    elapsed_secs = elapsed.as_secs(),
                    "Cleaning up stale provider announcement"
                );
                if let Some(callback) = pending.callback {
                    callback(
                        pending.cid,
                        Err(format!(
                            "Provider announcement timed out after {}s",
                            elapsed.as_secs()
                        )),
                    );
                }
            }
        }

        // Cleanup stale content transfer outbound requests, if you track them similarly:
        let stale_content_requests: Vec<OutboundRequestId> = self
            .pending_content_requests
            .iter()
            .filter(|(_, pending)| now.duration_since(pending.created_at) > timeout)
            .map(|(id, _)| *id)
            .collect();

        for request_id in &stale_content_requests {
            if let Some(pending) = self.pending_content_requests.remove(request_id) {
                warn!(
                    ?request_id,
                    %pending.cid,
                    peer = %pending.peer_id,
                    "Cleaning up stale content request"
                );
                let _ = pending
                    .response_tx
                    .send(Err(ContentTransferError::RequestFailed {
                        message: format!("Request timed out after {}s", timeout.as_secs()),
                    }));
            }
        }

        let total_cleaned =
            stale_provider_queries.len() + stale_announcements.len() + stale_content_requests.len();

        if total_cleaned > 0 {
            info!(
                provider_queries = stale_provider_queries.len(),
                announcements = stale_announcements.len(),
                content_requests = stale_content_requests.len(),
                "Cleaned up stale queries"
            );
        }
    }

    /// Handle start_providing command (no callback).
    pub(crate) fn handle_start_providing(&mut self, cid: &ContentId) -> Result<QueryId, String> {
        self.handle_start_providing_impl(cid, None)
    }

    /// Handle start_providing command with optional callback.
    pub(crate) fn handle_start_providing_impl(
        &mut self,
        cid: &ContentId,
        callback: Option<ProviderConfirmationCallback>,
    ) -> Result<QueryId, String> {
        let key = Self::cid_to_record_key(cid);
        debug!(%cid, key = ?key, "Starting to provide content");

        match self.swarm.behaviour_mut().start_providing(key.clone()) {
            Ok(query_id) => {
                info!(%cid, ?query_id, "Provider announcement query started");

                // Add to persistent provider registry
                if let Err(e) = self.provider_registry.add(cid.clone()) {
                    error!(%cid, error = %e, "Failed to persist provided CID");
                    // DHT announcement still proceeds
                }

                // Track for callback if provided
                if callback.is_some() {
                    self.pending_provider_announcements.insert(
                        query_id,
                        PendingProviderAnnouncement {
                            cid: cid.clone(),
                            callback,
                            created_at: std::time::Instant::now(),
                        },
                    );
                    debug!(%cid, ?query_id, "Registered callback for provider announcement");
                }

                debug!(
                    %cid,
                    total_provided = self.provider_registry.count(),
                    "Added to provider registry"
                );

                Ok(query_id)
            }
            Err(e) => {
                warn!(%cid, error = ?e, "Failed to start providing");
                Err(format!("Failed to start providing: {:?}", e))
            }
        }
    }

    /// Handle stop_providing command.
    pub(crate) fn handle_stop_providing(&mut self, cid: &ContentId) -> Result<(), String> {
        let key = Self::cid_to_record_key(cid);
        debug!(%cid, key = ?key, "Stopping providing content");

        self.swarm.behaviour_mut().stop_providing(&key);

        // Remove from persistent provider registry
        match self.provider_registry.remove(cid) {
            Ok(was_present) => {
                info!(
                    %cid,
                    was_present,
                    total_provided = self.provider_registry.count(),
                    "Stopped providing content"
                );
            }
            Err(e) => {
                error!(%cid, error = %e, "Failed to remove from provider registry");
            }
        }

        Ok(())
    }
}
