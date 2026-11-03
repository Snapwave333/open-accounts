use openaccounts_account_registry::AccountRegistry;
use openaccounts_core::AccountId;
use openaccounts_database::Database;
use openaccounts_engagement_agent::{ClassificationCategory, EngagementAgent};
use openaccounts_lm_gateway::LMGateway;
use openaccounts_platform_intelligence::PlatformIntelligence;
use openaccounts_policy_engine::PolicyEngine;
use std::{path::PathBuf, sync::Arc};

fn temp_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("oa-engage-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

async fn setup_agent(studio_root: PathBuf) -> EngagementAgent {
    let intel = Arc::new(PlatformIntelligence::new());
    let policy = Arc::new(PolicyEngine::new(intel));
    let lm = Arc::new(LMGateway::new());
    let db = Arc::new(
        Database::init(&studio_root.join("test.sqlite"))
            .await
            .expect("test database"),
    );
    let registry = Arc::new(AccountRegistry::new(db));
    EngagementAgent::new(policy, lm, registry).with_studio_root(studio_root)
}

#[tokio::test]
async fn test_single_engagement_actions_refuse_instead_of_faking() {
    let root = temp_dir();
    let agent = setup_agent(root.clone()).await;
    let account_id = AccountId::new();

    let like = agent.like_post(account_id.clone(), "instagram", "post-1").await;
    assert!(like.is_err(), "a like that was never sent must not report success");

    let follow = agent.follow_user(account_id.clone(), "x", "@someone").await;
    assert!(follow.is_err());

    let reply = agent
        .dispatch_reply(account_id, "reddit", "thread-1", "text")
        .await;
    assert!(reply.is_err());

    assert!(agent.get_engagement_history().await.is_empty());
    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
async fn test_inbox_reads_the_studio_comment_queue() {
    let root = temp_dir();
    let agent = setup_agent(root.clone()).await;

    // No queue file yet: genuinely empty.
    assert!(agent.list_inbox().await.expect("inbox").is_empty());

    std::fs::create_dir_all(root.join("insights")).unwrap();
    std::fs::write(
        root.join("insights").join("comments.json"),
        r#"{"pending": {
            "a1": {"platform": "youtube", "author": "viewer", "text": "How?", "category": "question", "draft": "Like this."},
            "b2": {"platform": "tiktok", "author": "troll", "text": "...", "category": "toxic"}
        }}"#,
    )
    .unwrap();

    let inbox = agent.list_inbox().await.expect("inbox");
    assert_eq!(inbox.len(), 2);

    let question = inbox.iter().find(|m| m.message_id == "a1").unwrap();
    assert_eq!(question.category, ClassificationCategory::GeneralInquiry);
    // CDN organic replies run in auto mode (comments.py keeps the safety filter and daily caps).
    assert_eq!(question.status, "AutoApproved");
    assert!(!question.requires_human_approval);
    assert_eq!(question.drafted_reply.as_deref(), Some("Like this."));
    assert!(question.received_at.is_none());
    assert!(question.sentiment_score.is_none());

    let toxic = inbox.iter().find(|m| m.message_id == "b2").unwrap();
    assert_eq!(toxic.category, ClassificationCategory::SecurityOrLegalEscalation);
    assert_eq!(toxic.status, "NeedsHuman");
    assert!(toxic.requires_human_approval);

    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
async fn test_reply_without_studio_is_not_configured() {
    let missing = std::env::temp_dir().join(format!("oa-missing-{}", uuid::Uuid::new_v4()));
    let db_dir = temp_dir();
    let intel = Arc::new(PlatformIntelligence::new());
    let policy = Arc::new(PolicyEngine::new(intel));
    let db = Arc::new(Database::init(&db_dir.join("t.sqlite")).await.unwrap());
    let agent = EngagementAgent::new(policy, Arc::new(LMGateway::new()), Arc::new(AccountRegistry::new(db)))
        .with_studio_root(&missing);

    let err = agent
        .approve_and_send_reply("a1", "hello")
        .await
        .expect_err("no studio, no send");
    assert!(err.to_string().contains("Not configured"));
    let _ = std::fs::remove_dir_all(&db_dir);
}
