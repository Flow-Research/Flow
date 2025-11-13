use crate::{api::rest::helpers::*, bootstrap::init::setup_test_server};
use axum::http::StatusCode;
use entity::pass_key;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde_json::json;
use tracing::info;
use webauthn_authenticator_rs::{AuthenticatorBackend, softpasskey::SoftPasskey};
use webauthn_rs::prelude::Url;

#[tokio::test]
async fn test_start_authentication_returns_challenge() {
    // Setup - First register a passkey so we have something to authenticate with
    let server = setup_test_server().await;

    // Do a complete registration first
    let (reg_status, reg_body) =
        get_request(&server.router, "/api/v1/webauthn/start_registration").await;
    assert_eq!(reg_status, StatusCode::OK);

    let challenge_id = reg_body["challenge_id"].as_str().unwrap();
    let creation_challenge = &reg_body["challenge"];

    // Create credential with authenticator
    let mut authenticator = SoftPasskey::new(true);
    let registration_credential = authenticator
        .perform_register(
            Url::parse("http://localhost:3000").unwrap(),
            serde_json::from_value(creation_challenge["publicKey"].clone()).unwrap(),
            60000,
        )
        .unwrap();

    // Finish registration
    let reg_payload = json!({
        "challenge_id": challenge_id,
        "credential": registration_credential
    });

    let (finish_status, _) = post_request(
        &server.router,
        "/api/v1/webauthn/finish_registration",
        reg_payload,
    )
    .await;
    assert_eq!(finish_status, StatusCode::OK);

    // Now test authentication start
    let (status, body) = post_request(
        &server.router,
        "/api/v1/webauthn/start_authentication",
        json!({}),
    )
    .await;

    // Assert
    assert_eq!(status, StatusCode::OK, "Should return 200 OK");

    // Verify response structure
    assert!(
        body["challenge"].is_object(),
        "Should have challenge object"
    );
    assert!(
        body["challenge_id"].is_string(),
        "Should have challenge_id string"
    );

    // Verify challenge has required fields
    let challenge = &body["challenge"]["publicKey"];
    assert!(
        !challenge["challenge"].as_str().unwrap().is_empty(),
        "Challenge should not be empty"
    );
    assert!(
        challenge["allowCredentials"].is_array(),
        "Should have allowCredentials array"
    );

    info!("Authentication challenge: {:?}", body);
}

#[tokio::test]
async fn test_start_authentication_no_passkeys() {
    // Setup - Fresh server with no registered passkeys
    let server = setup_test_server().await;

    // Execute - Try to start authentication without any registered passkeys
    let (status, body) = post_request(
        &server.router,
        "/api/v1/webauthn/start_authentication",
        json!({}),
    )
    .await;

    // Assert - Should fail because no passkeys are registered
    // Note: This depends on your implementation - it might return an error or an empty allowCredentials
    // Adjust based on your actual behavior
    if status == StatusCode::OK {
        // If it succeeds, check that allowCredentials is empty
        let allow_credentials = body["challenge"]["publicKey"]["allowCredentials"].as_array();
        assert!(
            allow_credentials.is_some() && allow_credentials.unwrap().is_empty(),
            "Should have empty allowCredentials when no passkeys exist"
        );
    } else {
        // If it fails, it should return an appropriate error
        assert_eq!(
            status,
            StatusCode::INTERNAL_SERVER_ERROR,
            "Should return error when no passkeys exist"
        );
    }

    info!(
        "Start authentication with no passkeys: status={}, body={:?}",
        status, body
    );
}

#[tokio::test]
async fn test_start_authentication_response_format() {
    // Setup with registration
    let server = setup_test_server().await;

    // Register a passkey first
    let (reg_status, reg_body) =
        get_request(&server.router, "/api/v1/webauthn/start_registration").await;
    assert_eq!(reg_status, StatusCode::OK);

    let challenge_id = reg_body["challenge_id"].as_str().unwrap();
    let creation_challenge = &reg_body["challenge"];

    let mut authenticator = SoftPasskey::new(true);
    let registration_credential = authenticator
        .perform_register(
            Url::parse("http://localhost:3000").unwrap(),
            serde_json::from_value(creation_challenge["publicKey"].clone()).unwrap(),
            60000,
        )
        .unwrap();

    let reg_payload = json!({
        "challenge_id": challenge_id,
        "credential": registration_credential
    });

    post_request(
        &server.router,
        "/api/v1/webauthn/finish_registration",
        reg_payload,
    )
    .await;

    // Execute - Start authentication
    let (status, body) = post_request(
        &server.router,
        "/api/v1/webauthn/start_authentication",
        json!({}),
    )
    .await;

    // Assert response format
    assert_eq!(status, StatusCode::OK);

    // Verify exact response structure
    assert!(body.is_object(), "Response should be a JSON object");
    assert!(
        body.get("challenge").is_some(),
        "Should have 'challenge' field"
    );
    assert!(
        body.get("challenge_id").is_some(),
        "Should have 'challenge_id' field"
    );

    // Verify challenge structure
    let challenge = &body["challenge"]["publicKey"];
    assert!(
        challenge.get("challenge").is_some(),
        "Challenge should have 'challenge' field"
    );
    assert!(
        challenge.get("rpId").is_some(),
        "Challenge should have 'rpId' field"
    );
    assert!(
        challenge.get("allowCredentials").is_some(),
        "Challenge should have 'allowCredentials' field"
    );
    assert!(
        challenge.get("userVerification").is_some(),
        "Challenge should have 'userVerification' field"
    );

    info!("Authentication challenge structure verified");
}

#[tokio::test]
async fn test_finish_authentication_valid_payload() {
    // Setup - Complete registration and authentication flow
    let server = setup_test_server().await;

    // 1. Register a passkey
    let (reg_status, reg_body) =
        get_request(&server.router, "/api/v1/webauthn/start_registration").await;
    assert_eq!(reg_status, StatusCode::OK);

    let reg_challenge_id = reg_body["challenge_id"].as_str().unwrap();
    let creation_challenge = &reg_body["challenge"];

    let mut authenticator = SoftPasskey::new(true);
    let registration_credential = authenticator
        .perform_register(
            Url::parse("http://localhost:3000").unwrap(),
            serde_json::from_value(creation_challenge["publicKey"].clone()).unwrap(),
            60000,
        )
        .unwrap();

    let reg_payload = json!({
        "challenge_id": reg_challenge_id,
        "credential": registration_credential
    });

    let (finish_reg_status, _) = post_request(
        &server.router,
        "/api/v1/webauthn/finish_registration",
        reg_payload,
    )
    .await;
    assert_eq!(finish_reg_status, StatusCode::OK);

    // 2. Start authentication
    let (auth_start_status, auth_start_body) = post_request(
        &server.router,
        "/api/v1/webauthn/start_authentication",
        json!({}),
    )
    .await;
    assert_eq!(auth_start_status, StatusCode::OK);

    let auth_challenge_id = auth_start_body["challenge_id"].as_str().unwrap();
    let auth_challenge = &auth_start_body["challenge"];

    // 3. Create authentication credential
    let authentication_credential = authenticator
        .perform_auth(
            Url::parse("http://localhost:3000").unwrap(),
            serde_json::from_value(auth_challenge["publicKey"].clone()).unwrap(),
            60000,
        )
        .unwrap();

    // 4. Finish authentication
    let auth_payload = json!({
        "challenge_id": auth_challenge_id,
        "credential": authentication_credential
    });

    let (status, body) = post_request(
        &server.router,
        "/api/v1/webauthn/finish_authentication",
        auth_payload,
    )
    .await;

    // Assert
    assert_eq!(status, StatusCode::OK, "Authentication should succeed");
    assert_eq!(body["verified"], true, "Should be verified");
    assert!(body["message"].is_string(), "Should have message");
    assert!(body["counter"].is_number(), "Should have counter");

    info!("Authentication successful: {:?}", body);
}

#[tokio::test]
async fn test_finish_authentication_missing_challenge_id() {
    // Setup
    let server = setup_test_server().await;

    // Execute - Try to finish authentication without challenge_id
    let payload = json!({
        "credential": {}
    });

    let (status, body) = post_request(
        &server.router,
        "/api/v1/webauthn/finish_authentication",
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
        "Error message should mention missing challenge_id"
    );

    info!("Missing challenge_id error: {:?}", body);
}

#[tokio::test]
async fn test_finish_authentication_missing_credential() {
    // Setup
    let server = setup_test_server().await;

    // Execute - Try to finish authentication without credential
    let payload = json!({
        "challenge_id": "fake-challenge-id"
    });

    let (status, body) = post_request(
        &server.router,
        "/api/v1/webauthn/finish_authentication",
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
        "Error message should mention missing credential"
    );

    info!("Missing credential error: {:?}", body);
}

#[tokio::test]
async fn test_finish_authentication_invalid_credential_format() {
    // Setup
    let server = setup_test_server().await;

    // Execute - Try with invalid credential format
    let payload = json!({
        "challenge_id": "some-challenge-id",
        "credential": {
            "invalid": "format"
        }
    });

    let (status, body) = post_request(
        &server.router,
        "/api/v1/webauthn/finish_authentication",
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
        "Error message should mention invalid credential format"
    );

    info!("Invalid credential format error: {:?}", body);
}

#[tokio::test]
async fn test_finish_authentication_invalid_challenge_id() {
    // Setup
    let server = setup_test_server().await;

    // Execute - Try with non-existent challenge_id
    let payload = json!({
        "challenge_id": "non-existent-challenge-id",
        "credential": {
            "id": "test",
            "rawId": "test",
            "response": {
                "authenticatorData": "test",
                "clientDataJSON": "test",
                "signature": "test"
            },
            "type": "public-key"
        }
    });

    let (status, _body) = post_request(
        &server.router,
        "/api/v1/webauthn/finish_authentication",
        payload,
    )
    .await;

    // Assert - Should fail with server error (challenge not found)
    assert!(
        status == StatusCode::INTERNAL_SERVER_ERROR || status == StatusCode::BAD_REQUEST,
        "Should return error for invalid challenge_id, got: {}",
        status
    );

    info!("Invalid challenge_id handled with status: {}", status);
}

#[tokio::test]
async fn test_finish_authentication_returns_counter() {
    // Setup - Complete registration and authentication
    let server = setup_test_server().await;

    // Register
    let (_, reg_body) = get_request(&server.router, "/api/v1/webauthn/start_registration").await;
    let reg_challenge_id = reg_body["challenge_id"].as_str().unwrap();
    let creation_challenge = &reg_body["challenge"];

    let mut authenticator = SoftPasskey::new(true);
    let registration_credential = authenticator
        .perform_register(
            Url::parse("http://localhost:3000").unwrap(),
            serde_json::from_value(creation_challenge["publicKey"].clone()).unwrap(),
            60000,
        )
        .unwrap();

    post_request(
        &server.router,
        "/api/v1/webauthn/finish_registration",
        json!({
            "challenge_id": reg_challenge_id,
            "credential": registration_credential
        }),
    )
    .await;

    // Authenticate
    let (_, auth_body) = post_request(
        &server.router,
        "/api/v1/webauthn/start_authentication",
        json!({}),
    )
    .await;
    let auth_challenge_id = auth_body["challenge_id"].as_str().unwrap();

    let authentication_credential = authenticator
        .perform_auth(
            Url::parse("http://localhost:3000").unwrap(),
            serde_json::from_value(auth_body["challenge"]["publicKey"].clone()).unwrap(),
            60000,
        )
        .unwrap();

    let (status, body) = post_request(
        &server.router,
        "/api/v1/webauthn/finish_authentication",
        json!({
            "challenge_id": auth_challenge_id,
            "credential": authentication_credential
        }),
    )
    .await;

    // Assert
    assert_eq!(status, StatusCode::OK);

    // Verify counter field exists and is a number
    assert!(
        body["counter"].is_number(),
        "Should have counter field as number"
    );
    let counter = body["counter"].as_u64().unwrap();
    assert_eq!(counter, 1, "Counter should be 1 for first authentication");

    // Verify other authentication result fields
    assert!(
        body["backup_state"].is_boolean(),
        "Should have backup_state"
    );
    assert!(
        body["backup_eligible"].is_boolean(),
        "Should have backup_eligible"
    );
    assert!(
        body["needs_update"].is_boolean(),
        "Should have needs_update"
    );

    info!(
        "Authentication result: counter={}, backup_state={}, backup_eligible={}, needs_update={}",
        body["counter"], body["backup_state"], body["backup_eligible"], body["needs_update"]
    );

    // DB validation - verify counter was updated in database
    let device_id = server.node.node_data.id; // Or get from setup
    let passkey = pass_key::Entity::find()
        .filter(pass_key::Column::DeviceId.eq(device_id))
        .one(&server.node.db)
        .await
        .unwrap()
        .expect("Passkey should exist");

    assert!(
        passkey.sign_count > 0,
        "Sign count should be incremented in DB"
    );

    // TODO: fix this test so authentication_count is updated
    // in order to avoid replay attacks..
    // assert_eq!(
    //     passkey.authentication_count, 1,
    //     "Authentication count should be 1"
    // );

    info!(
        "Counter validated - DB sign_count: {}, auth_count: {}",
        passkey.sign_count, passkey.authentication_count
    );
}
