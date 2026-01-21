//! Configuration for distributed search.
//!
//! Provides configurable settings for network query behavior, caching,
//! and opt-in query response handling.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Default maximum results per query response.
fn default_max_results_per_query() -> u32 {
    20
}

/// Default number of peers to query.
fn default_peer_count() -> u32 {
    15
}

/// Default network timeout in milliseconds.
fn default_network_timeout_ms() -> u64 {
    500
}

/// Default retry enabled.
fn default_retry_enabled() -> bool {
    true
}

/// Default cache TTL in seconds.
fn default_cache_ttl_secs() -> u64 {
    15
}

/// Default maximum cache entries.
fn default_cache_max_entries() -> usize {
    1000
}

/// Configuration for distributed search.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchConfig {
    /// Enable responding to network search queries.
    /// Default is `false` (privacy-first).
    #[serde(default)]
    pub respond_to_queries: bool,

    /// Maximum results to return per query response.
    #[serde(default = "default_max_results_per_query")]
    pub max_results_per_query: u32,

    /// Number of peers to query for live search.
    #[serde(default = "default_peer_count")]
    pub peer_count: u32,

    /// Timeout for network queries in milliseconds.
    #[serde(default = "default_network_timeout_ms")]
    pub network_timeout_ms: u64,

    /// Whether to retry failed peer queries once.
    #[serde(default = "default_retry_enabled")]
    pub retry_enabled: bool,

    /// Cache TTL in seconds.
    #[serde(default = "default_cache_ttl_secs")]
    pub cache_ttl_secs: u64,

    /// Maximum cache size in entries.
    #[serde(default = "default_cache_max_entries")]
    pub cache_max_entries: usize,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            respond_to_queries: false, // Privacy-first default
            max_results_per_query: default_max_results_per_query(),
            peer_count: default_peer_count(),
            network_timeout_ms: default_network_timeout_ms(),
            retry_enabled: default_retry_enabled(),
            cache_ttl_secs: default_cache_ttl_secs(),
            cache_max_entries: default_cache_max_entries(),
        }
    }
}

impl SearchConfig {
    /// Create a new configuration with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable or disable query response.
    pub fn with_respond_to_queries(mut self, respond: bool) -> Self {
        self.respond_to_queries = respond;
        self
    }

    /// Set maximum results per query.
    pub fn with_max_results(mut self, max: u32) -> Self {
        self.max_results_per_query = max;
        self
    }

    /// Set number of peers to query.
    pub fn with_peer_count(mut self, count: u32) -> Self {
        self.peer_count = count;
        self
    }

    /// Set network timeout in milliseconds.
    pub fn with_timeout_ms(mut self, timeout_ms: u64) -> Self {
        self.network_timeout_ms = timeout_ms;
        self
    }

    /// Set network timeout as Duration.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.network_timeout_ms = timeout.as_millis() as u64;
        self
    }

    /// Enable or disable retry.
    pub fn with_retry(mut self, retry: bool) -> Self {
        self.retry_enabled = retry;
        self
    }

    /// Set cache TTL in seconds.
    pub fn with_cache_ttl_secs(mut self, ttl: u64) -> Self {
        self.cache_ttl_secs = ttl;
        self
    }

    /// Set cache TTL as Duration.
    pub fn with_cache_ttl(mut self, ttl: Duration) -> Self {
        self.cache_ttl_secs = ttl.as_secs();
        self
    }

    /// Set maximum cache entries.
    pub fn with_cache_size(mut self, size: usize) -> Self {
        self.cache_max_entries = size;
        self
    }

    /// Get the network timeout as a Duration.
    pub fn network_timeout(&self) -> Duration {
        Duration::from_millis(self.network_timeout_ms)
    }

    /// Get the cache TTL as a Duration.
    pub fn cache_ttl(&self) -> Duration {
        Duration::from_secs(self.cache_ttl_secs)
    }

    /// Load configuration from environment variables.
    ///
    /// Environment variables:
    /// - `FLOW_SEARCH_RESPOND_TO_QUERIES`: Enable query responses (true/false)
    /// - `FLOW_SEARCH_MAX_RESULTS_PER_QUERY`: Max results per response
    /// - `FLOW_SEARCH_PEER_COUNT`: Number of peers to query
    /// - `FLOW_SEARCH_NETWORK_TIMEOUT_MS`: Query timeout in ms
    /// - `FLOW_SEARCH_RETRY_ENABLED`: Enable retry (true/false)
    /// - `FLOW_SEARCH_CACHE_TTL_SECS`: Cache TTL in seconds
    /// - `FLOW_SEARCH_CACHE_MAX_ENTRIES`: Max cache entries
    pub fn from_env() -> Self {
        let mut config = Self::default();

        if let Ok(val) = std::env::var("FLOW_SEARCH_RESPOND_TO_QUERIES") {
            config.respond_to_queries = val.to_lowercase() == "true";
        }

        if let Ok(val) = std::env::var("FLOW_SEARCH_MAX_RESULTS_PER_QUERY") {
            if let Ok(n) = val.parse() {
                config.max_results_per_query = n;
            }
        }

        if let Ok(val) = std::env::var("FLOW_SEARCH_PEER_COUNT") {
            if let Ok(n) = val.parse() {
                config.peer_count = n;
            }
        }

        if let Ok(val) = std::env::var("FLOW_SEARCH_NETWORK_TIMEOUT_MS") {
            if let Ok(n) = val.parse() {
                config.network_timeout_ms = n;
            }
        }

        if let Ok(val) = std::env::var("FLOW_SEARCH_RETRY_ENABLED") {
            config.retry_enabled = val.to_lowercase() == "true";
        }

        if let Ok(val) = std::env::var("FLOW_SEARCH_CACHE_TTL_SECS") {
            if let Ok(n) = val.parse() {
                config.cache_ttl_secs = n;
            }
        }

        if let Ok(val) = std::env::var("FLOW_SEARCH_CACHE_MAX_ENTRIES") {
            if let Ok(n) = val.parse() {
                config.cache_max_entries = n;
            }
        }

        config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = SearchConfig::default();

        assert!(!config.respond_to_queries);
        assert_eq!(config.max_results_per_query, 20);
        assert_eq!(config.peer_count, 15);
        assert_eq!(config.network_timeout_ms, 500);
        assert!(config.retry_enabled);
        assert_eq!(config.cache_ttl_secs, 15);
        assert_eq!(config.cache_max_entries, 1000);
    }

    #[test]
    fn test_config_builder() {
        let config = SearchConfig::new()
            .with_respond_to_queries(true)
            .with_max_results(50)
            .with_peer_count(20)
            .with_timeout_ms(1000)
            .with_retry(false)
            .with_cache_ttl_secs(30)
            .with_cache_size(500);

        assert!(config.respond_to_queries);
        assert_eq!(config.max_results_per_query, 50);
        assert_eq!(config.peer_count, 20);
        assert_eq!(config.network_timeout_ms, 1000);
        assert!(!config.retry_enabled);
        assert_eq!(config.cache_ttl_secs, 30);
        assert_eq!(config.cache_max_entries, 500);
    }

    #[test]
    fn test_duration_methods() {
        let config = SearchConfig::new()
            .with_timeout(Duration::from_millis(750))
            .with_cache_ttl(Duration::from_secs(60));

        assert_eq!(config.network_timeout(), Duration::from_millis(750));
        assert_eq!(config.cache_ttl(), Duration::from_secs(60));
    }

    #[test]
    fn test_config_serialization() {
        let config = SearchConfig::new()
            .with_respond_to_queries(true)
            .with_peer_count(10);

        let json = serde_json::to_string(&config).unwrap();
        let parsed: SearchConfig = serde_json::from_str(&json).unwrap();

        assert!(parsed.respond_to_queries);
        assert_eq!(parsed.peer_count, 10);
    }

    #[test]
    fn test_config_deserialization_with_defaults() {
        // Minimal JSON should use defaults for missing fields
        let json = r#"{"respond_to_queries": true}"#;
        let config: SearchConfig = serde_json::from_str(json).unwrap();

        assert!(config.respond_to_queries);
        assert_eq!(config.max_results_per_query, 20); // default
        assert_eq!(config.peer_count, 15); // default
    }
}
