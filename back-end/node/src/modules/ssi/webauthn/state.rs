use errors::AppError;
use std::sync::Arc;
use tracing::info;
use webauthn_rs::prelude::*;

#[derive(Clone)]
pub struct AuthState {
    pub webauthn: Arc<Webauthn>,
}

/// Configuration for WebAuthn authentication
#[derive(Debug, Clone)]
pub struct AuthConfig {
    /// Relying Party ID
    pub rp_id: String,
    /// Relying Party origin URL
    pub rp_origin: String,
    /// Relying Party display name
    pub rp_name: String,
}

impl AuthConfig {
    /// Load authentication configuration from environment variables
    pub fn from_env() -> Result<Self, AppError> {
        use std::env;

        let rp_id = env::var("WEBAUTHN_RP_ID").unwrap_or_else(|_| "localhost".to_string());

        let rp_origin =
            env::var("WEBAUTHN_RP_ORIGIN").unwrap_or_else(|_| "http://localhost:8080".to_string());

        let rp_name = env::var("WEBAUTHN_RP_NAME").unwrap_or_else(|_| "Flow WebAuthn".to_string());

        Ok(Self {
            rp_id,
            rp_origin,
            rp_name,
        })
    }
}

impl AuthState {
    /// Create a new AuthState from configuration
    pub fn new(config: AuthConfig) -> Result<Self, AppError> {
        info!("Initializing WebAuthn authstate");

        let rp_origin = Url::parse(&config.rp_origin)
            .map_err(|e| AppError::Config(format!("Invalid WebAuthn origin URL: {}", e)))?;

        let builder = WebauthnBuilder::new(&config.rp_id, &rp_origin)
            .map_err(|e| AppError::Config(format!("Invalid WebAuthn configuration: {}", e)))?;

        let builder = builder.rp_name(&config.rp_name);

        let webauthn = Arc::new(
            builder
                .build()
                .map_err(|e| AppError::Config(format!("Failed to build WebAuthn: {}", e)))?,
        );

        Ok(AuthState { webauthn })
    }

    /// Create a new AuthState from environment variables
    pub fn from_env() -> Result<Self, AppError> {
        let config = AuthConfig::from_env()?;
        Self::new(config)
    }
}
