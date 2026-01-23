//! Network management handlers.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Json;
use serde_json::{json, Value};
use tracing::{error, info, warn};

use crate::api::dto::{DialPeerRequest, NetworkStatusResponse, PeerInfoResponse, PeersResponse};
use crate::api::servers::app_state::AppState;
use crate::modules::network::config::NetworkConfig;

/// Get network status.
pub async fn status(
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

/// Get connected peers.
pub async fn peers(
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
            direction: p.connection_direction.as_str().to_string(),
            discovery_source: p.discovery_source.as_str().to_string(),
        })
        .collect();

    let count = peer_responses.len();
    info!(peer_count = count, "Peers list requested");

    Ok(Json(PeersResponse {
        count,
        peers: peer_responses,
    }))
}

/// Start the network.
pub async fn start(
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

/// Stop the network.
pub async fn stop(
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

    node.broadcast_network_event("NetworkStopped", serde_json::json!({}))
        .await;

    info!("Network stopped via API");

    Ok(Json(json!({
        "success": true,
        "message": "Network stopped successfully"
    })))
}

/// Dial a peer.
pub async fn dial(
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
