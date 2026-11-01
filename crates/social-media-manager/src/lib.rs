use openaccounts_brand_manager::BrandManager;
use openaccounts_content_strategy::ContentStrategyEngine;
use openaccounts_creative_pipeline::CreativePipeline;
use openaccounts_engagement_agent::EngagementAgent;
use openaccounts_publishing_engine::PublishingEngine;
use openaccounts_social_analytics::SocialAnalyticsEngine;
use serde::{Deserialize, Serialize};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
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

pub struct SocialMediaManagerAgent {
    pub brand_manager: Arc<BrandManager>,
    pub content_strategy: Arc<ContentStrategyEngine>,
    pub creative_pipeline: Arc<CreativePipeline>,
    pub publishing_engine: Arc<PublishingEngine>,
    pub engagement_agent: Arc<EngagementAgent>,
    pub social_analytics: Arc<SocialAnalyticsEngine>,
    pub is_autonomous_loop_running: Arc<AtomicBool>,
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
            is_autonomous_loop_running: Arc::new(AtomicBool::new(false)),
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

    pub fn list_collaborating_agents(&self) -> Vec<SubAgentAssignment> {
        vec![
            SubAgentAssignment {
                role: SubAgentRole::ContentStrategist,
                title: "Head of Marketing Strategy".into(),
                current_task: "Synthesizing cross-platform audience briefs".into(),
                assigned_model_profile: "Claude 3.5 Sonnet".into(),
                is_active: true,
            },
            SubAgentAssignment {
                role: SubAgentRole::TrendResearcher,
                title: "Live Trend Intelligence Agent".into(),
                current_task: "Scanning viral hashtag velocity on TikTok and X".into(),
                assigned_model_profile: "Gemini 1.5 Flash".into(),
                is_active: true,
            },
            SubAgentAssignment {
                role: SubAgentRole::GraphicDesigner,
                title: "Vector & Photo Production Specialist".into(),
                current_task: "Driving Vector Craft CLI and Photo Craft CLI".into(),
                assigned_model_profile: "Local Tool Router".into(),
                is_active: true,
            },
            SubAgentAssignment {
                role: SubAgentRole::VideoDirector,
                title: "Short-Form Video & Reel Director".into(),
                current_task: "Assembling 9:16 vertical reels with FFmpeg and Effect Craft".into(),
                assigned_model_profile: "GPT-4o".into(),
                is_active: true,
            },
            SubAgentAssignment {
                role: SubAgentRole::CommunityManager,
                title: "Community Moderation & Engagement".into(),
                current_task: "Classifying incoming audience comments and inquiries".into(),
                assigned_model_profile: "Ollama Llama-3.1".into(),
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
