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

fn post(post_id: &str, key: &str, requires_approval: bool) -> SocialPostPayload {
    SocialPostPayload {
        post_id: post_id.into(),
        account_id: AccountId::new(),
        platform: "instagram".into(),
        caption: "caption".into(),
        hashtags: vec![],
        media_paths: vec![],
        idempotency_key: key.into(),
        scheduled_at: None,
        requires_human_approval: requires_approval,
        is_approved: false,
        retry_count: 0,
        status: "Draft".into(),
    }
}

#[tokio::test]
async fn test_platform_auth_is_not_claimed_without_credentials() {
    let engine = setup_engine();
    let res = engine.verify_platform_auth("instagram", &AccountId::new()).await;
    assert!(res.is_err(), "auth must not be reported without real tokens");
}

#[tokio::test]
async fn test_queue_approval_gate_idempotency_and_no_fake_publish() {
    let engine = setup_engine();

    let post_id = engine
        .queue_post(post("post-01", "key-1", true))
        .await
        .expect("queue");
    let queued = engine.list_posts().await;
    assert_eq!(queued[0].status, "AwaitingApproval");

    // Publishing is refused, and the post is not marked published.
    let res = engine.approve_and_publish(&post_id).await;
    assert!(res.is_err());
    let after = engine.list_posts().await;
    assert_eq!(after[0].status, "AwaitingApproval");

    // An approved post is queued as Approved, never Published.
    let mut approved = post("post-02", "key-2", false);
    approved.is_approved = true;
    engine.queue_post(approved).await.expect("queue");
    let posts = engine.list_posts().await;
    let p2 = posts.iter().find(|p| p.post_id == "post-02").unwrap();
    assert_eq!(p2.status, "Approved");

    // Same idempotency key is rejected.
    let dup = engine.queue_post(post("post-03", "key-1", false)).await;
    assert!(dup.is_err(), "duplicate idempotency key must be blocked");

    // Unknown post id is an error.
    assert!(engine.approve_and_publish("missing").await.is_err());
}
