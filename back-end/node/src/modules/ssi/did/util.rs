use log::{error, info};
use ssi::dids::{DIDKey, Document as DIDDocument};
use ssi::jwk::{JWK, Params as JWKParams};
use webauthn_rs::prelude::{COSEKey, Passkey};

use crate::modules::ssi::did::resolvers::peer::generator::PeerDidGenerator;

/// Generate both did:key and did:peer from a passkey
///
/// Useful when you want to create multiple DID representations
/// of the same passkey for different purposes.
pub fn generate_dids_from_passkey(
    passkey: &Passkey,
) -> Result<(String, String), Box<dyn std::error::Error>> {
    let did_key = generate_did_key_from_passkey(passkey)?;
    let did_peer = generate_did_peer_from_passkey(passkey)?;

    Ok((did_key, did_peer))
}

/// Generate a did:peer from a WebAuthn passkey
///
/// Creates a did:peer:0 (inception key) from the passkey's public key.
/// This is a self-certifying DID that can be shared directly with peers
/// without requiring any blockchain or centralized registry.
///
/// # Example
/// ```ignore
/// let did_peer = generate_did_peer_from_passkey(&passkey)?;
/// // Returns: "did:peer:0z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRoAnwWsdvktH"
/// ```
pub fn generate_did_peer_from_passkey(
    passkey: &Passkey,
) -> Result<String, Box<dyn std::error::Error>> {
    let did = PeerDidGenerator::from_passkey(passkey)?;

    info!("Generated did:peer: {did}");
    Ok(did)
}

/// Generate a did:key from a WebAuthn passkey
pub fn generate_did_key_from_passkey(
    passkey: &Passkey,
) -> Result<String, Box<dyn std::error::Error>> {
    let cose_key = passkey.get_public_key();

    // Convert COSE key to JWK
    let jwk = cose_to_jwk(cose_key)?;

    // Generate DID from JWK
    let did = DIDKey::generate(&jwk)?;

    info!("Generated DID: {did}");
    Ok(did.into_string())
}

/// Convert COSE key to JWK format
pub fn cose_to_jwk(cose_key: &COSEKey) -> Result<JWK, Box<dyn std::error::Error>> {
    use ssi::jwk::{ECParams, OctetParams};

    // Extract key type and algorithm from COSE key
    let alg = &cose_key.type_;

    // WebAuthn typically uses ES256 (ECDSA with P-256 and SHA-256)
    // COSE algorithm -7 = ES256
    match alg {
        webauthn_rs::prelude::COSEAlgorithm::ES256 => {
            // Extract x and y coordinates from COSE key
            let (x, y) = extract_ec_coordinates(cose_key)?;

            let ec_params = ECParams {
                curve: Some("P-256".to_string()),
                x_coordinate: Some(ssi::jwk::Base64urlUInt(x)),
                y_coordinate: Some(ssi::jwk::Base64urlUInt(y)),
                ecc_private_key: None,
            };

            Ok(JWK {
                params: JWKParams::EC(ec_params),
                public_key_use: Some("sig".to_string()),
                key_operations: Some(vec!["verify".to_string()]),
                algorithm: Some(ssi::jwk::Algorithm::ES256),
                key_id: None,
                x509_url: None,
                x509_certificate_chain: None,
                x509_thumbprint_sha1: None,
                x509_thumbprint_sha256: None,
            })
        }
        webauthn_rs::prelude::COSEAlgorithm::EDDSA => {
            // Extract public key bytes for EdDSA
            let public_key_bytes = extract_eddsa_public_key(cose_key)?;

            let octet_params = OctetParams {
                curve: "Ed25519".to_string(),
                public_key: ssi::jwk::Base64urlUInt(public_key_bytes),
                private_key: None,
            };

            Ok(JWK {
                params: JWKParams::OKP(octet_params),
                public_key_use: Some("sig".to_string()),
                key_operations: Some(vec!["verify".to_string()]),
                algorithm: Some(ssi::jwk::Algorithm::EdDSA),
                key_id: None,
                x509_url: None,
                x509_certificate_chain: None,
                x509_thumbprint_sha1: None,
                x509_thumbprint_sha256: None,
            })
        }
        _ => {
            error!("Unsupported COSE algorithm: {alg:?}");
            Err("Unsupported COSE algorithm".into())
        }
    }
}

/// Extract EC (P-256) coordinates from COSE key
pub fn extract_ec_coordinates(
    cose_key: &COSEKey,
) -> Result<(Vec<u8>, Vec<u8>), Box<dyn std::error::Error>> {
    use webauthn_rs::prelude::COSEKeyType;

    match &cose_key.key {
        COSEKeyType::EC_EC2(ec2_key) => {
            let x = ec2_key.x.as_ref().to_vec();
            let y = ec2_key.y.as_ref().to_vec();

            info!(
                "Extracted EC2 coordinates: x={} bytes, y={} bytes",
                x.len(),
                y.len()
            );
            Ok((x, y))
        }
        _ => Err("Expected EC_EC2 key type for ES256".into()),
    }
}

/// Extract EdDSA public key from COSE key
pub fn extract_eddsa_public_key(cose_key: &COSEKey) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    use webauthn_rs::prelude::COSEKeyType;

    match &cose_key.key {
        COSEKeyType::EC_OKP(okp_key) => {
            let public_key = okp_key.x.as_ref().to_vec();

            info!("Extracted OKP public key: {} bytes", public_key.len());
            Ok(public_key)
        }
        _ => Err("Expected EC_OKP key type for EdDSA".into()),
    }
}

/// Create a W3C DID Document from a DID and JWK
pub fn create_did_document(
    did: &str,
    jwk: &JWK,
) -> Result<DIDDocument, Box<dyn std::error::Error>> {
    use ssi::dids::document::verification_method::{DIDVerificationMethod, ValueOrReference};
    use std::collections::BTreeMap;

    // Convert string DID to DIDBuf
    let did_buf = did.parse::<ssi::dids::DIDBuf>()?;

    // Create verification method ID
    let verification_method_id = format!("{did}#key-1").parse::<ssi::dids::DIDURLBuf>()?;

    // Create verification method with JWK in properties
    let mut properties = BTreeMap::new();
    properties.insert("publicKeyJwk".to_string(), serde_json::to_value(jwk)?);

    let verification_method = DIDVerificationMethod::new(
        verification_method_id.clone(),
        "JsonWebKey2020".to_string(),
        did_buf.clone(),
        properties,
    );

    // Create reference for verification relationships
    let vm_reference = ValueOrReference::Reference(verification_method_id.clone().into());

    // Create document
    let mut doc = DIDDocument::new(did_buf);

    // Add verification method
    doc.verification_method = vec![verification_method];

    // Add authentication relationship
    doc.verification_relationships.authentication = vec![vm_reference.clone()];

    // Add assertion method for signing
    doc.verification_relationships.assertion_method = vec![vm_reference];

    Ok(doc)
}

/// Serialize DID Document to JSON
pub fn did_document_to_json(doc: &DIDDocument) -> Result<String, Box<dyn std::error::Error>> {
    let json = serde_json::to_string_pretty(doc)?;
    Ok(json)
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_did_generation() {
        // This would require a real passkey for testing
        // You can add integration tests when you have passkeys
    }
}
