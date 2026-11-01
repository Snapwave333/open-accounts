use openaccounts_core::AccountId;
use openaccounts_engagement_agent::{EngagementActionType, EngagementAgent};
use openaccounts_lm_gateway::LMGateway;
use openaccounts_platform_intelligence::PlatformIntelligence;
use openaccounts_policy_engine::PolicyEngine;
use std::sync::Arc;

fn setup_test_agent() -> EngagementAgent {
    let intel = Arc::new(PlatformIntelligence::new());
    let policy = Arc::new(PolicyEngine::new(intel));
    let lm = Arc::new(LMGateway::new());
    EngagementAgent::new(policy, lm)
}

#[tokio::test]
async fn test_explicit_engagement_actions() {
    let agent = setup_test_agent();
    let account_id = AccountId::new();

    // 1. Explicitly test liking a post
    let like_res = agent
        .like_post(account_id.clone(), "instagram", "post-reel-101")
        .await
        .expect("Liking post failed");
    assert!(like_res.success);
    assert_eq!(like_res.action_type, EngagementActionType::Like);
    assert_eq!(like_res.target_id, "post-reel-101");

    // 2. Explicitly test following a user
    let follow_res = agent
        .follow_user(account_id.clone(), "x", "@tech_influencer")
        .await
        .expect("Following user failed");
    assert!(follow_res.success);
    assert_eq!(follow_res.action_type, EngagementActionType::Follow);
    assert_eq!(follow_res.target_id, "@tech_influencer");

    // 3. Explicitly test subscribing to a channel
    let sub_res = agent
        .subscribe_channel(account_id.clone(), "youtube", "UC_openaccounts_channel")
        .await
        .expect("Subscribing to channel failed");
    assert!(sub_res.success);
    assert_eq!(sub_res.action_type, EngagementActionType::Subscribe);

    // 4. Explicitly test resharing content
    let reshare_res = agent
        .reshare_content(account_id.clone(), "x", "tweet-8891", Some("Insightful take on local vaults!"))
        .await
        .expect("Resharing failed");
    assert!(reshare_res.success);
    assert_eq!(reshare_res.action_type, EngagementActionType::Reshare);

    // 5. Explicitly test commenting
    let comment_res = agent
        .post_comment(
            account_id.clone(),
            "instagram",
            "post-reel-101",
            "Great demonstration of zeroized offline encryption!",
        )
        .await
        .expect("Posting comment failed");
    assert!(comment_res.success);
    assert_eq!(comment_res.action_type, EngagementActionType::Comment);

    // 6. Explicitly test responding / reply dispatch
    let reply_res = agent
        .dispatch_reply(
            account_id.clone(),
            "reddit",
            "comment-thread-441",
            "Our Rust engine prevents LLM credential exposure completely.",
        )
        .await
        .expect("Dispatching reply failed");
    assert!(reply_res.success);
    assert_eq!(reply_res.action_type, EngagementActionType::Reply);

    // 7. Verify all actions recorded in audit history
    let history = agent.get_engagement_history().await;
    assert_eq!(history.len(), 6, "Expected 6 engagement action records");
}

#[tokio::test]
async fn test_inbox_classification_and_reply_approval() {
    let agent = setup_test_agent();
    let inbox = agent.list_inbox().await;
    assert_eq!(inbox.len(), 2);

    let sales_lead = inbox.iter().find(|m| m.message_id == "inb-101").unwrap();
    assert!(!sales_lead.requires_human_approval);

    let legal_escalation = inbox.iter().find(|m| m.message_id == "inb-102").unwrap();
    assert!(legal_escalation.requires_human_approval);

    // Approve response
    agent
        .approve_and_send_reply(
            "inb-102",
            "We have forwarded your enterprise request to our legal review team.",
        )
        .await
        .expect("Reply approval failed");

    let updated_inbox = agent.list_inbox().await;
    let updated_msg = updated_inbox.iter().find(|m| m.message_id == "inb-102").unwrap();
    assert_eq!(updated_msg.status, "Replied");
    assert!(updated_msg.drafted_reply.is_some());
}
