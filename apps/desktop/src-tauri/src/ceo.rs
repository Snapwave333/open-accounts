//! The CEO chat: V. Sterling, the studio's CEO agent, reached through the
//! shared `ceo_chat.py` service in StickExplainers. The same service answers
//! the Telegram bot, so both channels share one conversation and one pending
//! action. The desktop channel streams: every JSONL line the service prints
//! is forwarded to the UI as a `ceo://event` Tauri event.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{path::PathBuf, process::Stdio, sync::Arc};
use tauri::{AppHandle, Emitter};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::Command,
    sync::Mutex,
};
use tracing::{info, warn};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// One turn at a time: the service appends to a shared file, and two
/// overlapping answers would interleave.
pub struct CeoChannel {
    studio_root: PathBuf,
    python: String,
    turn: Arc<Mutex<()>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CeoHistory {
    pub messages: Vec<Value>,
    pub pending: Value,
    pub updated_at: Option<String>,
    /// Where the conversation file lives, so the UI can say so.
    pub store: String,
    pub available: bool,
    pub error: Option<String>,
}

impl CeoChannel {
    pub fn new(studio_root: PathBuf, python: String) -> Self {
        Self {
            studio_root,
            python,
            turn: Arc::new(Mutex::new(())),
        }
    }

    fn script(&self) -> PathBuf {
        self.studio_root.join("ceo_chat.py")
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new(&self.python);
        cmd.arg("-X").arg("utf8").arg(self.script()).args(args).current_dir(&self.studio_root);
        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);
        cmd
    }

    pub fn is_available(&self) -> bool {
        self.script().is_file()
    }

    /// Runs a one-shot subcommand and parses its stdout as JSON.
    async fn run_json(&self, args: &[&str]) -> Result<Value, String> {
        if !self.is_available() {
            return Err(format!("ceo_chat.py not found in {}", self.studio_root.display()));
        }
        let output = self
            .command(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| format!("cannot launch {}: {e}", self.python))?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let last = stderr.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("no output");
            return Err(format!("ceo_chat.py {} exited with {:?}: {last}", args.join(" "), output.status.code()));
        }
        // The last non-empty line is the JSON document (warnings may precede it).
        let line = stdout.lines().rev().find(|l| l.trim().starts_with('{')).unwrap_or("");
        serde_json::from_str(line).map_err(|e| format!("ceo_chat.py returned unreadable JSON: {e}"))
    }

    pub async fn history(&self, limit: i64) -> CeoHistory {
        let store = self.studio_root.join("insights").join("ceo").join("conversation.json");
        let limit = limit.clamp(1, 400).to_string();
        match self.run_json(&["history", "--limit", &limit]).await {
            Ok(v) => CeoHistory {
                messages: v.get("messages").and_then(|m| m.as_array()).cloned().unwrap_or_default(),
                pending: v.get("pending").cloned().unwrap_or(Value::Object(Default::default())),
                updated_at: v.get("updated_at").and_then(|u| u.as_str()).map(str::to_string),
                store: store.to_string_lossy().to_string(),
                available: true,
                error: None,
            },
            Err(e) => CeoHistory {
                messages: Vec::new(),
                pending: Value::Object(Default::default()),
                updated_at: None,
                store: store.to_string_lossy().to_string(),
                available: self.is_available(),
                error: Some(e),
            },
        }
    }

    pub async fn clear(&self) -> Result<Value, String> {
        self.run_json(&["clear"]).await
    }

    pub async fn cancel(&self) -> Result<Value, String> {
        let v = self.run_json(&["cancel", "--channel", "desktop"]).await?;
        Ok(v.get("message").cloned().unwrap_or(Value::Null))
    }

    /// Streams one turn. `stdin_json` is the document the service reads
    /// (`send`), or None (`confirm`). Returns the final CEO message.
    pub async fn stream_turn(
        &self,
        app: &AppHandle,
        subcommand: &str,
        stdin_json: Option<Value>,
    ) -> Result<Value, String> {
        if !self.is_available() {
            return Err(format!("ceo_chat.py not found in {}", self.studio_root.display()));
        }
        let _turn = match self.turn.try_lock() {
            Ok(guard) => guard,
            Err(_) => return Err("The CEO is still answering the previous message".into()),
        };

        let mut child = self
            .command(&[subcommand, "--channel", "desktop"])
            .stdin(if stdin_json.is_some() { Stdio::piped() } else { Stdio::null() })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("cannot launch {}: {e}", self.python))?;

        if let Some(doc) = stdin_json {
            let mut stdin = child.stdin.take().ok_or("no stdin")?;
            let bytes = serde_json::to_vec(&doc).map_err(|e| e.to_string())?;
            stdin.write_all(&bytes).await.map_err(|e| e.to_string())?;
            stdin.shutdown().await.map_err(|e| e.to_string())?;
        }

        let stdout = child.stdout.take().ok_or("no stdout")?;
        let stderr = child.stderr.take();
        let mut lines = BufReader::new(stdout).lines();
        let mut final_message: Option<Value> = None;
        let mut error: Option<String> = None;

        while let Ok(Some(line)) = lines.next_line().await {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let event: Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(_) => {
                    // Stray prints from the studio modules are not events.
                    continue;
                }
            };
            match event.get("type").and_then(|t| t.as_str()) {
                Some("done") => final_message = event.get("message").cloned(),
                Some("error") => {
                    error = event.get("text").and_then(|t| t.as_str()).map(str::to_string)
                }
                _ => {}
            }
            if let Err(e) = app.emit("ceo://event", event) {
                warn!("cannot emit CEO event: {e}");
            }
        }

        let status = child.wait().await.map_err(|e| e.to_string())?;
        if final_message.is_none() {
            let mut stderr_text = String::new();
            if let Some(mut s) = stderr {
                let _ = tokio::io::AsyncReadExt::read_to_string(&mut s, &mut stderr_text).await;
            }
            let last = stderr_text.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("");
            let msg = error.unwrap_or_else(|| {
                format!("ceo_chat.py {subcommand} ended (exit {:?}) without an answer: {last}", status.code())
            });
            let _ = app.emit("ceo://event", serde_json::json!({ "type": "error", "text": msg }));
            return Err(msg);
        }
        info!("CEO {subcommand} turn finished");
        Ok(final_message.unwrap_or(Value::Null))
    }
}
