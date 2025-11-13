use node::modules::ssi::did::{
    resolvers::{DidResolver, ResolutionError},
    types::ResolutionOptions,
};
use tracing::info;

// ============================================================================
// INTEGRATION TESTS - DID:KEY Resolution
// ============================================================================

mod did_key_resolution_tests {
    use super::*;

    #[tokio::test]
    async fn test_resolve_did_key_ed25519() {
        let resolver = DidResolver::new();
        let did = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
        info!("DID Key: {}", did);

        let result = resolver
            .resolve_did(did, &ResolutionOptions::default())
            .await;

        assert!(result.is_ok());
        let res = result.unwrap();

        // Verify success
        assert!(res.is_success());
        assert!(res.did_document.is_some());

        // Verify metadata
        assert_eq!(
            res.did_resolution_metadata.did_method,
            Some("key".to_string())
        );
        assert!(res.did_resolution_metadata.error.is_none());
        assert!(res.did_resolution_metadata.duration.is_some());
        assert!(res.did_resolution_metadata.resolved_at.is_some());

        // Verify VDR info
        let vdr = res.did_resolution_metadata.verifiable_data_registry;
        assert!(vdr.is_some());
        let vdr = vdr.unwrap();
        assert_eq!(vdr.registry_type, "cryptographic");
        assert_eq!(vdr.verified, true);
        assert!(vdr.registry_proof.is_some());

        // Verify cache TTL (should be None for deterministic methods)
        assert_eq!(res.did_resolution_metadata.cache_ttl, None);

        // Log DID document for inspection
        if let Some(doc) = res.did_document {
            info!(
                "DID Document: {}",
                serde_json::to_string_pretty(&doc).unwrap()
            );
        }
    }

    #[tokio::test]
    async fn test_resolve_did_key_secp256k1() {
        let resolver = DidResolver::new();
        let did = "did:key:zQ3shokFTS3brHcDQrn82RUDfCZESWL1ZdCEJwekUDPQiYBme";

        let result = resolver
            .resolve_did(did, &ResolutionOptions::default())
            .await;

        assert!(result.is_ok());
        let res = result.unwrap();
        assert!(res.is_success());
    }

    #[tokio::test]
    async fn test_resolve_did_key_p256() {
        let resolver = DidResolver::new();
        let did = "did:key:zDnaerx9CtfPpYsw3fEXcPfGNNfCJQjWsmSBBqRcJFHr7g";

        let result = resolver
            .resolve_did(did, &ResolutionOptions::default())
            .await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_success());
    }

    #[tokio::test]
    async fn test_resolve_did_key_with_options() {
        let resolver = DidResolver::new();
        let did = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";

        let options = ResolutionOptions::new().no_cache();

        let result = resolver.resolve_did(did, &options).await;

        assert!(result.is_ok());
        let res = result.unwrap();
        assert_eq!(res.did_resolution_metadata.from_cache, Some(false));
    }
}

// ============================================================================
// INTEGRATION TESTS - DID:JWK Resolution
// ============================================================================

mod did_jwk_resolution_tests {
    use super::*;

    #[tokio::test]
    async fn test_resolve_did_jwk_p256() {
        let resolver = DidResolver::new();
        let did = "did:jwk:eyJjcnYiOiJQLTI1NiIsImt0eSI6IkVDIiwieCI6ImFjYklRaXVNczNpOF91c3pFakoydHBUdFJNNEVVM3l6OTFQSDZDZEgyVjAiLCJ5IjoiX0tjeUxqOXZXTXB0bm1LdG00NkdxRHo4d2Y3NEk1TEtncmwyR3pIM25TRSJ9";

        let result = resolver
            .resolve_did(did, &ResolutionOptions::default())
            .await;

        assert!(result.is_ok());
        let res = result.unwrap();

        assert!(res.is_success());
        assert_eq!(
            res.did_resolution_metadata.did_method,
            Some("jwk".to_string())
        );

        // Verify document structure
        let doc = res.did_document.unwrap();
        assert_eq!(doc.id.as_str(), did);
        assert!(!doc.verification_method.is_empty());
    }

    #[tokio::test]
    async fn test_resolve_did_jwk_ed25519() {
        let resolver = DidResolver::new();
        let did = "did:jwk:eyJrdHkiOiJPS1AiLCJjcnYiOiJFZDI1NTE5IiwieCI6IjExcVlBWUt4Q3JmVlNfN1R5V1FIT2c3aGN2UGFwaU1scndJYWFQY0hVUm8ifQ";

        let result = resolver
            .resolve_did(did, &ResolutionOptions::default())
            .await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_success());
    }
}

// ============================================================================
// INTEGRATION TESTS - DID:WEB Resolution (may fail without network)
// ============================================================================

mod did_web_resolution_tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Ignore by default as it requires network
    async fn test_resolve_did_web_w3c() {
        let resolver = DidResolver::new();
        let did = "did:web:w3c-ccg.github.io";

        let result = resolver
            .resolve_did(did, &ResolutionOptions::default())
            .await;

        match result {
            Ok(res) => {
                assert!(res.is_success());
                assert_eq!(
                    res.did_resolution_metadata.did_method,
                    Some("web".to_string())
                );

                // Verify VDR info for web
                let vdr = res
                    .did_resolution_metadata
                    .verifiable_data_registry
                    .unwrap();
                assert_eq!(vdr.registry_type, "https");
                assert!(vdr.registry_endpoint.is_some());
                assert_eq!(
                    vdr.registry_endpoint.unwrap(),
                    "https://w3c-ccg.github.io/.well-known/did.json"
                );

                // Should have cache TTL
                assert_eq!(res.did_resolution_metadata.cache_ttl, Some(3600));
            }
            Err(ResolutionError::NetworkError(_)) => {
                // Network errors are acceptable in tests
                info!("Network error - skipping test");
            }
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }

    #[test]
    fn test_web_endpoint_conversion_basic() {
        assert_eq!(
            DidResolver::web_endpoint_from_did("did:web:example.com")
                .ok()
                .map(|u| u.to_string())
                .unwrap(),
            "https://example.com/.well-known/did.json"
        );
    }

    #[test]
    fn test_web_endpoint_conversion_with_path() {
        assert_eq!(
            DidResolver::web_endpoint_from_did("did:web:example.com:user:alice")
                .ok()
                .map(|u| u.to_string())
                .unwrap(),
            "https://example.com/user/alice/did.json"
        );
    }

    #[test]
    fn test_web_endpoint_conversion_with_port() {
        assert_eq!(
            DidResolver::web_endpoint_from_did("did:web:example.com%3A3000")
                .ok()
                .map(|u| u.to_string())
                .unwrap(),
            "https://example.com:3000/.well-known/did.json"
        );
    }

    #[test]
    fn test_web_endpoint_conversion_with_port_and_path() {
        assert_eq!(
            DidResolver::web_endpoint_from_did("did:web:example.com%3A8080:api:v1")
                .ok()
                .map(|u| u.to_string())
                .unwrap(),
            "https://example.com:8080/api/v1/did.json"
        );
    }

    #[test]
    fn test_web_endpoint_conversion_invalid() {
        assert_eq!(
            DidResolver::web_endpoint_from_did("did:key:invalid")
                .ok()
                .map(|u| u.to_string())
                .unwrap_or_default(),
            ""
        );
    }
}

// ============================================================================
// ERROR HANDLING TESTS
// ============================================================================

mod error_handling_tests {
    use super::*;

    #[tokio::test]
    async fn test_invalid_did_format() {
        let resolver = DidResolver::new();
        let invalid_dids = vec![
            "not-a-did",
            "did:",
            "did::",
            "did:invalid",
            "",
            "http://example.com",
        ];

        for did in invalid_dids {
            let result = resolver
                .resolve_did(did, &ResolutionOptions::default())
                .await;

            assert!(result.is_err(), "Should fail for: {}", did);
            match result.unwrap_err() {
                ResolutionError::InvalidDid(_) => {} // Expected
                other => panic!("Expected InvalidDid error, got: {:?}", other),
            }
        }
    }

    #[tokio::test]
    async fn test_unsupported_method() {
        let resolver = DidResolver::new();
        let did = "did:unsupported:12345";

        let result = resolver
            .resolve_did(did, &ResolutionOptions::default())
            .await;

        assert!(result.is_err());
        match result.unwrap_err() {
            ResolutionError::MethodNotSupported(method) => {
                assert_eq!(method, "unsupported");
            }
            other => panic!("Expected MethodNotSupported error, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_malformed_did_key() {
        let resolver = DidResolver::new();
        let did = "did:key:invalid-multibase-string";

        let result = resolver
            .resolve_did(did, &ResolutionOptions::default())
            .await;

        assert!(result.is_err());
        // Should be InvalidDid or ResolutionFailed
    }

    #[tokio::test]
    async fn test_resolution_error_display() {
        let errors = vec![
            ResolutionError::InvalidDid("test".to_string()),
            ResolutionError::NotFound,
            ResolutionError::MethodNotSupported("test".to_string()),
            ResolutionError::NetworkError("test".to_string()),
            ResolutionError::InvalidDidDocument("test".to_string()),
            ResolutionError::SecurityError("test".to_string()),
            ResolutionError::Deactivated,
            ResolutionError::RepresentationNotSupported("test".to_string()),
            ResolutionError::ResolutionFailed("test".to_string()),
            ResolutionError::InternalError("test".to_string()),
        ];

        for error in errors {
            let display = format!("{}", error);
            assert!(!display.is_empty());

            let error_code = error.error_code();
            assert!(!error_code.is_empty());
        }
    }

    #[tokio::test]
    async fn test_error_result_helper() {
        use node::modules::ssi::did::resolvers::types::ResolutionResult;

        let error = ResolutionError::NotFound;
        let result = ResolutionResult::error(error);

        assert!(!result.is_success());
        assert!(result.did_document.is_none());
        assert_eq!(
            result.did_resolution_metadata.error,
            Some("notFound".to_string())
        );
    }
}

// ============================================================================
// TIMEOUT TESTS
// ============================================================================

mod timeout_tests {
    use super::*;

    #[tokio::test]
    async fn test_timeout_fast_resolution() {
        let resolver = DidResolver::new();
        let did = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";

        let mut options = ResolutionOptions::default();
        options.timeout_ms = Some(5000); // 5 seconds

        let result = resolver.resolve_did(did, &options).await;

        assert!(
            result.is_ok(),
            "Fast resolution should complete within timeout"
        );
    }

    #[tokio::test]
    async fn test_timeout_zero() {
        let resolver = DidResolver::new();
        let did = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";

        let mut options = ResolutionOptions::default();
        options.timeout_ms = Some(0); // Immediate timeout

        let result = resolver.resolve_did(did, &options).await;
        match result {
            Ok(_) => {
                // Very fast system - resolution completed in < 1ms
                info!("Resolution completed within 1ms (fast system)");
            }
            Err(ResolutionError::NetworkError(msg)) => {
                // Timeout occurred as expected
                assert!(msg.contains("Timeout") || msg.contains("timeout"));
            }
            Err(e) => {
                panic!("Expected timeout or success, got: {:?}", e);
            }
        }
    }

    #[tokio::test]
    #[ignore] // Slow test - ignore in CI
    async fn test_timeout_slow_resolution() {
        let resolver = DidResolver::new();
        // Using did:web which requires network
        let did = "did:web:example.com:some:very:long:path";

        let mut options = ResolutionOptions::default();
        options.timeout_ms = Some(100); // 100ms timeout

        let result = resolver.resolve_did(did, &options).await;

        // Should either timeout or fail quickly
        assert!(result.is_err());
    }
}

// ============================================================================
// VDR INFO VALIDATION TESTS
// ============================================================================

mod vdr_info_tests {
    use super::*;

    #[tokio::test]
    async fn test_vdr_info_did_key() {
        let resolver = DidResolver::new();
        let did = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";

        let result = resolver
            .resolve_did(did, &ResolutionOptions::default())
            .await
            .unwrap();

        let vdr = result
            .did_resolution_metadata
            .verifiable_data_registry
            .unwrap();
        assert_eq!(vdr.registry_type, "cryptographic");
        assert!(vdr.registry_endpoint.is_none());
        assert_eq!(vdr.verified, true);
        assert!(vdr.registry_proof.is_some());
        assert_eq!(vdr.registry_version, Some("did:key:latest".to_string()));
    }

    #[tokio::test]
    async fn test_vdr_info_did_jwk() {
        let resolver = DidResolver::new();
        let did = "did:jwk:eyJrdHkiOiJPS1AiLCJjcnYiOiJFZDI1NTE5IiwieCI6IjExcVlBWUt4Q3JmVlNfN1R5V1FIT2c3aGN2UGFwaU1scndJYWFQY0hVUm8ifQ";

        let result = resolver
            .resolve_did(did, &ResolutionOptions::default())
            .await
            .unwrap();

        let vdr = result
            .did_resolution_metadata
            .verifiable_data_registry
            .unwrap();
        assert_eq!(vdr.registry_type, "cryptographic");
        assert_eq!(vdr.verified, true);
    }

    #[test]
    fn test_vdr_serialization() {
        use node::modules::ssi::did::types::{RegistryProof, VdrInfo};

        let vdr = VdrInfo {
            registry_type: "cryptographic".to_string(),
            registry_endpoint: None,
            verified: true,
            registry_proof: Some(RegistryProof::CryptographicProof {
                signature: "test".to_string(),
                signature_algorithm: "ES256".to_string(),
                public_key_id: "did:key:test".to_string(),
                signed_data: "data".to_string(),
            }),
            registry_version: Some("v1".to_string()),
        };

        let json = serde_json::to_string(&vdr).unwrap();
        assert!(json.contains("cryptographic"));

        let deserialized: VdrInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.registry_type, "cryptographic");
    }
}

// ============================================================================
// METADATA VALIDATION TESTS
// ============================================================================

mod metadata_tests {
    use super::*;

    #[tokio::test]
    async fn test_resolution_metadata_completeness() {
        let resolver = DidResolver::new();
        let did = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";

        let result = resolver
            .resolve_did(did, &ResolutionOptions::default())
            .await
            .unwrap();

        let meta = result.did_resolution_metadata;

        // Required fields
        assert!(meta.content_type.is_some());
        assert!(meta.did_method.is_some());
        assert!(meta.resolved_at.is_some());
        assert!(meta.duration.is_some());

        // Optional fields
        assert!(meta.error.is_none()); // Should be None for success

        // Check duration is reasonable
        assert!(meta.duration.unwrap() < 10000); // Less than 10 seconds
    }

    #[tokio::test]
    async fn test_cache_ttl_suggestions() {
        let resolver = DidResolver::new();

        // Test different methods and their TTLs
        let test_cases = vec![
            (
                "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK",
                None,
            ),
            (
                "did:jwk:eyJrdHkiOiJPS1AiLCJjcnYiOiJFZDI1NTE5IiwieCI6IjExcVlBWUt4Q3JmVlNfN1R5V1FIT2c3aGN2UGFwaU1scndJYWFQY0hVUm8ifQ",
                None,
            ),
        ];

        for (did, expected_ttl) in test_cases {
            let result = resolver
                .resolve_did(did, &ResolutionOptions::default())
                .await
                .unwrap();

            assert_eq!(result.did_resolution_metadata.cache_ttl, expected_ttl);
        }
    }

    #[test]
    fn test_metadata_serialization() {
        use node::modules::ssi::did::types::ResolutionMetadata;

        let meta = ResolutionMetadata::success("key");

        let json = serde_json::to_string(&meta).unwrap();
        assert!(json.contains("key"));

        let deserialized: ResolutionMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.did_method, Some("key".to_string()));
    }

    #[test]
    fn test_document_metadata_default() {
        use node::modules::ssi::did::types::DocumentMetadata;

        let meta = DocumentMetadata::default();

        assert!(meta.created.is_none());
        assert!(meta.updated.is_none());
        assert!(meta.deactivated.is_none());
        assert!(meta.version_id.is_none());
    }
}

// ============================================================================
// RESOLVER CONFIGURATION TESTS
// ============================================================================

mod resolver_config_tests {
    use super::*;

    #[test]
    fn test_resolver_default() {
        let resolver = DidResolver::default();
        let methods = resolver.supported_methods();

        assert!(methods.contains(&"key"));
        assert!(methods.contains(&"jwk"));
        assert!(methods.contains(&"web"));
        assert!(methods.contains(&"pkh"));
        assert!(methods.contains(&"ethr"));
        assert!(methods.contains(&"ion"));
        assert!(methods.contains(&"tz"));
    }

    #[test]
    fn test_supported_methods() {
        let resolver = DidResolver::new();
        let methods = resolver.supported_methods();

        assert_eq!(methods.len(), 8);
        assert!(methods.contains(&"key"));
    }
}

// ============================================================================
// PERFORMANCE TESTS
// ============================================================================

#[cfg(test)]
mod performance_tests {
    use super::*;

    #[tokio::test]
    async fn test_resolution_performance_did_key() {
        let resolver = DidResolver::new();
        let did = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";

        let start = std::time::Instant::now();
        let result = resolver
            .resolve_did(did, &ResolutionOptions::default())
            .await
            .unwrap();
        let duration = start.elapsed();

        // Should be fast (under 1 second)
        assert!(
            duration.as_secs() < 1,
            "Resolution took too long: {:?}",
            duration
        );

        // Verify duration is tracked
        assert!(result.did_resolution_metadata.duration.is_some());
    }

    #[tokio::test]
    async fn test_concurrent_resolutions() {
        let resolver = std::sync::Arc::new(DidResolver::new());
        let did = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";

        let mut handles = vec![];

        for _ in 0..10 {
            let resolver_clone = resolver.clone();
            let did_clone = did.to_string();

            let handle = tokio::spawn(async move {
                resolver_clone
                    .resolve_did(&did_clone, &ResolutionOptions::default())
                    .await
            });

            handles.push(handle);
        }

        for handle in handles {
            let result = handle.await.unwrap();
            assert!(result.is_ok());
        }
    }
}
