use chrono::Utc;
use openaccounts_core::AccountId;
use openaccounts_platform_intelligence::PlatformIntelligence;
use openaccounts_policy_engine::PolicyEngine;
use openaccounts_publishing_engine::{PublishingEngine, SocialPostPayload};
use std::sync::Arc;

fn setup_engine() -> PublishingEngine {
    let intel = Arc::new(PlatformIntelligence::new());
    let policy = Arc::new(PolicyEngine::new(intel));
    PublishingEngine::new(policy)
}

#[tokio::test]
async fn test_platform_auth_verification() {
    let engine = setup_engine();
    let account_id = AccountId::new();

    // Verify Instagram Auth Scopes
    let ig_auth = engine
        .verify_platform_auth("instagram", &account_id)
        .await
        .expect("Auth check failed");
    assert!(ig_auth.is_authenticated);
    assert!(ig_auth.scopes.contains(&"instagram_content_publish".into()));

    // Verify TikTok Auth Scopes
    let tt_auth = engine
        .verify_platform_auth("tiktok", &account_id)
        .await
        .expect("Auth check failed");
    assert!(tt_auth.is_authenticated);
    assert!(tt_auth.scopes.contains(&"video.publish".into()));

    // Verify YouTube Auth Scopes
    let yt_auth = engine
        .verify_platform_auth("youtube", &account_id)
        .await
        .expect("Auth check failed");
    assert!(yt_auth.is_authenticated);
    assert!(yt_auth.scopes.iter().any(|s| s.contains("youtube.upload")));

    // Verify X Auth Scopes
    let x_auth = engine
        .verify_platform_auth("x", &account_id)
        .await
        .expect("Auth check failed");
    assert!(x_auth.is_authenticated);
    assert!(x_auth.scopes.contains(&"tweet.write".into()));
}

#[tokio::test]
async fn test_publishing_with_idempotency_and_approval_gate() {
    let engine = setup_engine();
    let account_id = AccountId::new();

    let post = SocialPostPayload {
        post_id: "post-demo-01".into(),
        account_id: account_id.clone(),
        platform: "instagram".into(),
        caption: "Protecting digital autonomy with local credentials.".into(),
        hashtags: vec!["#security".into(), "#privacy".into()],
        media_paths: vec!["/renders/reel_1080x1920.mp4".into()],
        idempotency_key: "idem-key-991823".into(),
        scheduled_at: Some(Utc::now()),
        requires_human_approval: true,
        is_approved: false,
        retry_count: 0,
        status: "Draft".into(),
    };

    // 1. Queue post requiring approval
    let post_id = engine.queue_post(post.clone()).await.expect("Queue failed");
    let posts = engine.list_posts().await;
    let queued = posts.iter().find(|p| p.post_id == post_id).unwrap();
    assert_eq!(queued.status, "AwaitingApproval");

    // 2. Approve and publish
    let pub_res = engine
        .approve_and_publish(&post_id)
        .await
        .expect("Publish failed");
    assert_eq!(pub_res.post_id, post_id);
    assert!(pub_res.remote_permalink.contains("instagram.com"));

    // 3. Attempt duplicate publish with same idempotency key -> Must fail!
    let duplicate_post = SocialPostPayload {
        post_id: "post-demo-02".into(),
        account_id,
        platform: "instagram".into(),
        caption: "Duplicate attempt".into(),
        hashtags: vec![],
        media_paths: vec![],
        idempotency_key: "idem-key-991823".into(),
        scheduled_at: None,
        requires_human_approval: false,
        is_approved: true,
        retry_count: 0,
        status: "Draft".into(),
    };

    let dup_res = engine.queue_post(duplicate_post).await;
    assert!(dup_res.is_err(), "Duplicate post must be blocked by idempotency key");
}
