#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod autonomy;
mod campaigns;
mod ceo;
mod commands;
mod influencer;
mod state;

use commands::*;
use serde::{Deserialize, Serialize};
use state::AppState;
use std::path::PathBuf;
use tauri::{Manager, PhysicalPosition, PhysicalSize, WindowEvent};
use tracing::{info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// The main window's last size, position and maximized state, restored on
/// the next launch so the console opens where the operator left it.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct WindowState {
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    maximized: bool,
}

fn window_state_path(data_dir: &PathBuf) -> PathBuf {
    data_dir.join("window_state.json")
}

fn restore_window(window: &tauri::WebviewWindow, path: &PathBuf) {
    let Some(saved) = std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str::<WindowState>(&raw).ok())
    else {
        return;
    };
    if saved.width < 640 || saved.height < 480 {
        return;
    }
    // Only restore a position that is still on a monitor.
    let on_screen = window
        .available_monitors()
        .map(|monitors| {
            monitors.iter().any(|m| {
                let p = m.position();
                let s = m.size();
                saved.x + 50 >= p.x
                    && saved.y + 50 >= p.y
                    && saved.x + 50 < p.x + s.width as i32
                    && saved.y + 50 < p.y + s.height as i32
            })
        })
        .unwrap_or(false);
    let _ = window.set_size(PhysicalSize::new(saved.width, saved.height));
    if on_screen {
        let _ = window.set_position(PhysicalPosition::new(saved.x, saved.y));
    }
    if saved.maximized {
        let _ = window.maximize();
    }
}

fn save_window(window: &tauri::WebviewWindow, path: &PathBuf) {
    let maximized = window.is_maximized().unwrap_or(false);
    let (Ok(size), Ok(pos)) = (window.inner_size(), window.outer_position()) else {
        return;
    };
    if window.is_minimized().unwrap_or(false) {
        return;
    }
    let mut state = WindowState {
        width: size.width,
        height: size.height,
        x: pos.x,
        y: pos.y,
        maximized,
    };
    if maximized {
        // Keep the pre-maximize geometry so un-maximizing later lands sensibly.
        if let Some(prev) = std::fs::read_to_string(path)
            .ok()
            .and_then(|raw| serde_json::from_str::<WindowState>(&raw).ok())
        {
            state.width = prev.width;
            state.height = prev.height;
            state.x = prev.x;
            state.y = prev.y;
        }
    }
    if let Ok(json) = serde_json::to_string_pretty(&state) {
        if let Err(e) = std::fs::write(path, json) {
            warn!("cannot save window state to {}: {e}", path.display());
        }
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let state = AppState::init().await;
    let window_state_file = window_state_path(&state.data_dir);
    let window_state_for_events = window_state_file.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(state)
        .setup(move |app| {
            if let Some(window) = app.get_webview_window("main") {
                restore_window(&window, &window_state_file);
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }

            // Start the supervised autonomy loop with the real studio jobs.
            let state = app.state::<AppState>();
            state.autonomy.start(Some(app.handle().clone()));
            info!("OpenAccounts window ready");

            Ok(())
        })
        .on_window_event(move |window, event| {
            if window.label() != "main" {
                return;
            }
            match event {
                WindowEvent::Resized(_) | WindowEvent::Moved(_) | WindowEvent::CloseRequested { .. } => {
                    if let Some(w) = window.get_webview_window("main") {
                        save_window(&w, &window_state_for_events);
                    }
                }
                _ => {}
            }
        })
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
            get_policy_version_history,
            list_email_identities,
            allocate_email_identity,
            list_managed_multi_accounts,
            execute_10_step_account_preflight,
            list_brands,
            list_campaigns,
            list_calendar_posts,
            research_trends,
            list_creative_tools,
            create_creative_job,
            get_creative_job_status,
            list_publishing_posts,
            approve_and_publish_post,
            list_unified_inbox,
            approve_and_send_inbox_reply,
            list_social_analytics,
            start_autonomous_marketing_loop,
            pause_autonomous_marketing_loop,
            is_autonomous_marketing_loop_running,
            get_stick_bridge_state,
            run_garbage_collection,
            get_gc_report,
            open_external,
            send_stick_bridge_message,
            get_autonomy_status,
            run_autonomy_cycle_now,
            list_autonomy_cycles,
            get_chrome_relay_status,
            ceo_history,
            ceo_send,
            ceo_confirm,
            ceo_cancel,
            ceo_clear,
            list_campaign_overviews,
            get_campaign_detail,
            get_ui_state,
            set_ui_state,
            fanvue_status,
            fanvue_calendar,
            fanvue_posts,
            fanvue_analytics,
            fanvue_policy_status,
            fanvue_weekly_report,
        ])
        .run(tauri::generate_context!())
        .expect("error while running OpenAccounts application");
}
