//! Configuration for content transfer protocol.

use crate::utils::env::{env_bool, env_u32, env_u64, env_usize};
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
            request_timeout_secs: env_u64("CONTENT_TRANSFER_TIMEOUT_SECS", 30),
            max_concurrent_requests: env_usize("CONTENT_TRANSFER_MAX_CONCURRENT", 64),
            max_retries: env_u32("CONTENT_TRANSFER_MAX_RETRIES", 3),
            retry_delay_ms: env_u64("CONTENT_TRANSFER_RETRY_DELAY_MS", 1000),
            enabled: env_bool("CONTENT_TRANSFER_ENABLED", true),
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
