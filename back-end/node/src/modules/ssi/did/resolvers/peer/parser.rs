use super::error::PeerDidError;

#[derive(Debug, Clone)]
pub enum KeyType {
    Ed25519,   // 0xed
    X25519,    // 0xec
    Secp256k1, // 0xe7
    P256,      // 0x8024
}

#[derive(Debug, Clone)]
pub struct VerificationMethod {
    pub key_type: KeyType,
    pub public_key: Vec<u8>,
    pub purpose: Purpose,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Purpose {
    Verification,   // .E
    KeyAgreement,   // .V
    Authentication, // .A (if needed)
}

#[derive(Debug, Clone)]
pub struct ServiceEndpoint {
    pub service_type: String,
    pub endpoint: String,
    pub routing_keys: Vec<String>,
    pub accept: Vec<String>,
}

#[derive(Debug)]
pub struct ParsedPeerDid {
    pub _numalgo: u8,
    pub methods: Vec<VerificationMethod>,
    pub services: Vec<ServiceEndpoint>,
}

impl ParsedPeerDid {
    /// Parse a did:peer string
    pub fn parse(did: &str) -> Result<Self, PeerDidError> {
        if !did.starts_with("did:peer:") {
            return Err(PeerDidError::InvalidFormat);
        }

        let method_specific = &did[9..]; // Skip "did:peer:"

        if method_specific.is_empty() {
            return Err(PeerDidError::InvalidFormat);
        }

        // Parse numalgo (first character)
        let numalgo = method_specific
            .chars()
            .next()
            .and_then(|c| c.to_digit(10))
            .ok_or(PeerDidError::InvalidFormat)? as u8;

        match numalgo {
            0 => Self::parse_numalgo0(&method_specific[1..]),
            2 => Self::parse_numalgo2(&method_specific[1..]),
            _ => Err(PeerDidError::UnsupportedNumalgo(numalgo)),
        }
    }

    /// Parse numalgo:0 (inception key)
    fn parse_numalgo0(encoded: &str) -> Result<Self, PeerDidError> {
        // Format: did:peer:0{multibase-encoded-key}
        let (_base, decoded) =
            multibase::decode(encoded).map_err(|e| PeerDidError::InvalidEncoding(e.to_string()))?;

        if decoded.len() < 2 {
            return Err(PeerDidError::InvalidEncoding("Key too short".to_string()));
        }

        // Extract multicodec prefix
        let key_type = match decoded[0] {
            0xed => KeyType::Ed25519,
            0xec => KeyType::X25519,
            0xe7 => KeyType::Secp256k1,
            0x80 if decoded.len() > 1 && decoded[1] == 0x24 => KeyType::P256,
            _ => return Err(PeerDidError::UnsupportedKeyType),
        };

        // Skip multicodec prefix (1 or 2 bytes)
        let key_start = if matches!(key_type, KeyType::P256) {
            2
        } else {
            2
        };
        let public_key = decoded[key_start..].to_vec();

        Ok(ParsedPeerDid {
            _numalgo: 0,
            methods: vec![VerificationMethod {
                key_type,
                public_key,
                purpose: Purpose::Verification,
            }],
            services: Vec::new(),
        })
    }

    /// Parse numalgo:2 (multiple keys + services)
    fn parse_numalgo2(encoded: &str) -> Result<Self, PeerDidError> {
        // Format: did:peer:2.{transform}{value}.{transform}{value}...
        let mut methods = Vec::new();
        let mut services = Vec::new();

        // Split by dots
        for part in encoded.split('.').filter(|s| !s.is_empty()) {
            if part.is_empty() {
                continue;
            }

            let transform = part.chars().next().ok_or(PeerDidError::InvalidFormat)?;
            let value = &part[1..];

            match transform {
                'E' => {
                    // Verification key
                    let method = Self::decode_key(value, Purpose::Verification)?;
                    methods.push(method);
                }
                'V' => {
                    // Key agreement
                    let method = Self::decode_key(value, Purpose::KeyAgreement)?;
                    methods.push(method);
                }
                'A' => {
                    // Authentication key
                    let method = Self::decode_key(value, Purpose::Authentication)?;
                    methods.push(method);
                }
                'S' => {
                    // Service endpoint
                    let service = Self::decode_service(value)?;
                    services.push(service);
                }
                _ => {
                    // Unknown transform - skip or error
                    tracing::warn!("Unknown transform code: {}", transform);
                }
            }
        }

        Ok(ParsedPeerDid {
            _numalgo: 2,
            methods,
            services,
        })
    }

    fn decode_key(encoded: &str, purpose: Purpose) -> Result<VerificationMethod, PeerDidError> {
        let (_, decoded) =
            multibase::decode(encoded).map_err(|e| PeerDidError::InvalidEncoding(e.to_string()))?;

        if decoded.len() < 2 {
            return Err(PeerDidError::InvalidEncoding("Key too short".to_string()));
        }

        let key_type = match decoded[0] {
            0xed => KeyType::Ed25519,
            0xec => KeyType::X25519,
            0xe7 => KeyType::Secp256k1,
            0x80 if decoded.len() > 1 && decoded[1] == 0x24 => KeyType::P256,
            _ => return Err(PeerDidError::UnsupportedKeyType),
        };

        let key_start = if matches!(key_type, KeyType::P256) {
            2
        } else {
            2
        };
        let public_key = decoded[key_start..].to_vec();

        Ok(VerificationMethod {
            key_type,
            public_key,
            purpose,
        })
    }

    fn decode_service(encoded: &str) -> Result<ServiceEndpoint, PeerDidError> {
        use base64::Engine;

        // Service is base64url encoded JSON
        let decoded =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(encoded.as_bytes())?;

        let json_str =
            String::from_utf8(decoded).map_err(|e| PeerDidError::InvalidEncoding(e.to_string()))?;

        let service: serde_json::Value = serde_json::from_str(&json_str)?;

        Ok(ServiceEndpoint {
            service_type: service["t"].as_str().unwrap_or("").to_string(),
            endpoint: service["s"].as_str().unwrap_or("").to_string(),
            routing_keys: service["r"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            accept: service["a"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
        })
    }
}
