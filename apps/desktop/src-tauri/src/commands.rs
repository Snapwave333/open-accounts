use crate::state::AppState;
use openaccounts_account_registry::AccountRecord;
use openaccounts_core::{AccountId, Platform, SystemStats, TaskId};
use openaccounts_credential_vault::SecretMetadataView;
use openaccounts_human_intervention::HumanTask;
use openaccounts_lm_gateway::ModelProfile;
use tauri::State;

#[tauri::command]
pub async fn get_system_stats(state: State<'_, AppState>) -> Result<SystemStats, String> {
    let accounts = state.account_registry.list_accounts().await.unwrap_or_default();
    let tasks = state.human_controller.list_pending_tasks().await;
    let is_kill_active = state.automation_engine.is_kill_switch_active();

    Ok(SystemStats {
        total_accounts: accounts.len() as u64,
        active_automations: 6, // illustrative default
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
