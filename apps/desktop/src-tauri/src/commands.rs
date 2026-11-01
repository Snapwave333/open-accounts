use crate::state::AppState;
use openaccounts_account_creator::{AccountCreationPreflightReport, AccountCreationPreflightRequest};
use openaccounts_account_identity_manager::AccountCategory;
use openaccounts_account_registry::AccountRecord;
use openaccounts_chrome_relay_adapter::{ChromeRelayAdapter, ChromeRelayConfig};
use openaccounts_core::{AccountId, Platform, SystemStats, TaskId};
use openaccounts_credential_vault::SecretMetadataView;
use openaccounts_email_identity::EmailIdentity;
use openaccounts_human_intervention::HumanTask;
use openaccounts_lm_gateway::ModelProfile;
use openaccounts_multi_account_registry::ManagedAccountView;
use openaccounts_platform_intelligence::PlatformKnowledgeProfile;
use openaccounts_policy_engine::PolicyCheckResult;
use openaccounts_policy_monitor::PolicyMonitorAlert;
use openaccounts_policy_versioning::PolicyVersionRecord;
use std::sync::Arc;
use tauri::State;

#[tauri::command]
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
        last_active: Some(chrono::Utc::now()),
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
    state.lm_gateway.register_profile(profile).await;
    Ok(())
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

// --- PLATFORM INTELLIGENCE & COMPLIANCE COMMANDS ---

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

#[tauri::command]
pub async fn get_policy_version_history(
    state: State<'_, AppState>,
    platform_id: String,
) -> Result<Vec<PolicyVersionRecord>, String> {
    Ok(state.policy_versioning.get_history(&platform_id).await)
}

// --- EMAIL IDENTITY COMMANDS ---

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

// --- MULTI-ACCOUNT REGISTRY COMMANDS ---

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
