use openaccounts_account_identity_manager::{AccountCategory, AccountIdentityManager};
use openaccounts_account_registry::{AccountRecord, AccountRegistry};
use openaccounts_core::{AccountId, OpenAccountsError, Result};
use openaccounts_policy_engine::PolicyEngine;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedAccountView {
    pub account: AccountRecord,
    pub category: AccountCategory,
    pub session_isolated: bool,
    pub email_address: Option<String>,
}

pub struct MultiAccountRegistry {
    registry: Arc<AccountRegistry>,
    identity_manager: Arc<AccountIdentityManager>,
    policy_engine: Arc<PolicyEngine>,
}

impl MultiAccountRegistry {
    pub fn new(
        registry: Arc<AccountRegistry>,
        identity_manager: Arc<AccountIdentityManager>,
        policy_engine: Arc<PolicyEngine>,
    ) -> Self {
        Self {
            registry,
            identity_manager,
            policy_engine,
        }
    }

    pub async fn can_create_additional_account(
        &self,
        platform: &str,
        category: &AccountCategory,
    ) -> Result<bool> {
        let existing = self.identity_manager.list_by_platform(platform).await;
        let count = existing.len() as u32;

        let purpose_str = match category {
            AccountCategory::Personal => "Personal",
            AccountCategory::Business => "Business",
            AccountCategory::Brand => "Brand",
            AccountCategory::CreativeProject => "Creative Project",
            AccountCategory::Community => "Community",
        };

        let check = self
            .policy_engine
            .check_multi_account_eligibility(platform, count, purpose_str, false)
            .await?;

        if !check.is_allowed {
            return Err(OpenAccountsError::PermissionDenied(check.reason));
        }

        Ok(true)
    }

    pub async fn list_managed_accounts(&self) -> Result<Vec<ManagedAccountView>> {
        let accounts = self.registry.list_accounts().await?;
        let mut views = Vec::new();

        for acc in accounts {
            let identity = self.identity_manager.get_identity(&acc.id).await;
            let category = identity
                .as_ref()
                .map(|i| i.category.clone())
                .unwrap_or(AccountCategory::Personal);

            views.push(ManagedAccountView {
                email_address: acc.email.clone(),
                account: acc,
                category,
                session_isolated: true,
            });
        }

        Ok(views)
    }
}
