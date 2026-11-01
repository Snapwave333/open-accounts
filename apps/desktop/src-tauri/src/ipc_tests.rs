use crate::{commands::*, state::AppState};
use regex::Regex;
use serde_json::{json, Value};
use async_trait::async_trait;
use openaccounts_platform_connectors::{
    InteractionKind, PlatformAuthInfo, PlatformConnector, PublishRequest, RemoteActionReceipt, RemoteInteraction,
    RemoteMetrics,
};
use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
};
use tauri::Manager;
use tauri::{
    ipc::{CallbackFn, InvokeBody},
    test::{get_ipc_response, mock_builder, mock_context, noop_assets, INVOKE_KEY},
    webview::InvokeRequest,
    WebviewWindow, WebviewWindowBuilder,
};

const MAIN_RS: &str = include_str!("main.rs");
const UI_SERVICE: &str = include_str!("../../angular/src/app/services/tauri.service.ts");

fn which_ffmpeg() -> bool {
    std::process::Command::new("ffmpeg").arg("-version").output().map(|o| o.status.success()).unwrap_or(false)
}

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
    assert!(connections
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["kind"] == "social" && c["id"] == "youtube" && c["service"] == "YouTube"));
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

    let profile = json!({
        "id": "Local Llama",
        "name": "",
        "provider_type": "ollama",
        "endpoint": "http://127.0.0.1:9",
        "model_id": "llama3.2",
        "api_key_secret_id": null,
        "max_tokens": 256,
        "temperature": 0.2
    });
    h.ok("register_lm_profile", json!({ "profile": profile }));
    let mut bad = profile.clone();
    bad["endpoint"] = json!("ftp://nope");
    assert!(h.err("register_lm_profile", json!({ "profile": bad })).contains("http"));
    let mut builtin = profile.clone();
    builtin["id"] = json!("nim-cascade");
    assert!(h.err("register_lm_profile", json!({ "profile": builtin })).contains("Built-in"));
    assert!(h.ok("list_lm_profiles", json!({})).as_array().unwrap().iter().any(|p| p["id"] == "local-llama"));
    assert_eq!(h.ok("get_chat_lm_profile", json!({})), "nim-cascade");
    h.ok("set_chat_lm_profile", json!({ "profile_id": "local-llama" }));
    assert_eq!(h.ok("get_chat_lm_profile", json!({})), "local-llama");
    let chat_err = h.err("agent_chat", json!({ "message": "hi", "scope": "All accounts" }));
    let first = chat_err.find("local-llama:").expect("selected profile reported");
    let fallback = chat_err.find("nim-cascade:").expect("fallback profile reported");
    assert!(first < fallback, "selected profile must be tried first: {chat_err}");
    h.ok("remove_lm_profile", json!({ "profile_id": "local-llama" }));
    assert_eq!(h.ok("get_chat_lm_profile", json!({})), "nim-cascade");
    assert!(h.err("remove_lm_profile", json!({ "profile_id": "nim-cascade" })).contains("cannot be removed"));

    let act = |kind: &str, args: Value| json!({ "action": { "kind": kind, "label": kind, "args": args } });
    assert_eq!(h.ok("execute_agent_action", act("resume_autopilot", json!({}))), "Autopilot resumed.");
    assert_eq!(h.ok("is_autonomous_marketing_loop_running", json!({})), true);
    assert_eq!(h.ok("execute_agent_action", act("pause_autopilot", json!({}))), "Autopilot paused.");
    assert!(h
        .ok("execute_agent_action", act("run_job", json!({ "name": "autopilot" })))
        .as_str()
        .unwrap()
        .contains("skipped: autopilot paused"));
    assert!(h.err("execute_agent_action", act("sync_inbox", json!({ "account": "@snapwave" }))).contains("not connected"));
    assert!(h.err("execute_agent_action", act("sync_inbox", json!({ "account": "@ghost" }))).contains("No account matches"));
    assert!(h.err("execute_agent_action", act("drop_tables", json!({}))).contains("not an allowed"));

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

    let x_id = h.ok("create_account", json!({ "platform": "x", "username": "@e2e_x", "email": null }));
    let x_id = x_id.as_str().unwrap();
    let long = "a".repeat(281);
    assert!(h.err("schedule_post", json!({ "account_id": x_id, "caption": long })).contains("allows 280"));
    assert!(h.err("schedule_post", json!({ "account_id": x_id, "caption": "hi", "scheduled_at": "tomorrow" })).contains("RFC 3339"));
    let slot = h.ok(
        "schedule_post",
        json!({ "account_id": x_id, "caption": "Launch day! #rust #tauri", "scheduled_at": "2030-01-01T09:00:00Z", "media": [], "approve": false }),
    );
    assert_eq!(slot["hashtags"], json!(["#rust", "#tauri"]));
    assert_eq!(slot["target_platform"], "x");
    let slot_id = slot["slot_id"].as_str().unwrap();
    let flow = h.ok("get_activity_flow", json!({}));
    let card = flow["up_next"].as_array().unwrap().iter().find(|i| i["ref_id"] == slot_id).unwrap().clone();
    assert_eq!(card["tag"], "NEEDS APPROVAL");
    assert_eq!(card["account_handle"], "@e2e_x");
    h.ok("approve_calendar_slot", json!({ "slot_id": slot_id }));
    let calendar = h.ok("list_calendar_posts", json!({}));
    assert_eq!(calendar.as_array().unwrap().iter().find(|s| s["slot_id"] == slot_id).unwrap()["is_approved"], true);
    assert!(h.err("draft_post_caption", json!({ "account_id": x_id, "brief": "" })).contains("Describe"));
    h.ok("delete_calendar_slot", json!({ "slot_id": slot_id }));
    assert!(h.err("render_post_video", json!({ "account_id": x_id, "title": "", "script": "  " })).contains("caption"));
    let job_id = h.ok(
        "render_post_video",
        json!({ "account_id": x_id, "title": "Launch teaser", "script": "Shipping today", "duration_seconds": 1.5 }),
    );
    let job_id = job_id.as_str().unwrap().to_string();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(90);
    let job = loop {
        let job = h.ok("get_creative_job_status", json!({ "job_id": job_id }));
        if job["status"] == "Completed" || job["status"] == "Failed" || std::time::Instant::now() > deadline {
            break job;
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    };
    if which_ffmpeg() {
        assert_eq!(job["status"], "Completed", "{job}");
        let path = job["output_path"].as_str().unwrap().to_string();
        assert!(std::path::Path::new(&path).is_file());
        assert_eq!(job["validation"]["width"], 1920);
        let media_slot = h.ok("schedule_post", json!({ "account_id": x_id, "caption": "With video", "media": [path] }));
        assert_eq!(media_slot["content_type"], "Media_Post");
        let flow = h.ok("get_activity_flow", json!({}));
        assert!(flow["just_did"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["kind"] == "render" && i["title"] == "Render: Launch teaser" && i["platform"] == "x"));
        h.ok("delete_calendar_slot", json!({ "slot_id": media_slot["slot_id"] }));
    } else {
        assert_eq!(job["status"], "Failed");
    }
    assert!(h.err("delete_calendar_slot", json!({ "slot_id": slot_id })).contains("not found"));
    h.ok("delete_account", json!({ "account_id": x_id }));

    assert_eq!(h.ok("delete_account", json!({ "account_id": id })), true);
    assert_eq!(h.ok("get_system_stats", json!({}))["total_accounts"].as_u64().unwrap(), seeded);

    drop(h);
    std::fs::remove_dir_all(&root).ok();
}

#[derive(Default)]
struct StudioYouTube {
    replies: Mutex<Vec<String>>,
}

#[async_trait]
impl PlatformConnector for StudioYouTube {
    fn platform(&self) -> &str {
        "youtube"
    }
    async fn verify_auth(&self) -> openaccounts_core::Result<PlatformAuthInfo> {
        Ok(PlatformAuthInfo {
            platform: "youtube".into(),
            account_handle: Some("@ClearlyDrawnLessons".into()),
            account_remote_id: Some("UC-e2e".into()),
            scopes: vec![],
            is_authenticated: true,
            error: None,
        })
    }
    async fn like(&self, _: &str) -> openaccounts_core::Result<RemoteActionReceipt> {
        Err(openaccounts_core::OpenAccountsError::Automation("not used".into()))
    }
    async fn follow(&self, _: &str) -> openaccounts_core::Result<RemoteActionReceipt> {
        Err(openaccounts_core::OpenAccountsError::Automation("not used".into()))
    }
    async fn subscribe(&self, _: &str) -> openaccounts_core::Result<RemoteActionReceipt> {
        Err(openaccounts_core::OpenAccountsError::Automation("not used".into()))
    }
    async fn reshare(&self, _: &str, _: Option<&str>) -> openaccounts_core::Result<RemoteActionReceipt> {
        Err(openaccounts_core::OpenAccountsError::Automation("not used".into()))
    }
    async fn comment(&self, _: &str, _: &str) -> openaccounts_core::Result<RemoteActionReceipt> {
        Err(openaccounts_core::OpenAccountsError::Automation("not used".into()))
    }
    async fn reply(&self, target: &str, text: &str) -> openaccounts_core::Result<RemoteActionReceipt> {
        self.replies.lock().unwrap().push(format!("{target}|{text}"));
        Ok(RemoteActionReceipt { remote_id: format!("r-{target}"), permalink: Some(format!("https://youtube.com/c/{target}")) })
    }
    async fn publish(&self, _: PublishRequest) -> openaccounts_core::Result<RemoteActionReceipt> {
        Err(openaccounts_core::OpenAccountsError::Automation("not used".into()))
    }
    async fn fetch_inbox(
        &self,
        _since: Option<chrono::DateTime<chrono::Utc>>,
        _limit: u32,
    ) -> openaccounts_core::Result<Vec<RemoteInteraction>> {
        let now = chrono::Utc::now();
        Ok(vec![
            RemoteInteraction {
                remote_id: "c-viewer".into(),
                author_handle: "@viewer".into(),
                kind: InteractionKind::Comment,
                text: "Love this episode, amazing animation!".into(),
                permalink: Some("https://youtube.com/watch?v=e2e&lc=c-viewer".into()),
                created_at: now,
            },
            RemoteInteraction {
                remote_id: "c-legal".into(),
                author_handle: "@lawfirm".into(),
                kind: InteractionKind::Comment,
                text: "Our attorney is reviewing a copyright claim on this video".into(),
                permalink: None,
                created_at: now,
            },
        ])
    }
    async fn fetch_metrics(&self) -> openaccounts_core::Result<RemoteMetrics> {
        Ok(RemoteMetrics { followers: Some(1234), views: Some(98765), posts: Some(42), ..Default::default() })
    }
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

async fn serve_scripted_llm(listener: tokio::net::TcpListener) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    loop {
        let Ok((mut sock, _)) = listener.accept().await else { break };
        tokio::spawn(async move {
            let mut buf = vec![0u8; 65536];
            let mut read = 0;
            loop {
                let Ok(n) = sock.read(&mut buf[read..]).await else { return };
                if n == 0 {
                    break;
                }
                read += n;
                let text = String::from_utf8_lossy(&buf[..read]).to_string();
                if let Some(pos) = text.find("\r\n\r\n") {
                    let len = text[..pos]
                        .to_lowercase()
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap_or(0)))
                        .unwrap_or(0);
                    if read >= pos + 4 + len {
                        break;
                    }
                }
            }
            let content = "You have 7 accounts and autopilot is paused.\n```action\n{\"action\": \"pause_autopilot\", \"args\": {}}\n```";
            let body = json!({
                "model": "e2e-scripted-llm",
                "choices": [{ "message": { "role": "assistant", "content": content } }],
                "usage": { "total_tokens": 42 }
            })
            .to_string();
            let resp = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = sock.write_all(resp.as_bytes()).await;
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
    let youtube = Arc::new(StudioYouTube::default());
    h._app.state::<AppState>().connectors.set_connector("youtube", youtube.clone());

    let listener = rt.block_on(tokio::net::TcpListener::bind("127.0.0.1:0")).unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    rt.spawn(serve_bridge(listener, dist, h.window.clone()));
    let llm = rt.block_on(tokio::net::TcpListener::bind("127.0.0.1:0")).unwrap();
    let llm_url = format!("http://{}/v1", llm.local_addr().unwrap());
    rt.spawn(serve_scripted_llm(llm));

    let node_path = std::process::Command::new("npm")
        .args(["root", "-g"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    let output = std::process::Command::new("node")
        .arg(&script)
        .arg(&url)
        .arg(&llm_url)
        .env("NODE_PATH", node_path)
        .output()
        .expect("node is required for the UI end-to-end test");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let replies = youtube.replies.lock().unwrap().clone();
    drop(h);
    std::fs::remove_dir_all(&root).ok();
    println!("{stdout}");
    assert_eq!(replies, vec!["c-viewer|Thanks so much from the whole studio!".to_string()], "{stdout}\n{stderr}");
    assert!(output.status.success(), "UI e2e failed\n{stdout}\n{stderr}");
}
