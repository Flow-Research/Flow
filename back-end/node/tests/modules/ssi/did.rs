use node::modules::ssi::did::util::{
    cose_to_jwk, create_did_document, did_document_to_json, extract_ec_coordinates,
    extract_eddsa_public_key, generate_did_key_from_passkey, generate_did_peer_from_passkey,
};
use ssi::jwk::Params as JWKParams;
use tracing::info;
use webauthn_rs::prelude::{
    COSEAlgorithm, COSEEC2Key, COSEKey, COSEKeyType, COSEOKPKey, ECDSACurve, EDDSACurve,
};

use crate::modules::ssi::fixtures::{load_eddsa_passkey, load_es256_passkey};

// ========== Helper Functions ==========

/// Create a mock ES256 COSE key
fn create_es256_cose_key() -> COSEKey {
    use base64::Engine;
    use base64::prelude::BASE64_URL_SAFE_NO_PAD;

    let x_bytes = BASE64_URL_SAFE_NO_PAD
        .decode("fLO-YipbYWNFU4De2Zrx-vkXV_0nJSyftd0g3CXmQvk")
        .unwrap();
    let y_bytes = BASE64_URL_SAFE_NO_PAD
        .decode("3UJufImjr2da-STs1-14FxWWviCE4uFsGjuXbDoeGsc")
        .unwrap();

    COSEKey {
        type_: COSEAlgorithm::ES256,
        key: COSEKeyType::EC_EC2(COSEEC2Key {
            curve: ECDSACurve::SECP256R1,
            x: x_bytes.into(),
            y: y_bytes.into(),
        }),
    }
}

/// Create a mock EdDSA COSE key
fn create_eddsa_cose_key() -> COSEKey {
    use base64::Engine;
    use base64::prelude::BASE64_URL_SAFE_NO_PAD;

    let x_bytes = BASE64_URL_SAFE_NO_PAD
        .decode("11qYAYKxCrfVS_7TyWQHOg7hcvPapiMlrwIaaPcHURo")
        .unwrap();

    COSEKey {
        type_: COSEAlgorithm::EDDSA,
        key: COSEKeyType::EC_OKP(COSEOKPKey {
            curve: EDDSACurve::ED25519,
            x: x_bytes.into(),
        }),
    }
}

// ========== DID Generation Tests ==========

#[test]
fn test_generate_did_key_from_es256_passkey() {
    let passkey = load_es256_passkey().0;

    let result = generate_did_key_from_passkey(&passkey);

    assert!(
        result.is_ok(),
        "Should successfully generate DID from ES256 passkey"
    );

    let did = result.unwrap();
    assert!(did.starts_with("did:key:"), "DID should use did:key method");
    assert!(did.len() > 20, "DID should have substantial length");

    info!("Generated ES256 DID: {}", did);
}

#[test]
fn test_generate_did_key_from_eddsa_passkey() {
    let passkey = load_eddsa_passkey().0;

    let result = generate_did_key_from_passkey(&passkey);

    assert!(
        result.is_ok(),
        "Should successfully generate DID from EdDSA passkey"
    );

    let did = result.unwrap();
    assert!(did.starts_with("did:key:"), "DID should use did:key method");
    assert!(did.len() > 20, "DID should have substantial length");

    info!("Generated EdDSA DID: {}", did);
}

// ========== COSE to JWK Conversion Tests ==========

#[test]
fn test_cose_to_jwk_es256() {
    let cose_key = create_es256_cose_key();

    let result = cose_to_jwk(&cose_key);

    assert!(
        result.is_ok(),
        "Should successfully convert ES256 COSE to JWK"
    );

    let jwk = result.unwrap();

    // Verify JWK structure
    match &jwk.params {
        JWKParams::EC(ec_params) => {
            assert_eq!(
                ec_params.curve,
                Some("P-256".to_string()),
                "Should use P-256 curve"
            );
            assert!(ec_params.x_coordinate.is_some(), "Should have x coordinate");
            assert!(ec_params.y_coordinate.is_some(), "Should have y coordinate");
            assert!(
                ec_params.ecc_private_key.is_none(),
                "Should not have private key"
            );
        }
        _ => panic!("Expected EC params for ES256 key"),
    }

    assert_eq!(
        jwk.public_key_use,
        Some("sig".to_string()),
        "Should be for signatures"
    );
    assert_eq!(
        jwk.algorithm,
        Some(ssi::jwk::Algorithm::ES256),
        "Should have ES256 algorithm"
    );

    info!("ES256 JWK: {:?}", jwk);
}

#[test]
fn test_cose_to_jwk_eddsa() {
    let cose_key = create_eddsa_cose_key();

    let result = cose_to_jwk(&cose_key);

    assert!(
        result.is_ok(),
        "Should successfully convert EdDSA COSE to JWK"
    );

    let jwk = result.unwrap();

    // Verify JWK structure
    match &jwk.params {
        JWKParams::OKP(okp_params) => {
            assert_eq!(okp_params.curve, "Ed25519", "Should use Ed25519 curve");
            assert!(
                !okp_params.public_key.0.is_empty(),
                "Should have public key"
            );
            assert!(
                okp_params.private_key.is_none(),
                "Should not have private key"
            );
        }
        _ => panic!("Expected OKP params for EdDSA key"),
    }

    assert_eq!(
        jwk.public_key_use,
        Some("sig".to_string()),
        "Should be for signatures"
    );
    assert_eq!(
        jwk.algorithm,
        Some(ssi::jwk::Algorithm::EdDSA),
        "Should have EdDSA algorithm"
    );

    info!("EdDSA JWK: {:?}", jwk);
}

// ========== Coordinate Extraction Tests ==========

#[test]
fn test_extract_ec_coordinates_valid() {
    let cose_key = create_es256_cose_key();

    let result = extract_ec_coordinates(&cose_key);

    assert!(result.is_ok(), "Should successfully extract EC coordinates");

    let (x, y) = result.unwrap();
    assert_eq!(x.len(), 32, "X coordinate should be 32 bytes for P-256");
    assert_eq!(y.len(), 32, "Y coordinate should be 32 bytes for P-256");
    assert!(!x.is_empty(), "X coordinate should not be empty");
    assert!(!y.is_empty(), "Y coordinate should not be empty");

    info!(
        "Extracted coordinates: x={} bytes, y={} bytes",
        x.len(),
        y.len()
    );
}

#[test]
fn test_extract_ec_coordinates_invalid_key_type() {
    let cose_key = create_eddsa_cose_key(); // Wrong key type

    let result = extract_ec_coordinates(&cose_key);

    assert!(result.is_err(), "Should fail with wrong key type");
    assert!(
        result.unwrap_err().to_string().contains("EC_EC2"),
        "Error should mention expected key type"
    );
}

#[test]
fn test_extract_eddsa_public_key_valid() {
    let cose_key = create_eddsa_cose_key();

    let result = extract_eddsa_public_key(&cose_key);

    assert!(
        result.is_ok(),
        "Should successfully extract EdDSA public key"
    );

    let public_key = result.unwrap();
    assert_eq!(
        public_key.len(),
        32,
        "Ed25519 public key should be 32 bytes"
    );
    assert!(!public_key.is_empty(), "Public key should not be empty");

    info!("Extracted EdDSA public key: {} bytes", public_key.len());
}

#[test]
fn test_extract_eddsa_public_key_invalid_key_type() {
    let cose_key = create_es256_cose_key(); // Wrong key type

    let result = extract_eddsa_public_key(&cose_key);

    assert!(result.is_err(), "Should fail with wrong key type");
    assert!(
        result.unwrap_err().to_string().contains("EC_OKP"),
        "Error should mention expected key type"
    );
}

// ========== DID Document Tests ==========

#[test]
fn test_create_did_document_structure() {
    let passkey = load_es256_passkey().0;
    let did = generate_did_key_from_passkey(&passkey).unwrap();
    let jwk = cose_to_jwk(passkey.get_public_key()).unwrap();

    let result = create_did_document(&did, &jwk);

    assert!(result.is_ok(), "Should successfully create DID document");

    let doc = result.unwrap();

    // Verify document structure
    assert_eq!(doc.id.as_str(), did, "Document ID should match DID");
    assert!(
        !doc.verification_method.is_empty(),
        "Should have verification methods"
    );

    info!("Created DID document for: {}", did);
}

#[test]
fn test_did_document_has_verification_methods() {
    let passkey = load_es256_passkey().0;
    let did = generate_did_key_from_passkey(&passkey).unwrap();
    let jwk = cose_to_jwk(passkey.get_public_key()).unwrap();
    let doc = create_did_document(&did, &jwk).unwrap();

    // Verify verification method
    assert_eq!(
        doc.verification_method.len(),
        1,
        "Should have exactly one verification method"
    );

    let vm = &doc.verification_method[0];
    assert!(
        vm.id.as_str().contains("#key-1"),
        "Verification method should have #key-1 fragment"
    );
    assert_eq!(vm.type_, "JsonWebKey2020", "Should use JsonWebKey2020 type");
    assert_eq!(vm.controller.as_str(), did, "Controller should be the DID");

    // Verify verification relationships
    assert!(
        !doc.verification_relationships.authentication.is_empty(),
        "Should have authentication relationship"
    );
    assert!(
        !doc.verification_relationships.assertion_method.is_empty(),
        "Should have assertion method relationship"
    );

    info!("Verification methods: {}", doc.verification_method.len());
}

#[test]
fn test_did_document_serialization() {
    let passkey = load_es256_passkey().0;
    let did = generate_did_key_from_passkey(&passkey).unwrap();
    let jwk = cose_to_jwk(passkey.get_public_key()).unwrap();
    let doc = create_did_document(&did, &jwk).unwrap();

    let result = did_document_to_json(&doc);

    assert!(result.is_ok(), "Should successfully serialize DID document");

    let json = result.unwrap();
    assert!(!json.is_empty(), "JSON should not be empty");

    // Verify it's valid JSON
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("Should be valid JSON");

    // Check essential fields
    assert!(parsed.get("id").is_some(), "Should have 'id' field");
    assert!(
        parsed.get("verificationMethod").is_some(),
        "Should have 'verificationMethod' field"
    );

    info!("DID Document JSON length: {} bytes", json.len());
    info!("DID Document:\n{}", json);
}

#[test]
fn test_did_key_deterministic_generation() {
    let passkey = load_es256_passkey().0;

    // Generate DID multiple times
    let did1 = generate_did_key_from_passkey(&passkey).unwrap();
    let did2 = generate_did_key_from_passkey(&passkey).unwrap();
    let did3 = generate_did_key_from_passkey(&passkey).unwrap();

    // All should be identical (deterministic)
    assert_eq!(did1, did2, "DID generation should be deterministic");
    assert_eq!(did2, did3, "DID generation should be deterministic");

    info!("Deterministic DID: {}", did1);

    // Verify JWK conversion is also deterministic
    let jwk1 = cose_to_jwk(passkey.get_public_key()).unwrap();
    let jwk2 = cose_to_jwk(passkey.get_public_key()).unwrap();

    let jwk1_json = serde_json::to_string(&jwk1).unwrap();
    let jwk2_json = serde_json::to_string(&jwk2).unwrap();

    assert_eq!(
        jwk1_json, jwk2_json,
        "JWK conversion should be deterministic"
    );
}

#[test]
fn test_different_passkeys_generate_different_dids() {
    let passkey1 = load_es256_passkey().0;
    let passkey2 = load_eddsa_passkey().0;

    let did1 = generate_did_key_from_passkey(&passkey1).unwrap();
    let did2 = generate_did_key_from_passkey(&passkey2).unwrap();

    assert_ne!(
        did1, did2,
        "Different passkeys should generate different DIDs"
    );

    info!("ES256 DID: {}", did1);
    info!("EdDSA DID: {}", did2);
}

#[test]
fn test_generate_did_peer_from_passkey() {
    use crate::modules::ssi::fixtures::load_es256_passkey;

    let passkey = load_es256_passkey().0;

    let result = generate_did_peer_from_passkey(&passkey);

    assert!(result.is_ok());
    let did = result.unwrap();
    assert!(did.starts_with("did:peer:0"));
    info!("Generated did:peer: {}", did);
}

#[test]
fn test_did_peer_deterministic() {
    use crate::modules::ssi::fixtures::load_es256_passkey;

    let passkey = load_es256_passkey().0;

    let did1 = generate_did_peer_from_passkey(&passkey).unwrap();
    let did2 = generate_did_peer_from_passkey(&passkey).unwrap();

    assert_eq!(did1, did2, "Same passkey should generate same did:peer");
}
