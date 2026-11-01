use openaccounts_account_creator::AccountCreator;
use openaccounts_account_identity_manager::AccountIdentityManager;
use openaccounts_account_registry::AccountRegistry;
use openaccounts_audit_engine::AuditEngine;
use openaccounts_authentication_engine::AuthenticationEngine;
use openaccounts_automation_engine::AutomationEngine;
use openaccounts_brand_manager::BrandManager;
use openaccounts_compliance_audit::ComplianceAuditEngine;
use openaccounts_content_strategy::ContentStrategyEngine;
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
