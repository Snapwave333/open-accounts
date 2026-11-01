use async_trait::async_trait;
use chrono::{DateTime, Utc};
use openaccounts_core::{OpenAccountsError, Result};
use openaccounts_platform_connectors::{
    ConnectorRegistry, PlatformAuthInfo, PlatformConnector, PublishRequest, RemoteActionReceipt, RemoteInteraction,
    RemoteMetrics, StaticTokenSource,
};
use openaccounts_social_analytics::{engagement_rate, SocialAnalyticsEngine};
use std::sync::Arc;

struct MetricsOnly(RemoteMetrics);

#[async_trait]
impl PlatformConnector for MetricsOnly {
    fn platform(&self) -> &str {
        "youtube"
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
        unreachable!()
    }
    async fn fetch_inbox(&self, _: Option<DateTime<Utc>>, _: u32) -> Result<Vec<RemoteInteraction>> {
        Ok(vec![])
    }
    async fn fetch_metrics(&self) -> Result<RemoteMetrics> {
        Ok(self.0.clone())
    }
}

#[tokio::test]
async fn analytics_start_empty_and_store_api_sourced_rows() {
    let remote = RemoteMetrics { followers: Some(1200), posts: Some(48), views: Some(90_000), ..Default::default() };
    let registry = ConnectorRegistry::new(Arc::new(StaticTokenSource::new()))
        .with_connector("youtube", Arc::new(MetricsOnly(remote)));
    let engine = SocialAnalyticsEngine::with_connectors(Arc::new(registry));
    assert!(engine.list_metrics().await.is_empty());

    let row = engine.refresh_metrics("acc-1", "YouTube").await.unwrap();
    assert!(row.is_verified_by_api);
    assert_eq!(row.platform, "youtube");
    assert_eq!(row.followers, Some(1200));
    assert_eq!(row.views, 90_000);
    assert_eq!(engine.list_metrics().await.len(), 1);
    assert!(engine.latest_for("acc-1", "youtube").await.is_some());

    let err = engine.refresh_metrics("acc-2", "x").await.unwrap_err();
    assert!(err.to_string().contains("x is not connected"));
}

#[test]
fn engagement_rate_uses_real_counts_only() {
    assert_eq!(engagement_rate(&RemoteMetrics::default()), 0.0);
    let m = RemoteMetrics { likes: Some(50), comments: Some(30), shares: Some(20), views: Some(1000), ..Default::default() };
    assert!((engagement_rate(&m) - 10.0).abs() < 0.001);
}
