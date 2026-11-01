use async_trait::async_trait;
use chrono::{DateTime, Utc};
use openaccounts_core::{AccountId, OpenAccountsError, Result};
use openaccounts_platform_connectors::{
    ConnectorRegistry, PlatformAuthInfo, PlatformConnector, PublishRequest, RemoteActionReceipt, RemoteInteraction,
    RemoteMetrics, StaticTokenSource,
};
use openaccounts_platform_intelligence::PlatformIntelligence;
use openaccounts_policy_engine::PolicyEngine;
use openaccounts_publishing_engine::{
    PublishingEngine, SocialPostPayload, STATUS_AWAITING_APPROVAL, STATUS_FAILED, STATUS_PUBLISHED,
};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Mutex,
};

struct FakePublisher {
    platform: String,
    published: Mutex<Vec<PublishRequest>>,
    fail: AtomicBool,
    counter: AtomicUsize,
}

impl FakePublisher {
    fn new(platform: &str) -> Arc<Self> {
        Arc::new(Self {
            platform: platform.into(),
            published: Mutex::new(vec![]),
            fail: AtomicBool::new(false),
            counter: AtomicUsize::new(0),
        })
    }
}

#[async_trait]
impl PlatformConnector for FakePublisher {
    fn platform(&self) -> &str {
        &self.platform
    }
    async fn verify_auth(&self) -> Result<PlatformAuthInfo> {
        Ok(PlatformAuthInfo {
            platform: self.platform.clone(),
            account_handle: Some("@brand".into()),
            account_remote_id: Some("17841".into()),
            scopes: vec!["instagram_content_publish".into()],
            is_authenticated: true,
            error: None,
        })
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
    async fn publish(&self, request: PublishRequest) -> Result<RemoteActionReceipt> {
        if self.fail.load(Ordering::SeqCst) {
            return Err(OpenAccountsError::Automation("media container rejected".into()));
        }
        let n = self.counter.fetch_add(1, Ordering::SeqCst) + 1;
        self.published.lock().unwrap().push(request);
        Ok(RemoteActionReceipt {
            remote_id: format!("media-{n}"),
            permalink: Some(format!("https://www.instagram.com/p/media-{n}/")),
        })
    }
    async fn fetch_inbox(&self, _: Option<DateTime<Utc>>, _: u32) -> Result<Vec<RemoteInteraction>> {
        Ok(vec![])
    }
    async fn fetch_metrics(&self) -> Result<RemoteMetrics> {
        Ok(RemoteMetrics::default())
    }
}

fn engine_with(connector: Option<Arc<FakePublisher>>) -> PublishingEngine {
    let intel = Arc::new(PlatformIntelligence::new());
    let policy = Arc::new(PolicyEngine::new(intel));
    let mut registry = ConnectorRegistry::new(Arc::new(StaticTokenSource::new()));
    if let Some(c) = connector {
        let platform = c.platform.clone();
        registry = registry.with_connector(&platform, c);
    }
    PublishingEngine::with_connectors(policy, Arc::new(registry))
}

fn post(id: &str, key: &str, needs_approval: bool) -> SocialPostPayload {
    SocialPostPayload {
        post_id: id.into(),
        account_id: AccountId::new(),
        platform: "instagram".into(),
        caption: "Protecting digital autonomy with local credentials.".into(),
        hashtags: vec!["#security".into()],
        media_paths: vec!["https://cdn.example.com/reel.mp4".into()],
        idempotency_key: key.into(),
        scheduled_at: Some(Utc::now()),
        requires_human_approval: needs_approval,
        is_approved: false,
        status: "Draft".into(),
        ..Default::default()
    }
}

#[tokio::test]
async fn auth_status_comes_from_the_platform_not_fabricated_scopes() {
    let engine = engine_with(Some(FakePublisher::new("instagram")));
    let account = AccountId::new();
    let ig = engine.verify_platform_auth("instagram", &account).await.unwrap();
    assert!(ig.is_authenticated);
    assert_eq!(ig.account_handle.as_deref(), Some("@brand"));

    let yt = engine.verify_platform_auth("youtube", &account).await.unwrap();
    assert!(!yt.is_authenticated);
    assert!(yt.scopes.is_empty());
    assert!(yt.error_message.unwrap().contains("YOUTUBE_ACCESS_TOKEN"));
}

#[tokio::test]
async fn approval_gate_then_real_publish_with_receipt() {
    let fake = FakePublisher::new("instagram");
    let engine = engine_with(Some(fake.clone()));

    let id = engine.queue_post(post("p1", "k1", true)).await.unwrap();
    assert_eq!(engine.get_post(&id).await.unwrap().status, STATUS_AWAITING_APPROVAL);
    assert!(fake.published.lock().unwrap().is_empty());

    let result = engine.approve_and_publish(&id).await.unwrap();
    assert_eq!(result.remote_media_id, "media-1");
    assert!(result.remote_permalink.unwrap().contains("instagram.com"));
    let stored = engine.get_post(&id).await.unwrap();
    assert_eq!(stored.status, STATUS_PUBLISHED);
    assert_eq!(stored.remote_id.as_deref(), Some("media-1"));

    let again = engine.approve_and_publish(&id).await;
    assert!(again.unwrap_err().to_string().contains("already published"));

    let dup = engine.queue_post(post("p2", "k1", false)).await;
    assert!(dup.unwrap_err().to_string().contains("Duplicate"));
    assert_eq!(fake.published.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn failures_are_recorded_and_retryable() {
    let fake = FakePublisher::new("instagram");
    fake.fail.store(true, Ordering::SeqCst);
    let engine = engine_with(Some(fake.clone()));

    let err = engine.queue_post(post("p3", "k3", false)).await.unwrap_err();
    assert!(err.to_string().contains("rejected"));
    let stored = engine.get_post("p3").await.unwrap();
    assert_eq!(stored.status, STATUS_FAILED);
    assert_eq!(stored.retry_count, 1);
    assert!(stored.error_message.is_some());

    fake.fail.store(false, Ordering::SeqCst);
    engine.queue_post(post("p3", "k3", false)).await.unwrap();
    let stored = engine.get_post("p3").await.unwrap();
    assert_eq!(stored.status, STATUS_PUBLISHED);
    assert_eq!(stored.retry_count, 1);
}

#[tokio::test]
async fn unconnected_platform_fails_instead_of_faking_success() {
    let engine = engine_with(None);
    let err = engine.queue_post(post("p4", "k4", false)).await.unwrap_err();
    assert!(err.to_string().contains("instagram is not connected"), "{err}");
    assert_eq!(engine.get_post("p4").await.unwrap().status, STATUS_FAILED);
}

#[tokio::test]
async fn concurrent_queueing_publishes_once() {
    let fake = FakePublisher::new("instagram");
    let engine = Arc::new(engine_with(Some(fake.clone())));
    let mut handles = vec![];
    for i in 0..8 {
        let engine = engine.clone();
        handles.push(tokio::spawn(async move { engine.queue_post(post(&format!("c{i}"), "same-key", false)).await }));
    }
    let mut ok = 0;
    for h in handles {
        if h.await.unwrap().is_ok() {
            ok += 1;
        }
    }
    assert_eq!(ok, 1);
    assert_eq!(fake.published.lock().unwrap().len(), 1);
}
