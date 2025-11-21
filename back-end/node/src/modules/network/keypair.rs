use errors::AppError;
use libp2p::identity::{Keypair, ed25519};

/// Convert a raw 32-byte Ed25519 private key to a libp2p Keypair
///
/// # Arguments
/// * `private_key_bytes` - The 32-byte Ed25519 private key from NodeData
///
/// # Returns
/// A libp2p Keypair that can be used for Swarm initialization
///
/// # Errors
/// Returns `AppError::Crypto` if the key is invalid or conversion fails
pub fn ed25519_to_libp2p_keypair(private_key_bytes: &[u8]) -> Result<Keypair, AppError> {
    // Validate key length
    if private_key_bytes.len() != 32 {
        return Err(AppError::Crypto(format!(
            "Invalid Ed25519 private key length: expected 32 bytes, got {}",
            private_key_bytes.len()
        )));
    }

    // Convert to ed25519_dalek SecretKey format
    let secret_key = ed25519_dalek::SigningKey::from_bytes(
        private_key_bytes
            .try_into()
            .map_err(|_| AppError::Crypto("Failed to convert key bytes".to_string()))?,
    );

    // Convert to libp2p ed25519::Keypair
    // Note: libp2p's ed25519::Keypair uses the same underlying format
    let libp2p_keypair =
        ed25519::Keypair::try_from_bytes(&mut secret_key.to_keypair_bytes().to_vec())
            .map_err(|e| AppError::Crypto(format!("Failed to create libp2p keypair: {}", e)))?;

    Ok(Keypair::from(libp2p_keypair))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use libp2p::identity::KeyType;
    use rand::rngs::OsRng;

    #[test]
    fn test_valid_key_conversion() {
        // Generate a test key
        let signing_key = SigningKey::generate(&mut OsRng);
        let private_bytes = signing_key.to_bytes();

        // Convert to libp2p
        let result = ed25519_to_libp2p_keypair(&private_bytes);
        assert!(result.is_ok());

        let keypair = result.unwrap();
        // Verify it's an Ed25519 keypair by checking the key type
        assert_eq!(keypair.key_type(), KeyType::Ed25519);
    }

    #[test]
    fn test_invalid_key_length() {
        let invalid_key = vec![0u8; 16]; // Wrong length
        let result = ed25519_to_libp2p_keypair(&invalid_key);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AppError::Crypto(_)));
    }

    #[test]
    fn test_peer_id_derivation() {
        // Generate key
        let signing_key = SigningKey::generate(&mut OsRng);
        let private_bytes = signing_key.to_bytes();

        // Convert and derive PeerId
        let keypair = ed25519_to_libp2p_keypair(&private_bytes).unwrap();
        let peer_id = keypair.public().to_peer_id();

        // PeerId should be deterministic from public key
        assert_eq!(peer_id, keypair.public().to_peer_id());
    }

    #[test]
    fn test_keypair_can_sign() {
        // Verify the keypair actually works for signing
        let signing_key = SigningKey::generate(&mut OsRng);
        let private_bytes = signing_key.to_bytes();

        let keypair = ed25519_to_libp2p_keypair(&private_bytes).unwrap();

        // Test that we can use the keypair to sign data
        let test_data = b"test message";
        let signature = keypair.sign(test_data);

        // Verify the signature works
        assert!(keypair.public().verify(test_data, &signature.unwrap()));
    }
}
