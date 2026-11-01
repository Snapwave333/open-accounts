use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifiedPlatformMetric {
    pub metric_id: String,
    pub platform: String,
    pub account_id: String,
    pub post_id: Option<String>,
    pub impressions: u64,
    pub reach: u64,
    pub views: u64,
    pub engagement_rate: f32, // percentage
    pub comments: u32,
    pub shares: u32,
    pub saves: u32,
    pub conversions: u32,
    pub is_verified_by_api: bool,
    pub captured_at: DateTime<Utc>,
}

pub struct SocialAnalyticsEngine {
    metrics: Arc<RwLock<Vec<VerifiedPlatformMetric>>>,
}

impl SocialAnalyticsEngine {
    pub fn new() -> Self {
        let default_metrics = vec![
            VerifiedPlatformMetric {
                metric_id: "m-01".into(),
                platform: "instagram".into(),
                account_id: "acc-1".into(),
                post_id: Some("post-991".into()),
                impressions: 48200,
                reach: 39500,
                views: 28400,
                engagement_rate: 6.8,
                comments: 312,
                shares: 124,
                saves: 840,
                conversions: 45,
                is_verified_by_api: true,
                captured_at: Utc::now(),
            },
            VerifiedPlatformMetric {
                metric_id: "m-02".into(),
                platform: "youtube".into(),
                account_id: "acc-3".into(),
                post_id: Some("short-112".into()),
                impressions: 112000,
                reach: 89000,
                views: 74200,
                engagement_rate: 8.2,
                comments: 540,
                shares: 410,
                saves: 1200,
                conversions: 89,
                is_verified_by_api: true,
                captured_at: Utc::now(),
            },
        ];

        Self {
            metrics: Arc::new(RwLock::new(default_metrics)),
        }
    }

    pub async fn list_metrics(&self) -> Vec<VerifiedPlatformMetric> {
        self.metrics.read().await.clone()
    }
}

impl Default for SocialAnalyticsEngine {
    fn default() -> Self {
        Self::new()
    }
}
