use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, TokenData, Validation};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use thiserror::Error;

static JWT_SECRET: OnceLock<String> = OnceLock::new();

#[derive(Debug, Error)]
pub enum JwtError {
    #[error("JWT encoding failed: {0}")]
    EncodingFailed(#[from] jsonwebtoken::errors::Error),
    #[error("JWT secret not initialized")]
    SecretNotInitialized,
    #[error("Token expired")]
    TokenExpired,
    #[error("Invalid token")]
    InvalidToken,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String,
    pub did: String,
    pub device_id: String,
    pub iat: i64,
    pub exp: i64,
}

impl Claims {
    pub fn new(did: &str, device_id: &str, expiry_hours: i64) -> Self {
        let now = Utc::now();
        Self {
            sub: did.to_string(),
            did: did.to_string(),
            device_id: device_id.to_string(),
            iat: now.timestamp(),
            exp: (now + Duration::hours(expiry_hours)).timestamp(),
        }
    }
}

pub fn init_jwt_secret(secret: &str) {
    let _ = JWT_SECRET.set(secret.to_string());
}

fn get_secret() -> Result<&'static str, JwtError> {
    JWT_SECRET
        .get()
        .map(|s| s.as_str())
        .ok_or(JwtError::SecretNotInitialized)
}

pub fn generate_token(did: &str, device_id: &str) -> Result<String, JwtError> {
    let secret = get_secret()?;
    let claims = Claims::new(did, device_id, 1);

    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )?;

    Ok(token)
}

pub fn generate_token_with_expiry(
    did: &str,
    device_id: &str,
    expiry_hours: i64,
) -> Result<String, JwtError> {
    let secret = get_secret()?;
    let claims = Claims::new(did, device_id, expiry_hours);

    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )?;

    Ok(token)
}

pub fn validate_token(token: &str) -> Result<TokenData<Claims>, JwtError> {
    let secret = get_secret()?;

    let validation = Validation::default();
    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )?;

    Ok(token_data)
}

pub fn extract_did_from_token(token: &str) -> Result<String, JwtError> {
    let token_data = validate_token(token)?;
    Ok(token_data.claims.did)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() {
        init_jwt_secret("test-secret-key-for-testing-purposes-only");
    }

    #[test]
    fn test_generate_and_validate_token() {
        setup();
        let did = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
        let device_id = "test-device-123";

        let token = generate_token(did, device_id).unwrap();
        assert!(!token.is_empty());

        let token_data = validate_token(&token).unwrap();
        assert_eq!(token_data.claims.did, did);
        assert_eq!(token_data.claims.device_id, device_id);
    }

    #[test]
    fn test_extract_did_from_token() {
        setup();
        let did = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
        let device_id = "test-device-123";

        let token = generate_token(did, device_id).unwrap();
        let extracted_did = extract_did_from_token(&token).unwrap();

        assert_eq!(extracted_did, did);
    }

    #[test]
    fn test_invalid_token() {
        setup();
        let result = validate_token("invalid.token.here");
        assert!(result.is_err());
    }
}
