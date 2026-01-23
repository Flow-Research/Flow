//! GossipSub event handler - message receiving and subscription events

use super::NetworkEventLoop;
use crate::modules::network::gossipsub::Message;
use libp2p::gossipsub;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

impl NetworkEventLoop {
    /// Handle GossipSub events
    pub(crate) async fn handle_gossipsub_event(&mut self, event: gossipsub::Event) {
        match event {
            gossipsub::Event::Message {
                propagation_source,
                message_id,
                message,
            } => {
                debug!(
                    source = %propagation_source,
                    message_id = %message_id,
                    topic = %message.topic,
                    data_len = message.data.len(),
                    "GossipSub message received"
                );

                // Deserialize and validate
                match Message::deserialize(&message.data) {
                    Ok(flow_msg) => {
                        // Validate message
                        if let Err(e) = flow_msg.validate(self.gossipsub_config.max_transmit_size) {
                            warn!(
                                message_id = %message_id,
                                error = %e,
                                "Message validation failed"
                            );
                            return;
                        }

                        // Check expiry
                        if flow_msg.is_expired() {
                            debug!(message_id = %message_id, "Dropping expired message");
                            return;
                        }

                        info!(
                            message_id = %flow_msg.id,
                            topic = %flow_msg.topic,
                            source = %flow_msg.source,
                            payload_type = ?std::mem::discriminant(&flow_msg.payload),
                            "Valid message received, routing to subscribers"
                        );

                        // Route via MessageRouter (persists + delivers to subscribers)
                        let router = Arc::clone(&self.message_router);
                        let msg_id = flow_msg.id.clone();
                        let msg_topic = flow_msg.topic.clone();

                        tokio::spawn(async move {
                            match router.route(flow_msg).await {
                                Ok(delivered) => {
                                    if delivered {
                                        debug!(
                                            message_id = %msg_id,
                                            topic = %msg_topic,
                                            "Message delivered to subscribers"
                                        );
                                    } else {
                                        debug!(
                                            message_id = %msg_id,
                                            topic = %msg_topic,
                                            "Message persisted but no active subscribers"
                                        );
                                    }
                                }
                                Err(e) => {
                                    error!(
                                        message_id = %msg_id,
                                        error = %e,
                                        "Failed to route message"
                                    );
                                }
                            }
                        });
                    }
                    Err(e) => {
                        warn!(
                            message_id = %message_id,
                            error = %e,
                            "Failed to deserialize message"
                        );
                    }
                }
            }

            gossipsub::Event::Subscribed { peer_id, topic } => {
                info!(peer_id = %peer_id, topic = %topic, "Peer subscribed to topic");
            }

            gossipsub::Event::Unsubscribed { peer_id, topic } => {
                info!(peer_id = %peer_id, topic = %topic, "Peer unsubscribed from topic");
            }

            gossipsub::Event::GossipsubNotSupported { peer_id } => {
                debug!(peer_id = %peer_id, "Peer does not support GossipSub");
            }

            _ => {}
        }
    }
}
