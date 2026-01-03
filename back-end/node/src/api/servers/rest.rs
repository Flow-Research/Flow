use std::collections::HashMap;

use crate::{
    api::{
        dto::{ApiError, DialPeerRequest, NetworkStatusResponse, PeerInfoResponse, PeersResponse},
        servers::app_state::AppState,
    },
    bootstrap::config::Config,
    modules::network::config::NetworkConfig,
    modules::ssi::jwt,
};
use axum::{
    Router,
    extract::{Path, Query, State},
    http::{HeaderValue, Method, StatusCode},
    response::Json,
    routing::{get, post},
};
use errors::AppError;
use serde_json::{Value, json};
use tower_http::cors::{Any, CorsLayer};
use tracing::{error, info, warn};
use webauthn_rs::prelude::{PublicKeyCredential, RegisterPublicKeyCredential};

pub fn build_router(app_state: AppState, config: &Config) -> Router {
    let origins: Vec<HeaderValue> = config
        .cors
        .allowed_origins
        .iter()
        .filter_map(|origin| origin.parse::<HeaderValue>().ok())
        .collect();

    let mut cors = CorsLayer::new()
        .allow_origin(origins)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers(Any)
        .max_age(std::time::Duration::from_secs(3600));

    if config.cors.allow_credentials {
        cors = cors.allow_credentials(true);
    }

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
        .route(
            format!("{}/spaces", api_base).as_str(),
            get(list_spaces).post(create_space),
        )
        .route(format!("{}/health", api_base).as_str(), get(health_check))
        .route(
            format!("{}/spaces/search", api_base).as_str(),
            get(query_space),
        )
        .route(
            format!("{}/spaces/:key", api_base).as_str(),
            get(get_space).delete(delete_space),
        )
        .route(
            format!("{}/spaces/:key/status", api_base).as_str(),
            get(get_space_status),
        )
        .route(
            format!("{}/spaces/:key/reindex", api_base).as_str(),
            post(reindex_space),
        )
        .route(
            format!("{}/spaces/:key/entities", api_base).as_str(),
            get(list_entities),
        )
        .route(
            format!("{}/spaces/:key/entities/:id", api_base).as_str(),
            get(get_entity),
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
    let app = build_router(app_state.clone(), config);

    let bind_addr = format!("0.0.0.0:{}", config.server.rest_port);
    info!("Starting REST server on {}", &bind_addr);
    info!(
        "CORS allowed origins: {:?}",
        config.cors.allowed_origins
    );

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    axum::serve(listener, app).await?;

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

async fn list_spaces(
    State(app_state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, String)> {
    use entity::space;
    use sea_orm::EntityTrait;

    let node = app_state.node.read().await;

    let spaces = space::Entity::find()
        .all(&node.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let space_list: Vec<Value> = spaces
        .into_iter()
        .map(|s| {
            json!({
                "id": s.id,
                "key": s.key,
                "location": s.location,
                "time_created": s.time_created.to_string()
            })
        })
        .collect();

    Ok(Json(json!(space_list)))
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

async fn get_space(
    State(app_state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<Value>, ApiError> {
    use entity::space;
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

    let node = app_state.node.read().await;

    let space = space::Entity::find()
        .filter(space::Column::Key.eq(&key))
        .one(&node.db)
        .await?
        .ok_or_else(|| ApiError::not_found(&format!("Space '{}'", key)))?;

    Ok(Json(json!({
        "id": space.id,
        "key": space.key,
        "location": space.location,
        "time_created": space.time_created.to_string()
    })))
}

async fn delete_space(
    State(app_state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<Value>, ApiError> {
    use entity::space;
    use sea_orm::{ColumnTrait, EntityTrait, ModelTrait, QueryFilter};

    let node = app_state.node.read().await;

    let space = space::Entity::find()
        .filter(space::Column::Key.eq(&key))
        .one(&node.db)
        .await?
        .ok_or_else(|| ApiError::not_found(&format!("Space '{}'", key)))?;

    space.delete(&node.db).await?;

    info!(space_key = %key, "Space deleted");

    Ok(Json(json!({"success": true, "message": format!("Space {} deleted", key)})))
}

async fn get_space_status(
    State(app_state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<Value>, ApiError> {
    use entity::{space, space_index_status};
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

    let node = app_state.node.read().await;

    let space = space::Entity::find()
        .filter(space::Column::Key.eq(&key))
        .one(&node.db)
        .await?
        .ok_or_else(|| ApiError::not_found(&format!("Space '{}'", key)))?;

    let status = space_index_status::Entity::find()
        .filter(space_index_status::Column::SpaceId.eq(space.id))
        .one(&node.db)
        .await?;

    match status {
        Some(s) => Ok(Json(json!({
            "indexing_in_progress": s.indexing_in_progress.unwrap_or(false),
            "last_indexed": s.last_indexed.map(|t| t.to_string()),
            "files_indexed": s.files_indexed.unwrap_or(0),
            "chunks_stored": s.chunks_stored.unwrap_or(0),
            "files_failed": s.files_failed.unwrap_or(0),
            "last_error": s.last_error
        }))),
        None => Ok(Json(json!({
            "indexing_in_progress": false,
            "last_indexed": null,
            "files_indexed": 0,
            "chunks_stored": 0,
            "files_failed": 0,
            "last_error": null
        }))),
    }
}

async fn reindex_space(
    State(app_state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let node = app_state.node.read().await;

    node.pipeline_manager
        .index_space(&key)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;

    info!(space_key = %key, "Space reindexing started");

    Ok(Json(json!({"success": true, "message": format!("Reindexing started for space {}", key)})))
}

async fn list_entities(
    State(app_state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<Value>, ApiError> {
    use entity::{kg_entity, space};
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder};

    let node = app_state.node.read().await;

    let space = space::Entity::find()
        .filter(space::Column::Key.eq(&key))
        .one(&node.db)
        .await?
        .ok_or_else(|| ApiError::not_found(&format!("Space '{}'", key)))?;

    let entities = kg_entity::Entity::find()
        .filter(kg_entity::Column::SpaceId.eq(space.id))
        .order_by_desc(kg_entity::Column::CreatedAt)
        .all(&node.db)
        .await?;

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

async fn get_entity(
    State(app_state): State<AppState>,
    Path((key, id)): Path<(String, i32)>,
) -> Result<Json<Value>, ApiError> {
    use entity::{kg_edge, kg_entity, space};
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

    let node = app_state.node.read().await;

    let space = space::Entity::find()
        .filter(space::Column::Key.eq(&key))
        .one(&node.db)
        .await?
        .ok_or_else(|| ApiError::not_found(&format!("Space '{}'", key)))?;

    let entity = kg_entity::Entity::find_by_id(id)
        .filter(kg_entity::Column::SpaceId.eq(space.id))
        .one(&node.db)
        .await?
        .ok_or_else(|| ApiError::not_found(&format!("Entity {}", id)))?;

    let outgoing_edges = kg_edge::Entity::find()
        .filter(kg_edge::Column::SourceId.eq(id))
        .all(&node.db)
        .await?;

    let incoming_edges = kg_edge::Entity::find()
        .filter(kg_edge::Column::TargetId.eq(id))
        .all(&node.db)
        .await?;

    let edges: Vec<Value> = outgoing_edges
        .into_iter()
        .map(|e| {
            json!({
                "id": e.id,
                "edge_type": e.edge_type,
                "target_id": e.target_id,
                "direction": "outgoing"
            })
        })
        .chain(incoming_edges.into_iter().map(|e| {
            json!({
                "id": e.id,
                "edge_type": e.edge_type,
                "source_id": e.source_id,
                "direction": "incoming"
            })
        }))
        .collect();

    Ok(Json(json!({
        "id": entity.id,
        "cid": entity.cid,
        "name": entity.name,
        "entity_type": entity.entity_type,
        "properties": entity.properties,
        "created_at": entity.created_at.to_string(),
        "edges": edges
    })))
}

async fn query_space(
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

async fn health_check() -> Json<Value> {
    Json(json!({"status": "healthy", "timestamp": chrono::Utc::now()}))
}

// ==================== Network Handlers ====================

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
