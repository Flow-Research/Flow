use std::path::PathBuf;

use node::bootstrap::config::Config;
use serial_test::serial;

use crate::util::temp_env::TempEnv;

#[test]
#[serial]
fn test_config_validation_with_valid_env() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TempEnv::new();

    // Set up valid environment
    env.set("DATABASE_URL", "sqlite://test.db");
    env.set("DB_MAX_CONNECTIONS", "50");
    env.set("DB_MIN_CONNECTIONS", "10");
    env.set("REST_PORT", "9090");
    env.set("WEBSOCKET_PORT", "9091");
    env.set("KV_STORE_PATH", "/tmp/test-kv");

    let config = Config::from_env()?;

    // Verify all values
    assert_eq!(config.db.url, "sqlite://test.db");
    assert_eq!(config.db.max_connections, 50);
    assert_eq!(config.db.min_connections, 10);
    assert_eq!(config.server.rest_port, 9090);
    assert_eq!(config.server.websocket_port, 9091);
    assert_eq!(config.kv.path, PathBuf::from("/tmp/test-kv"));

    Ok(())
}

#[test]
#[serial]
fn test_config_applies_defaults() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TempEnv::new();

    // Clear optional fields
    env.remove("DB_MAX_CONNECTIONS");
    env.remove("REST_PORT");
    env.remove("KV_STORE_PATH");

    // Only set required field
    env.set("DATABASE_URL", "sqlite://test.db");
    env.set("KV_STORE_PATH", "/tmp/flow-kv");

    let config = Config::from_env()?;

    // Verify defaults are applied
    assert_eq!(
        config.db.max_connections, 100,
        "Should use default max connections"
    );
    assert_eq!(
        config.db.min_connections, 5,
        "Should use default min connections"
    );
    assert_eq!(
        config.server.rest_port, 8080,
        "Should use default REST port"
    );
    assert_eq!(
        config.server.websocket_port, 8081,
        "Should use default WebSocket port"
    );
    assert_eq!(
        config.kv.path,
        PathBuf::from("/tmp/flow-kv"),
        "Should use default KV path"
    );
    assert_eq!(config.server.host, "0.0.0.0", "Should use default host");

    Ok(())
}
