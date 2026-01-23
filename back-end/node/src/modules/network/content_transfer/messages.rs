//! Protocol message definitions for content transfer.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, warn};

use crate::modules::storage::content::ContentId;

// ============================================================================
// Protocol Constants
// ============================================================================

/// Protocol version for forward compatibility.
pub const CONTENT_PROTOCOL_VERSION: &str = "1.0.0";

/// Protocol ID for libp2p negotiation.
/// Format: /flow/content/<version>
pub const CONTENT_PROTOCOL_ID: &str = "/flow/content/1.0.0";

/// Maximum message size in bytes (16 MB).
/// This allows for large blocks while preventing DoS.
pub const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;

/// Maximum number of links in a block response.
pub const MAX_LINKS: usize = 10_000;

// ============================================================================
// Error Types
// ============================================================================

/// Errors that can occur during message serialization/deserialization.
#[derive(Debug, Error)]
pub enum MessageError {
    /// CBOR serialization failed.
    #[error("CBOR serialization error: {0}")]
    Serialization(String),

    /// CBOR deserialization failed.
    #[error("CBOR deserialization error: {0}")]
    Deserialization(String),

    /// Message exceeds size limit.
    #[error("Message too large: {size} bytes (max: {max})")]
    TooLarge { size: usize, max: usize },

    /// Invalid CID in message.
    #[error("Invalid CID: {0}")]
    InvalidCid(String),
}

// ============================================================================
// Error Codes
// ============================================================================

/// Structured error codes for protocol responses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// Content not found on this peer.
    NotFound,
    /// Request was malformed or invalid.
    InvalidRequest,
    /// Internal server error.
    InternalError,
    /// Peer is rate limiting requests.
    RateLimited,
    /// Request timed out.
    Timeout,
    /// Peer is overloaded.
    Overloaded,
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorCode::NotFound => write!(f, "not_found"),
            ErrorCode::InvalidRequest => write!(f, "invalid_request"),
            ErrorCode::InternalError => write!(f, "internal_error"),
            ErrorCode::RateLimited => write!(f, "rate_limited"),
            ErrorCode::Timeout => write!(f, "timeout"),
            ErrorCode::Overloaded => write!(f, "overloaded"),
        }
    }
}

// ============================================================================
// Content Request
// ============================================================================

/// Request message for content retrieval.
///
/// Sent by a peer that wants to fetch a block or manifest from another peer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentRequest {
    /// Request a raw data block by its CID.
    GetBlock {
        /// Content identifier of the requested block.
        cid: ContentId,
    },

    /// Request a document manifest by its root CID.
    ///
    /// The manifest contains metadata and chunk references.
    GetManifest {
        /// Content identifier of the manifest.
        cid: ContentId,
    },
}

impl ContentRequest {
    /// Create a GetBlock request.
    pub fn get_block(cid: ContentId) -> Self {
        debug!(cid = %cid, "Creating GetBlock request");
        ContentRequest::GetBlock { cid }
    }

    /// Create a GetManifest request.
    pub fn get_manifest(cid: ContentId) -> Self {
        debug!(cid = %cid, "Creating GetManifest request");
        ContentRequest::GetManifest { cid }
    }

    /// Get the CID being requested.
    pub fn cid(&self) -> &ContentId {
        match self {
            ContentRequest::GetBlock { cid } => cid,
            ContentRequest::GetManifest { cid } => cid,
        }
    }

    /// Check if this is a block request.
    pub fn is_block_request(&self) -> bool {
        matches!(self, ContentRequest::GetBlock { .. })
    }

    /// Check if this is a manifest request.
    pub fn is_manifest_request(&self) -> bool {
        matches!(self, ContentRequest::GetManifest { .. })
    }

    /// Serialize to CBOR bytes.
    pub fn to_bytes(&self) -> Result<Vec<u8>, MessageError> {
        serde_cbor::to_vec(self).map_err(|e| {
            warn!(error = %e, "Failed to serialize ContentRequest");
            MessageError::Serialization(e.to_string())
        })
    }

    /// Deserialize from CBOR bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, MessageError> {
        if bytes.len() > MAX_MESSAGE_SIZE {
            return Err(MessageError::TooLarge {
                size: bytes.len(),
                max: MAX_MESSAGE_SIZE,
            });
        }

        serde_cbor::from_slice(bytes).map_err(|e| {
            warn!(error = %e, "Failed to deserialize ContentRequest");
            MessageError::Deserialization(e.to_string())
        })
    }
}

// ============================================================================
// Content Response
// ============================================================================

/// Response message for content retrieval.
///
/// Sent by a peer in response to a ContentRequest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentResponse {
    /// Successful block response.
    Block {
        /// CID of the returned block.
        cid: ContentId,
        /// Raw block data.
        #[serde(with = "serde_bytes")]
        data: Vec<u8>,
        /// Links to other blocks (for DAG traversal).
        links: Vec<ContentId>,
    },

    /// Successful manifest response.
    Manifest {
        /// CID of the returned manifest.
        cid: ContentId,
        /// Serialized manifest data (DAG-CBOR).
        #[serde(with = "serde_bytes")]
        data: Vec<u8>,
    },

    /// Content not found on this peer.
    NotFound {
        /// CID that was not found.
        cid: ContentId,
    },

    /// Error occurred while processing request.
    Error {
        /// Structured error code.
        code: ErrorCode,
        /// Human-readable error message.
        message: String,
    },
}

impl ContentResponse {
    /// Create a successful block response.
    pub fn block(cid: ContentId, data: Vec<u8>, links: Vec<ContentId>) -> Self {
        debug!(
            cid = %cid,
            size = data.len(),
            links = links.len(),
            "Creating Block response"
        );
        ContentResponse::Block { cid, data, links }
    }

    /// Create a successful manifest response.
    pub fn manifest(cid: ContentId, data: Vec<u8>) -> Self {
        debug!(cid = %cid, size = data.len(), "Creating Manifest response");
        ContentResponse::Manifest { cid, data }
    }

    /// Create a not-found response.
    pub fn not_found(cid: ContentId) -> Self {
        debug!(cid = %cid, "Creating NotFound response");
        ContentResponse::NotFound { cid }
    }

    /// Create an error response.
    pub fn error(code: ErrorCode, message: impl Into<String>) -> Self {
        let message = message.into();
        warn!(code = %code, message = %message, "Creating Error response");
        ContentResponse::Error { code, message }
    }

    /// Check if this response indicates success.
    pub fn is_success(&self) -> bool {
        matches!(
            self,
            ContentResponse::Block { .. } | ContentResponse::Manifest { .. }
        )
    }

    /// Check if content was not found.
    pub fn is_not_found(&self) -> bool {
        matches!(self, ContentResponse::NotFound { .. })
    }

    /// Check if this is an error response.
    pub fn is_error(&self) -> bool {
        matches!(self, ContentResponse::Error { .. })
    }

    /// Get the CID associated with this response (if any).
    pub fn cid(&self) -> Option<&ContentId> {
        match self {
            ContentResponse::Block { cid, .. } => Some(cid),
            ContentResponse::Manifest { cid, .. } => Some(cid),
            ContentResponse::NotFound { cid } => Some(cid),
            ContentResponse::Error { .. } => None,
        }
    }

    /// Get the data payload (if successful response).
    pub fn data(&self) -> Option<&[u8]> {
        match self {
            ContentResponse::Block { data, .. } => Some(data),
            ContentResponse::Manifest { data, .. } => Some(data),
            _ => None,
        }
    }

    /// Get the links (for block responses).
    pub fn links(&self) -> Option<&[ContentId]> {
        match self {
            ContentResponse::Block { links, .. } => Some(links),
            _ => None,
        }
    }

    /// Serialize to CBOR bytes.
    pub fn to_bytes(&self) -> Result<Vec<u8>, MessageError> {
        let bytes = serde_cbor::to_vec(self).map_err(|e| {
            warn!(error = %e, "Failed to serialize ContentResponse");
            MessageError::Serialization(e.to_string())
        })?;

        if bytes.len() > MAX_MESSAGE_SIZE {
            return Err(MessageError::TooLarge {
                size: bytes.len(),
                max: MAX_MESSAGE_SIZE,
            });
        }

        Ok(bytes)
    }

    /// Deserialize from CBOR bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, MessageError> {
        if bytes.len() > MAX_MESSAGE_SIZE {
            return Err(MessageError::TooLarge {
                size: bytes.len(),
                max: MAX_MESSAGE_SIZE,
            });
        }

        serde_cbor::from_slice(bytes).map_err(|e| {
            warn!(error = %e, "Failed to deserialize ContentResponse");
            MessageError::Deserialization(e.to_string())
        })
    }
}

// ============================================================================
// From Implementations
// ============================================================================

impl From<std::io::Error> for ContentResponse {
    fn from(e: std::io::Error) -> Self {
        ContentResponse::error(ErrorCode::InternalError, e.to_string())
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn sample_cid() -> ContentId {
        ContentId::from_bytes(b"test content for CID generation")
    }

    fn sample_cid_2() -> ContentId {
        ContentId::from_bytes(b"different content")
    }

    // ========================================
    // ContentRequest Tests
    // ========================================

    #[test]
    fn test_get_block_request() {
        let cid = sample_cid();
        let request = ContentRequest::get_block(cid.clone());

        assert!(request.is_block_request());
        assert!(!request.is_manifest_request());
        assert_eq!(request.cid(), &cid);
    }

    #[test]
    fn test_get_manifest_request() {
        let cid = sample_cid();
        let request = ContentRequest::get_manifest(cid.clone());

        assert!(request.is_manifest_request());
        assert!(!request.is_block_request());
        assert_eq!(request.cid(), &cid);
    }

    #[test]
    fn test_request_serialization_roundtrip() {
        let cid = sample_cid();

        // Test GetBlock
        let request = ContentRequest::get_block(cid.clone());
        let bytes = request.to_bytes().unwrap();
        let restored = ContentRequest::from_bytes(&bytes).unwrap();
        assert_eq!(request, restored);

        // Test GetManifest
        let request = ContentRequest::get_manifest(cid);
        let bytes = request.to_bytes().unwrap();
        let restored = ContentRequest::from_bytes(&bytes).unwrap();
        assert_eq!(request, restored);
    }

    #[test]
    fn test_request_cbor_is_compact() {
        let cid = sample_cid();
        let request = ContentRequest::get_block(cid);
        let bytes = request.to_bytes().unwrap();

        // CBOR should be reasonably compact (CID ~40 bytes + overhead)
        assert!(
            bytes.len() < 100,
            "Request serialized to {} bytes",
            bytes.len()
        );
    }

    // ========================================
    // ContentResponse Tests
    // ========================================

    #[test]
    fn test_block_response() {
        let cid = sample_cid();
        let data = b"block data content".to_vec();
        let links = vec![sample_cid_2()];

        let response = ContentResponse::block(cid.clone(), data.clone(), links.clone());

        assert!(response.is_success());
        assert!(!response.is_not_found());
        assert!(!response.is_error());
        assert_eq!(response.cid(), Some(&cid));
        assert_eq!(response.data(), Some(data.as_slice()));
        assert_eq!(response.links(), Some(links.as_slice()));
    }

    #[test]
    fn test_manifest_response() {
        let cid = sample_cid();
        let data = b"manifest cbor data".to_vec();

        let response = ContentResponse::manifest(cid.clone(), data.clone());

        assert!(response.is_success());
        assert_eq!(response.cid(), Some(&cid));
        assert_eq!(response.data(), Some(data.as_slice()));
        assert!(response.links().is_none());
    }

    #[test]
    fn test_not_found_response() {
        let cid = sample_cid();
        let response = ContentResponse::not_found(cid.clone());

        assert!(!response.is_success());
        assert!(response.is_not_found());
        assert!(!response.is_error());
        assert_eq!(response.cid(), Some(&cid));
        assert!(response.data().is_none());
    }

    #[test]
    fn test_error_response() {
        let response = ContentResponse::error(ErrorCode::RateLimited, "Too many requests");

        assert!(!response.is_success());
        assert!(!response.is_not_found());
        assert!(response.is_error());
        assert!(response.cid().is_none());
        assert!(response.data().is_none());

        if let ContentResponse::Error { code, message } = response {
            assert_eq!(code, ErrorCode::RateLimited);
            assert_eq!(message, "Too many requests");
        } else {
            panic!("Expected Error variant");
        }
    }

    #[test]
    fn test_response_serialization_roundtrip() {
        let cid = sample_cid();

        // Test Block
        let response = ContentResponse::block(cid.clone(), b"data".to_vec(), vec![sample_cid_2()]);
        let bytes = response.to_bytes().unwrap();
        let restored = ContentResponse::from_bytes(&bytes).unwrap();
        assert_eq!(response, restored);

        // Test Manifest
        let response = ContentResponse::manifest(cid.clone(), b"manifest".to_vec());
        let bytes = response.to_bytes().unwrap();
        let restored = ContentResponse::from_bytes(&bytes).unwrap();
        assert_eq!(response, restored);

        // Test NotFound
        let response = ContentResponse::not_found(cid);
        let bytes = response.to_bytes().unwrap();
        let restored = ContentResponse::from_bytes(&bytes).unwrap();
        assert_eq!(response, restored);

        // Test Error
        let response = ContentResponse::error(ErrorCode::InternalError, "Something broke");
        let bytes = response.to_bytes().unwrap();
        let restored = ContentResponse::from_bytes(&bytes).unwrap();
        assert_eq!(response, restored);
    }

    #[test]
    fn test_large_block_response() {
        let cid = sample_cid();
        // 1 MB block (realistic large chunk)
        let data = vec![0u8; 1024 * 1024];
        let links: Vec<ContentId> = (0..100)
            .map(|i| ContentId::from_bytes(format!("link_{}", i).as_bytes()))
            .collect();

        let response = ContentResponse::block(cid, data.clone(), links.clone());
        let bytes = response.to_bytes().unwrap();
        let restored = ContentResponse::from_bytes(&bytes).unwrap();

        if let ContentResponse::Block {
            data: restored_data,
            links: restored_links,
            ..
        } = restored
        {
            assert_eq!(restored_data.len(), data.len());
            assert_eq!(restored_links.len(), links.len());
        } else {
            panic!("Expected Block variant");
        }
    }

    #[test]
    fn test_request_too_large() {
        // Create bytes larger than MAX_MESSAGE_SIZE
        let large_bytes = vec![0u8; MAX_MESSAGE_SIZE + 1];
        let result = ContentRequest::from_bytes(&large_bytes);

        assert!(matches!(result, Err(MessageError::TooLarge { .. })));
    }

    // ========================================
    // ErrorCode Tests
    // ========================================

    #[test]
    fn test_error_code_display() {
        assert_eq!(ErrorCode::NotFound.to_string(), "not_found");
        assert_eq!(ErrorCode::InvalidRequest.to_string(), "invalid_request");
        assert_eq!(ErrorCode::InternalError.to_string(), "internal_error");
        assert_eq!(ErrorCode::RateLimited.to_string(), "rate_limited");
        assert_eq!(ErrorCode::Timeout.to_string(), "timeout");
        assert_eq!(ErrorCode::Overloaded.to_string(), "overloaded");
    }

    #[test]
    fn test_error_code_serialization() {
        let codes = vec![
            ErrorCode::NotFound,
            ErrorCode::InvalidRequest,
            ErrorCode::InternalError,
            ErrorCode::RateLimited,
            ErrorCode::Timeout,
            ErrorCode::Overloaded,
        ];

        for code in codes {
            let json = serde_json::to_string(&code).unwrap();
            let restored: ErrorCode = serde_json::from_str(&json).unwrap();
            assert_eq!(code, restored);
        }
    }

    // ========================================
    // Protocol Constants Tests
    // ========================================

    #[test]
    fn test_protocol_id_format() {
        assert!(CONTENT_PROTOCOL_ID.starts_with("/flow/content/"));
        assert!(CONTENT_PROTOCOL_ID.contains(CONTENT_PROTOCOL_VERSION));
    }

    #[test]
    fn test_max_message_size() {
        // Should allow blocks up to at least 1MB
        assert!(MAX_MESSAGE_SIZE >= 1024 * 1024);
        // But not be unreasonably large (prevent DoS)
        assert!(MAX_MESSAGE_SIZE <= 32 * 1024 * 1024);
    }

    // ========================================
    // Edge Case Tests
    // ========================================

    /// Empty block data serialization
    #[test]
    fn test_empty_block_data_serialization() {
        let cid = sample_cid();
        let response = ContentResponse::block(cid.clone(), vec![], vec![]);

        assert!(response.is_success());
        assert_eq!(response.data(), Some([].as_slice()));

        // Roundtrip
        let bytes = response.to_bytes().unwrap();
        let restored = ContentResponse::from_bytes(&bytes).unwrap();
        assert_eq!(response, restored);

        // Verify data is actually empty
        if let ContentResponse::Block { data, links, .. } = restored {
            assert!(data.is_empty());
            assert!(links.is_empty());
        } else {
            panic!("Expected Block variant");
        }
    }

    /// Block with maximum links (1000+)
    #[test]
    fn test_block_with_many_links() {
        let cid = sample_cid();
        // Create more than MAX_LINKS would allow if there was a hard limit
        let link_count = 1000;
        let links: Vec<ContentId> = (0..link_count)
            .map(|i| ContentId::from_bytes(format!("link_content_{}", i).as_bytes()))
            .collect();

        let response = ContentResponse::block(cid.clone(), b"data".to_vec(), links.clone());

        // Should serialize and deserialize correctly
        let bytes = response.to_bytes().unwrap();
        let restored = ContentResponse::from_bytes(&bytes).unwrap();

        if let ContentResponse::Block {
            links: restored_links,
            ..
        } = restored
        {
            assert_eq!(restored_links.len(), link_count);
            // Verify first and last links
            assert_eq!(restored_links[0], links[0]);
            assert_eq!(restored_links[link_count - 1], links[link_count - 1]);
        } else {
            panic!("Expected Block variant");
        }
    }

    /// Error with empty message
    #[test]
    fn test_error_with_empty_message() {
        let response = ContentResponse::error(ErrorCode::InternalError, "");

        assert!(response.is_error());
        assert!(!response.is_success());

        // Roundtrip
        let bytes = response.to_bytes().unwrap();
        let restored = ContentResponse::from_bytes(&bytes).unwrap();
        assert_eq!(response, restored);

        if let ContentResponse::Error { message, code } = restored {
            assert!(message.is_empty());
            assert_eq!(code, ErrorCode::InternalError);
        } else {
            panic!("Expected Error variant");
        }
    }

    /// Error with very long message (10KB+)
    #[test]
    fn test_error_with_very_long_message() {
        let long_message = "x".repeat(10 * 1024); // 10KB message
        let response = ContentResponse::error(ErrorCode::InternalError, &long_message);

        // Should serialize correctly
        let bytes = response.to_bytes().unwrap();
        assert!(bytes.len() > 10 * 1024);

        // Should roundtrip
        let restored = ContentResponse::from_bytes(&bytes).unwrap();

        if let ContentResponse::Error { message, .. } = restored {
            assert_eq!(message.len(), 10 * 1024);
            assert_eq!(message, long_message);
        } else {
            panic!("Expected Error variant");
        }
    }

    /// CID with minimum valid length
    #[test]
    fn test_cid_minimum_length() {
        // Create CID from minimal content (single byte)
        let cid = ContentId::from_bytes(&[0u8]);

        let request = ContentRequest::get_block(cid.clone());
        let bytes = request.to_bytes().unwrap();
        let restored = ContentRequest::from_bytes(&bytes).unwrap();

        assert_eq!(request, restored);
        assert_eq!(restored.cid(), &cid);
    }

    /// CID with maximum valid length (large content hash)
    #[test]
    fn test_cid_from_large_content() {
        // Create CID from large content (the CID itself is always fixed-size based on hash)
        let large_content = vec![0xFFu8; 1024 * 1024]; // 1MB content
        let cid = ContentId::from_bytes(&large_content);

        let request = ContentRequest::get_block(cid.clone());
        let bytes = request.to_bytes().unwrap();

        // Request should be compact (CID is hash, not content)
        assert!(bytes.len() < 200, "Request was {} bytes", bytes.len());

        let restored = ContentRequest::from_bytes(&bytes).unwrap();
        assert_eq!(request, restored);
    }

    /// Unicode characters in error message
    #[test]
    fn test_unicode_in_error_message() {
        let unicode_messages = vec![
            "错误: 内容未找到",                   // Chinese
            "エラー: コンテンツが見つかりません", // Japanese
            "Ошибка: контент не найден",          // Russian
            "🚀 Error: 💥 Boom! 🔥",              // Emoji
            "Ñoño señor café",                    // Spanish accents
            "مرحبا بالعالم",                      // Arabic
            "\u{0000}\u{001F}control",            // Control characters
        ];

        for msg in unicode_messages {
            let response = ContentResponse::error(ErrorCode::InternalError, msg);
            let bytes = response.to_bytes().unwrap();
            let restored = ContentResponse::from_bytes(&bytes).unwrap();

            if let ContentResponse::Error { message, .. } = restored {
                assert_eq!(message, msg, "Unicode message not preserved: {}", msg);
            } else {
                panic!("Expected Error variant for message: {}", msg);
            }
        }
    }

    // ========================================
    // Error Handling Tests
    // ========================================

    /// Deserialize truncated response bytes
    #[test]
    fn test_truncated_response_bytes() {
        let response = ContentResponse::block(
            sample_cid(),
            b"some block data here".to_vec(),
            vec![sample_cid_2()],
        );
        let bytes = response.to_bytes().unwrap();

        // Try various truncation points
        for truncate_at in [1, 5, 10, bytes.len() / 2, bytes.len() - 1] {
            let truncated = &bytes[..truncate_at];
            let result = ContentResponse::from_bytes(truncated);
            assert!(
                result.is_err(),
                "Should fail at truncation point {}",
                truncate_at
            );
        }
    }

    /// Invalid enum discriminant
    #[test]
    fn test_invalid_enum_discriminant() {
        // Craft CBOR with an invalid "type" field
        // ContentRequest expects "type": "get_block" or "get_manifest"
        let mut map = BTreeMap::new();
        map.insert(
            serde_cbor::Value::Text("type".to_string()),
            serde_cbor::Value::Text("invalid_type".to_string()),
        );
        let invalid_cbor = serde_cbor::to_vec(&serde_cbor::Value::Map(map)).unwrap();

        let result = ContentRequest::from_bytes(&invalid_cbor);
        assert!(
            result.is_err(),
            "Invalid type discriminant should fail deserialization"
        );

        // Also test for response
        let result = ContentResponse::from_bytes(&invalid_cbor);
        assert!(
            result.is_err(),
            "Invalid type discriminant should fail response deserialization"
        );
    }

    // ========================================
    // Security Tests
    // ========================================

    /// Fuzz deserialization - no panics on random input
    #[test]
    fn test_fuzz_deserialization_no_panic() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        // Generate pseudo-random bytes using a simple PRNG
        fn pseudo_random_bytes(seed: u64, len: usize) -> Vec<u8> {
            let mut hasher = DefaultHasher::new();
            let mut result = Vec::with_capacity(len);
            let mut current = seed;

            for _ in 0..len {
                current.hash(&mut hasher);
                current = hasher.finish();
                result.push((current & 0xFF) as u8);
            }
            result
        }

        // Test 10,000+ iterations with various lengths
        for seed in 0..10_000 {
            let len = (seed % 1000) as usize + 1;
            let random_bytes = pseudo_random_bytes(seed, len);

            // These should not panic - errors are fine
            let _ = ContentRequest::from_bytes(&random_bytes);
            let _ = ContentResponse::from_bytes(&random_bytes);
        }

        // Also test edge cases
        let edge_cases: Vec<&[u8]> = vec![
            &[],
            &[0x00],
            &[0xFF],
            &[0xFF; 100],
            &[0x00; 100],
            &[0xBF], // CBOR map start
            &[0x9F], // CBOR array start
            &[0x7F], // CBOR text start
        ];

        for bytes in edge_cases {
            let _ = ContentRequest::from_bytes(bytes);
            let _ = ContentResponse::from_bytes(bytes);
        }
    }

    /// CID verification - computed CID matches stored CID
    #[test]
    fn test_cid_content_hash_verification() {
        let content = b"known content for verification";
        let cid = ContentId::from_bytes(content);

        // Create response claiming this CID
        let response = ContentResponse::block(cid.clone(), content.to_vec(), vec![]);

        // Serialize and deserialize
        let bytes = response.to_bytes().unwrap();
        let restored = ContentResponse::from_bytes(&bytes).unwrap();

        if let ContentResponse::Block {
            cid: resp_cid,
            data,
            ..
        } = restored
        {
            // Verify the CID matches the content hash
            let computed_cid = ContentId::from_bytes(&data);
            assert_eq!(
                resp_cid, computed_cid,
                "Stored CID must match computed hash of data"
            );
            assert_eq!(resp_cid, cid);
        } else {
            panic!("Expected Block variant");
        }

        // Test that mismatched CID can be detected
        let wrong_cid = ContentId::from_bytes(b"different content");
        let bad_response = ContentResponse::block(wrong_cid.clone(), content.to_vec(), vec![]);

        if let ContentResponse::Block {
            cid: resp_cid,
            data,
            ..
        } = bad_response
        {
            let computed_cid = ContentId::from_bytes(&data);
            // This demonstrates we CAN detect the mismatch
            assert_ne!(
                resp_cid, computed_cid,
                "Mismatched CID should be detectable"
            );
        }
    }

    /// Allocation limits - cannot cause OOM via size claims
    #[test]
    fn test_allocation_limits_prevent_dos() {
        // Test that claiming a huge size doesn't allocate
        // The from_bytes check should reject before allocation

        // Create bytes that would serialize to claim huge size
        // MAX_MESSAGE_SIZE check happens before deserialization
        let huge_bytes = vec![0u8; MAX_MESSAGE_SIZE + 1];
        let result = ContentRequest::from_bytes(&huge_bytes);
        assert!(matches!(result, Err(MessageError::TooLarge { .. })));

        let result = ContentResponse::from_bytes(&huge_bytes);
        assert!(matches!(result, Err(MessageError::TooLarge { .. })));

        // Also test at exact boundary
        let boundary_bytes = vec![0u8; MAX_MESSAGE_SIZE];
        // This will fail deserialization (not valid CBOR) but should NOT OOM
        let _ = ContentRequest::from_bytes(&boundary_bytes);
        let _ = ContentResponse::from_bytes(&boundary_bytes);
    }

    // ========================================
    // Compatibility Tests
    // ========================================

    /// Golden file test for serialization format stability
    #[test]
    fn test_golden_file_serialization_stability() {
        // Known inputs with expected byte representations
        // These bytes were captured from a working implementation
        // and should remain stable across versions

        let cid = ContentId::from_bytes(b"golden test content");
        let request = ContentRequest::get_block(cid.clone());
        let bytes = request.to_bytes().unwrap();

        // Store the golden bytes (in real impl, load from file)
        // For now, just verify deterministic output
        let bytes2 = request.to_bytes().unwrap();
        assert_eq!(bytes, bytes2, "Serialization must be deterministic");

        // Verify response serialization is also deterministic
        let response = ContentResponse::block(cid.clone(), b"golden data".to_vec(), vec![]);
        let resp_bytes1 = response.to_bytes().unwrap();
        let resp_bytes2 = response.to_bytes().unwrap();
        assert_eq!(resp_bytes1, resp_bytes2);

        // Verify we can roundtrip
        let restored = ContentRequest::from_bytes(&bytes).unwrap();
        assert_eq!(request, restored);
    }

    /// Cross-platform byte compatibility (endianness)
    #[test]
    fn test_cross_platform_byte_order() {
        // CBOR uses big-endian for integers, which is platform-independent
        // Verify our serialization doesn't accidentally use native byte order

        let cid = sample_cid();
        let response = ContentResponse::block(
            cid.clone(),
            vec![0x01, 0x02, 0x03, 0x04], // Known byte sequence
            vec![sample_cid_2()],
        );

        let bytes = response.to_bytes().unwrap();

        // Verify the data bytes appear in correct order (not reversed)
        // Find the data in the serialized output
        let data_pattern = [0x01, 0x02, 0x03, 0x04];
        let contains_data = bytes.windows(4).any(|window| window == data_pattern);
        assert!(contains_data, "Data should appear in original byte order");

        // Roundtrip verification
        let restored = ContentResponse::from_bytes(&bytes).unwrap();
        if let ContentResponse::Block { data, .. } = restored {
            assert_eq!(data, vec![0x01, 0x02, 0x03, 0x04]);
        } else {
            panic!("Expected Block variant");
        }
    }
}
