use crate::{
    agent::{self, AgentChatReply, ChatAttachmentInput, ChatTurn},
    flow::{self, ActivityFlow},
    state::{AppState, AUTOPILOT_SETTING},
};
use openaccounts_account_creator::{AccountCreationPreflightReport, AccountCreationPreflightRequest};
use openaccounts_account_identity_manager::AccountCategory;
use openaccounts_account_registry::AccountRecord;
use openaccounts_audit_engine::AuditRecord;
use openaccounts_brand_manager::BrandProfile;
use openaccounts_content_strategy::{CalendarPostSlot, CampaignBrief, TrendTopic};
use openaccounts_core::{AccountId, Platform, SecretId, SystemStats, TaskId};
use openaccounts_creative_pipeline::{CreativeAssetSpec, CreativeJobStatus, ToolCapability};
use openaccounts_credential_vault::{SecretMetadataView, SecretPayload, SecretType, VaultStatus};
use openaccounts_database::ChainVerification;
use openaccounts_email_identity::EmailIdentity;
use openaccounts_embedded_browser_adapter::EmbeddedBrowserAdapter;
use openaccounts_engagement_agent::{EngagementActionType, UnifiedInboxMessage};
use openaccounts_human_intervention::HumanTask;
use openaccounts_lm_gateway::{ModelProfile, ProfileUsage};
use openaccounts_multi_account_registry::ManagedAccountView;
use openaccounts_platform_intelligence::PlatformKnowledgeProfile;
use openaccounts_policy_engine::PolicyCheckResult;
use openaccounts_policy_monitor::{PolicyCycleReport, PolicyMonitorAlert};
use openaccounts_policy_versioning::PolicyVersionRecord;
use openaccounts_publishing_engine::SocialPostPayload;
use openaccounts_scheduler::ScheduleJob;
use openaccounts_social_analytics::VerifiedPlatformMetric;
use openaccounts_social_media_manager::{
    bridge::{GarbageCollectionReport, StickBridgeMessage, StickBridgeState},
    SubAgentAssignment,
};
use serde::Serialize;
use std::{collections::HashMap, sync::Arc, time::Duration};
use tauri::State;
use tracing::{info, warn};

type CmdResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn parse_uuid(id: &str, what: &str) -> CmdResult<uuid::Uuid> {
    uuid::Uuid::parse_str(id.trim()).map_err(|_| format!("Invalid {what} id: {id}"))
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_system_stats(state: State<'_, AppState>) -> CmdResult<SystemStats> {
    let accounts = state.account_registry.list_accounts().await.map_err(err)?;
    let tasks = state.human_controller.list_pending_tasks().await;
    let halted = state.automation_engine.is_kill_switch_active();
    let active_jobs = state.scheduler.active_job_count().await as u64;
    let loop_running = state.social_media_manager.is_loop_running();

    Ok(SystemStats {
        total_accounts: accounts.len() as u64,
        active_automations: if halted { 0 } else { active_jobs + u64::from(loop_running) },
        pending_human_tasks: tasks.len() as u64,
        system_status: if halted {
            "EMERGENCY_HALTED".into()
        } else if loop_running {
            "AUTOPILOT".into()
        } else {
            "ONLINE".into()
        },
        updated_at: chrono::Utc::now(),
    })
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_activity_flow(state: State<'_, AppState>) -> CmdResult<ActivityFlow> {
    Ok(flow::build(&state).await)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_connection_status(
    state: State<'_, AppState>,
    force: Option<bool>,
) -> CmdResult<Vec<crate::connections::ConnectionStatus>> {
    Ok(crate::connections::statuses(&state, force.unwrap_or(false)).await)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn run_flow_item(state: State<'_, AppState>, kind: String, ref_id: Option<String>) -> CmdResult<String> {
    if state.automation_engine.is_kill_switch_active() {
        return Err("Kill switch is active; reset it before running automations".into());
    }
    match kind.as_str() {
        "job" => {
            let id = TaskId(parse_uuid(ref_id.as_deref().unwrap_or_default(), "job")?);
            state.scheduler.run_now(&id).await.map_err(err)
        }
        "slot" => {
            let cp = state.social_media_manager.execute_autonomous_cycle().await.map_err(err)?;
            Ok(format!("Published {} slot(s), {} failed", cp.processed_slots_count, cp.failed_slots_count))
        }
        "render" => {
            let job_id = ref_id.ok_or("Missing render job id")?;
            spawn_render(&state, job_id.clone());
            Ok(format!("Rendering {job_id}"))
        }
        other => Err(format!("'{other}' items cannot be run directly")),
    }
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_scheduled_jobs(state: State<'_, AppState>) -> CmdResult<Vec<ScheduleJob>> {
    Ok(state.scheduler.list_jobs().await)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_accounts(state: State<'_, AppState>) -> CmdResult<Vec<AccountRecord>> {
    state.account_registry.list_accounts().await.map_err(err)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn create_account(
    state: State<'_, AppState>,
    platform: String,
    username: String,
    email: Option<String>,
) -> CmdResult<String> {
    let username = username.trim().to_string();
    if username.is_empty() {
        return Err("Username is required".into());
    }
    let email = email.map(|e| e.trim().to_string()).filter(|e| !e.is_empty());
    if let Some(e) = &email {
        if !e.contains('@') || e.starts_with('@') || e.ends_with('@') {
            return Err(format!("'{e}' is not a valid email address"));
        }
    }
    let platform = Platform::from(platform.as_str());
    let duplicate = state
        .account_registry
        .list_accounts()
        .await
        .map_err(err)?
        .into_iter()
        .any(|a| a.platform == platform && a.username.eq_ignore_ascii_case(&username));
    if duplicate {
        return Err(format!("{username} is already registered on {platform}"));
    }

    let now = chrono::Utc::now();
    let record = AccountRecord {
        id: AccountId::new(),
        platform,
        username,
        email,
        phone: None,
        status: "active".into(),
        profile_metadata: serde_json::json!({ "created_via": "manual" }),
        created_at: now,
        updated_at: now,
        last_active: None,
    };
    let id = state.account_registry.create_account(record).await.map_err(err)?;
    Ok(id.0.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn delete_account(state: State<'_, AppState>, account_id: String) -> CmdResult<bool> {
    let id = AccountId(parse_uuid(&account_id, "account")?);
    state.account_registry.delete_account(&id).await.map_err(err)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_human_tasks(state: State<'_, AppState>) -> CmdResult<Vec<HumanTask>> {
    Ok(state.human_controller.list_pending_tasks().await)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn resolve_human_task(state: State<'_, AppState>, task_id: String) -> CmdResult<()> {
    let id = TaskId(parse_uuid(&task_id, "task")?);
    let task = state.human_controller.get_task(&id).await;
    state.human_controller.resolve_task(&id).await.map_err(err)?;

    let pending_account = match task.and_then(|t| t.account_id) {
        Some(acc) => state
            .account_creator
            .pending_registrations()
            .await
            .into_iter()
            .find(|r| r.account_id.0.to_string() == acc && r.human_task_id.as_deref() == Some(task_id.as_str()))
            .map(|r| r.account_id),
        None => None,
    };
    if let Some(account_id) = pending_account {
        let creator = state.account_creator.clone();
        tokio::spawn(async move {
            match creator.complete_registration(&account_id, Duration::from_secs(180)).await {
                Ok(report) => info!("Registration for {} finished with status {:?}", report.username, report.status),
                Err(e) => warn!("Registration completion failed: {}", e),
            }
        });
    }
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn dismiss_human_task(state: State<'_, AppState>, task_id: String) -> CmdResult<()> {
    let id = TaskId(parse_uuid(&task_id, "task")?);
    state.human_controller.dismiss_task(&id).await.map_err(err)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_vault_status(state: State<'_, AppState>) -> CmdResult<VaultStatus> {
    Ok(state.vault.status().await)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn is_vault_unlocked(state: State<'_, AppState>) -> CmdResult<bool> {
    Ok(state.vault.is_unlocked().await)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn unlock_vault(state: State<'_, AppState>, password: String) -> CmdResult<()> {
    state.vault.unlock(&password).await.map_err(err)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn lock_vault(state: State<'_, AppState>) -> CmdResult<()> {
    state.vault.lock().await;
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn change_vault_password(state: State<'_, AppState>, current: String, new_password: String) -> CmdResult<()> {
    state.vault.change_master_password(&current, &new_password).await.map_err(err)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_vault_secrets(state: State<'_, AppState>) -> CmdResult<Vec<SecretMetadataView>> {
    if !state.vault.is_unlocked().await {
        return Err("Vault is locked".into());
    }
    Ok(state.vault.list_metadata().await)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn store_vault_secret(
    state: State<'_, AppState>,
    label: String,
    secret_type: String,
    value: String,
    account_id: Option<String>,
) -> CmdResult<String> {
    let label = label.trim().to_string();
    if label.is_empty() || value.is_empty() {
        return Err("Label and value are required".into());
    }
    let secret_type: SecretType =
        serde_json::from_value(serde_json::Value::String(secret_type.clone())).map_err(|_| format!("Unknown secret type {secret_type}"))?;
    let id = state
        .vault
        .store_secret(account_id.filter(|a| !a.is_empty()), secret_type, label, SecretPayload { value, metadata: None })
        .await
        .map_err(err)?;
    Ok(id.0.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn reveal_vault_secret(state: State<'_, AppState>, secret_id: String) -> CmdResult<String> {
    let id = SecretId(parse_uuid(&secret_id, "secret")?);
    let payload = state.vault.get_secret(&id).await.map_err(err)?;
    Ok(payload.value.clone())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn delete_vault_secret(state: State<'_, AppState>, secret_id: String) -> CmdResult<()> {
    let id = SecretId(parse_uuid(&secret_id, "secret")?);
    state.vault.delete_secret(&id).await.map_err(err)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_lm_profiles(state: State<'_, AppState>) -> CmdResult<Vec<ModelProfile>> {
    Ok(state.lm_gateway.list_profiles().await)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn register_lm_profile(state: State<'_, AppState>, profile: ModelProfile) -> CmdResult<()> {
    let mut profile = profile;
    profile.id = profile.id.trim().to_lowercase().replace(' ', "-");
    if profile.id.is_empty() || profile.model_id.trim().is_empty() {
        return Err("Profile id and model id are required".into());
    }
    if profile.id == openaccounts_lm_gateway::DEFAULT_PROFILE_ID || profile.id == "nim-vision-cascade" {
        return Err("Built-in profiles cannot be replaced".into());
    }
    if let Some(endpoint) = profile.endpoint.as_deref().filter(|e| !e.trim().is_empty()) {
        if !(endpoint.starts_with("http://") || endpoint.starts_with("https://")) {
            return Err("Endpoint must start with http:// or https://".into());
        }
    }
    if profile.name.trim().is_empty() {
        profile.name = profile.id.clone();
    }
    crate::profiles::save(&state.db, &profile).await?;
    state.lm_gateway.register_profile(profile).await;
    *state.connection_cache.write().await = None;
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn remove_lm_profile(state: State<'_, AppState>, profile_id: String) -> CmdResult<()> {
    if !state.lm_gateway.remove_profile(&profile_id).await {
        return Err(format!("LM profile '{profile_id}' cannot be removed"));
    }
    crate::profiles::delete(&state.db, &profile_id).await?;
    if state.db.get_setting(agent::CHAT_PROFILE_SETTING).await.ok().flatten().as_deref() == Some(profile_id.as_str()) {
        state.db.set_setting(agent::CHAT_PROFILE_SETTING, openaccounts_lm_gateway::DEFAULT_PROFILE_ID).await.map_err(err)?;
    }
    *state.connection_cache.write().await = None;
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn set_chat_lm_profile(state: State<'_, AppState>, profile_id: String) -> CmdResult<()> {
    if !state.lm_gateway.has_profile(&profile_id).await {
        return Err(format!("LM profile '{profile_id}' is not registered"));
    }
    state.db.set_setting(agent::CHAT_PROFILE_SETTING, &profile_id).await.map_err(err)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_chat_lm_profile(state: State<'_, AppState>) -> CmdResult<String> {
    Ok(state
        .db
        .get_setting(agent::CHAT_PROFILE_SETTING)
        .await
        .map_err(err)?
        .unwrap_or_else(|| openaccounts_lm_gateway::DEFAULT_PROFILE_ID.to_string()))
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_lm_usage(state: State<'_, AppState>) -> CmdResult<HashMap<String, ProfileUsage>> {
    Ok(state.lm_gateway.usage_report().await)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn agent_chat(
    state: State<'_, AppState>,
    message: String,
    scope: String,
    history: Option<Vec<ChatTurn>>,
    attachments: Option<Vec<ChatAttachmentInput>>,
) -> CmdResult<AgentChatReply> {
    agent::chat(&state, &message, &scope, history.unwrap_or_default(), attachments.unwrap_or_default()).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn execute_agent_action(state: State<'_, AppState>, action: agent::AgentAction) -> CmdResult<String> {
    let result = agent::execute_action(&state, &action).await;
    let status = if result.is_ok() { "Completed" } else { "Failed" };
    let details = serde_json::json!({ "kind": action.kind, "args": action.args, "result": result.clone().unwrap_or_else(|e| e) }).to_string();
    let _ = state
        .db
        .append_audit(openaccounts_database::AuditInsert {
            execution_id: "AGENT_CHAT",
            account_id: None,
            automation_id: None,
            action_type: "AgentAction",
            risk_level: "Low",
            details: &details,
            status,
        })
        .await;
    result
}

#[tauri::command(rename_all = "snake_case")]
pub async fn trigger_kill_switch(state: State<'_, AppState>) -> CmdResult<()> {
    state.automation_engine.trigger_kill_switch();
    state.social_media_manager.pause_autonomous_loop();
    state.db.set_setting(AUTOPILOT_SETTING, "false").await.map_err(err)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn reset_kill_switch(state: State<'_, AppState>) -> CmdResult<()> {
    state.automation_engine.reset_kill_switch();
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn is_kill_switch_active(state: State<'_, AppState>) -> CmdResult<bool> {
    Ok(state.automation_engine.is_kill_switch_active())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_audit_log(state: State<'_, AppState>, limit: Option<u32>) -> CmdResult<Vec<AuditRecord>> {
    state.audit_engine.recent(limit.unwrap_or(100).min(1000)).await.map_err(err)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn verify_audit_chain(state: State<'_, AppState>) -> CmdResult<ChainVerification> {
    state.audit_engine.verify_integrity().await.map_err(err)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_platform_profiles(state: State<'_, AppState>) -> CmdResult<Vec<PlatformKnowledgeProfile>> {
    let mut profiles = state.platform_intelligence.list_profiles().await;
    profiles.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(profiles)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_platform_profile(
    state: State<'_, AppState>,
    platform_id: String,
) -> CmdResult<Option<PlatformKnowledgeProfile>> {
    Ok(state.platform_intelligence.get_profile(&platform_id).await)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn check_multi_account_policy(
    state: State<'_, AppState>,
    platform_id: String,
    current_count: u32,
    purpose: String,
) -> CmdResult<PolicyCheckResult> {
    state
        .policy_engine
        .check_multi_account_eligibility(&platform_id, current_count, &purpose, false)
        .await
        .map_err(err)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn run_policy_monitor_cycle(state: State<'_, AppState>) -> CmdResult<PolicyCycleReport> {
    Ok(state.policy_monitor.run_policy_check_cycle_report().await)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_policy_monitor_report(state: State<'_, AppState>) -> CmdResult<Option<PolicyCycleReport>> {
    Ok(state.policy_monitor.last_report().await)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_policy_alerts(state: State<'_, AppState>) -> CmdResult<Vec<PolicyMonitorAlert>> {
    Ok(state.policy_monitor.last_report().await.map(|r| r.alerts).unwrap_or_default())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn record_policy_change(
    state: State<'_, AppState>,
    platform_id: String,
    section: String,
    new_summary: String,
) -> CmdResult<()> {
    state
        .policy_monitor
        .record_manual_policy_update(&platform_id, &section, &new_summary, true)
        .await
        .map_err(err)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_policy_version_history(
    state: State<'_, AppState>,
    platform_id: String,
) -> CmdResult<Vec<PolicyVersionRecord>> {
    Ok(state.policy_versioning.get_history(&platform_id).await)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_email_identities(state: State<'_, AppState>) -> CmdResult<Vec<EmailIdentity>> {
    Ok(state.email_manager.list_identities().await)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_email_providers(state: State<'_, AppState>) -> CmdResult<Vec<String>> {
    Ok(state.email_manager.list_providers().await)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn allocate_email_identity(
    state: State<'_, AppState>,
    prefix: String,
    platform: Option<String>,
    provider_id: Option<String>,
) -> CmdResult<EmailIdentity> {
    let provider = match provider_id {
        Some(p) => p,
        None => state
            .email_manager
            .list_providers()
            .await
            .into_iter()
            .next()
            .ok_or("No mailbox connected. Set GMAIL_ACCESS_TOKEN or OUTLOOK_ACCESS_TOKEN and restart.")?,
    };
    state.email_manager.allocate_identity(&provider, &prefix, platform).await.map_err(err)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_managed_multi_accounts(state: State<'_, AppState>) -> CmdResult<Vec<ManagedAccountView>> {
    state.multi_account_registry.list_managed_accounts().await.map_err(err)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn execute_10_step_account_preflight(
    state: State<'_, AppState>,
    platform: String,
    category_str: String,
    desired_username: Option<String>,
    preferred_email: Option<String>,
    email_provider_id: Option<String>,
) -> CmdResult<AccountCreationPreflightReport> {
    let category = match category_str.to_lowercase().as_str() {
        "business" => AccountCategory::Business,
        "brand" => AccountCategory::Brand,
        "creative_project" | "creativeproject" => AccountCategory::CreativeProject,
        "community" => AccountCategory::Community,
        _ => AccountCategory::Personal,
    };
    let browser = Arc::new(EmbeddedBrowserAdapter::new());
    let req = AccountCreationPreflightRequest {
        platform,
        category,
        desired_username,
        preferred_email,
        email_provider_id,
    };
    state.account_creator.execute_10_step_preflight(browser, req).await.map_err(err)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_pending_registrations(state: State<'_, AppState>) -> CmdResult<Vec<AccountCreationPreflightReport>> {
    Ok(state.account_creator.pending_registrations().await)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn complete_account_registration(
    state: State<'_, AppState>,
    account_id: String,
    timeout_seconds: Option<u64>,
) -> CmdResult<AccountCreationPreflightReport> {
    let id = AccountId(parse_uuid(&account_id, "account")?);
    state
        .account_creator
        .complete_registration(&id, Duration::from_secs(timeout_seconds.unwrap_or(120).min(900)))
        .await
        .map_err(err)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn cancel_account_registration(state: State<'_, AppState>, account_id: String) -> CmdResult<()> {
    let id = AccountId(parse_uuid(&account_id, "account")?);
    state.account_creator.cancel_registration(&id).await.map_err(err)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_brands(state: State<'_, AppState>) -> CmdResult<Vec<BrandProfile>> {
    Ok(state.brand_manager.list_brands().await)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_campaigns(state: State<'_, AppState>) -> CmdResult<Vec<CampaignBrief>> {
    Ok(state.content_strategy.list_campaigns().await)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_calendar_posts(state: State<'_, AppState>) -> CmdResult<Vec<CalendarPostSlot>> {
    Ok(state.content_strategy.list_calendar().await)
}

async fn account_for(state: &AppState, account_id: &str) -> CmdResult<AccountRecord> {
    let id = AccountId(parse_uuid(account_id, "account")?);
    state
        .account_registry
        .get_account(&id)
        .await
        .map_err(err)?
        .ok_or_else(|| format!("Account {account_id} not found"))
}

#[tauri::command(rename_all = "snake_case")]
pub async fn schedule_post(
    state: State<'_, AppState>,
    account_id: String,
    caption: String,
    scheduled_at: Option<String>,
    media: Option<Vec<String>>,
    approve: Option<bool>,
) -> CmdResult<CalendarPostSlot> {
    let account = account_for(&state, &account_id).await?;
    let caption = caption.trim().to_string();
    if caption.is_empty() {
        return Err("Caption is required".into());
    }
    let platform = account.platform.to_string();
    let limit = openaccounts_content_strategy::caption_char_limit(&platform);
    if caption.chars().count() > limit {
        return Err(format!("Caption is {} characters; {platform} allows {limit}", caption.chars().count()));
    }
    let scheduled_time = match scheduled_at.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(ts) => chrono::DateTime::parse_from_rfc3339(ts)
            .map(|d| d.with_timezone(&chrono::Utc))
            .map_err(|_| format!("'{ts}' is not a valid RFC 3339 time"))?,
        None => chrono::Utc::now(),
    };
    let hashtags: Vec<String> = caption
        .split_whitespace()
        .filter(|w| w.starts_with('#') && w.len() > 1)
        .map(|w| w.trim_end_matches(|c: char| !c.is_alphanumeric()).to_string())
        .collect();
    let media = media.unwrap_or_default().into_iter().filter(|m| !m.trim().is_empty()).collect::<Vec<_>>();
    let slot = CalendarPostSlot {
        slot_id: format!("slot-{}", uuid::Uuid::new_v4().simple()),
        campaign_id: None,
        brand_id: state.brand_manager.list_brands().await.first().map(|b| b.brand_id.clone()).unwrap_or_default(),
        target_platform: platform,
        scheduled_time,
        content_type: if media.is_empty() { "Text_Post".into() } else { "Media_Post".into() },
        caption,
        hashtags,
        asset_ids: media,
        is_approved: approve.unwrap_or(false),
        status: "Scheduled".into(),
        account_id: Some(account.id.0.to_string()),
        published_at: None,
    };
    state.content_strategy.schedule_post_slot(slot.clone()).await;
    Ok(slot)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn approve_calendar_slot(state: State<'_, AppState>, slot_id: String) -> CmdResult<()> {
    if state.content_strategy.approve_slot(&slot_id).await {
        Ok(())
    } else {
        Err(format!("Slot {slot_id} not found"))
    }
}

#[tauri::command(rename_all = "snake_case")]
pub async fn delete_calendar_slot(state: State<'_, AppState>, slot_id: String) -> CmdResult<()> {
    match state.content_strategy.remove_slot(&slot_id).await.map_err(err)? {
        true => Ok(()),
        false => Err(format!("Slot {slot_id} not found")),
    }
}

#[tauri::command(rename_all = "snake_case")]
pub async fn draft_post_caption(state: State<'_, AppState>, account_id: String, brief: String) -> CmdResult<String> {
    let account = account_for(&state, &account_id).await?;
    let profile = state
        .db
        .get_setting(agent::CHAT_PROFILE_SETTING)
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| openaccounts_lm_gateway::DEFAULT_PROFILE_ID.to_string());
    state
        .content_strategy
        .draft_caption(&account.platform.to_string(), &account.username, &brief, &profile)
        .await
        .map_err(err)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn research_trends(state: State<'_, AppState>, platform: String) -> CmdResult<Vec<TrendTopic>> {
    state.content_strategy.research_trends(&platform).await.map_err(err)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_creative_tools(state: State<'_, AppState>) -> CmdResult<Vec<ToolCapability>> {
    Ok(state.creative_pipeline.list_tools().await)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn rediscover_creative_tools(state: State<'_, AppState>) -> CmdResult<Vec<ToolCapability>> {
    state.creative_pipeline.discover_tools().await;
    Ok(state.creative_pipeline.list_tools().await)
}

fn spawn_render(state: &AppState, job_id: String) {
    let pipeline = state.creative_pipeline.clone();
    tokio::spawn(async move {
        if let Err(e) = pipeline.execute_job(&job_id).await {
            warn!("Creative job {} failed: {}", job_id, e);
        }
    });
}

#[tauri::command(rename_all = "snake_case")]
pub async fn create_creative_job(state: State<'_, AppState>, spec: CreativeAssetSpec, start: Option<bool>) -> CmdResult<String> {
    let id = state.creative_pipeline.create_production_job(spec).await.map_err(err)?;
    if start.unwrap_or(true) {
        spawn_render(&state, id.clone());
    }
    Ok(id)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_creative_job_status(state: State<'_, AppState>, job_id: String) -> CmdResult<Option<CreativeJobStatus>> {
    Ok(state.creative_pipeline.get_job_status(&job_id).await)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_creative_jobs(state: State<'_, AppState>) -> CmdResult<Vec<CreativeJobStatus>> {
    Ok(state.creative_pipeline.list_jobs().await)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_publishing_posts(state: State<'_, AppState>) -> CmdResult<Vec<SocialPostPayload>> {
    Ok(state.publishing_engine.list_posts().await)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn approve_and_publish_post(state: State<'_, AppState>, post_id: String) -> CmdResult<Option<String>> {
    if state.automation_engine.is_kill_switch_active() {
        return Err("Kill switch is active".into());
    }
    state
        .publishing_engine
        .approve_and_publish(&post_id)
        .await
        .map(|r| r.remote_permalink)
        .map_err(err)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_unified_inbox(state: State<'_, AppState>) -> CmdResult<Vec<UnifiedInboxMessage>> {
    Ok(state.engagement_agent.list_inbox().await)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn sync_inbox(state: State<'_, AppState>, account_id: String) -> CmdResult<Vec<UnifiedInboxMessage>> {
    let id = AccountId(parse_uuid(&account_id, "account")?);
    let account = state
        .account_registry
        .get_account(&id)
        .await
        .map_err(err)?
        .ok_or_else(|| format!("Account {account_id} not found"))?;
    state
        .engagement_agent
        .sync_inbox(id, &account.platform.to_string())
        .await
        .map_err(err)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn approve_and_send_inbox_reply(
    state: State<'_, AppState>,
    message_id: String,
    final_reply: String,
) -> CmdResult<()> {
    if state.automation_engine.is_kill_switch_active() {
        return Err("Kill switch is active".into());
    }
    state
        .engagement_agent
        .approve_and_send_reply(&message_id, &final_reply)
        .await
        .map_err(err)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn redraft_inbox_reply(state: State<'_, AppState>, message_id: String) -> CmdResult<Option<String>> {
    state.engagement_agent.redraft_reply(&message_id).await.map_err(err)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn archive_inbox_message(state: State<'_, AppState>, message_id: String) -> CmdResult<()> {
    state.engagement_agent.archive_message(&message_id).await.map_err(err)
}

#[derive(Debug, Clone, Serialize)]
pub struct EngagementQuota {
    pub account_id: String,
    pub account_handle: String,
    pub platform: String,
    pub action: String,
    pub used: u32,
    pub cap: u32,
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_engagement_quotas(state: State<'_, AppState>) -> CmdResult<Vec<EngagementQuota>> {
    let actions = [
        (EngagementActionType::Like, "likes"),
        (EngagementActionType::Follow, "follows"),
        (EngagementActionType::Comment, "comments"),
        (EngagementActionType::Reply, "replies"),
        (EngagementActionType::Subscribe, "subs"),
        (EngagementActionType::Reshare, "reshares"),
    ];
    let mut out = Vec::new();
    for acc in state.account_registry.list_accounts().await.map_err(err)? {
        let platform = acc.platform.to_string();
        if platform == "email" || platform == "paypal" || platform == "shopify" {
            continue;
        }
        let caps = state.engagement_agent.caps_for(&acc.id).await;
        for (action, label) in actions {
            let cap = caps.limit(action);
            if cap == 0 {
                continue;
            }
            let remaining = state.engagement_agent.remaining_quota(&acc.id, action).await;
            out.push(EngagementQuota {
                account_id: acc.id.0.to_string(),
                account_handle: acc.username.clone(),
                platform: platform.clone(),
                action: label.into(),
                used: cap.saturating_sub(remaining),
                cap,
            });
        }
    }
    Ok(out)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_social_analytics(state: State<'_, AppState>) -> CmdResult<Vec<VerifiedPlatformMetric>> {
    Ok(state.social_analytics.list_metrics().await)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn refresh_social_metrics(state: State<'_, AppState>, account_id: String) -> CmdResult<VerifiedPlatformMetric> {
    let id = AccountId(parse_uuid(&account_id, "account")?);
    let account = state
        .account_registry
        .get_account(&id)
        .await
        .map_err(err)?
        .ok_or_else(|| format!("Account {account_id} not found"))?;
    state
        .social_analytics
        .refresh_metrics(&account_id, &account.platform.to_string())
        .await
        .map_err(err)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn start_autonomous_marketing_loop(state: State<'_, AppState>) -> CmdResult<()> {
    if state.automation_engine.is_kill_switch_active() {
        return Err("Reset the kill switch before resuming autopilot".into());
    }
    state.social_media_manager.start_autonomous_loop();
    state.db.set_setting(AUTOPILOT_SETTING, "true").await.map_err(err)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn pause_autonomous_marketing_loop(state: State<'_, AppState>) -> CmdResult<()> {
    state.social_media_manager.pause_autonomous_loop();
    state.db.set_setting(AUTOPILOT_SETTING, "false").await.map_err(err)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn is_autonomous_marketing_loop_running(state: State<'_, AppState>) -> CmdResult<bool> {
    Ok(state.social_media_manager.is_loop_running())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_collaborating_agents(state: State<'_, AppState>) -> CmdResult<Vec<SubAgentAssignment>> {
    Ok(state.social_media_manager.list_collaborating_agents().await)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_stick_bridge_state(state: State<'_, AppState>) -> CmdResult<StickBridgeState> {
    Ok(state.social_media_manager.get_stick_bridge_state())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_last_garbage_collection(state: State<'_, AppState>) -> CmdResult<Option<GarbageCollectionReport>> {
    Ok(state.gc_report.read().await.clone())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn run_garbage_collection(state: State<'_, AppState>, apply: bool) -> CmdResult<GarbageCollectionReport> {
    let manager = state.social_media_manager.clone();
    let report = tokio::task::spawn_blocking(move || manager.run_garbage_collection(apply))
        .await
        .map_err(err)?
        .map_err(err)?;
    *state.gc_report.write().await = Some(report.clone());
    Ok(report)
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
) -> CmdResult<StickBridgeMessage> {
    state
        .social_media_manager
        .send_stick_bridge_message(&from_agent, &to_agent, &channel, &subject, &body, create_ceo_directive)
        .map_err(err)
}
