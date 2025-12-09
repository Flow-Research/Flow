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
    type Protocol = ContentProtocol;
    type Request = ContentRequest;
    type Response = ContentResponse;

    /// Read a request from the stream.
    async fn read_request<T>(
        &mut self,
        _protocol: &Self::Protocol,
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
        _protocol: &Self::Protocol,
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
        _protocol: &Self::Protocol,
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
        _protocol: &Self::Protocol,
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
        let protocol = ContentProtocol::new();

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
        let protocol = ContentProtocol::new();

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
        let protocol = ContentProtocol::new();

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
        let protocol = ContentProtocol::new();

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
        let protocol = ContentProtocol::new();

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
        let protocol = ContentProtocol::new();

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
        let protocol = ContentProtocol::new();

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
        let protocol = ContentProtocol::new();

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
        let protocol = ContentProtocol::new();

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
        let protocol = ContentProtocol::new();

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
        let protocol = ContentProtocol::new();

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
        let protocol = ContentProtocol::new();

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
        let protocol = ContentProtocol::new();

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
        let protocol = ContentProtocol::new();

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
}
