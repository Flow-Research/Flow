//! Integration tests for distributed search REST API endpoints.
//!
//! Tests the `/api/v1/search/distributed` endpoint for searching across
//! local and network sources.

use crate::bootstrap::init::setup_test_server;

use super::helpers::*;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::json;
use tower::ServiceExt;
use tracing::info;

// ============================================================================
// POST /api/v1/search/distributed - Basic Functionality
// ============================================================================

#[tokio::test]
async fn test_distributed_search_returns_200() {
    // Setup
    let server = setup_test_server().await;

    // Execute
    let body = json!({
        "query": "test search query"
    });
    let (status, response) = post_request(&server.router, "/api/v1/search/distributed", body).await;

    // Assert
    assert_eq!(
        status,
        StatusCode::OK,
        "Distributed search should return 200 OK"
    );

    // Verify response structure
    assert!(response.is_object(), "Response should be a JSON object");
    assert_eq!(
        response["success"].as_bool(),
        Some(true),
        "Response should indicate success"
    );
    assert_eq!(
        response["query"].as_str(),
        Some("test search query"),
        "Response should echo the query"
    );

    info!("Distributed search response: {:?}", response);
}

#[tokio::test]
async fn test_distributed_search_response_structure() {
    // Setup
    let server = setup_test_server().await;

    // Execute
    let body = json!({
        "query": "test query",
        "scope": "all",
        "limit": 10
    });
    let (status, response) = post_request(&server.router, "/api/v1/search/distributed", body).await;

    // Assert
    assert_eq!(status, StatusCode::OK);

    // Verify all required fields are present
    assert!(
        response.get("success").is_some(),
        "Should have 'success' field"
    );
    assert!(response.get("query").is_some(), "Should have 'query' field");
    assert!(response.get("scope").is_some(), "Should have 'scope' field");
    assert!(
        response.get("results").is_some(),
        "Should have 'results' field"
    );
    assert!(
        response.get("local_count").is_some(),
        "Should have 'local_count' field"
    );
    assert!(
        response.get("network_count").is_some(),
        "Should have 'network_count' field"
    );
    assert!(
        response.get("peers_queried").is_some(),
        "Should have 'peers_queried' field"
    );
    assert!(
        response.get("peers_responded").is_some(),
        "Should have 'peers_responded' field"
    );
    assert!(
        response.get("total_found").is_some(),
        "Should have 'total_found' field"
    );
    assert!(
        response.get("elapsed_ms").is_some(),
        "Should have 'elapsed_ms' field"
    );

    // Results should be an array
    assert!(
        response["results"].is_array(),
        "Results should be an array"
    );

    info!("Response structure verified: {:?}", response);
}

// ============================================================================
// Scope Parameter Tests
// ============================================================================

#[tokio::test]
async fn test_distributed_search_local_scope() {
    // Setup
    let server = setup_test_server().await;

    // Execute with local scope
    let body = json!({
        "query": "test",
        "scope": "local"
    });
    let (status, response) = post_request(&server.router, "/api/v1/search/distributed", body).await;

    // Assert
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response["scope"].as_str(), Some("local"));
    // Local scope shouldn't query network peers
    assert_eq!(
        response["peers_queried"].as_u64(),
        Some(0),
        "Local scope should not query peers"
    );
    assert_eq!(
        response["peers_responded"].as_u64(),
        Some(0),
        "Local scope should not have peer responses"
    );
}

#[tokio::test]
async fn test_distributed_search_network_scope() {
    // Setup
    let server = setup_test_server().await;

    // Execute with network scope
    let body = json!({
        "query": "test",
        "scope": "network"
    });
    let (status, response) = post_request(&server.router, "/api/v1/search/distributed", body).await;

    // Assert
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response["scope"].as_str(), Some("network"));
    // Network scope queries peers (though may get 0 responses in test env)
    assert!(
        response["peers_queried"].as_u64().is_some(),
        "Network scope should track peers queried"
    );
}

#[tokio::test]
async fn test_distributed_search_all_scope() {
    // Setup
    let server = setup_test_server().await;

    // Execute with 'all' scope
    let body = json!({
        "query": "test",
        "scope": "all"
    });
    let (status, response) = post_request(&server.router, "/api/v1/search/distributed", body).await;

    // Assert
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response["scope"].as_str(), Some("all"));
}

#[tokio::test]
async fn test_distributed_search_default_scope() {
    // Setup
    let server = setup_test_server().await;

    // Execute without explicit scope (should default to 'all')
    let body = json!({
        "query": "test"
    });
    let (status, response) = post_request(&server.router, "/api/v1/search/distributed", body).await;

    // Assert
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        response["scope"].as_str(),
        Some("all"),
        "Default scope should be 'all'"
    );
}

#[tokio::test]
async fn test_distributed_search_invalid_scope() {
    // Setup
    let server = setup_test_server().await;

    // Execute with invalid scope
    let body = json!({
        "query": "test",
        "scope": "invalid_scope"
    });
    let (status, response) = post_request(&server.router, "/api/v1/search/distributed", body).await;

    // Assert - should return validation error
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "Invalid scope should return 400"
    );
    // API returns error as { success: false, error: { code: ..., message: ... } }
    let error_message = response["error"]["message"].as_str().unwrap_or("");
    assert!(
        error_message.contains("scope") || error_message.contains("Invalid"),
        "Error should mention invalid scope: {}",
        error_message
    );
}

// ============================================================================
// Limit Parameter Tests
// ============================================================================

#[tokio::test]
async fn test_distributed_search_custom_limit() {
    // Setup
    let server = setup_test_server().await;

    // Execute with custom limit
    let body = json!({
        "query": "test",
        "limit": 50
    });
    let (status, response) = post_request(&server.router, "/api/v1/search/distributed", body).await;

    // Assert
    assert_eq!(status, StatusCode::OK);
    // Results array length should be <= limit (may be less if fewer results)
    let results = response["results"].as_array().unwrap();
    assert!(
        results.len() <= 50,
        "Results should respect the limit"
    );
}

#[tokio::test]
async fn test_distributed_search_limit_zero_invalid() {
    // Setup
    let server = setup_test_server().await;

    // Execute with limit = 0
    let body = json!({
        "query": "test",
        "limit": 0
    });
    let (status, _) = post_request(&server.router, "/api/v1/search/distributed", body).await;

    // Assert - limit 0 should be invalid
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "Limit 0 should be rejected"
    );
}

#[tokio::test]
async fn test_distributed_search_limit_exceeds_max() {
    // Setup
    let server = setup_test_server().await;

    // Execute with limit > 100
    let body = json!({
        "query": "test",
        "limit": 150
    });
    let (status, _) = post_request(&server.router, "/api/v1/search/distributed", body).await;

    // Assert - limit > 100 should be invalid
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "Limit > 100 should be rejected"
    );
}

// ============================================================================
// Query Validation Tests
// ============================================================================

#[tokio::test]
async fn test_distributed_search_empty_query() {
    // Setup
    let server = setup_test_server().await;

    // Execute with empty query
    let body = json!({
        "query": ""
    });
    let (status, _) = post_request(&server.router, "/api/v1/search/distributed", body).await;

    // Assert
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "Empty query should be rejected"
    );
}

#[tokio::test]
async fn test_distributed_search_whitespace_only_query() {
    // Setup
    let server = setup_test_server().await;

    // Execute with whitespace-only query
    let body = json!({
        "query": "   "
    });
    let (status, _) = post_request(&server.router, "/api/v1/search/distributed", body).await;

    // Assert
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "Whitespace-only query should be rejected"
    );
}

#[tokio::test]
async fn test_distributed_search_query_too_long() {
    // Setup
    let server = setup_test_server().await;

    // Execute with query > 1000 characters
    let long_query = "a".repeat(1001);
    let body = json!({
        "query": long_query
    });
    let (status, _) = post_request(&server.router, "/api/v1/search/distributed", body).await;

    // Assert
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "Query > 1000 chars should be rejected"
    );
}

#[tokio::test]
async fn test_distributed_search_missing_query() {
    // Setup
    let server = setup_test_server().await;

    // Execute without query field
    let body = json!({
        "scope": "all"
    });
    let (status, _) = post_request(&server.router, "/api/v1/search/distributed", body).await;

    // Assert - missing query should fail
    assert!(
        status == StatusCode::BAD_REQUEST || status == StatusCode::UNPROCESSABLE_ENTITY,
        "Missing query should be rejected, got: {}",
        status
    );
}

// ============================================================================
// HTTP Method Tests
// ============================================================================

#[tokio::test]
async fn test_distributed_search_rejects_get_method() {
    // Setup
    let server = setup_test_server().await;

    // Execute - Try GET (should not be allowed)
    let response = server
        .router
        .oneshot(
            Request::builder()
                .uri("/api/v1/search/distributed")
                .method(Method::GET)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Assert
    assert_eq!(
        response.status(),
        StatusCode::METHOD_NOT_ALLOWED,
        "GET method should not be allowed on distributed search endpoint"
    );
}

#[tokio::test]
async fn test_distributed_search_accepts_post_method() {
    // Setup
    let server = setup_test_server().await;

    // Execute - POST with valid body
    let body = json!({ "query": "test" });
    let (status, _) = post_request(&server.router, "/api/v1/search/distributed", body).await;

    // Assert
    assert_eq!(
        status,
        StatusCode::OK,
        "POST method should be allowed on distributed search endpoint"
    );
}

// ============================================================================
// Health Check Endpoint Tests
// ============================================================================

#[tokio::test]
async fn test_distributed_search_health_endpoint() {
    // Setup
    let server = setup_test_server().await;

    // Execute
    let (status, response) =
        get_request(&server.router, "/api/v1/search/distributed/health").await;

    // Assert
    assert_eq!(
        status,
        StatusCode::OK,
        "Health endpoint should return 200 OK"
    );

    // Verify response has status field
    assert!(
        response.get("status").is_some(),
        "Health response should have 'status' field"
    );

    let status_str = response["status"].as_str().unwrap_or("");
    assert!(
        status_str == "healthy" || status_str == "degraded",
        "Status should be 'healthy' or 'degraded', got: {}",
        status_str
    );

    info!("Distributed search health: {:?}", response);
}

#[tokio::test]
async fn test_distributed_search_health_cache_stats() {
    // Setup
    let server = setup_test_server().await;

    // Execute
    let (status, response) =
        get_request(&server.router, "/api/v1/search/distributed/health").await;

    // Assert
    assert_eq!(status, StatusCode::OK);

    // If healthy, should have cache stats
    if response["status"].as_str() == Some("healthy") {
        assert!(
            response.get("cache").is_some(),
            "Healthy response should include cache stats"
        );
    }
}

// ============================================================================
// Content-Type and Response Format Tests
// ============================================================================

#[tokio::test]
async fn test_distributed_search_content_type() {
    // Setup
    let server = setup_test_server().await;

    // Execute
    let response = server
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/search/distributed")
                .method(Method::POST)
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&json!({"query": "test"})).unwrap()))
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

// ============================================================================
// Performance Tests
// ============================================================================

#[tokio::test]
async fn test_distributed_search_reports_elapsed_time() {
    // Setup
    let server = setup_test_server().await;

    // Execute
    let body = json!({
        "query": "test query"
    });
    let (status, response) = post_request(&server.router, "/api/v1/search/distributed", body).await;

    // Assert
    assert_eq!(status, StatusCode::OK);

    // Should have elapsed_ms field
    let elapsed = response["elapsed_ms"].as_u64();
    assert!(elapsed.is_some(), "Should have elapsed_ms field");

    info!("Search completed in {}ms", elapsed.unwrap());
}

#[tokio::test]
async fn test_distributed_search_multiple_requests() {
    // Setup
    let server = setup_test_server().await;

    // Execute multiple requests
    for i in 0..3 {
        let body = json!({
            "query": format!("test query {}", i)
        });
        let (status, response) =
            post_request(&server.router, "/api/v1/search/distributed", body).await;

        assert_eq!(
            status,
            StatusCode::OK,
            "Request {} should succeed",
            i
        );
        assert_eq!(response["success"].as_bool(), Some(true));
    }
}
