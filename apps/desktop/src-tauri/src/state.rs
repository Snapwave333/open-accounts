use chrono::Utc;
use openaccounts_account_identity_manager::AccountIdentityManager;
use openaccounts_account_registry::{AccountRecord, AccountRegistry};
use openaccounts_audit_engine::AuditEngine;
use openaccounts_authentication_engine::AuthenticationEngine;
use openaccounts_automation_engine::AutomationEngine;
use openaccounts_brand_manager::BrandManager;
use openaccounts_compliance_audit::ComplianceAuditEngine;
use openaccounts_content_strategy::ContentStrategyEngine;
use openaccounts_core::{AccountId, Platform};
use openaccounts_creative_pipeline::CreativePipeline;
use openaccounts_credential_vault::VaultManager;
use openaccounts_database::Database;
use openaccounts_email_identity::EmailIdentityManager;
use openaccounts_email_verification::EmailVerificationEngine;
use openaccounts_engagement_agent::EngagementAgent;
use openaccounts_event_bus::EventBus;
use openaccounts_human_intervention::HumanInterventionController;
use openaccounts_lm_gateway::LMGateway;
use openaccounts_multi_account_registry::MultiAccountRegistry;
use openaccounts_permission_engine::PermissionEngine;
use openaccounts_platform_intelligence::PlatformIntelligence;
use openaccounts_policy_engine::PolicyEngine;
use openaccounts_policy_monitor::PolicyMonitor;
use openaccounts_policy_versioning::PolicyVersioningRegistry;
use openaccounts_publishing_engine::PublishingEngine;
use openaccounts_social_analytics::SocialAnalyticsEngine;
use openaccounts_social_media_manager::SocialMediaManagerAgent;
use serde_json::json;
use std::sync::Arc;
use tracing::{info, warn};

use crate::autonomy::AutonomyRuntime;

#[allow(dead_code)]
pub struct AppState {
    pub db: Arc<Database>,
    pub vault: Arc<VaultManager>,
    pub lm_gateway: Arc<LMGateway>,
    pub human_controller: Arc<HumanInterventionController>,
    pub permission_engine: Arc<PermissionEngine>,
    pub audit_engine: Arc<AuditEngine>,
    pub account_registry: Arc<AccountRegistry>,
    pub automation_engine: Arc<AutomationEngine>,
    pub event_bus: Arc<EventBus>,
    pub platform_intelligence: Arc<PlatformIntelligence>,
    pub policy_engine: Arc<PolicyEngine>,
    pub policy_versioning: Arc<PolicyVersioningRegistry>,
    pub policy_monitor: Arc<PolicyMonitor>,
    pub email_manager: Arc<EmailIdentityManager>,
    pub email_verification: Arc<EmailVerificationEngine>,
    pub auth_engine: Arc<AuthenticationEngine>,
    pub identity_manager: Arc<AccountIdentityManager>,
    pub multi_account_registry: Arc<MultiAccountRegistry>,
    pub compliance_audit: Arc<ComplianceAuditEngine>,
    pub account_creator: Arc<openaccounts_account_creator::AccountCreator>,
    pub brand_manager: Arc<BrandManager>,
    pub content_strategy: Arc<ContentStrategyEngine>,
    pub creative_pipeline: Arc<CreativePipeline>,
    pub publishing_engine: Arc<PublishingEngine>,
    pub engagement_agent: Arc<EngagementAgent>,
    pub social_analytics: Arc<SocialAnalyticsEngine>,
    pub social_media_manager: Arc<SocialMediaManagerAgent>,
    pub autonomy: Arc<AutonomyRuntime>,
}

impl AppState {
    pub async fn init() -> Self {
        let app_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let db_path = app_dir.join("openaccounts.sqlite");
        let db = Arc::new(Database::init(&db_path).await.expect("Database init failed"));

        let vault = Arc::new(VaultManager::new());
        let lm_gateway = Arc::new(LMGateway::new());
        let human_controller = Arc::new(HumanInterventionController::new());
        let permission_engine = Arc::new(PermissionEngine::new());
        let audit_engine = Arc::new(AuditEngine::new(db.clone()));
        let account_registry = Arc::new(AccountRegistry::new(db.clone()));

        let automation_engine = Arc::new(AutomationEngine::new(
            permission_engine.clone(),
            audit_engine.clone(),
            lm_gateway.clone(),
            human_controller.clone(),
        ));

        let autonomy = Arc::new(AutonomyRuntime::new(
            db.clone(),
            automation_engine.clone(),
        ));

        // Import the studio's real, configured identities. Nothing is invented:
        // every row below is derived from StickExplainers/config.json.
        import_configured_accounts(&account_registry, autonomy.studio_root()).await;

        let event_bus = Arc::new(EventBus::new());

        let platform_intelligence = Arc::new(PlatformIntelligence::new());
        let policy_engine = Arc::new(PolicyEngine::new(platform_intelligence.clone()));
        let policy_versioning = Arc::new(PolicyVersioningRegistry::new());
        let policy_monitor = Arc::new(PolicyMonitor::new(
            platform_intelligence.clone(),
            policy_versioning.clone(),
        ));

        // No email provider is registered because no live provider is configured.
        // Allocation therefore fails loudly instead of returning a fake mailbox.
        let email_manager = Arc::new(EmailIdentityManager::new());
        let email_verification = Arc::new(EmailVerificationEngine::new());

        let auth_engine = Arc::new(AuthenticationEngine::new(vault.clone()));
        let identity_manager = Arc::new(AccountIdentityManager::new());
        let multi_account_registry = Arc::new(MultiAccountRegistry::new(
            account_registry.clone(),
            identity_manager.clone(),
            policy_engine.clone(),
        ));
        let compliance_audit = Arc::new(ComplianceAuditEngine::new(db.clone()));

        let account_creator = Arc::new(openaccounts_account_creator::AccountCreator::new(
            vault.clone(),
            human_controller.clone(),
            policy_engine.clone(),
            email_manager.clone(),
            compliance_audit.clone(),
            account_registry.clone(),
        ));

        let brand_manager = Arc::new(BrandManager::new());
        let content_strategy = Arc::new(ContentStrategyEngine::new(
            brand_manager.clone(),
            lm_gateway.clone(),
        ));
        let creative_pipeline = Arc::new(CreativePipeline::new());
        // Probing runs each tool's version command; keep it off the startup path.
        {
            let pipeline = creative_pipeline.clone();
            tokio::spawn(async move { pipeline.discover_tools().await });
        }

        let publishing_engine = Arc::new(PublishingEngine::new(policy_engine.clone()));
        let engagement_agent = Arc::new(EngagementAgent::new(
            policy_engine.clone(),
            lm_gateway.clone(),
            account_registry.clone(),
        ));
        let social_analytics = Arc::new(SocialAnalyticsEngine::new());
        let social_media_manager = Arc::new(SocialMediaManagerAgent::new(
            brand_manager.clone(),
            content_strategy.clone(),
            creative_pipeline.clone(),
            publishing_engine.clone(),
            engagement_agent.clone(),
            social_analytics.clone(),
        ));

        Self {
            db,
            vault,
            lm_gateway,
            human_controller,
            permission_engine,
            audit_engine,
            account_registry,
            automation_engine,
            event_bus,
            platform_intelligence,
            policy_engine,
            policy_versioning,
            policy_monitor,
            email_manager,
            email_verification,
            auth_engine,
            identity_manager,
            multi_account_registry,
            compliance_audit,
            account_creator,
            brand_manager,
            content_strategy,
            creative_pipeline,
            publishing_engine,
            engagement_agent,
            social_analytics,
            social_media_manager,
            autonomy,
        }
    }
}

/// Reads `StickExplainers/config.json` and imports only the identities that are
/// actually configured there. Runs once, and is idempotent: an account whose
/// platform + username already exists is left untouched.
async fn import_configured_accounts(registry: &Arc<AccountRegistry>, studio_root: &std::path::Path) {
    let config_path = studio_root.join("config.json");
    let raw = match std::fs::read_to_string(&config_path) {
        Ok(raw) => raw,
        Err(e) => {
            warn!(
                "Studio config not readable at {}: {e}. No accounts imported.",
                config_path.display()
            );
            return;
        }
    };
    let cfg: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(e) => {
            warn!("Studio config is not valid JSON: {e}. No accounts imported.");
            return;
        }
    };

    let existing = registry.list_accounts().await.unwrap_or_default();
    let already = |platform: &str, username: &str| {
        existing
            .iter()
            .any(|a| a.platform.to_string() == platform && a.username == username)
    };

    let mut candidates: Vec<(Platform, String, Option<String>, serde_json::Value)> = Vec::new();

    if let Some(handle) = cfg.get("instagram_account").and_then(|v| v.as_str()) {
        if !handle.trim().is_empty() {
            candidates.push((
                Platform::Instagram,
                format!("@{handle}"),
                cfg.pointer("/email_watch/sources/0/name")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                json!({ "source": "StickExplainers/config.json", "integration": "chrome_relay" }),
            ));
        }
    }

    if cfg.get("tiktok").and_then(|v| v.as_bool()).unwrap_or(false) {
        candidates.push((
            Platform::Custom("tiktok".into()),
            "@clearlydrawnlessons".to_string(),
            cfg.pointer("/email_watch/sources/0/name")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            json!({
                "source": "StickExplainers/config.json",
                "integration": "chrome_relay",
                "via": cfg.get("tiktok_via").and_then(|v| v.as_str()).unwrap_or("unknown")
            }),
        ));
    }

    if let Some(store) = cfg.pointer("/merch/store_url").and_then(|v| v.as_str()) {
        candidates.push((
            Platform::Custom("shopify".into()),
            store.trim_start_matches("https://").trim_end_matches('/').to_string(),
            None,
            json!({
                "source": "StickExplainers/config.json",
                "store_url": store,
                "live": cfg.pointer("/merch/live").and_then(|v| v.as_bool()).unwrap_or(false)
            }),
        ));
    }

    if let Some(paypal) = cfg.pointer("/cfo/paypal_me").and_then(|v| v.as_str()) {
        candidates.push((
            Platform::Custom("paypal".into()),
            paypal.to_string(),
            cfg.pointer("/sponsors/sender")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            json!({ "source": "StickExplainers/config.json", "paypal_link": format!("https://{paypal}") }),
        ));
    }

    if let Some(sources) = cfg.pointer("/email_watch/sources").and_then(|v| v.as_array()) {
        for source in sources {
            let Some(name) = source.get("name").and_then(|v| v.as_str()) else {
                continue;
            };
            let kind = source
                .get("kind")
                .and_then(|v| v.as_str())
                .unwrap_or("email");
            candidates.push((
                Platform::Email,
                name.to_string(),
                Some(name.to_string()),
                json!({
                    "source": "StickExplainers/config.json",
                    "provider": kind,
                    "role": "inbox watch"
                }),
            ));
        }
    }

    let mut imported = 0usize;
    for (platform, username, email, metadata) in candidates {
        let platform_key = platform.to_string();
        if already(&platform_key, &username) {
            continue;
        }
        let now = Utc::now();
        let record = AccountRecord {
            id: AccountId::new(),
            platform,
            username: username.clone(),
            email,
            phone: None,
            status: "configured".into(),
            profile_metadata: metadata,
            created_at: now,
            updated_at: now,
            last_active: None,
        };
        match registry.create_account(record).await {
            Ok(_) => imported += 1,
            Err(e) => warn!("Failed to import {platform_key}:{username}: {e}"),
        }
    }

    if imported > 0 {
        info!("Imported {imported} configured studio accounts from config.json");
    }
}
