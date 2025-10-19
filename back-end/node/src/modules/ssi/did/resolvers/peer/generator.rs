use super::error::PeerDidError;
use webauthn_rs::prelude::{COSEKey, Passkey};

/// Generate did:peer from WebAuthn passkey
pub struct PeerDidGenerator;

impl PeerDidGenerator {
    /// Generate did:peer:0 from a WebAuthn passkey (numalgo 0 - inception key)
    ///
    /// This creates a simple did:peer with a single verification key.
    /// Perfect for basic peer-to-peer identity.
    ///
    /// # Example
    /// ```ignore
    /// let did = PeerDidGenerator::from_passkey(&passkey)?;
    /// // Returns: "did:peer:0z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRoAnwWsdvktH"
    /// ```
    pub fn from_passkey(passkey: &Passkey) -> Result<String, PeerDidError> {
        let cose_key = passkey.get_public_key();
        Self::from_cose_key(cose_key)
    }

    /// Generate did:peer:0 from a COSE key
    pub fn from_cose_key(cose_key: &COSEKey) -> Result<String, PeerDidError> {
        use webauthn_rs::prelude::COSEAlgorithm;

        match &cose_key.type_ {
            COSEAlgorithm::ES256 => {
                // P-256 key
                Self::generate_numalgo0_p256(cose_key)
            }
            COSEAlgorithm::EDDSA => {
                // Ed25519 key
                Self::generate_numalgo0_ed25519(cose_key)
            }
            _ => Err(PeerDidError::UnsupportedKeyType),
        }
    }

    /// Generate did:peer:0 from Ed25519 public key bytes
    pub fn from_ed25519_bytes(public_key: &[u8]) -> Result<String, PeerDidError> {
        if public_key.len() != 32 {
            return Err(PeerDidError::InvalidEncoding(
                "Ed25519 key must be 32 bytes".to_string(),
            ));
        }

        // Multicodec prefix for Ed25519: 0xed01
        let mut multicodec_key = vec![0xed, 0x01];
        multicodec_key.extend_from_slice(public_key);

        // Encode as base58btc
        let encoded = multibase::encode(multibase::Base::Base58Btc, &multicodec_key);

        Ok(format!("did:peer:0{encoded}"))
    }

    /// Generate did:peer:0 from X25519 public key bytes
    pub fn from_x25519_bytes(public_key: &[u8]) -> Result<String, PeerDidError> {
        if public_key.len() != 32 {
            return Err(PeerDidError::InvalidEncoding(
                "X25519 key must be 32 bytes".to_string(),
            ));
        }

        // Multicodec prefix for X25519: 0xec01
        let mut multicodec_key = vec![0xec, 0x01];
        multicodec_key.extend_from_slice(public_key);

        let encoded = multibase::encode(multibase::Base::Base58Btc, &multicodec_key);

        Ok(format!("did:peer:0{encoded}"))
    }

    /// Generate did:peer:0 from P-256 key (WebAuthn ES256)
    fn generate_numalgo0_p256(cose_key: &COSEKey) -> Result<String, PeerDidError> {
        use webauthn_rs::prelude::COSEKeyType;

        match &cose_key.key {
            COSEKeyType::EC_EC2(ec2_key) => {
                let x = ec2_key.x.as_ref();
                let y = ec2_key.y.as_ref();

                // For P-256, combine x and y coordinates (32 bytes each)
                let mut public_key = Vec::with_capacity(64);
                public_key.extend_from_slice(x);
                public_key.extend_from_slice(y);

                // Multicodec prefix for P-256: 0x8024
                let mut multicodec_key = vec![0x80, 0x24];
                multicodec_key.extend_from_slice(&public_key);

                let encoded = multibase::encode(multibase::Base::Base58Btc, &multicodec_key);

                Ok(format!("did:peer:0{encoded}"))
            }
            _ => Err(PeerDidError::UnsupportedKeyType),
        }
    }

    /// Generate did:peer:0 from Ed25519 key
    fn generate_numalgo0_ed25519(cose_key: &COSEKey) -> Result<String, PeerDidError> {
        use webauthn_rs::prelude::COSEKeyType;

        match &cose_key.key {
            COSEKeyType::EC_OKP(okp_key) => {
                let public_key = okp_key.x.as_ref();

                // Multicodec prefix for Ed25519: 0xed01
                let mut multicodec_key = vec![0xed, 0x01];
                multicodec_key.extend_from_slice(public_key);

                let encoded = multibase::encode(multibase::Base::Base58Btc, &multicodec_key);

                Ok(format!("did:peer:0{encoded}"))
            }
            _ => Err(PeerDidError::UnsupportedKeyType),
        }
    }

    /// Generate did:peer:2 with multiple keys (advanced)
    ///
    /// This allows creating a did:peer with separate keys for:
    /// - Verification/Authentication (signing)
    /// - Key Agreement (encryption)
    ///
    /// Useful for DIDComm where you need both signing and encryption keys.
    pub fn generate_numalgo2(
        verification_keys: Vec<Vec<u8>>,
        encryption_keys: Vec<Vec<u8>>,
    ) -> Result<String, PeerDidError> {
        let mut parts = vec!["did:peer:2".to_string()];

        // Add verification keys (transform: E)
        for key in verification_keys {
            let encoded = Self::encode_key_with_prefix('E', &key, 0xed)?;
            parts.push(format!(".{encoded}"));
        }

        // Add encryption keys (transform: V)
        for key in encryption_keys {
            let encoded = Self::encode_key_with_prefix('V', &key, 0xec)?;
            parts.push(format!(".{encoded}"));
        }

        Ok(parts.join(""))
    }

    /// Helper: Encode key with multicodec and transform prefix
    fn encode_key_with_prefix(
        transform: char,
        public_key: &[u8],
        multicodec_prefix: u8,
    ) -> Result<String, PeerDidError> {
        let mut multicodec_key = vec![multicodec_prefix, 0x01];
        multicodec_key.extend_from_slice(public_key);

        let encoded = multibase::encode(multibase::Base::Base58Btc, &multicodec_key);

        Ok(format!("{transform}{encoded}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_from_ed25519_bytes() {
        // Test vector: 32-byte Ed25519 public key
        let public_key = [
            0x11, 0xa9, 0x80, 0x01, 0x82, 0xb1, 0x0a, 0xb7, 0xd5, 0x4b, 0xfe, 0xd3, 0xc9, 0x64,
            0x07, 0x3a, 0x0e, 0xe1, 0x72, 0xf3, 0xda, 0xa6, 0x23, 0x25, 0xaf, 0x02, 0x1a, 0x68,
            0xf7, 0x07, 0x51, 0x1a,
        ];

        let did = PeerDidGenerator::from_ed25519_bytes(&public_key).unwrap();

        assert!(did.starts_with("did:peer:0z"));
        assert!(did.len() > 20);

        println!("Generated did:peer: {}", did);
    }

    #[test]
    fn test_generate_from_x25519_bytes() {
        let public_key = [0u8; 32]; // Dummy key for testing

        let did = PeerDidGenerator::from_x25519_bytes(&public_key).unwrap();

        assert!(did.starts_with("did:peer:0z"));
    }

    #[test]
    fn test_invalid_key_length() {
        let short_key = [0u8; 16]; // Too short

        let result = PeerDidGenerator::from_ed25519_bytes(&short_key);

        assert!(result.is_err());
    }
}
