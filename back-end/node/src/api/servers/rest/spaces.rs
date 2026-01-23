//! Space management handlers.
//!
//! These handlers follow the thin controller pattern:
//! - Extract request parameters
//! - Validate input
//! - Delegate to SpaceService or Node
//! - Convert to HTTP response

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::Json;
use serde_json::{json, Value};
use tracing::info;

use crate::api::dto::ApiError;
use crate::api::servers::app_state::AppState;
use crate::modules::storage::spaces::{EdgeDirection, ServiceError, SpaceService};

// ============================================================================
// Error Conversion
// ============================================================================

impl From<ServiceError> for ApiError {
    fn from(err: ServiceError) -> Self {
        match err {
            ServiceError::Database(e) => ApiError::internal(e.to_string()),
            ServiceError::NotFound(msg) => ApiError::not_found(&msg),
            ServiceError::EntityNotFound(msg) => ApiError::not_found(&msg),
        }
    }
}

// ============================================================================
// Handlers (Thin Controllers)
// ============================================================================

/// GET /api/v1/spaces
pub async fn list(State(app_state): State<AppState>) -> Result<Json<Value>, (StatusCode, String)> {
    let node = app_state.node.read().await;
    let service = SpaceService::new(&node.db);

    let spaces = service
        .list_all()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let space_list: Vec<Value> = spaces
        .into_iter()
        .map(|s| {
            json!({
                "id": s.id,
                "key": s.key,
                "name": s.name,
                "location": s.location,
                "time_created": s.created_at.to_string()
            })
        })
        .collect();

    Ok(Json(json!(space_list)))
}

/// POST /api/v1/spaces
pub async fn create(
    State(app_state): State<AppState>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, (StatusCode, String)> {
    // Extract and validate
    let dir = payload["dir"].as_str().unwrap_or("/tmp/space");

    // Delegate to Node (create_space involves filesystem operations)
    let node = app_state.node.read().await;
    match node.create_space(dir).await {
        Ok(_) => Ok(Json(json!({"status": "success"}))),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

/// GET /api/v1/spaces/:key
pub async fn get(
    State(app_state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let node = app_state.node.read().await;
    let service = SpaceService::new(&node.db);

    let space = service.get_by_key(&key).await?;

    Ok(Json(json!({
        "id": space.id,
        "key": space.key,
        "name": space.name,
        "location": space.location,
        "time_created": space.created_at.to_string()
    })))
}

/// DELETE /api/v1/spaces/:key
pub async fn delete(
    State(app_state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let node = app_state.node.read().await;
    let service = SpaceService::new(&node.db);

    service.delete_by_key(&key).await?;

    Ok(Json(
        json!({"success": true, "message": format!("Space {} deleted", key)}),
    ))
}

/// GET /api/v1/spaces/:key/status
pub async fn status(
    State(app_state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let node = app_state.node.read().await;
    let service = SpaceService::new(&node.db);

    let status = service.get_status(&key).await?;

    Ok(Json(json!({
        "indexing_in_progress": status.indexing_in_progress,
        "last_indexed": status.last_indexed.map(|t| t.to_string()),
        "files_indexed": status.files_indexed,
        "chunks_stored": status.chunks_stored,
        "files_failed": status.files_failed,
        "last_error": status.last_error
    })))
}

/// POST /api/v1/spaces/:key/reindex
pub async fn reindex(
    State(app_state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<Value>, ApiError> {
    // Delegate to Node's pipeline_manager (involves async indexing)
    let node = app_state.node.read().await;

    node.pipeline_manager
        .index_space(&key)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;

    info!(space_key = %key, "Space reindexing started");

    Ok(Json(
        json!({"success": true, "message": format!("Reindexing started for space {}", key)}),
    ))
}

/// GET /api/v1/spaces/:key/entities
pub async fn list_entities(
    State(app_state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let node = app_state.node.read().await;
    let service = SpaceService::new(&node.db);

    let entities = service.list_entities(&key).await?;

    let entity_list: Vec<Value> = entities
        .into_iter()
        .map(|e| {
            json!({
                "id": e.id,
                "cid": e.cid,
                "name": e.name,
                "entity_type": e.entity_type,
                "properties": e.properties,
                "created_at": e.created_at.to_string()
            })
        })
        .collect();

    Ok(Json(json!(entity_list)))
}

/// GET /api/v1/spaces/:key/entities/:id
pub async fn get_entity(
    State(app_state): State<AppState>,
    Path((key, id)): Path<(String, i32)>,
) -> Result<Json<Value>, ApiError> {
    let node = app_state.node.read().await;
    let service = SpaceService::new(&node.db);

    let result = service.get_entity(&key, id).await?;

    let edges: Vec<Value> = result
        .edges
        .into_iter()
        .map(|e| {
            let (direction, other_key) = match e.direction {
                EdgeDirection::Outgoing => ("outgoing", "target_id"),
                EdgeDirection::Incoming => ("incoming", "source_id"),
            };
            json!({
                "id": e.id,
                "edge_type": e.edge_type,
                other_key: e.other_id,
                "direction": direction
            })
        })
        .collect();

    Ok(Json(json!({
        "id": result.entity.id,
        "cid": result.entity.cid,
        "name": result.entity.name,
        "entity_type": result.entity.entity_type,
        "properties": result.entity.properties,
        "created_at": result.entity.created_at.to_string(),
        "edges": edges
    })))
}
