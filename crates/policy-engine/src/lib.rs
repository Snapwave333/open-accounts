use openaccounts_core::{OpenAccountsError, Result};
use openaccounts_platform_intelligence::{ConfidenceLevel, PlatformIntelligence};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyCheckResult {
    pub is_allowed: bool,
    pub reason: String,
    pub official_policy_url: String,
    pub requires_human_approval: bool,
    pub requires_workflow_pause: bool,
}

pub struct PolicyEngine {
    platform_intelligence: Arc<PlatformIntelligence>,
}

impl PolicyEngine {
    pub fn new(platform_intelligence: Arc<PlatformIntelligence>) -> Self {
        Self {
            platform_intelligence,
        }
    }

    pub async fn check_multi_account_eligibility(
        &self,
        platform_id: &str,
        existing_accounts_count: u32,
        purpose: &str,
        is_ban_evasion_check: bool,
    ) -> Result<PolicyCheckResult> {
        let profile = self
            .platform_intelligence
            .get_profile(platform_id)
            .await
            .ok_or_else(|| OpenAccountsError::Internal(format!("Platform {platform_id} not registered")))?;

        let policy = &profile.multiple_account_policy;

        if is_ban_evasion_check {
            return Ok(PolicyCheckResult {
                is_allowed: false,
                reason: "Prohibited: OpenAccounts strictly prohibits account creation to evade suspensions or bans.".into(),
                official_policy_url: policy.official_url.clone(),
                requires_human_approval: false,
                requires_workflow_pause: true,
            });
        }

        if profile.confidence_level == ConfidenceLevel::Low {
            return Ok(PolicyCheckResult {
                is_allowed: false,
                reason: "Compliance uncertainty: Policy confidence is Low. Explicit human legal sign-off required.".into(),
                official_policy_url: policy.official_url.clone(),
                requires_human_approval: true,
                requires_workflow_pause: true,
            });
        }

        if !policy.allowed {
            return Ok(PolicyCheckResult {
                is_allowed: false,
                reason: format!("{} terms strictly limit individual users to a single account.", profile.name),
                official_policy_url: policy.official_url.clone(),
                requires_human_approval: true,
                requires_workflow_pause: false,
            });
        }

        if let Some(max_limit) = policy.max_accounts_per_identity {
            if existing_accounts_count >= max_limit {
                return Ok(PolicyCheckResult {
                    is_allowed: false,
                    reason: format!(
                        "Maximum accounts limit ({}) reached for {} per official identity policy.",
                        max_limit, profile.name
                    ),
                    official_policy_url: policy.official_url.clone(),
                    requires_human_approval: true,
                    requires_workflow_pause: false,
                });
            }
        }

        let is_purpose_valid = policy
            .allowed_purposes
            .iter()
            .any(|p| p.to_lowercase().contains(&purpose.to_lowercase()));

        if !is_purpose_valid && !policy.allowed_purposes.is_empty() {
            return Ok(PolicyCheckResult {
                is_allowed: false,
                reason: format!(
                    "Purpose '{}' is not explicitly authorized under {} multiple-account rules.",
                    purpose, profile.name
                ),
                official_policy_url: policy.official_url.clone(),
                requires_human_approval: true,
                requires_workflow_pause: false,
            });
        }

        info!("Multiple-account eligibility check passed for {}", platform_id);
        Ok(PolicyCheckResult {
            is_allowed: true,
            reason: format!("Account creation permitted under {} rules.", profile.name),
            official_policy_url: policy.official_url.clone(),
            requires_human_approval: false,
            requires_workflow_pause: false,
        })
    }

    pub async fn check_automation_compliance(
        &self,
        platform_id: &str,
        action: &str,
        is_official_api: bool,
        is_headless_browser: bool,
    ) -> Result<PolicyCheckResult> {
        let profile = self
            .platform_intelligence
            .get_profile(platform_id)
            .await
            .ok_or_else(|| OpenAccountsError::Internal(format!("Platform {platform_id} not registered")))?;

        let policy = &profile.bot_and_automation_policy;

        if policy.prohibited_automation_types.iter().any(|p| p.to_lowercase().contains(&action.to_lowercase())) {
            return Ok(PolicyCheckResult {
                is_allowed: false,
                reason: format!("Action '{}' is explicitly prohibited by {} automation terms.", action, profile.name),
                official_policy_url: policy.official_url.clone(),
                requires_human_approval: false,
                requires_workflow_pause: true,
            });
        }

        if policy.requires_official_api && !is_official_api && is_headless_browser {
            return Ok(PolicyCheckResult {
                is_allowed: false,
                reason: format!("{} requires official API access; unauthorized headless browser automation prohibited.", profile.name),
                official_policy_url: policy.official_url.clone(),
                requires_human_approval: true,
                requires_workflow_pause: true,
            });
        }

        Ok(PolicyCheckResult {
            is_allowed: true,
            reason: "Action complies with platform automation policy.".into(),
            official_policy_url: policy.official_url.clone(),
            requires_human_approval: false,
            requires_workflow_pause: false,
        })
    }
}
