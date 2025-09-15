use chrono::{DateTime, Utc};
use node::bootstrap::init::{AuthMetadata, initialize_config_dir};
use std::fs;
use std::path::PathBuf;

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
