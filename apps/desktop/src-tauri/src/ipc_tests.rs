use crate::{commands::*, state::AppState};
use regex::Regex;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use tauri::{
    ipc::{CallbackFn, InvokeBody},
    test::{get_ipc_response, mock_builder, mock_context, noop_assets, INVOKE_KEY},
    webview::InvokeRequest,
    WebviewWindow, WebviewWindowBuilder,
};

const MAIN_RS: &str = include_str!("main.rs");
const UI_SERVICE: &str = include_str!("../../angular/src/app/services/tauri.service.ts");

fn registered_commands() -> BTreeSet<String> {
    let start = MAIN_RS.find("generate_handler![").expect("handler list");
    let end = MAIN_RS[start..].find(']').expect("handler end") + start;
    MAIN_RS[start + "generate_handler![".len()..end]
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn ui_commands() -> BTreeSet<String> {
    let re = Regex::new(r#"(?:invoke|act|load)(?:<[^>]*>)?\(\s*'([a-z0-9_]+)'"#).unwrap();
    let ternary = Regex::new(r#"act\(\s*this\.[^?]+\?\s*'([a-z0-9_]+)'\s*:\s*'([a-z0-9_]+)'"#).unwrap();
    let mut out: BTreeSet<String> = re.captures_iter(UI_SERVICE).map(|c| c[1].to_string()).collect();
    for c in ternary.captures_iter(UI_SERVICE) {
        out.insert(c[1].to_string());
        out.insert(c[2].to_string());
    }
    out
}

struct Harness {
    window: WebviewWindow<tauri::test::MockRuntime>,
    _app: tauri::App<tauri::test::MockRuntime>,
}

fn invoke(window: &WebviewWindow<tauri::test::MockRuntime>, cmd: &str, args: Value) -> Result<Value, Value> {
    get_ipc_response(
        window,
        InvokeRequest {
            cmd: cmd.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: "tauri://localhost".parse().unwrap(),
            body: InvokeBody::Json(args),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        },
    )
    .map(|b| b.deserialize::<Value>().unwrap())
}

fn boot(root: &std::path::Path) -> (tokio::runtime::Runtime, Harness) {
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().unwrap();
    tauri::async_runtime::set(rt.handle().clone());
    std::env::set_var("OPENACCOUNTS_STICK_ROOT", std::env::temp_dir().join("oa-no-studio-fixture"));
    let state = rt.block_on(AppState::init_at(root.to_path_buf())).expect("boot");
    let app = mock_builder()
        .manage(state)
        .invoke_handler(app_commands!())
        .build(mock_context(noop_assets()))
        .expect("mock app");
    let window = WebviewWindowBuilder::new(&app, "main", Default::default()).build().unwrap();
    (rt, Harness { window, _app: app })
}

impl Harness {
    fn call(&self, cmd: &str, args: Value) -> Result<Value, Value> {
        invoke(&self.window, cmd, args)
    }

    fn ok(&self, cmd: &str, args: Value) -> Value {
        self.call(cmd, args).unwrap_or_else(|e| panic!("{cmd} failed: {e}"))
    }

    fn err(&self, cmd: &str, args: Value) -> String {
        match self.call(cmd, args) {
            Ok(v) => panic!("{cmd} unexpectedly succeeded with {v}"),
            Err(e) => e.as_str().map(str::to_string).unwrap_or_else(|| e.to_string()),
        }
    }
}

#[test]
fn every_ui_command_is_registered() {
    let registered = registered_commands();
    let missing: Vec<_> = ui_commands().difference(&registered).cloned().collect();
    assert!(missing.is_empty(), "UI invokes unregistered commands: {missing:?}");
    assert!(registered.len() > 60);
    let ui = ui_commands();
    for expected in ["trigger_kill_switch", "reset_kill_switch", "start_autonomous_marketing_loop", "agent_chat", "store_vault_secret"] {
        assert!(ui.contains(expected), "UI command scan missed {expected}");
    }
}

#[test]
fn ui_payloads_round_trip_through_real_ipc() {
    let root = std::env::temp_dir().join(format!("oa-ipc-{}", uuid::Uuid::new_v4()));
    let (_rt, h) = boot(&root);

    let stats = h.ok("get_system_stats", json!({}));
    assert_eq!(stats["system_status"], "ONLINE");
    let seeded = stats["total_accounts"].as_u64().unwrap();

    let id = h.ok("create_account", json!({ "platform": "Reddit", "username": "e2e_user", "email": null }));
    let id = id.as_str().unwrap().to_string();
    assert!(h.err("create_account", json!({ "platform": "reddit", "username": "E2E_USER", "email": null })).contains("already registered"));
    assert!(h.err("create_account", json!({ "platform": "x", "username": "a", "email": "nope" })).contains("not a valid email"));
    let accounts = h.ok("list_accounts", json!({}));
    let created = accounts.as_array().unwrap().iter().find(|a| a["id"] == id.as_str()).unwrap();
    assert_eq!(created["platform"], "reddit");
    assert_eq!(h.ok("get_system_stats", json!({}))["total_accounts"].as_u64().unwrap(), seeded + 1);

    let quotas = h.ok("get_engagement_quotas", json!({}));
    assert!(quotas.as_array().unwrap().iter().any(|q| q["account_id"] == id.as_str() && q["used"] == 0));

    let vault = h.ok("get_vault_status", json!({}));
    assert_eq!(vault["initialized"], false);
    assert!(h.err("unlock_vault", json!({ "password": "short" })).contains("at least"));
    h.ok("unlock_vault", json!({ "password": "ipc-master-password" }));
    let secret_id = h.ok(
        "store_vault_secret",
        json!({ "label": "reddit:access_token", "secret_type": "OAuthToken", "value": "tok-ipc", "account_id": null }),
    );
    let secret_id = secret_id.as_str().unwrap();
    assert_eq!(h.ok("reveal_vault_secret", json!({ "secret_id": secret_id })), "tok-ipc");
    assert_eq!(h.ok("list_vault_secrets", json!({})).as_array().unwrap().len(), 1);
    let connections = h.ok("get_connection_status", json!({ "force": false }));
    assert!(connections.as_array().unwrap().iter().any(|c| c["kind"] == "social"));
    h.ok("delete_vault_secret", json!({ "secret_id": secret_id }));
    h.ok("lock_vault", json!({}));
    assert!(h.err("list_vault_secrets", json!({})).contains("locked"));

    let flow = h.ok("get_activity_flow", json!({}));
    let job = flow["up_next"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["kind"] == "job" && i["title"] == "Autopilot marketing cycle")
        .unwrap()
        .clone();
    assert_eq!(
        h.ok("run_flow_item", json!({ "kind": "job", "ref_id": job["ref_id"] })),
        "skipped: autopilot paused"
    );
    assert!(h.err("run_flow_item", json!({ "kind": "inbox", "ref_id": null })).contains("cannot be run"));

    h.ok("start_autonomous_marketing_loop", json!({}));
    assert_eq!(h.ok("is_autonomous_marketing_loop_running", json!({})), true);
    h.ok("trigger_kill_switch", json!({}));
    assert_eq!(h.ok("is_autonomous_marketing_loop_running", json!({})), false);
    assert!(h.err("start_autonomous_marketing_loop", json!({})).contains("kill switch"));
    assert_eq!(h.ok("get_system_stats", json!({}))["system_status"], "EMERGENCY_HALTED");
    h.ok("reset_kill_switch", json!({}));

    assert!(h.err("resolve_human_task", json!({ "task_id": "not-a-uuid" })).contains("Invalid task id"));
    assert!(h
        .err("agent_chat", json!({ "message": "hi", "scope": "All accounts", "history": [], "attachments": [] }))
        .contains("No language model answered"));
    assert_eq!(h.ok("list_unified_inbox", json!({})), json!([]));
    assert_eq!(h.ok("list_pending_registrations", json!({})), json!([]));
    assert_eq!(h.ok("get_last_garbage_collection", json!({})), Value::Null);
    assert_eq!(h.ok("get_stick_bridge_state", json!({}))["status"], "NOT_FOUND");
    assert!(h.err("run_garbage_collection", json!({ "apply": true })).contains("not found"));
    assert_eq!(h.ok("verify_audit_chain", json!({}))["is_intact"], true);

    assert_eq!(h.ok("delete_account", json!({ "account_id": id })), true);
    assert_eq!(h.ok("get_system_stats", json!({}))["total_accounts"].as_u64().unwrap(), seeded);

    drop(h);
    std::fs::remove_dir_all(&root).ok();
}

fn content_type(path: &std::path::Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript",
        Some("css") => "text/css",
        Some("ico") => "image/x-icon",
        Some("svg") => "image/svg+xml",
        Some("json") => "application/json",
        _ => "application/octet-stream",
    }
}

async fn serve_bridge(
    listener: tokio::net::TcpListener,
    dist: std::path::PathBuf,
    window: WebviewWindow<tauri::test::MockRuntime>,
) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    loop {
        let Ok((mut sock, _)) = listener.accept().await else { break };
        let dist = dist.clone();
        let window = window.clone();
        tokio::spawn(async move {
            let mut buf = Vec::new();
            let mut chunk = [0u8; 8192];
            let (head_end, content_len) = loop {
                let Ok(n) = sock.read(&mut chunk).await else { return };
                if n == 0 {
                    return;
                }
                buf.extend_from_slice(&chunk[..n]);
                if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&buf[..pos]).to_lowercase();
                    let len = head
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap_or(0)))
                        .unwrap_or(0);
                    break (pos + 4, len);
                }
            };
            while buf.len() < head_end + content_len {
                let Ok(n) = sock.read(&mut chunk).await else { return };
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
            }
            let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
            let mut parts = head.split_whitespace();
            let method = parts.next().unwrap_or("").to_string();
            let path = parts.next().unwrap_or("/").split('?').next().unwrap_or("/").to_string();

            let (status, ctype, body): (&str, &str, Vec<u8>) = if method == "POST" && path == "/__invoke" {
                let req: Value = serde_json::from_slice(&buf[head_end..]).unwrap_or(Value::Null);
                let cmd = req["cmd"].as_str().unwrap_or_default().to_string();
                let args = req["args"].clone();
                let result = tokio::task::spawn_blocking(move || invoke(&window, &cmd, args)).await.unwrap();
                let payload = match result {
                    Ok(v) => json!({ "ok": true, "value": v }),
                    Err(e) => json!({ "ok": false, "error": e }),
                };
                ("200 OK", "application/json", serde_json::to_vec(&payload).unwrap())
            } else {
                let rel = path.trim_start_matches('/');
                let candidate = dist.join(if rel.is_empty() { "index.html" } else { rel });
                let file = if candidate.is_file() && candidate.starts_with(&dist) { candidate } else { dist.join("index.html") };
                match std::fs::read(&file) {
                    Ok(bytes) => ("200 OK", content_type(&file), bytes),
                    Err(_) => ("404 Not Found", "text/plain", b"missing".to_vec()),
                }
            };
            let header = format!(
                "HTTP/1.1 {status}\r\ncontent-type: {ctype}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                body.len()
            );
            let _ = sock.write_all(header.as_bytes()).await;
            let _ = sock.write_all(&body).await;
        });
    }
}

#[test]
#[ignore = "drives the real Angular UI in headless Chromium; run with --ignored after `npm run build`"]
fn ui_end_to_end_in_real_browser() {
    let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dist = manifest.join("../angular/dist/angular/browser").canonicalize().expect("build the Angular app first");
    let script = manifest.join("../angular/e2e/ui.e2e.cjs");
    let root = std::env::temp_dir().join(format!("oa-ui-e2e-{}", uuid::Uuid::new_v4()));
    let (rt, h) = boot(&root);

    let listener = rt.block_on(tokio::net::TcpListener::bind("127.0.0.1:0")).unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    rt.spawn(serve_bridge(listener, dist, h.window.clone()));

    let node_path = std::process::Command::new("npm")
        .args(["root", "-g"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    let output = std::process::Command::new("node")
        .arg(&script)
        .arg(&url)
        .env("NODE_PATH", node_path)
        .output()
        .expect("node is required for the UI end-to-end test");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    drop(h);
    std::fs::remove_dir_all(&root).ok();
    println!("{stdout}");
    assert!(output.status.success(), "UI e2e failed\n{stdout}\n{stderr}");
}
