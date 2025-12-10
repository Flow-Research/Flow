//! Content transfer protocol messages for peer-to-peer block exchange.
//!
//! This module defines the request/response message types used for
//! transferring content-addressed blocks between Flow nodes.
//!
//! # Protocol
//!
//! The protocol uses a simple request-response pattern:
//! 1. Requester sends `ContentRequest` (GetBlock or GetManifest)
//! 2. Responder replies with `ContentResponse` (Block, Manifest, NotFound, or Error)
//!
//! # Example
//!
//! ```ignore
//! // Request a block
//! let request = ContentRequest::GetBlock { cid: my_cid };
//!
//! // Serialize and send over network...
//!
//! // Handle response
//! match response {
//!     ContentResponse::Block { cid, data, links } => {
//!         // Store the block
//!     }
//!     ContentResponse::NotFound { cid } => {
//!         // Try another peer
//!     }
//!     _ => {}
//! }
//! ```

mod codec;
mod config;
mod error;
mod messages;

pub use codec::{ContentCodec, ContentProtocol};
pub use config::ContentTransferConfig;
pub use error::ContentTransferError;
pub use messages::{
    CONTENT_PROTOCOL_ID, CONTENT_PROTOCOL_VERSION, ContentRequest, ContentResponse, ErrorCode,
    MAX_LINKS, MAX_MESSAGE_SIZE, MessageError,
};
