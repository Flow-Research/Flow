use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    AuthFailed,
    NotFound,
    ValidationError,
    SpaceExists,
    IndexingInProgress,
    LlmError,
    InternalError,
}

impl ErrorCode {
    pub fn status_code(&self) -> StatusCode {
        match self {
            ErrorCode::AuthFailed => StatusCode::UNAUTHORIZED,
            ErrorCode::NotFound => StatusCode::NOT_FOUND,
            ErrorCode::ValidationError => StatusCode::BAD_REQUEST,
            ErrorCode::SpaceExists => StatusCode::CONFLICT,
            ErrorCode::IndexingInProgress => StatusCode::CONFLICT,
            ErrorCode::LlmError => StatusCode::SERVICE_UNAVAILABLE,
            ErrorCode::InternalError => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ApiError {
    pub code: ErrorCode,
    pub message: String,
}

impl ApiError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn not_found(resource: &str) -> Self {
        Self::new(ErrorCode::NotFound, format!("{} not found", resource))
    }

    pub fn validation(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::ValidationError, message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InternalError, message)
    }

    pub fn auth_failed(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::AuthFailed, message)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.code.status_code();
        let body = Json(json!({
            "success": false,
            "error": {
                "code": self.code,
                "message": self.message
            }
        }));
        (status, body).into_response()
    }
}

impl From<sea_orm::DbErr> for ApiError {
    fn from(err: sea_orm::DbErr) -> Self {
        ApiError::internal(err.to_string())
    }
}

impl From<String> for ApiError {
    fn from(err: String) -> Self {
        ApiError::internal(err)
    }
}

#[derive(Debug, Serialize)]
pub struct NetworkStatusResponse {
    pub running: bool,
    pub peer_id: String,
    pub connected_peers: usize,
    pub known_peers: usize,
    pub uptime_secs: Option<u64>,
    pub subscribed_topics: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct PeersResponse {
    pub count: usize,
    pub peers: Vec<PeerInfoResponse>,
}

#[derive(Debug, Serialize)]
pub struct PeerInfoResponse {
    pub peer_id: String,
    pub addresses: Vec<String>,
    pub connection_count: usize,
    pub direction: String,
    pub discovery_source: String,
}

#[derive(Debug, Deserialize)]
pub struct DialPeerRequest {
    pub address: String,
}
