use chrono::{DateTime, Utc};
use openaccounts_core::{AccountId, OpenAccountsError, Result};
use openaccounts_email_provider_gateway::{EmailProviderAdapter, EmailProviderType};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use tracing::info;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailIdentity {
    pub identity_id: String,
    pub email_address: String,
    pub provider_type: EmailProviderType,
    pub assigned_account_id: Option<AccountId>,
    pub platform: Option<String>,
    pub is_primary: bool,
    pub created_at: DateTime<Utc>,
}

pub struct EmailIdentityManager {
    identities: Arc<RwLock<HashMap<String, EmailIdentity>>>,
    providers: Arc<RwLock<HashMap<String, Arc<dyn EmailProviderAdapter>>>>,
}

impl EmailIdentityManager {
    pub fn new() -> Self {
        Self {
            identities: Arc::new(RwLock::new(HashMap::new())),
            providers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn register_provider(&self, provider_id: &str, adapter: Arc<dyn EmailProviderAdapter>) {
        self.providers
            .write()
            .await
            .insert(provider_id.to_string(), adapter);
        info!("Registered email provider: {}", provider_id);
    }

    pub async fn allocate_identity(
        &self,
        provider_id: &str,
        prefix: &str,
        platform: Option<String>,
    ) -> Result<EmailIdentity> {
        let providers = self.providers.read().await;
        let adapter = providers
            .get(provider_id)
            .ok_or_else(|| OpenAccountsError::Internal(format!("Provider {provider_id} not configured")))?;

        let email_address = adapter.create_alias_or_mailbox(prefix).await?;
        let identity_id = Uuid::new_v4().to_string();

        let identity = EmailIdentity {
            identity_id: identity_id.clone(),
            email_address,
            provider_type: EmailProviderType::CustomDomainApi,
            assigned_account_id: None,
            platform,
            is_primary: true,
            created_at: Utc::now(),
        };

        self.identities
            .write()
            .await
            .insert(identity_id.clone(), identity.clone());

        info!("Allocated new email identity: {}", identity.email_address);
        Ok(identity)
    }

    pub async fn assign_to_account(&self, identity_id: &str, account_id: AccountId) -> Result<()> {
        let mut lock = self.identities.write().await;
        let identity = lock
            .get_mut(identity_id)
            .ok_or_else(|| OpenAccountsError::Internal("Identity not found".into()))?;

        identity.assigned_account_id = Some(account_id);
        Ok(())
    }

    pub async fn list_identities(&self) -> Vec<EmailIdentity> {
        self.identities.read().await.values().cloned().collect()
    }
}

impl Default for EmailIdentityManager {
    fn default() -> Self {
        Self::new()
    }
}
