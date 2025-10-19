use crate::api::node::Node;
use crate::modules::ssi::did::util::{
    cose_to_jwk, create_did_document, did_document_to_json, generate_did_key_from_passkey,
};
use base64::prelude::*;
use entity::pass_key;
use entity::user;
use log::{error, info};
use once_cell::sync::Lazy;
use sea_orm::{
    ActiveModelTrait,
    ActiveValue::{NotSet, Set},
    ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use webauthn_rs::prelude::{
    AuthenticationResult, CreationChallengeResponse, CredentialID, Passkey, PasskeyAuthentication,
    PasskeyRegistration, PublicKeyCredential, RegisterPublicKeyCredential,
    RequestChallengeResponse, Uuid, WebauthnError,
};

struct RegistrationSession {
    uuid: Uuid,
    device_id: String,
    reg_state: PasskeyRegistration,
    created_at: Instant,
}

struct AuthenticationSession {
    uuid: Uuid,
    device_id: String,
    auth_state: PasskeyAuthentication,
    created_at: Instant,
}

static REG_CACHE: Lazy<Arc<Mutex<HashMap<String, RegistrationSession>>>> =
    Lazy::new(|| Arc::new(Mutex::new(HashMap::new())));

static AUTH_CACHE: Lazy<Arc<Mutex<HashMap<String, AuthenticationSession>>>> =
    Lazy::new(|| Arc::new(Mutex::new(HashMap::new())));

pub async fn start_registration(
    node: &Node,
) -> Result<(CreationChallengeResponse, String), WebauthnError> {
    info!("Starting registration.");

    let uuid = Uuid::new_v4();
    let device_id = node.node_data.id.clone();

    // Query existing credentials for this device_id and exclude them
    let existing_creds = get_existing_credentials(&node.db, &device_id)
        .await
        .map_err(|e| {
            error!("Failed to query existing credentials: {e}");
            WebauthnError::CredentialRetrievalError
        })?;

    let exclude_creds = if existing_creds.is_empty() {
        None
    } else {
        Some(existing_creds)
    };

    let res = match node.auth_state.webauthn.start_passkey_registration(
        uuid,
        device_id.as_str(),
        device_id.as_str(),
        exclude_creds,
    ) {
        Ok((ccr, reg_state)) => {
            let challenge_key = BASE64_STANDARD.encode(&ccr.public_key.challenge);
            let session = RegistrationSession {
                uuid,
                device_id,
                reg_state,
                created_at: Instant::now(),
            };

            let mut cache = REG_CACHE.lock().map_err(|e| {
                error!("Failed to acquire lock on REG_CACHE: {e}");
                WebauthnError::CredentialPersistenceError
            })?;

            // Clean up expired entries before inserting so the expired ones don't pile up
            cache.retain(|_, v| v.created_at.elapsed() < Duration::from_secs(300));
            cache.insert(challenge_key.clone(), session);

            info!(
                "Started Registration process with challenge: {challenge_key}"
            );
            (ccr, challenge_key)
        }
        Err(e) => {
            info!("error -> {e:?}");
            return Err(e);
        }
    };

    Ok(res)
}

pub async fn finish_registration(
    node: &Node,
    challenge_key: &str,
    reg: RegisterPublicKeyCredential,
) -> Result<(String, String), WebauthnError> {
    info!("Finishing registration for challenge_id: {challenge_key}");

    let (_uuid, device_id, reg_state) = {
        let mut cache = REG_CACHE.lock().map_err(|e| {
            error!("Failed to acquire lock on REG_CACHE: {e}");
            WebauthnError::CredentialRetrievalError
        })?;

        let session = cache
            .remove(challenge_key)
            .ok_or_else(|| WebauthnError::MismatchedChallenge)?;

        // Check if expired
        if session.created_at.elapsed() > Duration::from_secs(300) {
            return Err(WebauthnError::ChallengeNotFound);
        }

        (session.uuid, session.device_id, session.reg_state)
    };

    // Complete the registration
    let passkey = node
        .auth_state
        .webauthn
        .finish_passkey_registration(&reg, &reg_state)?;

    // Generate DID from passkey using SSI
    let did = generate_did_key_from_passkey(&passkey).map_err(|e| {
        error!("Failed to generate DID: {e}");
        WebauthnError::CredentialPersistenceError
    })?;

    // Create DID Document
    let jwk = cose_to_jwk(passkey.get_public_key()).map_err(|e| {
        error!("Failed to convert COSE to JWK: {e}");
        WebauthnError::CredentialPersistenceError
    })?;

    let did_doc = create_did_document(&did, &jwk).map_err(|e| {
        error!("Failed to create DID document: {e}");
        WebauthnError::CredentialPersistenceError
    })?;

    let did_doc_json = did_document_to_json(&did_doc).map_err(|e| {
        error!("Failed to serialize DID document: {e}");
        WebauthnError::CredentialPersistenceError
    })?;

    info!("Generated DID: {did}");
    info!("DID Document: {did_doc_json}");

    // Create or get user with DID
    let user = get_or_create_user(
        &node.db,
        &did,
        &device_id,
        &device_id,
        Some(did_doc_json.clone()),
    )
    .await
    .map_err(|e| {
        error!("Failed to create/get user: {e}");
        WebauthnError::CredentialPersistenceError
    })?;

    store_passkey(&node.db, user.id, &device_id, &passkey)
        .await
        .map_err(|e| {
            error!("Failed to store Passkey: {e}");
            WebauthnError::CredentialPersistenceError
        })?;

    info!(
        "Passkey stored successfully for user: {0} (DID: {did})",
        user.id
    );

    Ok((did, did_doc_json))
}

pub async fn store_passkey(
    db: &DatabaseConnection,
    user_id: i32,
    device_id: &str,
    passkey: &Passkey,
) -> Result<pass_key::ActiveModel, Box<dyn std::error::Error>> {
    info!("Storing passkey for user {user_id} with device_id {device_id}");

    // Serialize the entire Passkey object to JSON for storage
    let json_data = serde_json::to_string(&passkey)?;

    // Extract credential_id as bytes
    let credential_id: Vec<u8> = passkey.cred_id().as_ref().to_vec();

    // Extract public key (COSE key)
    // Note: The public key in Passkey is stored as COSEKey, we need to serialize it
    let public_key = serde_json::to_vec(&passkey.get_public_key())?;

    let attestation = "None".to_string();
    let name = format!(
        "Passkey-{}-{}",
        &device_id[..8],
        chrono::Utc::now().format("%Y%m%d")
    );

    let new_passkey = pass_key::ActiveModel {
        id: NotSet,
        user_id: Set(user_id),
        device_id: Set(device_id.to_string()),
        credential_id: Set(credential_id),
        public_key: Set(public_key),
        sign_count: Set(0),
        authentication_count: Set(0),
        name: Set(name),
        attestation: Set(attestation),
        json_data: Set(json_data),
        time_created: Set(chrono::Utc::now().into()),
        last_authenticated: Set(chrono::Utc::now().into()),
    };

    match new_passkey.insert(db).await {
        Ok(passkey) => {
            info!(
                "Successfully saved Passkey: {}, Device_id: {}, Public Key: {:x?}",
                passkey.id, device_id, passkey.public_key
            );
            Ok(passkey.into())
        }
        Err(e) => Err(Box::new(e)),
    }
}

/// Start the authentication process
pub async fn start_authentication(
    node: &Node,
) -> Result<(RequestChallengeResponse, String), WebauthnError> {
    let device_id = node.node_data.id.as_str();
    info!("Starting authentication for device: {device_id}");

    // Get all passkeys for this device
    let passkeys = get_passkeys_for_device(&node.db, device_id)
        .await
        .map_err(|_| WebauthnError::CredentialRetrievalError)?;

    if passkeys.is_empty() {
        return Err(WebauthnError::CredentialNotFound);
    }

    let uuid = Uuid::new_v4();
    let challenge_key = Uuid::new_v4().to_string();

    let res = match node
        .auth_state
        .webauthn
        .start_passkey_authentication(&passkeys)
    {
        Ok((rcr, auth_state)) => {
            let session = AuthenticationSession {
                uuid,
                device_id: device_id.to_string(),
                auth_state,
                created_at: Instant::now(),
            };

            let mut cache = AUTH_CACHE.lock().map_err(|e| {
                error!("Failed to acquire lock on AUTH_CACHE: {e}");
                WebauthnError::CredentialPersistenceError
            })?;

            // Clean up expired entries before inserting so the expired ones don't pile up
            cache.retain(|_, v| v.created_at.elapsed() < Duration::from_secs(300));
            cache.insert(challenge_key.clone(), session);

            info!(
                "Started authentication process with challenge: {challenge_key}"
            );
            (rcr, challenge_key)
        }
        Err(e) => {
            info!("Authentication start error -> {e:?}");
            return Err(e);
        }
    };

    Ok(res)
}

/// Complete the authentication process
pub async fn finish_authentication(
    node: &Node,
    challenge_key: &str,
    auth: PublicKeyCredential,
) -> Result<AuthenticationResult, WebauthnError> {
    let (_uuid, device_id, auth_state) = {
        let mut cache = AUTH_CACHE.lock().map_err(|e| {
            error!("Failed to acquire lock on AUTH_CACHE: {e}");
            WebauthnError::CredentialRetrievalError
        })?;

        let session = cache
            .remove(challenge_key)
            .ok_or_else(|| WebauthnError::ChallengeNotFound)?;

        // Check if expired
        if session.created_at.elapsed() > Duration::from_secs(300) {
            return Err(WebauthnError::ChallengeNotFound);
        }

        (session.uuid, session.device_id, session.auth_state)
    };

    // Complete the authentication
    let auth_result = node
        .auth_state
        .webauthn
        .finish_passkey_authentication(&auth, &auth_state)?;

    // Update passkey counter
    update_passkey_counter(&node.db, &device_id, auth_result.counter())
        .await
        .map_err(|_| WebauthnError::CredentialCounterUpdateFailure)?;

    info!(
        "Authentication successful for device: {} with counter: {}",
        device_id,
        auth_result.counter()
    );

    Ok(auth_result)
}

/// Get all passkeys for a specific device
pub async fn get_passkeys_for_device(
    db: &DatabaseConnection,
    device_id: &str,
) -> Result<Vec<Passkey>, Box<dyn std::error::Error>> {
    let passkeys = pass_key::Entity::find()
        .filter(pass_key::Column::DeviceId.eq(device_id))
        .all(db)
        .await?;

    let mut result = Vec::new();
    for passkey_model in passkeys {
        // Deserialize the JSON data back to Passkey
        let passkey: Passkey = serde_json::from_str(&passkey_model.json_data)?;
        result.push(passkey);
    }

    Ok(result)
}

/// Update passkey counter after successful authentication
async fn update_passkey_counter(
    db: &DatabaseConnection,
    device_id: &str,
    new_counter: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    let passkey = pass_key::Entity::find()
        .filter(pass_key::Column::DeviceId.eq(device_id))
        .one(db)
        .await?
        .ok_or("Passkey not found")?;

    let mut active_model: pass_key::ActiveModel = passkey.into();
    active_model.sign_count = Set(new_counter as i32);
    active_model.update(db).await?;

    info!(
        "Updated passkey counter for device: {device_id} to {new_counter}"
    );
    Ok(())
}

/// Retrieve all passkeys for a given device_id
/// Used during registration to populate exclude_credentials
async fn get_passkeys_by_device_id(
    db: &DatabaseConnection,
    device_id: &str,
) -> Result<Vec<Passkey>, Box<dyn std::error::Error>> {
    use entity::pass_key::Column;
    use entity::pass_key::Entity as PassKeyEntity;

    info!("Retrieving passkeys for device_id: {device_id}");

    let passkey_models = PassKeyEntity::find()
        .filter(Column::DeviceId.eq(device_id))
        .all(db)
        .await?;

    let mut passkeys = Vec::new();
    for model in passkey_models {
        match serde_json::from_str::<Passkey>(&model.json_data) {
            Ok(passkey) => {
                info!(
                    "Loaded passkey with credential_id: {:x?}",
                    model.credential_id
                );
                passkeys.push(passkey);
            }
            Err(e) => {
                error!("Failed to deserialize passkey {}: {}", model.id, e);
                // Continue loading other passkeys even if one fails
            }
        }
    }

    Ok(passkeys)
}

/// Retrieve a single passkey by credential_id
/// Used during authentication to find the passkey for verification
async fn _get_passkey_by_credential_id(
    db: &DatabaseConnection,
    credential_id: &[u8],
) -> Result<Option<(pass_key::Model, Passkey)>, Box<dyn std::error::Error>> {
    use entity::pass_key::Column;
    use entity::pass_key::Entity as PassKeyEntity;

    info!("Looking up passkey by credential_id: {credential_id:x?}");

    let passkey_model = PassKeyEntity::find()
        .filter(Column::CredentialId.eq(credential_id))
        .one(db)
        .await?;

    match passkey_model {
        Some(model) => {
            let passkey = serde_json::from_str::<Passkey>(&model.json_data)?;
            info!("Found passkey with ID: {}", model.id);
            Ok(Some((model, passkey)))
        }
        None => {
            info!("No passkey found for credential_id");
            Ok(None)
        }
    }
}

/// Retrieve all credential IDs for a device (for exclude_credentials)
/// Returns a list of CredentialID objects that WebAuthn can use
async fn get_existing_credentials(
    db: &DatabaseConnection,
    device_id: &str,
) -> Result<Vec<CredentialID>, Box<dyn std::error::Error>> {
    let passkeys = get_passkeys_by_device_id(db, device_id).await?;

    let credential_ids: Vec<CredentialID> =
        passkeys.iter().map(|pk| pk.cred_id().clone()).collect();

    info!(
        "Found {} existing credentials for device_id: {}",
        credential_ids.len(),
        device_id
    );

    Ok(credential_ids)
}

/// Update the sign_count for a passkey after successful authentication
async fn _update_sign_count(
    db: &DatabaseConnection,
    passkey_id: i32,
    new_count: u32,
    updated_passkey: &Passkey,
) -> Result<(), Box<dyn std::error::Error>> {
    use entity::pass_key::Entity as PassKeyEntity;

    info!(
        "Updating sign_count for passkey {}: {} -> {}",
        passkey_id,
        new_count.saturating_sub(1),
        new_count
    );

    // Serialize the updated passkey
    let json_data = serde_json::to_string(updated_passkey)?;

    // Update both sign_count and json_data
    let mut passkey: pass_key::ActiveModel = PassKeyEntity::find_by_id(passkey_id)
        .one(db)
        .await?
        .ok_or("Passkey not found")?
        .into();

    passkey.sign_count = Set(new_count as i32);
    passkey.json_data = Set(json_data);

    passkey.update(db).await?;

    info!("Sign count updated successfully for passkey {passkey_id}");

    Ok(())
}

// Add user management function
async fn get_or_create_user(
    db: &DatabaseConnection,
    did: &str,
    device_id: &str,
    username: &str,
    public_key_jwk: Option<String>,
) -> Result<user::Model, Box<dyn std::error::Error>> {
    // Try to find existing user by DID
    if let Some(user) = user::Entity::find()
        .filter(user::Column::Did.eq(did))
        .one(db)
        .await?
    {
        info!("Found existing user with DID: {did}");
        return Ok(user);
    }

    // Create new user
    info!("Creating new user with DID: {did}");
    let device_ids = vec![device_id.to_string()];

    let new_user = user::ActiveModel {
        id: NotSet,
        did: Set(did.to_string()),
        device_ids: Set(serde_json::to_string(&device_ids)?),
        username: Set(username.to_string()),
        display_name: Set(username.to_string()),
        public_key_jwk: Set(public_key_jwk.unwrap_or_default()),
        time_created: Set(chrono::Utc::now().into()),
        last_login: Set(chrono::Utc::now().into()),
    };

    let user = new_user.insert(db).await?;
    info!("Created user with ID: {0} and DID: {did}", user.id);

    Ok(user)
}
