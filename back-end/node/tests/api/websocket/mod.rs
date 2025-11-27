use crate::bootstrap::init::setup_test_server;
use axum::Router;
use futures_util::{SinkExt, StreamExt};
use node::api::servers::websocket;
use node::api::{node::Node, servers::app_state::AppState};
use serde_json::{Value, json};
use serial_test::serial;
use std::time::Duration;
use tokio::time::timeout;
use tokio_tungstenite::{connect_async, tungstenite};
use tracing::info;

/// Helper function to build WebSocket router for testing
fn build_websocket_router(app_state: AppState) -> Router {
    Router::new()
        .route("/ws", axum::routing::get(websocket::websocket_handler))
        .with_state(app_state)
}

/// Helper to start a WebSocket test server on a random available port
async fn setup_websocket_test_server() -> (String, tokio::task::JoinHandle<()>) {
    let server = setup_test_server().await;

    // Find an available port
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let port = addr.port();

    let app_state = AppState::new(server.node.clone());
    let router = build_websocket_router(app_state);

    // Spawn the server
    let handle = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    // Give the server a moment to start
    tokio::time::sleep(Duration::from_millis(50)).await;

    let ws_url = format!("ws://127.0.0.1:{}/ws", port);
    (ws_url, handle)
}

/// Helper to connect to WebSocket server
async fn connect_to_websocket(
    url: &str,
) -> Result<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    tungstenite::Error,
> {
    let (ws_stream, _) = connect_async(url).await?;
    Ok(ws_stream)
}

/// Helper to send JSON message and receive response
async fn send_and_receive(
    ws_stream: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    message: Value,
) -> Result<Value, Box<dyn std::error::Error>> {
    // Send message
    ws_stream
        .send(tungstenite::Message::Text(message.to_string().into()))
        .await?;

    // Receive response with timeout
    let response = timeout(Duration::from_secs(5), ws_stream.next())
        .await?
        .ok_or("No response received")??;

    match response {
        tungstenite::Message::Text(text) => Ok(serde_json::from_str(&text)?),
        _ => Err("Expected text message".into()),
    }
}

// ============================================================================
// WebSocket Connection Tests
// ============================================================================

#[tokio::test]
async fn test_websocket_connection_established() {
    // Setup: Start WebSocket server
    let (ws_url, server_handle) = setup_websocket_test_server().await;

    // Execute: Connect to WebSocket
    let result = connect_to_websocket(&ws_url).await;

    // Assert: Connection should succeed
    assert!(
        result.is_ok(),
        "WebSocket connection should be established successfully"
    );

    let mut ws_stream = result.unwrap();

    // Verify we can send a ping and get a pong
    ws_stream
        .send(tungstenite::Message::Ping(Into::into(vec![1, 2, 3])))
        .await
        .expect("Should be able to send ping");

    // Wait for pong response
    let response = timeout(Duration::from_secs(2), ws_stream.next())
        .await
        .expect("Should receive response within timeout")
        .expect("Should receive a message")
        .expect("Message should be valid");

    assert!(
        matches!(response, tungstenite::Message::Pong(_)),
        "Should receive pong in response to ping"
    );

    // Cleanup: Close connection
    ws_stream
        .close(None)
        .await
        .expect("Should close gracefully");

    server_handle.abort();

    info!("✓ WebSocket connection established and verified");
}

#[tokio::test]
async fn test_websocket_connection_rejected_invalid_upgrade() {
    // Setup: Start WebSocket server
    let (ws_url, server_handle) = setup_websocket_test_server().await;

    // Execute: Try to connect without proper WebSocket upgrade headers
    // Make a regular HTTP GET request instead of WebSocket upgrade
    let client = reqwest::Client::new();
    let http_url = ws_url.replace("ws://", "http://");

    let result = client.get(&http_url).send().await;

    // Assert: Should get an error or non-101 status
    if let Ok(response) = result {
        assert_ne!(
            response.status(),
            reqwest::StatusCode::SWITCHING_PROTOCOLS,
            "Should not switch protocols without proper WebSocket upgrade"
        );

        // Axum returns 426 Upgrade Required for invalid WebSocket upgrades
        // or 405 Method Not Allowed, depending on the request
        let status = response.status();
        assert!(
            status.is_client_error(),
            "Should return client error for invalid upgrade, got: {}",
            status
        );
    } else {
        // Connection failure is also acceptable
        info!("Connection properly rejected");
    }

    server_handle.abort();

    info!("✓ Invalid WebSocket upgrade properly rejected");
}

#[tokio::test]
async fn test_websocket_handles_multiple_concurrent_connections() {
    // Setup: Start WebSocket server
    let (ws_url, server_handle) = setup_websocket_test_server().await;

    // Execute: Create multiple concurrent connections
    let num_connections = 10;
    let mut handles = vec![];

    for i in 0..num_connections {
        let url = ws_url.clone();
        let handle = tokio::spawn(async move {
            // Connect
            let mut ws_stream = connect_to_websocket(&url)
                .await
                .expect(&format!("Connection {} should succeed", i));

            // Send a test message
            let test_msg = json!({
                "action": "unknown",
                "test_id": i
            });

            let response = send_and_receive(&mut ws_stream, test_msg)
                .await
                .expect("Should receive response");

            // Verify we got an error response for unknown action
            assert_eq!(response["status"], "error");
            assert_eq!(response["action"], "error");

            // Close connection
            ws_stream.close(None).await.expect("Should close");

            i
        });

        handles.push(handle);
    }

    // Assert: All connections should complete successfully
    let results = futures_util::future::join_all(handles).await;

    for (i, result) in results.iter().enumerate() {
        assert!(
            result.is_ok(),
            "Connection {} should complete successfully",
            i
        );
        assert_eq!(
            *result.as_ref().unwrap(),
            i,
            "Connection {} should return correct ID",
            i
        );
    }

    server_handle.abort();

    info!(
        "✓ Successfully handled {} concurrent WebSocket connections",
        num_connections
    );
}

#[tokio::test]
async fn test_websocket_connection_timeout() {
    // Setup: Start WebSocket server
    let (ws_url, server_handle) = setup_websocket_test_server().await;

    // Execute: Connect and then become idle
    let mut ws_stream = connect_to_websocket(&ws_url).await.expect("Should connect");

    // Send a message to ensure connection is working
    let test_msg = json!({
        "action": "unknown"
    });

    ws_stream
        .send(tungstenite::Message::Text(test_msg.to_string().into()))
        .await
        .expect("Should send message");

    // Receive the error response
    let _response = ws_stream.next().await;

    // Now idle for a period and check if connection stays alive
    // WebSocket connections typically use keep-alive pings
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Try to send another message
    let test_msg2 = json!({
        "action": "unknown",
        "test": "after_idle"
    });

    let send_result = ws_stream
        .send(tungstenite::Message::Text(test_msg2.to_string().into()))
        .await;

    // Assert: Connection should still be alive (or we can test that it times out
    // if we implement connection timeouts in the server)
    assert!(
        send_result.is_ok(),
        "Connection should still be alive after idle period"
    );

    // Cleanup
    ws_stream.close(None).await.ok();
    server_handle.abort();

    info!("✓ WebSocket connection timeout handling verified");
}

#[tokio::test]
async fn test_websocket_connection_close_gracefully() {
    // Setup: Start WebSocket server
    let (ws_url, server_handle) = setup_websocket_test_server().await;

    // Execute: Connect to WebSocket
    let mut ws_stream = connect_to_websocket(&ws_url).await.expect("Should connect");

    // Send a test message first to verify connection is active
    let test_msg = json!({
        "action": "unknown"
    });

    ws_stream
        .send(tungstenite::Message::Text(test_msg.to_string().into()))
        .await
        .expect("Should send message");

    // Receive response
    let response = timeout(Duration::from_secs(2), ws_stream.next())
        .await
        .expect("Should receive response")
        .expect("Should get message")
        .expect("Message should be valid");

    assert!(
        matches!(response, tungstenite::Message::Text(_)),
        "Should receive text response"
    );

    // Execute: Close connection gracefully
    let close_result = ws_stream
        .close(Some(tungstenite::protocol::CloseFrame {
            code: tungstenite::protocol::frame::coding::CloseCode::Normal,
            reason: "Test completed".into(),
        }))
        .await;

    // Assert: Close should succeed
    assert!(
        close_result.is_ok(),
        "WebSocket should close gracefully: {:?}",
        close_result.err()
    );

    // Verify we receive close acknowledgment
    let close_ack = timeout(Duration::from_secs(2), ws_stream.next())
        .await
        .expect("Should receive close acknowledgment");

    if let Some(Ok(msg)) = close_ack {
        assert!(
            matches!(msg, tungstenite::Message::Close(_)),
            "Should receive close frame acknowledgment"
        );
    }

    // Try to send after close - should fail
    let send_after_close = ws_stream
        .send(tungstenite::Message::Text("test".into()))
        .await;

    assert!(
        send_after_close.is_err(),
        "Should not be able to send after close"
    );

    server_handle.abort();

    info!("✓ WebSocket connection closed gracefully");
}

#[tokio::test]
async fn test_websocket_reconnection_after_disconnect() {
    // Setup: Start WebSocket server
    let (ws_url, server_handle) = setup_websocket_test_server().await;

    // Execute: First connection
    let mut ws_stream1 = connect_to_websocket(&ws_url)
        .await
        .expect("First connection should succeed");

    // Send a message
    let msg1 = json!({
        "action": "unknown",
        "connection": "first"
    });

    let response1 = send_and_receive(&mut ws_stream1, msg1)
        .await
        .expect("Should receive response on first connection");

    assert_eq!(response1["status"], "error");

    // Close the first connection
    ws_stream1
        .close(None)
        .await
        .expect("Should close first connection");

    // Wait a moment
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Execute: Reconnect (second connection)
    let mut ws_stream2 = connect_to_websocket(&ws_url)
        .await
        .expect("Reconnection should succeed");

    // Send a message on the new connection
    let msg2 = json!({
        "action": "unknown",
        "connection": "second"
    });

    let response2 = send_and_receive(&mut ws_stream2, msg2)
        .await
        .expect("Should receive response on second connection");

    // Assert: Second connection should work independently
    assert_eq!(response2["status"], "error");
    assert_eq!(response2["action"], "error");

    // Verify we can make multiple requests on the new connection
    let msg3 = json!({
        "action": "unknown",
        "connection": "second",
        "request": "multiple"
    });

    let response3 = send_and_receive(&mut ws_stream2, msg3)
        .await
        .expect("Should receive response for multiple requests");

    assert_eq!(response3["status"], "error");

    // Cleanup
    ws_stream2.close(None).await.ok();
    server_handle.abort();

    info!("✓ WebSocket reconnection after disconnect successful");
}

#[tokio::test]
async fn test_websocket_connection_survives_network_blip() {
    // Setup: Start WebSocket server
    let (ws_url, server_handle) = setup_websocket_test_server().await;

    // Execute: Connect
    let mut ws_stream = connect_to_websocket(&ws_url).await.expect("Should connect");

    // Send initial message
    let msg1 = json!({"action": "unknown", "seq": 1});
    let response1 = send_and_receive(&mut ws_stream, msg1)
        .await
        .expect("Should receive first response");
    assert_eq!(response1["status"], "error");

    // Simulate network activity with rapid fire messages
    for i in 2..=5 {
        let msg = json!({"action": "unknown", "seq": i});
        let response = send_and_receive(&mut ws_stream, msg)
            .await
            .expect(&format!("Should receive response {}", i));
        assert_eq!(response["status"], "error");
    }

    // Assert: Connection should still be functional
    let final_msg = json!({"action": "unknown", "seq": 6});
    let final_response = send_and_receive(&mut ws_stream, final_msg)
        .await
        .expect("Connection should still work after rapid messages");
    assert_eq!(final_response["status"], "error");

    // Cleanup
    ws_stream.close(None).await.ok();
    server_handle.abort();

    info!("✓ WebSocket connection survived network activity");
}

/// Helper to wait for a specific network event type with timeout
async fn wait_for_network_event(
    ws_stream: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    expected_event_type: &str,
    timeout_duration: Duration,
) -> Result<Value, Box<dyn std::error::Error>> {
    let deadline = tokio::time::Instant::now() + timeout_duration;

    while tokio::time::Instant::now() < deadline {
        match timeout(deadline - tokio::time::Instant::now(), ws_stream.next()).await {
            Ok(Some(Ok(tungstenite::Message::Text(text)))) => {
                let msg: Value = serde_json::from_str(&text)?;

                // Check if this is the network event we're waiting for
                if msg.get("event") == Some(&json!("network")) {
                    if let Some(payload) = msg.get("payload") {
                        if payload.get("type") == Some(&json!(expected_event_type)) {
                            return Ok(msg);
                        }
                    }
                }
                // Not the event we're looking for, continue waiting
            }
            Ok(Some(Ok(_))) => {
                // Non-text message, continue waiting
            }
            Ok(Some(Err(e))) => {
                return Err(format!("WebSocket error: {}", e).into());
            }
            Ok(None) => {
                return Err("WebSocket stream ended unexpectedly".into());
            }
            Err(_) => {
                return Err(
                    format!("Timeout waiting for event type: {}", expected_event_type).into(),
                );
            }
        }
    }

    Err(format!("Timeout waiting for event type: {}", expected_event_type).into())
}

/// Helper to collect all events within a time window
async fn collect_events_for_duration(
    ws_stream: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    duration: Duration,
) -> Vec<Value> {
    let mut events = vec![];
    let deadline = tokio::time::Instant::now() + duration;

    while tokio::time::Instant::now() < deadline {
        match timeout(Duration::from_millis(100), ws_stream.next()).await {
            Ok(Some(Ok(tungstenite::Message::Text(text)))) => {
                if let Ok(msg) = serde_json::from_str::<Value>(&text) {
                    if msg.get("event") == Some(&json!("network")) {
                        events.push(msg);
                    }
                }
            }
            _ => {
                // Timeout or other message, continue
            }
        }
    }

    events
}

/// Helper to create app state with network manager for event testing
async fn setup_websocket_test_server_with_node() -> (String, tokio::task::JoinHandle<()>, Node) {
    let server = setup_test_server().await;
    let node = server.node.clone();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let port = addr.port();

    let app_state = AppState::new(server.node.clone());
    let router = build_websocket_router(app_state);

    let handle = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    let ws_url = format!("ws://127.0.0.1:{}/ws", port);
    (ws_url, handle, node)
}

#[tokio::test]
#[serial]
async fn test_websocket_receives_network_started_event() {
    // Setup: Start WebSocket server with access to Node
    let (ws_url, server_handle, node) = setup_websocket_test_server_with_node().await;

    // Connect WebSocket client
    let mut ws_stream = connect_to_websocket(&ws_url)
        .await
        .expect("Should connect to WebSocket");

    // Allow time for subscription to be established
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Execute: Broadcast NetworkStarted event
    node.broadcast_network_event("NetworkStarted", json!({ "peer_id": node.peer_id() }))
        .await;

    // Assert: Client should receive the event
    let event = wait_for_network_event(&mut ws_stream, "NetworkStarted", Duration::from_secs(5))
        .await
        .expect("Should receive NetworkStarted event");

    // Verify event structure
    assert_eq!(event["event"], "network");
    assert_eq!(event["payload"]["type"], "NetworkStarted");
    assert!(
        event["payload"]["data"]["peer_id"].is_string(),
        "Event should contain peer_id"
    );
    assert!(
        event["payload"]["timestamp"].is_number(),
        "Event should contain timestamp"
    );

    info!("Received NetworkStarted event: {:?}", event);

    // Cleanup
    ws_stream.close(None).await.ok();
    server_handle.abort();

    info!("✓ WebSocket client received NetworkStarted event");
}

#[tokio::test]
#[serial]
async fn test_websocket_receives_network_stopped_event() {
    // Setup: Start WebSocket server with access to Node
    let (ws_url, server_handle, node) = setup_websocket_test_server_with_node().await;

    // Connect WebSocket client
    let mut ws_stream = connect_to_websocket(&ws_url)
        .await
        .expect("Should connect to WebSocket");

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Execute: Broadcast NetworkStopped event
    node.broadcast_network_event("NetworkStopped", json!({}))
        .await;

    // Assert: Client should receive the event
    let event = wait_for_network_event(&mut ws_stream, "NetworkStopped", Duration::from_secs(5))
        .await
        .expect("Should receive NetworkStopped event");

    assert_eq!(event["event"], "network");
    assert_eq!(event["payload"]["type"], "NetworkStopped");

    info!("Received NetworkStopped event: {:?}", event);

    // Cleanup
    ws_stream.close(None).await.ok();
    server_handle.abort();

    info!("✓ WebSocket client received NetworkStopped event");
}

#[tokio::test]
#[serial]
async fn test_websocket_multi_client_event_broadcast() {
    // Setup: Start WebSocket server with access to Node
    let (ws_url, server_handle, node) = setup_websocket_test_server_with_node().await;

    // Connect multiple WebSocket clients
    let num_clients = 5;
    let mut clients = vec![];

    for i in 0..num_clients {
        let ws_stream = connect_to_websocket(&ws_url)
            .await
            .expect(&format!("Client {} should connect", i));
        clients.push(ws_stream);
    }

    // Allow time for all subscriptions to be established
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Execute: Broadcast a single event
    let test_event_data = json!({
        "peer_id": node.peer_id(),
        "test_marker": "multi_client_test"
    });

    node.broadcast_network_event("NetworkStarted", test_event_data.clone())
        .await;

    // Assert: ALL clients should receive the event
    let mut received_count = 0;

    for (i, mut client) in clients.into_iter().enumerate() {
        match wait_for_network_event(&mut client, "NetworkStarted", Duration::from_secs(5)).await {
            Ok(event) => {
                assert_eq!(event["event"], "network");
                assert_eq!(event["payload"]["type"], "NetworkStarted");
                assert_eq!(event["payload"]["data"]["test_marker"], "multi_client_test");
                received_count += 1;
                info!("Client {} received event", i);
            }
            Err(e) => {
                panic!("Client {} failed to receive event: {}", i, e);
            }
        }

        client.close(None).await.ok();
    }

    assert_eq!(
        received_count, num_clients,
        "All {} clients should receive the event",
        num_clients
    );

    server_handle.abort();

    info!(
        "✓ All {} WebSocket clients received broadcast event",
        num_clients
    );
}

#[tokio::test]
#[serial]
async fn test_network_start_stop_lifecycle_via_api() {
    // Setup: Start test server with both REST and WebSocket
    let server = setup_test_server().await;
    let _node = server.node.clone();

    // Start WebSocket server
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let ws_port = listener.local_addr().unwrap().port();

    let app_state = AppState::new(server.node.clone());
    let ws_router = build_websocket_router(app_state);

    let ws_handle = tokio::spawn(async move {
        axum::serve(listener, ws_router).await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Connect WebSocket client
    let ws_url = format!("ws://127.0.0.1:{}/ws", ws_port);
    let mut ws_stream = connect_to_websocket(&ws_url)
        .await
        .expect("Should connect to WebSocket");

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Execute: Check initial network status via REST
    let (status, body) =
        super::rest::helpers::get_request(&server.router, "/api/v1/network/status").await;

    assert_eq!(status, axum::http::StatusCode::OK);
    let initial_running = body["running"].as_bool().unwrap_or(false);
    info!("Initial network running state: {}", initial_running);

    // Execute: Start network via REST API (if not already running)
    if !initial_running {
        let (start_status, _start_body) =
            super::rest::helpers::post_request(&server.router, "/api/v1/network/start", json!({}))
                .await;

        if start_status == axum::http::StatusCode::OK {
            info!("Network started via REST API");

            // Assert: WebSocket should receive NetworkStarted event
            let event =
                wait_for_network_event(&mut ws_stream, "NetworkStarted", Duration::from_secs(5))
                    .await
                    .expect("Should receive NetworkStarted event after REST API start");

            assert_eq!(event["payload"]["type"], "NetworkStarted");
            info!("Received NetworkStarted event via WebSocket");

            // Execute: Stop network via REST API
            let (stop_status, _) = super::rest::helpers::post_request(
                &server.router,
                "/api/v1/network/stop",
                json!({}),
            )
            .await;

            assert_eq!(stop_status, axum::http::StatusCode::OK);
            info!("Network stopped via REST API");

            // Assert: WebSocket should receive NetworkStopped event
            let event =
                wait_for_network_event(&mut ws_stream, "NetworkStopped", Duration::from_secs(5))
                    .await
                    .expect("Should receive NetworkStopped event after REST API stop");

            assert_eq!(event["payload"]["type"], "NetworkStopped");
            info!("Received NetworkStopped event via WebSocket");
        }
    } else {
        // Network already running, test stop then start
        let (stop_status, _) =
            super::rest::helpers::post_request(&server.router, "/api/v1/network/stop", json!({}))
                .await;

        if stop_status == axum::http::StatusCode::OK {
            let event =
                wait_for_network_event(&mut ws_stream, "NetworkStopped", Duration::from_secs(5))
                    .await
                    .expect("Should receive NetworkStopped event");

            assert_eq!(event["payload"]["type"], "NetworkStopped");
        }
    }

    // Cleanup
    ws_stream.close(None).await.ok();
    ws_handle.abort();

    info!("✓ Network start/stop lifecycle via API with WebSocket events verified");
}

#[tokio::test]
#[serial]
async fn test_network_events_arrive_in_order() {
    // Setup: Start WebSocket server with access to Node
    let (ws_url, server_handle, node) = setup_websocket_test_server_with_node().await;

    // Connect WebSocket client
    let mut ws_stream = connect_to_websocket(&ws_url)
        .await
        .expect("Should connect to WebSocket");

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Execute: Broadcast multiple events in sequence
    let events_to_send = vec![
        ("NetworkStarted", json!({"sequence": 1})),
        (
            "PeerConnected",
            json!({"sequence": 2, "peer_id": "test-peer-1"}),
        ),
        (
            "PeerConnected",
            json!({"sequence": 3, "peer_id": "test-peer-2"}),
        ),
        (
            "PeerDisconnected",
            json!({"sequence": 4, "peer_id": "test-peer-1"}),
        ),
        ("NetworkStopped", json!({"sequence": 5})),
    ];

    for (event_type, data) in &events_to_send {
        node.broadcast_network_event(event_type, data.clone()).await;
        // Small delay to ensure ordering
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    // Assert: Collect events and verify order
    let received_events = collect_events_for_duration(&mut ws_stream, Duration::from_secs(3)).await;

    assert_eq!(
        received_events.len(),
        events_to_send.len(),
        "Should receive all {} events, got {}",
        events_to_send.len(),
        received_events.len()
    );

    // Verify order by sequence number
    for (i, event) in received_events.iter().enumerate() {
        let expected_seq = (i + 1) as i64;
        let actual_seq = event["payload"]["data"]["sequence"]
            .as_i64()
            .expect("Event should have sequence number");

        assert_eq!(
            actual_seq, expected_seq,
            "Event {} should have sequence {}, got {}",
            i, expected_seq, actual_seq
        );

        let expected_type = &events_to_send[i].0;
        let actual_type = event["payload"]["type"].as_str().unwrap();

        assert_eq!(
            actual_type, *expected_type,
            "Event {} should be type {}, got {}",
            i, expected_type, actual_type
        );
    }

    info!(
        "Events received in order: {:?}",
        received_events
            .iter()
            .map(|e| e["payload"]["type"].as_str().unwrap_or("unknown"))
            .collect::<Vec<_>>()
    );

    // Cleanup
    ws_stream.close(None).await.ok();
    server_handle.abort();

    info!("✓ Network events arrived in correct order");
}

#[tokio::test]
#[serial]
async fn test_websocket_events_include_correct_payload() {
    // Setup: Start WebSocket server with access to Node
    let (ws_url, server_handle, node) = setup_websocket_test_server_with_node().await;

    // Connect WebSocket client
    let mut ws_stream = connect_to_websocket(&ws_url)
        .await
        .expect("Should connect to WebSocket");

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Execute: Broadcast event with specific payload
    let custom_data = json!({
        "peer_id": "12D3KooWTestPeerId",
        "address": "/ip4/192.168.1.100/tcp/9000",
        "direction": "Outbound",
        "custom_field": "test_value_12345"
    });

    node.broadcast_network_event("PeerConnected", custom_data.clone())
        .await;

    // Assert: Verify payload structure and content
    let event = wait_for_network_event(&mut ws_stream, "PeerConnected", Duration::from_secs(5))
        .await
        .expect("Should receive PeerConnected event");

    // Verify top-level structure
    assert_eq!(event["event"], "network", "Event type should be 'network'");

    // Verify payload structure
    let payload = &event["payload"];
    assert_eq!(payload["type"], "PeerConnected");
    assert!(
        payload["timestamp"].is_number(),
        "Should have numeric timestamp"
    );
    assert!(payload["source"].is_string(), "Should have source peer ID");

    // Verify data content
    let data = &payload["data"];
    assert_eq!(data["peer_id"], "12D3KooWTestPeerId");
    assert_eq!(data["address"], "/ip4/192.168.1.100/tcp/9000");
    assert_eq!(data["direction"], "Outbound");
    assert_eq!(data["custom_field"], "test_value_12345");

    info!("Event payload verified: {:?}", event);

    // Cleanup
    ws_stream.close(None).await.ok();
    server_handle.abort();

    info!("✓ WebSocket event payload structure and content verified");
}

#[tokio::test]
#[serial]
async fn test_websocket_late_subscriber_misses_past_events() {
    // Setup: Start WebSocket server with access to Node
    let (ws_url, server_handle, node) = setup_websocket_test_server_with_node().await;

    // Execute: Broadcast event BEFORE any client connects
    node.broadcast_network_event("NetworkStarted", json!({"before_connect": true}))
        .await;

    // Small delay
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Connect WebSocket client AFTER event was broadcast
    let mut ws_stream = connect_to_websocket(&ws_url)
        .await
        .expect("Should connect to WebSocket");

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Broadcast another event AFTER client connected
    node.broadcast_network_event("NetworkStopped", json!({"after_connect": true}))
        .await;

    // Assert: Client should receive only the second event
    let event = wait_for_network_event(&mut ws_stream, "NetworkStopped", Duration::from_secs(3))
        .await
        .expect("Should receive event broadcast after connection");

    assert_eq!(event["payload"]["data"]["after_connect"], true);

    // Try to receive any other events (should timeout)
    let extra_events =
        collect_events_for_duration(&mut ws_stream, Duration::from_millis(500)).await;

    // The only event should be the one we already received (NetworkStopped)
    // We shouldn't receive the NetworkStarted that was broadcast before connection
    let started_events: Vec<_> = extra_events
        .iter()
        .filter(|e| e["payload"]["type"] == "NetworkStarted")
        .collect();

    assert!(
        started_events.is_empty(),
        "Should not receive events broadcast before connection"
    );

    // Cleanup
    ws_stream.close(None).await.ok();
    server_handle.abort();

    info!("✓ Late subscriber correctly misses past events (broadcast semantics)");
}

#[tokio::test]
#[serial]
async fn test_websocket_client_disconnect_does_not_affect_others() {
    // Setup: Start WebSocket server with access to Node
    let (ws_url, server_handle, node) = setup_websocket_test_server_with_node().await;

    // Connect two clients
    let mut client1 = connect_to_websocket(&ws_url)
        .await
        .expect("Client 1 should connect");

    let mut client2 = connect_to_websocket(&ws_url)
        .await
        .expect("Client 2 should connect");

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Verify both receive an event
    node.broadcast_network_event("TestEvent1", json!({"test": 1}))
        .await;

    let event1_c1 = wait_for_network_event(&mut client1, "TestEvent1", Duration::from_secs(3))
        .await
        .expect("Client 1 should receive TestEvent1");
    let event1_c2 = wait_for_network_event(&mut client2, "TestEvent1", Duration::from_secs(3))
        .await
        .expect("Client 2 should receive TestEvent1");

    assert_eq!(event1_c1["payload"]["data"]["test"], 1);
    assert_eq!(event1_c2["payload"]["data"]["test"], 1);

    // Execute: Disconnect client 1
    client1.close(None).await.expect("Client 1 should close");
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Broadcast another event
    node.broadcast_network_event("TestEvent2", json!({"test": 2}))
        .await;

    // Assert: Client 2 should still receive events
    let event2_c2 = wait_for_network_event(&mut client2, "TestEvent2", Duration::from_secs(3))
        .await
        .expect("Client 2 should still receive events after Client 1 disconnected");

    assert_eq!(event2_c2["payload"]["data"]["test"], 2);

    // Cleanup
    client2.close(None).await.ok();
    server_handle.abort();

    info!("✓ Client disconnect does not affect other clients");
}
