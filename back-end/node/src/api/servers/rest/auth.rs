//! WebAuthn authentication handlers.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Json;
use serde_json::{json, Value};
use tracing::{error, info};
use webauthn_rs::prelude::{PublicKeyCredential, RegisterPublicKeyCredential};

use crate::api::servers::app_state::AppState;
use crate::modules::ssi::jwt;

/// Start WebAuthn registration.
pub async fn start_registration(
    State(app_state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let node = app_state.node.read().await;
    match node.start_webauthn_registration().await {
        Ok((challenge, challenge_key)) => {
            info!(
                "WebAuthn registration started successfully with challenge_id: {}",
                challenge_key
            );
            Ok(Json(json!({
                "challenge": challenge,
                "challenge_id": challenge_key
            })))
        }
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

/// Finish WebAuthn registration.
pub async fn finish_registration(
    State(app_state): State<AppState>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let challenge_id = payload["challenge_id"].as_str().ok_or_else(|| {
        error!("Missing challenge_id in request payload");
        (StatusCode::BAD_REQUEST, "Missing challenge_id".to_string())
    })?;

    let credential_value = payload["credential"].as_object().ok_or_else(|| {
        error!("Missing credential in request payload");
        (StatusCode::BAD_REQUEST, "Missing credential".to_string())
    })?;

    let reg_credential = serde_json::from_value::<RegisterPublicKeyCredential>(
        serde_json::Value::Object(credential_value.clone()),
    )
    .map_err(|e| {
        error!("Failed to parse credential: {}", e);
        (
            StatusCode::BAD_REQUEST,
            format!("Invalid credential format: {}", e),
        )
    })?;

    let node = app_state.node.read().await;
    let (did, did_document) = node
        .finish_webauthn_registration(challenge_id, reg_credential)
        .await
        .map_err(|e| {
            error!(
                "WebAuthn registration failed for challenge_id {}: {}",
                challenge_id, e
            );
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        })?;

    Ok(Json(json!({
        "verified": true,
        "message": "Passkey registered successfully",
        "did": did,
        "didDocument": serde_json::from_str::<Value>(&did_document).unwrap_or(json!({}))
    })))
}

/// Start WebAuthn authentication.
pub async fn start_authentication(
    State(app_state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let node = app_state.node.read().await;
    match node.start_webauthn_authentication().await {
        Ok((challenge, challenge_id)) => {
            info!(
                "WebAuthn authentication started successfully with challenge_id: {}",
                challenge_id
            );
            Ok(Json(json!({
                "challenge": challenge,
                "challenge_id": challenge_id
            })))
        }
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

/// Finish WebAuthn authentication.
pub async fn finish_authentication(
    State(app_state): State<AppState>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let challenge_id = payload["challenge_id"].as_str().ok_or_else(|| {
        error!("Missing challenge_id in request payload");
        (StatusCode::BAD_REQUEST, "Missing challenge_id".to_string())
    })?;

    let credential_value = payload["credential"].as_object().ok_or_else(|| {
        error!("Missing credential in request payload");
        (StatusCode::BAD_REQUEST, "Missing credential".to_string())
    })?;

    let auth_credential = serde_json::from_value::<PublicKeyCredential>(serde_json::Value::Object(
        credential_value.clone(),
    ))
    .map_err(|e| {
        error!("Failed to parse authentication credential: {}", e);
        (
            StatusCode::BAD_REQUEST,
            format!("Invalid credential format: {}", e),
        )
    })?;

    let node = app_state.node.read().await;
    let auth_result = node
        .finish_webauthn_authentication(challenge_id, auth_credential)
        .await
        .map_err(|e| {
            error!(
                "WebAuthn authentication failed for challenge_id {}: {}",
                challenge_id, e
            );
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        })?;

    let device_id = &node.node_data.id;
    let user = get_user_by_device_id(&node.db, device_id)
        .await
        .map_err(|e| {
            error!("Failed to get user for device {}: {}", device_id, e);
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        })?;

    let token = jwt::generate_token(&user.did, device_id).map_err(|e| {
        error!("Failed to generate JWT: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    })?;

    info!(
        "Authentication successful for user {} (DID: {})",
        user.id, user.did
    );

    Ok(Json(json!({
        "verified": true,
        "message": "Authentication successful",
        "token": token,
        "did": user.did,
        "counter": auth_result.counter(),
        "backup_state": auth_result.backup_state(),
        "backup_eligible": auth_result.backup_eligible(),
        "needs_update": auth_result.needs_update()
    })))
}

async fn get_user_by_device_id(
    db: &sea_orm::DatabaseConnection,
    device_id: &str,
) -> Result<entity::user::Model, String> {
    use entity::pass_key;
    use entity::user;
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

    let passkey = pass_key::Entity::find()
        .filter(pass_key::Column::DeviceId.eq(device_id))
        .one(db)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Passkey not found".to_string())?;

    let user = user::Entity::find_by_id(passkey.user_id)
        .one(db)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "User not found".to_string())?;

    Ok(user)
}
