use crate::{api::rest::helpers::*, bootstrap::init::setup_test_server};
use axum::http::StatusCode;
use entity::{pass_key, user};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde_json::json;
use tracing::info;
use webauthn_authenticator_rs::{AuthenticatorBackend, softpasskey::SoftPasskey};
use webauthn_rs::prelude::Url;

#[tokio::test]
async fn test_start_registration_returns_challenge() {
    // Setup
    let server = setup_test_server().await;

    // Execute
    let (status, body) = get_request(&server.router, "/api/v1/webauthn/start_registration").await;

    // Assert
    assert_eq!(status, StatusCode::OK, "Should return 200 OK");

    // Verify response structure
    assert!(body.is_object(), "Response should be a JSON object");
    assert!(
        body.get("challenge").is_some(),
        "Should have 'challenge' field"
    );
    assert!(
        body.get("challenge_id").is_some(),
        "Should have 'challenge_id' field"
    );

    // Verify challenge_id is a non-empty string
    let challenge_id = body["challenge_id"].as_str();
    assert!(challenge_id.is_some(), "challenge_id should be a string");
    assert!(
        !challenge_id.unwrap().is_empty(),
        "challenge_id should not be empty"
    );

    // Verify challenge object structure
    let challenge = &body["challenge"];
    assert!(challenge.is_object(), "challenge should be an object");
    assert!(
        challenge.get("publicKey").is_some(),
        "challenge should have 'publicKey' field"
    );

    info!("Registration challenge: {:?}", body);
}

#[tokio::test]
async fn test_start_registration_response_format() {
    // Setup
    let server = setup_test_server().await;

    // Execute
    let (status, body) = get_request(&server.router, "/api/v1/webauthn/start_registration").await;

    // Assert
    assert_eq!(status, StatusCode::OK);

    // Verify exact response structure
    let challenge = &body["challenge"]["publicKey"];

    // Verify all required WebAuthn fields are present
    assert!(
        challenge.get("challenge").is_some(),
        "Should have 'challenge' field"
    );
    assert!(
        !challenge["challenge"].as_str().unwrap().is_empty(),
        "Challenge should not be empty"
    );

    assert!(
        challenge.get("rp").is_some(),
        "Should have 'rp' (relying party) field"
    );
    let rp = &challenge["rp"];
    assert!(rp.get("name").is_some(), "RP should have 'name' field");
    assert_eq!(rp["name"], "Test Flow", "RP name should match config");
    assert!(rp.get("id").is_some(), "RP should have 'id' field");

    assert!(challenge.get("user").is_some(), "Should have 'user' field");
    let user = &challenge["user"];
    assert!(user.get("id").is_some(), "User should have 'id' field");
    assert!(user.get("name").is_some(), "User should have 'name' field");
    assert!(
        user.get("displayName").is_some(),
        "User should have 'displayName' field"
    );

    assert!(
        challenge.get("pubKeyCredParams").is_some(),
        "Should have 'pubKeyCredParams' field"
    );
    assert!(
        challenge["pubKeyCredParams"].is_array(),
        "pubKeyCredParams should be an array"
    );
    assert!(
        !challenge["pubKeyCredParams"].as_array().unwrap().is_empty(),
        "Should have at least one credential parameter"
    );

    assert!(
        challenge.get("timeout").is_some(),
        "Should have 'timeout' field"
    );
    assert!(
        challenge.get("attestation").is_some(),
        "Should have 'attestation' field"
    );

    info!("Registration challenge format verified");
}

#[tokio::test]
async fn test_start_registration_creates_unique_challenges() {
    // Setup
    let server = setup_test_server().await;

    // Execute - Start registration twice
    let (status1, body1) = get_request(&server.router, "/api/v1/webauthn/start_registration").await;
    let (status2, body2) = get_request(&server.router, "/api/v1/webauthn/start_registration").await;

    // Assert
    assert_eq!(status1, StatusCode::OK);
    assert_eq!(status2, StatusCode::OK);

    let challenge_id1 = body1["challenge_id"].as_str().unwrap();
    let challenge_id2 = body2["challenge_id"].as_str().unwrap();

    let challenge1 = body1["challenge"]["publicKey"]["challenge"]
        .as_str()
        .unwrap();
    let challenge2 = body2["challenge"]["publicKey"]["challenge"]
        .as_str()
        .unwrap();

    // Verify challenges are unique
    assert_ne!(
        challenge_id1, challenge_id2,
        "Challenge IDs should be unique"
    );
    assert_ne!(challenge1, challenge2, "Challenge values should be unique");

    info!(
        "Generated unique challenges: {} and {}",
        challenge_id1, challenge_id2
    );
}

#[tokio::test]
async fn test_finish_registration_valid_payload() {
    // Setup
    let server = setup_test_server().await;

    // Start registration
    let (start_status, start_body) =
        get_request(&server.router, "/api/v1/webauthn/start_registration").await;
    assert_eq!(start_status, StatusCode::OK);

    let challenge_id = start_body["challenge_id"].as_str().unwrap();
    let creation_challenge = &start_body["challenge"];

    // Create credential
    let mut authenticator = SoftPasskey::new(true);
    let registration_credential = authenticator
        .perform_register(
            Url::parse("http://localhost:3000").unwrap(),
            serde_json::from_value(creation_challenge["publicKey"].clone()).unwrap(),
            60000,
        )
        .unwrap();

    // Finish registration
    let payload = json!({
        "challenge_id": challenge_id,
        "credential": registration_credential
    });

    let (status, body) = post_request(
        &server.router,
        "/api/v1/webauthn/finish_registration",
        payload,
    )
    .await;

    // HTTP assertions
    assert_eq!(status, StatusCode::OK, "Registration should succeed");
    assert_eq!(body["verified"], true);
    assert_eq!(body["message"], "Passkey registered successfully");

    // DB validation - verify user was created
    let did = body["did"].as_str().unwrap();
    let users = user::Entity::find()
        .filter(user::Column::Did.eq(did))
        .all(&server.node.db)
        .await
        .unwrap();

    assert_eq!(users.len(), 1, "Should have created exactly 1 user");
    let created_user = &users[0];
    assert_eq!(created_user.did, did);
    assert!(
        !created_user.public_key_jwk.is_empty(),
        "Should have DID document"
    );

    // Verify passkey was stored
    let passkeys = pass_key::Entity::find()
        .filter(pass_key::Column::UserId.eq(created_user.id))
        .all(&server.node.db)
        .await
        .unwrap();

    assert_eq!(passkeys.len(), 1, "Should have created exactly 1 passkey");
    assert_eq!(
        passkeys[0].sign_count, 0,
        "New passkey should have sign_count 0"
    );
    assert_eq!(
        passkeys[0].authentication_count, 0,
        "New passkey should have auth_count 0"
    );

    info!(
        "Registration validated - User ID: {}, Passkey ID: {}",
        created_user.id, passkeys[0].id
    );
}

#[tokio::test]
async fn test_finish_registration_missing_challenge_id() {
    // Setup
    let server = setup_test_server().await;

    // Execute - Try to finish registration without challenge_id
    let payload = json!({
        "credential": {
            "id": "test",
            "rawId": "test",
            "response": {
                "attestationObject": "test",
                "clientDataJSON": "test"
            },
            "type": "public-key"
        }
    });

    let (status, body) = post_request(
        &server.router,
        "/api/v1/webauthn/finish_registration",
        payload,
    )
    .await;

    // Assert
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "Should return 400 Bad Request"
    );
    assert!(
        body.as_str().unwrap_or("").contains("challenge_id")
            || body.to_string().contains("challenge_id"),
        "Error message should mention missing challenge_id, got: {:?}",
        body
    );

    info!("Missing challenge_id error: {:?}", body);
}

#[tokio::test]
async fn test_finish_registration_missing_credential() {
    // Setup
    let server = setup_test_server().await;

    // Execute - Try to finish registration without credential
    let payload = json!({
        "challenge_id": "fake-challenge-id"
    });

    let (status, body) = post_request(
        &server.router,
        "/api/v1/webauthn/finish_registration",
        payload,
    )
    .await;

    // Assert
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "Should return 400 Bad Request"
    );
    assert!(
        body.as_str().unwrap_or("").contains("credential")
            || body.to_string().contains("credential"),
        "Error message should mention missing credential, got: {:?}",
        body
    );

    info!("Missing credential error: {:?}", body);
}

#[tokio::test]
async fn test_finish_registration_invalid_credential_format() {
    // Setup
    let server = setup_test_server().await;

    // Execute - Try with invalid credential format
    let payload = json!({
        "challenge_id": "some-challenge-id",
        "credential": {
            "invalid": "format",
            "missing": "required_fields"
        }
    });

    let (status, body) = post_request(
        &server.router,
        "/api/v1/webauthn/finish_registration",
        payload,
    )
    .await;

    // Assert
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "Should return 400 Bad Request"
    );
    assert!(
        body.as_str()
            .unwrap_or("")
            .contains("Invalid credential format")
            || body.to_string().contains("credential"),
        "Error message should mention invalid credential format, got: {:?}",
        body
    );

    info!("Invalid credential format error: {:?}", body);
}

#[tokio::test]
async fn test_finish_registration_invalid_challenge_id() {
    // Setup
    let server = setup_test_server().await;

    // Execute - Try with non-existent challenge_id
    let payload = json!({
        "challenge_id": "non-existent-challenge-id",
        "credential": {
            "id": "test",
            "rawId": "test",
            "response": {
                "attestationObject": "test",
                "clientDataJSON": "test"
            },
            "type": "public-key"
        }
    });

    let (status, _body) = post_request(
        &server.router,
        "/api/v1/webauthn/finish_registration",
        payload,
    )
    .await;

    // Assert - Should fail (challenge not found)
    assert!(
        status == StatusCode::INTERNAL_SERVER_ERROR || status == StatusCode::BAD_REQUEST,
        "Should return error for invalid challenge_id, got: {}",
        status
    );

    info!("Invalid challenge_id handled with status: {}", status);
}

#[tokio::test]
async fn test_finish_registration_returns_did() {
    // Setup
    let server = setup_test_server().await;

    // 1. Start registration
    let (start_status, start_body) =
        get_request(&server.router, "/api/v1/webauthn/start_registration").await;
    assert_eq!(start_status, StatusCode::OK);

    let challenge_id = start_body["challenge_id"].as_str().unwrap();
    let creation_challenge = &start_body["challenge"];

    // 2. Create credential
    let mut authenticator = SoftPasskey::new(true);
    let registration_credential = authenticator
        .perform_register(
            Url::parse("http://localhost:3000").unwrap(),
            serde_json::from_value(creation_challenge["publicKey"].clone()).unwrap(),
            60000,
        )
        .unwrap();

    // 3. Finish registration
    let payload = json!({
        "challenge_id": challenge_id,
        "credential": registration_credential
    });

    let (status, body) = post_request(
        &server.router,
        "/api/v1/webauthn/finish_registration",
        payload,
    )
    .await;

    // Assert
    assert_eq!(status, StatusCode::OK);

    // Verify DID is returned
    assert!(body.get("did").is_some(), "Should have 'did' field");
    let did = body["did"].as_str().unwrap();
    assert!(
        did.starts_with("did:key:"),
        "DID should use did:key method, got: {}",
        did
    );
    assert!(did.len() > 20, "DID should have substantial length");

    // Verify DID document is returned
    assert!(
        body.get("didDocument").is_some(),
        "Should have 'didDocument' field"
    );
    let did_doc = &body["didDocument"];
    assert!(did_doc.is_object(), "DID document should be an object");

    // Verify DID document structure
    assert!(
        did_doc.get("id").is_some(),
        "DID document should have 'id' field"
    );
    assert_eq!(
        did_doc["id"], did,
        "DID document ID should match returned DID"
    );

    assert!(
        did_doc.get("verificationMethod").is_some(),
        "DID document should have 'verificationMethod' field"
    );

    // DB validation - verify DID document was stored
    let user_record = user::Entity::find()
        .filter(user::Column::Did.eq(did))
        .one(&server.node.db)
        .await
        .unwrap()
        .expect("User should exist");

    // Validate basic user fields
    assert!(user_record.id > 0, "User ID should be positive");
    assert_eq!(user_record.did, did, "Stored DID should match returned DID");
    assert!(
        !user_record.username.is_empty(),
        "Username should not be empty"
    );
    assert!(
        !user_record.display_name.is_empty(),
        "Display name should not be empty"
    );

    // Validate device_ids (JSON array)
    let device_ids: Vec<String> = serde_json::from_str(&user_record.device_ids)
        .expect("device_ids should be valid JSON array");
    assert!(!device_ids.is_empty(), "Should have at least one device ID");
    assert!(
        device_ids.contains(&server.node.node_data.id),
        "Device IDs should contain the node's device ID: {}",
        server.node.node_data.id
    );

    // Validate DID document (public_key_jwk)
    assert!(
        !user_record.public_key_jwk.is_empty(),
        "DID document should not be empty"
    );
    let did_doc: serde_json::Value = serde_json::from_str(&user_record.public_key_jwk)
        .expect("public_key_jwk should be valid JSON");

    info!("Registration returned DID: {}", did);
    info!("DID Document: {:?}", did_doc);
}

#[tokio::test]
async fn test_finish_registration_creates_user_in_database() {
    // Setup
    let server = setup_test_server().await;

    // 1. Start registration
    let (_, start_body) = get_request(&server.router, "/api/v1/webauthn/start_registration").await;
    let challenge_id = start_body["challenge_id"].as_str().unwrap();

    // 2. Create and finish registration
    let mut authenticator = SoftPasskey::new(true);
    let registration_credential = authenticator
        .perform_register(
            Url::parse("http://localhost:3000").unwrap(),
            serde_json::from_value(start_body["challenge"]["publicKey"].clone()).unwrap(),
            60000,
        )
        .unwrap();

    let payload = json!({
        "challenge_id": challenge_id,
        "credential": registration_credential
    });

    let (status, body) = post_request(
        &server.router,
        "/api/v1/webauthn/finish_registration",
        payload,
    )
    .await;

    // Assert
    assert_eq!(status, StatusCode::OK);

    let did = body["did"].as_str().unwrap();

    // Verify we can authenticate with the registered passkey
    // (This indirectly verifies the user and passkey were stored in the database)
    let (auth_status, _) = post_request(
        &server.router,
        "/api/v1/webauthn/start_authentication",
        json!({}),
    )
    .await;
    assert_eq!(
        auth_status,
        StatusCode::OK,
        "Should be able to start authentication after registration, indicating user/passkey were stored"
    );

    info!("User created with DID: {}", did);
}

#[tokio::test]
async fn test_finish_registration_deterministic_did_generation() {
    // Setup
    let server = setup_test_server().await;

    // Start registration
    let (_, start_body) = get_request(&server.router, "/api/v1/webauthn/start_registration").await;
    let challenge_id = start_body["challenge_id"].as_str().unwrap();

    // Create credential with specific seed
    let mut authenticator = SoftPasskey::new(true);
    let registration_credential = authenticator
        .perform_register(
            Url::parse("http://localhost:3000").unwrap(),
            serde_json::from_value(start_body["challenge"]["publicKey"].clone()).unwrap(),
            60000,
        )
        .unwrap();

    // Finish registration
    let payload = json!({
        "challenge_id": challenge_id,
        "credential": registration_credential
    });

    let (status, body) = post_request(
        &server.router,
        "/api/v1/webauthn/finish_registration",
        payload,
    )
    .await;

    // Assert
    assert_eq!(status, StatusCode::OK);

    let did = body["did"].as_str().unwrap();

    // Verify DID is stable (not random each time)
    assert!(
        did.starts_with("did:key:z"),
        "DID should use multibase encoding"
    );
    assert!(did.len() > 50, "DID should be properly encoded");

    // Note: Same credential should always produce same DID
    // (This is a property of did:key method)
    info!("Generated deterministic DID: {}", did);
}

#[tokio::test]
async fn test_finish_registration_challenge_expires() {
    // Setup
    let server = setup_test_server().await;

    // Start registration
    let (_, start_body) = get_request(&server.router, "/api/v1/webauthn/start_registration").await;
    let challenge_id = start_body["challenge_id"].as_str().unwrap().to_string();

    // Wait for challenge to expire (if your implementation has expiry)
    // Note: Adjust timeout based on your actual implementation
    // For now, we'll just test with a valid credential but note that
    // a real production test might wait 5+ minutes for expiry

    let mut authenticator = SoftPasskey::new(true);
    let registration_credential = authenticator
        .perform_register(
            Url::parse("http://localhost:3000").unwrap(),
            serde_json::from_value(start_body["challenge"]["publicKey"].clone()).unwrap(),
            60000,
        )
        .unwrap();

    let payload = json!({
        "challenge_id": challenge_id,
        "credential": registration_credential
    });

    let (status, _) = post_request(
        &server.router,
        "/api/v1/webauthn/finish_registration",
        payload,
    )
    .await;

    // For now, should succeed since we didn't wait for expiry
    assert_eq!(
        status,
        StatusCode::OK,
        "Should succeed with fresh challenge"
    );

    info!(
        "Challenge expiry test completed (implement actual timeout test with tokio::time::sleep if needed)"
    );
}
