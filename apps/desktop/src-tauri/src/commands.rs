use crate::state::AppState;
use openaccounts_account_creator::{AccountCreationPreflightReport, AccountCreationPreflightRequest};
use openaccounts_account_identity_manager::AccountCategory;
use openaccounts_account_registry::AccountRecord;
use openaccounts_brand_manager::BrandProfile;
use openaccounts_chrome_relay_adapter::{ChromeRelayAdapter, ChromeRelayConfig};
use openaccounts_content_strategy::{CalendarPostSlot, CampaignBrief, TrendTopic};
use openaccounts_core::{AccountId, Platform, SystemStats, TaskId};
use openaccounts_creative_pipeline::{CreativeAssetSpec, CreativeJobStatus, ToolCapability};
use openaccounts_credential_vault::SecretMetadataView;
use openaccounts_email_identity::EmailIdentity;
use openaccounts_engagement_agent::UnifiedInboxMessage;
use openaccounts_human_intervention::HumanTask;
use openaccounts_lm_gateway::ModelProfile;
use openaccounts_multi_account_registry::ManagedAccountView;
use openaccounts_platform_intelligence::PlatformKnowledgeProfile;
use openaccounts_policy_engine::PolicyCheckResult;
use openaccounts_policy_monitor::PolicyMonitorAlert;
use openaccounts_policy_versioning::PolicyVersionRecord;
use openaccounts_publishing_engine::SocialPostPayload;
use openaccounts_social_analytics::VerifiedPlatformMetric;
use openaccounts_social_media_manager::{
    bridge::{GarbageCollectionReport, StickBridgeMessage, StickBridgeState},
    SubAgentAssignment,
};
use std::sync::Arc;
use tauri::State;

#[tauri::command(rename_all = "snake_case")]
pub async fn get_system_stats(state: State<'_, AppState>) -> Result<SystemStats, String> {
    let accounts = state.account_registry.list_accounts().await.unwrap_or_default();
    let tasks = state.human_controller.list_pending_tasks().await;
    let is_kill_active = state.automation_engine.is_kill_switch_active();

    Ok(SystemStats {
        total_accounts: accounts.len() as u64,
        active_automations: 6,
        pending_human_tasks: tasks.len() as u64,
        system_status: if is_kill_active {
            "EMERGENCY_HALTED".into()
        } else {
            "ONLINE".into()
        },
        updated_at: chrono::Utc::now(),
    })
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_accounts(state: State<'_, AppState>) -> Result<Vec<AccountRecord>, String> {
    state
        .account_registry
        .list_accounts()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn create_account(
    state: State<'_, AppState>,
    platform: String,
    username: String,
    email: Option<String>,
) -> Result<String, String> {
    let plat = match platform.to_lowercase().as_str() {
        "instagram" => Platform::Instagram,
        "reddit" => Platform::Reddit,
        "youtube" => Platform::YouTube,
        "discord" => Platform::Discord,
        "x" => Platform::X,
        _ => Platform::Custom(platform),
    };

    let record = AccountRecord {
        id: AccountId::new(),
        platform: plat,
        username,
        email,
        phone: None,
        status: "active".into(),
        profile_metadata: serde_json::json!({}),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        last_active: Some(chrono::Utc::now()),
    };

    let id = state
        .account_registry
        .create_account(record)
        .await
        .map_err(|e| e.to_string())?;

    Ok(id.0.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_human_tasks(state: State<'_, AppState>) -> Result<Vec<HumanTask>, String> {
    Ok(state.human_controller.list_pending_tasks().await)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn resolve_human_task(
    state: State<'_, AppState>,
    task_id: String,
) -> Result<(), String> {
    let id = TaskId(uuid::Uuid::parse_str(&task_id).map_err(|e| e.to_string())?);
    state
        .human_controller
        .resolve_task(&id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn is_vault_unlocked(state: State<'_, AppState>) -> Result<bool, String> {
    Ok(state.vault.is_unlocked().await)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn unlock_vault(
    state: State<'_, AppState>,
    password: String,
) -> Result<(), String> {
    state.vault.unlock(&password).await.map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn lock_vault(state: State<'_, AppState>) -> Result<(), String> {
    state.vault.lock().await;
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_vault_secrets(
    state: State<'_, AppState>,
) -> Result<Vec<SecretMetadataView>, String> {
    Ok(state.vault.list_metadata().await)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_lm_profiles(state: State<'_, AppState>) -> Result<Vec<ModelProfile>, String> {
    Ok(state.lm_gateway.list_profiles().await)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn register_lm_profile(
    state: State<'_, AppState>,
    profile: ModelProfile,
) -> Result<(), String> {
    state.lm_gateway.register_profile(profile).await;
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn trigger_kill_switch(state: State<'_, AppState>) -> Result<(), String> {
    state.automation_engine.trigger_kill_switch();
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn reset_kill_switch(state: State<'_, AppState>) -> Result<(), String> {
    state.automation_engine.reset_kill_switch();
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn is_kill_switch_active(state: State<'_, AppState>) -> Result<bool, String> {
    Ok(state.automation_engine.is_kill_switch_active())
}

// --- PLATFORM INTELLIGENCE & COMPLIANCE ---

#[tauri::command(rename_all = "snake_case")]
pub async fn list_platform_profiles(
    state: State<'_, AppState>,
) -> Result<Vec<PlatformKnowledgeProfile>, String> {
    Ok(state.platform_intelligence.list_profiles().await)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_platform_profile(
    state: State<'_, AppState>,
    platform_id: String,
) -> Result<Option<PlatformKnowledgeProfile>, String> {
    Ok(state.platform_intelligence.get_profile(&platform_id).await)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn check_multi_account_policy(
    state: State<'_, AppState>,
    platform_id: String,
    current_count: u32,
    purpose: String,
) -> Result<PolicyCheckResult, String> {
    state
        .policy_engine
        .check_multi_account_eligibility(&platform_id, current_count, &purpose, false)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn run_policy_monitor_cycle(
    state: State<'_, AppState>,
) -> Result<Vec<PolicyMonitorAlert>, String> {
    Ok(state.policy_monitor.run_policy_check_cycle().await)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn simulate_policy_change(
    state: State<'_, AppState>,
    platform_id: String,
    section: String,
    new_summary: String,
) -> Result<(), String> {
    state
        .policy_monitor
        .simulate_policy_update(&platform_id, &section, &new_summary, true)
        .await;
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_policy_version_history(
    state: State<'_, AppState>,
    platform_id: String,
) -> Result<Vec<PolicyVersionRecord>, String> {
    Ok(state.policy_versioning.get_history(&platform_id).await)
}

// --- EMAIL IDENTITY ---

#[tauri::command(rename_all = "snake_case")]
pub async fn list_email_identities(
    state: State<'_, AppState>,
) -> Result<Vec<EmailIdentity>, String> {
    Ok(state.email_manager.list_identities().await)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn allocate_email_identity(
    state: State<'_, AppState>,
    prefix: String,
    platform: Option<String>,
) -> Result<EmailIdentity, String> {
    state
        .email_manager
        .allocate_identity("primary_domain", &prefix, platform)
        .await
        .map_err(|e| e.to_string())
}

// --- MULTI-ACCOUNT REGISTRY ---

#[tauri::command(rename_all = "snake_case")]
pub async fn list_managed_multi_accounts(
    state: State<'_, AppState>,
) -> Result<Vec<ManagedAccountView>, String> {
    state
        .multi_account_registry
        .list_managed_accounts()
        .await
        .map_err(|e| e.to_string())
}

// --- 10-STEP MANDATORY ACCOUNT CREATION PREFLIGHT ---

#[tauri::command(rename_all = "snake_case")]
pub async fn execute_10_step_account_preflight(
    state: State<'_, AppState>,
    platform: String,
    category_str: String,
    desired_username: Option<String>,
    preferred_email: Option<String>,
) -> Result<AccountCreationPreflightReport, String> {
    let category = match category_str.to_lowercase().as_str() {
        "business" => AccountCategory::Business,
        "brand" => AccountCategory::Brand,
        "creative_project" | "creativeproject" => AccountCategory::CreativeProject,
        "community" => AccountCategory::Community,
        _ => AccountCategory::Personal,
    };

    let browser = Arc::new(ChromeRelayAdapter::new(ChromeRelayConfig {
        endpoint: "ws://localhost:9222".into(),
        tab_lease_id: Some("tab_lease_preflight".into()),
        origin_fence: vec![format!("{}.com", platform)],
    }));

    let req = AccountCreationPreflightRequest {
        platform,
        category,
        desired_username,
        preferred_email,
        email_provider_id: Some("primary_domain".into()),
    };

    state
        .account_creator
        .execute_10_step_preflight(browser, req)
        .await
        .map_err(|e| e.to_string())
}

// --- SOCIAL MEDIA MARKETING & CREATIVE ENGINE COMMANDS ---

#[tauri::command(rename_all = "snake_case")]
pub async fn list_brands(state: State<'_, AppState>) -> Result<Vec<BrandProfile>, String> {
    Ok(state.brand_manager.list_brands().await)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_campaigns(state: State<'_, AppState>) -> Result<Vec<CampaignBrief>, String> {
    Ok(state.content_strategy.list_campaigns().await)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_calendar_posts(
    state: State<'_, AppState>,
) -> Result<Vec<CalendarPostSlot>, String> {
    Ok(state.content_strategy.list_calendar().await)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn research_trends(
    state: State<'_, AppState>,
    platform: String,
) -> Result<Vec<TrendTopic>, String> {
    Ok(state.content_strategy.research_trends(&platform).await)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_creative_tools(
    state: State<'_, AppState>,
) -> Result<Vec<ToolCapability>, String> {
    Ok(state.creative_pipeline.list_tools().await)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn create_creative_job(
    state: State<'_, AppState>,
    spec: CreativeAssetSpec,
) -> Result<String, String> {
    state
        .creative_pipeline
        .create_production_job(spec)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_creative_job_status(
    state: State<'_, AppState>,
    job_id: String,
) -> Result<Option<CreativeJobStatus>, String> {
    Ok(state.creative_pipeline.get_job_status(&job_id).await)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_publishing_posts(
    state: State<'_, AppState>,
) -> Result<Vec<SocialPostPayload>, String> {
    Ok(state.publishing_engine.list_posts().await)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn approve_and_publish_post(
    state: State<'_, AppState>,
    post_id: String,
) -> Result<(), String> {
    state
        .publishing_engine
        .approve_and_publish(&post_id)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_unified_inbox(
    state: State<'_, AppState>,
) -> Result<Vec<UnifiedInboxMessage>, String> {
    Ok(state.engagement_agent.list_inbox().await)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn approve_and_send_inbox_reply(
    state: State<'_, AppState>,
    message_id: String,
    final_reply: String,
) -> Result<(), String> {
    state
        .engagement_agent
        .approve_and_send_reply(&message_id, &final_reply)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_social_analytics(
    state: State<'_, AppState>,
) -> Result<Vec<VerifiedPlatformMetric>, String> {
    Ok(state.social_analytics.list_metrics().await)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn start_autonomous_marketing_loop(
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.social_media_manager.start_autonomous_loop();
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn pause_autonomous_marketing_loop(
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.social_media_manager.pause_autonomous_loop();
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn is_autonomous_marketing_loop_running(
    state: State<'_, AppState>,
) -> Result<bool, String> {
    Ok(state.social_media_manager.is_loop_running())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_collaborating_agents(
    state: State<'_, AppState>,
) -> Result<Vec<SubAgentAssignment>, String> {
    Ok(state.social_media_manager.list_collaborating_agents())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_stick_bridge_state(
    state: State<'_, AppState>,
) -> Result<StickBridgeState, String> {
    Ok(state.social_media_manager.get_stick_bridge_state())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn run_garbage_collection(
    state: State<'_, AppState>,
    apply: bool,
) -> Result<GarbageCollectionReport, String> {
    state
        .social_media_manager
        .run_garbage_collection(apply)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn send_stick_bridge_message(
    state: State<'_, AppState>,
    from_agent: String,
    to_agent: String,
    channel: String,
    subject: String,
    body: String,
    create_ceo_directive: bool,
) -> Result<StickBridgeMessage, String> {
    state
        .social_media_manager
        .send_stick_bridge_message(
            &from_agent,
            &to_agent,
            &channel,
            &subject,
            &body,
            create_ceo_directive,
        )
        .map_err(|e| e.to_string())
}
