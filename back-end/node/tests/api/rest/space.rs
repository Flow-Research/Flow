use crate::{api::rest::helpers::*, bootstrap::init::setup_test_server};
use axum::http::StatusCode;
use entity::space;
use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter};
use serde_json::json;
use tempfile::TempDir;
use tracing::info;

#[tokio::test]
async fn test_create_space_valid_directory() {
    // Setup
    let server = setup_test_server().await;
    let temp_dir = TempDir::new().unwrap();
    let dir_path = temp_dir.path().to_str().unwrap();

    // Execute
    let payload = json!({
        "dir": dir_path
    });

    let (status, body) = post_request(&server.router, "/api/v1/spaces", payload).await;

    // Assert
    assert_eq!(status, StatusCode::OK, "Should return 200 OK");
    assert_eq!(body["status"], "success", "Should return success status");

    info!("Created space at: {}", dir_path);
    info!("Response: {:?}", body);

    // Validate database entry
    let db = &server.node.db;
    let spaces = space::Entity::find()
        .filter(space::Column::Location.contains(dir_path))
        .all(db)
        .await
        .unwrap();

    assert_eq!(spaces.len(), 1, "Should have exactly 1 space in database");

    let stored_space = &spaces[0];
    assert!(
        stored_space.location.contains(dir_path),
        "Location should match"
    );
    assert_eq!(
        stored_space.key.len(),
        64,
        "Key should be 64 char SHA-256 hash"
    );
    assert!(
        stored_space.time_created.timestamp() > 0,
        "Should have valid timestamp"
    );

    info!("Created space at: {}", dir_path);
    info!(
        "Database entry validated: ID={}, Key={}",
        stored_space.id, stored_space.key
    );
}

#[tokio::test]
async fn test_create_space_creates_directory_if_not_exists() {
    // Setup
    let server = setup_test_server().await;
    let temp_dir = TempDir::new().unwrap();
    let new_dir = temp_dir.path().join("new_space_dir");
    let dir_path = new_dir.to_str().unwrap();

    assert!(!new_dir.exists(), "Directory should not exist initially");

    let payload = json!({ "dir": dir_path });
    let (status, body) = post_request(&server.router, "/api/v1/spaces", payload).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "success");
    assert!(new_dir.exists(), "Directory should be created");
    assert!(new_dir.is_dir(), "Should be a directory");

    // DB validation
    let spaces = space::Entity::find()
        .filter(space::Column::Location.contains(dir_path))
        .all(&server.node.db)
        .await
        .unwrap();

    assert_eq!(spaces.len(), 1, "Should have exactly 1 space in DB");
    assert_eq!(spaces[0].key.len(), 64, "Should have valid key");

    info!("Directory created and DB entry validated: {}", dir_path);
}

#[tokio::test]
async fn test_create_space_with_nested_directory() {
    // Setup
    let server = setup_test_server().await;
    let temp_dir = TempDir::new().unwrap();
    let nested_dir = temp_dir
        .path()
        .join("parent")
        .join("child")
        .join("grandchild");
    let dir_path = nested_dir.to_str().unwrap();

    // Execute
    let payload = json!({
        "dir": dir_path
    });

    let (status, body) = post_request(&server.router, "/api/v1/spaces", payload).await;

    // Assert
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "success");

    // Verify nested directory was created
    assert!(nested_dir.exists(), "Nested directory should be created");

    info!("Nested directory created: {}", dir_path);
}

#[tokio::test]
async fn test_create_space_invalid_directory() {
    // Setup
    let server = setup_test_server().await;

    // Use a path that's definitely invalid on Unix-like systems
    // Note: This might behave differently on Windows
    let invalid_path = "/dev/null/invalid/path/that/cannot/be/created";

    // Execute
    let payload = json!({
        "dir": invalid_path
    });

    let (status, body) = post_request(&server.router, "/api/v1/spaces", payload).await;

    // Assert
    assert_eq!(
        status,
        StatusCode::INTERNAL_SERVER_ERROR,
        "Should return 500 for invalid directory"
    );

    // Error message should be present
    assert!(
        body.is_string(),
        "Should return error message as string, got: {:?}",
        body
    );

    info!("Invalid directory error: {:?}", body);
}

#[tokio::test]
async fn test_create_space_duplicate() {
    // Setup
    let server = setup_test_server().await;
    let temp_dir = TempDir::new().unwrap();
    let dir_path = temp_dir.path().to_str().unwrap();

    let payload = json!({ "dir": dir_path });

    // First creation
    let (status1, body1) = post_request(&server.router, "/api/v1/spaces", payload.clone()).await;
    assert_eq!(status1, StatusCode::OK);
    assert_eq!(body1["status"], "success");

    // Count after first insert
    let count_after_first = space::Entity::find()
        .filter(space::Column::Location.contains(dir_path))
        .count(&server.node.db)
        .await
        .unwrap();

    assert_eq!(
        count_after_first, 1,
        "Should have 1 entry after first creation"
    );

    // Second creation (duplicate)
    let (status2, body2) = post_request(&server.router, "/api/v1/spaces", payload).await;
    assert_eq!(status2, StatusCode::OK, "Duplicate should be idempotent");
    assert_eq!(body2["status"], "success");

    // Verify NO duplicate was created in database
    let count_after_second = space::Entity::find()
        .filter(space::Column::Location.contains(dir_path))
        .count(&server.node.db)
        .await
        .unwrap();

    assert_eq!(
        count_after_second, 1,
        "Should still have only 1 entry (not duplicated)"
    );

    info!("Duplicate space creation is idempotent - no DB duplication");
}

#[tokio::test]
async fn test_create_space_no_directory_provided() {
    // Setup
    let server = setup_test_server().await;

    // Execute - Don't provide dir field (should default to /tmp/space)
    let payload = json!({});

    let (status, body) = post_request(&server.router, "/api/v1/spaces", payload).await;

    // Assert - Should use default directory /tmp/space
    assert_eq!(
        status,
        StatusCode::OK,
        "Should succeed with default directory"
    );
    assert_eq!(body["status"], "success");

    info!("Space created with default directory");
}

#[tokio::test]
async fn test_create_space_empty_directory_string() {
    // Setup
    let server = setup_test_server().await;

    // Execute - Provide empty string as directory
    let payload = json!({
        "dir": ""
    });

    let (status, _body) = post_request(&server.router, "/api/v1/spaces", payload).await;

    // Assert - Should fail because empty string is not a valid path
    assert_eq!(
        status,
        StatusCode::INTERNAL_SERVER_ERROR,
        "Empty directory string should fail"
    );

    info!("Empty directory string handled correctly");
}

#[tokio::test]
async fn test_create_space_relative_path() {
    // Setup
    let server = setup_test_server().await;

    // Execute - Use relative path
    let payload = json!({
        "dir": "./test_space_relative"
    });

    let (status, body) = post_request(&server.router, "/api/v1/spaces", payload).await;

    // Assert - Should succeed (relative paths are canonicalized)
    assert_eq!(status, StatusCode::OK, "Relative path should be accepted");
    assert_eq!(body["status"], "success");

    info!("Relative path handled correctly");

    // Cleanup
    let _ = std::fs::remove_dir_all("./test_space_relative");
}

#[tokio::test]
async fn test_create_space_with_special_characters() {
    // Setup
    let server = setup_test_server().await;
    let temp_dir = TempDir::new().unwrap();

    // Create directory with special characters (but valid on most filesystems)
    let special_dir = temp_dir.path().join("space-with_special.chars");
    let dir_path = special_dir.to_str().unwrap();

    // Execute
    let payload = json!({
        "dir": dir_path
    });

    let (status, body) = post_request(&server.router, "/api/v1/spaces", payload).await;

    // Assert
    assert_eq!(status, StatusCode::OK, "Should handle special characters");
    assert_eq!(body["status"], "success");
    assert!(
        special_dir.exists(),
        "Directory with special chars should be created"
    );

    info!("Special characters handled: {}", dir_path);
}

#[tokio::test]
async fn test_create_space_generates_deterministic_key() {
    // Setup
    let server = setup_test_server().await;
    let temp_dir = TempDir::new().unwrap();
    let dir_path = temp_dir.path().to_str().unwrap();

    let payload = json!({ "dir": dir_path });

    let (status1, _) = post_request(&server.router, "/api/v1/spaces", payload.clone()).await;
    assert_eq!(status1, StatusCode::OK);

    // Get the space key from first creation
    let space1 = space::Entity::find()
        .filter(space::Column::Location.contains(dir_path))
        .one(&server.node.db)
        .await
        .unwrap()
        .expect("Space should exist");

    let key1 = space1.key.clone();
    let id1 = space1.id;

    // Second creation
    let (status2, _) = post_request(&server.router, "/api/v1/spaces", payload).await;
    assert_eq!(status2, StatusCode::OK);

    // Verify the key is still the same (deterministic)
    let space2 = space::Entity::find()
        .filter(space::Column::Location.contains(dir_path))
        .one(&server.node.db)
        .await
        .unwrap()
        .expect("Space should still exist");

    assert_eq!(key1, space2.key, "Key should be deterministic");
    assert_eq!(id1, space2.id, "Should be the same database record");

    info!("Deterministic key generation verified: {}", key1);
}

#[tokio::test]
async fn test_create_space_different_directories_succeed() {
    // Setup
    let server = setup_test_server().await;
    let temp_dir = TempDir::new().unwrap();

    let dir1 = temp_dir.path().join("space1");
    let dir2 = temp_dir.path().join("space2");

    let payload1 = json!({ "dir": dir1.to_str().unwrap() });
    let payload2 = json!({ "dir": dir2.to_str().unwrap() });

    let (status1, _) = post_request(&server.router, "/api/v1/spaces", payload1).await;
    let (status2, _) = post_request(&server.router, "/api/v1/spaces", payload2).await;

    assert_eq!(status1, StatusCode::OK);
    assert_eq!(status2, StatusCode::OK);
    assert!(dir1.exists());
    assert!(dir2.exists());

    // DB validation - find our specific spaces
    let space1 = space::Entity::find()
        .filter(space::Column::Location.contains("space1"))
        .one(&server.node.db)
        .await
        .unwrap();

    let space2 = space::Entity::find()
        .filter(space::Column::Location.contains("space2"))
        .one(&server.node.db)
        .await
        .unwrap();

    assert!(space1.is_some(), "Space1 should exist in database");
    assert!(space2.is_some(), "Space2 should exist in database");

    // Verify they have different keys
    assert_ne!(
        space1.unwrap().key,
        space2.unwrap().key,
        "Different directories should have different keys"
    );

    info!("Multiple different spaces created successfully in database");
}

#[tokio::test]
async fn test_create_space_malformed_json() {
    // Setup
    let server = setup_test_server().await;

    // Execute - Send non-object JSON (array instead)
    let payload = json!([1, 2, 3]);

    let (status, _body) = post_request(&server.router, "/api/v1/spaces", payload).await;

    // Assert - Should handle gracefully (defaults to /tmp/space)
    // or return error depending on implementation
    assert!(
        status == StatusCode::OK || status == StatusCode::BAD_REQUEST,
        "Should handle malformed JSON, got: {}",
        status
    );

    info!("Malformed JSON handled with status: {}", status);
}

#[tokio::test]
async fn test_create_space_concurrent_requests() {
    // Setup
    let server = setup_test_server().await;
    let temp_dir = TempDir::new().unwrap();
    let dir_path = temp_dir.path().to_str().unwrap().to_string();

    // Execute - Send concurrent requests for same directory
    let payload = json!({ "dir": dir_path });

    let app_clone = server.router.clone();
    let payload_clone = payload.clone();

    let handle1 =
        tokio::spawn(async move { post_request(&server.router, "/api/v1/spaces", payload).await });

    let handle2 =
        tokio::spawn(
            async move { post_request(&app_clone, "/api/v1/spaces", payload_clone).await },
        );

    let (result1, result2) = tokio::join!(handle1, handle2);

    let (status1, _) = result1.unwrap();
    let (status2, _) = result2.unwrap();

    // Assert - Both should succeed (idempotent)
    assert_eq!(status1, StatusCode::OK, "First request should succeed");
    assert_eq!(status2, StatusCode::OK, "Second request should succeed");

    info!("Concurrent requests handled successfully");
}
