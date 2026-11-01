use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConfidenceLevel {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicySection {
    pub title: String,
    pub official_url: String,
    pub publication_date: Option<String>,
    pub last_verified: DateTime<Utc>,
    pub summary: String,
    pub full_text: Option<String>,
    pub key_rules: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationPolicy {
    pub official_url: String,
    pub is_automation_permitted: bool,
    pub allowed_automation_types: Vec<String>,
    pub prohibited_automation_types: Vec<String>,
    pub requires_official_api: bool,
    pub headless_browser_allowed: bool,
    pub human_takeover_required_on_challenge: bool,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreationRequirements {
    pub official_url: String,
    pub requires_email: bool,
    pub allows_email_aliases: bool,
    pub requires_phone: bool,
    pub allows_voip: bool,
    pub minimum_age: u8,
    pub captcha_enforced: bool,
    pub legal_consent_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultipleAccountPolicy {
    pub official_url: String,
    pub allowed: bool,
    pub max_accounts_per_identity: Option<u32>,
    pub allowed_purposes: Vec<String>, // e.g. "Personal", "Business", "Brand", "Testing"
    pub prohibits_ban_evasion: bool,
    pub requires_distinct_emails: bool,
    pub requires_identity_linking: bool,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthVerificationPolicy {
    pub official_url: String,
    pub supports_2fa: bool,
    pub supports_oauth: bool,
    pub supported_2fa_types: Vec<String>, // "TOTP", "SMS", "EmailOTP", "SecurityKey"
    pub session_duration_days: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiPolicy {
    pub official_url: String,
    pub public_api_available: bool,
    pub requires_developer_application: bool,
    pub commercial_use_permitted: bool,
    pub scopes_supported: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitRule {
    pub action: String,
    pub limit_per_hour: Option<u32>,
    pub limit_per_day: Option<u32>,
    pub official_doc_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserRestrictions {
    pub official_url: String,
    pub cloudflare_or_bot_protection: bool,
    pub prohibits_scraping: bool,
    pub requires_standard_user_agent: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnforcementPolicy {
    pub official_url: String,
    pub strike_system: bool,
    pub shadowban_possible: bool,
    pub temporary_lock_duration_hours: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppealsPolicy {
    pub official_url: String,
    pub appeal_form_url: Option<String>,
    pub typical_response_days: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacyPolicy {
    pub official_url: String,
    pub gdpr_compliant: bool,
    pub data_retention_days: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonetizationPolicy {
    pub official_url: String,
    pub monetization_supported: bool,
    pub minimum_followers: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformKnowledgeProfile {
    pub id: String,
    pub name: String,
    pub version: String,
    pub confidence_level: ConfidenceLevel,
    pub last_verified: DateTime<Utc>,
    pub community_guidelines: PolicySection,
    pub terms_of_service: PolicySection,
    pub bot_and_automation_policy: AutomationPolicy,
    pub account_creation_requirements: CreationRequirements,
    pub multiple_account_policy: MultipleAccountPolicy,
    pub authentication_and_verification: AuthVerificationPolicy,
    pub api_availability: ApiPolicy,
    pub rate_limits: Vec<RateLimitRule>,
    pub browser_automation_restrictions: BrowserRestrictions,
    pub enforcement_procedures: EnforcementPolicy,
    pub appeals_process: AppealsPolicy,
    pub privacy_and_retention: PrivacyPolicy,
    pub monetization_restrictions: MonetizationPolicy,
}
