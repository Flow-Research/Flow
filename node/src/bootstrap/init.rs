use directories::BaseDirs;
use std::path::{Path, PathBuf};
use std::{env, error::Error, fs, io::Write};

use errors::AppError;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use ed25519_dalek::SigningKey;
use multibase::Base;

pub struct NodeData {
    pub id: String,
    pub private_key: Vec<u8>,
    pub public_key: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AuthMetadata {
    pub schema: String,
    pub did: String,
    pub created_at: String,
    pub pub_key_multibase: String,
}

struct Paths {
    config_dir: PathBuf,
    keystore_dir: PathBuf,
    auth_file: PathBuf,
    priv_key_file: PathBuf,
    pub_key_file: PathBuf,
}

pub fn initialize() -> Result<NodeData, AppError> {
    let config_dir = get_flow_config_dir();
    initialize_config_dir(&config_dir)
}

pub fn get_flow_config_dir() -> String {
    if let Ok(p) = env::var("FLOW_CONFIG_HOME") {
        return p;
    }

    if let Some(b) = BaseDirs::new() {
        // Linux: ~/.config/flow
        // macOS: ~/Library/Application Support/flow
        // Windows: %APPDATA%\flow
        return b.config_dir().join("flow").to_string_lossy().into_owned();
    }

    // Fallback if BaseDirs fails (very rare)
    "flow".to_string()
}

pub fn initialize_config_dir(dir: &str) -> Result<NodeData, AppError> {
    let p = paths(dir);
    let _created = create_directory(&p.config_dir)
        .map_err(|e| AppError::Bootstrap(format!("Failed to create directory. {}", e)))?;

    if p.auth_file.exists() {
        return load_existing(&p).map_err(|e| {
            AppError::Bootstrap(format!(
                "Error while loading existing configurations. {}",
                e
            ))
        });
    }

    bootstrap_new(&p)
}

fn paths(dir: &str) -> Paths {
    let config_dir = PathBuf::from(dir);
    let keystore_dir = config_dir.join("keystore");

    Paths {
        config_dir: config_dir.clone(),
        keystore_dir: keystore_dir.clone(),
        auth_file: config_dir.join("auth.json"),
        priv_key_file: keystore_dir.join("ed25519.priv"),
        pub_key_file: keystore_dir.join("ed25519.pub"),
    }
}

fn generate_keys_and_did() -> (Vec<u8>, Vec<u8>, String, String) {
    let mut rng = OsRng;
    let sk = SigningKey::generate(&mut rng);
    let vk = sk.verifying_key();

    let priv_key_bytes = sk.to_bytes().to_vec(); // 32 bytes
    let pub_key_bytes = vk.to_bytes().to_vec(); // 32 bytes

    // multicodec prefix for ed25519-pub: 0xED 0x01
    let mut multicodec_key = Vec::with_capacity(2 + pub_key_bytes.len());
    multicodec_key.extend_from_slice(&[0xED, 0x01]);
    multicodec_key.extend_from_slice(&pub_key_bytes);

    let pub_key_multibase = multibase::encode(Base::Base58Btc, &multicodec_key);
    let did = format!("did:key:{}", pub_key_multibase);

    (priv_key_bytes, pub_key_bytes, pub_key_multibase, did)
}

fn create_directory<P: AsRef<Path>>(path: P) -> Result<bool, Box<dyn Error>> {
    let path = path.as_ref();

    // Check if path already exists and is a directory
    if path.exists() {
        if path.is_dir() {
            return Ok(false); // Already exists as directory
        } else {
            return Err(format!("Path exists but is not a directory: {}", path.display()).into());
        }
    }

    // Create the directory and all parents
    fs::create_dir_all(path)?;

    Ok(true)
}

fn load_existing(p: &Paths) -> Result<NodeData, Box<dyn Error>> {
    let meta: AuthMetadata = serde_json::from_slice(&fs::read(&p.auth_file)?)?;
    let priv_key_bytes = fs::read(&p.priv_key_file)?;
    let pub_key_bytes = fs::read(&p.pub_key_file)?;

    Ok(NodeData {
        id: meta.did,
        private_key: priv_key_bytes,
        public_key: pub_key_bytes,
    })
}

fn bootstrap_new(p: &Paths) -> Result<NodeData, AppError> {
    ensure_keystore_dir(p)
        .map_err(|e| AppError::Bootstrap(format!("Failed to setup Keystore directories. {}", e)))?;

    let (priv_key_bytes, pub_key_bytes, pub_key_multibase, did) = generate_keys_and_did();

    write_atomic_with_mode(&p.priv_key_file, &priv_key_bytes, 0o600)
        .map_err(|e| AppError::Bootstrap(format!("Failed to write private key: {}", e)))?;

    write_atomic_with_mode(&p.pub_key_file, &pub_key_bytes, 0o644)
        .map_err(|e| AppError::Bootstrap(format!("Failed to write public key: {}", e)))?;

    let meta = AuthMetadata {
        schema: "flow-auth/v1".to_string(),
        did: did.clone(),
        created_at: chrono::Utc::now().to_rfc3339(),
        pub_key_multibase: pub_key_multibase,
    };

    let json = serde_json::to_string_pretty(&meta)
        .map_err(|e| AppError::Bootstrap(format!("Failed to serialize auth metadata: {}", e)))?;

    write_atomic_with_mode(&p.auth_file, json.as_bytes(), 0o644)
        .map_err(|e| AppError::Bootstrap(format!("Failed to write auth file: {}", e)))?;

    Ok(NodeData {
        id: did,
        private_key: priv_key_bytes,
        public_key: pub_key_bytes,
    })
}

fn ensure_keystore_dir(p: &Paths) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(&p.keystore_dir)?;

    #[cfg(unix)]
    {
        let mut perms = fs::metadata(&p.keystore_dir)?.permissions();
        perms.set_mode(0o700);
        fs::set_permissions(&p.keystore_dir, perms)?;
    }

    Ok(())
}

// Atomic write with permissions
fn write_atomic_with_mode(path: &Path, data: &[u8], mode: u32) -> Result<(), Box<dyn Error>> {
    let parent = path.parent().ok_or("No parent directory")?;
    let mut tmp = NamedTempFile::new_in(parent)?;
    tmp.write_all(data)?;
    tmp.flush()?;

    #[cfg(unix)]
    {
        let mut perms = fs::metadata(tmp.path())?.permissions();
        perms.set_mode(mode);
        fs::set_permissions(tmp.path(), perms)?;
    }

    // Persist atomically to destination
    tmp.persist(path)?;
    Ok(())
}
