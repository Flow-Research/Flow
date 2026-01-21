//! Search handlers.

use std::collections::HashMap;

use axum::extract::{Query, State};
use axum::response::Json;
use serde_json::{json, Value};

use crate::api::dto::ApiError;
use crate::api::servers::app_state::AppState;

/// Query a space.
pub async fn query(
    State(app_state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let space_key = params
        .get("space_key")
        .ok_or_else(|| ApiError::validation("space_key is required"))?;
    let query = params
        .get("query")
        .ok_or_else(|| ApiError::validation("query is required"))?;

    let node = app_state.node.read().await;

    let response = node
        .query_space(space_key, query)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;

    Ok(Json(json!({
        "success": true,
        "response": response
    })))
}
