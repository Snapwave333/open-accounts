use chrono::{DateTime, Utc};
use openaccounts_core::AccountId;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AccountCategory {
    Personal,
    Business,
    Brand,
    CreativeProject,
    Community,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountIdentityProfile {
    pub account_id: AccountId,
    pub category: AccountCategory,
    pub identity_name: String,
    pub email_identity_id: Option<String>,
    pub platform: String,
    pub is_isolated_session: bool,
    pub created_at: DateTime<Utc>,
}

pub struct AccountIdentityManager {
    profiles: Arc<RwLock<HashMap<AccountId, AccountIdentityProfile>>>,
}

impl AccountIdentityManager {
    pub fn new() -> Self {
        Self {
            profiles: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn register_identity(
        &self,
        account_id: AccountId,
        category: AccountCategory,
        identity_name: String,
        email_identity_id: Option<String>,
        platform: String,
    ) -> AccountIdentityProfile {
        let profile = AccountIdentityProfile {
            account_id: account_id.clone(),
            category,
            identity_name,
            email_identity_id,
            platform,
            is_isolated_session: true,
            created_at: Utc::now(),
        };

        self.profiles.write().await.insert(account_id, profile.clone());
        profile
    }

    pub async fn get_identity(&self, account_id: &AccountId) -> Option<AccountIdentityProfile> {
        self.profiles.read().await.get(account_id).cloned()
    }

    pub async fn list_by_platform(&self, platform: &str) -> Vec<AccountIdentityProfile> {
        self.profiles
            .read()
            .await
            .values()
            .filter(|p| p.platform.eq_ignore_ascii_case(platform))
            .cloned()
            .collect()
    }
}

impl Default for AccountIdentityManager {
    fn default() -> Self {
        Self::new()
    }
}
