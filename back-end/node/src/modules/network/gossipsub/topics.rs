use libp2p::gossipsub::{IdentTopic, TopicHash as GossipTopicHash};
use std::fmt;

/// Well-known topic prefixes for Flow network
pub const TOPIC_PREFIX: &str = "/flow/v1";

/// System topic constants for direct use
pub const SYSTEM_NETWORK_EVENTS_TOPIC: &str = "/flow/v1/system/network-events";

/// Pre-defined Flow topics
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Topic {
    /// updates for a specific space
    Updates(String),

    /// Global task announcements
    TaskAnnouncements,

    /// Agent status broadcasts
    AgentStatus,

    /// Compute marketplace offers
    ComputeOffers,

    /// Network-wide announcements
    Announcements,

    /// Custom topic
    Custom(String),
}

impl Topic {
    /// Get the full topic string
    pub fn to_topic_string(&self) -> String {
        match self {
            Topic::Updates(space_key) => {
                format!("{}/updates/{}", TOPIC_PREFIX, space_key)
            }
            Topic::TaskAnnouncements => format!("{}/tasks", TOPIC_PREFIX),
            Topic::AgentStatus => format!("{}/agents/status", TOPIC_PREFIX),
            Topic::ComputeOffers => format!("{}/compute/offers", TOPIC_PREFIX),
            Topic::Announcements => format!("{}/announcements", TOPIC_PREFIX),
            Topic::Custom(name) => format!("{}/custom/{}", TOPIC_PREFIX, name),
        }
    }

    /// Convert to libp2p IdentTopic
    pub fn to_ident_topic(&self) -> IdentTopic {
        IdentTopic::new(self.to_topic_string())
    }

    /// Get the topic hash
    pub fn hash(&self) -> TopicHash {
        TopicHash(self.to_ident_topic().hash())
    }

    /// Parse a topic string back to Topic
    pub fn from_topic_string(s: &str) -> Option<Self> {
        if !s.starts_with(TOPIC_PREFIX) {
            return None;
        }

        let suffix = &s[TOPIC_PREFIX.len()..];

        if suffix.starts_with("/updates/") {
            let space_key = suffix.strip_prefix("/updates/")?;
            return Some(Topic::Updates(space_key.to_string()));
        }

        match suffix {
            "/tasks" => Some(Topic::TaskAnnouncements),
            "/agents/status" => Some(Topic::AgentStatus),
            "/compute/offers" => Some(Topic::ComputeOffers),
            "/announcements" => Some(Topic::Announcements),
            s if s.starts_with("/custom/") => {
                let name = s.strip_prefix("/custom/")?;
                Some(Topic::Custom(name.to_string()))
            }
            _ => None,
        }
    }
}

impl fmt::Display for Topic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_topic_string())
    }
}

/// Wrapper around GossipSub topic hash
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TopicHash(pub GossipTopicHash);

impl TopicHash {
    pub fn inner(&self) -> &GossipTopicHash {
        &self.0
    }
}

impl From<GossipTopicHash> for TopicHash {
    fn from(hash: GossipTopicHash) -> Self {
        TopicHash(hash)
    }
}

impl fmt::Display for TopicHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_topic_strings() {
        assert_eq!(Topic::TaskAnnouncements.to_topic_string(), "/flow/v1/tasks");
        assert_eq!(
            Topic::AgentStatus.to_topic_string(),
            "/flow/v1/agents/status"
        );
        assert_eq!(
            Topic::Updates("my-space".to_string()).to_topic_string(),
            "/flow/v1/updates/my-space"
        );
        assert_eq!(
            Topic::Custom("test".to_string()).to_topic_string(),
            "/flow/v1/custom/test"
        );
    }

    #[test]
    fn test_topic_parsing() {
        assert_eq!(
            Topic::from_topic_string("/flow/v1/tasks"),
            Some(Topic::TaskAnnouncements)
        );
        assert_eq!(
            Topic::from_topic_string("/flow/v1/updates/space-123"),
            Some(Topic::Updates("space-123".to_string()))
        );
        assert_eq!(
            Topic::from_topic_string("/flow/v1/custom/mychannel"),
            Some(Topic::Custom("mychannel".to_string()))
        );
        assert_eq!(Topic::from_topic_string("/invalid"), None);
    }

    #[test]
    fn test_topic_hash_uniqueness() {
        let topic1 = Topic::TaskAnnouncements;
        let topic2 = Topic::AgentStatus;

        assert_ne!(topic1.hash(), topic2.hash());
    }

    #[test]
    fn test_topic_hash_consistency() {
        let topic = Topic::TaskAnnouncements;
        let hash1 = topic.hash();
        let hash2 = topic.hash();

        assert_eq!(hash1, hash2);
    }
}
