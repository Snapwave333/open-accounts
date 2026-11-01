#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod state;

use commands::*;
use state::AppState;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let state = AppState::init().await;

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            get_system_stats,
            list_accounts,
            create_account,
            list_human_tasks,
            resolve_human_task,
            is_vault_unlocked,
            unlock_vault,
            lock_vault,
            list_vault_secrets,
            list_lm_profiles,
            register_lm_profile,
            trigger_kill_switch,
            reset_kill_switch,
            is_kill_switch_active,
            list_platform_profiles,
            get_platform_profile,
            check_multi_account_policy,
            run_policy_monitor_cycle,
            simulate_policy_change,
            get_policy_version_history,
            list_email_identities,
            allocate_email_identity,
            list_managed_multi_accounts,
            execute_10_step_account_preflight,
        ])
        .run(tauri::generate_context!())
        .expect("error while running OpenAccounts application");
}
