use dotenvy::dotenv;
use errors::AppError;
use std::str::FromStr;
use std::{env, time::Duration};

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub rest_port: u16,
    pub websocket_port: u16,
    pub host: String,
}

#[derive(Debug, Clone)]
pub struct DbConfig {
    pub url: String,
    pub max_connections: u32,
    pub min_connections: u32,
    pub connect_timeout: Duration,
    pub idle_timeout: Duration,
    pub max_lifetime: Duration,
    pub logging_enabled: bool,
}

#[derive(Debug, Clone)]
pub struct KvConfig {
    pub path: String,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub db: DbConfig,
    pub kv: KvConfig,
    pub server: ServerConfig,
}

impl Config {
    pub fn from_env() -> Result<Self, AppError> {
        dotenv().ok();

        // Helper to parse a u64
        let get_env_u64 = |key: &str, default: u64| -> Result<u64, AppError> {
            Ok(env::var(key)
                .ok()
                .map(|s| s.parse::<u64>())
                .transpose()
                .map_err(|_| AppError::Config(format!("Invalid value for {key}")))?
                .unwrap_or(default))
        };

        // --- helper to parse a boolean ---
        let get_env_bool = |key: &str, default: bool| -> Result<bool, AppError> {
            Ok(env::var(key)
                .ok()
                .map(|s| bool::from_str(&s.to_lowercase()))
                .transpose()
                .map_err(|_| AppError::Config(format!("Invalid boolean value for {key}")))?
                .unwrap_or(default))
        };

        // --- Parse all variables ---

        // DbConfig
        let database_url = env::var("DATABASE_URL")
            .map_err(|_| AppError::Config("DATABASE_URL must be set".to_string()))?;

        let max_connections = get_env_u64("DB_MAX_CONNECTIONS", 100)? as u32;
        let min_connections = get_env_u64("DB_MIN_CONNECTIONS", 5)? as u32;
        let connect_timeout_secs = get_env_u64("DB_CONNECT_TIMEOUT", 8)?;
        let idle_timeout_secs = get_env_u64("DB_IDLE_TIMEOUT", 600)?;
        let max_lifetime_secs = get_env_u64("DB_MAX_LIFETIME", 1800)?;
        let logging_enabled = get_env_bool("DB_LOGGING_ENABLED", false)?; // <-- Parse the new variable

        // KvConfig
        let kv_path = env::var("KV_STORE_PATH").unwrap_or("/tmp/flow-kv".to_string());

        // ServerConfig
        let rest_port = get_env_u64("REST_PORT", 8080)? as u16;
        let websocket_port = get_env_u64("WEBSOCKET_PORT", 8081)? as u16;
        let host = env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());

        Ok(Self {
            db: DbConfig {
                url: database_url,
                max_connections,
                min_connections,
                connect_timeout: Duration::from_secs(connect_timeout_secs),
                idle_timeout: Duration::from_secs(idle_timeout_secs),
                max_lifetime: Duration::from_secs(max_lifetime_secs),
                logging_enabled,
            },
            kv: KvConfig { path: kv_path },
            server: ServerConfig {
                rest_port,
                websocket_port,
                host,
            },
        })
    }
}
