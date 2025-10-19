use ssi::dids::Document as DIDDocument;

use super::super::types::{DocumentMetadata, ResolutionMetadata};

/// W3C DID Resolution result
/// Spec: resolve(did, resolutionOptions) → « didResolutionMetadata, didDocument, didDocumentMetadata »
/// See: https://www.w3.org/TR/did-core/#did-resolution
#[derive(Debug, Clone)]
pub struct ResolutionResult {
    /// The DID Document (if resolution was successful)
    pub did_document: Option<DIDDocument>,

    /// Metadata about the resolution process
    pub did_resolution_metadata: ResolutionMetadata,

    /// Metadata about the DID Document
    pub did_document_metadata: DocumentMetadata,
}

impl ResolutionResult {
    /// Create a successful resolution result
    pub fn success(document: DIDDocument, method: impl Into<String>) -> Self {
        Self {
            did_document: Some(document),
            did_resolution_metadata: ResolutionMetadata::success(method),
            did_document_metadata: DocumentMetadata::default(),
        }
    }

    /// Create an error resolution result
    pub fn error(error: ResolutionError) -> Self {
        let error_code = error.error_code();
        Self {
            did_document: None,
            did_resolution_metadata: ResolutionMetadata::error(error_code),
            did_document_metadata: DocumentMetadata::default(),
        }
    }

    /// Check if the resolution was successful
    pub fn is_success(&self) -> bool {
        self.did_document.is_some() && self.did_resolution_metadata.error.is_none()
    }
}

/// DID Resolution errors following W3C DID Core specification
/// See: https://www.w3.org/TR/did-spec-registries/#error
#[derive(Debug, Clone)]
pub enum ResolutionError {
    /// The DID supplied is invalid
    InvalidDid(String),

    /// The DID Document was not found
    NotFound,

    /// The DID method is not supported by this resolver
    MethodNotSupported(String),

    /// Network error occurred during resolution
    NetworkError(String),

    /// The DID Document is invalid or malformed
    InvalidDidDocument(String),

    /// A security or cryptographic error occurred
    SecurityError(String),

    /// The DID has been deactivated
    Deactivated,

    /// The requested representation is not supported
    RepresentationNotSupported(String),

    /// Generic resolution failure
    ResolutionFailed(String),

    /// Internal resolver error
    InternalError(String),
}

impl ResolutionError {
    /// Get the W3C standard error code for this error
    pub fn error_code(&self) -> &str {
        match self {
            Self::InvalidDid(_) => "invalidDid",
            Self::NotFound => "notFound",
            Self::MethodNotSupported(_) => "methodNotSupported",
            Self::NetworkError(_) => "networkError",
            Self::InvalidDidDocument(_) => "invalidDidDocument",
            Self::SecurityError(_) => "securityError",
            Self::Deactivated => "deactivated",
            Self::RepresentationNotSupported(_) => "representationNotSupported",
            Self::ResolutionFailed(_) => "resolutionFailed",
            Self::InternalError(_) => "internalError",
        }
    }
}

impl std::fmt::Display for ResolutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidDid(msg) => write!(f, "Invalid DID: {msg}"),
            Self::NotFound => write!(f, "DID not found"),
            Self::MethodNotSupported(method) => write!(f, "DID method '{method}' not supported"),
            Self::NetworkError(msg) => write!(f, "Network error: {msg}"),
            Self::InvalidDidDocument(msg) => write!(f, "Invalid DID Document: {msg}"),
            Self::SecurityError(msg) => write!(f, "Security error: {msg}"),
            Self::Deactivated => write!(f, "DID has been deactivated"),
            Self::RepresentationNotSupported(repr) => {
                write!(f, "Representation '{repr}' not supported")
            }
            Self::ResolutionFailed(msg) => write!(f, "Resolution failed: {msg}"),
            Self::InternalError(msg) => write!(f, "Internal error: {msg}"),
        }
    }
}

impl std::error::Error for ResolutionError {}

/// Convert from SSI's resolution error to our error type
impl From<ssi::dids::resolution::Error> for ResolutionError {
    fn from(err: ssi::dids::resolution::Error) -> Self {
        use ssi::dids::resolution::Error as SsiError;

        match err {
            SsiError::MethodNotSupported(m) => Self::MethodNotSupported(m),
            SsiError::NotFound => Self::NotFound,
            SsiError::InvalidData(_) => Self::InvalidDidDocument(err.to_string()),
            SsiError::InvalidMethodSpecificId(m) => Self::InvalidDid(m),
            SsiError::NoRepresentation => Self::RepresentationNotSupported("none".to_string()),
            SsiError::RepresentationNotSupported(r) => Self::RepresentationNotSupported(r),
            SsiError::InvalidOptions => Self::ResolutionFailed("Invalid options".to_string()),
            SsiError::Internal(msg) => Self::InternalError(msg),
        }
    }
}
