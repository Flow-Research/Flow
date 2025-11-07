use axum::Router;
use chrono::{DateTime, Utc};
use errors::AppError;
use node::api::servers::app_state::AppState;
use node::api::servers::rest;
use node::bootstrap::init::{AuthMetadata, initialize_config_dir};
use node::modules::ai_pipeline::PipelineManager;
use node::modules::ai_pipeline::index::config::IndexingConfig;
use std::fs;
use std::path::{Path, PathBuf};

use migration::{Migrator, MigratorTrait};
use node::api::node::Node;
use node::bootstrap::init::NodeData;
use node::modules::ssi::webauthn::state::AuthState;
use sea_orm::{Database, DatabaseConnection};
use tempfile::TempDir;

/// Test server container with access to all components
pub struct TestServer {
    pub router: Router,
    pub node: Node,
    pub temp: TempDir,
}

/// Setup a test server with app state
pub async fn setup_test_server() -> TestServer {
    let (node, temp) = setup_test_node().await;
    let node_clone = node.clone();
    let app_state = AppState::new(node);
    let router = rest::build_router(app_state);

    TestServer {
        router,
        node: node_clone,
        temp,
    }
}

// Helper to create test Node
pub async fn setup_test_node() -> (Node, TempDir) {
    setup_test_node_with_device_id("test-node--").await
}

/// Setup a test node with a custom device ID
pub async fn setup_test_node_with_device_id(device_id: &str) -> (Node, TempDir) {
    let temp_dir = TempDir::new().unwrap();

    // Setup database
    let db_path = temp_dir.path().join("test.db");
    let db_url = format!("sqlite://{}?mode=rwc", db_path.display());
    let db = Database::connect(&db_url).await.unwrap();
    Migrator::up(&db, None).await.unwrap();

    // Setup KV store
    let kv_path = temp_dir.path().join("kv");
    let kv = sled::open(&kv_path).unwrap();

    // Setup auth state
    let auth_config = node::modules::ssi::webauthn::state::AuthConfig {
        rp_id: "localhost".to_string(),
        rp_origin: "http://localhost:3000".to_string(),
        rp_name: "Test Flow".to_string(),
    };
    let auth_state = AuthState::new(auth_config).unwrap();

    // Create node data with custom device_id
    let node_data = NodeData {
        id: device_id.to_string(),
        private_key: vec![0u8; 32],
        public_key: vec![0u8; 32],
    };

    let indexing_config = IndexingConfig::from_env()
        .map_err(|_| AppError::Internal("Failed to initialize IndexingConfig".to_string()))
        .unwrap();
    let pipeline_manager = PipelineManager::new(db.clone(), indexing_config);

    let node = Node::new(node_data, db, kv, auth_state, pipeline_manager);

    (node, temp_dir)
}

/// Setup just a test database (no node) - useful for testing storage functions
pub async fn setup_test_db() -> (DatabaseConnection, TempDir) {
    let temp_dir = TempDir::new().unwrap();

    let db_path = temp_dir.path().join("test.db");
    let db_url = format!("sqlite://{}?mode=rwc", db_path.display());
    let db = Database::connect(&db_url).await.unwrap();
    Migrator::up(&db, None).await.unwrap();

    (db, temp_dir)
}

/// Setup test infrastructure for multiple nodes sharing the same database
pub async fn setup_test_multi_node() -> (DatabaseConnection, TempDir) {
    setup_test_db().await
}

/// Create a node with custom device_id using existing database
pub fn create_test_node_with_db(device_id: &str, db: DatabaseConnection, kv_path: &Path) -> Node {
    let kv = sled::open(kv_path).unwrap();

    let auth_config = node::modules::ssi::webauthn::state::AuthConfig {
        rp_id: "localhost".to_string(),
        rp_origin: "http://localhost:3000".to_string(),
        rp_name: "Test Flow".to_string(),
    };
    let auth_state = AuthState::new(auth_config).unwrap();

    let node_data = NodeData {
        id: device_id.to_string(),
        private_key: vec![0u8; 32],
        public_key: vec![0u8; 32],
    };

    let indexing_config = IndexingConfig::from_env()
        .map_err(|_| AppError::Internal("Failed to initialize IndexingConfig".to_string()))
        .unwrap();
    let pipeline_manager = PipelineManager::new(db.clone(), indexing_config);

    Node::new(node_data, db, kv, auth_state, pipeline_manager)
}

fn compute_did_from_pubkey(pub_key_bytes: &[u8]) -> String {
    // multicodec prefix for ed25519-pub: 0xED 0x01 (varint encoded)
    let mut multicodec_key = Vec::with_capacity(2 + pub_key_bytes.len());
    multicodec_key.extend_from_slice(&[0xED, 0x01]);
    multicodec_key.extend_from_slice(pub_key_bytes);
    let pub_key_multibase = multibase::encode(multibase::Base::Base58Btc, &multicodec_key);
    format!("did:key:{}", pub_key_multibase)
}

#[test]
fn bootstrap_first_run_creates_files_and_auth() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::TempDir::new()?;
    let config_dir: PathBuf = tmp.path().join("flow-config");

    // Run bootstrap
    let _node_data = initialize_config_dir(config_dir.to_string_lossy().into_owned().as_str())?;

    // Paths
    let keystore = config_dir.join("keystore");
    let auth_file = config_dir.join("auth.json");
    let priv_key_file = keystore.join("ed25519.priv");
    let pub_key_file = keystore.join("ed25519.pub");

    // Existence
    assert!(config_dir.is_dir());
    assert!(keystore.is_dir());
    assert!(auth_file.is_file());
    assert!(priv_key_file.is_file());
    assert!(pub_key_file.is_file());

    // Key sizes
    let priv_key = fs::read(&priv_key_file)?;
    let pub_key = fs::read(&pub_key_file)?;
    assert_eq!(priv_key.len(), 32);
    assert_eq!(pub_key.len(), 32);

    // auth.json content
    let auth_json = fs::read_to_string(&auth_file)?;
    let auth: AuthMetadata = serde_json::from_str(&auth_json)?;

    assert_eq!(auth.schema, "flow-auth/v1");
    // createdAt parseable
    let _ts: DateTime<Utc> = auth.created_at.parse()?;

    // DID derived from pubkey matches
    let expected_did = compute_did_from_pubkey(&pub_key);
    assert_eq!(auth.did, expected_did);

    // pubKeyMultibase in auth must match the suffix of DID
    let did_suffix = auth.did.strip_prefix("did:key:").unwrap_or("");
    assert_eq!(did_suffix, auth.pub_key_multibase);

    // On Unix, verify permissions
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let ks_mode = fs::metadata(&keystore)?.permissions().mode() & 0o777;
        assert_eq!(ks_mode, 0o700);

        let priv_mode = fs::metadata(&priv_key_file)?.permissions().mode() & 0o777;
        assert_eq!(priv_mode, 0o600);

        let pub_mode = fs::metadata(&pub_key_file)?.permissions().mode() & 0o777;
        assert_eq!(pub_mode, 0o644);
    }

    Ok(())
}

#[test]
fn subsequent_run_loads_existing_without_change() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::TempDir::new()?;
    let config_dir: PathBuf = tmp.path().join("flow-config");

    // First run
    initialize_config_dir(config_dir.to_string_lossy().into_owned().as_str())?;

    // Snapshot current state
    let keystore = config_dir.join("keystore");
    let auth_file = config_dir.join("auth.json");
    let priv_key_file = keystore.join("ed25519.priv");
    let pub_key_file = keystore.join("ed25519.pub");

    let auth_json_1 = fs::read(&auth_file)?;
    let priv_key_1 = fs::read(&priv_key_file)?;
    let pub_key_1 = fs::read(&pub_key_file)?;

    // Second run should load existing state (not regenerate)
    initialize_config_dir(config_dir.to_string_lossy().into_owned().as_str())?;

    let auth_json_2 = fs::read(&auth_file)?;
    let priv_key_2 = fs::read(&priv_key_file)?;
    let pub_key_2 = fs::read(&pub_key_file)?;

    // Keys and auth should be unchanged
    assert_eq!(priv_key_1, priv_key_2);
    assert_eq!(pub_key_1, pub_key_2);
    assert_eq!(auth_json_1, auth_json_2);

    Ok(())
}

#[test]
#[cfg(unix)]
fn test_bootstrap_with_invalid_permissions() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;

    let tmp = tempfile::TempDir::new()?;
    let config_dir = tmp.path().join("flow-config");

    // Create config dir but make it read-only
    fs::create_dir(&config_dir)?;
    let mut perms = fs::metadata(&config_dir)?.permissions();
    perms.set_mode(0o444); // Read-only
    fs::set_permissions(&config_dir, perms)?;

    // Attempt bootstrap - should fail because we can't create keystore
    let result = initialize_config_dir(config_dir.to_string_lossy().as_ref());

    assert!(
        result.is_err(),
        "Bootstrap should fail with read-only directory"
    );

    // Verify error message mentions permissions
    if let Err(e) = result {
        let error_msg = e.to_string().to_lowercase();
        assert!(
            error_msg.contains("permission") || error_msg.contains("denied"),
            "Error should mention permissions: {}",
            e
        );
    }

    // Cleanup: restore permissions so temp dir can be deleted
    let mut perms = fs::metadata(&config_dir)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&config_dir, perms)?;

    Ok(())
}

#[test]
#[cfg(unix)]
fn test_keystore_has_correct_permissions() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;

    let tmp = tempfile::TempDir::new()?;
    let config_dir = tmp.path().join("flow-config");

    // Run bootstrap
    initialize_config_dir(config_dir.to_string_lossy().as_ref())?;

    let keystore = config_dir.join("keystore");
    let priv_key = keystore.join("ed25519.priv");
    let pub_key = keystore.join("ed25519.pub");

    // Verify keystore directory is 0o700 (owner only)
    let ks_mode = fs::metadata(&keystore)?.permissions().mode() & 0o777;
    assert_eq!(ks_mode, 0o700, "Keystore should be 0o700");

    // Verify private key is 0o600 (owner read/write only)
    let priv_mode = fs::metadata(&priv_key)?.permissions().mode() & 0o777;
    assert_eq!(priv_mode, 0o600, "Private key should be 0o600");

    // Verify public key is 0o644 (owner read/write, others read)
    let pub_mode = fs::metadata(&pub_key)?.permissions().mode() & 0o777;
    assert_eq!(pub_mode, 0o644, "Public key should be 0o644");

    Ok(())
}

#[test]
fn test_bootstrap_with_corrupted_auth_file() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::TempDir::new()?;
    let config_dir = tmp.path().join("flow-config");

    // First, create a valid bootstrap
    initialize_config_dir(config_dir.to_string_lossy().as_ref())?;

    // Now corrupt the auth.json file
    let auth_file = config_dir.join("auth.json");
    fs::write(&auth_file, b"{ invalid json }")?;

    // Attempt to load - should fail
    let result = initialize_config_dir(config_dir.to_string_lossy().as_ref());

    assert!(result.is_err(), "Should fail with corrupted auth file");

    if let Err(e) = result {
        let error_msg = e.to_string();
        assert!(
            error_msg.contains("loading existing") || error_msg.contains("parse"),
            "Error should mention parsing issue: {}",
            e
        );
    }

    Ok(())
}

#[test]
fn test_bootstrap_with_missing_fields_in_auth() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::TempDir::new()?;
    let config_dir = tmp.path().join("flow-config");
    fs::create_dir_all(&config_dir)?;

    let auth_file = config_dir.join("auth.json");

    // Write auth.json with missing 'did' field
    let incomplete_auth = r#"{
        "schema": "flow-auth/v1",
        "created_at": "2025-01-08T00:00:00Z",
        "pub_key_multibase": "z6Mkp..."
    }"#;

    fs::write(&auth_file, incomplete_auth)?;

    // Create keystore with dummy keys
    let keystore = config_dir.join("keystore");
    fs::create_dir_all(&keystore)?;
    fs::write(keystore.join("ed25519.priv"), &[0u8; 32])?;
    fs::write(keystore.join("ed25519.pub"), &[0u8; 32])?;

    // Attempt to load - should fail
    let result = initialize_config_dir(config_dir.to_string_lossy().as_ref());

    assert!(result.is_err(), "Should fail with missing fields");

    Ok(())
}

#[test]
fn test_bootstrap_with_empty_auth_file() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::TempDir::new()?;
    let config_dir = tmp.path().join("flow-config");
    fs::create_dir_all(&config_dir)?;

    let auth_file = config_dir.join("auth.json");
    fs::write(&auth_file, b"")?; // Empty file

    let result = initialize_config_dir(config_dir.to_string_lossy().as_ref());

    assert!(result.is_err(), "Should fail with empty auth file");

    Ok(())
}

#[tokio::test]
async fn test_bootstrap_concurrent_initialization() -> Result<(), Box<dyn std::error::Error>> {
    use std::sync::Arc;
    use tokio::task;

    let tmp = tempfile::TempDir::new()?;
    let config_dir = Arc::new(
        tmp.path()
            .join("flow-config")
            .to_string_lossy()
            .into_owned(),
    );

    // Spawn 10 concurrent initialization attempts
    let mut handles = vec![];

    for i in 0..10 {
        let dir = Arc::clone(&config_dir);
        let handle = task::spawn_blocking(move || {
            let result = initialize_config_dir(&dir);
            (i, result)
        });
        handles.push(handle);
    }

    // Wait for all to complete
    let results: Vec<_> = futures_util::future::join_all(handles)
        .await
        .into_iter()
        .map(|r| r.unwrap())
        .collect();

    // Count successes
    let successes: Vec<_> = results.iter().filter(|(_, r)| r.is_ok()).collect();

    // At least one should succeed
    assert!(
        !successes.is_empty(),
        "At least one initialization should succeed"
    );

    // Verify all successful results have the same DID
    let dids: Vec<String> = successes
        .iter()
        .map(|(_, r)| r.as_ref().unwrap().id.clone())
        .collect();

    let first_did = &dids[0];
    for did in &dids {
        assert_eq!(
            did, first_did,
            "All successful initializations should produce the same DID"
        );
    }

    // Verify files exist and are valid
    let config_path = PathBuf::from(config_dir.as_ref());
    assert!(config_path.join("auth.json").exists());
    assert!(config_path.join("keystore/ed25519.priv").exists());
    assert!(config_path.join("keystore/ed25519.pub").exists());

    Ok(())
}
