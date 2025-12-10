//! Configuration for content transfer protocol.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Configuration for content transfer behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentTransferConfig {
    /// Request timeout in seconds.
    pub request_timeout_secs: u64,
    /// Maximum concurrent requests per peer.
    pub max_concurrent_requests: usize,
    /// Number of retry attempts.
    pub max_retries: u32,
    /// Delay between retries in milliseconds.
    pub retry_delay_ms: u64,
    /// Whether content transfer is enabled.
    pub enabled: bool,
}

impl Default for ContentTransferConfig {
    fn default() -> Self {
        Self {
            request_timeout_secs: 30,
            max_concurrent_requests: 64,
            max_retries: 3,
            retry_delay_ms: 1000,
            enabled: true,
        }
    }
}

impl ContentTransferConfig {
    /// Load configuration from environment variables.
    pub fn from_env() -> Self {
        Self {
            request_timeout_secs: std::env::var("CONTENT_TRANSFER_TIMEOUT_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(30),
            max_concurrent_requests: std::env::var("CONTENT_TRANSFER_MAX_CONCURRENT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(64),
            max_retries: std::env::var("CONTENT_TRANSFER_MAX_RETRIES")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(3),
            retry_delay_ms: std::env::var("CONTENT_TRANSFER_RETRY_DELAY_MS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1000),
            enabled: std::env::var("CONTENT_TRANSFER_ENABLED")
                .map(|s| s != "false" && s != "0")
                .unwrap_or(true),
        }
    }

    /// Get request timeout as Duration.
    pub fn request_timeout(&self) -> Duration {
        Duration::from_secs(self.request_timeout_secs)
    }

    /// Get retry delay as Duration.
    pub fn retry_delay(&self) -> Duration {
        Duration::from_millis(self.retry_delay_ms)
    }
}
