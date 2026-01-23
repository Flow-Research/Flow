use axum::{
    extract::FromRequestParts,
    http::{header::AUTHORIZATION, request::Parts, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

use crate::modules::ssi::jwt::{self, Claims};

pub struct AuthenticatedUser {
    pub did: String,
    pub device_id: String,
    pub claims: Claims,
}

pub enum AuthError {
    MissingAuthHeader,
    InvalidAuthHeaderFormat,
    InvalidToken(String),
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AuthError::MissingAuthHeader => (StatusCode::UNAUTHORIZED, "Missing Authorization header"),
            AuthError::InvalidAuthHeaderFormat => (
                StatusCode::UNAUTHORIZED,
                "Invalid Authorization header format. Expected: Bearer <token>",
            ),
            AuthError::InvalidToken(e) => {
                return (StatusCode::UNAUTHORIZED, Json(json!({ "error": format!("Invalid token: {}", e) }))).into_response()
            }
        };
        (status, Json(json!({ "error": message }))).into_response()
    }
}

impl<S> FromRequestParts<S> for AuthenticatedUser
where
    S: Send + Sync,
{
    type Rejection = AuthError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let auth_header = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .ok_or(AuthError::MissingAuthHeader)?;

        let token = auth_header
            .strip_prefix("Bearer ")
            .ok_or(AuthError::InvalidAuthHeaderFormat)?;

        let token_data = jwt::validate_token(token).map_err(|e| AuthError::InvalidToken(e.to_string()))?;

        Ok(AuthenticatedUser {
            did: token_data.claims.did.clone(),
            device_id: token_data.claims.device_id.clone(),
            claims: token_data.claims,
        })
    }
}
