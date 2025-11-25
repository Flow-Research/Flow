use super::message::Message;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast};
use tracing::{debug, info};

/// Configuration for subscription manager
#[derive(Debug, Clone)]
pub struct SubscriptionConfig {
    /// Buffer size for each topic channel
    pub channel_buffer_size: usize,

    /// Maximum subscribers per topic
    pub max_subscribers_per_topic: usize,
}

impl Default for SubscriptionConfig {
    fn default() -> Self {
        Self {
            channel_buffer_size: 1000,
            max_subscribers_per_topic: 100,
        }
    }
}

/// Handle for receiving messages from a subscribed topic
pub struct SubscriptionHandle {
    pub topic: String,
    pub receiver: broadcast::Receiver<Message>,
}

impl SubscriptionHandle {
    /// Receive next message (async)
    pub async fn recv(&mut self) -> Result<Message, broadcast::error::RecvError> {
        self.receiver.recv().await
    }

    /// Try to receive without blocking
    pub fn try_recv(&mut self) -> Result<Message, broadcast::error::TryRecvError> {
        self.receiver.try_recv()
    }
}

/// Per-topic subscription channel
struct TopicChannel {
    sender: broadcast::Sender<Message>,
    subscriber_count: usize,
}

/// Manages per-topic subscription channels
pub struct TopicSubscriptionManager {
    /// Per-topic broadcast channels
    channels: Arc<RwLock<HashMap<String, TopicChannel>>>,

    /// Configuration
    config: SubscriptionConfig,
}

impl std::fmt::Debug for TopicSubscriptionManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TopicSubscriptionManager")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl TopicSubscriptionManager {
    pub fn new(config: SubscriptionConfig) -> Self {
        Self {
            channels: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    /// Subscribe to a topic, returns a handle for receiving messages
    pub async fn subscribe(&self, topic: impl Into<String>) -> SubscriptionHandle {
        let topic = topic.into();
        let mut channels = self.channels.write().await;

        let receiver = if let Some(channel) = channels.get_mut(&topic) {
            channel.subscriber_count += 1;
            debug!(
                topic = %topic,
                subscribers = channel.subscriber_count,
                "Added subscriber to existing topic channel"
            );
            channel.sender.subscribe()
        } else {
            // Create new channel for this topic
            let (sender, receiver) = broadcast::channel(self.config.channel_buffer_size);
            channels.insert(
                topic.clone(),
                TopicChannel {
                    sender,
                    subscriber_count: 1,
                },
            );
            info!(topic = %topic, "Created new topic channel");
            receiver
        };

        SubscriptionHandle { topic, receiver }
    }

    /// Unsubscribe from a topic (called when handle is dropped or explicitly)
    pub async fn unsubscribe(&self, topic: &str) {
        let mut channels = self.channels.write().await;

        if let Some(channel) = channels.get_mut(topic) {
            channel.subscriber_count = channel.subscriber_count.saturating_sub(1);

            if channel.subscriber_count == 0 {
                channels.remove(topic);
                info!(topic = %topic, "Removed empty topic channel");
            } else {
                debug!(
                    topic = %topic,
                    subscribers = channel.subscriber_count,
                    "Decremented subscriber count"
                );
            }
        }
    }

    /// Route a message to its topic channel
    /// Returns true if message was delivered to at least one subscriber
    pub async fn route(&self, message: Message) -> bool {
        let channels = self.channels.read().await;

        if let Some(channel) = channels.get(&message.topic) {
            match channel.sender.send(message.clone()) {
                Ok(count) => {
                    debug!(
                        topic = %message.topic,
                        message_id = %message.id,
                        receivers = count,
                        "Message routed to subscribers"
                    );
                    true
                }
                Err(_) => {
                    // No active receivers (all dropped)
                    debug!(
                        topic = %message.topic,
                        message_id = %message.id,
                        "No active receivers for topic"
                    );
                    false
                }
            }
        } else {
            debug!(
                topic = %message.topic,
                message_id = %message.id,
                "No subscribers for topic"
            );
            false
        }
    }

    /// Check if a topic has subscribers
    pub async fn has_subscribers(&self, topic: &str) -> bool {
        let channels = self.channels.read().await;
        channels.contains_key(topic)
    }

    /// Get subscriber count for a topic
    pub async fn subscriber_count(&self, topic: &str) -> usize {
        let channels = self.channels.read().await;
        channels.get(topic).map(|c| c.subscriber_count).unwrap_or(0)
    }

    /// Get all subscribed topics
    pub async fn subscribed_topics(&self) -> Vec<String> {
        let channels = self.channels.read().await;
        channels.keys().cloned().collect()
    }
}

impl Default for TopicSubscriptionManager {
    fn default() -> Self {
        Self::new(SubscriptionConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::network::gossipsub::MessagePayload;
    use libp2p::identity::Keypair;

    fn test_peer_id() -> libp2p::PeerId {
        Keypair::generate_ed25519().public().to_peer_id()
    }

    #[tokio::test]
    async fn test_subscribe_creates_channel() {
        let manager = TopicSubscriptionManager::default();

        let _handle = manager.subscribe("/flow/v1/test").await;

        assert!(manager.has_subscribers("/flow/v1/test").await);
        assert_eq!(manager.subscriber_count("/flow/v1/test").await, 1);
    }

    #[tokio::test]
    async fn test_multiple_subscribers() {
        let manager = TopicSubscriptionManager::default();
        let topic = "/flow/v1/test";

        let _h1 = manager.subscribe(topic).await;
        let _h2 = manager.subscribe(topic).await;
        let _h3 = manager.subscribe(topic).await;

        assert_eq!(manager.subscriber_count(topic).await, 3);
    }

    #[tokio::test]
    async fn test_unsubscribe() {
        let manager = TopicSubscriptionManager::default();
        let topic = "/flow/v1/test";

        let _h1 = manager.subscribe(topic).await;
        let _h2 = manager.subscribe(topic).await;

        manager.unsubscribe(topic).await;
        assert_eq!(manager.subscriber_count(topic).await, 1);

        manager.unsubscribe(topic).await;
        assert!(!manager.has_subscribers(topic).await);
    }

    #[tokio::test]
    async fn test_route_message() {
        let manager = TopicSubscriptionManager::default();
        let topic = "/flow/v1/test";

        let mut handle = manager.subscribe(topic).await;

        let msg = Message::new(test_peer_id(), topic, MessagePayload::Ping { nonce: 123 });

        let routed = manager.route(msg.clone()).await;
        assert!(routed);

        let received = handle.try_recv().unwrap();
        assert_eq!(received.id, msg.id);
    }

    #[tokio::test]
    async fn test_route_to_multiple_subscribers() {
        let manager = TopicSubscriptionManager::default();
        let topic = "/flow/v1/test";

        let mut h1 = manager.subscribe(topic).await;
        let mut h2 = manager.subscribe(topic).await;

        let msg = Message::new(test_peer_id(), topic, MessagePayload::Ping { nonce: 456 });

        manager.route(msg.clone()).await;

        // Both should receive
        assert_eq!(h1.try_recv().unwrap().id, msg.id);
        assert_eq!(h2.try_recv().unwrap().id, msg.id);
    }

    #[tokio::test]
    async fn test_route_to_wrong_topic() {
        let manager = TopicSubscriptionManager::default();

        let _handle = manager.subscribe("/flow/v1/topic-a").await;

        let msg = Message::new(
            test_peer_id(),
            "/flow/v1/topic-b",
            MessagePayload::Ping { nonce: 1 },
        );

        let routed = manager.route(msg).await;
        assert!(!routed); // No subscribers for topic-b
    }
}
