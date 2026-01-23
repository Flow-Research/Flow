//! Error types for distributed search.

use axum::http::StatusCode;
use thiserror::Error;

/// Errors that can occur during distributed search.
#[derive(Debug, Error)]
pub enum DistributedSearchError {
    /// Invalid query parameters.
    #[error("invalid query: {0}")]
    InvalidQuery(String),

    /// Invalid scope parameter.
    #[error("invalid scope: {0}")]
    InvalidScope(String),

    /// Embedding generation failed.
    #[error("embedding error: {0}")]
    Embedding(String),

    /// Qdrant vector database error.
    #[error("qdrant error: {0}")]
    Qdrant(String),

    /// Network communication error.
    #[error("network error: {0}")]
    Network(String),

    /// Message serialization error.
    #[error("serialization error: {0}")]
    Serialization(String),

    /// Message deserialization error.
    #[error("deserialization error: {0}")]
    Deserialization(String),

    /// No peers available for network search.
    #[error("no peers available")]
    NoPeersAvailable,

    /// Search operation timed out.
    #[error("timeout")]
    Timeout,

    /// Internal error.
    #[error("internal error: {0}")]
    Internal(String),

    /// Query validation failed.
    #[error("validation error: {0}")]
    Validation(String),
}

impl DistributedSearchError {
    /// Get the HTTP status code for this error.
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::InvalidQuery(_) | Self::InvalidScope(_) | Self::Validation(_) => {
                StatusCode::BAD_REQUEST
            }
            Self::NoPeersAvailable => StatusCode::SERVICE_UNAVAILABLE,
            Self::Timeout => StatusCode::GATEWAY_TIMEOUT,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// Get the error code string for this error.
    pub fn error_code(&self) -> &'static str {
        match self {
            Self::InvalidQuery(_) => "INVALID_QUERY",
            Self::InvalidScope(_) => "INVALID_SCOPE",
            Self::Validation(_) => "VALIDATION_ERROR",
            Self::Embedding(_) => "EMBEDDING_ERROR",
            Self::Qdrant(_) => "QDRANT_ERROR",
            Self::Network(_) => "NETWORK_ERROR",
            Self::Serialization(_) => "SERIALIZATION_ERROR",
            Self::Deserialization(_) => "DESERIALIZATION_ERROR",
            Self::NoPeersAvailable => "NETWORK_UNAVAILABLE",
            Self::Timeout => "TIMEOUT",
            Self::Internal(_) => "INTERNAL_ERROR",
        }
    }

    /// Create an invalid query error.
    pub fn invalid_query(msg: impl Into<String>) -> Self {
        Self::InvalidQuery(msg.into())
    }

    /// Create a validation error.
    pub fn validation(msg: impl Into<String>) -> Self {
        Self::Validation(msg.into())
    }

    /// Create a network error.
    pub fn network(msg: impl Into<String>) -> Self {
        Self::Network(msg.into())
    }

    /// Create an internal error.
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }
}

/// Result type alias for distributed search operations.
pub type SearchResult<T> = Result<T, DistributedSearchError>;

/// Validation helper for search requests.
pub struct RequestValidator;

impl RequestValidator {
    /// Maximum allowed query length.
    pub const MAX_QUERY_LENGTH: usize = 1000;

    /// Maximum allowed result limit.
    pub const MAX_LIMIT: u32 = 100;

    /// Validate a search query string.
    pub fn validate_query(query: &str) -> SearchResult<()> {
        if query.is_empty() {
            return Err(DistributedSearchError::validation("Query cannot be empty"));
        }

        if query.len() > Self::MAX_QUERY_LENGTH {
            return Err(DistributedSearchError::validation(format!(
                "Query too long: {} chars (max {})",
                query.len(),
                Self::MAX_QUERY_LENGTH
            )));
        }

        Ok(())
    }

    /// Validate a result limit.
    pub fn validate_limit(limit: u32) -> SearchResult<()> {
        if limit == 0 {
            return Err(DistributedSearchError::validation(
                "Limit must be greater than 0",
            ));
        }

        if limit > Self::MAX_LIMIT {
            return Err(DistributedSearchError::validation(format!(
                "Limit too high: {} (max {})",
                limit,
                Self::MAX_LIMIT
            )));
        }

        Ok(())
    }

    /// Validate a complete search request.
    pub fn validate(query: &str, limit: u32) -> SearchResult<()> {
        Self::validate_query(query)?;
        Self::validate_limit(limit)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_status_codes() {
        assert_eq!(
            DistributedSearchError::invalid_query("test").status_code(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            DistributedSearchError::NoPeersAvailable.status_code(),
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(
            DistributedSearchError::Timeout.status_code(),
            StatusCode::GATEWAY_TIMEOUT
        );
        assert_eq!(
            DistributedSearchError::internal("test").status_code(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn test_error_codes() {
        assert_eq!(
            DistributedSearchError::invalid_query("test").error_code(),
            "INVALID_QUERY"
        );
        assert_eq!(
            DistributedSearchError::NoPeersAvailable.error_code(),
            "NETWORK_UNAVAILABLE"
        );
    }

    #[test]
    fn test_validate_query() {
        // Empty query
        assert!(RequestValidator::validate_query("").is_err());

        // Valid query
        assert!(RequestValidator::validate_query("test query").is_ok());

        // Too long query
        let long_query = "a".repeat(1001);
        assert!(RequestValidator::validate_query(&long_query).is_err());

        // Exactly at limit
        let max_query = "a".repeat(1000);
        assert!(RequestValidator::validate_query(&max_query).is_ok());
    }

    #[test]
    fn test_validate_limit() {
        // Zero limit
        assert!(RequestValidator::validate_limit(0).is_err());

        // Valid limits
        assert!(RequestValidator::validate_limit(1).is_ok());
        assert!(RequestValidator::validate_limit(50).is_ok());
        assert!(RequestValidator::validate_limit(100).is_ok());

        // Too high limit
        assert!(RequestValidator::validate_limit(101).is_err());
    }

    #[test]
    fn test_validate_request() {
        // Valid request
        assert!(RequestValidator::validate("test", 10).is_ok());

        // Invalid query
        assert!(RequestValidator::validate("", 10).is_err());

        // Invalid limit
        assert!(RequestValidator::validate("test", 0).is_err());
    }
}
