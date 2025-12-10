//! Property-based tests for content transfer messages.
//!
//! These tests use proptest to verify invariants across a wide
//! range of automatically generated inputs.

#![cfg(test)]

use node::modules::network::content_transfer::{
    ContentRequest, ContentResponse, ErrorCode, MAX_MESSAGE_SIZE,
};
use node::modules::storage::content::ContentId;
use proptest::prelude::*;

// ============================================================================
// Arbitrary Implementations for Proptest
// ============================================================================

/// Generate arbitrary ContentId
fn arb_content_id() -> impl Strategy<Value = ContentId> {
    // Generate random bytes and create CID from them
    prop::collection::vec(any::<u8>(), 1..1000).prop_map(|bytes| ContentId::from_bytes(&bytes))
}

/// Generate arbitrary ErrorCode
fn arb_error_code() -> impl Strategy<Value = ErrorCode> {
    prop_oneof![
        Just(ErrorCode::NotFound),
        Just(ErrorCode::InvalidRequest),
        Just(ErrorCode::InternalError),
        Just(ErrorCode::RateLimited),
        Just(ErrorCode::Timeout),
        Just(ErrorCode::Overloaded),
    ]
}

/// Generate arbitrary ContentRequest
fn arb_content_request() -> impl Strategy<Value = ContentRequest> {
    arb_content_id().prop_flat_map(|cid| {
        prop_oneof![
            Just(ContentRequest::GetBlock { cid: cid.clone() }),
            Just(ContentRequest::GetManifest { cid }),
        ]
    })
}

/// Generate arbitrary ContentResponse (within size limits)
fn arb_content_response() -> impl Strategy<Value = ContentResponse> {
    // Keep data sizes reasonable to avoid test timeouts
    let max_data_size = 64 * 1024; // 64KB for tests
    let max_links = 100;

    prop_oneof![
        // Block response
        (
            arb_content_id(),
            prop::collection::vec(any::<u8>(), 0..max_data_size),
            prop::collection::vec(arb_content_id(), 0..max_links),
        )
            .prop_map(|(cid, data, links)| ContentResponse::Block { cid, data, links }),
        // Manifest response
        (
            arb_content_id(),
            prop::collection::vec(any::<u8>(), 0..max_data_size),
        )
            .prop_map(|(cid, data)| ContentResponse::Manifest { cid, data }),
        // NotFound response
        arb_content_id().prop_map(|cid| ContentResponse::NotFound { cid }),
        // Error response
        (arb_error_code(), ".*")
            .prop_map(|(code, message)| { ContentResponse::Error { code, message } }),
    ]
}

// ============================================================================
// Property-Based Tests
// ============================================================================

proptest! {
    /// All ContentRequest variants roundtrip correctly
    #[test]
    fn prop_content_request_roundtrip(request in arb_content_request()) {
        let bytes = request.to_bytes().expect("Serialization should succeed");
        let restored = ContentRequest::from_bytes(&bytes).expect("Deserialization should succeed");
        prop_assert_eq!(request, restored);
    }

    /// All ContentResponse variants roundtrip correctly
    #[test]
    fn prop_content_response_roundtrip(response in arb_content_response()) {
        let bytes = response.to_bytes().expect("Serialization should succeed");
        let restored = ContentResponse::from_bytes(&bytes).expect("Deserialization should succeed");
        prop_assert_eq!(response, restored);
    }

    /// Serialized size is always bounded by MAX_MESSAGE_SIZE
    #[test]
    fn prop_serialized_size_bounded(request in arb_content_request()) {
        let bytes = request.to_bytes().expect("Serialization should succeed");
        prop_assert!(
            bytes.len() <= MAX_MESSAGE_SIZE,
            "Serialized request size {} exceeds max {}",
            bytes.len(),
            MAX_MESSAGE_SIZE
        );
    }

    /// Property: Request type identification is consistent
    #[test]
    fn prop_request_type_identification(request in arb_content_request()) {
        // Exactly one of these should be true
        let is_block = request.is_block_request();
        let is_manifest = request.is_manifest_request();

        prop_assert!(
            is_block ^ is_manifest,
            "Request must be exactly one type: block={}, manifest={}",
            is_block, is_manifest
        );
    }

    /// Property: Response success identification is consistent
    #[test]
    fn prop_response_success_identification(response in arb_content_response()) {
        let is_success = response.is_success();
        let is_not_found = response.is_not_found();
        let is_error = response.is_error();

        // Exactly one category should be true
        let count = [is_success, is_not_found, is_error]
            .iter()
            .filter(|&&b| b)
            .count();

        // Note: is_success covers Block and Manifest
        prop_assert!(
            count == 1,
            "Response must be in exactly one category: success={}, not_found={}, error={}",
            is_success, is_not_found, is_error
        );
    }

    /// Property: CID accessor always returns consistent value
    #[test]
    fn prop_cid_accessor_consistency(request in arb_content_request()) {
        let cid1 = request.cid();
        let cid2 = request.cid();
        prop_assert_eq!(cid1, cid2, "CID accessor must be deterministic");
    }

    /// Property: Serialization is deterministic
    #[test]
    fn prop_serialization_deterministic(request in arb_content_request()) {
        let bytes1 = request.to_bytes().unwrap();
        let bytes2 = request.to_bytes().unwrap();
        prop_assert_eq!(bytes1, bytes2, "Serialization must be deterministic");
    }

    /// Property: Random bytes should not deserialize successfully (usually)
    #[test]
    fn prop_random_bytes_fail_safely(bytes in prop::collection::vec(any::<u8>(), 1..500)) {
        // Should not panic, may or may not succeed
        let _ = ContentRequest::from_bytes(&bytes);
        let _ = ContentResponse::from_bytes(&bytes);
        // Test passes if we get here without panic
    }
}

// ============================================================================
// Additional Property Tests for Edge Cases
// ============================================================================

proptest! {
    /// Property: Empty data blocks roundtrip
    #[test]
    fn prop_empty_data_block_roundtrip(
        cid in arb_content_id(),
        links in prop::collection::vec(arb_content_id(), 0..10)
    ) {
        let response = ContentResponse::Block {
            cid: cid.clone(),
            data: vec![],
            links,
        };

        let bytes = response.to_bytes().unwrap();
        let restored = ContentResponse::from_bytes(&bytes).unwrap();
        prop_assert_eq!(response, restored);
    }

    /// Property: All ErrorCode variants serialize correctly
    #[test]
    fn prop_error_code_roundtrip(
        code in arb_error_code(),
        message in ".*"
    ) {
        let response = ContentResponse::Error { code, message: message.clone() };
        let bytes = response.to_bytes().unwrap();
        let restored = ContentResponse::from_bytes(&bytes).unwrap();

        if let ContentResponse::Error { code: c, message: m } = restored {
            prop_assert_eq!(code, c);
            prop_assert_eq!(message, m);
        } else {
            prop_assert!(false, "Expected Error variant");
        }
    }
}
