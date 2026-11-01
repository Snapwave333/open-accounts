#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod agent;
mod commands;
mod flow;
mod state;
mod tokens;

use commands::*;
use state::AppState;
use tauri::Manager;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let state = match AppState::init().await {
        Ok(state) => state,
        Err(e) => {
            tracing::error!("OpenAccounts failed to start: {}", e);
            std::process::exit(1);
        }
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(state)
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_system_stats,
            get_activity_flow,
            run_flow_item,
            list_scheduled_jobs,
            list_accounts,
            create_account,
            delete_account,
            list_human_tasks,
            resolve_human_task,
            dismiss_human_task,
            get_vault_status,
            is_vault_unlocked,
            unlock_vault,
            lock_vault,
            change_vault_password,
            list_vault_secrets,
            store_vault_secret,
            reveal_vault_secret,
            delete_vault_secret,
            list_lm_profiles,
            register_lm_profile,
            set_chat_lm_profile,
            get_lm_usage,
            agent_chat,
            trigger_kill_switch,
            reset_kill_switch,
            is_kill_switch_active,
            list_audit_log,
            verify_audit_chain,
            list_platform_profiles,
            get_platform_profile,
            check_multi_account_policy,
            run_policy_monitor_cycle,
            get_policy_monitor_report,
            list_policy_alerts,
            record_policy_change,
            get_policy_version_history,
            list_email_identities,
            list_email_providers,
            allocate_email_identity,
            list_managed_multi_accounts,
            execute_10_step_account_preflight,
            list_pending_registrations,
            complete_account_registration,
            cancel_account_registration,
            list_brands,
            list_campaigns,
            list_calendar_posts,
            research_trends,
            list_creative_tools,
            rediscover_creative_tools,
            create_creative_job,
            get_creative_job_status,
            list_creative_jobs,
            list_publishing_posts,
            approve_and_publish_post,
            list_unified_inbox,
            sync_inbox,
            approve_and_send_inbox_reply,
            redraft_inbox_reply,
            archive_inbox_message,
            get_engagement_quotas,
            list_social_analytics,
            refresh_social_metrics,
            start_autonomous_marketing_loop,
            pause_autonomous_marketing_loop,
            is_autonomous_marketing_loop_running,
            list_collaborating_agents,
            get_stick_bridge_state,
            get_last_garbage_collection,
            run_garbage_collection,
            send_stick_bridge_message,
        ])
        .run(tauri::generate_context!())
        .expect("error while running OpenAccounts application");
}
