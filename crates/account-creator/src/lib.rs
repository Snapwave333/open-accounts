use chrono::{DateTime, Utc};
use openaccounts_account_identity_manager::AccountCategory;
use openaccounts_account_registry::{AccountRecord, AccountRegistry};
use openaccounts_browser_abstraction::{BrowserProvider, ChallengeType};
use openaccounts_compliance_audit::{ComplianceAuditEngine, ComplianceEventType};
use openaccounts_core::{AccountId, OpenAccountsError, Platform, Result, SecretId, TaskId};
use openaccounts_credential_vault::{SecretPayload, SecretType, VaultManager};
use openaccounts_email_identity::EmailIdentityManager;
use openaccounts_email_verification::{is_verification_candidate, EmailVerificationEngine};
use openaccounts_human_intervention::{HumanInterventionController, HumanTaskStatus};
use openaccounts_policy_engine::PolicyEngine;
use rand::{rngs::OsRng, seq::SliceRandom, Rng};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::sync::RwLock;
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PreflightStepNumber {
    Step1PolicyVerification,
    Step2EligibilityAndMultiAccount,
    Step3EmailIdentityAllocation,
    Step4CredentialPreparation,
    Step5RegistrationExecution,
    Step6HumanInterventionCheckpoint,
    Step7EmailVerificationConfirmation,
    Step8SecureCredentialStorage,
    Step9AccountRegistryCreation,
    Step10FinalComplianceAudit,
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PreflightStatus {
    Failed,
    AwaitingHuman,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountCreationPreflightReport {
    pub account_id: AccountId,
    pub platform: String,
    pub allocated_email: String,
    pub username: String,
    pub step_results: Vec<PreflightStepResult>,
    pub is_ready_for_automation: bool,
    #[serde(default = "default_status")]
    pub status: PreflightStatus,
    #[serde(default)]
    pub human_task_id: Option<String>,
    #[serde(default)]
    pub signup_url: Option<String>,
}

fn default_status() -> PreflightStatus {
    PreflightStatus::Failed
}

struct PendingRegistration {
    report: AccountCreationPreflightReport,
    category: AccountCategory,
    email_provider_id: Option<String>,
    secret_id: SecretId,
    human_task_id: TaskId,
    started_at: DateTime<Utc>,
    browser: Arc<dyn BrowserProvider>,
}

pub fn signup_url(platform: &str, fallback: &str) -> String {
    let url = match platform {
        "instagram" | "threads" => "https://www.instagram.com/accounts/emailsignup/",
        "facebook" => "https://www.facebook.com/r.php",
        "tiktok" => "https://www.tiktok.com/signup",
        "youtube" | "google" => "https://accounts.google.com/signup",
        "x" | "twitter" => "https://x.com/i/flow/signup",
        "reddit" => "https://www.reddit.com/register/",
        "discord" => "https://discord.com/register",
        "telegram" => "https://web.telegram.org/",
        "twitch" => "https://www.twitch.tv/signup",
        "pinterest" => "https://www.pinterest.com/signup/",
        "linkedin" => "https://www.linkedin.com/signup",
        "snapchat" => "https://accounts.snapchat.com/accounts/v2/signup",
        "github" => "https://github.com/signup",
        "microsoft" => "https://signup.live.com/",
        "apple" => "https://account.apple.com/account",
        "amazon" => "https://www.amazon.com/ap/register",
        "shopify" => "https://www.shopify.com/signup",
        "etsy" => "https://www.etsy.com/join",
        "patreon" => "https://www.patreon.com/signup",
        "wordpress" => "https://wordpress.com/start",
        _ => "",
    };
    if url.is_empty() { fallback.to_string() } else { url.to_string() }
}

pub fn verification_sender_domain(platform: &str) -> &str {
    match platform {
        "instagram" | "threads" => "instagram.com",
        "facebook" => "facebookmail.com",
        "youtube" | "google" => "google.com",
        "x" | "twitter" => "x.com",
        "microsoft" => "microsoft.com",
        "twitch" => "twitch.tv",
        other => other,
    }
}

fn platform_enum(platform: &str) -> Platform {
    match platform {
        "instagram" => Platform::Instagram,
        "reddit" => Platform::Reddit,
        "youtube" => Platform::YouTube,
        "discord" => Platform::Discord,
        "x" => Platform::X,
        other => Platform::Custom(other.into()),
    }
}

fn purpose_for(category: &AccountCategory) -> &'static str {
    match category {
        AccountCategory::Personal => "Personal",
        AccountCategory::Business => "Business",
        AccountCategory::Brand => "Brand",
        AccountCategory::CreativeProject => "Creative",
        AccountCategory::Community => "Community",
    }
}

fn step(n: u8, name: &str, passed: bool, details: impl Into<String>) -> PreflightStepResult {
    PreflightStepResult { step: n, name: name.into(), passed, details: details.into() }
}

fn sanitize_handle(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '.')
        .take(30)
        .collect()
}

pub struct AccountCreator {
    vault: Arc<VaultManager>,
    human_controller: Arc<HumanInterventionController>,
    policy_engine: Arc<PolicyEngine>,
    email_manager: Arc<EmailIdentityManager>,
    compliance_audit: Arc<ComplianceAuditEngine>,
    account_registry: Arc<AccountRegistry>,
    verification: EmailVerificationEngine,
    pending: RwLock<HashMap<AccountId, PendingRegistration>>,
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
            verification: EmailVerificationEngine::new(),
            pending: RwLock::new(HashMap::new()),
        }
    }

    pub fn generate_secure_password(length: usize) -> String {
        const LOWER: &[u8] = b"abcdefghijkmnopqrstuvwxyz";
        const UPPER: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ";
        const DIGITS: &[u8] = b"23456789";
        const SYMBOLS: &[u8] = b"!@#$%^&*-_=+?";
        let length = length.max(12);
        let mut rng = OsRng;
        let all: Vec<u8> = [LOWER, UPPER, DIGITS, SYMBOLS].concat();
        let mut chars: Vec<u8> = [LOWER, UPPER, DIGITS, SYMBOLS]
            .iter()
            .map(|set| set[rng.gen_range(0..set.len())])
            .collect();
        while chars.len() < length {
            chars.push(all[rng.gen_range(0..all.len())]);
        }
        chars.shuffle(&mut rng);
        String::from_utf8(chars).unwrap_or_default()
    }

    async fn audit(&self, event: ComplianceEventType, platform: &str, account_id: &AccountId, outcome: &str, details: serde_json::Value) {
        if let Err(e) = self
            .compliance_audit
            .log_event(event, platform, Some(&account_id.0.to_string()), None, outcome, details)
            .await
        {
            warn!("Compliance audit write failed: {}", e);
        }
    }

    fn finish(mut report: AccountCreationPreflightReport, status: PreflightStatus) -> AccountCreationPreflightReport {
        report.status = status;
        report.is_ready_for_automation = status == PreflightStatus::Completed && report.step_results.iter().all(|s| s.passed);
        report
    }

    pub async fn execute_10_step_preflight(
        &self,
        browser: Arc<dyn BrowserProvider>,
        req: AccountCreationPreflightRequest,
    ) -> Result<AccountCreationPreflightReport> {
        let account_id = AccountId::new();
        let platform = req.platform.trim().to_lowercase();
        let mut report = AccountCreationPreflightReport {
            account_id: account_id.clone(),
            platform: platform.clone(),
            allocated_email: String::new(),
            username: String::new(),
            step_results: Vec::new(),
            is_ready_for_automation: false,
            status: PreflightStatus::Failed,
            human_task_id: None,
            signup_url: None,
        };
        self.audit(ComplianceEventType::PreflightStarted, &platform, &account_id, "Started", serde_json::json!({})).await;

        let policy = match self
            .policy_engine
            .check_automation_compliance(&platform, "Assisted account registration", false, false)
            .await
        {
            Ok(p) => p,
            Err(e) => {
                report.step_results.push(step(1, "Official Platform Policy Verification", false, e.to_string()));
                return Ok(Self::finish(report, PreflightStatus::Failed));
            }
        };
        self.audit(
            ComplianceEventType::PolicyVerification,
            &platform,
            &account_id,
            if policy.is_allowed { "Passed" } else { "Blocked" },
            serde_json::json!({ "reason": policy.reason, "url": policy.official_policy_url }),
        )
        .await;
        report.step_results.push(step(
            1,
            "Official Platform Policy Verification",
            policy.is_allowed,
            format!("{} ({})", policy.reason, policy.official_policy_url),
        ));
        if !policy.is_allowed {
            return Ok(Self::finish(report, PreflightStatus::Failed));
        }

        let existing = self.account_registry.count_for_platform(&platform).await.unwrap_or(0);
        let eligibility = self
            .policy_engine
            .check_multi_account_eligibility(&platform, existing, purpose_for(&req.category), false)
            .await?;
        self.audit(
            ComplianceEventType::MultiAccountCheck,
            &platform,
            &account_id,
            if eligibility.is_allowed { "Passed" } else { "Blocked" },
            serde_json::json!({ "existing_accounts": existing, "reason": eligibility.reason }),
        )
        .await;
        report.step_results.push(step(
            2,
            "Account Eligibility & Multiple-Account Checks",
            eligibility.is_allowed,
            format!("{} Existing {} accounts: {}.", eligibility.reason, platform, existing),
        ));
        if !eligibility.is_allowed {
            return Ok(Self::finish(report, PreflightStatus::Failed));
        }

        let provider_id = match req.email_provider_id.clone() {
            Some(id) => Some(id),
            None => self.email_manager.list_providers().await.into_iter().next(),
        };
        let email = match (req.preferred_email.clone().filter(|e| e.contains('@')), provider_id.as_deref()) {
            (Some(pref), _) => {
                report.step_results.push(step(3, "Authorized Email Identity Allocation", true, format!("Using operator-supplied address {pref}.")));
                pref
            }
            (None, Some(pid)) => {
                let prefix = req.desired_username.as_deref().map(sanitize_handle).filter(|s| !s.is_empty()).unwrap_or_else(|| platform.clone());
                match self.email_manager.allocate_identity(pid, &prefix, Some(platform.clone())).await {
                    Ok(identity) => {
                        let _ = self.email_manager.assign_to_account(&identity.identity_id, account_id.clone()).await;
                        self.audit(
                            ComplianceEventType::EmailIdentityAllocated,
                            &platform,
                            &account_id,
                            "Allocated",
                            serde_json::json!({ "provider": pid }),
                        )
                        .await;
                        report.step_results.push(step(
                            3,
                            "Authorized Email Identity Allocation",
                            true,
                            format!("Allocated {} via {pid}.", identity.email_address),
                        ));
                        identity.email_address
                    }
                    Err(e) => {
                        report.step_results.push(step(3, "Authorized Email Identity Allocation", false, e.to_string()));
                        return Ok(Self::finish(report, PreflightStatus::Failed));
                    }
                }
            }
            (None, None) => {
                report.step_results.push(step(
                    3,
                    "Authorized Email Identity Allocation",
                    false,
                    "No email address supplied and no mailbox connected. Provide an email or set GMAIL_ACCESS_TOKEN / OUTLOOK_ACCESS_TOKEN.",
                ));
                return Ok(Self::finish(report, PreflightStatus::Failed));
            }
        };
        report.allocated_email = email.clone();

        let username = req
            .desired_username
            .as_deref()
            .map(sanitize_handle)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| format!("{}_{}", purpose_for(&req.category).to_lowercase(), &account_id.0.simple().to_string()[..6]));
        report.username = username.clone();

        if !self.vault.is_unlocked().await {
            report.step_results.push(step(4, "Secure Credential Preparation", false, "Unlock the vault before creating accounts so the generated password can be stored."));
            return Ok(Self::finish(report, PreflightStatus::Failed));
        }
        let password = Self::generate_secure_password(24);
        let secret_id = self
            .vault
            .store_secret(
                Some(account_id.0.to_string()),
                SecretType::Password,
                format!("{platform}:credential"),
                SecretPayload { value: password, metadata: Some(format!("username={username}; email={email}")) },
            )
            .await?;
        report.step_results.push(step(
            4,
            "Secure Credential Preparation",
            true,
            "Generated a 24-character password and stored it encrypted in the vault.",
        ));

        let url = signup_url(&platform, &policy.official_policy_url);
        report.signup_url = Some(url.clone());
        let challenge = match browser.navigate(&url).await {
            Ok(()) => {
                let state = browser.get_page_state().await.ok();
                let detected = state.as_ref().and_then(|s| s.pending_challenge.clone());
                let title = state.as_ref().map(|s| s.title.clone()).unwrap_or_default();
                report.step_results.push(step(
                    5,
                    "Registration Execution via Approved Provider",
                    true,
                    match &detected {
                        Some(c) => format!("Opened {url} ({title}). Detected challenge: {}.", c.description),
                        None => format!("Opened {url} ({title})."),
                    },
                ));
                detected.map(|c| c.challenge_type).unwrap_or(ChallengeType::LegalConsent)
            }
            Err(e) => {
                report.step_results.push(step(5, "Registration Execution via Approved Provider", false, format!("Could not open {url}: {e}")));
                return Ok(Self::finish(report, PreflightStatus::Failed));
            }
        };

        let task_id = self
            .human_controller
            .create_task(
                Some(account_id.0.to_string()),
                None,
                challenge,
                format!("Finish {platform} sign-up for {username}"),
                format!(
                    "Complete the {platform} registration form in the open browser using username '{username}' and email '{email}'. \
                     The password is in the vault under '{platform}:credential'. Accept the terms yourself, solve any CAPTCHA, \
                     then mark this task resolved so OpenAccounts can confirm the verification email."
                ),
                Some(url),
            )
            .await?;
        report.human_task_id = Some(task_id.0.to_string());
        report.step_results.push(step(
            6,
            "Human Intervention Gate Checkpoint",
            false,
            format!("Waiting for you to complete sign-up and resolve task {task_id}."),
        ));

        let report = Self::finish(report, PreflightStatus::AwaitingHuman);
        self.pending.write().await.insert(
            account_id.clone(),
            PendingRegistration {
                report: report.clone(),
                category: req.category,
                email_provider_id: provider_id,
                secret_id,
                human_task_id: task_id,
                started_at: Utc::now(),
                browser,
            },
        );
        info!("Preflight for {} on {} is waiting for human sign-up", username, platform);
        Ok(report)
    }

    pub async fn pending_registrations(&self) -> Vec<AccountCreationPreflightReport> {
        self.pending.read().await.values().map(|p| p.report.clone()).collect()
    }

    pub async fn cancel_registration(&self, account_id: &AccountId) -> Result<()> {
        let pending = self
            .pending
            .write()
            .await
            .remove(account_id)
            .ok_or_else(|| OpenAccountsError::AccountNotFound(account_id.to_string()))?;
        let _ = self.human_controller.dismiss_task(&pending.human_task_id).await;
        let _ = self.vault.delete_secret(&pending.secret_id).await;
        let _ = pending.browser.close().await;
        Ok(())
    }

    pub async fn complete_registration(
        &self,
        account_id: &AccountId,
        verification_timeout: Duration,
    ) -> Result<AccountCreationPreflightReport> {
        let task_status = {
            let pending = self.pending.read().await;
            let p = pending
                .get(account_id)
                .ok_or_else(|| OpenAccountsError::AccountNotFound(account_id.to_string()))?;
            self.human_controller.get_task(&p.human_task_id).await.map(|t| t.status)
        };
        if task_status != Some(HumanTaskStatus::Resolved) {
            return Err(OpenAccountsError::HumanInterventionRequired(
                "Resolve the sign-up task after finishing registration in the browser".into(),
            ));
        }

        let p = self
            .pending
            .write()
            .await
            .remove(account_id)
            .ok_or_else(|| OpenAccountsError::AccountNotFound(account_id.to_string()))?;
        let mut report = p.report.clone();
        let platform = report.platform.clone();
        if let Some(s) = report.step_results.iter_mut().find(|s| s.step == 6) {
            s.passed = true;
            s.details = format!("Operator completed sign-up and resolved task {}.", p.human_task_id);
        }
        self.audit(ComplianceEventType::HumanApprovalGranted, &platform, account_id, "Resolved", serde_json::json!({})).await;

        let provider = match &p.email_provider_id {
            Some(id) => self.email_manager.provider(id).await,
            None => None,
        };
        let verified_by = match provider {
            Some(provider) => {
                let domain = verification_sender_domain(&platform).to_string();
                let deadline = tokio::time::Instant::now() + verification_timeout;
                let recipient = report.allocated_email.to_lowercase();
                let mut found = None;
                let mut last_error = None;
                loop {
                    match provider.fetch_recent_messages(20, Some(p.started_at)).await {
                        Ok(messages) => {
                            found = messages
                                .iter()
                                .filter(|m| is_verification_candidate(m, &domain))
                                .filter(|m| m.recipient.to_lowercase().contains(&recipient) || m.recipient.is_empty())
                                .map(|m| self.verification.extract_code_or_link(m))
                                .find(|x| x.code.is_some() || x.verification_link.is_some());
                        }
                        Err(e) => last_error = Some(e.to_string()),
                    }
                    if found.is_some() || tokio::time::Instant::now() >= deadline {
                        break;
                    }
                    tokio::time::sleep(self.verification.poll_interval()).await;
                }
                match found {
                    Some(x) => {
                        self.audit(ComplianceEventType::OtpExtracted, &platform, account_id, "Found", serde_json::json!({ "sender": x.sender })).await;
                        let what = match (&x.code, &x.verification_link) {
                            (Some(code), _) => format!("code {code}"),
                            (None, Some(link)) => format!("link {link}"),
                            _ => String::new(),
                        };
                        report.step_results.push(step(7, "Email Verification & Ownership Confirmation", true, format!("Verification email from {} received ({what}).", x.sender)));
                        "mailbox"
                    }
                    None => {
                        report.step_results.push(step(
                            7,
                            "Email Verification & Ownership Confirmation",
                            false,
                            format!(
                                "No verification email reached {} within {}s{}.",
                                report.allocated_email,
                                verification_timeout.as_secs(),
                                last_error.map(|e| format!(" (last error: {e})")).unwrap_or_default()
                            ),
                        ));
                        self.pending.write().await.insert(account_id.clone(), PendingRegistration { report: report.clone(), ..p });
                        report.step_results.retain(|s| s.step != 7);
                        report.step_results.push(step(7, "Email Verification & Ownership Confirmation", false, "Still waiting for the verification email; retry when it arrives."));
                        return Ok(Self::finish(report, PreflightStatus::AwaitingHuman));
                    }
                }
            }
            None => {
                report.step_results.push(step(
                    7,
                    "Email Verification & Ownership Confirmation",
                    true,
                    "No mailbox connected; ownership confirmed by the operator when resolving the sign-up task.",
                ));
                "operator"
            }
        };

        match self.vault.get_secret(&p.secret_id).await {
            Ok(_) => report.step_results.push(step(8, "Secure Credential Vaulting", true, "Credential decrypts correctly from the vault.")),
            Err(e) => {
                report.step_results.push(step(8, "Secure Credential Vaulting", false, format!("Vault check failed: {e}")));
                return Ok(Self::finish(report, PreflightStatus::Failed));
            }
        }

        let now = Utc::now();
        let record = AccountRecord {
            id: account_id.clone(),
            platform: platform_enum(&platform),
            username: report.username.clone(),
            email: Some(report.allocated_email.clone()),
            phone: None,
            status: "active".into(),
            profile_metadata: serde_json::json!({
                "category": p.category,
                "created_via": "guided_preflight",
                "vault_secret_id": p.secret_id.0.to_string(),
                "verified_by": verified_by,
            }),
            created_at: now,
            updated_at: now,
            last_active: Some(now),
        };
        match self.account_registry.create_account(record).await {
            Ok(_) => report.step_results.push(step(9, "Account Registry Creation", true, "Account saved to the local registry.")),
            Err(e) => {
                report.step_results.push(step(9, "Account Registry Creation", false, e.to_string()));
                return Ok(Self::finish(report, PreflightStatus::Failed));
            }
        }

        let logged = self
            .compliance_audit
            .log_event(
                ComplianceEventType::PreflightCompleted,
                &platform,
                Some(&account_id.0.to_string()),
                None,
                "Success",
                serde_json::json!({ "verified_by": verified_by }),
            )
            .await;
        report.step_results.push(match logged {
            Ok(id) => step(10, "Final Compliance & Audit Verification", true, format!("Hash-chained audit entry {id} recorded.")),
            Err(e) => step(10, "Final Compliance & Audit Verification", false, e.to_string()),
        });
        let _ = p.browser.close().await;
        Ok(Self::finish(report, PreflightStatus::Completed))
    }
}
