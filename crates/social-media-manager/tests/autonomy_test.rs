use openaccounts_brand_manager::BrandManager;
use openaccounts_content_strategy::ContentStrategyEngine;
use openaccounts_creative_pipeline::CreativePipeline;
use openaccounts_engagement_agent::EngagementAgent;
use openaccounts_lm_gateway::LMGateway;
use openaccounts_platform_intelligence::PlatformIntelligence;
use openaccounts_policy_engine::PolicyEngine;
use openaccounts_publishing_engine::PublishingEngine;
use openaccounts_social_analytics::SocialAnalyticsEngine;
use openaccounts_social_media_manager::SocialMediaManagerAgent;
use std::sync::Arc;

fn setup_full_stack() -> (Arc<SocialMediaManagerAgent>, Arc<ContentStrategyEngine>) {
    let intel = Arc::new(PlatformIntelligence::new());
    let policy = Arc::new(PolicyEngine::new(intel));
    let lm = Arc::new(LMGateway::new());

    let brand = Arc::new(BrandManager::new());
    let strategy = Arc::new(ContentStrategyEngine::new(brand.clone(), lm.clone()));
    let creative = Arc::new(CreativePipeline::new());
    let publishing = Arc::new(PublishingEngine::new(policy.clone()));
    let engagement = Arc::new(EngagementAgent::new(policy.clone(), lm.clone()));
    let analytics = Arc::new(SocialAnalyticsEngine::new());

    let manager = Arc::new(SocialMediaManagerAgent::new(
        brand,
        strategy.clone(),
        creative,
        publishing,
        engagement,
        analytics,
    ));

    (manager, strategy)
}

#[tokio::test]
async fn test_autonomous_cycle_duplicate_prevention_and_human_checkpoints() {
    let (manager, strategy) = setup_full_stack();

    // Schedule 1 unapproved slot (requires human review) and 1 pre-approved slot
    strategy
        .schedule_post_slot(openaccounts_content_strategy::CalendarPostSlot {
            slot_id: "slot-01".into(),
            campaign_id: Some("camp-01".into()),
            brand_id: "brand-core".into(),
            target_platform: "instagram".into(),
            scheduled_time: chrono::Utc::now(),
            content_type: "Video_Reel".into(),
            caption: "9:16 vertical reel test".into(),
            hashtags: vec!["#reel".into()],
            asset_ids: vec!["asset-reel-1".into()],
            is_approved: false, // requires human checkpoint
            status: "Scheduled".into(),
        })
        .await;

    strategy
        .schedule_post_slot(openaccounts_content_strategy::CalendarPostSlot {
            slot_id: "slot-02".into(),
            campaign_id: Some("camp-01".into()),
            brand_id: "brand-core".into(),
            target_platform: "x".into(),
            scheduled_time: chrono::Utc::now(),
            content_type: "Text_Post".into(),
            caption: "Autonomous publication test".into(),
            hashtags: vec!["#security".into()],
            asset_ids: vec![],
            is_approved: true, // auto-publish eligible
            status: "Scheduled".into(),
        })
        .await;

    // 1. Test loop start & pause
    assert!(!manager.is_loop_running());
    manager.start_autonomous_loop();
    assert!(manager.is_loop_running());
    manager.pause_autonomous_loop();
    assert!(!manager.is_loop_running());

    // 2. Execute autonomous cycle
    let checkpoint = manager
        .execute_autonomous_cycle()
        .await
        .expect("Autonomous cycle failed");

    // Unapproved slot must pause for human review
    assert_eq!(checkpoint.pending_human_approvals_count, 1, "Unapproved slot must pause at human checkpoint");
    assert_eq!(checkpoint.processed_slots_count, 1, "Approved slot must be published");

    // 3. Re-run cycle: Duplicate prevention must prevent re-publishing the same slot
    let second_checkpoint = manager
        .execute_autonomous_cycle()
        .await
        .expect("Second cycle failed");

    assert_eq!(
        second_checkpoint.processed_slots_count, 0,
        "Duplicate posts must be skipped via idempotency keys"
    );
}

#[tokio::test]
async fn test_restart_recovery_from_checkpoint() {
    let (manager, _strategy) = setup_full_stack();

    // Run cycle and get checkpoint
    manager.start_autonomous_loop();
    let original_checkpoint = manager
        .execute_autonomous_cycle()
        .await
        .expect("Cycle failed");

    // Simulate crash / restart with fresh manager
    let (new_manager, _) = setup_full_stack();
    assert!(!new_manager.is_loop_running());

    // Restore from saved checkpoint
    new_manager
        .restore_from_checkpoint(original_checkpoint.clone())
        .await
        .expect("Restore failed");

    assert!(new_manager.is_loop_running(), "Loop status must be restored from checkpoint");
    let restored_cp = new_manager.get_last_checkpoint().await.unwrap();
    assert_eq!(restored_cp.checkpoint_id, original_checkpoint.checkpoint_id);
    assert_eq!(
        restored_cp.published_idempotency_keys.len(),
        original_checkpoint.published_idempotency_keys.len()
    );
}
