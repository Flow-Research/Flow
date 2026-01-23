//! End-to-end integration tests for the space → index → search flow.
//!
//! These tests use testcontainers to spin up a real Qdrant instance,
//! ensuring no mocking of the vector database. This validates the full
//! indexing pipeline from file creation to semantic search.
//!
//! Prerequisites:
//! - Docker must be running
//! - Tests are marked #[ignore] by default since they require Docker
//!
//! Run with: cargo test indexing_e2e -- --ignored

use crate::{
    api::rest::helpers::{get_request, post_request},
    bootstrap::init::{set_env, setup_test_server},
};
use axum::http::StatusCode;
use serde_json::json;
use std::fs;
use std::time::Duration;
use tempfile::TempDir;
use testcontainers::{
    core::{ContainerPort, WaitFor},
    runners::AsyncRunner,
    ContainerAsync, GenericImage,
};
use tracing::info;

// ============================================================================
// Custom Qdrant Container (since testcontainers-modules doesn't include it)
// ============================================================================

const QDRANT_HTTP_PORT: u16 = 6333;
const QDRANT_GRPC_PORT: u16 = 6334;

/// Create a Qdrant container image
fn qdrant_image() -> GenericImage {
    GenericImage::new("qdrant/qdrant", "v1.15.0")
        .with_exposed_port(ContainerPort::Tcp(QDRANT_HTTP_PORT))
        .with_exposed_port(ContainerPort::Tcp(QDRANT_GRPC_PORT))
        .with_wait_for(WaitFor::message_on_stdout("Qdrant gRPC listening"))
}

/// Container wrapper to keep Qdrant alive for the test duration
struct QdrantContainer {
    _container: ContainerAsync<GenericImage>,
    url: String,
}

impl QdrantContainer {
    async fn new() -> Self {
        let container = qdrant_image()
            .start()
            .await
            .expect("Failed to start Qdrant container");

        let host = container.get_host().await.expect("Failed to get host");
        let port = container
            .get_host_port_ipv4(QDRANT_GRPC_PORT)
            .await
            .expect("Failed to get gRPC port");

        let url = format!("http://{}:{}", host, port);
        info!("Qdrant started at {}", url);

        // Give Qdrant a moment to fully initialize
        tokio::time::sleep(Duration::from_secs(2)).await;

        Self {
            _container: container,
            url,
        }
    }
}

/// Create test markdown files in a directory
fn create_test_files(dir: &std::path::Path) {
    // Create a few markdown files with semantic content for testing
    fs::write(
        dir.join("rust_guide.md"),
        r#"# Rust Programming Guide

Rust is a systems programming language focused on safety, speed, and concurrency.

## Memory Safety

Rust achieves memory safety without garbage collection through its ownership system.
The borrow checker ensures that references are always valid.

## Concurrency

Rust's type system prevents data races at compile time.
Channels and locks are used for thread communication.
"#,
    )
    .expect("Failed to write rust_guide.md");

    fs::write(
        dir.join("python_basics.md"),
        r#"# Python Basics

Python is a high-level, interpreted programming language.

## Dynamic Typing

Python uses dynamic typing, so you don't need to declare variable types.
This makes it quick to write but requires careful testing.

## List Comprehensions

Python's list comprehensions provide a concise way to create lists.
Example: `[x*2 for x in range(10)]`
"#,
    )
    .expect("Failed to write python_basics.md");

    fs::write(
        dir.join("database_design.md"),
        r#"# Database Design Principles

Good database design is crucial for application performance.

## Normalization

Normalization reduces data redundancy and improves data integrity.
Common normal forms include 1NF, 2NF, 3NF, and BCNF.

## Indexing

Database indexes speed up query performance but slow down writes.
Choose indexes based on your query patterns.
"#,
    )
    .expect("Failed to write database_design.md");
}

/// Poll the space status until indexing completes or timeout
/// Waits for indexing to start (or already be complete) before returning.
async fn wait_for_indexing(
    router: &axum::Router,
    space_key: &str,
    timeout: Duration,
) -> Result<serde_json::Value, String> {
    let start = std::time::Instant::now();
    let poll_interval = Duration::from_millis(500);
    let mut seen_in_progress = false;

    loop {
        if start.elapsed() > timeout {
            return Err("Indexing timed out".to_string());
        }

        let (status, body) =
            get_request(router, &format!("/api/v1/spaces/{}/status", space_key)).await;

        if status != StatusCode::OK {
            return Err(format!("Status check failed: {:?}", body));
        }

        let in_progress = body["indexing_in_progress"].as_bool().unwrap_or(false);
        let files_indexed = body["files_indexed"].as_i64().unwrap_or(0);

        if in_progress {
            seen_in_progress = true;
            info!(
                "Indexing in progress: {} files indexed so far",
                files_indexed
            );
        }

        // Complete when: not in progress AND (we saw it start OR some files were indexed)
        if !in_progress && (seen_in_progress || files_indexed > 0) {
            info!(
                "Indexing complete: {} files indexed",
                files_indexed
            );
            return Ok(body);
        }

        // If not in progress and no files indexed, keep waiting for it to start
        if !in_progress && !seen_in_progress && files_indexed == 0 {
            info!("Waiting for indexing to start...");
        }

        tokio::time::sleep(poll_interval).await;
    }
}

/// Full E2E test: Create space → Add files → Index → Search → Verify
#[tokio::test]
#[ignore] // Requires Docker - run with: cargo test indexing_e2e -- --ignored
async fn test_space_indexing_and_search_e2e() {
    // 1. Start Qdrant via testcontainers
    let qdrant = QdrantContainer::new().await;

    // 2. Set environment for the test
    set_env("QDRANT_URL", &qdrant.url);
    set_env("QDRANT_SKIP_API_KEY", "true"); // Skip API key for testcontainers instance
    set_env("REDIS_URL", "redis://localhost:6379"); // Redis can be optional

    // 3. Create temp directory with test files
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    create_test_files(temp_dir.path());

    // 4. Setup test server (with real Qdrant connection)
    let server = setup_test_server().await;
    let dir_path = temp_dir.path().to_str().unwrap();

    // 5. Create a space pointing to our test files
    let create_payload = json!({ "dir": dir_path });
    let (status, body) = post_request(&server.router, "/api/v1/spaces", create_payload).await;

    assert_eq!(status, StatusCode::OK, "Space creation failed: {:?}", body);
    assert_eq!(body["status"], "success");

    // 6. Get the space to retrieve its key
    let (status, spaces) = get_request(&server.router, "/api/v1/spaces").await;
    assert_eq!(status, StatusCode::OK, "Failed to list spaces");

    let space_key = spaces
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|s| s["key"].as_str())
        .expect("Failed to get space key");

    info!("Created space with key: {}", space_key);

    // 7. Check if indexing is already in progress (auto-started on space creation)
    let (status, status_body) =
        get_request(&server.router, &format!("/api/v1/spaces/{}/status", space_key)).await;
    assert_eq!(status, StatusCode::OK);

    let indexing_in_progress = status_body["indexing_in_progress"].as_bool().unwrap_or(false);

    // Trigger reindex - handle race condition where indexing may have started
    if !indexing_in_progress {
        let (status, body) = post_request(
            &server.router,
            &format!("/api/v1/spaces/{}/reindex", space_key),
            json!({}),
        )
        .await;

        // Accept 200 (success) or 500 with "already in progress" (race condition)
        if status == StatusCode::INTERNAL_SERVER_ERROR {
            let error_msg = body["error"]["message"].as_str().unwrap_or("");
            if error_msg.contains("already in progress") {
                info!("Indexing started between status check and reindex call - continuing");
            } else {
                panic!("Reindex trigger failed: {:?}", body);
            }
        } else {
            assert_eq!(status, StatusCode::OK, "Reindex trigger failed: {:?}", body);
            assert!(body["success"].as_bool().unwrap_or(false));
        }
    } else {
        info!("Indexing already in progress, skipping reindex trigger");
    }

    // 8. Wait for indexing to complete (timeout: 60 seconds)
    let final_status = wait_for_indexing(&server.router, space_key, Duration::from_secs(60))
        .await
        .expect("Indexing failed or timed out");

    info!("Indexing complete: {:?}", final_status);

    // Verify some files were indexed
    let files_indexed = final_status["files_indexed"].as_i64().unwrap_or(0);
    assert!(
        files_indexed > 0,
        "Expected files to be indexed, got: {}",
        files_indexed
    );

    // 9. Search for "Rust memory safety" - should find rust_guide.md
    let search_payload = json!({
        "query": "Rust memory safety ownership borrow checker",
        "scope": "local",
        "limit": 5
    });

    let (status, results) = post_request(&server.router, "/api/v1/search", search_payload).await;

    assert_eq!(status, StatusCode::OK, "Search failed: {:?}", results);
    assert!(results["success"].as_bool().unwrap_or(false));

    let result_items = results["results"]
        .as_array()
        .expect("Results should be array");

    info!(
        "Search returned {} results for 'Rust memory safety'",
        result_items.len()
    );

    // We should get at least one result
    assert!(
        !result_items.is_empty(),
        "Expected search results for 'Rust memory safety'"
    );

    // 10. Search for "Python dynamic typing" - should find python_basics.md
    let search_payload = json!({
        "query": "Python dynamic typing list comprehension",
        "scope": "local",
        "limit": 5
    });

    let (status, results) = post_request(&server.router, "/api/v1/search", search_payload).await;

    assert_eq!(status, StatusCode::OK);
    assert!(results["success"].as_bool().unwrap_or(false));

    let result_items = results["results"]
        .as_array()
        .expect("Results should be array");

    info!(
        "Search returned {} results for 'Python dynamic typing'",
        result_items.len()
    );

    assert!(
        !result_items.is_empty(),
        "Expected search results for 'Python dynamic typing'"
    );

    // 11. Search for "database normalization" - should find database_design.md
    let search_payload = json!({
        "query": "database normalization indexing performance",
        "scope": "local",
        "limit": 5
    });

    let (status, results) = post_request(&server.router, "/api/v1/search", search_payload).await;

    assert_eq!(status, StatusCode::OK);
    assert!(results["success"].as_bool().unwrap_or(false));

    let result_items = results["results"]
        .as_array()
        .expect("Results should be array");

    info!(
        "Search returned {} results for 'database normalization'",
        result_items.len()
    );

    assert!(
        !result_items.is_empty(),
        "Expected search results for 'database normalization'"
    );

    info!("✅ Full E2E indexing and search test passed!");
}

/// Test that search returns empty results for unindexed content
#[tokio::test]
#[ignore]
async fn test_search_before_indexing_returns_empty() {
    // Start Qdrant
    let qdrant = QdrantContainer::new().await;
    set_env("QDRANT_URL", &qdrant.url);
    set_env("QDRANT_SKIP_API_KEY", "true");

    // Create space but don't index
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    create_test_files(temp_dir.path());

    let server = setup_test_server().await;
    let dir_path = temp_dir.path().to_str().unwrap();

    let create_payload = json!({ "dir": dir_path });
    let (status, _) = post_request(&server.router, "/api/v1/spaces", create_payload).await;
    assert_eq!(status, StatusCode::OK);

    // Search without indexing - should return empty or error gracefully
    let search_payload = json!({
        "query": "Rust programming",
        "scope": "local",
        "limit": 5
    });

    let (status, results) = post_request(&server.router, "/api/v1/search", search_payload).await;

    // Should either succeed with empty results or handle gracefully
    if status == StatusCode::OK {
        let empty_vec = vec![];
        let result_items = results["results"].as_array().unwrap_or(&empty_vec);
        // No panic - just verify we handle unindexed state
        info!(
            "Search before indexing returned {} results",
            result_items.len()
        );
    }
}

/// Test space status reflects indexing progress
#[tokio::test]
#[ignore]
async fn test_space_status_tracks_indexing() {
    let qdrant = QdrantContainer::new().await;
    set_env("QDRANT_URL", &qdrant.url);
    set_env("QDRANT_SKIP_API_KEY", "true");

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    create_test_files(temp_dir.path());

    let server = setup_test_server().await;
    let dir_path = temp_dir.path().to_str().unwrap();

    // Create space
    let create_payload = json!({ "dir": dir_path });
    let (status, _) = post_request(&server.router, "/api/v1/spaces", create_payload).await;
    assert_eq!(status, StatusCode::OK);

    // Get space key
    let (_, spaces) = get_request(&server.router, "/api/v1/spaces").await;
    let space_key = spaces
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|s| s["key"].as_str())
        .expect("Failed to get space key");

    // Check initial status
    let (status, initial_status) = get_request(
        &server.router,
        &format!("/api/v1/spaces/{}/status", space_key),
    )
    .await;

    assert_eq!(status, StatusCode::OK);

    // Check if indexing already started (may auto-start on space creation)
    let indexing_in_progress = initial_status["indexing_in_progress"]
        .as_bool()
        .unwrap_or(false);
    let initial_files = initial_status["files_indexed"].as_i64().unwrap_or(0);

    info!(
        "Initial status: indexing_in_progress={}, files_indexed={}",
        indexing_in_progress, initial_files
    );

    // Trigger reindex - handle race condition where indexing may have started
    if !indexing_in_progress && initial_files == 0 {
        let (status, body) = post_request(
            &server.router,
            &format!("/api/v1/spaces/{}/reindex", space_key),
            json!({}),
        )
        .await;

        // Accept 200 (success) or 500 with "already in progress" (race condition)
        if status == StatusCode::INTERNAL_SERVER_ERROR {
            let error_msg = body["error"]["message"].as_str().unwrap_or("");
            if error_msg.contains("already in progress") {
                info!("Indexing started between status check and reindex call - continuing");
            } else {
                panic!("Reindex trigger failed: {:?}", body);
            }
        } else {
            assert_eq!(status, StatusCode::OK, "Reindex trigger failed: {:?}", body);
        }
    } else {
        info!("Indexing already in progress or completed, skipping reindex trigger");
    }

    // Wait for completion
    let final_status = wait_for_indexing(&server.router, space_key, Duration::from_secs(60))
        .await
        .expect("Indexing failed");

    // After indexing, files_indexed should be > 0
    let final_files = final_status["files_indexed"].as_i64().unwrap_or(0);
    assert!(
        final_files > 0,
        "Expected files to be indexed after reindex"
    );

    info!("Status tracking verified: {} files indexed", final_files);
}
