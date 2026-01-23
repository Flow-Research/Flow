use thiserror::Error;

use super::ContentId;

/// Errors that can occur during block store operations.
#[derive(Debug, Error)]
pub enum BlockStoreError {
    /// Storage backend error
    #[error("Storage error: {0}")]
    Storage(String),

    /// Block exceeds maximum allowed size
    #[error("Block too large: {size} bytes (max: {max})")]
    BlockTooLarge { size: usize, max: usize },

    /// Invalid CID format or encoding
    #[error("Invalid CID: {0}")]
    InvalidCid(String),

    /// Block not found in store
    #[error("Block not found: {0}")]
    NotFound(ContentId),

    /// Serialization/deserialization error
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Data integrity verification failed
    #[error("Integrity error: expected {expected}, got {actual}")]
    IntegrityError {
        expected: ContentId,
        actual: ContentId,
    },

    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),

    /// Database already exists or is locked
    #[error("Database error: {0}")]
    Database(String),
}

impl From<rocksdb::Error> for BlockStoreError {
    fn from(err: rocksdb::Error) -> Self {
        BlockStoreError::Storage(err.to_string())
    }
}

impl From<serde_ipld_dagcbor::EncodeError<std::collections::TryReserveError>> for BlockStoreError {
    fn from(err: serde_ipld_dagcbor::EncodeError<std::collections::TryReserveError>) -> Self {
        BlockStoreError::Serialization(err.to_string())
    }
}

impl From<serde_ipld_dagcbor::DecodeError<std::io::Error>> for BlockStoreError {
    fn from(err: serde_ipld_dagcbor::DecodeError<std::io::Error>) -> Self {
        BlockStoreError::Serialization(err.to_string())
    }
}

impl From<BlockStoreError> for errors::AppError {
    fn from(err: BlockStoreError) -> Self {
        errors::AppError::Storage(Box::new(err))
    }
}
