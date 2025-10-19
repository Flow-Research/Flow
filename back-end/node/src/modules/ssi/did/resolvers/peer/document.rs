use super::{error::PeerDidError, parser::*};
use base64::Engine;
use log::error;
use ssi::dids::{Document as DIDDocument, document::DIDVerificationMethod};
use std::collections::BTreeMap;

pub fn create_did_document(did: &str, parsed: ParsedPeerDid) -> Result<DIDDocument, PeerDidError> {
    use ssi::dids::document::verification_method::ValueOrReference;

    // Parse DID into SSI's DIDBuf
    let did_buf = did
        .parse::<ssi::dids::DIDBuf>()
        .map_err(|e| PeerDidError::DidParseError(e.to_string()))?;

    let mut doc = DIDDocument::new(did_buf.clone());

    // Add verification methods
    let mut auth_refs = Vec::new();
    let mut assertion_refs = Vec::new();
    let mut key_agreement_refs = Vec::new();

    for (idx, method) in parsed.methods.iter().enumerate() {
        let vm_id = format!("{}#key-{}", did, idx + 1)
            .parse::<ssi::dids::DIDURLBuf>()
            .map_err(|e| PeerDidError::DidParseError(e.to_string()))?;

        let vm = create_verification_method(&vm_id, &did_buf, method)?;
        doc.verification_method.push(vm);

        // Add to appropriate relationship
        let vm_ref = ValueOrReference::Reference(vm_id.into());

        match method.purpose {
            Purpose::Verification | Purpose::Authentication => {
                auth_refs.push(vm_ref.clone());
                assertion_refs.push(vm_ref);
            }
            Purpose::KeyAgreement => {
                key_agreement_refs.push(vm_ref);
            }
        }
    }

    doc.verification_relationships.authentication = auth_refs;
    doc.verification_relationships.assertion_method = assertion_refs;
    doc.verification_relationships.key_agreement = key_agreement_refs;

    // Add services
    for (idx, service) in parsed.services.iter().enumerate() {
        let service_id = format!("{}#service-{}", did, idx + 1);
        doc.service.push(create_service(service_id, service)?);
    }

    Ok(doc)
}

fn create_verification_method(
    id: &ssi::dids::DIDURLBuf,
    controller: &ssi::dids::DIDBuf,
    method: &VerificationMethod,
) -> Result<DIDVerificationMethod, PeerDidError> {
    let (vm_type, key_field) = match method.key_type {
        KeyType::Ed25519 => ("Ed25519VerificationKey2020", "publicKeyMultibase"),
        KeyType::X25519 => ("X25519KeyAgreementKey2020", "publicKeyMultibase"),
        KeyType::Secp256k1 => ("EcdsaSecp256k1VerificationKey2019", "publicKeyMultibase"),
        KeyType::P256 => ("JsonWebKey2020", "publicKeyJwk"),
    };

    let mut properties = BTreeMap::new();

    // Encode key appropriately
    if vm_type == "JsonWebKey2020" {
        // For P-256, use JWK format
        let jwk = serde_json::json!({
            "kty": "EC",
            "crv": "P-256",
            "x": base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(&method.public_key[0..32]),
            "y": base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(&method.public_key[32..64]),
        });
        properties.insert(key_field.to_string(), jwk);
    } else {
        // For others, use multibase
        let multicodec_prefix = match method.key_type {
            KeyType::Ed25519 => vec![0xed, 0x01],
            KeyType::X25519 => vec![0xec, 0x01],
            KeyType::Secp256k1 => vec![0xe7, 0x01],
            _ => vec![],
        };

        let mut multicodec_key = multicodec_prefix;
        multicodec_key.extend_from_slice(&method.public_key);

        let encoded = multibase::encode(multibase::Base::Base58Btc, &multicodec_key);
        properties.insert(key_field.to_string(), serde_json::Value::String(encoded));
    }

    Ok(DIDVerificationMethod::new(
        id.clone(),
        vm_type.to_string(),
        controller.clone(),
        properties,
    ))
}

fn create_service(
    id: String,
    service: &ServiceEndpoint,
) -> Result<ssi::dids::document::service::Service, PeerDidError> {
    use ssi::OneOrMany;
    use ssi::dids::document::service::{Endpoint, Service};

    // Create the endpoint - SSI expects an Endpoint enum
    let endpoint = if service.routing_keys.is_empty() && service.accept.is_empty() {
        // Simple URI endpoint
        Endpoint::Uri(service.endpoint.clone().parse().map_err(|e| {
            error!("Error encountered: {e}");
            PeerDidError::InvalidFormat
        })?)
    } else {
        // Complex endpoint with routing keys and accept
        Endpoint::Map(serde_json::json!({
            "uri": service.endpoint,
            "routingKeys": service.routing_keys,
            "accept": service.accept,
        }))
    };

    Ok(Service {
        id: id
            .parse()
            .map_err(|e| PeerDidError::DidParseError(format!("{e:?}")))?,

        type_: OneOrMany::One(service.service_type.clone()),

        service_endpoint: Some(OneOrMany::One(endpoint)),
        property_set: BTreeMap::new(),
    })
}
