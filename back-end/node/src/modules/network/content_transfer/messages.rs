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
}
