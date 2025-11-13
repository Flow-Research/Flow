use crate::bootstrap::init::setup_test_server;

use super::helpers::*;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use tower::ServiceExt;
use tracing::info;

#[tokio::test]
async fn test_health_endpoint_returns_200() {
    // Setup
    let server = setup_test_server().await;

    // Execute
    let (status, body) = get_request(&server.router, "/api/v1/health").await;

    // Assert
    assert_eq!(
        status,
        StatusCode::OK,
        "Health endpoint should return 200 OK"
    );

    // Verify response structure
    assert!(body.is_object(), "Response should be a JSON object");
    assert!(
        body.get("status").is_some(),
        "Response should have 'status' field"
    );
    assert_eq!(
        body["status"].as_str().unwrap(),
        "healthy",
        "Status should be 'healthy'"
    );

    // Verify timestamp is present
    assert!(
        body.get("timestamp").is_some(),
        "Response should have 'timestamp' field"
    );

    info!("Health check response: {:?}", body);
}

#[tokio::test]
async fn test_health_endpoint_response_format() {
    // Setup
    let server = setup_test_server().await;

    // Execute
    let (status, body) = get_request(&server.router, "/api/v1/health").await;

    // Assert response format
    assert_eq!(status, StatusCode::OK);

    // Verify exact fields
    let status_field = body["status"].as_str();
    assert!(status_field.is_some(), "Should have status field as string");

    let timestamp_field = body["timestamp"].as_str();
    assert!(
        timestamp_field.is_some(),
        "Should have timestamp field as string"
    );

    // Verify timestamp is valid ISO 8601 format
    let timestamp = timestamp_field.unwrap();
    assert!(
        timestamp.contains('T') && timestamp.contains('Z'),
        "Timestamp should be in ISO 8601 format with UTC timezone: {}",
        timestamp
    );

    info!("Timestamp format verified: {}", timestamp);
}

#[tokio::test]
async fn test_cors_headers_present() {
    // Setup
    let server = setup_test_server().await;

    // Execute - Make OPTIONS request (preflight)
    let response = server
        .router
        .oneshot(
            Request::builder()
                .uri("/api/v1/health")
                .method(Method::OPTIONS)
                .header("origin", "http://localhost:3000")
                .header("access-control-request-method", "GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Assert
    let headers = response.headers();

    assert_cors_headers(headers);

    // Additional specific CORS checks
    let allow_origin = headers.get("access-control-allow-origin");
    assert!(
        allow_origin.is_some(),
        "Should have access-control-allow-origin header"
    );

    info!("CORS headers verified: {:?}", headers);
}

#[tokio::test]
async fn test_health_endpoint_allows_get_method() {
    // Setup
    let server = setup_test_server().await;

    // Execute - Verify GET is allowed
    let response = server
        .router
        .oneshot(
            Request::builder()
                .uri("/api/v1/health")
                .method(Method::GET)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Assert
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "GET method should be allowed on health endpoint"
    );
}

#[tokio::test]
async fn test_health_endpoint_rejects_post_method() {
    // Setup
    let server = setup_test_server().await;

    // Execute - Try POST (should not be allowed)
    let response = server
        .router
        .oneshot(
            Request::builder()
                .uri("/api/v1/health")
                .method(Method::POST)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Assert
    assert_eq!(
        response.status(),
        StatusCode::METHOD_NOT_ALLOWED,
        "POST method should not be allowed on health endpoint"
    );
}

#[tokio::test]
async fn test_health_endpoint_multiple_calls() {
    // Setup
    let server = setup_test_server().await;

    // Execute - Call health endpoint multiple times
    let (status1, body1) = get_request(&server.router, "/api/v1/health").await;

    // Small delay to ensure different timestamps
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    let (status2, body2) = get_request(&server.router, "/api/v1/health").await;

    // Assert - Both calls should succeed
    assert_eq!(status1, StatusCode::OK);
    assert_eq!(status2, StatusCode::OK);

    // Both should have healthy status
    assert_eq!(body1["status"], "healthy");
    assert_eq!(body2["status"], "healthy");

    // Timestamps should be different (or at least exist)
    assert!(body1["timestamp"].is_string());
    assert!(body2["timestamp"].is_string());

    info!("Call 1 timestamp: {}", body1["timestamp"]);
    info!("Call 2 timestamp: {}", body2["timestamp"]);
}

#[tokio::test]
async fn test_health_endpoint_content_type() {
    // Setup
    let server = setup_test_server().await;

    // Execute
    let response = server
        .router
        .oneshot(
            Request::builder()
                .uri("/api/v1/health")
                .method(Method::GET)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Assert
    let content_type = response.headers().get("content-type");
    assert!(content_type.is_some(), "Should have content-type header");

    let content_type_value = content_type.unwrap().to_str().unwrap();
    assert!(
        content_type_value.contains("application/json"),
        "Content-Type should be application/json, got: {}",
        content_type_value
    );

    info!("Content-Type: {}", content_type_value);
}
