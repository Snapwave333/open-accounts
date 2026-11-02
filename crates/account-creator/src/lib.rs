use openaccounts_account_identity_manager::AccountCategory;
use openaccounts_account_registry::AccountRegistry;
use openaccounts_browser_abstraction::ChallengeType;
use openaccounts_compliance_audit::{ComplianceAuditEngine, ComplianceEventType};
use openaccounts_core::{OpenAccountsError, Result};
use openaccounts_credential_vault::VaultManager;
use openaccounts_email_identity::EmailIdentityManager;
use openaccounts_human_intervention::HumanInterventionController;
use openaccounts_policy_engine::PolicyEngine;
use rand::{distributions::Alphanumeric, Rng};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreflightStepResult {
    pub step: u8,
    pub name: String,
    pub passed: bool,
    pub details: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountCreationPreflightRequest {
    pub platform: String,
    pub category: AccountCategory,
    pub desired_username: Option<String>,
    pub preferred_email: Option<String>,
    pub email_provider_id: Option<String>,
}

/// What the preflight really established. Registration itself is done by a
/// human; the run stops at the first step that cannot be completed for real.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountCreationPreflightReport {
    pub platform: String,
    pub allocated_email: Option<String>,
    pub username: Option<String>,
    pub step_results: Vec<PreflightStepResult>,
    /// Id of the human task created for the registration, when one was.
    pub human_task_id: Option<String>,
    /// Always false: no account exists until a human registers it.
    pub is_ready_for_automation: bool,
}

pub struct AccountCreator {
    #[allow(dead_code)]
    vault: Arc<VaultManager>,
    human_controller: Arc<HumanInterventionController>,
    policy_engine: Arc<PolicyEngine>,
    email_manager: Arc<EmailIdentityManager>,
    compliance_audit: Arc<ComplianceAuditEngine>,
    account_registry: Arc<AccountRegistry>,
}

fn purpose_for(category: &AccountCategory) -> &'static str {
    match category {
        AccountCategory::Personal => "Personal",
        AccountCategory::Business => "Business",
        AccountCategory::Brand => "Brand",
        AccountCategory::CreativeProject => "Creative Project",
        AccountCategory::Community => "Community",
    }
}

impl AccountCreator {
    pub fn new(
        vault: Arc<VaultManager>,
        human_controller: Arc<HumanInterventionController>,
        policy_engine: Arc<PolicyEngine>,
        email_manager: Arc<EmailIdentityManager>,
        compliance_audit: Arc<ComplianceAuditEngine>,
        account_registry: Arc<AccountRegistry>,
    ) -> Self {
        Self {
            vault,
            human_controller,
            policy_engine,
            email_manager,
            compliance_audit,
            account_registry,
        }
    }

    pub fn generate_secure_password(length: usize) -> String {
        rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(length)
            .map(char::from)
            .collect()
    }

    /// Runs the checks that can actually be performed before an account is
    /// registered: policy reference lookup, multiple-account eligibility against
    /// the accounts really in the registry, and email allocation. Registration
    /// is handed to a human task. Nothing is written to the account registry
    /// and no step is reported as passed unless it was.
    pub async fn execute_10_step_preflight(
        &self,
        req: AccountCreationPreflightRequest,
    ) -> Result<AccountCreationPreflightReport> {
        let platform = req.platform.to_lowercase();
        let mut report = AccountCreationPreflightReport {
            platform: platform.clone(),
            allocated_email: None,
            username: req.desired_username.clone(),
            step_results: Vec::new(),
            human_task_id: None,
            is_ready_for_automation: false,
        };

        // STEP 1: platform policy reference on file
        info!("Account preflight step 1: policy reference for {platform}");
        let Some(profile) = self.policy_engine.platform_profile(&platform).await else {
            report.step_results.push(PreflightStepResult {
                step: 1,
                name: "Platform Policy Reference".into(),
                passed: false,
                details: format!("No policy reference data for '{platform}'."),
            });
            self.log(ComplianceEventType::PolicyVerification, &platform, None, "NoPolicyData")
                .await?;
            return Ok(report);
        };
        let creation = &profile.account_creation_requirements;
        report.step_results.push(PreflightStepResult {
            step: 1,
            name: "Platform Policy Reference".into(),
            passed: true,
            details: format!(
                "Stored {} policy data (version {}, {:?} confidence). Terms: {}. \
                 Requirements: email {}, phone {}, minimum age {}.",
                profile.name,
                profile.version,
                profile.confidence_level,
                profile.terms_of_service.official_url,
                creation.requires_email,
                creation.requires_phone,
                creation.minimum_age
            ),
        });
        self.log(
            ComplianceEventType::PolicyVerification,
            &platform,
            Some(&profile.terms_of_service.official_url),
            "ReferenceFound",
        )
        .await?;

        // STEP 2: multiple-account eligibility, counted from the real registry
        info!("Account preflight step 2: multiple-account eligibility");
        let existing = self
            .account_registry
            .list_accounts()
            .await?
            .iter()
            .filter(|a| a.platform.to_string().eq_ignore_ascii_case(&platform))
            .count() as u32;
        let check = self
            .policy_engine
            .check_multi_account_eligibility(&platform, existing, purpose_for(&req.category), false)
            .await?;
        report.step_results.push(PreflightStepResult {
            step: 2,
            name: "Multiple-Account Eligibility".into(),
            passed: check.is_allowed,
            details: format!(
                "{} existing {platform} account(s) in the registry. {}",
                existing, check.reason
            ),
        });
        self.log(
            ComplianceEventType::MultiAccountCheck,
            &platform,
            Some(&check.official_policy_url),
            if check.is_allowed { "Allowed" } else { "Refused" },
        )
        .await?;
        if !check.is_allowed {
            return Ok(report);
        }

        // STEP 3: email identity
        info!("Account preflight step 3: email identity");
        let email = match req.preferred_email.clone() {
            Some(email) => {
                report.step_results.push(PreflightStepResult {
                    step: 3,
                    name: "Email Identity".into(),
                    passed: true,
                    details: format!("Using operator-supplied address {email} (not verified)."),
                });
                email
            }
            None => {
                let provider = req.email_provider_id.as_deref().unwrap_or("primary_domain");
                let prefix = req.desired_username.clone().unwrap_or_else(|| platform.clone());
                match self
                    .email_manager
                    .allocate_identity(provider, &prefix, Some(platform.clone()))
                    .await
                {
                    Ok(identity) => {
                        report.step_results.push(PreflightStepResult {
                            step: 3,
                            name: "Email Identity".into(),
                            passed: true,
                            details: format!(
                                "Allocated {} through provider '{provider}'.",
                                identity.email_address
                            ),
                        });
                        identity.email_address
                    }
                    Err(e) => {
                        report.step_results.push(PreflightStepResult {
                            step: 3,
                            name: "Email Identity".into(),
                            passed: false,
                            details: format!("{e}. Supply an email address to continue."),
                        });
                        return Ok(report);
                    }
                }
            }
        };
        report.allocated_email = Some(email.clone());

        // STEP 4: registration is performed by a human
        info!("Account preflight step 4: registration handed to a human");
        let who = req
            .desired_username
            .as_deref()
            .map(|u| format!(" with username '{u}'"))
            .unwrap_or_default();
        let task_id = self
            .human_controller
            .create_task(
                None,
                None,
                ChallengeType::ManualReview,
                format!("Register a {} account", profile.name),
                format!(
                    "Create the {} account for {email}{who} on the platform's own sign-up page. \
                     Accept the terms, CAPTCHA and verification yourself. \
                     Account requirements: {}. Add the account to OpenAccounts once it exists.",
                    profile.name, creation.official_url
                ),
                Some(creation.official_url.clone()),
            )
            .await?;
        report.human_task_id = Some(task_id.to_string());
        report.step_results.push(PreflightStepResult {
            step: 4,
            name: "Registration".into(),
            passed: false,
            details: format!(
                "Registration is not automated. Human task {task_id} created; \
                 no account has been registered yet."
            ),
        });

        Ok(report)
    }

    async fn log(
        &self,
        event: ComplianceEventType,
        platform: &str,
        policy_url: Option<&str>,
        outcome: &str,
    ) -> Result<String> {
        self.compliance_audit
            .log_event(event, platform, None, policy_url, outcome, serde_json::json!({}))
            .await
            .map_err(|e| OpenAccountsError::Internal(format!("Compliance log failed: {e}")))
    }
}
