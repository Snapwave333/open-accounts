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
    /// No platform metrics source is connected yet, so there is no data.
    pub fn new() -> Self {
        Self {
            metrics: Arc::new(RwLock::new(Vec::new())),
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
