use std::collections::HashMap;

use crate::{
    api::{
        dto::{DialPeerRequest, NetworkStatusResponse, PeerInfoResponse, PeersResponse},
        servers::app_state::AppState,
    },
    bootstrap::config::Config,
    modules::network::config::NetworkConfig,
};
use axum::{
    Router,
    extract::{Query, State},
    http::{HeaderValue, Method, StatusCode},
    response::Json,
    routing::{get, post},
};
use errors::AppError;
use serde_json::{Value, json};
use tower_http::cors::{Any, CorsLayer};
use tracing::{error, info, warn};
use webauthn_rs::prelude::{PublicKeyCredential, RegisterPublicKeyCredential};

/// Build the router with all routes configured
pub fn build_router(app_state: AppState) -> Router {
    // Configure CORS
    let cors = CorsLayer::new()
        // Allow requests from these origins
        .allow_origin(["http://localhost:3000".parse::<HeaderValue>().unwrap()])
        // Allow these HTTP methods
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        // Allow headers
        .allow_headers(Any)
        // Cache preflight requests for 1 hour
        .max_age(std::time::Duration::from_secs(3600));

    let api_base: &str = "/api/v1";

    // Configure Router
    Router::new()
        .route(
            format!("{}/webauthn/start_registration", api_base).as_str(),
            get(start_webauthn_registration),
        )
        .route(
            format!("{}/webauthn/finish_registration", api_base).as_str(),
            post(finish_webauthn_registration),
        )
        .route(
            format!("{}/webauthn/start_authentication", api_base).as_str(),
            post(start_webauthn_authentication),
        )
        .route(
            format!("{}/webauthn/finish_authentication", api_base).as_str(),
            post(finish_webauthn_authentication),
        )
        .route(format!("{}/spaces", api_base).as_str(), post(create_space))
        .route(format!("{}/health", api_base).as_str(), get(health_check))
        .route(
            format!("{}/spaces/search", api_base).as_str(),
            get(query_space),
        )
        .route(
            format!("{}/network/status", api_base).as_str(),
            get(get_network_status),
        )
        .route(
            format!("{}/network/peers", api_base).as_str(),
            get(get_network_peers),
        )
        .route(
            format!("{}/network/start", api_base).as_str(),
            post(start_network),
        )
        .route(
            format!("{}/network/stop", api_base).as_str(),
            post(stop_network),
        )
        .route(
            format!("{}/network/dial", api_base).as_str(),
            post(dial_peer),
        )
        .with_state(app_state)
        .layer(cors)
}

pub async fn start(app_state: &AppState, config: &Config) -> Result<(), AppError> {
    let app = build_router(app_state.clone());

    let bind_addr = format!("0.0.0.0:{}", config.server.rest_port);
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    axum::serve(listener, app).await?;

    info!("Rest Server up on addr: {}", &bind_addr);

    Ok(())
}

async fn start_webauthn_registration(
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

async fn finish_webauthn_registration(
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

async fn start_webauthn_authentication(
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

async fn finish_webauthn_authentication(
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

    Ok(Json(json!({
        "verified": true,
        "message": "Authentication successful",
        "counter": auth_result.counter(),
        "backup_state": auth_result.backup_state(),
        "backup_eligible": auth_result.backup_eligible(),
        "needs_update": auth_result.needs_update()
    })))
}

async fn create_space(
    State(app_state): State<AppState>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let node = app_state.node.read().await;
    let dir = payload["dir"].as_str().unwrap_or("/tmp/space");

    match node.create_space(dir).await {
        Ok(_) => Ok(Json(json!({"status": "success"}))),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

async fn query_space(
    State(app_state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let space_key = params
        .get("space_key")
        .ok_or((StatusCode::BAD_REQUEST, "space_key is required".to_string()))?;
    let query = params
        .get("query")
        .ok_or((StatusCode::BAD_REQUEST, "query is required".to_string()))?;

    let node = app_state.node.read().await;

    match node.query_space(space_key, query).await {
        Ok(response) => Ok(Json(json!({
            "status": "success",
            "response": response
        }))),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

async fn health_check() -> Json<Value> {
    Json(json!({"status": "healthy", "timestamp": chrono::Utc::now()}))
}

// ==================== Network Handlers ====================

/// GET /api/v1/network/status
async fn get_network_status(
    State(app_state): State<AppState>,
) -> Result<Json<NetworkStatusResponse>, (StatusCode, String)> {
    let node = app_state.node.read().await;

    let running = node.is_network_running().await;
    let peer_id = node.peer_id();

    let (connected_peers, known_peers, uptime_secs, subscribed_topics) = if running {
        match node.network_stats().await {
            Ok(stats) => {
                let topics = node.subscribed_topics().await.unwrap_or_default();
                (
                    stats.connected_peers,
                    stats.known_peers,
                    Some(stats.uptime_secs),
                    topics,
                )
            }
            Err(e) => {
                warn!(error = %e, "Failed to get network stats");
                (0, 0, None, vec![])
            }
        }
    } else {
        (0, 0, None, vec![])
    };

    info!(
        running = running,
        peer_id = %peer_id,
        connected_peers = connected_peers,
        "Network status requested"
    );

    Ok(Json(NetworkStatusResponse {
        running,
        peer_id,
        connected_peers,
        known_peers,
        uptime_secs,
        subscribed_topics,
    }))
}

async fn get_network_peers(
    State(app_state): State<AppState>,
) -> Result<Json<PeersResponse>, (StatusCode, String)> {
    let node = app_state.node.read().await;

    if !node.is_network_running().await {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "Network is not running".to_string(),
        ));
    }

    let peers = node
        .connected_peers()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let peer_responses: Vec<PeerInfoResponse> = peers
        .into_iter()
        .map(|p| PeerInfoResponse {
            peer_id: p.peer_id.to_string(),
            addresses: p.addresses.iter().map(|a| a.to_string()).collect(),
            connection_count: p.connection_count,
            direction: format!("{:?}", p.connection_direction),
            discovery_source: format!("{:?}", p.discovery_source),
        })
        .collect();

    let count = peer_responses.len();
    info!(peer_count = count, "Peers list requested");

    Ok(Json(PeersResponse {
        count,
        peers: peer_responses,
    }))
}

async fn start_network(
    State(app_state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let node = app_state.node.read().await;

    if node.is_network_running().await {
        return Err((
            StatusCode::CONFLICT,
            "Network is already running".to_string(),
        ));
    }

    let config = NetworkConfig::from_env();

    node.start_network(&config).await.map_err(|e| {
        error!(error = %e, "Failed to start network");
        (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    })?;

    // Broadcast event using existing infrastructure
    node.broadcast_network_event(
        "NetworkStarted",
        serde_json::json!({ "peer_id": node.peer_id() }),
    )
    .await;

    info!(peer_id = %node.peer_id(), "Network started via API");

    Ok(Json(json!({
        "success": true,
        "message": "Network started successfully",
        "peer_id": node.peer_id()
    })))
}

async fn stop_network(
    State(app_state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let node = app_state.node.read().await;

    if !node.is_network_running().await {
        return Err((StatusCode::CONFLICT, "Network is not running".to_string()));
    }

    node.stop_network().await.map_err(|e| {
        error!(error = %e, "Failed to stop network");
        (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    })?;

    // Broadcast event using existing infrastructure
    node.broadcast_network_event("NetworkStopped", serde_json::json!({}))
        .await;

    info!("Network stopped via API");

    Ok(Json(json!({
        "success": true,
        "message": "Network stopped successfully"
    })))
}

async fn dial_peer(
    State(app_state): State<AppState>,
    Json(payload): Json<DialPeerRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let node = app_state.node.read().await;

    if !node.is_network_running().await {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "Network is not running".to_string(),
        ));
    }

    node.dial_peer(&payload.address).await.map_err(|e| {
        warn!(address = %payload.address, error = %e, "Failed to dial peer");
        (StatusCode::BAD_REQUEST, e.to_string())
    })?;

    info!(address = %payload.address, "Dialed peer via API");

    Ok(Json(json!({
        "success": true,
        "message": format!("Dialing peer at {}", payload.address)
    })))
}
