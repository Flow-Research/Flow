//! Environment variable parsing utilities.
//!
//! Provides consistent, ergonomic helpers for loading configuration from environment variables.
//! Each helper follows the pattern: try env var → parse → fallback to default.
//!
//! # Example
//!
//! ```rust
//! use crate::utils::env::{env_u64, env_bool, env_string, env_path};
//!
//! let port = env_u64("SERVER_PORT", 8080);
//! let debug = env_bool("DEBUG_MODE", false);
//! let name = env_string("APP_NAME", "my-app");
//! let data_dir = env_path("DATA_DIR", "/tmp/data");
//! ```

use std::path::PathBuf;

/// Get a u64 from environment, with default fallback.
///
/// Returns `default` if:
/// - Environment variable is not set
/// - Value cannot be parsed as u64
#[inline]
pub fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

/// Get a u32 from environment, with default fallback.
#[inline]
pub fn env_u32(key: &str, default: u32) -> u32 {
    std::env::var(key)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

/// Get an i32 from environment, with default fallback.
#[inline]
pub fn env_i32(key: &str, default: i32) -> i32 {
    std::env::var(key)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

/// Get a usize from environment, with default fallback.
#[inline]
pub fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

/// Get a bool from environment, with default fallback.
///
/// Recognizes: "true", "1", "yes", "on" (case-insensitive) as true.
/// Everything else (including unset) returns the default.
#[inline]
pub fn env_bool(key: &str, default: bool) -> bool {
    std::env::var(key)
        .ok()
        .map(|v| matches!(v.to_lowercase().as_str(), "true" | "1" | "yes" | "on"))
        .unwrap_or(default)
}

/// Get a String from environment, with default fallback.
#[inline]
pub fn env_string(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

/// Get an optional String from environment.
///
/// Returns `None` if the environment variable is not set.
#[inline]
pub fn env_string_opt(key: &str) -> Option<String> {
    std::env::var(key).ok()
}

/// Get a PathBuf from environment, with default fallback.
#[inline]
pub fn env_path(key: &str, default: &str) -> PathBuf {
    std::env::var(key)
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(default))
}

/// Get a Duration (in seconds) from environment, with default fallback.
#[inline]
pub fn env_duration_secs(key: &str, default_secs: u64) -> std::time::Duration {
    std::time::Duration::from_secs(env_u64(key, default_secs))
}

/// Get a Duration (in milliseconds) from environment, with default fallback.
#[inline]
pub fn env_duration_ms(key: &str, default_ms: u64) -> std::time::Duration {
    std::time::Duration::from_millis(env_u64(key, default_ms))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: env::set_var and env::remove_var are unsafe in recent Rust versions.
    // Tests that need to modify env vars use unsafe blocks.

    #[test]
    fn test_env_u64_default() {
        // Uses a unique key that won't be set
        assert_eq!(env_u64("TEST_ENV_U64_UNSET_12345", 42), 42);
    }

    #[test]
    fn test_env_bool_default() {
        // Uses a unique key that won't be set
        assert!(!env_bool("TEST_ENV_BOOL_UNSET_12345", false));
        assert!(env_bool("TEST_ENV_BOOL_UNSET_12345", true));
    }

    #[test]
    fn test_env_string_default() {
        // Uses a unique key that won't be set
        assert_eq!(env_string("TEST_ENV_STRING_UNSET_12345", "default"), "default");
    }

    #[test]
    fn test_env_path_default() {
        // Uses a unique key that won't be set
        assert_eq!(env_path("TEST_ENV_PATH_UNSET_12345", "/default"), PathBuf::from("/default"));
    }

    #[test]
    fn test_env_usize_default() {
        assert_eq!(env_usize("TEST_ENV_USIZE_UNSET_12345", 100), 100);
    }

    #[test]
    fn test_env_duration_secs_default() {
        let d = env_duration_secs("TEST_ENV_DUR_UNSET_12345", 60);
        assert_eq!(d, std::time::Duration::from_secs(60));
    }
}
