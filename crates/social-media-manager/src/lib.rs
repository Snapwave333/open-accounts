pub mod bridge;

use crate::bridge::{
    GarbageCollectionReport, StickBridgeMessage, StickBridgeState, StickExplainersBridge,
};
use chrono::{DateTime, Utc};
use openaccounts_brand_manager::BrandManager;
use openaccounts_content_strategy::ContentStrategyEngine;
use openaccounts_core::Result;
use openaccounts_creative_pipeline::CreativePipeline;
use openaccounts_engagement_agent::EngagementAgent;
use openaccounts_publishing_engine::{PublishingEngine, SocialPostPayload};
use openaccounts_social_analytics::SocialAnalyticsEngine;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tokio::sync::RwLock;
use tracing::{info, warn};

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
}

pub struct SocialMediaManagerAgent {
    pub brand_manager: Arc<BrandManager>,
    pub content_strategy: Arc<ContentStrategyEngine>,
    pub creative_pipeline: Arc<CreativePipeline>,
    pub publishing_engine: Arc<PublishingEngine>,
    pub engagement_agent: Arc<EngagementAgent>,
    pub social_analytics: Arc<SocialAnalyticsEngine>,
    pub stick_bridge: Arc<StickExplainersBridge>,
    pub is_autonomous_loop_running: Arc<AtomicBool>,
    published_idempotency_keys: Arc<RwLock<HashSet<String>>>,
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
            published_idempotency_keys: Arc::new(RwLock::new(HashSet::new())),
            last_checkpoint: Arc::new(RwLock::new(None)),
        }
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

    /// Executes one full autonomous campaign cycle with restart recovery,
    /// duplicate prevention, policy enforcement, and human approval checkpoints.
    pub async fn execute_autonomous_cycle(&self) -> Result<AutonomousStateCheckpoint> {
        info!("Beginning autonomous marketing operations cycle...");

        // 1. Fetch scheduled posts from Content Calendar
        let slots = self.content_strategy.list_calendar().await;
        let mut published_count = 0;
        let mut pending_approvals = 0;

        for slot in &slots {
            // Duplicate prevention via idempotency key
            let idempotency_key = format!("slot-{}-{}", slot.slot_id, slot.target_platform);
            {
                let keys = self.published_idempotency_keys.read().await;
                if keys.contains(&idempotency_key) {
                    continue; // Skip duplicate publication
                }
            }

            // Human approval checkpoint
            if !slot.is_approved {
                pending_approvals += 1;
                info!(
                    "Slot {} requires human approval. Pausing publication until user reviews caption and assets.",
                    slot.slot_id
                );
                continue;
            }

            // Stage post for publishing
            let post_payload = SocialPostPayload {
                post_id: format!("post-{}", slot.slot_id),
                account_id: openaccounts_core::AccountId::new(),
                platform: slot.target_platform.clone(),
                caption: slot.caption.clone(),
                hashtags: slot.hashtags.clone(),
                media_paths: slot.asset_ids.clone(),
                idempotency_key: idempotency_key.clone(),
                scheduled_at: Some(slot.scheduled_time),
                requires_human_approval: false,
                is_approved: true,
                retry_count: 0,
                status: "Published".into(),
            };

            // Queue and dispatch post with policy verification
            match self.publishing_engine.queue_post(post_payload).await {
                Ok(_) => {
                    self.published_idempotency_keys.write().await.insert(idempotency_key);
                    published_count += 1;
                }
                Err(e) => {
                    warn!("Failed to publish slot {}: {:?}", slot.slot_id, e);
                }
            }
        }

        // Create recovery checkpoint
        let checkpoint = AutonomousStateCheckpoint {
            checkpoint_id: uuid::Uuid::new_v4().to_string(),
            last_cycle_timestamp: Utc::now(),
            processed_slots_count: published_count,
            published_idempotency_keys: self.published_idempotency_keys.read().await.iter().cloned().collect(),
            pending_human_approvals_count: pending_approvals,
            is_loop_active: self.is_loop_running(),
        };

        *self.last_checkpoint.write().await = Some(checkpoint.clone());
        info!("Autonomous cycle completed. Checkpoint saved: {:?}", checkpoint.checkpoint_id);
        Ok(checkpoint)
    }

    /// Restore state from a previously saved checkpoint (restart recovery)
    pub async fn restore_from_checkpoint(&self, checkpoint: AutonomousStateCheckpoint) -> Result<()> {
        info!("Restoring autonomous state from checkpoint {}", checkpoint.checkpoint_id);
        let mut keys = self.published_idempotency_keys.write().await;
        for key in &checkpoint.published_idempotency_keys {
            keys.insert(key.clone());
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

    pub fn list_collaborating_agents(&self) -> Vec<SubAgentAssignment> {
        vec![
            SubAgentAssignment {
                role: SubAgentRole::ContentStrategist,
                title: "Head of Marketing Strategy".into(),
                current_task: "Synthesizing cross-platform audience briefs".into(),
                assigned_model_profile: "NIM: nvidia/nemotron-3.5-super-vl-120b-a12b".into(),
                is_active: true,
            },
            SubAgentAssignment {
                role: SubAgentRole::TrendResearcher,
                title: "Live Trend Intelligence Agent".into(),
                current_task: "Scanning viral hashtag velocity on TikTok and X".into(),
                assigned_model_profile: "NIM: nvidia/nemotron-3-nano-omni-30b-a3b-reasoning".into(),
                is_active: true,
            },
            SubAgentAssignment {
                role: SubAgentRole::GraphicDesigner,
                title: "Vector & Photo Production Specialist".into(),
                current_task: "Driving Vector Craft CLI and Photo Craft CLI".into(),
                assigned_model_profile: "NIM Vision: meta/llama-3.2-90b-vision-instruct".into(),
                is_active: true,
            },
            SubAgentAssignment {
                role: SubAgentRole::VideoDirector,
                title: "Short-Form Video & Reel Director".into(),
                current_task: "Assembling 9:16 vertical reels with FFmpeg and Effect Craft".into(),
                assigned_model_profile: "NIM: meta/muse-glimmer-30b".into(),
                is_active: true,
            },
            SubAgentAssignment {
                role: SubAgentRole::CommunityManager,
                title: "Community Moderation & Engagement".into(),
                current_task: "Classifying incoming audience comments and inquiries".into(),
                assigned_model_profile: "NIM: nvidia/nemotron-3.5-lightning-30b-a3b".into(),
                is_active: true,
            },
            SubAgentAssignment {
                role: SubAgentRole::ComplianceAgent,
                title: "Policy & Rate Limit Gatekeeper".into(),
                current_task: "Enforcing official Graph and X API compliance limits".into(),
                assigned_model_profile: "Deterministic Policy Engine".into(),
                is_active: true,
            },
        ]
    }
}
