use openaccounts_browser_abstraction::{BrowserProvider, ChallengeType};
use openaccounts_core::{AccountId, Platform, Result};
use openaccounts_credential_vault::{SecretPayload, SecretType, VaultManager};
use openaccounts_human_intervention::HumanInterventionController;
use rand::{distributions::Alphanumeric, Rng};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CreationMode {
    Guided,
    Autonomous,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountCreationPlan {
    pub platform: Platform,
    pub mode: CreationMode,
    pub desired_username: Option<String>,
    pub email: String,
    pub phone: Option<String>,
}

pub struct AccountCreator {
    vault: Arc<VaultManager>,
    human_controller: Arc<HumanInterventionController>,
}

impl AccountCreator {
    pub fn new(
        vault: Arc<VaultManager>,
        human_controller: Arc<HumanInterventionController>,
    ) -> Self {
        Self {
            vault,
            human_controller,
        }
    }

    pub fn generate_secure_password(length: usize) -> String {
        rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(length)
            .map(char::from)
            .collect()
    }

    pub async fn execute_creation(
        &self,
        browser: Arc<dyn BrowserProvider>,
        plan: AccountCreationPlan,
    ) -> Result<AccountId> {
        let account_id = AccountId::new();
        let username = plan.desired_username.unwrap_or_else(|| {
            format!("user_{}", &account_id.0.to_string()[..8])
        });
        let password = Self::generate_secure_password(20);

        info!("Starting account creation for {} on platform {}", username, plan.platform);

        let registration_url = match plan.platform {
            Platform::Instagram => "https://www.instagram.com/accounts/emailsignup/",
            Platform::Reddit => "https://www.reddit.com/register/",
            _ => "https://example.com/register",
        };
        browser.navigate(registration_url).await?;

        let task_id = self
            .human_controller
            .create_task(
                Some(account_id.0.to_string()),
                None,
                ChallengeType::LegalConsent,
                format!("Review and Accept Terms for {}", plan.platform),
                "Please review the platform Terms of Service and Privacy Policy before continuing.".into(),
                Some(registration_url.to_string()),
            )
            .await?;

        info!("Created legal compliance human task: {}", task_id);

        if self.vault.is_unlocked().await {
            self.vault
                .store_secret(
                    Some(account_id.0.to_string()),
                    SecretType::Password,
                    format!("{}:password", plan.platform),
                    SecretPayload {
                        value: password,
                        metadata: Some(format!("Email: {}", plan.email)),
                    },
                )
                .await?;
        }

        Ok(account_id)
    }
}
