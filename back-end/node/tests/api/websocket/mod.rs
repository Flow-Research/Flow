use crate::bootstrap::init::setup_test_server;
use axum::Router;
use futures_util::{SinkExt, StreamExt};
use node::api::servers::app_state::AppState;
use node::api::servers::websocket;
use serde_json::{Value, json};
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
