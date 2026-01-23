//! Codec implementation for content transfer protocol.
//!
//! This module implements the libp2p `request_response::Codec` trait
//! for serializing and deserializing content transfer messages.
//!
//! # Framing
//!
//! Messages are framed with a 4-byte big-endian length prefix:
//! ```text
//! +--------------------------+------------------+
//! | Length (4 bytes, BE)     | Payload (CBOR)   |
//! +--------------------------+------------------+
//! ```

use super::messages::{ContentRequest, ContentResponse, MAX_MESSAGE_SIZE};
use async_trait::async_trait;
use futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use libp2p::StreamProtocol;
use libp2p::request_response::Codec;
use std::io;
use tracing::{debug, error, trace, warn};

// ============================================================================
// Protocol
// ============================================================================

/// The protocol identifier for content transfer.
///
/// libp2p uses this to negotiate which protocol to use when connecting.
#[derive(Debug, Clone, Default)]
pub struct ContentProtocol;

impl ContentProtocol {
    /// The protocol string used for libp2p negotiation.
    pub const PROTOCOL_NAME: &'static str = "/flow/content/1.0.0";

    /// Create a new ContentProtocol.
    pub fn new() -> Self {
        Self
    }

    /// Get the protocol as a StreamProtocol.
    pub fn as_stream_protocol() -> StreamProtocol {
        StreamProtocol::new(Self::PROTOCOL_NAME)
    }
}

impl AsRef<str> for ContentProtocol {
    fn as_ref(&self) -> &str {
        Self::PROTOCOL_NAME
    }
}

// ============================================================================
// Codec
// ============================================================================

/// Length prefix size in bytes (u32 big-endian).
const LENGTH_PREFIX_SIZE: usize = 4;

/// Codec for content transfer messages.
///
/// Implements length-prefixed CBOR serialization over async streams.
///
/// # Example
///
/// ```ignore
/// let codec = ContentCodec::new();
/// // Used internally by libp2p request-response behaviour
/// ```
#[derive(Debug, Clone)]
pub struct ContentCodec {
    /// Maximum message size in bytes.
    max_message_size: usize,
}

impl Default for ContentCodec {
    fn default() -> Self {
        Self::new()
    }
}

impl ContentCodec {
    /// Create a new codec with default max message size.
    pub fn new() -> Self {
        Self {
            max_message_size: MAX_MESSAGE_SIZE,
        }
    }

    /// Create a codec with a custom max message size.
    pub fn with_max_size(max_message_size: usize) -> Self {
        Self { max_message_size }
    }

    /// Read a length-prefixed message from the stream.
    async fn read_length_prefixed<T>(&self, io: &mut T, context: &str) -> io::Result<Vec<u8>>
    where
        T: AsyncRead + Unpin + Send,
    {
        // Read 4-byte length prefix
        let mut length_buf = [0u8; LENGTH_PREFIX_SIZE];
        io.read_exact(&mut length_buf).await.map_err(|e| {
            if e.kind() == io::ErrorKind::UnexpectedEof {
                debug!(
                    context = context,
                    "Connection closed while reading length prefix"
                );
            } else {
                warn!(context = context, error = %e, "Failed to read length prefix");
            }
            e
        })?;

        let length = u32::from_be_bytes(length_buf) as usize;
        trace!(context = context, length = length, "Read length prefix");

        // Validate length
        if length > self.max_message_size {
            error!(
                context = context,
                length = length,
                max = self.max_message_size,
                "Message exceeds maximum size"
            );
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "Message too large: {} bytes (max: {})",
                    length, self.max_message_size
                ),
            ));
        }

        if length == 0 {
            warn!(context = context, "Received zero-length message");
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Empty message not allowed",
            ));
        }

        // Read payload
        let mut payload = vec![0u8; length];
        io.read_exact(&mut payload).await.map_err(|e| {
            warn!(
                context = context,
                error = %e,
                expected = length,
                "Failed to read message payload"
            );
            e
        })?;

        trace!(
            context = context,
            bytes = payload.len(),
            "Read message payload"
        );

        Ok(payload)
    }

    /// Write a length-prefixed message to the stream.
    async fn write_length_prefixed<T>(
        &self,
        io: &mut T,
        payload: &[u8],
        context: &str,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        let length = payload.len();

        // Validate length
        if length > self.max_message_size {
            error!(
                context = context,
                length = length,
                max = self.max_message_size,
                "Message exceeds maximum size"
            );
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "Message too large: {} bytes (max: {})",
                    length, self.max_message_size
                ),
            ));
        }

        // Write length prefix
        let length_buf = (length as u32).to_be_bytes();
        io.write_all(&length_buf).await.map_err(|e| {
            warn!(context = context, error = %e, "Failed to write length prefix");
            e
        })?;

        // Write payload
        io.write_all(payload).await.map_err(|e| {
            warn!(context = context, error = %e, "Failed to write payload");
            e
        })?;

        // Flush to ensure data is sent
        io.flush().await.map_err(|e| {
            warn!(context = context, error = %e, "Failed to flush stream");
            e
        })?;

        trace!(
            context = context,
            bytes = length,
            "Wrote length-prefixed message"
        );

        Ok(())
    }
}

#[async_trait]
impl Codec for ContentCodec {
    type Protocol = StreamProtocol;
    type Request = ContentRequest;
    type Response = ContentResponse;

    /// Read a request from the stream.
    async fn read_request<T>(
        &mut self,
        _protocol: &StreamProtocol,
        io: &mut T,
    ) -> io::Result<Self::Request>
    where
        T: AsyncRead + Unpin + Send,
    {
        let payload = self.read_length_prefixed(io, "read_request").await?;

        ContentRequest::from_bytes(&payload).map_err(|e| {
            warn!(error = %e, "Failed to deserialize request");
            io::Error::new(io::ErrorKind::InvalidData, e.to_string())
        })
    }

    /// Read a response from the stream.
    async fn read_response<T>(
        &mut self,
        _protocol: &StreamProtocol,
        io: &mut T,
    ) -> io::Result<Self::Response>
    where
        T: AsyncRead + Unpin + Send,
    {
        let payload = self.read_length_prefixed(io, "read_response").await?;

        ContentResponse::from_bytes(&payload).map_err(|e| {
            warn!(error = %e, "Failed to deserialize response");
            io::Error::new(io::ErrorKind::InvalidData, e.to_string())
        })
    }

    /// Write a request to the stream.
    async fn write_request<T>(
        &mut self,
        _protocol: &StreamProtocol,
        io: &mut T,
        req: Self::Request,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        debug!(request = ?req.cid(), "Writing request");

        let payload = req.to_bytes().map_err(|e| {
            warn!(error = %e, "Failed to serialize request");
            io::Error::new(io::ErrorKind::InvalidData, e.to_string())
        })?;

        self.write_length_prefixed(io, &payload, "write_request")
            .await
    }

    /// Write a response to the stream.
    async fn write_response<T>(
        &mut self,
        _protocol: &StreamProtocol,
        io: &mut T,
        res: Self::Response,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        debug!(
            response_type = if res.is_success() {
                "success"
            } else if res.is_not_found() {
                "not_found"
            } else {
                "error"
            },
            cid = ?res.cid(),
            "Writing response"
        );

        let payload = res.to_bytes().map_err(|e| {
            warn!(error = %e, "Failed to serialize response");
            io::Error::new(io::ErrorKind::InvalidData, e.to_string())
        })?;

        self.write_length_prefixed(io, &payload, "write_response")
            .await
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::storage::content::ContentId;
    use futures::io::Cursor;

    fn sample_cid() -> ContentId {
        ContentId::from_bytes(b"test content for codec testing")
    }

    fn sample_cid_2() -> ContentId {
        ContentId::from_bytes(b"another piece of content")
    }

    // ========================================
    // Protocol Tests
    // ========================================

    #[test]
    fn test_protocol_name() {
        let protocol = ContentProtocol::new();
        assert_eq!(protocol.as_ref(), "/flow/content/1.0.0");
    }

    #[test]
    fn test_protocol_stream_protocol() {
        let stream_protocol = ContentProtocol::as_stream_protocol();
        assert_eq!(stream_protocol.as_ref(), "/flow/content/1.0.0");
    }

    // ========================================
    // Codec Request Tests
    // ========================================

    #[tokio::test]
    async fn test_request_roundtrip_get_block() {
        let mut codec = ContentCodec::new();
        let protocol = ContentProtocol::as_stream_protocol();

        let request = ContentRequest::get_block(sample_cid());

        // Write to buffer
        let mut buffer = Vec::new();
        codec
            .write_request(&protocol, &mut buffer, request.clone())
            .await
            .unwrap();

        // Read back
        let mut cursor = Cursor::new(buffer);
        let restored = codec.read_request(&protocol, &mut cursor).await.unwrap();

        assert_eq!(request, restored);
    }

    #[tokio::test]
    async fn test_request_roundtrip_get_manifest() {
        let mut codec = ContentCodec::new();
        let protocol = ContentProtocol::as_stream_protocol();

        let request = ContentRequest::get_manifest(sample_cid());

        // Write to buffer
        let mut buffer = Vec::new();
        codec
            .write_request(&protocol, &mut buffer, request.clone())
            .await
            .unwrap();

        // Read back
        let mut cursor = Cursor::new(buffer);
        let restored = codec.read_request(&protocol, &mut cursor).await.unwrap();

        assert_eq!(request, restored);
    }

    // ========================================
    // Codec Response Tests
    // ========================================

    #[tokio::test]
    async fn test_response_roundtrip_block() {
        let mut codec = ContentCodec::new();
        let protocol = ContentProtocol::as_stream_protocol();

        let response = ContentResponse::block(
            sample_cid(),
            b"block data here".to_vec(),
            vec![sample_cid_2()],
        );

        // Write to buffer
        let mut buffer = Vec::new();
        codec
            .write_response(&protocol, &mut buffer, response.clone())
            .await
            .unwrap();

        // Read back
        let mut cursor = Cursor::new(buffer);
        let restored = codec.read_response(&protocol, &mut cursor).await.unwrap();

        assert_eq!(response, restored);
    }

    #[tokio::test]
    async fn test_response_roundtrip_manifest() {
        let mut codec = ContentCodec::new();
        let protocol = ContentProtocol::as_stream_protocol();

        let response = ContentResponse::manifest(sample_cid(), b"manifest cbor data".to_vec());

        // Write to buffer
        let mut buffer = Vec::new();
        codec
            .write_response(&protocol, &mut buffer, response.clone())
            .await
            .unwrap();

        // Read back
        let mut cursor = Cursor::new(buffer);
        let restored = codec.read_response(&protocol, &mut cursor).await.unwrap();

        assert_eq!(response, restored);
    }

    #[tokio::test]
    async fn test_response_roundtrip_not_found() {
        let mut codec = ContentCodec::new();
        let protocol = ContentProtocol::as_stream_protocol();

        let response = ContentResponse::not_found(sample_cid());

        // Write to buffer
        let mut buffer = Vec::new();
        codec
            .write_response(&protocol, &mut buffer, response.clone())
            .await
            .unwrap();

        // Read back
        let mut cursor = Cursor::new(buffer);
        let restored = codec.read_response(&protocol, &mut cursor).await.unwrap();

        assert_eq!(response, restored);
    }

    #[tokio::test]
    async fn test_response_roundtrip_error() {
        let mut codec = ContentCodec::new();
        let protocol = ContentProtocol::as_stream_protocol();

        let response =
            ContentResponse::error(super::super::messages::ErrorCode::RateLimited, "slow down");

        // Write to buffer
        let mut buffer = Vec::new();
        codec
            .write_response(&protocol, &mut buffer, response.clone())
            .await
            .unwrap();

        // Read back
        let mut cursor = Cursor::new(buffer);
        let restored = codec.read_response(&protocol, &mut cursor).await.unwrap();

        assert_eq!(response, restored);
    }

    // ========================================
    // Large Message Tests
    // ========================================

    #[tokio::test]
    async fn test_large_block_response() {
        let mut codec = ContentCodec::new();
        let protocol = ContentProtocol::as_stream_protocol();

        // 1 MB block
        let large_data = vec![0xABu8; 1024 * 1024];
        let links: Vec<ContentId> = (0..50)
            .map(|i| ContentId::from_bytes(format!("link_{}", i).as_bytes()))
            .collect();

        let response = ContentResponse::block(sample_cid(), large_data.clone(), links.clone());

        // Write to buffer
        let mut buffer = Vec::new();
        codec
            .write_response(&protocol, &mut buffer, response)
            .await
            .unwrap();

        // Verify length prefix is correct
        let length = u32::from_be_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]) as usize;
        assert_eq!(length, buffer.len() - 4);

        // Read back
        let mut cursor = Cursor::new(buffer);
        let restored = codec.read_response(&protocol, &mut cursor).await.unwrap();

        if let ContentResponse::Block {
            data,
            links: restored_links,
            ..
        } = restored
        {
            assert_eq!(data.len(), large_data.len());
            assert_eq!(restored_links.len(), links.len());
        } else {
            panic!("Expected Block response");
        }
    }

    // ========================================
    // Error Handling Tests
    // ========================================

    #[tokio::test]
    async fn test_message_too_large_on_write() {
        let mut codec = ContentCodec::with_max_size(100);
        let protocol = ContentProtocol::as_stream_protocol();

        // Create a response larger than 100 bytes
        let response = ContentResponse::block(sample_cid(), vec![0u8; 200], vec![]);

        let mut buffer = Vec::new();
        let result = codec.write_response(&protocol, &mut buffer, response).await;

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Message too large")
        );
    }

    #[tokio::test]
    async fn test_message_too_large_on_read() {
        let mut codec = ContentCodec::with_max_size(100);
        let protocol = ContentProtocol::as_stream_protocol();

        // Create a buffer with a length prefix indicating 200 bytes
        let mut buffer = Vec::new();
        buffer.extend_from_slice(&(200u32).to_be_bytes());
        buffer.extend_from_slice(&[0u8; 200]);

        let mut cursor = Cursor::new(buffer);
        let result = codec.read_response(&protocol, &mut cursor).await;

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Message too large")
        );
    }

    #[tokio::test]
    async fn test_empty_message_rejected() {
        let mut codec = ContentCodec::new();
        let protocol = ContentProtocol::as_stream_protocol();

        // Create a buffer with zero length prefix
        let mut buffer = Vec::new();
        buffer.extend_from_slice(&(0u32).to_be_bytes());

        let mut cursor = Cursor::new(buffer);
        let result = codec.read_request(&protocol, &mut cursor).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Empty message"));
    }

    #[tokio::test]
    async fn test_truncated_length_prefix() {
        let mut codec = ContentCodec::new();
        let protocol = ContentProtocol::as_stream_protocol();

        // Only 2 bytes instead of 4
        let buffer = vec![0u8; 2];

        let mut cursor = Cursor::new(buffer);
        let result = codec.read_request(&protocol, &mut cursor).await;

        assert!(result.is_err());
        // UnexpectedEof
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::UnexpectedEof);
    }

    #[tokio::test]
    async fn test_truncated_payload() {
        let mut codec = ContentCodec::new();
        let protocol = ContentProtocol::as_stream_protocol();

        // Length says 100, but only 50 bytes of payload
        let mut buffer = Vec::new();
        buffer.extend_from_slice(&(100u32).to_be_bytes());
        buffer.extend_from_slice(&[0u8; 50]);

        let mut cursor = Cursor::new(buffer);
        let result = codec.read_request(&protocol, &mut cursor).await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::UnexpectedEof);
    }

    #[tokio::test]
    async fn test_invalid_cbor_payload() {
        let mut codec = ContentCodec::new();
        let protocol = ContentProtocol::as_stream_protocol();

        // Valid length prefix but garbage CBOR
        let garbage = vec![0xFF, 0xFF, 0xFF, 0xFF];
        let mut buffer = Vec::new();
        buffer.extend_from_slice(&(garbage.len() as u32).to_be_bytes());
        buffer.extend_from_slice(&garbage);

        let mut cursor = Cursor::new(buffer);
        let result = codec.read_request(&protocol, &mut cursor).await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::InvalidData);
    }

    // ========================================
    // Length Prefix Format Tests
    // ========================================

    #[tokio::test]
    async fn test_length_prefix_format() {
        let mut codec = ContentCodec::new();
        let protocol = ContentProtocol::as_stream_protocol();

        let request = ContentRequest::get_block(sample_cid());

        let mut buffer = Vec::new();
        codec
            .write_request(&protocol, &mut buffer, request)
            .await
            .unwrap();

        // First 4 bytes should be big-endian length
        let length = u32::from_be_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]) as usize;

        // Length should match payload size
        assert_eq!(length, buffer.len() - 4);

        // Verify the payload is valid CBOR
        let payload = &buffer[4..];
        let _: ContentRequest = serde_cbor::from_slice(payload).unwrap();
    }

    #[tokio::test]
    async fn test_multiple_requests_same_stream() {
        let mut codec = ContentCodec::new();
        let protocol = ContentProtocol::as_stream_protocol();

        // Create multiple different requests
        let requests = vec![
            ContentRequest::get_block(sample_cid()),
            ContentRequest::get_manifest(sample_cid_2()),
            ContentRequest::get_block(ContentId::from_bytes(b"third request")),
        ];

        // Write all requests to the same buffer
        let mut buffer = Vec::new();
        for request in &requests {
            codec
                .write_request(&protocol, &mut buffer, request.clone())
                .await
                .unwrap();
        }

        // Read all back from the same stream
        let mut cursor = Cursor::new(buffer);
        for expected in &requests {
            let restored = codec.read_request(&protocol, &mut cursor).await.unwrap();
            assert_eq!(expected, &restored);
        }
    }

    // ========================================
    // Multiple responses on same stream
    // ========================================

    #[tokio::test]
    async fn test_multiple_responses_same_stream() {
        let mut codec = ContentCodec::new();
        let protocol = ContentProtocol::as_stream_protocol();

        // Create multiple different responses
        let responses = vec![
            ContentResponse::block(sample_cid(), b"data1".to_vec(), vec![]),
            ContentResponse::not_found(sample_cid_2()),
            ContentResponse::error(
                super::super::messages::ErrorCode::RateLimited,
                "rate limited",
            ),
            ContentResponse::manifest(
                ContentId::from_bytes(b"manifest"),
                b"manifest data".to_vec(),
            ),
        ];

        // Write all responses to the same buffer
        let mut buffer = Vec::new();
        for response in &responses {
            codec
                .write_response(&protocol, &mut buffer, response.clone())
                .await
                .unwrap();
        }

        // Read all back from the same stream
        let mut cursor = Cursor::new(buffer);
        for expected in &responses {
            let restored = codec.read_response(&protocol, &mut cursor).await.unwrap();
            assert_eq!(expected, &restored);
        }
    }

    // ========================================
    // Minimum size message roundtrip
    // ========================================

    #[tokio::test]
    async fn test_minimum_size_message_roundtrip() {
        let mut codec = ContentCodec::new();
        let protocol = ContentProtocol::as_stream_protocol();

        // Create smallest possible request (single byte content CID)
        let min_cid = ContentId::from_bytes(&[0u8]);
        let request = ContentRequest::get_block(min_cid);

        let mut buffer = Vec::new();
        codec
            .write_request(&protocol, &mut buffer, request.clone())
            .await
            .unwrap();

        // Verify it's relatively small (header + minimal CBOR)
        assert!(buffer.len() < 100, "Minimum message should be small");
        assert!(
            buffer.len() >= 4 + 1,
            "Must have at least length prefix + 1 byte"
        );

        let mut cursor = Cursor::new(buffer);
        let restored = codec.read_request(&protocol, &mut cursor).await.unwrap();
        assert_eq!(request, restored);
    }

    // ========================================
    // Empty block data through codec
    // ========================================

    #[tokio::test]
    async fn test_empty_block_data_through_codec() {
        let mut codec = ContentCodec::new();
        let protocol = ContentProtocol::as_stream_protocol();

        // Block with empty data and no links
        let response = ContentResponse::block(sample_cid(), vec![], vec![]);

        let mut buffer = Vec::new();
        codec
            .write_response(&protocol, &mut buffer, response.clone())
            .await
            .unwrap();

        let mut cursor = Cursor::new(buffer);
        let restored = codec.read_response(&protocol, &mut cursor).await.unwrap();

        if let ContentResponse::Block { data, links, .. } = restored {
            assert!(data.is_empty(), "Data should be empty");
            assert!(links.is_empty(), "Links should be empty");
        } else {
            panic!("Expected Block response");
        }
    }

    /// Test concurrent codec operations to ensure thread-safety and data integrity.
    /// This simulates real-world scenarios where multiple streams are processed
    /// simultaneously by different codec instances.
    #[tokio::test]
    async fn test_concurrent_codec_operations_stress() {
        use std::sync::Arc;
        use tokio::sync::Barrier;

        let protocol = ContentProtocol::as_stream_protocol();
        let num_tasks = 10;
        let iterations_per_task = 50;

        // Barrier ensures all tasks start at the same time for maximum contention
        let barrier = Arc::new(Barrier::new(num_tasks));

        let mut handles = Vec::new();

        for task_id in 0..num_tasks {
            let barrier = Arc::clone(&barrier);
            let protocol = protocol.clone();

            let handle = tokio::spawn(async move {
                // Wait for all tasks to be ready
                barrier.wait().await;

                let mut codec = ContentCodec::new();

                for i in 0..iterations_per_task {
                    // Create unique content per iteration to detect any cross-contamination
                    let unique_data = format!("task_{}_iter_{}_data", task_id, i);
                    let cid = ContentId::from_bytes(unique_data.as_bytes());

                    // Test request roundtrip
                    let request = ContentRequest::get_block(cid.clone());
                    let mut req_buffer = Vec::new();
                    codec
                        .write_request(&protocol, &mut req_buffer, request.clone())
                        .await
                        .expect("write_request failed");

                    let mut req_cursor = Cursor::new(req_buffer);
                    let restored_request = codec
                        .read_request(&protocol, &mut req_cursor)
                        .await
                        .expect("read_request failed");

                    assert_eq!(
                        request, restored_request,
                        "Request mismatch at task {} iter {}",
                        task_id, i
                    );

                    // Test response roundtrip with varying payload sizes
                    let payload_size = (i % 10 + 1) * 100; // 100 to 1000 bytes
                    let payload: Vec<u8> = (0..payload_size)
                        .map(|j| ((task_id * 17 + i + j) % 256) as u8)
                        .collect();

                    let response = ContentResponse::block(cid.clone(), payload.clone(), vec![]);
                    let mut resp_buffer = Vec::new();
                    codec
                        .write_response(&protocol, &mut resp_buffer, response.clone())
                        .await
                        .expect("write_response failed");

                    let mut resp_cursor = Cursor::new(resp_buffer);
                    let restored_response = codec
                        .read_response(&protocol, &mut resp_cursor)
                        .await
                        .expect("read_response failed");

                    // Verify the payload wasn't corrupted
                    if let ContentResponse::Block {
                        data,
                        cid: restored_cid,
                        ..
                    } = restored_response
                    {
                        assert_eq!(
                            data, payload,
                            "Payload corrupted at task {} iter {}",
                            task_id, i
                        );
                        assert_eq!(
                            restored_cid, cid,
                            "CID mismatch at task {} iter {}",
                            task_id, i
                        );
                    } else {
                        panic!("Expected Block response at task {} iter {}", task_id, i);
                    }
                }

                (task_id, iterations_per_task)
            });

            handles.push(handle);
        }

        // Wait for all tasks and verify they all completed successfully
        let results: Vec<_> = futures::future::join_all(handles)
            .await
            .into_iter()
            .map(|r| r.expect("Task panicked"))
            .collect();

        assert_eq!(results.len(), num_tasks);
        let total_ops: usize = results.iter().map(|(_, ops)| *ops).sum();
        assert_eq!(total_ops, num_tasks * iterations_per_task);
    }

    /// Test that codec state doesn't leak between sequential operations.
    /// Ensures partial failures don't corrupt subsequent operations.
    #[tokio::test]
    async fn test_codec_state_isolation_after_errors() {
        let mut codec = ContentCodec::with_max_size(500);
        let protocol = ContentProtocol::as_stream_protocol();

        // 1. Successful operation
        let request1 = ContentRequest::get_block(sample_cid());
        let mut buffer1 = Vec::new();
        codec
            .write_request(&protocol, &mut buffer1, request1.clone())
            .await
            .unwrap();

        // 2. Failed operation (message too large)
        let large_response = ContentResponse::block(sample_cid(), vec![0u8; 1000], vec![]);
        let mut buffer_fail = Vec::new();
        let result = codec
            .write_response(&protocol, &mut buffer_fail, large_response)
            .await;
        assert!(result.is_err(), "Should fail for oversized message");

        // 3. Subsequent operation should still work correctly
        let request2 = ContentRequest::get_manifest(sample_cid_2());
        let mut buffer2 = Vec::new();
        codec
            .write_request(&protocol, &mut buffer2, request2.clone())
            .await
            .unwrap();

        // 4. Both successful buffers should decode correctly
        let mut cursor1 = Cursor::new(buffer1);
        let restored1 = codec.read_request(&protocol, &mut cursor1).await.unwrap();
        assert_eq!(request1, restored1, "First request corrupted after error");

        let mut cursor2 = Cursor::new(buffer2);
        let restored2 = codec.read_request(&protocol, &mut cursor2).await.unwrap();
        assert_eq!(request2, restored2, "Request after error was corrupted");
    }

    /// Test sequential message framing - verifies multiple messages written
    /// to the same buffer can be read back correctly without boundary corruption.
    #[tokio::test]
    async fn test_sequential_message_framing() {
        let mut codec = ContentCodec::new();
        let protocol = ContentProtocol::as_stream_protocol();

        // Simulate a multiplexed stream with multiple messages concatenated
        let mut shared_buffer = Vec::new();

        let request1 = ContentRequest::get_block(sample_cid());
        let response1 = ContentResponse::block(sample_cid(), b"response_1".to_vec(), vec![]);
        let request2 = ContentRequest::get_manifest(sample_cid_2());
        let response2 = ContentResponse::not_found(sample_cid_2());

        // Write messages in interleaved order
        codec
            .write_request(&protocol, &mut shared_buffer, request1.clone())
            .await
            .unwrap();
        codec
            .write_response(&protocol, &mut shared_buffer, response1.clone())
            .await
            .unwrap();
        codec
            .write_request(&protocol, &mut shared_buffer, request2.clone())
            .await
            .unwrap();
        codec
            .write_response(&protocol, &mut shared_buffer, response2.clone())
            .await
            .unwrap();

        // Read back in the same order - cursor advances through the buffer
        let mut cursor = Cursor::new(shared_buffer);

        let restored_req1 = codec.read_request(&protocol, &mut cursor).await.unwrap();
        let restored_resp1 = codec.read_response(&protocol, &mut cursor).await.unwrap();
        let restored_req2 = codec.read_request(&protocol, &mut cursor).await.unwrap();
        let restored_resp2 = codec.read_response(&protocol, &mut cursor).await.unwrap();

        assert_eq!(request1, restored_req1);
        assert_eq!(response1, restored_resp1);
        assert_eq!(request2, restored_req2);
        assert_eq!(response2, restored_resp2);

        // Verify we consumed the entire buffer
        assert_eq!(cursor.position() as usize, cursor.get_ref().len());
    }

    // ========================================
    // Stream closed mid-read
    // ========================================

    #[tokio::test]
    async fn test_stream_closed_mid_read_after_length_prefix() {
        let mut codec = ContentCodec::new();
        let protocol = ContentProtocol::as_stream_protocol();

        // Write a valid request
        let request = ContentRequest::get_block(sample_cid());
        let mut full_buffer = Vec::new();
        codec
            .write_request(&protocol, &mut full_buffer, request)
            .await
            .unwrap();

        // Create truncated buffer: only length prefix + partial payload
        let length = u32::from_be_bytes([
            full_buffer[0],
            full_buffer[1],
            full_buffer[2],
            full_buffer[3],
        ]) as usize;

        // Include length prefix + half the payload
        let truncated_len = 4 + (length / 2);
        let truncated = full_buffer[..truncated_len].to_vec();

        let mut cursor = Cursor::new(truncated);
        let result = codec.read_request(&protocol, &mut cursor).await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::UnexpectedEof);
    }

    // ========================================
    // Stream write error propagated
    // ========================================

    use std::pin::Pin;
    use std::task::{Context, Poll};

    /// A mock writer that fails after writing N bytes
    struct FailingWriter {
        bytes_until_fail: usize,
        bytes_written: usize,
    }

    impl FailingWriter {
        fn new(bytes_until_fail: usize) -> Self {
            Self {
                bytes_until_fail,
                bytes_written: 0,
            }
        }
    }

    impl futures::io::AsyncWrite for FailingWriter {
        fn poll_write(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            if self.bytes_written >= self.bytes_until_fail {
                return Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "Simulated write failure",
                )));
            }

            let remaining = self.bytes_until_fail - self.bytes_written;
            let to_write = buf.len().min(remaining);
            self.bytes_written += to_write;

            if to_write == 0 && !buf.is_empty() {
                Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "Simulated write failure",
                )))
            } else {
                Poll::Ready(Ok(to_write))
            }
        }

        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }

        fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    #[tokio::test]
    async fn test_stream_write_error_propagated() {
        let mut codec = ContentCodec::new();
        let protocol = ContentProtocol::as_stream_protocol();

        // Fail after writing just the length prefix
        let mut failing_writer = FailingWriter::new(4);

        let request = ContentRequest::get_block(sample_cid());
        let result = codec
            .write_request(&protocol, &mut failing_writer, request)
            .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::BrokenPipe);
    }

    // ========================================
    // ContentCodec is Send + Sync
    // ========================================

    #[test]
    fn test_content_codec_is_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}

        assert_send::<ContentCodec>();
        assert_sync::<ContentCodec>();
    }

    // ========================================
    // Multiple codec instances don't interfere
    // ========================================

    #[tokio::test]
    async fn test_multiple_codec_instances_independent() {
        let protocol = ContentProtocol::as_stream_protocol();

        // Create multiple codec instances with different configurations
        let mut codec_default = ContentCodec::new();
        let mut codec_small = ContentCodec::with_max_size(1024);
        let mut codec_large = ContentCodec::with_max_size(10 * 1024 * 1024);

        let request = ContentRequest::get_block(sample_cid());

        // Each codec should work independently
        let mut buf1 = Vec::new();
        let mut buf2 = Vec::new();
        let mut buf3 = Vec::new();

        codec_default
            .write_request(&protocol, &mut buf1, request.clone())
            .await
            .unwrap();
        codec_small
            .write_request(&protocol, &mut buf2, request.clone())
            .await
            .unwrap();
        codec_large
            .write_request(&protocol, &mut buf3, request.clone())
            .await
            .unwrap();

        // All should produce identical output
        assert_eq!(buf1, buf2);
        assert_eq!(buf2, buf3);

        // All should read correctly
        let mut c1 = Cursor::new(buf1);
        let mut c2 = Cursor::new(buf2);
        let mut c3 = Cursor::new(buf3);

        let r1 = codec_default
            .read_request(&protocol, &mut c1)
            .await
            .unwrap();
        let r2 = codec_small.read_request(&protocol, &mut c2).await.unwrap();
        let r3 = codec_large.read_request(&protocol, &mut c3).await.unwrap();

        assert_eq!(r1, r2);
        assert_eq!(r2, r3);
        assert_eq!(r1, request);
    }

    // ========================================
    // Slow stream (byte-by-byte) eventually succeeds
    // ========================================

    /// A wrapper that yields data one byte at a time
    struct SlowReader<R> {
        inner: R,
    }

    impl<R: futures::io::AsyncRead + Unpin> futures::io::AsyncRead for SlowReader<R> {
        fn poll_read(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut [u8],
        ) -> Poll<io::Result<usize>> {
            // Limit to 1 byte at a time
            let limited_buf = if buf.len() > 1 { &mut buf[..1] } else { buf };
            Pin::new(&mut self.inner).poll_read(cx, limited_buf)
        }
    }

    #[tokio::test]
    async fn test_slow_stream_byte_by_byte_succeeds() {
        let mut codec = ContentCodec::new();
        let protocol = ContentProtocol::as_stream_protocol();

        let request = ContentRequest::get_block(sample_cid());

        let mut buffer = Vec::new();
        codec
            .write_request(&protocol, &mut buffer, request.clone())
            .await
            .unwrap();

        // Read one byte at a time
        let slow_cursor = SlowReader {
            inner: Cursor::new(buffer),
        };

        let mut slow = Box::pin(slow_cursor);
        let restored = codec.read_request(&protocol, &mut slow).await.unwrap();

        assert_eq!(request, restored);
    }

    // ========================================
    // Chunked stream (random chunk sizes) succeeds
    // ========================================

    /// A wrapper that yields data in random chunk sizes
    struct ChunkedReader<R> {
        inner: R,
        max_chunk: usize,
    }

    impl<R: futures::io::AsyncRead + Unpin> futures::io::AsyncRead for ChunkedReader<R> {
        fn poll_read(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut [u8],
        ) -> Poll<io::Result<usize>> {
            // Random chunk size between 1 and max_chunk
            let chunk_size = (rand::random::<usize>() % self.max_chunk).max(1);
            let limited_buf = if buf.len() > chunk_size {
                &mut buf[..chunk_size]
            } else {
                buf
            };
            Pin::new(&mut self.inner).poll_read(cx, limited_buf)
        }
    }

    #[tokio::test]
    async fn test_chunked_stream_random_sizes_succeeds() {
        let mut codec = ContentCodec::new();
        let protocol = ContentProtocol::as_stream_protocol();

        // Use a larger response to test chunking behavior
        let response = ContentResponse::block(
            sample_cid(),
            vec![0xAB; 4096], // 4KB of data
            vec![sample_cid_2()],
        );

        let mut buffer = Vec::new();
        codec
            .write_response(&protocol, &mut buffer, response.clone())
            .await
            .unwrap();

        // Read with random chunk sizes (1-100 bytes)
        let chunked_cursor = ChunkedReader {
            inner: Cursor::new(buffer),
            max_chunk: 100,
        };

        let mut chunked = Box::pin(chunked_cursor);
        let restored = codec.read_response(&protocol, &mut chunked).await.unwrap();

        assert_eq!(response, restored);
    }

    // ========================================
    // Errors include operation context
    // ========================================

    #[tokio::test]
    async fn test_errors_include_operation_context() {
        let mut codec = ContentCodec::with_max_size(100);
        let protocol = ContentProtocol::as_stream_protocol();

        // Test write error includes context
        let large_response = ContentResponse::block(sample_cid(), vec![0u8; 200], vec![]);
        let mut buffer = Vec::new();
        let write_err = codec
            .write_response(&protocol, &mut buffer, large_response)
            .await
            .unwrap_err();

        let err_msg = write_err.to_string();
        assert!(
            err_msg.contains("Message too large") || err_msg.contains("too large"),
            "Error should mention size: {}",
            err_msg
        );

        // Test read error includes context
        let mut oversized_buffer = Vec::new();
        oversized_buffer.extend_from_slice(&(200u32).to_be_bytes());
        oversized_buffer.extend_from_slice(&[0u8; 200]);

        let mut cursor = Cursor::new(oversized_buffer);
        let read_err = codec
            .read_request(&protocol, &mut cursor)
            .await
            .unwrap_err();

        let err_msg = read_err.to_string();
        assert!(
            err_msg.contains("Message too large") || err_msg.contains("too large"),
            "Error should mention size: {}",
            err_msg
        );
    }
}
