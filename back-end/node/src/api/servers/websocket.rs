use crate::{
    api::servers::app_state::AppState,
    bootstrap::config::Config,
    modules::network::gossipsub::{MessagePayload, SYSTEM_NETWORK_EVENTS_TOPIC},
};
use axum::{
    Router,
    extract::{State, WebSocketUpgrade, ws::WebSocket},
    response::Response,
    routing::get,
};
use errors::AppError;
use futures_util::{sink::SinkExt, stream::StreamExt};
use serde_json::{Value, json};
use tracing::{debug, info, warn};

pub async fn start(app_state: &AppState, config: &Config) -> Result<(), AppError> {
    let app = Router::new()
        .route("/ws", get(websocket_handler))
        .with_state(app_state.clone());

    let listener =
        tokio::net::TcpListener::bind(format!("0.0.0.0:{}", config.server.websocket_port)).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

pub async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(app_state): State<AppState>,
) -> Response {
    ws.on_upgrade(|socket| websocket_connection(socket, app_state))
}

async fn websocket_connection(socket: WebSocket, app_state: AppState) {
    let (mut sender, mut receiver) = socket.split();

    // Subscribe to network events using EXISTING TopicSubscriptionManager
    let mut network_events = {
        let node = app_state.node.read().await;
        node.network_manager
            .subscription_manager
            .subscribe(SYSTEM_NETWORK_EVENTS_TOPIC)
            .await
    };

    info!("WebSocket client connected, subscribed to network events");

    loop {
        tokio::select! {
            // Handle incoming messages from client
            msg = receiver.next() => {
                match msg {
                    Some(Ok(axum::extract::ws::Message::Text(text))) => {
                        if let Ok(payload) = serde_json::from_str::<Value>(&text) {
                            handle_websocket_message(&app_state, &mut sender, payload).await;
                        }
                    }
                    Some(Ok(axum::extract::ws::Message::Close(_))) => {
                        debug!("WebSocket client disconnected (close frame)");
                        break;
                    }
                    Some(Err(e)) => {
                        warn!(error = %e, "WebSocket receive error");
                        break;
                    }
                    None => {
                        debug!("WebSocket stream ended");
                        break;
                    }
                    _ => {}
                }
            }

            // Forward network events to client (using existing SubscriptionHandle)
            event = network_events.recv() => {
                match event {
                    Ok(message) => {
                        // Extract event data from the Message payload
                        let ws_message = match &message.payload {
                            MessagePayload::NetworkEvent { event_type, data } => {
                                json!({
                                    "event": "network",
                                    "payload": {
                                        "type": event_type,
                                        "data": data,
                                        "timestamp": message.timestamp,
                                        "source": message.source
                                    }
                                })
                            }
                            _ => {
                                // Handle other message types if needed
                                json!({
                                    "event": "message",
                                    "payload": {
                                        "topic": message.topic,
                                        "id": message.id
                                    }
                                })
                            }
                        };

                        if let Err(e) = sender
                            .send(axum::extract::ws::Message::Text(ws_message.to_string().into()))
                            .await
                        {
                            warn!(error = %e, "Failed to send event to WebSocket client");
                            break;
                        }

                        debug!(message_id = %message.id, "Sent network event to client");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        warn!(missed = n, "WebSocket client lagged behind network events");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        debug!("Network event channel closed");
                        break;
                    }
                }
            }
        }
    }

    info!("WebSocket client disconnected");
}

async fn handle_websocket_message(
    app_state: &AppState,
    sender: &mut futures_util::stream::SplitSink<WebSocket, axum::extract::ws::Message>,
    payload: Value,
) {
    let action = payload["action"].as_str().unwrap_or("");

    match action {
        "create_space" => {
            let node = app_state.node.read().await;
            let dir = payload["dir"].as_str().unwrap_or("/tmp/space");

            match node.create_space(dir).await {
                Ok(_) => {
                    let response = json!({
                        "action": "space_created",
                        "status": "success"
                    });
                    let _ = sender
                        .send(axum::extract::ws::Message::Text(
                            response.to_string().into(),
                        ))
                        .await;
                }
                Err(e) => {
                    let response = json!({
                        "action": "error",
                        "message": e.to_string(),
                        "status": "error"
                    });
                    let _ = sender
                        .send(axum::extract::ws::Message::Text(
                            response.to_string().into(),
                        ))
                        .await;
                }
            }
        }

        "get_network_status" => {
            let node = app_state.node.read().await;
            let running = node.is_network_running().await;

            let response = if running {
                match node.network_stats().await {
                    Ok(stats) => json!({
                        "action": "network_status",
                        "status": "success",
                        "data": {
                            "running": true,
                            "peer_id": node.peer_id(),
                            "connected_peers": stats.connected_peers,
                            "known_peers": stats.known_peers,
                            "uptime_secs": stats.uptime_secs
                        }
                    }),
                    Err(e) => json!({
                        "action": "error",
                        "message": e.to_string(),
                        "status": "error"
                    }),
                }
            } else {
                json!({
                    "action": "network_status",
                    "status": "success",
                    "data": {
                        "running": false,
                        "peer_id": node.peer_id()
                    }
                })
            };

            let _ = sender
                .send(axum::extract::ws::Message::Text(
                    response.to_string().into(),
                ))
                .await;
        }

        _ => {
            let response = json!({
                "action": "error",
                "message": "Unknown action",
                "status": "error"
            });
            let _ = sender
                .send(axum::extract::ws::Message::Text(
                    response.to_string().into(),
                ))
                .await;
        }
    }
}
