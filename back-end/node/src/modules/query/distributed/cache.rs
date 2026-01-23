//! Result cache with TTL expiration.
//!
//! Provides an LRU cache for search results to avoid redundant
//! network queries for repeated searches.

use std::num::NonZeroUsize;
use std::time::{Duration, Instant};

use lru::LruCache;

use super::types::DistributedSearchResponse;

/// Entry in the result cache.
#[derive(Debug, Clone)]
struct CacheEntry {
    /// The cached response.
    response: DistributedSearchResponse,
    /// When the entry was inserted.
    inserted_at: Instant,
}

/// LRU cache for search results with TTL expiration.
///
/// # Features
///
/// - LRU eviction when max size is reached
/// - TTL-based expiration for entries
/// - Thread-safe access via external synchronization
pub struct ResultCache {
    /// The underlying LRU cache.
    cache: LruCache<String, CacheEntry>,
    /// TTL for cache entries.
    ttl: Duration,
    /// Hit counter for metrics.
    hits: u64,
    /// Miss counter for metrics.
    misses: u64,
}

impl ResultCache {
    /// Create a new result cache.
    ///
    /// # Arguments
    ///
    /// * `max_entries` - Maximum number of entries to cache.
    /// * `ttl` - Time-to-live for cache entries.
    ///
    /// # Panics
    ///
    /// Panics if `max_entries` is 0.
    pub fn new(max_entries: usize, ttl: Duration) -> Self {
        let cap = NonZeroUsize::new(max_entries).expect("max_entries must be > 0");

        Self {
            cache: LruCache::new(cap),
            ttl,
            hits: 0,
            misses: 0,
        }
    }

    /// Get a cached response if it exists and hasn't expired.
    ///
    /// # Arguments
    ///
    /// * `key` - The cache key (typically hash of query + scope).
    ///
    /// # Returns
    ///
    /// The cached response if found and not expired, `None` otherwise.
    pub fn get(&mut self, key: &str) -> Option<DistributedSearchResponse> {
        if let Some(entry) = self.cache.get(key) {
            if entry.inserted_at.elapsed() < self.ttl {
                self.hits += 1;
                return Some(entry.response.clone());
            }
            // Entry expired - will be removed on next operation or peek
        }

        self.misses += 1;
        None
    }

    /// Insert a response into the cache.
    ///
    /// # Arguments
    ///
    /// * `key` - The cache key.
    /// * `response` - The response to cache.
    pub fn insert(&mut self, key: String, response: DistributedSearchResponse) {
        let entry = CacheEntry {
            response,
            inserted_at: Instant::now(),
        };
        self.cache.put(key, entry);
    }

    /// Check if a key exists and is not expired.
    pub fn contains(&mut self, key: &str) -> bool {
        self.get(key).is_some()
    }

    /// Remove an entry from the cache.
    pub fn remove(&mut self, key: &str) -> Option<DistributedSearchResponse> {
        self.cache.pop(key).map(|entry| entry.response)
    }

    /// Clear all entries from the cache.
    pub fn clear(&mut self) {
        self.cache.clear();
    }

    /// Get the current number of entries in the cache.
    ///
    /// Note: This includes expired entries that haven't been evicted yet.
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// Check if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }

    /// Get cache statistics.
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            entries: self.cache.len(),
            hits: self.hits,
            misses: self.misses,
            ttl_secs: self.ttl.as_secs(),
        }
    }

    /// Reset statistics counters.
    pub fn reset_stats(&mut self) {
        self.hits = 0;
        self.misses = 0;
    }
}

/// Cache statistics.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CacheStats {
    /// Current number of entries.
    pub entries: usize,
    /// Total cache hits.
    pub hits: u64,
    /// Total cache misses.
    pub misses: u64,
    /// TTL in seconds.
    pub ttl_secs: u64,
}

impl CacheStats {
    /// Calculate the hit rate as a percentage.
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            (self.hits as f64 / total as f64) * 100.0
        }
    }
}

/// Generate a cache key from query parameters.
///
/// # Arguments
///
/// * `query` - The search query text.
/// * `scope` - The search scope as a string.
/// * `limit` - The result limit.
pub fn cache_key(query: &str, scope: &str, limit: u32) -> String {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(query.as_bytes());
    hasher.update(scope.as_bytes());
    hasher.update(limit.to_le_bytes());

    let hash = hasher.finalize();
    // Use first 16 bytes as hex for a reasonably short key
    hex::encode(&hash[..16])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::query::distributed::types::DistributedSearchScope;

    fn create_test_response(query: &str) -> DistributedSearchResponse {
        DistributedSearchResponse::success(query, DistributedSearchScope::All, Vec::new())
    }

    #[test]
    fn test_cache_creation() {
        let cache = ResultCache::new(100, Duration::from_secs(15));

        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_cache_insert_and_get() {
        let mut cache = ResultCache::new(100, Duration::from_secs(15));

        let response = create_test_response("test query");
        cache.insert("key1".to_string(), response.clone());

        assert_eq!(cache.len(), 1);

        let cached = cache.get("key1");
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().query, "test query");
    }

    #[test]
    fn test_cache_miss() {
        let mut cache = ResultCache::new(100, Duration::from_secs(15));

        let cached = cache.get("nonexistent");
        assert!(cached.is_none());
    }

    #[test]
    fn test_cache_ttl_expiration() {
        // Use a very short TTL for testing
        let mut cache = ResultCache::new(100, Duration::from_millis(10));

        let response = create_test_response("test");
        cache.insert("key1".to_string(), response);

        // Should be present immediately
        assert!(cache.get("key1").is_some());

        // Wait for TTL to expire
        std::thread::sleep(Duration::from_millis(20));

        // Should be expired now
        assert!(cache.get("key1").is_none());
    }

    #[test]
    fn test_cache_lru_eviction() {
        let mut cache = ResultCache::new(2, Duration::from_secs(60));

        cache.insert("key1".to_string(), create_test_response("query1"));
        cache.insert("key2".to_string(), create_test_response("query2"));
        cache.insert("key3".to_string(), create_test_response("query3"));

        // key1 should have been evicted (LRU)
        assert!(cache.get("key1").is_none());
        assert!(cache.get("key2").is_some());
        assert!(cache.get("key3").is_some());
    }

    #[test]
    fn test_cache_remove() {
        let mut cache = ResultCache::new(100, Duration::from_secs(15));

        cache.insert("key1".to_string(), create_test_response("test"));
        assert_eq!(cache.len(), 1);

        let removed = cache.remove("key1");
        assert!(removed.is_some());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_cache_clear() {
        let mut cache = ResultCache::new(100, Duration::from_secs(15));

        cache.insert("key1".to_string(), create_test_response("test1"));
        cache.insert("key2".to_string(), create_test_response("test2"));
        assert_eq!(cache.len(), 2);

        cache.clear();
        assert!(cache.is_empty());
    }

    #[test]
    fn test_cache_stats() {
        let mut cache = ResultCache::new(100, Duration::from_secs(15));

        // Insert and get (hit)
        cache.insert("key1".to_string(), create_test_response("test"));
        cache.get("key1");

        // Miss
        cache.get("nonexistent");

        let stats = cache.stats();
        assert_eq!(stats.entries, 1);
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
        assert!((stats.hit_rate() - 50.0).abs() < 0.01);
    }

    #[test]
    fn test_cache_key_generation() {
        let key1 = cache_key("test query", "all", 10);
        let key2 = cache_key("test query", "all", 10);
        let key3 = cache_key("different query", "all", 10);
        let key4 = cache_key("test query", "local", 10);
        let key5 = cache_key("test query", "all", 20);

        // Same inputs should produce same key
        assert_eq!(key1, key2);

        // Different inputs should produce different keys
        assert_ne!(key1, key3);
        assert_ne!(key1, key4);
        assert_ne!(key1, key5);

        // Key should be 32 hex characters (16 bytes)
        assert_eq!(key1.len(), 32);
    }

    #[test]
    fn test_cache_stats_hit_rate_zero() {
        let cache = ResultCache::new(100, Duration::from_secs(15));
        let stats = cache.stats();

        assert_eq!(stats.hit_rate(), 0.0);
    }
}
