//! Search response message for peer responses.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::modules::query::distributed::error::{DistributedSearchError, SearchResult};

/// Individual search result in a network response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkSearchResult {
    /// Content identifier.
    pub cid: String,

    /// Document title (if available).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// Similarity score from the responder's search.
    pub score: f32,

    /// Text snippet with matching context.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
}

impl NetworkSearchResult {
    /// Create a new network search result.
    pub fn new(cid: impl Into<String>, score: f32) -> Self {
        Self {
            cid: cid.into(),
            title: None,
            score,
            snippet: None,
        }
    }

    /// Set the title.
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Set the snippet.
    pub fn with_snippet(mut self, snippet: impl Into<String>) -> Self {
        self.snippet = Some(snippet.into());
        self
    }
}

/// Search response message sent directly to the requester.
///
/// This message is sent in response to a `SearchQueryMessage` and contains
/// the search results from the responding peer.
///
/// # Encoding
///
/// Messages are serialized using DAG-CBOR for consistency with the
/// content protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResponseMessage {
    /// Correlation ID linking to the original query.
    pub query_id: Uuid,

    /// Responder's peer ID (base58 encoded).
    pub responder_peer_id: String,

    /// Search results from this peer.
    pub results: Vec<NetworkSearchResult>,

    /// Total matches found before applying the limit.
    pub total_matches: u32,

    /// Time taken to process the query on this peer (milliseconds).
    pub elapsed_ms: u32,
}

impl SearchResponseMessage {
    /// Create a new search response message.
    ///
    /// # Arguments
    ///
    /// * `query_id` - The query ID to correlate with.
    /// * `responder_peer_id` - This peer's ID (base58).
    /// * `results` - Search results to return.
    /// * `total_matches` - Total matches found before limit.
    pub fn new(
        query_id: Uuid,
        responder_peer_id: impl Into<String>,
        results: Vec<NetworkSearchResult>,
        total_matches: u32,
    ) -> Self {
        Self {
            query_id,
            responder_peer_id: responder_peer_id.into(),
            results,
            total_matches,
            elapsed_ms: 0,
        }
    }

    /// Set the elapsed time.
    pub fn with_elapsed(mut self, elapsed_ms: u32) -> Self {
        self.elapsed_ms = elapsed_ms;
        self
    }

    /// Serialize the message to DAG-CBOR bytes.
    pub fn to_cbor(&self) -> SearchResult<Vec<u8>> {
        serde_ipld_dagcbor::to_vec(self)
            .map_err(|e| DistributedSearchError::Serialization(e.to_string()))
    }

    /// Deserialize a message from DAG-CBOR bytes.
    pub fn from_cbor(bytes: &[u8]) -> SearchResult<Self> {
        serde_ipld_dagcbor::from_slice(bytes)
            .map_err(|e| DistributedSearchError::Deserialization(e.to_string()))
    }

    /// Get the number of results in this response.
    pub fn result_count(&self) -> usize {
        self.results.len()
    }

    /// Check if this response is empty.
    pub fn is_empty(&self) -> bool {
        self.results.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_result_creation() {
        let result = NetworkSearchResult::new("bafytest", 0.95)
            .with_title("Test Doc")
            .with_snippet("Test snippet");

        assert_eq!(result.cid, "bafytest");
        assert_eq!(result.score, 0.95);
        assert_eq!(result.title.as_deref(), Some("Test Doc"));
        assert_eq!(result.snippet.as_deref(), Some("Test snippet"));
    }

    #[test]
    fn test_response_creation() {
        let query_id = Uuid::new_v4();
        let results = vec![
            NetworkSearchResult::new("cid1", 0.9),
            NetworkSearchResult::new("cid2", 0.8),
        ];

        let response =
            SearchResponseMessage::new(query_id, "12D3KooWTest", results, 10).with_elapsed(50);

        assert_eq!(response.query_id, query_id);
        assert_eq!(response.responder_peer_id, "12D3KooWTest");
        assert_eq!(response.result_count(), 2);
        assert_eq!(response.total_matches, 10);
        assert_eq!(response.elapsed_ms, 50);
    }

    #[test]
    fn test_cbor_roundtrip() {
        let query_id = Uuid::new_v4();
        let results = vec![
            NetworkSearchResult::new("cid1", 0.9).with_title("Doc 1"),
            NetworkSearchResult::new("cid2", 0.8).with_snippet("Snippet 2"),
        ];

        let original =
            SearchResponseMessage::new(query_id, "12D3KooWTest", results, 5).with_elapsed(100);

        let bytes = original.to_cbor().unwrap();
        let decoded = SearchResponseMessage::from_cbor(&bytes).unwrap();

        assert_eq!(decoded.query_id, original.query_id);
        assert_eq!(decoded.responder_peer_id, original.responder_peer_id);
        assert_eq!(decoded.result_count(), original.result_count());
        assert_eq!(decoded.total_matches, original.total_matches);
        assert_eq!(decoded.elapsed_ms, original.elapsed_ms);

        // Check results
        assert_eq!(decoded.results[0].cid, "cid1");
        assert_eq!(decoded.results[0].title.as_deref(), Some("Doc 1"));
        assert_eq!(decoded.results[1].cid, "cid2");
        assert_eq!(decoded.results[1].snippet.as_deref(), Some("Snippet 2"));
    }

    #[test]
    fn test_empty_response() {
        let response = SearchResponseMessage::new(Uuid::new_v4(), "test", Vec::new(), 0);

        assert!(response.is_empty());
        assert_eq!(response.result_count(), 0);
    }
}
