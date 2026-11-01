pub mod bridge;

use crate::bridge::{
    GarbageCollectionReport, StickBridgeMessage, StickBridgeState, StickExplainersBridge,
};
use chrono::{DateTime, Utc};
use openaccounts_brand_manager::BrandManager;
use openaccounts_content_strategy::{CalendarPostSlot, ContentStrategyEngine};
use openaccounts_core::{AccountId, Result};
use openaccounts_creative_pipeline::CreativePipeline;
use openaccounts_engagement_agent::EngagementAgent;
use openaccounts_lm_gateway::{LMGateway, DEFAULT_PROFILE_ID};
use openaccounts_publishing_engine::{PublishingEngine, SocialPostPayload, STATUS_AWAITING_APPROVAL, STATUS_FAILED};
use openaccounts_social_analytics::SocialAnalyticsEngine;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::sync::RwLock;
use tracing::{info, warn};

pub const SLOT_STATUS_PUBLISHED: &str = "Published";
pub const SLOT_STATUS_FAILED: &str = "Failed";
pub const SLOT_STATUS_NEEDS_ACCOUNT: &str = "NeedsAccount";
pub const DEFAULT_DAILY_POST_CAP: u32 = 5;
pub const VISION_PROFILE_ID: &str = "nim-vision-cascade";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SubAgentRole {
    ContentStrategist,
    TrendResearcher,
    Copywriter,
    GraphicDesigner,
    Illustrator,
    VideoDirector,
    Animator,
    ThreeDArtist,
    VideoEditor,
    Publisher,
    CommunityManager,
    EngagementAgent,
    AnalyticsAgent,
    ComplianceAgent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubAgentAssignment {
    pub role: SubAgentRole,
    pub title: String,
    pub current_task: String,
    pub assigned_model_profile: String,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutonomousStateCheckpoint {
    pub checkpoint_id: String,
    pub last_cycle_timestamp: DateTime<Utc>,
    pub processed_slots_count: usize,
    pub published_idempotency_keys: Vec<String>,
    pub pending_human_approvals_count: usize,
    pub is_loop_active: bool,
    #[serde(default)]
    pub failed_slots_count: usize,
    #[serde(default)]
    pub skipped_slots_count: usize,
    #[serde(default)]
    pub deferred_slots_count: usize,
}

#[derive(Default)]
struct IdempotencyLedger {
    published: HashSet<String>,
    in_flight: HashSet<String>,
}

pub fn slot_idempotency_key(slot: &CalendarPostSlot) -> String {
    format!("slot-{}-{}", slot.slot_id, slot.target_platform.to_lowercase())
}

fn parse_account_id(raw: &str) -> Option<AccountId> {
    uuid::Uuid::parse_str(raw.trim()).ok().map(AccountId)
}

pub struct SocialMediaManagerAgent {
    post_caps: Arc<RwLock<std::collections::HashMap<String, u32>>>,
    pub brand_manager: Arc<BrandManager>,
    pub content_strategy: Arc<ContentStrategyEngine>,
    pub creative_pipeline: Arc<CreativePipeline>,
    pub publishing_engine: Arc<PublishingEngine>,
    pub engagement_agent: Arc<EngagementAgent>,
    pub social_analytics: Arc<SocialAnalyticsEngine>,
    pub stick_bridge: Arc<StickExplainersBridge>,
    pub is_autonomous_loop_running: Arc<AtomicBool>,
    lm_gateway: Option<Arc<LMGateway>>,
    ledger: Arc<RwLock<IdempotencyLedger>>,
    last_checkpoint: Arc<RwLock<Option<AutonomousStateCheckpoint>>>,
}

impl SocialMediaManagerAgent {
    pub fn new(
        brand_manager: Arc<BrandManager>,
        content_strategy: Arc<ContentStrategyEngine>,
        creative_pipeline: Arc<CreativePipeline>,
        publishing_engine: Arc<PublishingEngine>,
        engagement_agent: Arc<EngagementAgent>,
        social_analytics: Arc<SocialAnalyticsEngine>,
    ) -> Self {
        Self {
            brand_manager,
            content_strategy,
            creative_pipeline,
            publishing_engine,
            engagement_agent,
            social_analytics,
            stick_bridge: Arc::new(StickExplainersBridge::new()),
            is_autonomous_loop_running: Arc::new(AtomicBool::new(false)),
            lm_gateway: None,
            post_caps: Arc::new(RwLock::new(std::collections::HashMap::new())),
            ledger: Arc::new(RwLock::new(IdempotencyLedger::default())),
            last_checkpoint: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn set_daily_post_cap(&self, account_id: &str, cap: u32) {
        self.post_caps.write().await.insert(account_id.to_string(), cap);
    }

    pub async fn daily_post_cap(&self, account_id: &str) -> u32 {
        self.post_caps.read().await.get(account_id).copied().unwrap_or(DEFAULT_DAILY_POST_CAP)
    }

    pub fn with_lm_gateway(mut self, lm_gateway: Arc<LMGateway>) -> Self {
        self.lm_gateway = Some(lm_gateway);
        self
    }

    pub fn start_autonomous_loop(&self) {
        self.is_autonomous_loop_running.store(true, Ordering::SeqCst);
        info!("Autonomous Social Media Marketing Loop ACTIVATED.");
    }

    pub fn pause_autonomous_loop(&self) {
        self.is_autonomous_loop_running.store(false, Ordering::SeqCst);
        warn!("Autonomous Social Media Marketing Loop PAUSED.");
    }

    pub fn is_loop_running(&self) -> bool {
        self.is_autonomous_loop_running.load(Ordering::SeqCst)
    }

    pub fn spawn_autonomous_loop(self: Arc<Self>, interval: Duration) -> tokio::task::JoinHandle<()> {
        let interval = interval.max(Duration::from_secs(1));
        tokio::spawn(async move {
            loop {
                if self.is_loop_running() {
                    if let Err(e) = self.execute_autonomous_cycle().await {
                        warn!("Autonomous marketing cycle failed: {}", e);
                    }
                }
                tokio::time::sleep(interval).await;
            }
        })
    }

    async fn reserve(&self, key: &str) -> bool {
        let mut ledger = self.ledger.write().await;
        if ledger.published.contains(key) || ledger.in_flight.contains(key) {
            return false;
        }
        ledger.in_flight.insert(key.to_string());
        true
    }

    async fn release(&self, key: &str, published: bool) {
        let mut ledger = self.ledger.write().await;
        ledger.in_flight.remove(key);
        if published {
            ledger.published.insert(key.to_string());
        }
    }

    pub async fn execute_autonomous_cycle(&self) -> Result<AutonomousStateCheckpoint> {
        info!("Beginning autonomous marketing operations cycle...");
        let now = Utc::now();
        let slots = self.content_strategy.list_calendar().await;

        let mut published_count = 0;
        let mut pending_approvals = 0;
        let mut failed = 0;
        let mut skipped = 0;
        let mut deferred = 0;
        let window_start = now - chrono::Duration::hours(24);
        let mut posted_today: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
        for s in slots.iter().filter(|s| s.published_at.is_some_and(|t| t > window_start)) {
            if let Some(acc) = &s.account_id {
                *posted_today.entry(acc.clone()).or_default() += 1;
            }
        }

        for slot in &slots {
            let key = slot_idempotency_key(slot);
            if slot.status == SLOT_STATUS_PUBLISHED || self.ledger.read().await.published.contains(&key) {
                continue;
            }
            if !slot.is_approved {
                pending_approvals += 1;
                continue;
            }
            if slot.scheduled_time > now {
                deferred += 1;
                continue;
            }
            let Some(account_id) = slot.account_id.as_deref().and_then(parse_account_id) else {
                skipped += 1;
                warn!("Slot {} has no valid account assigned; flagging for review", slot.slot_id);
                self.content_strategy
                    .update_slot_status(&slot.slot_id, SLOT_STATUS_NEEDS_ACCOUNT)
                    .await;
                continue;
            };
            let account_key = slot.account_id.clone().unwrap_or_default();
            let cap = self.daily_post_cap(&account_key).await;
            if posted_today.get(&account_key).copied().unwrap_or(0) >= cap {
                deferred += 1;
                info!("Slot {} deferred: account reached its {} posts/24h cap", slot.slot_id, cap);
                continue;
            }
            if !self.reserve(&key).await {
                continue;
            }

            let payload = SocialPostPayload {
                post_id: format!("post-{}", slot.slot_id),
                account_id,
                platform: slot.target_platform.clone(),
                caption: slot.caption.clone(),
                hashtags: slot.hashtags.clone(),
                media_paths: slot.asset_ids.clone(),
                idempotency_key: key.clone(),
                scheduled_at: Some(slot.scheduled_time),
                requires_human_approval: false,
                is_approved: true,
                status: "Queued".into(),
                ..Default::default()
            };

            match self.publishing_engine.queue_post(payload).await {
                Ok(_) => {
                    self.release(&key, true).await;
                    self.content_strategy
                        .mark_published(&slot.slot_id, SLOT_STATUS_PUBLISHED)
                        .await;
                    *posted_today.entry(account_key.clone()).or_default() += 1;
                    published_count += 1;
                }
                Err(e) => {
                    self.release(&key, false).await;
                    self.content_strategy
                        .update_slot_status(&slot.slot_id, SLOT_STATUS_FAILED)
                        .await;
                    failed += 1;
                    warn!("Failed to publish slot {}: {}", slot.slot_id, e);
                }
            }
        }

        let checkpoint = AutonomousStateCheckpoint {
            checkpoint_id: uuid::Uuid::new_v4().to_string(),
            last_cycle_timestamp: Utc::now(),
            processed_slots_count: published_count,
            published_idempotency_keys: self.ledger.read().await.published.iter().cloned().collect(),
            pending_human_approvals_count: pending_approvals,
            is_loop_active: self.is_loop_running(),
            failed_slots_count: failed,
            skipped_slots_count: skipped,
            deferred_slots_count: deferred,
        };

        *self.last_checkpoint.write().await = Some(checkpoint.clone());
        info!(
            "Autonomous cycle completed: {} published, {} failed, {} awaiting approval, {} need an account, {} scheduled later",
            published_count, failed, pending_approvals, skipped, deferred
        );
        Ok(checkpoint)
    }

    pub async fn restore_from_checkpoint(&self, checkpoint: AutonomousStateCheckpoint) -> Result<()> {
        info!("Restoring autonomous state from checkpoint {}", checkpoint.checkpoint_id);
        {
            let mut ledger = self.ledger.write().await;
            ledger.published.extend(checkpoint.published_idempotency_keys.iter().cloned());
        }
        if checkpoint.is_loop_active {
            self.start_autonomous_loop();
        }
        *self.last_checkpoint.write().await = Some(checkpoint);
        Ok(())
    }

    pub async fn get_last_checkpoint(&self) -> Option<AutonomousStateCheckpoint> {
        self.last_checkpoint.read().await.clone()
    }

    pub fn get_stick_bridge_state(&self) -> StickBridgeState {
        self.stick_bridge.load_state()
    }

    pub fn run_garbage_collection(&self, apply: bool) -> Result<GarbageCollectionReport> {
        self.stick_bridge.run_garbage_collection(apply)
    }

    pub fn send_stick_bridge_message(
        &self,
        from_agent: &str,
        to_agent: &str,
        channel: &str,
        subject: &str,
        body: &str,
        create_ceo_directive: bool,
    ) -> Result<StickBridgeMessage> {
        self.stick_bridge.send_message(
            from_agent,
            to_agent,
            channel,
            subject,
            body,
            "openaccounts_to_stick",
            create_ceo_directive,
        )
    }

    async fn profile_label(&self, profile_id: &str) -> String {
        let Some(gateway) = self.lm_gateway.as_ref() else {
            return "No LM gateway attached".into();
        };
        match gateway.list_profiles().await.into_iter().find(|p| p.id == profile_id) {
            Some(p) => format!("{} · {}", p.id, p.model_id),
            None => format!("Profile '{profile_id}' not registered"),
        }
    }

    pub async fn list_collaborating_agents(&self) -> Vec<SubAgentAssignment> {
        let running = self.is_loop_running();
        let slots = self.content_strategy.list_calendar().await;
        let open_slots: Vec<&CalendarPostSlot> =
            slots.iter().filter(|s| s.status != SLOT_STATUS_PUBLISHED).collect();
        let awaiting_slot_approval = open_slots.iter().filter(|s| !s.is_approved).count();
        let needs_account = open_slots
            .iter()
            .filter(|s| s.account_id.as_deref().and_then(parse_account_id).is_none())
            .count();

        let inbox_pending = self.engagement_agent.pending_inbox_count().await;
        let posts = self.publishing_engine.list_posts().await;
        let posts_awaiting = posts.iter().filter(|p| p.status == STATUS_AWAITING_APPROVAL).count();
        let posts_failed = posts.iter().filter(|p| p.status == STATUS_FAILED).count();
        let tools = self.creative_pipeline.list_tools().await;
        let tools_ready = tools.iter().filter(|t| t.is_available).count();
        let metrics = self.social_analytics.list_metrics().await;
        let last_capture = metrics.first().map(|m| m.captured_at);
        let checkpoint = self.get_last_checkpoint().await;

        let text_profile = self.profile_label(DEFAULT_PROFILE_ID).await;
        let vision_profile = self.profile_label(VISION_PROFILE_ID).await;

        vec![
            SubAgentAssignment {
                role: SubAgentRole::ContentStrategist,
                title: "Head of Marketing Strategy".into(),
                current_task: format!(
                    "{} open calendar slots · {} awaiting approval · {} without an account",
                    open_slots.len(),
                    awaiting_slot_approval,
                    needs_account
                ),
                assigned_model_profile: text_profile.clone(),
                is_active: running && !open_slots.is_empty(),
            },
            SubAgentAssignment {
                role: SubAgentRole::TrendResearcher,
                title: "Live Trend Intelligence Agent".into(),
                current_task: "On demand: Reddit r/popular, Google Trends daily RSS, YouTube most popular".into(),
                assigned_model_profile: "Public trend sources".into(),
                is_active: running,
            },
            SubAgentAssignment {
                role: SubAgentRole::GraphicDesigner,
                title: "Creative Production Specialist".into(),
                current_task: format!("{} of {} creative tools detected on this machine", tools_ready, tools.len()),
                assigned_model_profile: vision_profile,
                is_active: running && tools_ready > 0,
            },
            SubAgentAssignment {
                role: SubAgentRole::Publisher,
                title: "Cross-Platform Publisher".into(),
                current_task: match checkpoint {
                    Some(cp) => format!(
                        "Last cycle {}: {} published, {} failed · {} posts awaiting approval, {} failed overall",
                        cp.last_cycle_timestamp.format("%Y-%m-%d %H:%M UTC"),
                        cp.processed_slots_count,
                        cp.failed_slots_count,
                        posts_awaiting,
                        posts_failed
                    ),
                    None => format!("No cycle run yet · {} posts awaiting approval", posts_awaiting),
                },
                assigned_model_profile: "Official platform APIs".into(),
                is_active: running,
            },
            SubAgentAssignment {
                role: SubAgentRole::CommunityManager,
                title: "Community Moderation & Engagement".into(),
                current_task: format!("{} inbox messages awaiting a response", inbox_pending),
                assigned_model_profile: text_profile,
                is_active: running && inbox_pending > 0,
            },
            SubAgentAssignment {
                role: SubAgentRole::AnalyticsAgent,
                title: "Performance Analytics".into(),
                current_task: match last_capture {
                    Some(at) => format!("{} API snapshots · latest {}", metrics.len(), at.format("%Y-%m-%d %H:%M UTC")),
                    None => "No platform metrics captured yet".into(),
                },
                assigned_model_profile: "Official platform APIs".into(),
                is_active: running,
            },
            SubAgentAssignment {
                role: SubAgentRole::ComplianceAgent,
                title: "Policy & Rate Limit Gatekeeper".into(),
                current_task: format!("Gating {} publishing records against platform policy", posts.len()),
                assigned_model_profile: "Deterministic Policy Engine".into(),
                is_active: true,
            },
        ]
    }
}
