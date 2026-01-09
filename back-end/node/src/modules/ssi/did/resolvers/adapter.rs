use chrono::Utc;
use ssi::dids::{AnyDidMethod as SsiResolver, DID, DIDResolver as SsiDIDResolver};
use std::time::Instant;

use crate::modules::ssi::did::resolvers::peer;

use super::super::types::{
    DocumentMetadata, RegistryProof, ResolutionMetadata, ResolutionOptions, VdrInfo,
};
use super::types::{ResolutionError, ResolutionResult};

use percent_encoding::percent_decode_str;
use std::borrow::Cow;
use thiserror::Error;
use url::Url;

#[derive(Debug, Error)]
pub enum DidWebError {
    #[error("not a did:web DID")]
    NotDidWeb,
    #[error("invalid authority")]
    InvalidAuthority,
    #[error("invalid url: {0}")]
    Url(#[from] url::ParseError),
}

/// DID resolver adapter
///
/// Wraps SSI's `AnyDidMethod` resolver with extended features:
/// - Verifiable Data Registry tracking
/// - Performance metrics
/// - Caching metadata
/// - Cryptographic proof collection
pub struct DidResolver {
    /// SSI's universal DID resolver (supports key, jwk, web, pkh, ethr, ion, tz)
    inner: SsiResolver,
}

impl DidResolver {
    /// Create a new resolver with default SSI resolver
    pub fn new() -> Self {
        Self {
            inner: SsiResolver::default(),
        }
    }

    /// Create with custom SSI resolver configuration
    pub fn with_resolver(resolver: SsiResolver) -> Self {
        Self { inner: resolver }
    }

    /// Convert our options to SSI options
    fn convert_options(options: &ResolutionOptions) -> ssi::dids::resolution::Options {
        // Start with the standard SSI options
        options.standard.clone()
    }

    /// Enrich SSI resolution result with metadata
    fn enrich_result(
        did: &str,
        ssi_output: ssi::dids::resolution::Output,
        duration_ms: u64,
        options: &ResolutionOptions,
    ) -> ResolutionResult {
        let method = did.split(':').nth(1).unwrap_or("unknown");

        // Extract document
        let did_document = Some(ssi_output.document.into_document());

        // Build metadata
        let did_resolution_metadata = ResolutionMetadata {
            content_type: ssi_output.metadata.content_type.clone(),
            error: None,
            verifiable_data_registry: Self::build_vdr_info(method, did),
            duration: Some(duration_ms),
            from_cache: options.no_cache.map(|nc| !nc),
            cache_ttl: Self::suggested_cache_ttl(method),
            resolved_at: Some(Utc::now()),
            did_method: Some(method.to_string()),
            additional: None,
        };

        // Convert SSI document metadata to our format
        let did_document_metadata = DocumentMetadata {
            created: None,
            updated: None,
            deactivated: ssi_output.document_metadata.deactivated,
            version_id: None,
            next_update: None,
            next_recovery: None,
            canonical_id: None,
            equivalent_id: None,
        };

        ResolutionResult {
            did_document,
            did_resolution_metadata,
            did_document_metadata,
        }
    }

    /// Build VDR info based on method type
    fn build_vdr_info(method: &str, did: &str) -> Option<VdrInfo> {
        match method {
            // TODO: Review these VDR infos in line with recommended standards.
            "key" | "jwk" => Some(VdrInfo {
                registry_type: "cryptographic".to_string(),
                registry_endpoint: None,
                verified: true,
                registry_proof: Some(RegistryProof::CryptographicProof {
                    signature: "self-certifying".to_string(),
                    signature_algorithm: "embedded-key".to_string(),
                    public_key_id: did.to_string(),
                    signed_data: did.to_string(),
                }),
                registry_version: Some(format!("did:{}:latest", method)),
            }),
            "web" => match Self::web_endpoint_from_did(did) {
                Ok(url) => Some(VdrInfo {
                    registry_type: "https".to_string(),
                    registry_endpoint: Some(url.to_string()),
                    verified: true,
                    registry_proof: Some(RegistryProof::HttpsProof {
                        url: url.to_string(),
                        tls_verified: true,
                        certificate_fingerprint: "pending-verification".to_string(),
                        response_headers: Default::default(),
                        retrieved_at: chrono::Utc::now(),
                    }),
                    registry_version: Some("HTTPS/1.1".to_string()),
                }),
                Err(_) => Some(VdrInfo {
                    registry_type: "https".to_string(),
                    registry_endpoint: None,
                    verified: false,
                    registry_proof: None,
                    registry_version: Some("HTTPS/1.1".to_string()),
                }),
            },
            // Methods without VDR verification implemented yet
            // When implementing, add actual blockchain verification logic
            "pkh" | "ethr" | "ion" | "tz" => None,
            _ => None,
        }
    }

    /// Convert did:web to HTTPS URL
    pub fn web_endpoint_from_did(did: &str) -> Result<Url, DidWebError> {
        let rest = did.strip_prefix("did:web:").ok_or(DidWebError::NotDidWeb)?;

        // Split on ":" to get authority + path segments (still encoded)
        let mut parts = rest.split(':');

        let authority_enc = parts.next().ok_or(DidWebError::InvalidAuthority)?;
        // Percent-decode authority so %3A can become ":" for ports
        let authority_dec: Cow<str> = percent_decode_str(authority_enc)
            .decode_utf8()
            .map_err(|_| DidWebError::InvalidAuthority)?;

        // Basic guardrails: no "/" or "@" in authority
        if authority_dec.contains('/') || authority_dec.contains('@') || authority_dec.is_empty() {
            return Err(DidWebError::InvalidAuthority);
        }

        // Seed a base https:// URL with the authority (host[:port])
        let mut url = Url::parse(&format!("https://{}/", authority_dec))?;

        // Collect and decode path segments
        let mut segs: Vec<String> = parts
            .map(|s| {
                let dec = percent_decode_str(s)
                    .decode_utf8()
                    .map_err(|_| DidWebError::Url(url::ParseError::IdnaError))?;
                // Disallow "." and ".." to avoid ambiguity
                if dec == "." || dec == ".." {
                    return Err(DidWebError::InvalidAuthority);
                }
                Ok(dec.to_string())
            })
            .collect::<Result<_, _>>()?;

        if segs.is_empty() {
            url.set_path(".well-known/did.json");
        } else {
            // Append "did.json" as terminal file
            segs.push("did.json".into());
            {
                let mut ps = url
                    .path_segments_mut()
                    .map_err(|_| DidWebError::Url(url::ParseError::RelativeUrlWithoutBase))?;
                ps.clear();
                for seg in &segs {
                    ps.push(seg);
                }
            }
        }

        Ok(url)
    }

    /// Suggest cache TTL based on method
    fn suggested_cache_ttl(method: &str) -> Option<u64> {
        match method {
            "key" | "jwk" => None, // Deterministic, cache indefinitely
            "web" => Some(3600),   // 1 hour
            "ion" => Some(300),    // 5 minutes
            "ethr" => Some(600),   // 10 minutes
            _ => Some(1800),       // 30 minutes default
        }
    }

    /// Resolve a DID using SSI's resolver with enhancements
    pub async fn resolve_did(
        &self,
        did: &str,
        options: &ResolutionOptions,
    ) -> Result<ResolutionResult, ResolutionError> {
        let future = async {
            // If a did:peer - handle locally
            if did.starts_with("did:peer:") {
                return peer::resolve_peer_did(did, options).await;
            } else {
                let start = Instant::now();

                let did_ref = DID::new(did.as_bytes()).map_err(|e| {
                    ResolutionError::InvalidDid(format!("Invalid DID format: {:?}", e))
                })?;
                let ssi_options = Self::convert_options(options);
                let ssi_output = self.inner.resolve_with(did_ref, ssi_options).await?;

                let duration_ms = start.elapsed().as_millis() as u64;

                Ok(Self::enrich_result(did, ssi_output, duration_ms, options))
            }
        };

        if let Some(timeout_ms) = options.timeout_ms {
            tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), future)
                .await
                .map_err(|_| ResolutionError::NetworkError("Timeout".to_string()))?
        } else {
            future.await
        }
    }

    /// Get list of supported methods
    pub fn supported_methods(&self) -> Vec<&str> {
        vec!["key", "jwk", "web", "pkh", "ethr", "ion", "tz", "peer"]
    }
}

impl Default for DidResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_resolve_did_key() {
        let resolver = DidResolver::new();
        let did = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";

        let result = resolver
            .resolve_did(did, &ResolutionOptions::default())
            .await
            .unwrap();

        assert!(result.is_success());
        assert_eq!(
            result.did_resolution_metadata.did_method,
            Some("key".to_string())
        );
        assert!(
            result
                .did_resolution_metadata
                .verifiable_data_registry
                .is_some()
        );
    }

    #[tokio::test]
    async fn test_resolve_did_jwk() {
        let resolver = DidResolver::new();
        let did = "did:jwk:eyJjcnYiOiJQLTI1NiIsImt0eSI6IkVDIiwieCI6ImFjYklRaXVNczNpOF91c3pFakoydHBUdFJNNEVVM3l6OTFQSDZDZEgyVjAiLCJ5IjoiX0tjeUxqOXZXTXB0bm1LdG00NkdxRHo4d2Y3NEk1TEtncmwyR3pIM25TRSJ9";

        let result = resolver
            .resolve_did(did, &ResolutionOptions::default())
            .await
            .unwrap();

        assert!(result.is_success());
        assert_eq!(
            result.did_resolution_metadata.did_method,
            Some("jwk".to_string())
        );
    }

    #[test]
    fn test_web_endpoint_conversion() {
        assert_eq!(
            DidResolver::web_endpoint_from_did("did:web:example.com")
                .ok()
                .map(|u| u.to_string())
                .unwrap(),
            "https://example.com/.well-known/did.json"
        );
    }
}
