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

    pub async fn list_providers(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.providers.read().await.keys().cloned().collect();
        ids.sort();
        ids
    }

    pub async fn provider(&self, provider_id: &str) -> Option<Arc<dyn EmailProviderAdapter>> {
        self.providers.read().await.get(provider_id).cloned()
    }

    pub async fn allocate_identity(
        &self,
        provider_id: &str,
        prefix: &str,
        platform: Option<String>,
    ) -> Result<EmailIdentity> {
        let adapter = self.provider(provider_id).await.ok_or_else(|| {
            OpenAccountsError::Internal(format!(
                "Email provider '{provider_id}' is not configured. Connect a Gmail or Outlook mailbox first."
            ))
        })?;

        let email_address = adapter.create_alias_or_mailbox(prefix).await?;
        let normalized_platform = platform.map(|p| p.trim().to_lowercase()).filter(|p| !p.is_empty());

        let mut identities = self.identities.write().await;
        let is_primary = !identities
            .values()
            .any(|i| i.is_primary && i.platform == normalized_platform);

        let identity = EmailIdentity {
            identity_id: Uuid::new_v4().to_string(),
            email_address,
            provider_type: adapter.provider_type(),
            assigned_account_id: None,
            platform: normalized_platform,
            is_primary,
            created_at: Utc::now(),
        };
        identities.insert(identity.identity_id.clone(), identity.clone());

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
        let mut list: Vec<EmailIdentity> = self.identities.read().await.values().cloned().collect();
        list.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| a.identity_id.cmp(&b.identity_id)));
        list
    }
}

impl Default for EmailIdentityManager {
    fn default() -> Self {
        Self::new()
    }
}
