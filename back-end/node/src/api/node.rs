use crate::bootstrap::init::NodeData;
use crate::modules::ai_pipeline::PipelineManager;
use crate::modules::space;
use crate::modules::ssi::webauthn;
use crate::modules::ssi::webauthn::state::AuthState;
use errors::AppError;
use log::info;
use sea_orm::DatabaseConnection;
use sled::Db;
use webauthn_rs::prelude::CreationChallengeResponse;
use webauthn_rs::prelude::{
    AuthenticationResult, PublicKeyCredential, RegisterPublicKeyCredential,
    RequestChallengeResponse,
};

#[derive(Clone)]
pub struct Node {
    pub node_data: NodeData,
    pub db: DatabaseConnection,
    pub kv: Db,
    pub auth_state: AuthState,
    pub pipeline_manager: PipelineManager,
}

impl Node {
    pub fn new(
        node_data: NodeData,
        db: DatabaseConnection,
        kv: Db,
        auth_state: AuthState,
        pipeline_manager: PipelineManager,
    ) -> Self {
        Node {
            node_data,
            db,
            kv,
            auth_state,
            pipeline_manager,
        }
    }

    pub async fn create_space(&self, dir: &str) -> Result<(), AppError> {
        info!("Setting up space in Directory: {}", dir);
        space::new_space(dir, &self.db, &self.pipeline_manager).await?;
        Ok(())
    }

    pub async fn start_webauthn_registration(
        &self,
    ) -> Result<(CreationChallengeResponse, String), AppError> {
        info!("Starting WebAuthn Registration..");
        webauthn::auth::start_registration(self)
            .await
            .map_err(|e| AppError::Auth(format!("WebAuthn registration failed: {}", e)))
    }

    pub async fn finish_webauthn_registration(
        &self,
        challenge_id: &str,
        reg: RegisterPublicKeyCredential,
    ) -> Result<(String, String), AppError> {
        info!("Finishing WebAuthn Registration..");
        webauthn::auth::finish_registration(self, challenge_id, reg)
            .await
            .map_err(|e| AppError::Auth(format!("WebAuthn registration failed: {}", e)))
    }

    pub async fn start_webauthn_authentication(
        &self,
    ) -> Result<(RequestChallengeResponse, String), AppError> {
        webauthn::auth::start_authentication(self)
            .await
            .map_err(|e| AppError::Auth(format!("WebAuthn authentication start failed: {}", e)))
    }

    pub async fn finish_webauthn_authentication(
        &self,
        challenge_id: &str,
        auth: PublicKeyCredential,
    ) -> Result<AuthenticationResult, AppError> {
        info!("Finishing WebAuthn Authentication..");
        webauthn::auth::finish_authentication(self, challenge_id, auth)
            .await
            .map_err(|e| AppError::Auth(format!("WebAuthn authentication failed: {}", e)))
    }
}
