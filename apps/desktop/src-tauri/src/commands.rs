use crate::autonomy::{AutonomyCycleReport, AutonomyStatus};
use crate::state::AppState;
use openaccounts_account_creator::{AccountCreationPreflightReport, AccountCreationPreflightRequest};
use openaccounts_account_identity_manager::AccountCategory;
use openaccounts_account_registry::AccountRecord;
use openaccounts_brand_manager::BrandProfile;
use openaccounts_chrome_relay_adapter::{ChromeRelayAdapter, ChromeRelayConfig, ChromeRelayStatus};
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
    SubAgentAssignment, SubAgentRole,
};
use tauri::State;

#[tauri::command]
pub async fn get_system_stats(state: State<'_, AppState>) -> Result<SystemStats, String> {
    let accounts = state.account_registry.list_accounts().await.unwrap_or_default();
    let tasks = state.human_controller.list_pending_tasks().await;
    let is_kill_active = state.automation_engine.is_kill_switch_active();
    let autonomy = state.autonomy.status().await;

    // Only count a genuinely running supervised loop as an active automation.
    let active_automations = if autonomy.loop_running && !autonomy.paused && !is_kill_active {
        1
    } else {
        0
    };

    Ok(SystemStats {
        total_accounts: accounts.len() as u64,
        active_automations,
        pending_human_tasks: tasks.len() as u64,
        system_status: if is_kill_active {
            "EMERGENCY_HALTED".into()
        } else if autonomy.paused {
            "PAUSED".into()
        } else if autonomy.loop_running {
            "ONLINE".into()
        } else {
            "STOPPED".into()
        },
        updated_at: chrono::Utc::now(),
    })
}

#[tauri::command]
pub async fn list_accounts(state: State<'_, AppState>) -> Result<Vec<AccountRecord>, String> {
    state
        .account_registry
        .list_accounts()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
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
        last_active: None,
    };

    let id = state
        .account_registry
        .create_account(record)
        .await
        .map_err(|e| e.to_string())?;

    Ok(id.0.to_string())
}

#[tauri::command]
pub async fn list_human_tasks(state: State<'_, AppState>) -> Result<Vec<HumanTask>, String> {
    Ok(state.human_controller.list_pending_tasks().await)
}

#[tauri::command]
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

#[tauri::command]
pub async fn is_vault_unlocked(state: State<'_, AppState>) -> Result<bool, String> {
    Ok(state.vault.is_unlocked().await)
}

#[tauri::command]
pub async fn unlock_vault(
    state: State<'_, AppState>,
    password: String,
) -> Result<(), String> {
    state.vault.unlock(&password).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn lock_vault(state: State<'_, AppState>) -> Result<(), String> {
    state.vault.lock().await;
    Ok(())
}

#[tauri::command]
pub async fn list_vault_secrets(
    state: State<'_, AppState>,
) -> Result<Vec<SecretMetadataView>, String> {
    Ok(state.vault.list_metadata().await)
}

#[tauri::command]
pub async fn list_lm_profiles(state: State<'_, AppState>) -> Result<Vec<ModelProfile>, String> {
    Ok(state.lm_gateway.list_profiles().await)
}

#[tauri::command]
pub async fn register_lm_profile(
    state: State<'_, AppState>,
    profile: ModelProfile,
) -> Result<(), String> {
    state
        .lm_gateway
        .register_profile(profile)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn trigger_kill_switch(state: State<'_, AppState>) -> Result<(), String> {
    state.automation_engine.trigger_kill_switch();
    Ok(())
}

#[tauri::command]
pub async fn reset_kill_switch(state: State<'_, AppState>) -> Result<(), String> {
    state.automation_engine.reset_kill_switch();
    Ok(())
}

#[tauri::command]
pub async fn is_kill_switch_active(state: State<'_, AppState>) -> Result<bool, String> {
    Ok(state.automation_engine.is_kill_switch_active())
}

// --- PLATFORM INTELLIGENCE & COMPLIANCE ---

#[tauri::command]
pub async fn list_platform_profiles(
    state: State<'_, AppState>,
) -> Result<Vec<PlatformKnowledgeProfile>, String> {
    Ok(state.platform_intelligence.list_profiles().await)
}

#[tauri::command]
pub async fn get_platform_profile(
    state: State<'_, AppState>,
    platform_id: String,
) -> Result<Option<PlatformKnowledgeProfile>, String> {
    Ok(state.platform_intelligence.get_profile(&platform_id).await)
}

#[tauri::command]
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

#[tauri::command]
pub async fn run_policy_monitor_cycle(
    state: State<'_, AppState>,
) -> Result<Vec<PolicyMonitorAlert>, String> {
    Ok(state.policy_monitor.run_policy_check_cycle().await)
}

#[tauri::command]
pub async fn get_policy_version_history(
    state: State<'_, AppState>,
    platform_id: String,
) -> Result<Vec<PolicyVersionRecord>, String> {
    Ok(state.policy_versioning.get_history(&platform_id).await)
}

// --- EMAIL IDENTITY ---

#[tauri::command]
pub async fn list_email_identities(
    state: State<'_, AppState>,
) -> Result<Vec<EmailIdentity>, String> {
    Ok(state.email_manager.list_identities().await)
}

#[tauri::command]
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

#[tauri::command]
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

#[tauri::command]
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

    let req = AccountCreationPreflightRequest {
        platform,
        category,
        desired_username,
        preferred_email,
        email_provider_id: None,
    };

    state
        .account_creator
        .execute_10_step_preflight(req)
        .await
        .map_err(|e| e.to_string())
}

// --- SOCIAL MEDIA MARKETING & CREATIVE ENGINE COMMANDS ---

#[tauri::command]
pub async fn list_brands(state: State<'_, AppState>) -> Result<Vec<BrandProfile>, String> {
    Ok(state.brand_manager.list_brands().await)
}

#[tauri::command]
pub async fn list_campaigns(state: State<'_, AppState>) -> Result<Vec<CampaignBrief>, String> {
    Ok(state.content_strategy.list_campaigns().await)
}

#[tauri::command]
pub async fn list_calendar_posts(
    state: State<'_, AppState>,
) -> Result<Vec<CalendarPostSlot>, String> {
    Ok(state.content_strategy.list_calendar().await)
}

#[tauri::command]
pub async fn research_trends(
    state: State<'_, AppState>,
    platform: String,
) -> Result<Vec<TrendTopic>, String> {
    state
        .content_strategy
        .research_trends(&platform)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_creative_tools(
    state: State<'_, AppState>,
) -> Result<Vec<ToolCapability>, String> {
    Ok(state.creative_pipeline.list_tools().await)
}

#[tauri::command]
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

#[tauri::command]
pub async fn get_creative_job_status(
    state: State<'_, AppState>,
    job_id: String,
) -> Result<Option<CreativeJobStatus>, String> {
    Ok(state.creative_pipeline.get_job_status(&job_id).await)
}

#[tauri::command]
pub async fn list_publishing_posts(
    state: State<'_, AppState>,
) -> Result<Vec<SocialPostPayload>, String> {
    Ok(state.publishing_engine.list_posts().await)
}

#[tauri::command]
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

#[tauri::command]
pub async fn list_unified_inbox(
    state: State<'_, AppState>,
) -> Result<Vec<UnifiedInboxMessage>, String> {
    state
        .engagement_agent
        .list_inbox()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
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

#[tauri::command]
pub async fn list_social_analytics(
    state: State<'_, AppState>,
) -> Result<Vec<VerifiedPlatformMetric>, String> {
    Ok(state.social_analytics.list_metrics().await)
}

#[tauri::command]
pub async fn start_autonomous_marketing_loop(
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.autonomy.resume();
    state.autonomy.start(None);
    Ok(())
}

#[tauri::command]
pub async fn pause_autonomous_marketing_loop(
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.autonomy.pause();
    Ok(())
}

#[tauri::command]
pub async fn is_autonomous_marketing_loop_running(
    state: State<'_, AppState>,
) -> Result<bool, String> {
    Ok(state.autonomy.is_running() && !state.autonomy.is_paused())
}

#[tauri::command]
pub async fn list_collaborating_agents(
    state: State<'_, AppState>,
) -> Result<Vec<SubAgentAssignment>, String> {
    let status = state.autonomy.status().await;
    let last = status.last_cycle.clone();
    let loop_active = status.loop_running && !status.paused;

    let job_summary = |job_name: &str| -> Option<String> {
        last.as_ref().and_then(|cycle| {
            cycle
                .jobs
                .iter()
                .find(|j| j.name == job_name)
                .map(|j| {
                    format!(
                        "{} ({})",
                        j.summary,
                        if j.success { "ok" } else { "failed" }
                    )
                })
        })
    };

    let assignment = |role: SubAgentRole, title: &str, task: String, active: bool| SubAgentAssignment {
        role,
        title: title.into(),
        current_task: task,
        assigned_model_profile: status.python.clone(),
        is_active: active,
    };

    let mut agents = Vec::new();

    agents.push(assignment(
        SubAgentRole::CommunityManager,
        "Comment & Reply Agent",
        job_summary("Comment sweep").unwrap_or_else(|| {
            if loop_active {
                "Queued — no completed cycle yet".into()
            } else {
                "Idle — autonomy loop is not running".into()
            }
        }),
        loop_active,
    ));

    agents.push(assignment(
        SubAgentRole::Publisher,
        "Social Publishing Agent",
        job_summary("Social publish").or_else(|| job_summary("Social plan")).unwrap_or_else(|| {
            if loop_active {
                "Queued — no completed cycle yet".into()
            } else {
                "Idle — autonomy loop is not running".into()
            }
        }),
        loop_active,
    ));

    agents.push(assignment(
        SubAgentRole::EngagementAgent,
        "Engagement Agent",
        job_summary("Engagement run").or_else(|| job_summary("Engagement stats")).unwrap_or_else(|| {
            if loop_active {
                "Queued — no completed cycle yet".into()
            } else {
                "Idle — autonomy loop is not running".into()
            }
        }),
        loop_active,
    ));

    agents.push(assignment(
        SubAgentRole::ComplianceAgent,
        "Retention / GC Agent",
        job_summary("Retention GC").unwrap_or_else(|| {
            "Idle — no garbage-collection cycle yet".into()
        }),
        loop_active,
    ));

    Ok(agents)
}

#[tauri::command]
pub async fn get_stick_bridge_state(
    state: State<'_, AppState>,
) -> Result<StickBridgeState, String> {
    Ok(state.social_media_manager.get_stick_bridge_state())
}

#[tauri::command]
pub async fn run_garbage_collection(
    state: State<'_, AppState>,
    apply: bool,
) -> Result<GarbageCollectionReport, String> {
    // retention.py can take minutes; keep it off the async runtime threads.
    let manager = state.social_media_manager.clone();
    tauri::async_runtime::spawn_blocking(move || manager.run_garbage_collection(apply))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_gc_report(
    state: State<'_, AppState>,
) -> Result<Option<GarbageCollectionReport>, String> {
    Ok(state.social_media_manager.read_gc_report())
}

#[tauri::command]
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

// --- AUTONOMY RUNTIME (real 24/7 job supervision) ---

#[tauri::command]
pub async fn get_autonomy_status(
    state: State<'_, AppState>,
) -> Result<AutonomyStatus, String> {
    Ok(state.autonomy.status().await)
}

#[tauri::command]
pub async fn run_autonomy_cycle_now(
    state: State<'_, AppState>,
) -> Result<AutonomyCycleReport, String> {
    Ok(state.autonomy.run_cycle().await)
}

#[tauri::command]
pub async fn list_autonomy_cycles(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> Result<Vec<AutonomyCycleReport>, String> {
    Ok(state.autonomy.recent_cycles(limit.unwrap_or(25)).await)
}

#[tauri::command]
pub async fn get_chrome_relay_status(
    state: State<'_, AppState>,
) -> Result<ChromeRelayStatus, String> {
    let _ = &state;
    let adapter = ChromeRelayAdapter::new(ChromeRelayConfig::default());
    Ok(adapter.probe().await)
}
