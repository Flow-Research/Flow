//! Content transfer event handler - block/manifest requests and responses

use super::NetworkEventLoop;
use crate::modules::network::content_transfer::{
    ContentRequest, ContentResponse, ContentTransferError, ErrorCode,
};
use crate::modules::network::manager::types::PendingContentRequest;
use crate::modules::storage::content::{Block, ContentId, DocumentManifest};
use libp2p::PeerId;
use libp2p::request_response::{self, OutboundRequestId};
use request_response::Event::*;
use request_response::Message::*;
use tokio::sync::oneshot;
use tracing::{debug, error, info, warn};

impl NetworkEventLoop {
    /// Handle content transfer request-response events
    pub(crate) async fn handle_content_transfer_event(
        &mut self,
        event: request_response::Event<ContentRequest, ContentResponse>,
    ) {
        match event {
            // Incoming request from a peer
            Message {
                peer,
                message: Request {
                    request, channel, ..
                },
                ..
            } => {
                info!(peer = %peer, request = ?request, "Received content request");
                self.handle_incoming_content_request(peer, request, channel)
                    .await;
            }

            // Response to our outbound request
            Message {
                peer,
                message:
                    Response {
                        request_id,
                        response,
                    },
                ..
            } => {
                debug!(peer = %peer, request_id = ?request_id, "Received content response");
                self.handle_content_response(request_id, response);
            }

            // Outbound request failed
            OutboundFailure {
                peer,
                request_id,
                error,
                ..
            } => {
                warn!(
                    peer = %peer,
                    request_id = ?request_id,
                    error = ?error,
                    "Content request failed"
                );
                self.handle_content_request_failure(request_id, error);
            }

            // Inbound request failed (we couldn't send response)
            InboundFailure {
                peer,
                request_id,
                error,
                ..
            } => {
                warn!(
                    peer = %peer,
                    request_id = ?request_id,
                    error = ?error,
                    "Failed to send content response"
                );
            }

            // Response sent successfully
            ResponseSent {
                peer, request_id, ..
            } => {
                debug!(peer = %peer, request_id = ?request_id, "Content response sent");
            }
        }
    }

    /// Handle incoming content request - serve from local BlockStore
    async fn handle_incoming_content_request(
        &mut self,
        peer: PeerId,
        request: ContentRequest,
        channel: request_response::ResponseChannel<ContentResponse>,
    ) {
        let response = match &self.block_store {
            Some(store) => {
                match &request {
                    ContentRequest::GetBlock { cid } => match store.get(cid) {
                        Ok(Some(block)) => {
                            info!(peer = %peer, cid = %cid, "Serving block");
                            ContentResponse::block(
                                cid.clone(),
                                block.data().to_vec(),
                                block.links().to_vec(),
                            )
                        }
                        Ok(None) => {
                            debug!(peer = %peer, cid = %cid, "Block not found");
                            ContentResponse::not_found(cid.clone())
                        }
                        Err(e) => {
                            error!(peer = %peer, cid = %cid, error = %e, "BlockStore error");
                            ContentResponse::error(
                                ErrorCode::InternalError,
                                format!("Storage error: {}", e),
                            )
                        }
                    },
                    ContentRequest::GetManifest { cid } => {
                        match store.get(cid) {
                            Ok(Some(block)) => {
                                // Manifest is stored as DAG-CBOR block
                                info!(peer = %peer, cid = %cid, "Serving manifest");
                                ContentResponse::manifest(cid.clone(), block.data().to_vec())
                            }
                            Ok(None) => {
                                debug!(peer = %peer, cid = %cid, "Manifest not found");
                                ContentResponse::not_found(cid.clone())
                            }
                            Err(e) => {
                                error!(peer = %peer, cid = %cid, error = %e, "BlockStore error");
                                ContentResponse::error(
                                    ErrorCode::InternalError,
                                    format!("Storage error: {}", e),
                                )
                            }
                        }
                    }
                }
            }
            None => {
                warn!(peer = %peer, "No BlockStore configured, cannot serve content");
                ContentResponse::error(
                    ErrorCode::InternalError,
                    "Content storage not available".to_string(),
                )
            }
        };

        if let Err(resp) = self
            .swarm
            .behaviour_mut()
            .send_content_response(channel, response)
        {
            error!(peer = %peer, "Failed to send content response: {:?}", resp);
        }
    }

    /// Handle successful content response
    fn handle_content_response(
        &mut self,
        request_id: OutboundRequestId,
        response: ContentResponse,
    ) {
        if let Some(pending) = self.pending_content_requests.remove(&request_id) {
            let _ = pending.response_tx.send(Ok(response));
        } else {
            warn!(request_id = ?request_id, "Received response for unknown request");
        }
    }

    /// Handle content request failure
    fn handle_content_request_failure(
        &mut self,
        request_id: OutboundRequestId,
        error: request_response::OutboundFailure,
    ) {
        if let Some(pending) = self.pending_content_requests.remove(&request_id) {
            let err = match error {
                request_response::OutboundFailure::DialFailure => {
                    ContentTransferError::PeerDisconnected {
                        peer_id: pending.peer_id.to_string(),
                    }
                }
                request_response::OutboundFailure::Timeout => ContentTransferError::Timeout {
                    timeout_secs: self.content_transfer_config.request_timeout_secs,
                },
                request_response::OutboundFailure::ConnectionClosed => {
                    ContentTransferError::PeerDisconnected {
                        peer_id: pending.peer_id.to_string(),
                    }
                }
                request_response::OutboundFailure::UnsupportedProtocols => {
                    ContentTransferError::RequestFailed {
                        message: "Peer does not support content transfer protocol".to_string(),
                    }
                }
                request_response::OutboundFailure::Io(e) => ContentTransferError::RequestFailed {
                    message: format!("I/O error: {}", e),
                },
            };
            let _ = pending.response_tx.send(Err(err));
        }
    }

    pub fn handle_request_block(
        &mut self,
        peer_id: PeerId,
        cid: ContentId,
        response_tx: oneshot::Sender<Result<Block, ContentTransferError>>,
    ) {
        let request = ContentRequest::get_block(cid.clone());

        // Wrap the block response sender in a ContentResponse handler
        let (content_tx, content_rx) = oneshot::channel();

        // Spawn a task to convert ContentResponse to Block
        let cid_clone = cid.clone();
        tokio::spawn(async move {
            let result = match content_rx.await {
                Ok(Ok(ContentResponse::Block { data, links, .. })) => {
                    match Block::from_parts(cid_clone.clone(), data, links) {
                        Ok(block) => Ok(block),
                        Err(e) => Err(ContentTransferError::RequestFailed {
                            message: format!("Invalid block data: {}", e),
                        }),
                    }
                }
                Ok(Ok(ContentResponse::NotFound { cid })) => Err(ContentTransferError::NotFound {
                    cid: cid.to_string(),
                }),
                Ok(Ok(ContentResponse::Error { code, message, .. })) => {
                    Err(ContentTransferError::RemoteError {
                        code: code.to_string(),
                        message,
                    })
                }
                Ok(Ok(_)) => Err(ContentTransferError::RequestFailed {
                    message: "Unexpected response type".to_string(),
                }),
                Ok(Err(e)) => Err(e),
                Err(_) => Err(ContentTransferError::Internal {
                    message: "Response channel closed".to_string(),
                }),
            };
            let _ = response_tx.send(result);
        });

        self.send_content_request(peer_id, cid, request, content_tx);
    }

    pub(crate) fn handle_request_manifest(
        &mut self,
        peer_id: PeerId,
        cid: ContentId,
        response_tx: oneshot::Sender<Result<DocumentManifest, ContentTransferError>>,
    ) {
        let request = ContentRequest::get_manifest(cid.clone());

        let (content_tx, content_rx) = oneshot::channel();

        tokio::spawn(async move {
            let result = match content_rx.await {
                Ok(Ok(ContentResponse::Manifest { data, .. })) => {
                    match DocumentManifest::from_dag_cbor(&data) {
                        Ok(manifest) => Ok(manifest),
                        Err(e) => Err(ContentTransferError::RequestFailed {
                            message: format!("Invalid manifest data: {}", e),
                        }),
                    }
                }
                Ok(Ok(ContentResponse::NotFound { cid })) => Err(ContentTransferError::NotFound {
                    cid: cid.to_string(),
                }),
                Ok(Ok(ContentResponse::Error { code, message, .. })) => {
                    Err(ContentTransferError::RemoteError {
                        code: code.to_string(),
                        message,
                    })
                }
                Ok(Ok(_)) => Err(ContentTransferError::RequestFailed {
                    message: "Unexpected response type".to_string(),
                }),
                Ok(Err(e)) => Err(e),
                Err(_) => Err(ContentTransferError::Internal {
                    message: "Response channel closed".to_string(),
                }),
            };
            let _ = response_tx.send(result);
        });

        self.send_content_request(peer_id, cid, request, content_tx);
    }

    fn send_content_request(
        &mut self,
        peer_id: PeerId,
        cid: ContentId,
        request: ContentRequest,
        response_tx: oneshot::Sender<Result<ContentResponse, ContentTransferError>>,
    ) {
        match self
            .swarm
            .behaviour_mut()
            .send_content_request(&peer_id, request)
        {
            Some(request_id) => {
                self.pending_content_requests.insert(
                    request_id,
                    PendingContentRequest {
                        response_tx,
                        cid,
                        peer_id,
                        created_at: std::time::Instant::now(),
                    },
                );
            }
            None => {
                let _ = response_tx.send(Err(ContentTransferError::RequestFailed {
                    message: "Content transfer not enabled".to_string(),
                }));
            }
        }
    }
}
