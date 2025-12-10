//! Error types for content transfer operations.

use thiserror::Error;

/// Errors that can occur during content transfer.
#[derive(Debug, Clone, Error)]
pub enum ContentTransferError {
    /// Content was not found on the remote peer.
    #[error("Content not found: {cid}")]
    NotFound { cid: String },

    /// Request timed out waiting for response.
    #[error("Request timed out after {timeout_secs}s")]
    Timeout { timeout_secs: u64 },

    /// The peer disconnected before responding.
    #[error("Peer disconnected: {peer_id}")]
    PeerDisconnected { peer_id: String },

    /// The remote peer returned an error.
    #[error("Remote error ({code}): {message}")]
    RemoteError { code: String, message: String },

    /// Failed to send request to peer.
    #[error("Request failed: {message}")]
    RequestFailed { message: String },

    /// Network-level error.
    #[error("Network error: {message}")]
    NetworkError { message: String },

    /// All retry attempts were exhausted.
    #[error("All {attempts} retry attempts exhausted")]
    RetriesExhausted { attempts: u32 },

    /// The network manager is not running.
    #[error("Network manager not started")]
    NotStarted,

    /// Internal channel error.
    #[error("Internal error: {message}")]
    Internal { message: String },
}
