//! REST API router configuration.
//!
//! This module contains route definitions and server startup logic.
//! All handler implementations are in their respective submodules.

mod auth;
mod content;
mod health;
mod network;
mod search;
mod spaces;

use crate::api::servers::app_state::AppState;
use crate::bootstrap::config::Config;
use axum::routing::{get, post};
use axum::Router;
use errors::AppError;
use http::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, ORIGIN};
use http::{HeaderValue, Method};
use tower_http::cors::CorsLayer;
use tracing::info;

/// Build the REST API router with all routes.
pub fn build_router(app_state: AppState, config: &Config) -> Router {
    let cors = build_cors_layer(config);
    let api = "/api/v1";

    Router::new()
        // Health
        .route(&format!("{api}/health"), get(health::check))
        // WebAuthn Authentication
        .route(
            &format!("{api}/webauthn/start_registration"),
            get(auth::start_registration),
        )
        .route(
            &format!("{api}/webauthn/finish_registration"),
            post(auth::finish_registration),
        )
        .route(
            &format!("{api}/webauthn/start_authentication"),
            post(auth::start_authentication),
        )
        .route(
            &format!("{api}/webauthn/finish_authentication"),
            post(auth::finish_authentication),
        )
        // Spaces
        .route(
            &format!("{api}/spaces"),
            get(spaces::list).post(spaces::create),
        )
        .route(&format!("{api}/spaces/search"), get(search::query))
        .route(
            &format!("{api}/spaces/{{key}}"),
            get(spaces::get).delete(spaces::delete),
        )
        .route(&format!("{api}/spaces/{{key}}/status"), get(spaces::status))
        .route(
            &format!("{api}/spaces/{{key}}/reindex"),
            post(spaces::reindex),
        )
        .route(
            &format!("{api}/spaces/{{key}}/entities"),
            get(spaces::list_entities),
        )
        .route(
            &format!("{api}/spaces/{{key}}/entities/{{id}}"),
            get(spaces::get_entity),
        )
        // Network
        .route(&format!("{api}/network/status"), get(network::status))
        .route(&format!("{api}/network/peers"), get(network::peers))
        .route(&format!("{api}/network/start"), post(network::start))
        .route(&format!("{api}/network/stop"), post(network::stop))
        .route(&format!("{api}/network/dial"), post(network::dial))
        // Content Discovery
        .route(&format!("{api}/content/publish"), post(content::publish))
        .route(
            &format!("{api}/content/published"),
            get(content::list_published),
        )
        .route(
            &format!("{api}/content/discovered"),
            get(content::list_discovered),
        )
        .route(&format!("{api}/content/{{cid}}"), get(content::get))
        .route(
            &format!("{api}/content/{{cid}}/index"),
            post(content::trigger_index),
        )
        .route(
            &format!("{api}/content/{{cid}}/providers"),
            get(content::providers),
        )
        // Search
        .route(&format!("{api}/search"), post(content::search))
        .with_state(app_state)
        .layer(cors)
}

fn build_cors_layer(config: &Config) -> CorsLayer {
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
        .allow_headers([ORIGIN, ACCEPT, CONTENT_TYPE, AUTHORIZATION])
        .max_age(std::time::Duration::from_secs(3600));

    if config.cors.allow_credentials {
        cors = cors.allow_credentials(true);
    }

    cors
}

/// Start the REST server.
pub async fn start(app_state: &AppState, config: &Config) -> Result<(), AppError> {
    let app = build_router(app_state.clone(), config);
    let bind_addr = format!("0.0.0.0:{}", config.server.rest_port);

    info!("Starting REST server on {}", &bind_addr);
    info!("CORS allowed origins: {:?}", config.cors.allowed_origins);

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
