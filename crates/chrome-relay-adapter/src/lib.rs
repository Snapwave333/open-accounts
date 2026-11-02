//! Real Chrome relay adapter.
//!
//! Every operation is executed against the live OpenCode Chrome Relay CLI
//! (`oc-relay`), which drives the operator's actual signed-in Chrome session.
//! Nothing is simulated: if the broker is offline or the relay rejects an
//! action, the adapter returns an error instead of pretending it succeeded.

use async_trait::async_trait;
use openaccounts_browser_abstraction::{BrowserProvider, ChallengeDetection, PageState};
use openaccounts_core::{OpenAccountsError, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    path::PathBuf,
    sync::Arc,
    time::Duration,
};
use tokio::{process::Command, sync::RwLock};
use tracing::{info, warn};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const DEFAULT_TIMEOUT_SECS: u64 = 60;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChromeRelayConfig {
    pub endpoint: String,
    pub tab_lease_id: Option<String>,
    pub origin_fence: Vec<String>,
}

impl Default for ChromeRelayConfig {
    fn default() -> Self {
        Self {
            endpoint: "ws://127.0.0.1:4098".into(),
            tab_lease_id: None,
            origin_fence: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelaySession {
    pub name: String,
    pub run_id: String,
    pub state: String,
    pub tab_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChromeRelayStatus {
    pub online: bool,
    pub broker_url: String,
    pub sessions: Vec<RelaySession>,
    pub error: Option<String>,
}

pub struct ChromeRelayAdapter {
    pub config: ChromeRelayConfig,
    current_state: Arc<RwLock<PageState>>,
}

impl ChromeRelayAdapter {
    pub fn new(config: ChromeRelayConfig) -> Self {
        Self {
            config,
            current_state: Arc::new(RwLock::new(PageState {
                url: "about:blank".into(),
                title: String::new(),
                text_content: String::new(),
                is_ready: false,
                pending_challenge: None,
            })),
        }
    }

    fn binary() -> String {
        std::env::var("OC_RELAY_BIN").unwrap_or_else(|_| "oc-relay".to_string())
    }

    async fn run_json(&self, args: &[&str]) -> Result<Value> {
        let mut command = Command::new(Self::binary());
        command
            .args(args)
            .arg("--json")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        if let Some(tab) = &self.config.tab_lease_id {
            command.arg("--tab").arg(tab);
        }
        #[cfg(windows)]
        command.creation_flags(CREATE_NO_WINDOW);

        let output = tokio::time::timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS), command.output())
            .await
            .map_err(|_| {
                OpenAccountsError::Browser(format!(
                    "Chrome relay timed out after {DEFAULT_TIMEOUT_SECS}s running {:?}",
                    args
                ))
            })?
            .map_err(|e| {
                OpenAccountsError::Browser(format!(
                    "Chrome relay CLI could not be launched ({}): {e}. Is `oc-relay` installed and the broker running?",
                    Self::binary()
                ))
            })?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        let parsed: Option<Value> = stdout
            .lines()
            .rev()
            .find_map(|line| serde_json::from_str::<Value>(line.trim()).ok());

        match parsed {
            Some(value) => {
                let success = value
                    .get("success")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(output.status.success());
                if !success {
                    let reason = value
                        .get("error")
                        .and_then(|v| v.as_str())
                        .or_else(|| value.get("outcome_summary").and_then(|v| v.as_str()))
                        .unwrap_or("relay reported failure");
                    return Err(OpenAccountsError::Browser(format!(
                        "Chrome relay rejected {:?}: {reason}",
                        args
                    )));
                }
                Ok(value)
            }
            None => Err(OpenAccountsError::Browser(format!(
                "Chrome relay returned no JSON for {:?} (exit {:?}): {}",
                args,
                output.status.code(),
                first_line(&stderr).unwrap_or_else(|| first_line(&stdout).unwrap_or_default())
            ))),
        }
    }

    /// Live broker health, straight from `oc-relay status`.
    pub async fn probe(&self) -> ChromeRelayStatus {
        match self.run_json(&["status"]).await {
            Ok(value) => {
                let sessions = value
                    .get("active_sessions")
                    .and_then(|v| v.as_array())
                    .map(|items| {
                        items
                            .iter()
                            .map(|s| RelaySession {
                                name: s
                                    .get("name")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or_default()
                                    .to_string(),
                                run_id: s
                                    .get("run_id")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or_default()
                                    .to_string(),
                                state: s
                                    .get("state")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or_default()
                                    .to_string(),
                                tab_id: s.get("tab_id").and_then(|v| v.as_i64()),
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                ChromeRelayStatus {
                    online: true,
                    broker_url: value
                        .get("broker_url")
                        .and_then(|v| v.as_str())
                        .unwrap_or(&self.config.endpoint)
                        .to_string(),
                    sessions,
                    error: None,
                }
            }
            Err(e) => ChromeRelayStatus {
                online: false,
                broker_url: self.config.endpoint.clone(),
                sessions: Vec::new(),
                error: Some(e.to_string()),
            },
        }
    }

    fn enforce_origin_fence(&self, url: &str) -> Result<()> {
        if self.config.origin_fence.is_empty() {
            return Ok(());
        }
        let permitted = self
            .config
            .origin_fence
            .iter()
            .any(|host| url.contains(host.trim_start_matches("*.")));
        if permitted {
            Ok(())
        } else {
            Err(OpenAccountsError::PermissionDenied(format!(
                "Origin fence {:?} does not permit {url}",
                self.config.origin_fence
            )))
        }
    }
}

#[async_trait]
impl BrowserProvider for ChromeRelayAdapter {
    async fn navigate(&self, url: &str) -> Result<()> {
        self.enforce_origin_fence(url)?;
        info!("Chrome relay navigating to {url}");
        self.run_json(&["nav", url]).await?;

        let mut state = self.current_state.write().await;
        state.url = url.to_string();
        state.is_ready = true;
        state.pending_challenge = None;
        Ok(())
    }

    async fn click(&self, selector: &str) -> Result<()> {
        info!("Chrome relay clicking {selector}");
        self.run_json(&["click", selector]).await?;
        Ok(())
    }

    async fn type_text(&self, selector: &str, text: &str, is_secret: bool) -> Result<()> {
        if is_secret {
            info!("Chrome relay typing secret into {selector}");
        } else {
            info!("Chrome relay typing {text:?} into {selector}");
        }
        self.run_json(&["type", selector, text]).await?;
        Ok(())
    }

    async fn get_page_state(&self) -> Result<PageState> {
        let value = self.run_json(&["snapshot"]).await?;
        let snapshot = value.get("snapshot").cloned().unwrap_or(Value::Null);

        let url = snapshot
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or("about:blank")
            .to_string();
        let title = snapshot
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();

        let labels: Vec<String> = snapshot
            .get("elements")
            .and_then(|v| v.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|e| e.get("label").and_then(|v| v.as_str()))
                    .filter(|l| !l.trim().is_empty())
                    .map(|l| l.to_string())
                    .collect()
            })
            .unwrap_or_default();

        let pending_challenge = detect_challenge(&labels, &title);

        let state = PageState {
            url,
            title,
            text_content: labels.join("\n"),
            is_ready: true,
            pending_challenge,
        };
        *self.current_state.write().await = state.clone();
        Ok(state)
    }

    async fn take_screenshot(&self) -> Result<Vec<u8>> {
        let path = std::env::temp_dir().join(format!(
            "openaccounts-shot-{}.png",
            uuid::Uuid::new_v4()
        ));
        let path_str = path.to_string_lossy().to_string();
        self.run_json(&["screenshot", &path_str]).await?;

        let bytes = tokio::fs::read(&path).await.map_err(|e| {
            OpenAccountsError::Browser(format!("Relay reported a screenshot but {path_str} is unreadable: {e}"))
        })?;
        let _ = tokio::fs::remove_file(&path).await;
        Ok(bytes)
    }

    async fn evaluate_script(&self, script: &str) -> Result<Value> {
        let value = self.run_json(&["eval", script]).await?;
        let raw = value
            .get("eval_result")
            .or_else(|| value.pointer("/outcome/eval_result"))
            .cloned()
            .unwrap_or(Value::Null);

        // The relay returns JS results as strings; surface JSON when it is JSON.
        Ok(match raw {
            Value::String(s) => serde_json::from_str(&s).unwrap_or(Value::String(s)),
            other => other,
        })
    }

    async fn pause(&self) -> Result<()> {
        info!("Chrome relay: pausing autonomous runs for human takeover");
        self.run_json(&["control", "pause"]).await?;
        Ok(())
    }

    async fn resume(&self) -> Result<()> {
        info!("Chrome relay: resuming autonomous runs");
        self.run_json(&["control", "restart"]).await?;
        Ok(())
    }

    async fn close(&self) -> Result<()> {
        match self.run_json(&["close-tab"]).await {
            Ok(_) => Ok(()),
            Err(e) => {
                warn!("Chrome relay close-tab failed: {e}");
                Err(e)
            }
        }
    }
}

fn first_line(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(|l| l.chars().take(200).collect())
}

fn detect_challenge(labels: &[String], title: &str) -> Option<ChallengeDetection> {
    let haystack = format!("{} {}", title, labels.join(" ")).to_lowercase();

    let rules: [(&str, &str, &str); 4] = [
        (
            "captcha",
            "CAPTCHA challenge detected on the page",
            "recaptcha|hcaptcha|turnstile|verify you are human|captcha",
        ),
        (
            "twofactor",
            "Two-factor authentication prompt detected",
            "two-factor|2fa|verification code|authenticator",
        ),
        (
            "legal",
            "Terms or consent acceptance required",
            "accept all cookies|terms of service|agree and continue|consent",
        ),
        (
            "age",
            "Age verification required",
            "date of birth|age verification|birthday",
        ),
    ];

    for (kind, description, needle) in rules {
        if needle.split('|').any(|n| haystack.contains(n)) {
            let challenge_type = match kind {
                "captcha" => openaccounts_browser_abstraction::ChallengeType::Captcha,
                "twofactor" => openaccounts_browser_abstraction::ChallengeType::TwoFactorCode,
                "legal" => openaccounts_browser_abstraction::ChallengeType::LegalConsent,
                _ => openaccounts_browser_abstraction::ChallengeType::AgeVerification,
            };
            return Some(ChallengeDetection {
                challenge_type,
                description: description.into(),
                element_hint: labels.iter().find(|l| !l.trim().is_empty()).cloned(),
            });
        }
    }
    None
}

/// Convenience for callers that only need the binary path for diagnostics.
pub fn relay_binary_path() -> PathBuf {
    PathBuf::from(ChromeRelayAdapter::binary())
}
