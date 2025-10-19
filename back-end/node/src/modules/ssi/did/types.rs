use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use ssi::dids::document::representation::MediaType;
// Re-export commonly used SSI types for convenience
pub use ssi::dids::resolution::{
    Error as SsiResolutionError, Options as SsiResolutionOptions,
    Parameters as SsiResolutionParameters,
};

/// Extended resolution options that wrap SSI's standard options
/// with additional production-ready features
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResolutionOptions {
    /// Standard W3C DID resolution options
    #[serde(flatten)]
    pub standard: SsiResolutionOptions,

    /// Whether to bypass cache (implementation-specific)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub no_cache: Option<bool>,

    /// Request timeout in milliseconds (implementation-specific)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

impl ResolutionOptions {
    /// Create options with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the accept media type
    pub fn with_accept(mut self, accept: &str) -> Result<Self, String> {
        let media_type = accept
            .parse::<MediaType>()
            .map_err(|e| format!("Invalid media type: {e}"))?;
        self.standard.accept = Some(media_type);
        Ok(self)
    }

    /// Disable cache for this resolution
    pub fn no_cache(mut self) -> Self {
        self.no_cache = Some(true);
        self
    }
}

/// Extended resolution metadata following W3C DID Core spec
/// with production-ready extensions for verifiable provenance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolutionMetadata {
    // W3C Standard properties
    /// MIME type of the returned didDocument
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,

    /// Error code if resolution failed
    /// Values: "invalidDid", "notFound", "representationNotSupported", etc.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,

    // Extended properties for production use
    /// Information about the Verifiable Data Registry used
    /// This provides verifiable provenance for the DID Document
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verifiable_data_registry: Option<VdrInfo>,

    /// Time taken to resolve (milliseconds)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<u64>,

    /// Whether result came from cache
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_cache: Option<bool>,

    /// Cache TTL if applicable (seconds)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_ttl: Option<u64>,

    /// Timestamp of resolution
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<DateTime<Utc>>,

    /// DID method used
    #[serde(skip_serializing_if = "Option::is_none")]
    pub did_method: Option<String>,

    /// Method-specific additional metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional: Option<serde_json::Value>,
}

impl Default for ResolutionMetadata {
    fn default() -> Self {
        Self {
            content_type: Some("application/did+ld+json".to_string()),
            error: None,
            verifiable_data_registry: None,
            duration: None,
            from_cache: None,
            cache_ttl: None,
            resolved_at: None,
            did_method: None,
            additional: None,
        }
    }
}

impl ResolutionMetadata {
    /// Create metadata indicating an error
    pub fn error(error: impl Into<String>) -> Self {
        Self {
            error: Some(error.into()),
            ..Default::default()
        }
    }

    /// Create successful metadata with DID method
    pub fn success(did_method: impl Into<String>) -> Self {
        Self {
            did_method: Some(did_method.into()),
            resolved_at: Some(Utc::now()),
            ..Default::default()
        }
    }
}

/// Document metadata following W3C DID Core spec
/// Extended with common versioning metadata
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DocumentMetadata {
    /// Timestamp of the creation of the DID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<DateTime<Utc>>,

    /// Timestamp of the most recent update to the DID document
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated: Option<DateTime<Utc>>,

    /// Whether the DID has been deactivated
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deactivated: Option<bool>,

    /// Version identifier for this version of the DID document
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version_id: Option<String>,

    /// Hash of the next update key for key rotation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_update: Option<String>,

    /// Hash of the next recovery key for account recovery
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_recovery: Option<String>,

    /// Canonical ID if this is an equivalent ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canonical_id: Option<String>,

    /// Equivalent IDs
    #[serde(skip_serializing_if = "Option::is_none")]
    pub equivalent_id: Option<Vec<String>>,
}

/// Verifiable Data Registry information
/// Provides cryptographic proof of where and how the DID was resolved
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VdrInfo {
    /// Type of verifiable data registry
    /// Examples: "blockchain", "distributed-ledger", "https", "ipfs", "local", "peer"
    pub registry_type: String,

    /// Specific endpoint or location used
    /// Examples:
    ///   - "https://ion.msidentity.com"
    ///   - "https://example.com/.well-known/did.json"
    ///   - "bitcoin:mainnet"
    ///   - "ethereum:mainnet"
    ///   - "local:database"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registry_endpoint: Option<String>,

    /// Whether the data was cryptographically verified against the registry
    pub verified: bool,

    /// Cryptographic proof of the data's existence/validity in the registry
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registry_proof: Option<RegistryProof>,

    /// Version or state of the registry at resolution time
    /// (e.g., block number, commit hash, timestamp)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registry_version: Option<String>,
}

/// Cryptographic proof from a Verifiable Data Registry
/// Different proof types for different registry types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum RegistryProof {
    /// For blockchain-based registries (Bitcoin, Ethereum, etc.)
    BlockchainAnchor {
        blockchain: String,
        block_number: u64,
        block_hash: String,
        transaction_hash: String,
        confirmations: u32,
    },

    /// For HTTPS-based registries (did:web, did:https)
    HttpsProof {
        url: String,
        tls_verified: bool,
        certificate_fingerprint: String,
        response_headers: HashMap<String, String>,
        retrieved_at: DateTime<Utc>,
    },

    /// For cryptographic signature-based verification
    CryptographicProof {
        signature: String,
        signature_algorithm: String,
        public_key_id: String,
        signed_data: String,
    },

    /// For IPFS/content-addressed storage
    ContentAddressedProof {
        cid: String,
        hash_algorithm: String,
        retrieved_at: DateTime<Utc>,
    },

    /// For local/peer registries
    LocalProof {
        stored_at: DateTime<Utc>,
        database_id: String,
    },
}
