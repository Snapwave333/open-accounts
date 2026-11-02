pub mod bridge;

use crate::bridge::{
    GarbageCollectionReport, StickBridgeMessage, StickBridgeState, StickExplainersBridge,
};
use openaccounts_brand_manager::BrandManager;
use openaccounts_content_strategy::ContentStrategyEngine;
use openaccounts_core::Result;
use openaccounts_creative_pipeline::CreativePipeline;
use openaccounts_engagement_agent::EngagementAgent;
use openaccounts_publishing_engine::PublishingEngine;
use openaccounts_social_analytics::SocialAnalyticsEngine;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

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
    pub stick_bridge: Arc<StickExplainersBridge>,
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
        }
    }

    pub fn get_stick_bridge_state(&self) -> StickBridgeState {
        self.stick_bridge.load_state()
    }

    pub fn run_garbage_collection(&self, apply: bool) -> Result<GarbageCollectionReport> {
        self.stick_bridge.run_garbage_collection(apply)
    }

    pub fn read_gc_report(&self) -> Option<GarbageCollectionReport> {
        self.stick_bridge.read_gc_report()
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
}
