use chrono::Utc;
use openaccounts_account_creator::AccountCreator;
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
use openaccounts_email_provider_gateway::{EmailProviderType, MockEmailProviderAdapter};
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
use openaccounts_scheduler::TaskScheduler;
use openaccounts_social_analytics::SocialAnalyticsEngine;
use openaccounts_social_media_manager::SocialMediaManagerAgent;
use serde_json::json;
use std::sync::Arc;

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
    pub scheduler: Arc<TaskScheduler>,
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
    pub account_creator: Arc<AccountCreator>,
    pub brand_manager: Arc<BrandManager>,
    pub content_strategy: Arc<ContentStrategyEngine>,
    pub creative_pipeline: Arc<CreativePipeline>,
    pub publishing_engine: Arc<PublishingEngine>,
    pub engagement_agent: Arc<EngagementAgent>,
    pub social_analytics: Arc<SocialAnalyticsEngine>,
    pub social_media_manager: Arc<SocialMediaManagerAgent>,
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
        if account_registry.list_accounts().await.map(|a| a.is_empty()).unwrap_or(true) {
            let now = Utc::now();
            let stick_accounts = vec![
                AccountRecord {
                    id: AccountId::new(),
                    platform: Platform::Instagram,
                    username: "@snapwave".into(),
                    email: Some("elikenny333@gmail.com".into()),
                    phone: None,
                    status: "active".into(),
                    profile_metadata: json!({
                        "source": "StickExplainers",
                        "brand": "Clearly Drawn News (CDN)",
                        "account_type": "Creator (Digital creator)",
                        "integration": "chrome_relay",
                        "daily_caps": { "likes": 12, "follows": 3, "comments": 2, "replies": 15, "feed_posts": 2 }
                    }),
                    created_at: now,
                    updated_at: now,
                    last_active: Some(now),
                },
                AccountRecord {
                    id: AccountId::new(),
                    platform: Platform::Custom("tiktok".into()),
                    username: "@clearlydrawnlessons".into(),
                    email: Some("elikenny333@gmail.com".into()),
                    phone: None,
                    status: "active".into(),
                    profile_metadata: json!({
                        "source": "StickExplainers",
                        "brand": "Clearly Drawn News (CDN)",
                        "integration": "chrome_relay",
                        "daily_caps": { "likes": 12, "follows": 3, "comments": 2, "replies": 15 }
                    }),
                    created_at: now,
                    updated_at: now,
                    last_active: Some(now),
                },
                AccountRecord {
                    id: AccountId::new(),
                    platform: Platform::YouTube,
                    username: "@ClearlyDrawnLessons".into(),
                    email: Some("elikenny333@gmail.com".into()),
                    phone: None,
                    status: "active".into(),
                    profile_metadata: json!({
                        "source": "StickExplainers",
                        "brand": "Clearly Drawn News (CDN)",
                        "channel_id": "UC16fbSsDJcc6RVvNsH3Cbfg",
                        "oauth_credentials": "~/.youtube-analytics-mcp/credentials.json",
                        "daily_caps": { "uploads": 9, "comment_replies": 20, "likes": 5, "subs": 1, "comments": 2 }
                    }),
                    created_at: now,
                    updated_at: now,
                    last_active: Some(now),
                },
                AccountRecord {
                    id: AccountId::new(),
                    platform: Platform::Custom("shopify".into()),
                    username: "zs56hs-fj.myshopify.com".into(),
                    email: Some("elikenny333@gmail.com".into()),
                    phone: None,
                    status: "active".into(),
                    profile_metadata: json!({
                        "source": "StickExplainers",
                        "brand": "Clearly Drawn News Merch (Printful)",
                        "store_url": "https://zs56hs-fj.myshopify.com",
                        "live": true
                    }),
                    created_at: now,
                    updated_at: now,
                    last_active: Some(now),
                },
                AccountRecord {
                    id: AccountId::new(),
                    platform: Platform::Custom("paypal".into()),
                    username: "paypal.me/snapwave333".into(),
                    email: Some("elikenny333@gmail.com".into()),
                    phone: None,
                    status: "active".into(),
                    profile_metadata: json!({
                        "source": "StickExplainers",
                        "brand": "Clearly Drawn News CFO",
                        "paypal_link": "https://paypal.me/snapwave333"
                    }),
                    created_at: now,
                    updated_at: now,
                    last_active: Some(now),
                },
                AccountRecord {
                    id: AccountId::new(),
                    platform: Platform::Email,
                    username: "elikenny333@gmail.com".into(),
                    email: Some("elikenny333@gmail.com".into()),
                    phone: None,
                    status: "active".into(),
                    profile_metadata: json!({
                        "source": "StickExplainers",
                        "provider": "gmail",
                        "role": "Primary Sponsor Outreach & Inbox Watch"
                    }),
                    created_at: now,
                    updated_at: now,
                    last_active: Some(now),
                },
                AccountRecord {
                    id: AccountId::new(),
                    platform: Platform::Email,
                    username: "focogaming@outlook.com".into(),
                    email: Some("focogaming@outlook.com".into()),
                    phone: None,
                    status: "active".into(),
                    profile_metadata: json!({
                        "source": "StickExplainers",
                        "provider": "outlook",
                        "role": "Secondary Inbox Watch"
                    }),
                    created_at: now,
                    updated_at: now,
                    last_active: Some(now),
                },
            ];
            for acc in stick_accounts {
                let _ = account_registry.create_account(acc).await;
            }
        }
        let automation_engine = Arc::new(AutomationEngine::new(
            permission_engine.clone(),
            audit_engine.clone(),
            lm_gateway.clone(),
            human_controller.clone(),
        ));
        let scheduler = Arc::new(TaskScheduler::new());
        let event_bus = Arc::new(EventBus::new());

        let platform_intelligence = Arc::new(PlatformIntelligence::new());
        let policy_engine = Arc::new(PolicyEngine::new(platform_intelligence.clone()));
        let policy_versioning = Arc::new(PolicyVersioningRegistry::new());
        let policy_monitor = Arc::new(PolicyMonitor::new(
            platform_intelligence.clone(),
            policy_versioning.clone(),
        ));

        let email_manager = Arc::new(EmailIdentityManager::new());
        let mock_email_adapter = Arc::new(MockEmailProviderAdapter::new(
            EmailProviderType::CustomDomainApi,
            "openaccounts.io".into(),
        ));
        email_manager
            .register_provider("primary_domain", mock_email_adapter)
            .await;
        let email_verification = Arc::new(EmailVerificationEngine::new());

        let auth_engine = Arc::new(AuthenticationEngine::new(vault.clone()));
        let identity_manager = Arc::new(AccountIdentityManager::new());
        let multi_account_registry = Arc::new(MultiAccountRegistry::new(
            account_registry.clone(),
            identity_manager.clone(),
            policy_engine.clone(),
        ));
        let compliance_audit = Arc::new(ComplianceAuditEngine::new(db.clone()));

        let account_creator = Arc::new(AccountCreator::new(
            vault.clone(),
            human_controller.clone(),
            policy_engine.clone(),
            email_manager.clone(),
            compliance_audit.clone(),
            account_registry.clone(),
        ));

        // Social Media Manager & Marketing Subsystems
        let brand_manager = Arc::new(BrandManager::new());
        let content_strategy = Arc::new(ContentStrategyEngine::new(
            brand_manager.clone(),
            lm_gateway.clone(),
        ));
        let creative_pipeline = Arc::new(CreativePipeline::new());
        creative_pipeline.discover_tools().await;

        let publishing_engine = Arc::new(PublishingEngine::new(policy_engine.clone()));
        let engagement_agent = Arc::new(EngagementAgent::new(
            policy_engine.clone(),
            lm_gateway.clone(),
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
            scheduler,
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
        }
    }
}
