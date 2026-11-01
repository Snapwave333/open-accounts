use chrono::{DateTime, Utc};
use openaccounts_core::{AccountId, Result};
use openaccounts_credential_vault::VaultManager;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuthMethod {
    Password,
    OAuth2,
    EmailOTP,
    SmsOTP,
    Totp2FA,
    Passkey,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthSessionRecord {
    pub session_id: String,
    pub account_id: AccountId,
    pub platform: String,
    pub auth_method: AuthMethod,
    pub authenticated_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub is_active: bool,
}

pub struct AuthenticationEngine {
    #[allow(dead_code)]
    vault: Arc<VaultManager>,
}

impl AuthenticationEngine {
    pub fn new(vault: Arc<VaultManager>) -> Self {
        Self { vault }
    }

    pub async fn record_session(
        &self,
        account_id: AccountId,
        platform: &str,
        method: AuthMethod,
        expires_at: Option<DateTime<Utc>>,
    ) -> Result<AuthSessionRecord> {
        info!("Recording authenticated session for account {}", account_id);
        Ok(AuthSessionRecord {
            session_id: uuid::Uuid::new_v4().to_string(),
            account_id,
            platform: platform.to_string(),
            auth_method: method,
            authenticated_at: Utc::now(),
            expires_at,
            is_active: expires_at.map(|t| t > Utc::now()).unwrap_or(true),
        })
    }
}
