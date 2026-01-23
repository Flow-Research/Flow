//! Health check handler.

use axum::response::Json;
use serde_json::{json, Value};

/// Health check endpoint.
pub async fn check() -> Json<Value> {
    Json(json!({"status": "healthy", "timestamp": chrono::Utc::now()}))
}
