use node::modules::ssi::did::resolvers::peer::generator::PeerDidGenerator;
use tracing::info;

// ============================================================================
// Parser Tests - Numalgo 0 (Inception Key)
// ============================================================================

#[tokio::test]
async fn test_parse_peer_did_numalgo0_ed25519() {
    // Generate a valid did:peer:0 with Ed25519 key
    let ed25519_key = [
        0x11, 0xa9, 0x80, 0x01, 0x82, 0xb1, 0x0a, 0xb7, 0xd5, 0x4b, 0xfe, 0xd3, 0xc9, 0x64, 0x07,
        0x3a, 0x0e, 0xe1, 0x72, 0xf3, 0xda, 0xa6, 0x23, 0x25, 0xaf, 0x02, 0x1a, 0x68, 0xf7, 0x07,
        0x51, 0x1a,
    ];

    let did = PeerDidGenerator::from_ed25519_bytes(&ed25519_key)
        .expect("Should generate valid did:peer:0");

    info!("Generated Ed25519 DID: {}", did);

    // Verify format
    assert!(
        did.starts_with("did:peer:0"),
        "Should start with did:peer:0"
    );
    assert!(did.len() > 20, "Should have sufficient length");

    // Now resolve it to verify round-trip
    use node::modules::ssi::did::resolvers::peer::resolve_peer_did;
    use node::modules::ssi::did::types::ResolutionOptions;

    let result = resolve_peer_did(&did, &ResolutionOptions::default())
        .await
        .expect("Should resolve successfully");

    // Verify DID Document
    let doc = result.did_document.expect("Should have DID document");
    assert_eq!(doc.id.to_string(), did);
    assert!(
        !doc.verification_method.is_empty(),
        "Should have verification methods"
    );

    // Verify it's Ed25519
    let vm = &doc.verification_method[0];
    assert_eq!(
        vm.type_, "Ed25519VerificationKey2020",
        "Should be Ed25519 verification key"
    );

    info!("✓ Successfully parsed and resolved Ed25519 did:peer:0");
}

#[tokio::test]
async fn test_parse_peer_did_numalgo0_x25519() {
    // Generate a valid did:peer:0 with X25519 key
    let x25519_key = [
        0x3d, 0x4d, 0xea, 0x2c, 0xb5, 0x52, 0xf1, 0x9e, 0x51, 0x2a, 0x8c, 0x4b, 0x88, 0xa7, 0x50,
        0x74, 0xed, 0x57, 0x6e, 0x93, 0x0f, 0x2f, 0x9e, 0x8d, 0xc8, 0xd5, 0xd7, 0xa9, 0xa3, 0x51,
        0x76, 0x21,
    ];

    let did =
        PeerDidGenerator::from_x25519_bytes(&x25519_key).expect("Should generate valid did:peer:0");

    info!("Generated X25519 DID: {}", did);

    assert!(
        did.starts_with("did:peer:0"),
        "Should start with did:peer:0"
    );

    // Resolve to verify
    use node::modules::ssi::did::resolvers::peer::resolve_peer_did;
    use node::modules::ssi::did::types::ResolutionOptions;

    let result = resolve_peer_did(&did, &ResolutionOptions::default())
        .await
        .expect("Should resolve successfully");

    let doc = result.did_document.expect("Should have DID document");
    assert_eq!(doc.id.to_string(), did);

    // Verify it's X25519
    let vm = &doc.verification_method[0];
    assert_eq!(
        vm.type_, "X25519KeyAgreementKey2020",
        "Should be X25519 key agreement key"
    );

    info!("✓ Successfully parsed and resolved X25519 did:peer:0");
}

#[tokio::test]
async fn test_parse_peer_did_numalgo0_p256() {
    // For P-256, we need to test with a manually constructed DID since the generator
    // requires a COSE key. We'll construct a valid did:peer:0 with P-256 multicodec.

    // P-256 public key (x and y coordinates, 32 bytes each = 64 bytes total)
    let p256_key = [0x42u8; 64]; // Dummy but valid length

    // Multicodec prefix for P-256: 0x8024
    let mut multicodec_key = vec![0x80, 0x24];
    multicodec_key.extend_from_slice(&p256_key);

    // Encode as multibase (base58btc)
    let encoded = multibase::encode(multibase::Base::Base58Btc, &multicodec_key);
    let did = format!("did:peer:0{}", encoded);

    info!("Generated P-256 DID: {}", did);

    // Resolve to verify
    use node::modules::ssi::did::resolvers::peer::resolve_peer_did;
    use node::modules::ssi::did::types::ResolutionOptions;

    let result = resolve_peer_did(&did, &ResolutionOptions::default())
        .await
        .expect("Should resolve P-256 did:peer:0");

    let doc = result.did_document.expect("Should have DID document");
    assert_eq!(doc.id.to_string(), did);

    // Verify it's P-256 (JsonWebKey2020)
    let vm = &doc.verification_method[0];
    assert_eq!(vm.type_, "JsonWebKey2020", "Should be JWK for P-256");

    info!("✓ Successfully parsed and resolved P-256 did:peer:0");
}

#[tokio::test]
async fn test_parse_peer_did_numalgo0_secp256k1() {
    // Secp256k1 public key (33 bytes compressed, or 65 bytes uncompressed)
    // Using compressed format (33 bytes)
    let secp256k1_key = [
        0x02, // Compressed key prefix
        0x79, 0xbe, 0x66, 0x7e, 0xf9, 0xdc, 0xbb, 0xac, 0x55, 0xa0, 0x62, 0x95, 0xce, 0x87, 0x0b,
        0x07, 0x02, 0x9b, 0xfc, 0xdb, 0x2d, 0xce, 0x28, 0xd9, 0x59, 0xf2, 0x81, 0x5b, 0x16, 0xf8,
        0x17, 0x98,
    ];

    // Multicodec prefix for Secp256k1: 0xe701
    let mut multicodec_key = vec![0xe7, 0x01];
    multicodec_key.extend_from_slice(&secp256k1_key);

    let encoded = multibase::encode(multibase::Base::Base58Btc, &multicodec_key);
    let did = format!("did:peer:0{}", encoded);

    info!("Generated Secp256k1 DID: {}", did);

    // Resolve to verify
    use node::modules::ssi::did::resolvers::peer::resolve_peer_did;
    use node::modules::ssi::did::types::ResolutionOptions;

    let result = resolve_peer_did(&did, &ResolutionOptions::default())
        .await
        .expect("Should resolve Secp256k1 did:peer:0");

    let doc = result.did_document.expect("Should have DID document");
    assert_eq!(doc.id.to_string(), did);

    // Verify it's Secp256k1
    let vm = &doc.verification_method[0];
    assert_eq!(
        vm.type_, "EcdsaSecp256k1VerificationKey2019",
        "Should be Secp256k1 verification key"
    );

    info!("✓ Successfully parsed and resolved Secp256k1 did:peer:0");
}

#[tokio::test]
async fn test_parse_peer_did_numalgo0_invalid_multibase() {
    // Invalid multibase encoding (contains invalid base58 characters)
    let invalid_did = "did:peer:0z!!!INVALID!!!";

    use node::modules::ssi::did::resolvers::peer::resolve_peer_did;
    use node::modules::ssi::did::types::ResolutionOptions;

    let result = resolve_peer_did(invalid_did, &ResolutionOptions::default()).await;

    assert!(
        result.is_err(),
        "Should fail to parse invalid multibase encoding"
    );

    if let Err(e) = result {
        info!("Expected error for invalid multibase: {}", e);
        // Should be an InvalidEncoding or MultibaseError
        assert!(
            e.to_string().contains("multibase")
                || e.to_string().contains("encoding")
                || e.to_string().contains("Invalid"),
            "Error should mention encoding issue"
        );
    }

    info!("✓ Correctly rejected invalid multibase encoding");
}

#[tokio::test]
async fn test_parse_peer_did_numalgo0_short_key() {
    // Key that's too short (only 1 byte after multicodec)
    let short_data = vec![0xed]; // Only multicodec, no key data

    let encoded = multibase::encode(multibase::Base::Base58Btc, &short_data);
    let did = format!("did:peer:0{}", encoded);

    info!("Testing short key DID: {}", did);

    use node::modules::ssi::did::resolvers::peer::resolve_peer_did;
    use node::modules::ssi::did::types::ResolutionOptions;

    let result = resolve_peer_did(&did, &ResolutionOptions::default()).await;

    assert!(result.is_err(), "Should fail to parse key that's too short");

    if let Err(e) = result {
        info!("Expected error for short key: {}", e);
        assert!(
            e.to_string().contains("short") || e.to_string().contains("Invalid"),
            "Error should mention key being too short"
        );
    }

    info!("✓ Correctly rejected key that's too short");
}

#[tokio::test]
async fn test_parse_peer_did_numalgo0_wrong_multicodec() {
    // Use an unsupported multicodec prefix
    let unsupported_key = [0x99u8; 32]; // 0x99 is not a supported multicodec

    let mut multicodec_key = vec![0x99, 0x01]; // Unsupported prefix
    multicodec_key.extend_from_slice(&unsupported_key);

    let encoded = multibase::encode(multibase::Base::Base58Btc, &multicodec_key);
    let did = format!("did:peer:0{}", encoded);

    info!("Testing wrong multicodec DID: {}", did);

    use node::modules::ssi::did::resolvers::peer::resolve_peer_did;
    use node::modules::ssi::did::types::ResolutionOptions;

    let result = resolve_peer_did(&did, &ResolutionOptions::default()).await;

    assert!(
        result.is_err(),
        "Should fail to parse unsupported multicodec prefix"
    );

    if let Err(e) = result {
        info!("Expected error for wrong multicodec: {}", e);
        assert!(
            e.to_string().contains("key type")
                || e.to_string().contains("Unsupported")
                || e.to_string().contains("method"),
            "Error should mention unsupported key type"
        );
    }

    info!("✓ Correctly rejected unsupported multicodec prefix");
}

// ============================================================================
// Additional Edge Case Tests
// ============================================================================

#[tokio::test]
async fn test_parse_peer_did_empty_string() {
    use node::modules::ssi::did::resolvers::peer::resolve_peer_did;
    use node::modules::ssi::did::types::ResolutionOptions;

    let result = resolve_peer_did("", &ResolutionOptions::default()).await;
    assert!(result.is_err(), "Should reject empty string");

    info!("✓ Correctly rejected empty DID string");
}

#[tokio::test]
async fn test_parse_peer_did_missing_numalgo() {
    use node::modules::ssi::did::resolvers::peer::resolve_peer_did;
    use node::modules::ssi::did::types::ResolutionOptions;

    let result = resolve_peer_did("did:peer:", &ResolutionOptions::default()).await;
    assert!(result.is_err(), "Should reject DID without numalgo");

    info!("✓ Correctly rejected DID without numalgo");
}

#[tokio::test]
async fn test_parse_peer_did_unsupported_numalgo() {
    use node::modules::ssi::did::resolvers::peer::resolve_peer_did;
    use node::modules::ssi::did::types::ResolutionOptions;

    // Numalgo 3 is not currently supported
    let result = resolve_peer_did("did:peer:3abc123", &ResolutionOptions::default()).await;
    assert!(result.is_err(), "Should reject unsupported numalgo");

    if let Err(e) = result {
        let err_msg = e.to_string();
        info!("Error message: {}", err_msg);
        assert!(
            err_msg.contains("not supported") || err_msg.contains("method"),
            "Error should mention method not supported, got: {}",
            err_msg
        );
    }

    info!("✓ Correctly rejected unsupported numalgo");
}

#[tokio::test]
async fn test_parse_peer_did_wrong_method() {
    use node::modules::ssi::did::resolvers::peer::resolve_peer_did;
    use node::modules::ssi::did::types::ResolutionOptions;

    // Not a did:peer
    let result = resolve_peer_did(
        "did:key:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRoAnwWsdvktH",
        &ResolutionOptions::default(),
    )
    .await;
    assert!(result.is_err(), "Should reject non-peer DID");

    info!("✓ Correctly rejected non-peer DID method");
}

// ============================================================================
// Parser Tests - Numalgo 2 (Multiple Keys + Services)
// ============================================================================

#[tokio::test]
async fn test_parse_peer_did_numalgo2_single_verification_key() {
    use node::modules::ssi::did::resolvers::peer::resolve_peer_did;
    use node::modules::ssi::did::types::ResolutionOptions;

    // Create a simple Ed25519 key for verification
    let ed25519_key = [0x42u8; 32];
    let mut multicodec_key = vec![0xed, 0x01];
    multicodec_key.extend_from_slice(&ed25519_key);

    // Encode as multibase
    let encoded_key = multibase::encode(multibase::Base::Base58Btc, &multicodec_key);

    // Create did:peer:2 with single verification key (E transform)
    let did = format!("did:peer:2.E{}", encoded_key);

    info!("Testing numalgo2 single key: {}", did);

    let result = resolve_peer_did(&did, &ResolutionOptions::default())
        .await
        .expect("Should resolve numalgo:2 with single verification key");

    let doc = result.did_document.expect("Should have DID document");
    assert_eq!(doc.id.to_string(), did);
    assert_eq!(
        doc.verification_method.len(),
        1,
        "Should have one verification method"
    );

    // Verify it's used for authentication and assertion
    assert!(
        !doc.verification_relationships.authentication.is_empty(),
        "Should have authentication relationship"
    );
    assert!(
        !doc.verification_relationships.assertion_method.is_empty(),
        "Should have assertion method relationship"
    );

    info!("✓ Successfully parsed numalgo:2 with single verification key");
}

#[tokio::test]
async fn test_parse_peer_did_numalgo2_multiple_keys() {
    use node::modules::ssi::did::resolvers::peer::resolve_peer_did;
    use node::modules::ssi::did::types::ResolutionOptions;

    // Create Ed25519 verification key
    let ed25519_key = [0x11u8; 32];
    let mut ed_multicodec = vec![0xed, 0x01];
    ed_multicodec.extend_from_slice(&ed25519_key);
    let ed_encoded = multibase::encode(multibase::Base::Base58Btc, &ed_multicodec);

    // Create X25519 key agreement key
    let x25519_key = [0x22u8; 32];
    let mut x_multicodec = vec![0xec, 0x01];
    x_multicodec.extend_from_slice(&x25519_key);
    let x_encoded = multibase::encode(multibase::Base::Base58Btc, &x_multicodec);

    // Create did:peer:2 with both keys
    let did = format!("did:peer:2.E{}.V{}", ed_encoded, x_encoded);

    info!("Testing numalgo2 multiple keys: {}", did);

    let result = resolve_peer_did(&did, &ResolutionOptions::default())
        .await
        .expect("Should resolve numalgo:2 with multiple keys");

    let doc = result.did_document.expect("Should have DID document");
    assert_eq!(doc.id.to_string(), did);
    assert_eq!(
        doc.verification_method.len(),
        2,
        "Should have two verification methods"
    );

    // Check that we have both types of relationships
    assert!(
        !doc.verification_relationships.authentication.is_empty(),
        "Should have authentication (from E key)"
    );
    assert!(
        !doc.verification_relationships.key_agreement.is_empty(),
        "Should have key agreement (from V key)"
    );

    // Verify key types
    let vm_types: Vec<&str> = doc
        .verification_method
        .iter()
        .map(|vm| vm.type_.as_str())
        .collect();
    assert!(
        vm_types.contains(&"Ed25519VerificationKey2020"),
        "Should have Ed25519 key"
    );
    assert!(
        vm_types.contains(&"X25519KeyAgreementKey2020"),
        "Should have X25519 key"
    );

    info!("✓ Successfully parsed numalgo:2 with multiple keys");
}

#[tokio::test]
async fn test_parse_peer_did_numalgo2_with_service_endpoint() {
    use base64::Engine;
    use node::modules::ssi::did::resolvers::peer::resolve_peer_did;
    use node::modules::ssi::did::types::ResolutionOptions;

    // Create a verification key
    let ed25519_key = [0x33u8; 32];
    let mut multicodec_key = vec![0xed, 0x01];
    multicodec_key.extend_from_slice(&ed25519_key);
    let key_encoded = multibase::encode(multibase::Base::Base58Btc, &multicodec_key);

    // Create service endpoint JSON
    let service_json = serde_json::json!({
        "t": "DIDCommMessaging",
        "s": "https://example.com/endpoint",
        "r": ["did:example:somemediator#somekey"],
        "a": ["didcomm/v2", "didcomm/aip2;env=rfc587"]
    });

    // Encode service as base64url (no padding)
    let service_str = service_json.to_string();
    let service_encoded =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(service_str.as_bytes());

    // Create did:peer:2 with key and service
    let did = format!("did:peer:2.E{}.S{}", key_encoded, service_encoded);

    info!("Testing numalgo2 with service: {}", did);

    let result = resolve_peer_did(&did, &ResolutionOptions::default())
        .await
        .expect("Should resolve numalgo:2 with service endpoint");

    let doc = result.did_document.expect("Should have DID document");
    assert_eq!(doc.id.to_string(), did);
    assert_eq!(
        doc.verification_method.len(),
        1,
        "Should have one verification method"
    );
    assert_eq!(doc.service.len(), 1, "Should have one service");

    // Verify service properties
    let service = &doc.service[0];
    assert_eq!(
        service.type_.first().unwrap(),
        "DIDCommMessaging",
        "Service type should match"
    );

    info!("✓ Successfully parsed numalgo:2 with service endpoint");
}

#[tokio::test]
async fn test_parse_peer_did_numalgo2_mixed_keys_and_services() {
    use base64::Engine;
    use node::modules::ssi::did::resolvers::peer::resolve_peer_did;
    use node::modules::ssi::did::types::ResolutionOptions;

    // Create verification key (Ed25519)
    let ed_key = [0x44u8; 32];
    let mut ed_multicodec = vec![0xed, 0x01];
    ed_multicodec.extend_from_slice(&ed_key);
    let ed_encoded = multibase::encode(multibase::Base::Base58Btc, &ed_multicodec);

    // Create key agreement key (X25519)
    let x_key = [0x55u8; 32];
    let mut x_multicodec = vec![0xec, 0x01];
    x_multicodec.extend_from_slice(&x_key);
    let x_encoded = multibase::encode(multibase::Base::Base58Btc, &x_multicodec);

    // Create authentication key (another Ed25519)
    let auth_key = [0x66u8; 32];
    let mut auth_multicodec = vec![0xed, 0x01];
    auth_multicodec.extend_from_slice(&auth_key);
    let auth_encoded = multibase::encode(multibase::Base::Base58Btc, &auth_multicodec);

    // Create first service
    let service1 = serde_json::json!({
        "t": "DIDCommMessaging",
        "s": "https://example.com/alice"
    });
    let service1_encoded =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(service1.to_string().as_bytes());

    // Create second service
    let service2 = serde_json::json!({
        "t": "LinkedDomains",
        "s": "https://alice.example.com"
    });
    let service2_encoded =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(service2.to_string().as_bytes());

    // Create complex did:peer:2 with multiple keys and services
    let did = format!(
        "did:peer:2.E{}.V{}.A{}.S{}.S{}",
        ed_encoded, x_encoded, auth_encoded, service1_encoded, service2_encoded
    );

    info!("Testing numalgo2 mixed: {}", did);

    let result = resolve_peer_did(&did, &ResolutionOptions::default())
        .await
        .expect("Should resolve numalgo:2 with mixed keys and services");

    let doc = result.did_document.expect("Should have DID document");
    assert_eq!(doc.id.to_string(), did);
    assert_eq!(
        doc.verification_method.len(),
        3,
        "Should have three verification methods"
    );
    assert_eq!(doc.service.len(), 2, "Should have two services");

    // Verify we have all the different relationship types
    assert!(
        !doc.verification_relationships.authentication.is_empty(),
        "Should have authentication relationships"
    );
    assert!(
        !doc.verification_relationships.assertion_method.is_empty(),
        "Should have assertion method relationships"
    );
    assert!(
        !doc.verification_relationships.key_agreement.is_empty(),
        "Should have key agreement relationship"
    );

    info!("✓ Successfully parsed complex numalgo:2 with mixed keys and services");
}

#[tokio::test]
async fn test_parse_peer_did_numalgo2_malformed_service() {
    use base64::Engine;
    use node::modules::ssi::did::resolvers::peer::resolve_peer_did;
    use node::modules::ssi::did::types::ResolutionOptions;

    // Create a valid key
    let ed_key = [0x77u8; 32];
    let mut multicodec_key = vec![0xed, 0x01];
    multicodec_key.extend_from_slice(&ed_key);
    let key_encoded = multibase::encode(multibase::Base::Base58Btc, &multicodec_key);

    // Create malformed service (invalid JSON)
    let malformed_service = "{this is not valid json!!!";
    let service_encoded =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(malformed_service.as_bytes());

    let did = format!("did:peer:2.E{}.S{}", key_encoded, service_encoded);

    info!("Testing numalgo2 with malformed service: {}", did);

    let result = resolve_peer_did(&did, &ResolutionOptions::default()).await;

    assert!(result.is_err(), "Should fail to parse malformed service");

    if let Err(e) = result {
        info!("Expected error for malformed service: {}", e);
        let err_msg = e.to_string();
        assert!(
            err_msg.contains("JSON") || err_msg.contains("json") || err_msg.contains("failed"),
            "Error should mention JSON parsing issue, got: {}",
            err_msg
        );
    }

    info!("✓ Correctly rejected malformed service endpoint");
}

#[tokio::test]
async fn test_parse_peer_did_numalgo2_unknown_transform() {
    use node::modules::ssi::did::resolvers::peer::resolve_peer_did;
    use node::modules::ssi::did::types::ResolutionOptions;

    // Create a valid verification key
    let ed_key = [0x88u8; 32];
    let mut multicodec_key = vec![0xed, 0x01];
    multicodec_key.extend_from_slice(&ed_key);
    let key_encoded = multibase::encode(multibase::Base::Base58Btc, &multicodec_key);

    // Include an unknown transform 'X' (not E, V, A, or S)
    let did = format!("did:peer:2.E{}.Xz6MkpTHR8VNsBxY", key_encoded);

    info!("Testing numalgo2 with unknown transform: {}", did);

    // According to the implementation, unknown transforms are logged as warnings
    // but don't cause failure - they're just skipped
    let result = resolve_peer_did(&did, &ResolutionOptions::default())
        .await
        .expect("Should resolve, ignoring unknown transform");

    let doc = result.did_document.expect("Should have DID document");
    assert_eq!(doc.id.to_string(), did);
    assert_eq!(
        doc.verification_method.len(),
        1,
        "Should have one verification method (unknown transform ignored)"
    );

    info!("✓ Successfully handled unknown transform (ignored as per spec)");
}

#[tokio::test]
async fn test_parse_peer_did_numalgo2_empty_parts() {
    use node::modules::ssi::did::resolvers::peer::resolve_peer_did;
    use node::modules::ssi::did::types::ResolutionOptions;

    // Test with trailing dots and empty parts
    let ed_key = [0x99u8; 32];
    let mut multicodec_key = vec![0xed, 0x01];
    multicodec_key.extend_from_slice(&ed_key);
    let key_encoded = multibase::encode(multibase::Base::Base58Btc, &multicodec_key);

    // DID with extra dots (should be filtered out)
    let did = format!("did:peer:2.E{}...", key_encoded);

    info!("Testing numalgo2 with empty parts: {}", did);

    // Empty parts should be filtered out by the parser
    let result = resolve_peer_did(&did, &ResolutionOptions::default())
        .await
        .expect("Should resolve, filtering empty parts");

    let doc = result.did_document.expect("Should have DID document");
    assert_eq!(doc.id.to_string(), did);
    assert_eq!(
        doc.verification_method.len(),
        1,
        "Should have one verification method"
    );

    info!("✓ Successfully handled empty parts in numalgo:2");
}

// ============================================================================
// Edge Case: Numalgo 2 with no keys or services
// ============================================================================

#[tokio::test]
async fn test_parse_peer_did_numalgo2_no_content() {
    use node::modules::ssi::did::resolvers::peer::resolve_peer_did;
    use node::modules::ssi::did::types::ResolutionOptions;

    // Numalgo 2 with no keys or services (just "did:peer:2")
    let did = "did:peer:2";

    info!("Testing numalgo2 with no content: {}", did);

    let result = resolve_peer_did(did, &ResolutionOptions::default())
        .await
        .expect("Should parse but create minimal DID document");

    let doc = result.did_document.expect("Should have DID document");
    assert_eq!(doc.id.to_string(), did);
    assert_eq!(
        doc.verification_method.len(),
        0,
        "Should have no verification methods"
    );
    assert_eq!(doc.service.len(), 0, "Should have no services");

    info!("✓ Successfully handled empty numalgo:2 DID");
}

// ============================================================================
// Document Generation Tests
// ============================================================================

#[tokio::test]
async fn test_create_did_document_from_parsed_peer() {
    use node::modules::ssi::did::resolvers::peer::resolve_peer_did;
    use node::modules::ssi::did::types::ResolutionOptions;

    // Generate a simple did:peer:0
    let ed25519_key = [0xAAu8; 32];
    let mut multicodec_key = vec![0xed, 0x01];
    multicodec_key.extend_from_slice(&ed25519_key);
    let encoded = multibase::encode(multibase::Base::Base58Btc, &multicodec_key);
    let did = format!("did:peer:0{}", encoded);

    let result = resolve_peer_did(&did, &ResolutionOptions::default())
        .await
        .expect("Should resolve and create DID document");

    let doc = result.did_document.expect("Should have DID document");

    // Verify document structure
    assert_eq!(doc.id.to_string(), did, "Document ID should match DID");
    assert!(
        !doc.verification_method.is_empty(),
        "Should have verification methods"
    );

    println!("✓ Successfully created DID document from parsed peer DID");
}

#[tokio::test]
async fn test_did_document_verification_methods_correct() {
    use node::modules::ssi::did::resolvers::peer::resolve_peer_did;
    use node::modules::ssi::did::types::ResolutionOptions;

    // Create numalgo:2 with multiple key types
    let ed_key = [0xBBu8; 32];
    let mut ed_multicodec = vec![0xed, 0x01];
    ed_multicodec.extend_from_slice(&ed_key);
    let ed_encoded = multibase::encode(multibase::Base::Base58Btc, &ed_multicodec);

    let x_key = [0xCCu8; 32];
    let mut x_multicodec = vec![0xec, 0x01];
    x_multicodec.extend_from_slice(&x_key);
    let x_encoded = multibase::encode(multibase::Base::Base58Btc, &x_multicodec);

    let secp_key = [0xDDu8; 33];
    let mut secp_multicodec = vec![0xe7, 0x01];
    secp_multicodec.extend_from_slice(&secp_key);
    let secp_encoded = multibase::encode(multibase::Base::Base58Btc, &secp_multicodec);

    let did = format!(
        "did:peer:2.E{}.V{}.E{}",
        ed_encoded, x_encoded, secp_encoded
    );

    let result = resolve_peer_did(&did, &ResolutionOptions::default())
        .await
        .expect("Should resolve");

    let doc = result.did_document.expect("Should have DID document");

    // Verify we have 3 verification methods
    assert_eq!(
        doc.verification_method.len(),
        3,
        "Should have 3 verification methods"
    );

    // Verify each method has correct structure
    for (idx, vm) in doc.verification_method.iter().enumerate() {
        // Check ID format
        assert!(
            vm.id.to_string().contains(&format!("#key-{}", idx + 1)),
            "Verification method ID should have correct fragment: {}",
            vm.id
        );

        // Check controller matches DID
        assert_eq!(
            vm.controller.to_string(),
            did,
            "Controller should match DID"
        );

        // Check type is valid
        assert!(
            [
                "Ed25519VerificationKey2020",
                "X25519KeyAgreementKey2020",
                "EcdsaSecp256k1VerificationKey2019"
            ]
            .contains(&vm.type_.as_str()),
            "Verification method type should be valid: {}",
            vm.type_
        );

        // Check has public key material
        assert!(
            vm.properties.contains_key("publicKeyMultibase"),
            "Should have public key material"
        );
    }

    println!("✓ Verification methods correctly generated with proper structure");
}

#[tokio::test]
async fn test_did_document_service_endpoints_correct() {
    use base64::Engine;
    use node::modules::ssi::did::resolvers::peer::resolve_peer_did;
    use node::modules::ssi::did::types::ResolutionOptions;

    // Create a key
    let ed_key = [0xEEu8; 32];
    let mut multicodec_key = vec![0xed, 0x01];
    multicodec_key.extend_from_slice(&ed_key);
    let key_encoded = multibase::encode(multibase::Base::Base58Btc, &multicodec_key);

    // Create two different services
    let service1 = serde_json::json!({
        "t": "DIDCommMessaging",
        "s": "https://example.com/alice",
        "r": ["did:example:mediator#key1"],
        "a": ["didcomm/v2"]
    });
    let service1_encoded =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(service1.to_string().as_bytes());

    let service2 = serde_json::json!({
        "t": "LinkedDomains",
        "s": "https://alice.example.com"
    });
    let service2_encoded =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(service2.to_string().as_bytes());

    let did = format!(
        "did:peer:2.E{}.S{}.S{}",
        key_encoded, service1_encoded, service2_encoded
    );

    let result = resolve_peer_did(&did, &ResolutionOptions::default())
        .await
        .expect("Should resolve");

    let doc = result.did_document.expect("Should have DID document");

    // Verify service endpoints
    assert_eq!(doc.service.len(), 2, "Should have 2 service endpoints");

    // Check service IDs
    for (idx, service) in doc.service.iter().enumerate() {
        assert!(
            service
                .id
                .to_string()
                .contains(&format!("#service-{}", idx + 1)),
            "Service ID should have correct fragment"
        );

        assert!(
            service.service_endpoint.is_some(),
            "Service should have endpoint"
        );
    }

    // Verify service types
    let service_types: Vec<String> = doc
        .service
        .iter()
        .map(|s| s.type_.first().unwrap().clone())
        .collect();

    assert!(
        service_types.iter().any(|t| t.contains("DIDCommMessaging")),
        "Should have DIDCommMessaging service"
    );
    assert!(
        service_types.iter().any(|t| t.contains("LinkedDomains")),
        "Should have LinkedDomains service"
    );

    println!("✓ Service endpoints correctly generated with proper structure");
}

#[tokio::test]
async fn test_did_document_relationships_correct() {
    use node::modules::ssi::did::resolvers::peer::resolve_peer_did;
    use node::modules::ssi::did::types::ResolutionOptions;

    // Create DID with different key purposes
    let ed_key = [0xFFu8; 32];
    let mut ed_multicodec = vec![0xed, 0x01];
    ed_multicodec.extend_from_slice(&ed_key);
    let ed_encoded = multibase::encode(multibase::Base::Base58Btc, &ed_multicodec);

    let x_key = [0x11u8; 32];
    let mut x_multicodec = vec![0xec, 0x01];
    x_multicodec.extend_from_slice(&x_key);
    let x_encoded = multibase::encode(multibase::Base::Base58Btc, &x_multicodec);

    let auth_key = [0x22u8; 32];
    let mut auth_multicodec = vec![0xed, 0x01];
    auth_multicodec.extend_from_slice(&auth_key);
    let auth_encoded = multibase::encode(multibase::Base::Base58Btc, &auth_multicodec);

    // E=Verification, V=KeyAgreement, A=Authentication
    let did = format!(
        "did:peer:2.E{}.V{}.A{}",
        ed_encoded, x_encoded, auth_encoded
    );

    let result = resolve_peer_did(&did, &ResolutionOptions::default())
        .await
        .expect("Should resolve");

    let doc = result.did_document.expect("Should have DID document");

    // Verify verification relationships
    let rels = &doc.verification_relationships;

    // E and A keys should be in authentication (2 keys)
    assert_eq!(
        rels.authentication.len(),
        2,
        "Should have 2 authentication relationships (E + A)"
    );

    // E and A keys should be in assertion method (2 keys)
    assert_eq!(
        rels.assertion_method.len(),
        2,
        "Should have 2 assertion method relationships (E + A)"
    );

    // Only V key should be in key agreement (1 key)
    assert_eq!(
        rels.key_agreement.len(),
        1,
        "Should have 1 key agreement relationship (V)"
    );

    // Verify the references point to actual verification methods
    for auth_ref in &rels.authentication {
        match auth_ref {
            ssi::dids::document::verification_method::ValueOrReference::Reference(id) => {
                let found = match id {
                    ssi::dids::DIDURLReferenceBuf::Absolute(absolute_id) => doc
                        .verification_method
                        .iter()
                        .any(|vm| vm.id == *absolute_id),
                    ssi::dids::DIDURLReferenceBuf::Relative(_) => false,
                };
                assert!(
                    found,
                    "Authentication reference should point to existing VM"
                );
            }
            _ => {}
        }
    }

    println!("✓ Verification relationships correctly assigned based on key purpose");
}

#[tokio::test]
async fn test_resolution_metadata_correct() {
    use node::modules::ssi::did::resolvers::peer::resolve_peer_did;
    use node::modules::ssi::did::types::ResolutionOptions;

    let ed_key = [0x33u8; 32];
    let mut multicodec_key = vec![0xed, 0x01];
    multicodec_key.extend_from_slice(&ed_key);
    let encoded = multibase::encode(multibase::Base::Base58Btc, &multicodec_key);
    let did = format!("did:peer:0{}", encoded);

    let result = resolve_peer_did(&did, &ResolutionOptions::default())
        .await
        .expect("Should resolve");

    let metadata = result.did_resolution_metadata;

    // Verify standard metadata fields
    assert_eq!(
        metadata.content_type,
        Some("application/did+json".to_string()),
        "Content type should be application/did+json"
    );

    assert!(metadata.error.is_none(), "Should not have error");

    assert_eq!(
        metadata.did_method,
        Some("peer".to_string()),
        "DID method should be 'peer'"
    );

    // Verify timing metadata
    assert!(metadata.duration.is_some(), "Should have duration");
    assert!(
        metadata.resolved_at.is_some(),
        "Should have resolved_at timestamp"
    );

    assert_eq!(
        metadata.from_cache,
        Some(false),
        "Should indicate not from cache"
    );

    println!("✓ Resolution metadata contains correct standard fields");
}

#[tokio::test]
async fn test_resolution_vdr_info_peer() {
    use node::modules::ssi::did::resolvers::peer::resolve_peer_did;
    use node::modules::ssi::did::types::{RegistryProof, ResolutionOptions};

    let ed_key = [0x44u8; 32];
    let mut multicodec_key = vec![0xed, 0x01];
    multicodec_key.extend_from_slice(&ed_key);
    let encoded = multibase::encode(multibase::Base::Base58Btc, &multicodec_key);
    let did = format!("did:peer:0{}", encoded);

    let result = resolve_peer_did(&did, &ResolutionOptions::default())
        .await
        .expect("Should resolve");

    let metadata = result.did_resolution_metadata;

    // Verify VDR info exists
    let vdr = metadata
        .verifiable_data_registry
        .expect("Should have VDR info for peer DIDs");

    // Verify VDR fields
    assert_eq!(
        vdr.registry_type, "peer-to-peer",
        "Registry type should be peer-to-peer"
    );
    assert!(
        vdr.verified,
        "Peer DIDs are self-certifying, should be verified"
    );
    assert!(
        vdr.registry_endpoint.is_none(),
        "Peer DIDs have no registry endpoint"
    );

    assert_eq!(
        vdr.registry_version,
        Some("did:peer:2".to_string()),
        "Should indicate did:peer version"
    );

    // Verify registry proof
    let proof = vdr.registry_proof.expect("Should have registry proof");

    match proof {
        RegistryProof::CryptographicProof {
            signature,
            signature_algorithm,
            public_key_id,
            signed_data,
        } => {
            assert_eq!(signature, "self-certifying", "Should be self-certifying");
            assert_eq!(
                signature_algorithm, "embedded-peer",
                "Algorithm should be embedded-peer"
            );
            assert_eq!(public_key_id, did, "Public key ID should be the DID itself");
            assert_eq!(signed_data, did, "Signed data should be the DID itself");
        }
        _ => panic!("Expected CryptographicProof for peer DID"),
    }

    println!("✓ VDR info correctly populated for peer DIDs");
}

// ============================================================================
// Generator Tests
// ============================================================================

#[tokio::test]
async fn test_generate_numalgo2_multi_key() {
    use node::modules::ssi::did::resolvers::peer::generator::PeerDidGenerator;
    use node::modules::ssi::did::resolvers::peer::resolve_peer_did;
    use node::modules::ssi::did::types::ResolutionOptions;

    // Generate verification and encryption keys
    let verification_key = vec![0xAAu8; 32];
    let encryption_key = vec![0xBBu8; 32];

    let did = PeerDidGenerator::generate_numalgo2(vec![verification_key], vec![encryption_key])
        .expect("Should generate numalgo:2 DID");

    println!("Generated numalgo:2 DID: {}", did);

    // Verify format
    assert!(
        did.starts_with("did:peer:2"),
        "Should start with did:peer:2"
    );
    assert!(
        did.contains(".E"),
        "Should contain verification key (E transform)"
    );
    assert!(
        did.contains(".V"),
        "Should contain encryption key (V transform)"
    );

    // Resolve to verify it's valid
    let result = resolve_peer_did(&did, &ResolutionOptions::default())
        .await
        .expect("Generated DID should be valid");

    let doc = result.did_document.expect("Should have DID document");
    assert_eq!(doc.verification_method.len(), 2, "Should have 2 keys");

    println!("✓ Successfully generated and validated numalgo:2 with multiple keys");
}

#[tokio::test]
async fn test_generate_numalgo2_with_encryption_key() {
    use node::modules::ssi::did::resolvers::peer::generator::PeerDidGenerator;
    use node::modules::ssi::did::resolvers::peer::resolve_peer_did;
    use node::modules::ssi::did::types::ResolutionOptions;

    // Generate with only encryption key
    let encryption_key1 = vec![0x11u8; 32];
    let encryption_key2 = vec![0x22u8; 32];

    let did = PeerDidGenerator::generate_numalgo2(
        vec![], // No verification keys
        vec![encryption_key1, encryption_key2],
    )
    .expect("Should generate with encryption keys only");

    println!("Generated encryption-only DID: {}", did);

    // Verify it has V transforms
    assert!(did.contains(".V"), "Should contain encryption keys");

    // Resolve and verify
    let result = resolve_peer_did(&did, &ResolutionOptions::default())
        .await
        .expect("Should resolve");

    let doc = result.did_document.expect("Should have DID document");

    // All keys should be in keyAgreement relationship
    assert_eq!(
        doc.verification_relationships.key_agreement.len(),
        2,
        "Should have 2 key agreement relationships"
    );

    // Verify key types
    for vm in &doc.verification_method {
        assert_eq!(
            vm.type_, "X25519KeyAgreementKey2020",
            "Should be X25519 key agreement keys"
        );
    }

    println!("✓ Successfully generated numalgo:2 with encryption keys");
}

#[test]
fn test_from_passkey_deterministic() {
    use node::modules::ssi::did::resolvers::peer::generator::PeerDidGenerator;

    // Note: This test demonstrates the concept but can't actually test with real passkeys
    // without WebAuthn infrastructure. In practice, the same passkey should always
    // generate the same DID.

    // Test that from_ed25519_bytes is deterministic
    let public_key = [0x55u8; 32];

    let did1 = PeerDidGenerator::from_ed25519_bytes(&public_key).expect("Should generate DID");
    let did2 = PeerDidGenerator::from_ed25519_bytes(&public_key).expect("Should generate DID");

    assert_eq!(did1, did2, "Same key should always produce same DID");

    println!("✓ DID generation is deterministic: {}", did1);
}

#[test]
fn test_from_passkey_different_keys_differ() {
    use node::modules::ssi::did::resolvers::peer::generator::PeerDidGenerator;

    // Different keys should produce different DIDs
    let key1 = [0x11u8; 32];
    let key2 = [0x22u8; 32];

    let did1 = PeerDidGenerator::from_ed25519_bytes(&key1).expect("Should generate first DID");
    let did2 = PeerDidGenerator::from_ed25519_bytes(&key2).expect("Should generate second DID");

    assert_ne!(did1, did2, "Different keys should produce different DIDs");

    println!("✓ Different keys produce different DIDs:");
    println!("  Key 1: {}", did1);
    println!("  Key 2: {}", did2);
}

// ============================================================================
// Additional Generator Tests
// ============================================================================

#[test]
fn test_generator_key_type_validation() {
    use node::modules::ssi::did::resolvers::peer::generator::PeerDidGenerator;

    // Test invalid key lengths
    let short_key = vec![0xAAu8; 16]; // Too short for Ed25519
    let result = PeerDidGenerator::from_ed25519_bytes(&short_key);
    assert!(result.is_err(), "Should reject short keys");

    let long_key = vec![0xBBu8; 64]; // Too long for Ed25519
    let result = PeerDidGenerator::from_ed25519_bytes(&long_key);
    assert!(result.is_err(), "Should reject long keys");

    // Test valid length
    let valid_key = vec![0xCCu8; 32];
    let result = PeerDidGenerator::from_ed25519_bytes(&valid_key);
    assert!(result.is_ok(), "Should accept 32-byte Ed25519 keys");

    println!("✓ Generator correctly validates key lengths");
}

#[test]
fn test_generator_x25519_validation() {
    use node::modules::ssi::did::resolvers::peer::generator::PeerDidGenerator;

    // Test X25519 key validation
    let short_key = vec![0xAAu8; 16];
    let result = PeerDidGenerator::from_x25519_bytes(&short_key);
    assert!(result.is_err(), "Should reject short X25519 keys");

    let valid_key = vec![0xBBu8; 32];
    let result = PeerDidGenerator::from_x25519_bytes(&valid_key);
    assert!(result.is_ok(), "Should accept 32-byte X25519 keys");

    println!("✓ Generator correctly validates X25519 keys");
}

#[tokio::test]
async fn test_round_trip_generation_and_resolution() {
    use node::modules::ssi::did::resolvers::peer::generator::PeerDidGenerator;
    use node::modules::ssi::did::resolvers::peer::resolve_peer_did;
    use node::modules::ssi::did::types::ResolutionOptions;

    // Generate a DID
    let public_key = [0x77u8; 32];
    let did = PeerDidGenerator::from_ed25519_bytes(&public_key).expect("Should generate DID");

    println!("Generated DID: {}", did);

    // Resolve it
    let result = resolve_peer_did(&did, &ResolutionOptions::default())
        .await
        .expect("Should resolve generated DID");

    let doc = result.did_document.expect("Should have DID document");

    // Verify round-trip
    assert_eq!(
        doc.id.to_string(),
        did,
        "Resolved DID should match generated"
    );
    assert_eq!(
        doc.verification_method.len(),
        1,
        "Should have one verification method"
    );

    // Verify the key material is present (though re-encoded with multicodec)
    let vm = &doc.verification_method[0];
    assert!(
        vm.properties.contains_key("publicKeyMultibase"),
        "Should contain public key"
    );

    println!("✓ Successfully completed generate → resolve round-trip");
}
