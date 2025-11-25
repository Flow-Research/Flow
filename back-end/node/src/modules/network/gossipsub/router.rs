use crate::modules::network::gossipsub::MessageError;

use super::message::Message;
use super::store::{MessageStore, StoreError};
use super::subscription::TopicSubscriptionManager;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

/// Statistics for the message router
#[derive(Debug, Clone, Default)]
pub struct RouterStats {
    pub messages_received: u64,
    pub messages_persisted: u64,
    pub messages_delivered: u64,
    pub messages_dropped_expired: u64,
    pub messages_dropped_invalid: u64,
}

/// Message router that persists messages and routes to subscribers
pub struct MessageRouter {
    /// Persistent message store
    store: Arc<MessageStore>,

    /// Per-topic subscription manager
    subscriptions: Arc<TopicSubscriptionManager>,

    /// Router statistics
    stats: Arc<RwLock<RouterStats>>,

    /// Maximum message size for validation
    max_message_size: usize,

    /// Cleanup task handle
    cleanup_handle: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for MessageRouter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MessageRouter")
            .field("max_message_size", &self.max_message_size)
            .field("cleanup_handle", &self.cleanup_handle.is_some())
            .finish_non_exhaustive()
    }
}

impl MessageRouter {
    pub fn new(
        store: Arc<MessageStore>,
        subscriptions: Arc<TopicSubscriptionManager>,
        max_message_size: usize,
    ) -> Self {
        Self {
            store,
            subscriptions,
            stats: Arc::new(RwLock::new(RouterStats::default())),
            max_message_size,
            cleanup_handle: None,
        }
    }

    /// Start background cleanup task
    pub fn start_cleanup(&mut self, interval: Duration) {
        let store = Arc::clone(&self.store);

        let handle = tokio::spawn(async move {
            let mut interval_timer = tokio::time::interval(interval);

            loop {
                interval_timer.tick().await;

                match store.cleanup() {
                    Ok(count) if count > 0 => {
                        info!(deleted = count, "Message cleanup completed");
                    }
                    Err(e) => {
                        error!(error = %e, "Message cleanup failed");
                    }
                    _ => {}
                }
            }
        });

        self.cleanup_handle = Some(handle);
    }

    /// Stop cleanup task
    pub fn stop_cleanup(&mut self) {
        if let Some(handle) = self.cleanup_handle.take() {
            handle.abort();
        }
    }

    /// Route an incoming message: validate, persist, and deliver
    pub async fn route(&self, message: Message) -> Result<bool, StoreError> {
        // Update received count
        {
            let mut stats = self.stats.write().await;
            stats.messages_received += 1;
        }

        // Validate message
        if let Err(e) = message.validate(self.max_message_size) {
            warn!(
                message_id = %message.id,
                error = %e,
                "Message validation failed"
            );
            let mut stats = self.stats.write().await;
            match e {
                MessageError::Expired => {
                    stats.messages_dropped_expired += 1;
                }
                _ => {
                    stats.messages_dropped_invalid += 1;
                }
            }
            return Ok(false);
        }

        // Check expiry
        if message.is_expired() {
            debug!(
                message_id = %message.id,
                "Dropping expired message"
            );
            let mut stats = self.stats.write().await;
            stats.messages_dropped_expired += 1;
            return Ok(false);
        }

        // Persist message (even if no current subscribers)
        if let Err(e) = self.store.store(&message) {
            error!(
                message_id = %message.id,
                error = %e,
                "Failed to persist message"
            );
            // Continue to try delivery even if persistence fails
        } else {
            let mut stats = self.stats.write().await;
            stats.messages_persisted += 1;
        }

        // Route to subscribers
        let delivered = self.subscriptions.route(message.clone()).await;

        if delivered {
            let mut stats = self.stats.write().await;
            stats.messages_delivered += 1;
        }

        debug!(
            message_id = %message.id,
            topic = %message.topic,
            delivered = delivered,
            "Message routed"
        );

        Ok(delivered)
    }

    /// Get historical messages for a topic (useful for new subscribers)
    pub async fn get_history(
        &self,
        topic: &str,
        since: Option<u64>,
        limit: usize,
    ) -> Result<Vec<Message>, StoreError> {
        self.store.get_by_topic(topic, since, limit)
    }

    /// Get router statistics
    pub async fn stats(&self) -> RouterStats {
        self.stats.read().await.clone()
    }

    /// Get subscription manager reference
    pub fn subscriptions(&self) -> &Arc<TopicSubscriptionManager> {
        &self.subscriptions
    }

    /// Get store reference
    pub fn store(&self) -> &Arc<MessageStore> {
        &self.store
    }
}

impl Drop for MessageRouter {
    fn drop(&mut self) {
        self.stop_cleanup();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::network::gossipsub::{MessagePayload, MessageStoreConfig};
    use libp2p::identity::Keypair;
    use tempfile::TempDir;

    fn test_peer_id() -> libp2p::PeerId {
        Keypair::generate_ed25519().public().to_peer_id()
    }

    fn create_router(temp_dir: &TempDir) -> MessageRouter {
        let store =
            Arc::new(MessageStore::new(temp_dir.path(), MessageStoreConfig::default()).unwrap());
        let subscriptions = Arc::new(TopicSubscriptionManager::default());
        MessageRouter::new(store, subscriptions, 65536)
    }

    #[tokio::test]
    async fn test_route_and_persist() {
        let temp_dir = TempDir::new().unwrap();
        let router = create_router(&temp_dir);

        let msg = Message::new(
            test_peer_id(),
            "/flow/v1/test",
            MessagePayload::Ping { nonce: 123 },
        );

        let result = router.route(msg.clone()).await;
        assert!(result.is_ok());

        // Message should be persisted
        let stored = router.store.get(&msg.id).unwrap();
        assert!(stored.is_some());

        // Stats updated
        let stats = router.stats().await;
        assert_eq!(stats.messages_received, 1);
        assert_eq!(stats.messages_persisted, 1);
    }

    #[tokio::test]
    async fn test_route_and_deliver() {
        let temp_dir = TempDir::new().unwrap();
        let router = create_router(&temp_dir);
        let topic = "/flow/v1/test";

        // Subscribe first
        let mut handle = router.subscriptions.subscribe(topic).await;

        let msg = Message::new(test_peer_id(), topic, MessagePayload::Ping { nonce: 456 });

        let delivered = router.route(msg.clone()).await.unwrap();
        assert!(delivered);

        // Should receive via subscription
        let received = handle.try_recv().unwrap();
        assert_eq!(received.id, msg.id);

        // Stats updated
        let stats = router.stats().await;
        assert_eq!(stats.messages_delivered, 1);
    }

    #[tokio::test]
    async fn test_expired_message_dropped() {
        let temp_dir = TempDir::new().unwrap();
        let router = create_router(&temp_dir);

        let mut msg = Message::new(test_peer_id(), "/test", MessagePayload::Ping { nonce: 1 });
        msg.timestamp = 0; // Very old
        msg.ttl = 1;

        let result = router.route(msg).await.unwrap();
        assert!(!result);

        let stats = router.stats().await;
        assert_eq!(stats.messages_dropped_expired, 1);
    }

    #[tokio::test]
    async fn test_get_history() {
        let temp_dir = TempDir::new().unwrap();
        let router = create_router(&temp_dir);
        let topic = "/flow/v1/test";

        // Route several messages
        for i in 0..5 {
            let msg = Message::new(test_peer_id(), topic, MessagePayload::Ping { nonce: i });
            router.route(msg).await.unwrap();
        }

        // Get history
        let history = router.get_history(topic, None, 10).await.unwrap();
        assert_eq!(history.len(), 5);
    }
}
