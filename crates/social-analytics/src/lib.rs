use chrono::{DateTime, Utc};
use openaccounts_core::Result;
use openaccounts_platform_connectors::{normalize_platform, ConnectorRegistry, RemoteMetrics};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;
use uuid::Uuid;

const MAX_STORED_METRICS: usize = 1000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifiedPlatformMetric {
    pub metric_id: String,
    pub platform: String,
    pub account_id: String,
    pub post_id: Option<String>,
    pub impressions: u64,
    pub reach: u64,
    pub views: u64,
    pub engagement_rate: f32,
    pub comments: u32,
    pub shares: u32,
    pub saves: u32,
    pub conversions: u32,
    pub is_verified_by_api: bool,
    pub captured_at: DateTime<Utc>,
    #[serde(default)]
    pub followers: Option<u64>,
    #[serde(default)]
    pub posts: Option<u64>,
    #[serde(default)]
    pub likes: Option<u64>,
}

fn clamp_u32(v: Option<u64>) -> u32 {
    v.unwrap_or(0).min(u32::MAX as u64) as u32
}

pub fn engagement_rate(metrics: &RemoteMetrics) -> f32 {
    let interactions = metrics.likes.unwrap_or(0) + metrics.comments.unwrap_or(0) + metrics.shares.unwrap_or(0);
    let audience = metrics
        .impressions
        .or(metrics.views)
        .or(metrics.reach)
        .or(metrics.followers)
        .unwrap_or(0);
    if interactions == 0 || audience == 0 {
        return 0.0;
    }
    ((interactions as f64 / audience as f64) * 100.0) as f32
}

pub fn metric_from_remote(account_id: &str, platform: &str, metrics: &RemoteMetrics) -> VerifiedPlatformMetric {
    VerifiedPlatformMetric {
        metric_id: Uuid::new_v4().to_string(),
        platform: normalize_platform(platform),
        account_id: account_id.to_string(),
        post_id: None,
        impressions: metrics.impressions.unwrap_or(0),
        reach: metrics.reach.unwrap_or(0),
        views: metrics.views.unwrap_or(0),
        engagement_rate: engagement_rate(metrics),
        comments: clamp_u32(metrics.comments),
        shares: clamp_u32(metrics.shares),
        saves: 0,
        conversions: 0,
        is_verified_by_api: true,
        captured_at: Utc::now(),
        followers: metrics.followers,
        posts: metrics.posts,
        likes: metrics.likes,
    }
}

pub struct SocialAnalyticsEngine {
    connectors: Arc<ConnectorRegistry>,
    metrics: Arc<RwLock<Vec<VerifiedPlatformMetric>>>,
}

impl SocialAnalyticsEngine {
    pub fn new() -> Self {
        Self::with_connectors(Arc::new(ConnectorRegistry::from_env()))
    }

    pub fn with_connectors(connectors: Arc<ConnectorRegistry>) -> Self {
        Self { connectors, metrics: Arc::new(RwLock::new(Vec::new())) }
    }

    pub async fn refresh_metrics(&self, account_id: &str, platform: &str) -> Result<VerifiedPlatformMetric> {
        let connector = self.connectors.connector_for(platform).await?;
        let remote = connector.fetch_metrics().await?;
        let metric = metric_from_remote(account_id, platform, &remote);
        let mut stored = self.metrics.write().await;
        stored.push(metric.clone());
        if stored.len() > MAX_STORED_METRICS {
            let excess = stored.len() - MAX_STORED_METRICS;
            stored.drain(..excess);
        }
        info!("Captured {} metrics for account {}", metric.platform, account_id);
        Ok(metric)
    }

    pub async fn list_metrics(&self) -> Vec<VerifiedPlatformMetric> {
        let mut rows = self.metrics.read().await.clone();
        rows.sort_by_key(|m| std::cmp::Reverse(m.captured_at));
        rows
    }

    pub async fn latest_for(&self, account_id: &str, platform: &str) -> Option<VerifiedPlatformMetric> {
        let platform = normalize_platform(platform);
        self.metrics
            .read()
            .await
            .iter()
            .filter(|m| m.account_id == account_id && m.platform == platform)
            .max_by_key(|m| m.captured_at)
            .cloned()
    }
}

impl Default for SocialAnalyticsEngine {
    fn default() -> Self {
        Self::new()
    }
}
