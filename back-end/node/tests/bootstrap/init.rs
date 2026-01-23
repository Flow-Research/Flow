use axum::Router;
use chrono::{DateTime, Utc};
use errors::AppError;
use migration::{Migrator, MigratorTrait};
use node::api::node::Node;
use node::api::servers::app_state::AppState;
use node::api::servers::rest;
use node::bootstrap::config::{Config, CorsConfig, DbConfig, JwtConfig, ServerConfig};
use node::bootstrap::init::NodeData;
use node::bootstrap::init::{AuthMetadata, initialize_config_dir};
use node::modules::ai::config::IndexingConfig;
use node::modules::ai::pipeline_manager::PipelineManager;
use node::modules::network::manager::NetworkManager;
use node::modules::ssi::webauthn::state::AuthState;
use node::modules::storage::{KvConfig, RocksDbKvStore};
use once_cell::sync::Lazy;
use sea_orm::{ConnectOptions, Database, DatabaseConnection};
use std::collections::hash_map::RandomState;
use std::fs;
use std::hash::{BuildHasher, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use tempfile::TempDir;
use tracing::info;

// Global mutex to serialize NetworkManager creation (env vars are process-global)
pub static NETWORK_MANAGER_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

/// Test server container with access to all components
pub struct TestServer {
    pub router: Router,
    pub node: Node,
    pub temp: TempDir,
}

pub fn create_test_node_data() -> NodeData {
    let signing_key = ed25519_dalek::SigningKey::generate(&mut rand::rngs::OsRng);
    let verifying_key = signing_key.verifying_key();

    NodeData {
        id: format!("did:key:test-{}", rand::random::<u32>()),
        private_key: signing_key.to_bytes().to_vec(),
        public_key: verifying_key.to_bytes().to_vec(),
    }
}

/// Create test node data with custom device ID
fn create_test_node_data_with_device_id(device_id: &str) -> NodeData {
    NodeData {
        id: device_id.to_string(),
        private_key: vec![0u8; 32],
        public_key: vec![0u8; 32],
    }
}

/// Setup test environment with auto-generated unique name
pub fn setup_test_env_auto(temp_dir: &TempDir) {
    let random_suffix = RandomState::new().build_hasher().finish().to_string();
    setup_test_env(temp_dir, &random_suffix);
}

pub fn setup_test_env(temp_dir: &TempDir, test_name: &str) {
    unsafe {
        std::env::set_var(
            "DHT_DB_PATH",
            temp_dir.path().join(format!("{}_dht", test_name)),
        );
        std::env::set_var(
            "PEER_REGISTRY_DB_PATH",
            temp_dir.path().join(format!("{}_registry", test_name)),
        );
        std::env::set_var(
            "GOSSIP_MESSAGE_DB_PATH",
            temp_dir.path().join(format!("{}_gossip", test_name)),
        );
        std::env::set_var(
            "BLOCK_STORE_PATH",
            temp_dir.path().join(format!("{}_blocks", test_name)),
        );
        std::env::set_var(
            "PROVIDER_REGISTRY_DB_PATH",
            temp_dir
                .path()
                .join(format!("{}_provider_registry", test_name)),
        );
    }
}

pub fn set_env(key: &str, value: &str) {
    unsafe {
        std::env::set_var(key, value);
    }
}

pub fn set_envs(envs: &Vec<(&str, &str)>) {
    for env in envs {
        unsafe {
            std::env::set_var(env.0, env.1);
        }
    }
}

pub fn remove_env(key: &str) {
    unsafe {
        std::env::remove_var(key);
    }
}

/// Remove multiple environment variables
pub fn remove_envs(keys: &[&str]) {
    for key in keys {
        unsafe {
            std::env::remove_var(key);
        }
    }
}

/// Setup a test server with app state
pub async fn setup_test_server() -> TestServer {
    let (node, temp) = setup_test_node().await;
    let node_clone = node.clone();
    let app_state = AppState::new(node);
    let config = create_test_config(&temp);
    node::modules::ssi::jwt::init_jwt_secret(&config.jwt.secret);
    let router = rest::build_router(app_state, &config);

    TestServer {
        router,
        node: node_clone,
        temp,
    }
}

fn create_test_config(temp: &TempDir) -> Config {
    Config {
        db: DbConfig {
            url: format!("sqlite://{}?mode=rwc", temp.path().join("test.db").display()),
            max_connections: 50,
            min_connections: 1,
            connect_timeout: Duration::from_secs(8),
            idle_timeout: Duration::from_secs(600),
            max_lifetime: Duration::from_secs(1800),
            logging_enabled: false,
        },
        kv: KvConfig {
            path: temp.path().join("kv"),
            enable_compression: true,
            max_open_files: 100,
            write_buffer_size: 64 * 1024 * 1024,
        },
        server: ServerConfig {
            rest_port: 8080,
            websocket_port: 8081,
            host: "0.0.0.0".to_string(),
        },
        jwt: JwtConfig {
            secret: "test-secret-key".to_string(),
            expiry_hours: 1,
        },
        cors: CorsConfig {
            allowed_origins: vec!["http://localhost:3000".to_string()],
            allow_credentials: true,
        },
    }
}

// Helper to create test Node
pub async fn setup_test_node() -> (Node, TempDir) {
    setup_test_node_with_device_id("test-node--").await
}

/// Setup a test database with migrations
async fn setup_test_database(temp_dir: &TempDir) -> DatabaseConnection {
    let db_path = temp_dir.path().join("test.db");
    let db_url = format!("sqlite://{}?mode=rwc", db_path.display());

    let mut opt = ConnectOptions::new(&db_url);
    opt.max_connections(50).min_connections(40);

    let db = Database::connect(opt).await.unwrap();
    Migrator::up(&db, None).await.unwrap();

    db
}

/// Setup a test KV store with RocksDB
fn setup_test_kv_store(temp_dir: &TempDir) -> Result<Arc<RocksDbKvStore>, AppError> {
    info!("Setting up KV Store with RocksDB");

    let kv_path = temp_dir.path().join("kv");

    let kv_config = KvConfig {
        path: kv_path.clone().into(),
        enable_compression: true,
        max_open_files: 1000,
        write_buffer_size: 64 * 1024 * 1024, // 64MB
    };

    let store = RocksDbKvStore::new(&kv_config.path, &kv_config)
        .map_err(|e| AppError::Storage(format!("Failed to initialize KV store: {}", e).into()))?;

    Ok(Arc::new(store))
}

/// Setup a test KV store with RocksDB
pub fn setup_kv_from_path(path: &Path) -> Result<Arc<RocksDbKvStore>, AppError> {
    info!("Setting up KV Store with RocksDB");

    let kv_path = path.join("kv");

    let kv_config = KvConfig {
        path: kv_path.clone().into(),
        enable_compression: true,
        max_open_files: 1000,
        write_buffer_size: 64 * 1024 * 1024, // 64MB
    };

    let store = RocksDbKvStore::new(&kv_config.path, &kv_config)
        .map_err(|e| AppError::Storage(format!("Failed to initialize KV store: {}", e).into()))?;

    Ok(Arc::new(store))
}

/// Setup test auth state
fn setup_test_auth_state() -> AuthState {
    let auth_config = node::modules::ssi::webauthn::state::AuthConfig {
        rp_id: "localhost".to_string(),
        rp_origin: "http://localhost:3000".to_string(),
        rp_name: "Test Flow".to_string(),
    };

    AuthState::new(auth_config).unwrap()
}

/// Setup test pipeline manager
fn setup_test_pipeline_manager(db: DatabaseConnection) -> Result<PipelineManager, AppError> {
    let indexing_config = IndexingConfig::from_env()
        .map_err(|_| AppError::Internal("Failed to initialize IndexingConfig".to_string()))?;

    Ok(PipelineManager::new(db, indexing_config))
}

/// Setup test network manager
async fn setup_test_network_manager(
    node_data: &NodeData,
    temp_dir: &TempDir,
) -> Result<Arc<NetworkManager>, Box<dyn std::error::Error>> {
    // Hold the lock across env var setting AND NetworkManager creation
    // This prevents parallel tests from overwriting each other's env vars
    let _guard = NETWORK_MANAGER_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());

    // Set environment variables for this test's unique paths
    setup_test_env_auto(temp_dir);

    let network_manager = NetworkManager::new(node_data).await?;
    Ok(Arc::new(network_manager))
}

/// Setup a test node with a custom device ID
pub async fn setup_test_node_with_device_id(device_id: &str) -> (Node, TempDir) {
    let temp_dir = TempDir::new().unwrap();

    // Setup all components
    let db = setup_test_database(&temp_dir).await;
    let kv = setup_test_kv_store(&temp_dir).unwrap();
    let auth_state = setup_test_auth_state();
    let node_data = create_test_node_data_with_device_id(device_id);
    let pipeline_manager = setup_test_pipeline_manager(db.clone()).unwrap();
    let network_manager = setup_test_network_manager(&node_data, &temp_dir)
        .await
        .unwrap();

    // Create and return the node
    let node = Node::new(
        node_data,
        db,
        kv,
        auth_state,
        pipeline_manager,
        network_manager,
    );

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
pub async fn create_test_node_with_db(
    device_id: &str,
    db: DatabaseConnection,
    kv_path: &Path,
) -> Node {
    // Setup all components using existing helpers
    let kv = setup_kv_from_path(kv_path).unwrap();
    let auth_state = setup_test_auth_state();
    let node_data = create_test_node_data_with_device_id(device_id);
    let pipeline_manager = setup_test_pipeline_manager(db.clone()).unwrap();
    let network_temp = TempDir::new().unwrap();
    let network_manager = setup_test_network_manager(&node_data, &network_temp)
        .await
        .unwrap();

    // Create and return the node
    Node::new(
        node_data,
        db,
        kv,
        auth_state,
        pipeline_manager,
        network_manager,
    )
}

fn compute_did_from_pubkey(pub_key_bytes: &[u8]) -> String {
    // multicodec prefix for ed25519-pub: 0xED 0x01 (varint encoded)
    let mut multicodec_key = Vec::with_capacity(2 + pub_key_bytes.len());
    multicodec_key.extend_from_slice(&[0xED, 0x01]);
    multicodec_key.extend_from_slice(pub_key_bytes);
    let pub_key_multibase = multibase::encode(multibase::Base::Base58Btc, &multicodec_key);
    format!("did:key:{}", pub_key_multibase)
}

use node::modules::network::config::NetworkConfig;
use node::modules::storage::content::{BlockStore, BlockStoreConfig};

// ============================================================================
// Two-Node Network Test Infrastructure
// ============================================================================

/// Configuration for setting up a two-node test network
pub struct TwoNodeTestConfig {
    pub provider_port: u16,
    pub seeker_port: u16,
    pub provider_prefix: String,
    pub seeker_prefix: String,
}

impl TwoNodeTestConfig {
    pub fn new(provider_port: u16, seeker_port: u16) -> Self {
        let provider_suffix = format!("_prov_{}", rand::random::<u32>());
        let seeker_suffix = format!("_seek_{}", rand::random::<u32>());
        Self {
            provider_port,
            seeker_port,
            provider_prefix: provider_suffix,
            seeker_prefix: seeker_suffix,
        }
    }

    /// Ports 9300/9301 - for network_provider tests
    pub fn network_provider() -> Self {
        Self::new(9300, 9301)
    }

    /// Ports 9400/9401 - for dag network tests
    pub fn dag_network() -> Self {
        Self::new(9400, 9401)
    }

    pub fn content_service() -> Self {
        Self::new(9600, 9601)
    }
}

/// Context for two-node test setup
pub struct TwoNodeTestContext {
    pub provider_manager: Arc<NetworkManager>,
    pub seeker_manager: Arc<NetworkManager>,
    pub provider_store: Arc<BlockStore>,
    pub seeker_store: Arc<BlockStore>,
    pub _provider_dir: TempDir,
    pub _seeker_dir: TempDir,
}

pub async fn setup_two_node_network(config: TwoNodeTestConfig) -> TwoNodeTestContext {
    // Acquire lock to prevent race conditions with env vars and RocksDB
    let _guard = NETWORK_MANAGER_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());

    let provider_dir = TempDir::new().expect("Failed to create provider temp dir");
    let seeker_dir = TempDir::new().expect("Failed to create seeker temp dir");

    // ========================================================================
    // Setup provider node
    // ========================================================================
    setup_test_env(&provider_dir, &config.provider_prefix);
    set_env("CONTENT_TRANSFER_ENABLED", "true");

    let provider_store_config = BlockStoreConfig {
        db_path: provider_dir.path().join("blocks"),
        ..Default::default()
    };
    let provider_store =
        Arc::new(BlockStore::new(provider_store_config).expect("Failed to create provider store"));

    let provider_data = create_test_node_data();
    let provider_manager = NetworkManager::with(&provider_data, Some(Arc::clone(&provider_store)))
        .await
        .expect("Failed to create provider manager");
    let provider_manager = Arc::new(provider_manager);

    // Start provider
    let mut provider_config = NetworkConfig::default();
    provider_config.listen_port = config.provider_port;
    provider_config.mdns.enabled = false;
    provider_manager
        .start(&provider_config)
        .await
        .expect("Failed to start provider");

    let provider_addr: libp2p::Multiaddr = format!(
        "/ip4/127.0.0.1/udp/{}/quic-v1/p2p/{}",
        config.provider_port,
        provider_manager.local_peer_id()
    )
    .parse()
    .unwrap();

    // ========================================================================
    // Setup seeker node
    // ========================================================================
    setup_test_env(&seeker_dir, &config.seeker_prefix);
    set_env("CONTENT_TRANSFER_ENABLED", "true");

    let seeker_store_config = BlockStoreConfig {
        db_path: seeker_dir.path().join("blocks"),
        ..Default::default()
    };
    let seeker_store =
        Arc::new(BlockStore::new(seeker_store_config).expect("Failed to create seeker store"));

    let seeker_data = create_test_node_data();
    let seeker_manager = NetworkManager::with(&seeker_data, Some(Arc::clone(&seeker_store)))
        .await
        .expect("Failed to create seeker manager");
    let seeker_manager = Arc::new(seeker_manager);

    // Start seeker with provider as bootstrap
    let mut seeker_config = NetworkConfig::default();
    seeker_config.listen_port = config.seeker_port;
    seeker_config.bootstrap_peers = vec![provider_addr];
    seeker_manager
        .start(&seeker_config)
        .await
        .expect("Failed to start seeker");

    // Wait for connection and DHT sync
    tokio::time::sleep(Duration::from_secs(2)).await;

    TwoNodeTestContext {
        provider_manager,
        seeker_manager,
        provider_store,
        seeker_store,
        _provider_dir: provider_dir,
        _seeker_dir: seeker_dir,
    }
}

pub async fn cleanup_two_node_network(ctx: TwoNodeTestContext) {
    // Stop managers first (releases their references to BlockStore and RocksDB)
    let _ = ctx.seeker_manager.stop().await;
    let _ = ctx.provider_manager.stop().await;

    tokio::time::sleep(Duration::from_millis(50)).await;

    drop(ctx.seeker_store);
    drop(ctx.provider_store);
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
