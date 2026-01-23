//! Integration tests for content discovery API endpoints.
//!
//! Tests cover:
//! - GET /api/v1/content/{cid} - Get content by CID
//! - GET /api/v1/content/published - List published content
//! - GET /api/v1/content/discovered - List discovered content
//! - POST /api/v1/content/{cid}/index - Trigger indexing
//! - GET /api/v1/content/{cid}/providers - List providers
//! - POST /api/v1/search - Federated search

use crate::bootstrap::init::setup_test_server;

use super::helpers::*;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::json;
use tower::ServiceExt;
use tracing::info;

// ============================================================================
// GET /api/v1/content/published tests
// ============================================================================

#[tokio::test]
async fn test_list_published_returns_empty_list() {
    // Setup
    let server = setup_test_server().await;

    // Execute
    let (status, body) = get_request(&server.router, "/api/v1/content/published").await;

    // Assert
    assert_eq!(
        status,
        StatusCode::OK,
        "List published endpoint should return 200 OK"
    );

    // Verify response structure
    assert!(body.is_object(), "Response should be a JSON object");
    assert_eq!(
        body["success"].as_bool(),
        Some(true),
        "Response should have success=true"
    );
    assert!(
        body["items"].is_array(),
        "Response should have items array"
    );
    assert!(
        body["total"].is_number(),
        "Response should have total count"
    );

    info!("List published response: {:?}", body);
}

#[tokio::test]
async fn test_list_published_with_pagination() {
    // Setup
    let server = setup_test_server().await;

    // Execute with pagination params
    let (status, body) = get_request(
        &server.router,
        "/api/v1/content/published?limit=10&offset=0",
    )
    .await;

    // Assert
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["success"].as_bool(), Some(true));

    info!("Paginated response: {:?}", body);
}

// ============================================================================
// GET /api/v1/content/discovered tests
// ============================================================================

#[tokio::test]
async fn test_list_discovered_returns_empty_list() {
    // Setup
    let server = setup_test_server().await;

    // Execute
    let (status, body) = get_request(&server.router, "/api/v1/content/discovered").await;

    // Assert
    assert_eq!(
        status,
        StatusCode::OK,
        "List discovered endpoint should return 200 OK"
    );

    // Verify response structure
    assert!(body.is_object(), "Response should be a JSON object");
    assert_eq!(body["success"].as_bool(), Some(true));
    assert!(body["items"].is_array());
    assert!(body["total"].is_number());

    info!("List discovered response: {:?}", body);
}

#[tokio::test]
async fn test_list_discovered_with_pagination() {
    // Setup
    let server = setup_test_server().await;

    // Execute with pagination params
    let (status, body) = get_request(
        &server.router,
        "/api/v1/content/discovered?limit=5&offset=0",
    )
    .await;

    // Assert
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["success"].as_bool(), Some(true));

    info!("Paginated discovered response: {:?}", body);
}

// ============================================================================
// GET /api/v1/content/{cid} tests
// ============================================================================

#[tokio::test]
async fn test_get_content_not_found() {
    // Setup
    let server = setup_test_server().await;

    // Execute - Request non-existent CID
    let (status, body) = get_request(
        &server.router,
        "/api/v1/content/bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi",
    )
    .await;

    // Assert
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "Non-existent content should return 404"
    );

    info!("Not found response: {:?}", body);
}

// ============================================================================
// GET /api/v1/content/{cid}/providers tests
// ============================================================================

#[tokio::test]
async fn test_list_providers_empty_for_unknown_cid() {
    // Setup
    let server = setup_test_server().await;

    // Execute - Request providers for non-existent CID
    let (status, body) = get_request(
        &server.router,
        "/api/v1/content/bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi/providers",
    )
    .await;

    // Assert
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["success"].as_bool(), Some(true));
    assert!(body["providers"].is_array());
    assert_eq!(
        body["providers"].as_array().unwrap().len(),
        0,
        "Unknown CID should have no providers"
    );

    info!("Providers response: {:?}", body);
}

// ============================================================================
// POST /api/v1/content/{cid}/index tests
// ============================================================================

#[tokio::test]
async fn test_trigger_index_not_found() {
    // Setup
    let server = setup_test_server().await;

    // Execute - Try to index non-existent remote content
    let (status, body) = post_request(
        &server.router,
        "/api/v1/content/bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi/index",
        json!({}),
    )
    .await;

    // Assert - Should return 404 since content doesn't exist in remote_content table
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "Index request for non-existent content should return 404"
    );

    info!("Index not found response: {:?}", body);
}

// ============================================================================
// POST /api/v1/search tests
// ============================================================================

#[tokio::test]
async fn test_search_returns_empty_results() {
    // Setup
    let server = setup_test_server().await;

    // Execute
    let (status, body) = post_request(
        &server.router,
        "/api/v1/search",
        json!({
            "query": "test search query",
            "scope": "all",
            "limit": 10
        }),
    )
    .await;

    // Assert
    assert_eq!(status, StatusCode::OK, "Search should return 200 OK");

    // Verify response structure
    assert_eq!(body["success"].as_bool(), Some(true));
    assert_eq!(body["query"].as_str(), Some("test search query"));
    assert!(body["results"].is_array());
    assert!(body["local_count"].is_number());
    assert!(body["remote_count"].is_number());

    info!("Search response: {:?}", body);
}

#[tokio::test]
async fn test_search_with_local_scope() {
    // Setup
    let server = setup_test_server().await;

    // Execute
    let (status, body) = post_request(
        &server.router,
        "/api/v1/search",
        json!({
            "query": "local only search",
            "scope": "local"
        }),
    )
    .await;

    // Assert
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["success"].as_bool(), Some(true));

    info!("Local search response: {:?}", body);
}

#[tokio::test]
async fn test_search_with_remote_scope() {
    // Setup
    let server = setup_test_server().await;

    // Execute
    let (status, body) = post_request(
        &server.router,
        "/api/v1/search",
        json!({
            "query": "remote only search",
            "scope": "remote"
        }),
    )
    .await;

    // Assert
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["success"].as_bool(), Some(true));

    info!("Remote search response: {:?}", body);
}

#[tokio::test]
async fn test_search_with_default_limit() {
    // Setup
    let server = setup_test_server().await;

    // Execute - No limit specified, should use default
    let (status, body) = post_request(
        &server.router,
        "/api/v1/search",
        json!({
            "query": "no limit search"
        }),
    )
    .await;

    // Assert
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["success"].as_bool(), Some(true));

    info!("Default limit search response: {:?}", body);
}

// ============================================================================
// HTTP Method tests
// ============================================================================

#[tokio::test]
async fn test_content_endpoints_method_validation() {
    // Setup
    let server = setup_test_server().await;

    // Test POST on GET-only endpoint
    let response = server
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/content/published")
                .method(Method::POST)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::METHOD_NOT_ALLOWED,
        "POST should not be allowed on /content/published"
    );

    // Test GET on POST-only endpoint
    let response = server
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/search")
                .method(Method::GET)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::METHOD_NOT_ALLOWED,
        "GET should not be allowed on /search"
    );
}

// ============================================================================
// Content-Type tests
// ============================================================================

#[tokio::test]
async fn test_content_endpoints_return_json() {
    // Setup
    let server = setup_test_server().await;

    // Execute
    let response = server
        .router
        .oneshot(
            Request::builder()
                .uri("/api/v1/content/published")
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
}
