use super::helpers::*;
use crate::bootstrap::init::setup_test_server;
use axum::http::StatusCode;
use serial_test::serial;
use tracing::info;

#[tokio::test]
#[serial]
async fn test_network_status_endpoint_returns_200() {
    let server = setup_test_server().await;

    let (status, body) = get_request(&server.router, "/api/v1/network/status").await;

    assert_eq!(status, StatusCode::OK, "Network status should return 200");
    assert!(body.is_object(), "Response should be a JSON object");

    assert!(body.get("running").is_some(), "Should have 'running' field");
    assert!(body.get("peer_id").is_some(), "Should have 'peer_id' field");
    assert!(
        body.get("connected_peers").is_some(),
        "Should have 'connected_peers' field"
    );

    info!("Network status response: {:?}", body);
}

#[tokio::test]
#[serial]
async fn test_network_status_response_structure() {
    let server = setup_test_server().await;

    let (status, body) = get_request(&server.router, "/api/v1/network/status").await;

    assert_eq!(status, StatusCode::OK);

    assert!(body["running"].is_boolean(), "running should be boolean");
    assert!(body["peer_id"].is_string(), "peer_id should be string");
    assert!(
        body["connected_peers"].is_number(),
        "connected_peers should be number"
    );
    assert!(
        body["known_peers"].is_number(),
        "known_peers should be number"
    );
    assert!(
        body["subscribed_topics"].is_array(),
        "subscribed_topics should be array"
    );

    let uptime = &body["uptime_secs"];
    assert!(
        uptime.is_null() || uptime.is_number(),
        "uptime_secs should be number or null"
    );
}

#[tokio::test]
#[serial]
async fn test_network_peers_when_not_running() {
    let server = setup_test_server().await;

    let (_, status_body) = get_request(&server.router, "/api/v1/network/status").await;

    if !status_body["running"].as_bool().unwrap_or(false) {
        let (status, _) = get_request(&server.router, "/api/v1/network/peers").await;
        assert_eq!(
            status,
            StatusCode::SERVICE_UNAVAILABLE,
            "Should return 503 when network not running"
        );
    }
}

#[tokio::test]
#[serial]
async fn test_network_peers_returns_list() {
    let server = setup_test_server().await;

    let (_, status_body) = get_request(&server.router, "/api/v1/network/status").await;

    if status_body["running"].as_bool().unwrap_or(false) {
        let (status, body) = get_request(&server.router, "/api/v1/network/peers").await;

        assert_eq!(status, StatusCode::OK);
        assert!(body.get("count").is_some(), "Should have 'count' field");
        assert!(body.get("peers").is_some(), "Should have 'peers' field");
        assert!(body["peers"].is_array(), "Peers should be an array");

        let count = body["count"].as_u64().unwrap() as usize;
        let peers = body["peers"].as_array().unwrap();
        assert_eq!(count, peers.len(), "Count should match peers array length");
    }
}

#[tokio::test]
#[serial]
async fn test_network_start_when_already_running() {
    let server = setup_test_server().await;

    let (_, status_body) = get_request(&server.router, "/api/v1/network/status").await;

    if status_body["running"].as_bool().unwrap_or(false) {
        let (status, _) = post_request(
            &server.router,
            "/api/v1/network/start",
            serde_json::json!({}),
        )
        .await;

        assert_eq!(
            status,
            StatusCode::CONFLICT,
            "Should return 409 when already running"
        );
    }
}

#[tokio::test]
#[serial]
async fn test_network_stop_when_not_running() {
    let server = setup_test_server().await;

    let (_, status_body) = get_request(&server.router, "/api/v1/network/status").await;

    if !status_body["running"].as_bool().unwrap_or(false) {
        let (status, _) = post_request(
            &server.router,
            "/api/v1/network/stop",
            serde_json::json!({}),
        )
        .await;

        assert_eq!(
            status,
            StatusCode::CONFLICT,
            "Should return 409 when not running"
        );
    }
}

#[tokio::test]
#[serial]
async fn test_dial_peer_invalid_address() {
    let server = setup_test_server().await;

    let (_, status_body) = get_request(&server.router, "/api/v1/network/status").await;

    if status_body["running"].as_bool().unwrap_or(false) {
        let (status, _) = post_request(
            &server.router,
            "/api/v1/network/dial",
            serde_json::json!({ "address": "invalid-address" }),
        )
        .await;

        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "Should return 400 for invalid address"
        );
    }
}

#[tokio::test]
#[serial]
async fn test_dial_peer_network_not_running() {
    let server = setup_test_server().await;

    let (_, status_body) = get_request(&server.router, "/api/v1/network/status").await;

    if !status_body["running"].as_bool().unwrap_or(false) {
        let (status, _) = post_request(
            &server.router,
            "/api/v1/network/dial",
            serde_json::json!({ "address": "/ip4/127.0.0.1/tcp/9000" }),
        )
        .await;

        assert_eq!(
            status,
            StatusCode::SERVICE_UNAVAILABLE,
            "Should return 503 when not running"
        );
    }
}
