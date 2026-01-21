//! Result ranking and merging engine.
//!
//! The ResultAggregator handles:
//! 1. Score normalization to 0-1 range per source
//! 2. Source penalty (0.9x for remote results)
//! 3. Popularity boost (+0.1 per additional source, max +0.3)
//! 4. Deduplication by CID
//! 5. Sorting by final adjusted score

use std::collections::HashMap;

use tracing::{debug, trace};

use super::types::{DistributedSearchResult, ResultSource, SourcedResult};

/// Score penalty applied to remote/network results.
/// Remote results receive 90% of their normalized score to prefer local content.
pub const REMOTE_SCORE_PENALTY: f32 = 0.9;

/// Popularity boost per additional source (beyond the first).
pub const POPULARITY_BOOST_PER_SOURCE: f32 = 0.1;

/// Maximum popularity boost cap.
pub const MAX_POPULARITY_BOOST: f32 = 0.3;

/// Result of aggregation including overflow tracking.
#[derive(Debug)]
pub struct AggregationResult {
    /// Final sorted and deduplicated results.
    pub results: Vec<DistributedSearchResult>,
    /// Count of results beyond the limit.
    pub overflow_count: u32,
    /// Total unique CIDs found before truncation.
    pub total_unique: u32,
}

/// Aggregates and ranks results from multiple sources.
///
/// # Algorithm
///
/// 1. Normalize scores to 0-1 range per source
/// 2. Apply source penalty (remote = 0.9x)
/// 3. Group results by CID
/// 4. Calculate popularity boost based on source count
/// 5. Deduplicate by keeping highest adjusted score
/// 6. Sort by final score descending
/// 7. Truncate to limit
pub struct ResultAggregator {
    /// Whether to apply source penalties.
    apply_penalties: bool,
    /// Whether to apply popularity boost.
    apply_boost: bool,
}

impl Default for ResultAggregator {
    fn default() -> Self {
        Self::new()
    }
}

impl ResultAggregator {
    /// Create a new result aggregator with default settings.
    pub fn new() -> Self {
        Self {
            apply_penalties: true,
            apply_boost: true,
        }
    }

    /// Disable source penalties (for testing or specific use cases).
    pub fn without_penalties(mut self) -> Self {
        self.apply_penalties = false;
        self
    }

    /// Disable popularity boost (for testing or specific use cases).
    pub fn without_boost(mut self) -> Self {
        self.apply_boost = false;
        self
    }

    /// Aggregate results from multiple sources.
    ///
    /// # Arguments
    ///
    /// * `results` - All sourced results to aggregate.
    /// * `limit` - Maximum number of results to return.
    ///
    /// # Returns
    ///
    /// `AggregationResult` with sorted results and overflow count.
    pub fn aggregate(&self, results: Vec<SourcedResult>, limit: u32) -> AggregationResult {
        if results.is_empty() {
            return AggregationResult {
                results: Vec::new(),
                overflow_count: 0,
                total_unique: 0,
            };
        }

        debug!(
            input_count = results.len(),
            limit = limit,
            "Starting result aggregation"
        );

        // Step 1: Normalize scores per source
        let normalized = self.normalize_scores(results);

        // Step 2: Apply source penalties
        let penalized = self.apply_source_penalties(normalized);

        // Step 3: Group by CID
        let grouped = self.group_by_cid(penalized);

        let total_unique = grouped.len() as u32;
        debug!(unique_cids = total_unique, "Grouped results by CID");

        // Step 4: Deduplicate and apply popularity boost
        let deduplicated = self.deduplicate_with_boost(grouped);

        // Step 5: Sort by adjusted score descending
        let mut sorted = deduplicated;
        sorted.sort_by(|a, b| {
            b.adjusted_score
                .partial_cmp(&a.adjusted_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Step 6: Truncate to limit
        let overflow_count = if sorted.len() > limit as usize {
            (sorted.len() - limit as usize) as u32
        } else {
            0
        };

        sorted.truncate(limit as usize);

        debug!(
            output_count = sorted.len(),
            overflow = overflow_count,
            "Aggregation complete"
        );

        AggregationResult {
            results: sorted,
            overflow_count,
            total_unique,
        }
    }

    /// Normalize scores to 0-1 range per source.
    fn normalize_scores(&self, results: Vec<SourcedResult>) -> Vec<SourcedResult> {
        // Group results by source_id to normalize per source
        let mut by_source: HashMap<String, Vec<SourcedResult>> = HashMap::new();

        for result in results {
            by_source
                .entry(result.source_id.clone())
                .or_default()
                .push(result);
        }

        let mut normalized = Vec::new();

        for (source_id, source_results) in by_source {
            // Find min and max scores for this source
            let (min_score, max_score) = source_results.iter().fold((f32::MAX, f32::MIN), |acc, r| {
                (acc.0.min(r.score), acc.1.max(r.score))
            });

            let range = max_score - min_score;

            trace!(
                source = %source_id,
                min = min_score,
                max = max_score,
                count = source_results.len(),
                "Normalizing source"
            );

            for mut result in source_results {
                if range > 0.0 {
                    result.score = (result.score - min_score) / range;
                } else {
                    // All same score, normalize to 1.0
                    result.score = 1.0;
                }
                normalized.push(result);
            }
        }

        normalized
    }

    /// Apply source penalties to normalized scores.
    fn apply_source_penalties(&self, results: Vec<SourcedResult>) -> Vec<SourcedResult> {
        if !self.apply_penalties {
            return results;
        }

        results
            .into_iter()
            .map(|mut r| {
                if r.source == ResultSource::Network {
                    r.score *= REMOTE_SCORE_PENALTY;
                    trace!(cid = %r.cid, penalized_score = r.score, "Applied remote penalty");
                }
                r
            })
            .collect()
    }

    /// Group results by CID.
    fn group_by_cid(&self, results: Vec<SourcedResult>) -> HashMap<String, Vec<SourcedResult>> {
        let mut grouped: HashMap<String, Vec<SourcedResult>> = HashMap::new();

        for result in results {
            grouped
                .entry(result.cid.clone())
                .or_default()
                .push(result);
        }

        grouped
    }

    /// Deduplicate results and apply popularity boost.
    fn deduplicate_with_boost(
        &self,
        grouped: HashMap<String, Vec<SourcedResult>>,
    ) -> Vec<DistributedSearchResult> {
        let mut final_results = Vec::with_capacity(grouped.len());

        for (cid, results) in grouped {
            // Count unique sources
            let sources_count = results.len() as u32;

            // Calculate popularity boost
            let boost = if self.apply_boost && sources_count > 1 {
                ((sources_count - 1) as f32 * POPULARITY_BOOST_PER_SOURCE).min(MAX_POPULARITY_BOOST)
            } else {
                0.0
            };

            // Find the best result (highest penalized score)
            let best = results
                .iter()
                .max_by(|a, b| {
                    a.score
                        .partial_cmp(&b.score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .expect("Group should not be empty");

            // Determine overall source type (local if any local source, otherwise network)
            let source = if results.iter().any(|r| r.source == ResultSource::Local) {
                ResultSource::Local
            } else {
                ResultSource::Network
            };

            // Calculate final adjusted score
            let adjusted_score = best.score + boost;

            trace!(
                cid = %cid,
                sources = sources_count,
                boost = boost,
                final_score = adjusted_score,
                "Deduplicated result"
            );

            final_results.push(DistributedSearchResult {
                cid,
                score: best.score,
                adjusted_score,
                source,
                sources_count,
                title: best.title.clone(),
                snippet: best.snippet.clone(),
                publisher_peer_id: best.publisher_peer_id.clone(),
            });
        }

        final_results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_local_result(cid: &str, score: f32) -> SourcedResult {
        SourcedResult::local(cid, score, "space-test-idx")
    }

    fn create_network_result(cid: &str, score: f32, peer: &str) -> SourcedResult {
        SourcedResult::network(cid, score, peer)
    }

    #[test]
    fn test_empty_aggregation() {
        let aggregator = ResultAggregator::new();
        let result = aggregator.aggregate(Vec::new(), 10);

        assert!(result.results.is_empty());
        assert_eq!(result.overflow_count, 0);
        assert_eq!(result.total_unique, 0);
    }

    #[test]
    fn test_single_result() {
        let aggregator = ResultAggregator::new();
        let results = vec![create_local_result("cid1", 0.9)];

        let result = aggregator.aggregate(results, 10);

        assert_eq!(result.results.len(), 1);
        assert_eq!(result.results[0].cid, "cid1");
        assert_eq!(result.results[0].sources_count, 1);
    }

    #[test]
    fn test_score_normalization() {
        let aggregator = ResultAggregator::new().without_penalties().without_boost();

        // Results from same source with different scores
        let results = vec![
            create_local_result("cid1", 0.5), // min
            create_local_result("cid2", 1.0), // max
            create_local_result("cid3", 0.75), // mid
        ];

        let result = aggregator.aggregate(results, 10);

        // Find results by cid
        let cid1 = result.results.iter().find(|r| r.cid == "cid1").unwrap();
        let cid2 = result.results.iter().find(|r| r.cid == "cid2").unwrap();
        let cid3 = result.results.iter().find(|r| r.cid == "cid3").unwrap();

        // After normalization: min→0, max→1, mid→0.5
        assert!((cid1.adjusted_score - 0.0).abs() < 0.01);
        assert!((cid2.adjusted_score - 1.0).abs() < 0.01);
        assert!((cid3.adjusted_score - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_source_penalty() {
        let aggregator = ResultAggregator::new().without_boost();

        let results = vec![
            create_local_result("cid1", 0.9),
            create_network_result("cid2", 0.9, "peer1"),
        ];

        let result = aggregator.aggregate(results, 10);

        let local = result.results.iter().find(|r| r.cid == "cid1").unwrap();
        let network = result.results.iter().find(|r| r.cid == "cid2").unwrap();

        // Both have same normalized score (1.0) since single source
        // But network should have penalty applied
        assert!(local.adjusted_score > network.adjusted_score);
        assert!((network.adjusted_score - 0.9).abs() < 0.01); // 1.0 * 0.9
    }

    #[test]
    fn test_popularity_boost() {
        let aggregator = ResultAggregator::new().without_penalties();

        // Same CID from multiple sources
        let results = vec![
            create_local_result("cid1", 0.8),
            create_network_result("cid1", 0.85, "peer1"),
            create_network_result("cid1", 0.9, "peer2"),
        ];

        let result = aggregator.aggregate(results, 10);

        assert_eq!(result.results.len(), 1);
        let cid1 = &result.results[0];

        assert_eq!(cid1.sources_count, 3);
        // Boost = (3-1) * 0.1 = 0.2
        // Best score after normalization + boost
        assert!(cid1.adjusted_score > cid1.score);
    }

    #[test]
    fn test_popularity_boost_cap() {
        let aggregator = ResultAggregator::new().without_penalties();

        // Same CID from 5 sources (should cap at 0.3 boost)
        let results = vec![
            create_local_result("cid1", 0.9),
            create_network_result("cid1", 0.9, "peer1"),
            create_network_result("cid1", 0.9, "peer2"),
            create_network_result("cid1", 0.9, "peer3"),
            create_network_result("cid1", 0.9, "peer4"),
        ];

        let result = aggregator.aggregate(results, 10);

        assert_eq!(result.results[0].sources_count, 5);
        // Boost should be capped at 0.3, not 0.4
        // Score = 1.0 (normalized) + 0.3 (capped boost) = 1.3
        assert!((result.results[0].adjusted_score - 1.3).abs() < 0.01);
    }

    #[test]
    fn test_deduplication() {
        let aggregator = ResultAggregator::new();

        // Same CID with different scores
        let results = vec![
            create_local_result("cid1", 0.8),
            create_local_result("cid1", 0.9), // Higher score, should be kept
        ];

        let result = aggregator.aggregate(results, 10);

        assert_eq!(result.results.len(), 1);
        assert_eq!(result.results[0].cid, "cid1");
        assert_eq!(result.results[0].sources_count, 2);
    }

    #[test]
    fn test_sorting() {
        let aggregator = ResultAggregator::new().without_penalties().without_boost();

        let results = vec![
            create_local_result("cid1", 0.5),
            create_local_result("cid2", 0.9),
            create_local_result("cid3", 0.7),
        ];

        let result = aggregator.aggregate(results, 10);

        // Should be sorted by adjusted score descending
        assert_eq!(result.results[0].cid, "cid2");
        assert_eq!(result.results[1].cid, "cid3");
        assert_eq!(result.results[2].cid, "cid1");
    }

    #[test]
    fn test_overflow_tracking() {
        let aggregator = ResultAggregator::new();

        let results = vec![
            create_local_result("cid1", 0.9),
            create_local_result("cid2", 0.8),
            create_local_result("cid3", 0.7),
            create_local_result("cid4", 0.6),
            create_local_result("cid5", 0.5),
        ];

        let result = aggregator.aggregate(results, 3);

        assert_eq!(result.results.len(), 3);
        assert_eq!(result.overflow_count, 2);
        assert_eq!(result.total_unique, 5);
    }

    #[test]
    fn test_local_source_preference() {
        let aggregator = ResultAggregator::new();

        // CID found both locally and from network
        let results = vec![
            create_local_result("cid1", 0.8),
            create_network_result("cid1", 0.9, "peer1"),
        ];

        let result = aggregator.aggregate(results, 10);

        // Should be marked as local since it was found locally
        assert_eq!(result.results[0].source, ResultSource::Local);
    }

    #[test]
    fn test_title_and_snippet_preservation() {
        let aggregator = ResultAggregator::new();

        let results = vec![create_local_result("cid1", 0.9)
            .with_title("Test Title")
            .with_snippet("Test snippet content")];

        let result = aggregator.aggregate(results, 10);

        assert_eq!(result.results[0].title.as_deref(), Some("Test Title"));
        assert_eq!(
            result.results[0].snippet.as_deref(),
            Some("Test snippet content")
        );
    }
}
