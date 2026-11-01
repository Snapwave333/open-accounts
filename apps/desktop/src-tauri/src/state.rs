use crate::tokens::VaultTokenSource;
use chrono::Utc;
use openaccounts_account_creator::AccountCreator;
use openaccounts_account_identity_manager::AccountIdentityManager;
use openaccounts_account_registry::{AccountRecord, AccountRegistry};
use openaccounts_audit_engine::AuditEngine;
use openaccounts_automation_engine::AutomationEngine;
use openaccounts_brand_manager::BrandManager;
use openaccounts_compliance_audit::ComplianceAuditEngine;
use openaccounts_content_strategy::ContentStrategyEngine;
use openaccounts_core::{AccountId, Platform};
use openaccounts_creative_pipeline::CreativePipeline;
use openaccounts_credential_vault::VaultManager;
use openaccounts_database::Database;
use openaccounts_email_identity::EmailIdentityManager;
use openaccounts_email_provider_gateway::{GmailApiAdapter, MicrosoftGraphAdapter};
use openaccounts_engagement_agent::{EngagementAgent, EngagementCaps, EngagementConfig};
use openaccounts_human_intervention::HumanInterventionController;
use openaccounts_lm_gateway::LMGateway;
use openaccounts_multi_account_registry::MultiAccountRegistry;
use openaccounts_permission_engine::PermissionEngine;
use openaccounts_platform_connectors::{ChainedTokenSource, ConnectorRegistry, EnvTokenSource, TokenSource};
use openaccounts_platform_intelligence::PlatformIntelligence;
use openaccounts_policy_engine::PolicyEngine;
use openaccounts_policy_monitor::PolicyMonitor;
use openaccounts_policy_versioning::PolicyVersioningRegistry;
use openaccounts_publishing_engine::PublishingEngine;
use openaccounts_scheduler::{JobHandler, TaskScheduler};
use openaccounts_social_analytics::SocialAnalyticsEngine;
use openaccounts_social_media_manager::{bridge::GarbageCollectionReport, SocialMediaManagerAgent};
use serde_json::json;
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::RwLock;
use tracing::{info, warn};

pub const AUTOPILOT_SETTING: &str = "autopilot_enabled";
pub const DATA_DIR_ENV: &str = "OPENACCOUNTS_DATA_DIR";
const DB_FILE: &str = "openaccounts.sqlite";

pub fn data_dir() -> PathBuf {
    if let Some(dir) = std::env::var(DATA_DIR_ENV).ok().filter(|v| !v.trim().is_empty()) {
        return PathBuf::from(dir);
    }
    if let Ok(cwd) = std::env::current_dir() {
        if cwd.join(DB_FILE).is_file() {
            return cwd;
        }
    }
    let base = if cfg!(target_os = "windows") {
        std::env::var("APPDATA").ok().map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var("HOME").ok().map(|h| PathBuf::from(h).join("Library").join("Application Support"))
    } else {
        std::env::var("XDG_DATA_HOME")
            .ok()
            .map(PathBuf::from)
            .or_else(|| std::env::var("HOME").ok().map(|h| PathBuf::from(h).join(".local").join("share")))
    };
    base.map(|b| b.join("OpenAccounts")).unwrap_or_else(|| PathBuf::from("."))
}

pub struct AppState {
    pub db: Arc<Database>,
    pub vault: Arc<VaultManager>,
    pub lm_gateway: Arc<LMGateway>,
    pub human_controller: Arc<HumanInterventionController>,
    pub audit_engine: Arc<AuditEngine>,
    pub account_registry: Arc<AccountRegistry>,
    pub automation_engine: Arc<AutomationEngine>,
    pub scheduler: Arc<TaskScheduler>,
    pub platform_intelligence: Arc<PlatformIntelligence>,
    pub policy_engine: Arc<PolicyEngine>,
    pub policy_versioning: Arc<PolicyVersioningRegistry>,
    pub policy_monitor: Arc<PolicyMonitor>,
    pub email_manager: Arc<EmailIdentityManager>,
    pub multi_account_registry: Arc<MultiAccountRegistry>,
    pub account_creator: Arc<AccountCreator>,
    pub connectors: Arc<ConnectorRegistry>,
    pub connection_cache: Arc<RwLock<crate::connections::ConnectionCache>>,
    pub brand_manager: Arc<BrandManager>,
    pub content_strategy: Arc<ContentStrategyEngine>,
    pub creative_pipeline: Arc<CreativePipeline>,
    pub publishing_engine: Arc<PublishingEngine>,
    pub engagement_agent: Arc<EngagementAgent>,
    pub social_analytics: Arc<SocialAnalyticsEngine>,
    pub social_media_manager: Arc<SocialMediaManagerAgent>,
    pub gc_report: Arc<RwLock<Option<GarbageCollectionReport>>>,
}

fn seed_accounts() -> Vec<AccountRecord> {
    let now = Utc::now();
    let account = |platform: Platform, username: &str, metadata: serde_json::Value| AccountRecord {
        id: AccountId::new(),
        platform,
        username: username.into(),
        email: Some(if username.contains('@') && !username.starts_with('@') { username.into() } else { "elikenny333@gmail.com".into() }),
        phone: None,
        status: "active".into(),
        profile_metadata: metadata,
        created_at: now,
        updated_at: now,
        last_active: None,
    };
    vec![
        account(Platform::Instagram, "@snapwave", json!({
            "source": "StickExplainers", "brand": "Clearly Drawn News (CDN)", "account_type": "Creator (Digital creator)",
            "integration": "chrome_relay",
            "daily_caps": { "likes": 12, "follows": 3, "comments": 2, "replies": 15, "feed_posts": 2 }
        })),
        account(Platform::Custom("tiktok".into()), "@clearlydrawnlessons", json!({
            "source": "StickExplainers", "brand": "Clearly Drawn News (CDN)", "integration": "chrome_relay",
            "daily_caps": { "likes": 12, "follows": 3, "comments": 2, "replies": 15 }
        })),
        account(Platform::YouTube, "@ClearlyDrawnLessons", json!({
            "source": "StickExplainers", "brand": "Clearly Drawn News (CDN)", "channel_id": "UC16fbSsDJcc6RVvNsH3Cbfg",
            "daily_caps": { "uploads": 9, "comment_replies": 20, "likes": 5, "subs": 1, "comments": 2 }
        })),
        account(Platform::Custom("shopify".into()), "zs56hs-fj.myshopify.com", json!({
            "source": "StickExplainers", "brand": "Clearly Drawn News Merch (Printful)", "store_url": "https://zs56hs-fj.myshopify.com"
        })),
        account(Platform::Custom("paypal".into()), "paypal.me/snapwave333", json!({
            "source": "StickExplainers", "brand": "Clearly Drawn News CFO", "paypal_link": "https://paypal.me/snapwave333"
        })),
        account(Platform::Email, "elikenny333@gmail.com", json!({
            "source": "StickExplainers", "provider": "gmail", "role": "Primary Sponsor Outreach & Inbox Watch"
        })),
        account(Platform::Email, "focogaming@outlook.com", json!({
            "source": "StickExplainers", "provider": "outlook", "role": "Secondary Inbox Watch"
        })),
    ]
}

fn handler<F, Fut>(f: F) -> JobHandler
where
    F: Fn() -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = openaccounts_core::Result<String>> + Send + 'static,
{
    Arc::new(move |_job| Box::pin(f()))
}

impl AppState {
    pub async fn init() -> Result<Self, String> {
        Self::init_at(data_dir()).await
    }

    pub async fn init_at(dir: PathBuf) -> Result<Self, String> {
        let db_path = dir.join(DB_FILE);
        info!("Using data directory {}", dir.display());
        let db = Arc::new(Database::init(&db_path).await.map_err(|e| format!("Database init failed: {e}"))?);

        let vault = Arc::new(VaultManager::with_database(db.clone()).await.map_err(|e| e.to_string())?);
        let lm_gateway = Arc::new(LMGateway::new());
        match crate::profiles::load_into(&db, &lm_gateway).await {
            Ok(n) if n > 0 => info!("Restored {} LM profiles", n),
            Ok(_) => {}
            Err(e) => warn!("Could not restore LM profiles: {}", e),
        }
        let human_controller = Arc::new(HumanInterventionController::with_database(db.clone()).await.map_err(|e| e.to_string())?);
        let permission_engine = Arc::new(PermissionEngine::new());
        let audit_engine = Arc::new(AuditEngine::new(db.clone()));
        let account_registry = Arc::new(AccountRegistry::new(db.clone()));
        if account_registry.list_accounts().await.map(|a| a.is_empty()).unwrap_or(false) {
            for acc in seed_accounts() {
                if let Err(e) = account_registry.create_account(acc).await {
                    warn!("Failed to seed account: {}", e);
                }
            }
        }

        let automation_engine = Arc::new(AutomationEngine::new(
            permission_engine.clone(),
            audit_engine.clone(),
            lm_gateway.clone(),
            human_controller.clone(),
        ));
        let scheduler = Arc::new(TaskScheduler::new());

        let platform_intelligence = Arc::new(PlatformIntelligence::new());
        let policy_engine = Arc::new(PolicyEngine::new(platform_intelligence.clone()));
        let policy_versioning = Arc::new(PolicyVersioningRegistry::new());
        let policy_monitor = Arc::new(
            PolicyMonitor::with_database(platform_intelligence.clone(), policy_versioning.clone(), db.clone())
                .await
                .map_err(|e| e.to_string())?,
        );

        let email_manager = Arc::new(EmailIdentityManager::new());
        if let Some(gmail) = GmailApiAdapter::from_env() {
            email_manager.register_provider("gmail", Arc::new(gmail)).await;
        }
        if let Some(outlook) = MicrosoftGraphAdapter::from_env() {
            email_manager.register_provider("outlook", Arc::new(outlook)).await;
        }

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

        let tokens: Arc<dyn TokenSource> = Arc::new(ChainedTokenSource::new(vec![
            Arc::new(VaultTokenSource::new(vault.clone())),
            Arc::new(EnvTokenSource),
        ]));
        let connectors = Arc::new(ConnectorRegistry::new(tokens));

        let brand_manager = Arc::new(BrandManager::new());
        let content_strategy = Arc::new(
            ContentStrategyEngine::new(brand_manager.clone(), lm_gateway.clone())
                .with_store(Arc::new(crate::stores::SqliteCalendarStore::new(db.clone()))),
        );
        match content_strategy.restore().await {
            Ok(n) if n > 0 => info!("Restored {} calendar slots", n),
            Ok(_) => {}
            Err(e) => warn!("Could not restore calendar: {}", e),
        }
        let creative_pipeline = Arc::new(CreativePipeline::new());
        creative_pipeline.discover_tools().await;

        let publishing_engine = Arc::new(PublishingEngine::with_connectors(policy_engine.clone(), connectors.clone()));
        let engagement_agent = Arc::new(
            EngagementAgent::with_connectors(
                policy_engine.clone(),
                lm_gateway.clone(),
                EngagementConfig::default(),
                connectors.clone(),
            )
            .with_store(Arc::new(crate::stores::SqliteEngagementStore::new(db.clone()))),
        );
        match engagement_agent.restore().await {
            Ok((actions, messages)) => info!("Restored {} recent engagement actions and {} inbox messages", actions, messages),
            Err(e) => warn!("Could not restore engagement state: {}", e),
        }
        for acc in account_registry.list_accounts().await.unwrap_or_default() {
            engagement_agent
                .set_account_caps(acc.id.clone(), EngagementCaps::from_profile_metadata(&acc.profile_metadata))
                .await;
        }
        let social_analytics = Arc::new(SocialAnalyticsEngine::with_connectors(connectors.clone()));
        let social_media_manager = Arc::new(
            SocialMediaManagerAgent::new(
                brand_manager.clone(),
                content_strategy.clone(),
                creative_pipeline.clone(),
                publishing_engine.clone(),
                engagement_agent.clone(),
                social_analytics.clone(),
            )
            .with_lm_gateway(lm_gateway.clone()),
        );
        for acc in account_registry.list_accounts().await.unwrap_or_default() {
            let caps = acc.profile_metadata.get("daily_caps");
            if let Some(cap) = ["feed_posts", "posts", "uploads"]
                .iter()
                .find_map(|k| caps.and_then(|c| c.get(*k)).and_then(|v| v.as_u64()))
            {
                social_media_manager
                    .set_daily_post_cap(&acc.id.0.to_string(), cap.min(u32::MAX as u64) as u32)
                    .await;
            }
        }
        if db.get_setting(AUTOPILOT_SETTING).await.ok().flatten().as_deref() == Some("true") {
            social_media_manager.start_autonomous_loop();
        }

        let state = Self {
            db,
            vault,
            lm_gateway,
            human_controller,
            audit_engine,
            account_registry,
            automation_engine,
            scheduler,
            platform_intelligence,
            policy_engine,
            policy_versioning,
            policy_monitor,
            email_manager,
            multi_account_registry,
            account_creator,
            connectors,
            connection_cache: Arc::new(RwLock::new(None)),
            brand_manager,
            content_strategy,
            creative_pipeline,
            publishing_engine,
            engagement_agent,
            social_analytics,
            social_media_manager,
            gc_report: Arc::new(RwLock::new(None)),
        };
        state.register_jobs().await;
        state.scheduler.clone().spawn_runner(Duration::from_secs(30));
        Ok(state)
    }

    async fn register_jobs(&self) {
        let register = |name: &str, expr: &str, h: JobHandler| {
            let scheduler = self.scheduler.clone();
            let name = name.to_string();
            let expr = expr.to_string();
            async move {
                if let Err(e) = scheduler.register_handler(name.clone(), expr, h).await {
                    warn!("Could not schedule '{}': {}", name, e);
                }
            }
        };

        let (manager, engine) = (self.social_media_manager.clone(), self.automation_engine.clone());
        register(
            "Autopilot marketing cycle",
            "every 15m",
            handler(move || {
                let (manager, engine) = (manager.clone(), engine.clone());
                async move {
                    if engine.is_kill_switch_active() {
                        return Ok("skipped: kill switch active".into());
                    }
                    if !manager.is_loop_running() {
                        return Ok("skipped: autopilot paused".into());
                    }
                    let cp = manager.execute_autonomous_cycle().await?;
                    Ok(format!(
                        "published {} · awaiting approval {}",
                        cp.processed_slots_count, cp.pending_human_approvals_count
                    ))
                }
            }),
        )
        .await;

        let (agent, registry, engine, manager) = (
            self.engagement_agent.clone(),
            self.account_registry.clone(),
            self.automation_engine.clone(),
            self.social_media_manager.clone(),
        );
        register(
            "Inbox sync",
            "every 10m",
            handler(move || {
                let (agent, registry, engine, manager) = (agent.clone(), registry.clone(), engine.clone(), manager.clone());
                async move {
                    if engine.is_kill_switch_active() {
                        return Ok("skipped: kill switch active".into());
                    }
                    let mut synced = 0usize;
                    let mut errors = Vec::new();
                    for acc in registry.list_accounts().await? {
                        let platform = acc.platform.to_string();
                        if !matches!(platform.as_str(), "x" | "youtube" | "reddit" | "instagram") {
                            continue;
                        }
                        match agent.sync_inbox(acc.id.clone(), &platform).await {
                            Ok(msgs) => synced += msgs.len(),
                            Err(e) => errors.push(format!("{}: {}", acc.username, e)),
                        }
                    }
                    let mut auto_replied = 0usize;
                    if manager.is_loop_running() && !engine.is_kill_switch_active() {
                        for outcome in agent.auto_reply_pending().await {
                            match outcome {
                                Ok(_) => auto_replied += 1,
                                Err(e) => errors.push(format!("auto-reply: {e}")),
                            }
                        }
                    }
                    let summary = format!("{synced} new interactions · {auto_replied} auto-replies");
                    Ok(if errors.is_empty() { summary } else { format!("{summary} · {}", errors.join("; ")) })
                }
            }),
        )
        .await;

        let (analytics, registry) = (self.social_analytics.clone(), self.account_registry.clone());
        register(
            "Analytics refresh",
            "every 6h",
            handler(move || {
                let (analytics, registry) = (analytics.clone(), registry.clone());
                async move {
                    let mut refreshed = 0usize;
                    let mut errors = Vec::new();
                    for acc in registry.list_accounts().await? {
                        let platform = acc.platform.to_string();
                        if !matches!(platform.as_str(), "x" | "youtube" | "reddit" | "instagram") {
                            continue;
                        }
                        match analytics.refresh_metrics(&acc.id.0.to_string(), &platform).await {
                            Ok(_) => refreshed += 1,
                            Err(e) => errors.push(format!("{}: {}", acc.username, e)),
                        }
                    }
                    Ok(format!("{refreshed} accounts refreshed{}", if errors.is_empty() { String::new() } else { format!(" · {}", errors.join("; ")) }))
                }
            }),
        )
        .await;

        let (monitor, gate) = (self.policy_monitor.clone(), self.human_controller.clone());
        register(
            "Policy document monitor",
            "@daily",
            handler(move || {
                let (monitor, gate) = (monitor.clone(), gate.clone());
                async move {
                    let r = monitor.run_policy_check_cycle_report().await;
                    let tasks = crate::escalation::escalate_policy_changes(&gate, &r).await;
                    Ok(format!(
                        "{} documents · {} changes · {} unreachable · {} review tasks",
                        r.documents_checked, r.changes_detected, r.unreachable, tasks
                    ))
                }
            }),
        )
        .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openaccounts_credential_vault::{SecretPayload, SecretType};

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn app_boots_end_to_end_and_persists_across_restarts() {
        let root = std::env::temp_dir().join(format!("oa-e2e-{}", uuid::Uuid::new_v4()));
        std::env::set_var("OPENACCOUNTS_STICK_ROOT", std::env::temp_dir().join("oa-no-studio-fixture"));
        let persisted_message: String;
        for var in ["NVIDIA_API_KEY", "X_ACCESS_TOKEN", "YOUTUBE_ACCESS_TOKEN", "REDDIT_ACCESS_TOKEN", "INSTAGRAM_ACCESS_TOKEN", "GMAIL_ACCESS_TOKEN", "OUTLOOK_ACCESS_TOKEN"] {
            std::env::remove_var(var);
        }

        {
            let state = AppState::init_at(root.clone()).await.expect("first boot");
            let accounts = state.account_registry.list_accounts().await.unwrap();
            assert_eq!(accounts.len(), seed_accounts().len());
            assert!(root.join(DB_FILE).is_file());

            let jobs = state.scheduler.list_jobs().await;
            assert_eq!(jobs.len(), 4);
            let autopilot = jobs.iter().find(|j| j.name == "Autopilot marketing cycle").unwrap();
            assert_eq!(state.scheduler.run_now(&autopilot.id).await.unwrap(), "skipped: autopilot paused");

            let flow = crate::flow::build(&state).await;
            assert!(flow.up_next.iter().filter(|i| i.kind == "job").count() >= 4);
            assert!(flow.just_did.is_empty() || flow.just_did.iter().all(|i| !i.title.is_empty()));

            let statuses = crate::connections::statuses(&state, true).await;
            let social: Vec<_> = statuses.iter().filter(|s| s.kind == "social").collect();
            assert_eq!(social.len(), 4);
            assert!(social.iter().all(|s| !s.connected && s.detail.contains("not connected")));
            assert!(statuses.iter().any(|s| s.kind == "email" && !s.connected));
            assert!(statuses.iter().any(|s| s.kind == "lm" && !s.connected && s.detail.contains("NVIDIA_API_KEY")));

            let chat = crate::agent::chat(&state, "What needs my attention?", "All accounts", vec![], vec![]).await;
            assert!(chat.unwrap_err().contains("No language model answered"));

            state.vault.unlock("end-to-end-master").await.unwrap();
            state
                .vault
                .store_secret(None, SecretType::OAuthToken, "x:access_token".into(), SecretPayload { value: "vault-token".into(), metadata: None })
                .await
                .unwrap();
            assert!(state.connectors.connector_for("x").await.is_ok());
            assert!(state.connectors.connector_for("reddit").await.is_err());
            state.vault.lock().await;
            assert!(state.connectors.connector_for("x").await.is_err());

            let audit = state.audit_engine.verify_integrity().await.unwrap();
            assert!(audit.is_intact);

            let profile = openaccounts_lm_gateway::ModelProfile {
                id: "persisted-claude".into(),
                name: "Persisted Claude".into(),
                provider_type: openaccounts_lm_gateway::ModelProviderType::Anthropic,
                endpoint: None,
                model_id: "claude-sonnet-5-5".into(),
                api_key_secret_id: Some("ANTHROPIC_API_KEY".into()),
                max_tokens: Some(700),
                temperature: Some(0.3),
            };
            crate::profiles::save(&state.db, &profile).await.unwrap();

            let first_account = state.account_registry.list_accounts().await.unwrap()[0].id.clone();
            let msg = state
                .engagement_agent
                .ingest_message(first_account, "instagram", "@visitor", openaccounts_engagement_agent::InteractionType::Comment, "Love the new reel!")
                .await
                .unwrap();
            state.engagement_agent.archive_message(&msg.message_id).await.unwrap();
            persisted_message = msg.message_id;

            state
                .content_strategy
                .schedule_post_slot(openaccounts_content_strategy::CalendarPostSlot {
                    slot_id: "persisted-slot".into(),
                    campaign_id: None,
                    brand_id: "brand".into(),
                    target_platform: "instagram".into(),
                    scheduled_time: Utc::now() + chrono::Duration::days(1),
                    content_type: "Text_Post".into(),
                    caption: "Tomorrow's post".into(),
                    hashtags: vec![],
                    asset_ids: vec![],
                    is_approved: false,
                    status: "Scheduled".into(),
                    account_id: None,
                    published_at: None,
                })
                .await;
            assert!(state.content_strategy.approve_slot("persisted-slot").await);
        }

        let state = AppState::init_at(root.clone()).await.expect("second boot");
        assert_eq!(state.account_registry.list_accounts().await.unwrap().len(), seed_accounts().len());
        assert!(state.vault.is_initialized().await);
        assert!(state.vault.unlock("wrong-password").await.is_err());
        state.vault.unlock("end-to-end-master").await.unwrap();
        assert_eq!(state.vault.list_metadata().await.len(), 1);
        assert!(!state.social_media_manager.is_loop_running());
        let instagram = state
            .account_registry
            .list_accounts()
            .await
            .unwrap()
            .into_iter()
            .find(|a| a.username == "@snapwave")
            .unwrap();
        assert_eq!(state.social_media_manager.daily_post_cap(&instagram.id.0.to_string()).await, 2);
        let restored_msg = state.engagement_agent.get_message(&persisted_message).await.expect("inbox restored");
        assert_eq!(restored_msg.status, "Archived");
        let slot = state.content_strategy.get_slot("persisted-slot").await.expect("calendar restored");
        assert!(slot.is_approved);
        let restored = state.lm_gateway.list_profiles().await;
        let claude = restored.iter().find(|p| p.id == "persisted-claude").expect("profile restored after restart");
        assert_eq!(claude.max_tokens, Some(700));
        assert_eq!(claude.provider_type, openaccounts_lm_gateway::ModelProviderType::Anthropic);

        std::fs::remove_dir_all(&root).ok();
    }
}
