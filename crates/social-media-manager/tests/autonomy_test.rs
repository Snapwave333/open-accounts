use async_trait::async_trait;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use openaccounts_brand_manager::BrandManager;
use openaccounts_content_strategy::{CalendarPostSlot, ContentStrategyEngine};
use openaccounts_core::{AccountId, OpenAccountsError, Result};
use openaccounts_creative_pipeline::CreativePipeline;
use openaccounts_engagement_agent::{EngagementAgent, EngagementConfig};
use openaccounts_lm_gateway::LMGateway;
use openaccounts_platform_connectors::{
    ConnectorRegistry, PlatformAuthInfo, PlatformConnector, PublishRequest, RemoteActionReceipt, RemoteInteraction,
    RemoteMetrics, StaticTokenSource,
};
use openaccounts_platform_intelligence::PlatformIntelligence;
use openaccounts_policy_engine::PolicyEngine;
use openaccounts_publishing_engine::PublishingEngine;
use openaccounts_social_analytics::SocialAnalyticsEngine;
use openaccounts_social_media_manager::{SocialMediaManagerAgent, SubAgentRole, SLOT_STATUS_NEEDS_ACCOUNT, SLOT_STATUS_PUBLISHED};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

struct FakeX {
    published: AtomicUsize,
    fail: AtomicBool,
}

#[async_trait]
impl PlatformConnector for FakeX {
    fn platform(&self) -> &str {
        "x"
    }
    async fn verify_auth(&self) -> Result<PlatformAuthInfo> {
        Err(OpenAccountsError::Internal("unused".into()))
    }
    async fn like(&self, _: &str) -> Result<RemoteActionReceipt> {
        unreachable!()
    }
    async fn follow(&self, _: &str) -> Result<RemoteActionReceipt> {
        unreachable!()
    }
    async fn subscribe(&self, _: &str) -> Result<RemoteActionReceipt> {
        unreachable!()
    }
    async fn reshare(&self, _: &str, _: Option<&str>) -> Result<RemoteActionReceipt> {
        unreachable!()
    }
    async fn comment(&self, _: &str, _: &str) -> Result<RemoteActionReceipt> {
        unreachable!()
    }
    async fn reply(&self, _: &str, _: &str) -> Result<RemoteActionReceipt> {
        unreachable!()
    }
    async fn publish(&self, _: PublishRequest) -> Result<RemoteActionReceipt> {
        tokio::time::sleep(Duration::from_millis(20)).await;
        if self.fail.load(Ordering::SeqCst) {
            return Err(OpenAccountsError::Automation("X returned HTTP 503".into()));
        }
        let n = self.published.fetch_add(1, Ordering::SeqCst) + 1;
        Ok(RemoteActionReceipt { remote_id: format!("tweet-{n}"), permalink: None })
    }
    async fn fetch_inbox(&self, _: Option<DateTime<Utc>>, _: u32) -> Result<Vec<RemoteInteraction>> {
        Ok(vec![])
    }
    async fn fetch_metrics(&self) -> Result<RemoteMetrics> {
        Ok(RemoteMetrics::default())
    }
}

struct Stack {
    manager: Arc<SocialMediaManagerAgent>,
    strategy: Arc<ContentStrategyEngine>,
    x: Arc<FakeX>,
}

fn setup_full_stack() -> Stack {
    let intel = Arc::new(PlatformIntelligence::new());
    let policy = Arc::new(PolicyEngine::new(intel));
    let lm = Arc::new(LMGateway::new());
    let x = Arc::new(FakeX { published: AtomicUsize::new(0), fail: AtomicBool::new(false) });
    let registry = Arc::new(ConnectorRegistry::new(Arc::new(StaticTokenSource::new())).with_connector("x", x.clone()));

    let brand = Arc::new(BrandManager::new());
    let strategy = Arc::new(ContentStrategyEngine::new(brand.clone(), lm.clone()));
    let creative = Arc::new(CreativePipeline::new());
    let publishing = Arc::new(PublishingEngine::with_connectors(policy.clone(), registry.clone()));
    let engagement = Arc::new(EngagementAgent::with_connectors(
        policy.clone(),
        lm.clone(),
        EngagementConfig { use_lm: false, ..Default::default() },
        registry.clone(),
    ));
    let analytics = Arc::new(SocialAnalyticsEngine::with_connectors(registry));

    let manager = Arc::new(
        SocialMediaManagerAgent::new(brand, strategy.clone(), creative, publishing, engagement, analytics)
            .with_lm_gateway(lm),
    );
    Stack { manager, strategy, x }
}

fn slot(id: &str, approved: bool, account: Option<String>, when: DateTime<Utc>) -> CalendarPostSlot {
    CalendarPostSlot {
        slot_id: id.into(),
        campaign_id: Some("camp-01".into()),
        brand_id: "brand-core".into(),
        target_platform: "x".into(),
        scheduled_time: when,
        content_type: "Text_Post".into(),
        caption: format!("Post {id}"),
        hashtags: vec!["#security".into()],
        asset_ids: vec![],
        is_approved: approved,
        status: "Scheduled".into(),
        account_id: account,
        published_at: None,
    }
}

fn account() -> Option<String> {
    Some(AccountId::new().0.to_string())
}

#[tokio::test]
async fn cycle_publishes_due_approved_slots_once_and_flags_the_rest() {
    let s = setup_full_stack();
    let now = Utc::now();
    s.strategy.schedule_post_slot(slot("unapproved", false, account(), now)).await;
    s.strategy.schedule_post_slot(slot("ready", true, account(), now)).await;
    s.strategy.schedule_post_slot(slot("no-account", true, None, now)).await;
    s.strategy
        .schedule_post_slot(slot("later", true, account(), now + ChronoDuration::hours(3)))
        .await;

    assert!(!s.manager.is_loop_running());
    s.manager.start_autonomous_loop();
    assert!(s.manager.is_loop_running());
    s.manager.pause_autonomous_loop();
    assert!(!s.manager.is_loop_running());

    let cp = s.manager.execute_autonomous_cycle().await.unwrap();
    assert_eq!(cp.processed_slots_count, 1);
    assert_eq!(cp.pending_human_approvals_count, 1);
    assert_eq!(cp.skipped_slots_count, 1);
    assert_eq!(cp.deferred_slots_count, 1);
    assert_eq!(cp.failed_slots_count, 0);
    assert_eq!(s.x.published.load(Ordering::SeqCst), 1);

    let calendar = s.strategy.list_calendar().await;
    let status = |id: &str| calendar.iter().find(|c| c.slot_id == id).unwrap().status.clone();
    assert_eq!(status("ready"), SLOT_STATUS_PUBLISHED);
    assert_eq!(status("no-account"), SLOT_STATUS_NEEDS_ACCOUNT);

    let second = s.manager.execute_autonomous_cycle().await.unwrap();
    assert_eq!(second.processed_slots_count, 0);
    assert_eq!(s.x.published.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn failed_publishes_are_counted_and_retried_next_cycle() {
    let s = setup_full_stack();
    s.x.fail.store(true, Ordering::SeqCst);
    s.strategy.schedule_post_slot(slot("flaky", true, account(), Utc::now())).await;

    let cp = s.manager.execute_autonomous_cycle().await.unwrap();
    assert_eq!(cp.processed_slots_count, 0);
    assert_eq!(cp.failed_slots_count, 1);
    assert!(cp.published_idempotency_keys.is_empty());

    s.x.fail.store(false, Ordering::SeqCst);
    let cp = s.manager.execute_autonomous_cycle().await.unwrap();
    assert_eq!(cp.processed_slots_count, 1);
    assert_eq!(s.x.published.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn concurrent_cycles_never_double_publish() {
    let s = setup_full_stack();
    for i in 0..5 {
        s.strategy.schedule_post_slot(slot(&format!("s{i}"), true, account(), Utc::now())).await;
    }
    let a = s.manager.clone();
    let b = s.manager.clone();
    let (ra, rb) = tokio::join!(a.execute_autonomous_cycle(), b.execute_autonomous_cycle());
    let total = ra.unwrap().processed_slots_count + rb.unwrap().processed_slots_count;
    assert_eq!(total, 5);
    assert_eq!(s.x.published.load(Ordering::SeqCst), 5);
}

#[tokio::test]
async fn restart_recovery_from_checkpoint_prevents_republishing() {
    let s = setup_full_stack();
    s.strategy.schedule_post_slot(slot("once", true, account(), Utc::now())).await;
    s.manager.start_autonomous_loop();
    let original = s.manager.execute_autonomous_cycle().await.unwrap();
    assert_eq!(original.published_idempotency_keys.len(), 1);

    let fresh = setup_full_stack();
    fresh.strategy.schedule_post_slot(slot("once", true, account(), Utc::now())).await;
    assert!(!fresh.manager.is_loop_running());
    fresh.manager.restore_from_checkpoint(original.clone()).await.unwrap();
    assert!(fresh.manager.is_loop_running());
    assert_eq!(fresh.manager.get_last_checkpoint().await.unwrap().checkpoint_id, original.checkpoint_id);

    let cp = fresh.manager.execute_autonomous_cycle().await.unwrap();
    assert_eq!(cp.processed_slots_count, 0);
    assert_eq!(fresh.x.published.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn agent_roster_reflects_live_state() {
    let s = setup_full_stack();
    s.strategy.schedule_post_slot(slot("pending", false, None, Utc::now())).await;
    let roster = s.manager.list_collaborating_agents().await;

    let strategist = roster.iter().find(|a| matches!(a.role, SubAgentRole::ContentStrategist)).unwrap();
    assert!(strategist.current_task.contains("1 open calendar slots"));
    assert!(strategist.current_task.contains("1 awaiting approval"));
    assert!(strategist.assigned_model_profile.starts_with("nim-cascade"));
    assert!(!strategist.is_active);

    let community = roster.iter().find(|a| matches!(a.role, SubAgentRole::CommunityManager)).unwrap();
    assert!(community.current_task.starts_with("0 inbox messages"));

    let publisher = roster.iter().find(|a| matches!(a.role, SubAgentRole::Publisher)).unwrap();
    assert!(publisher.current_task.starts_with("No cycle run yet"));
}

#[tokio::test]
async fn background_loop_runs_cycles_only_while_enabled() {
    let s = setup_full_stack();
    s.strategy.schedule_post_slot(slot("bg", true, account(), Utc::now())).await;
    let handle = s.manager.clone().spawn_autonomous_loop(Duration::from_secs(1));

    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(s.x.published.load(Ordering::SeqCst), 0);

    s.manager.start_autonomous_loop();
    tokio::time::sleep(Duration::from_millis(1300)).await;
    assert_eq!(s.x.published.load(Ordering::SeqCst), 1);
    handle.abort();
}

#[tokio::test]
async fn daily_post_cap_defers_extra_posts_per_account() {
    let stack = setup_full_stack();
    let acc = account();
    let other = account();
    let due = Utc::now() - chrono::Duration::minutes(1);
    stack.manager.set_daily_post_cap(acc.as_deref().unwrap(), 1).await;
    for (id, a) in [("cap-1", acc.clone()), ("cap-2", acc.clone()), ("cap-3", other.clone())] {
        stack.strategy.schedule_post_slot(slot(id, true, a, due)).await;
    }

    let first = stack.manager.execute_autonomous_cycle().await.unwrap();
    assert_eq!(first.processed_slots_count, 2, "one post for the capped account and one for the other");
    assert_eq!(first.deferred_slots_count, 1);
    assert_eq!(stack.x.published.load(Ordering::SeqCst), 2);

    let published: Vec<_> = stack
        .strategy
        .list_calendar()
        .await
        .into_iter()
        .filter(|s| s.published_at.is_some())
        .collect();
    assert_eq!(published.len(), 2);

    let second = stack.manager.execute_autonomous_cycle().await.unwrap();
    assert_eq!(second.processed_slots_count, 0, "cap still applies within the 24h window");
    assert_eq!(second.deferred_slots_count, 1);
    assert_eq!(stack.manager.daily_post_cap("unknown").await, openaccounts_social_media_manager::DEFAULT_DAILY_POST_CAP);
}
