use async_trait::async_trait;
use openaccounts_core::{OpenAccountsError, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::RwLock;
use tracing::{info, warn};

pub const NIM_URL: &str = "https://integrate.api.nvidia.com/v1/chat/completions";
pub const NIM_MODELS_URL: &str = "https://integrate.api.nvidia.com/v1/models";
pub const STICK_CASCADE_PATH: &str = r"C:\Users\chrom\Desktop\StickExplainers\insights\models\cascade.json";

pub const NIM_TEXT_BUILTIN: &[&str] = &[
    "nvidia/nemotron-3.5-super-vl-120b-a12b",
    "meta/llama-3.2-90b-vision-instruct",
    "meta/muse-glimmer-30b",
    "nvidia/nemotron-3.5-lightning-30b-a3b",
    "nvidia/nemotron-3-nano-omni-30b-a3b-reasoning",
    "openai/gpt-oss-20b",
    "minimaxai/minimax-m3.1-flash",
    "poolside/laguna-xs-2.1",
    "stepfun-ai/step-5-preview",
    "nvidia/nemotron-3-ultra-550b-a55b",
    "mistralai/mistral-large-4-1t-a52b",
    "nvidia/nemotron-3.5-bolt-e4b",
];

pub const NIM_VISION_BUILTIN: &[&str] = &[
    "nvidia/nemotron-3.5-super-vl-120b-a12b",
    "mistralai/mistral-large-4-1t-a52b",
    "nvidia/nemotron-3-nano-omni-30b-a3b-reasoning",
    "meta/muse-glimmer-30b",
    "stepfun-ai/step-5-preview",
];

const THINK_MIN_TOKENS: u32 = 4000;
const REASONING_ROOM: u32 = 2048;
const COOLDOWN_S: [u64; 3] = [120, 600, 1800];
const SLOW_S: f64 = 90.0;
const CASCADE_MAX_AGE_S: f64 = 21.0 * 86400.0;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ModelProviderType {
    Ollama,
    OpenAICompatible,
    Anthropic,
    Gemini,
    NvidiaNim,
    OpenRouter,
    OpenCodeCli,
    ClaudeCodeCli,
    CustomHttp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelProfile {
    pub id: String,
    pub name: String,
    pub provider_type: ModelProviderType,
    pub endpoint: Option<String>,
    pub model_id: String,
    pub api_key_secret_id: Option<String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest {
    pub prompt: Option<String>,
    pub messages: Vec<ChatMessage>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub stop: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse {
    pub content: String,
    pub model: String,
    pub tokens_used: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NimModelHealth {
    pub fails: usize,
    pub until_epoch_s: f64,
    pub last_ok_epoch_s: f64,
    pub last_fail_epoch_s: f64,
    pub slow_until_epoch_s: f64,
    pub why: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StickCascadeFile {
    #[serde(default)]
    pub at: f64,
    #[serde(default)]
    pub text: Vec<String>,
    #[serde(default)]
    pub vision: Vec<String>,
    #[serde(default)]
    pub dead: Vec<String>,
}

/// Strips inline `<think>...</think>` blocks and any trailing `</think>` prefix,
/// matching `StickExplainers/stickfx/llm.py::_strip_think`.
pub fn strip_think(txt: &str) -> String {
    let mut cleaned = String::with_capacity(txt.len());
    let mut rest = txt;
    while let Some(start) = rest.find("<think>") {
        cleaned.push_str(&rest[..start]);
        if let Some(end) = rest[start + 7..].find("</think>") {
            rest = &rest[start + 7 + end + 8..];
        } else {
            // Unclosed <think> tag: strip to end
            rest = "";
            break;
        }
    }
    cleaned.push_str(rest);
    if let Some((_before, after)) = cleaned.rsplit_once("</think>") {
        after.trim().to_string()
    } else {
        cleaned.trim().to_string()
    }
}

fn now_epoch_s() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// Resolves `NVIDIA_API_KEY` from process environment or Windows HKCU\Environment,
/// matching `StickExplainers/stickfx/llm.py::_env`.
pub fn resolve_nvidia_api_key() -> Option<String> {
    if let Ok(val) = std::env::var("NVIDIA_API_KEY") {
        if !val.trim().is_empty() {
            return Some(val.trim().to_string());
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(output) = std::process::Command::new("reg")
            .args(["query", r"HKCU\Environment", "/v", "NVIDIA_API_KEY"])
            .output()
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    if line.contains("NVIDIA_API_KEY") {
                        let parts: Vec<&str> = line.split_whitespace().collect();
                        if parts.len() >= 3 {
                            return Some(parts[2..].join(" "));
                        }
                    }
                }
            }
        }
    }
    None
}

/// Loads the ranked NIM model cascade from StickExplainers' `insights/models/cascade.json`,
/// falling back to `NIM_TEXT_BUILTIN` / `NIM_VISION_BUILTIN` as safety net.
pub fn load_stick_nim_cascade(cascade_path: Option<&Path>, kind: &str) -> Vec<String> {
    let builtin: Vec<String> = if kind == "vision" {
        NIM_VISION_BUILTIN.iter().map(|s| (*s).to_string()).collect()
    } else {
        NIM_TEXT_BUILTIN.iter().map(|s| (*s).to_string()).collect()
    };

    let path = cascade_path
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(STICK_CASCADE_PATH));

    let Ok(raw) = std::fs::read_to_string(&path) else {
        return builtin;
    };
    let Ok(parsed) = serde_json::from_str::<StickCascadeFile>(&raw) else {
        return builtin;
    };

    let ranked = if kind == "vision" {
        parsed.vision
    } else {
        parsed.text
    };

    let min_len = if kind == "vision" { 1 } else { 2 };
    let now = now_epoch_s();
    if ranked.len() < min_len || (parsed.at > 0.0 && (now - parsed.at) > CASCADE_MAX_AGE_S) {
        return builtin;
    }

    let dead: HashSet<String> = parsed.dead.into_iter().collect();
    let mut out = ranked.clone();
    for m in builtin {
        if !out.contains(&m) && !dead.contains(&m) {
            out.push(m);
        }
    }
    out
}

#[async_trait]
pub trait ModelAdapter: Send + Sync {
    async fn complete(&self, req: CompletionRequest, api_key: Option<String>) -> Result<CompletionResponse>;
}

pub struct NvidiaNimCascadeAdapter {
    endpoint: String,
    kind: String,
    cascade_path: Option<PathBuf>,
    client: Client,
    health: Arc<RwLock<HashMap<String, NimModelHealth>>>,
}

impl NvidiaNimCascadeAdapter {
    pub fn new(endpoint: Option<String>, kind: &str, cascade_path: Option<PathBuf>) -> Self {
        Self {
            endpoint: endpoint.unwrap_or_else(|| NIM_URL.to_string()),
            kind: kind.to_string(),
            cascade_path,
            client: Client::new(),
            health: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn current_cascade(&self) -> Vec<String> {
        load_stick_nim_cascade(self.cascade_path.as_deref(), &self.kind)
    }

    pub async fn ordered_models(&self) -> Vec<String> {
        let models = self.current_cascade();
        let health = self.health.read().await;
        let now = now_epoch_s();

        let mut up: Vec<String> = models
            .iter()
            .filter(|m| health.get(*m).map(|h| h.until_epoch_s <= now).unwrap_or(true))
            .cloned()
            .collect();

        if up.is_empty() {
            let mut all = models;
            all.sort_by(|a, b| {
                let ua = health.get(a).map(|h| h.until_epoch_s).unwrap_or(0.0);
                let ub = health.get(b).map(|h| h.until_epoch_s).unwrap_or(0.0);
                ua.partial_cmp(&ub).unwrap_or(std::cmp::Ordering::Equal)
            });
            return all;
        }

        up.sort_by_key(|m| {
            let is_slow = health
                .get(m)
                .map(|h| h.slow_until_epoch_s > now)
                .unwrap_or(false);
            let idx = models.iter().position(|x| x == m).unwrap_or(usize::MAX);
            (is_slow, idx)
        });
        up
    }

    pub async fn record_health(
        &self,
        model: &str,
        ok: bool,
        elapsed_s: f64,
        why: &str,
        retry_after_s: Option<u64>,
    ) {
        let mut health = self.health.write().await;
        let entry = health.entry(model.to_string()).or_default();
        let now = now_epoch_s();
        if ok {
            entry.fails = 0;
            entry.until_epoch_s = 0.0;
            entry.last_ok_epoch_s = now;
            entry.slow_until_epoch_s = if elapsed_s > SLOW_S { now + 1800.0 } else { 0.0 };
        } else {
            let wait = retry_after_s.unwrap_or_else(|| {
                COOLDOWN_S[entry.fails.min(COOLDOWN_S.len() - 1)]
            });
            entry.fails += 1;
            entry.until_epoch_s = now + wait as f64;
            entry.last_fail_epoch_s = now;
            entry.why = why.chars().take(80).collect();
            warn!("NIM {} skipped for {}s ({})", model, wait, entry.why);
        }
    }
}

#[async_trait]
impl ModelAdapter for NvidiaNimCascadeAdapter {
    async fn complete(&self, req: CompletionRequest, api_key: Option<String>) -> Result<CompletionResponse> {
        let key = api_key
            .or_else(resolve_nvidia_api_key)
            .ok_or_else(|| OpenAccountsError::LMGateway("NVIDIA_API_KEY not set".into()))?;

        let tokens = req.max_tokens.unwrap_or(1024).min(16000);
        let short = tokens < THINK_MIN_TOKENS;
        let ordered = self.ordered_models().await;
        let mut last_err = "no model tried".to_string();

        let url = if self.endpoint.ends_with("/chat/completions") {
            self.endpoint.clone()
        } else {
            format!("{}/chat/completions", self.endpoint.trim_end_matches('/'))
        };

        for model in ordered {
            let mut think_switch: Option<bool> = Some(!short);

            for attempt in 0..3 {
                let effective_max_tokens = tokens + if short { REASONING_ROOM } else { 0 };
                let mut body = serde_json::json!({
                    "model": model,
                    "messages": req.messages,
                    "max_tokens": effective_max_tokens,
                    "temperature": req.temperature.unwrap_or(0.6),
                });

                if model == "openai/gpt-oss-20b" && short {
                    body["reasoning_effort"] = serde_json::json!("low");
                }
                if think_switch == Some(false) {
                    body["chat_template_kwargs"] = serde_json::json!({ "enable_thinking": false });
                }

                let timeout_s = (60.0 + (tokens as f64) * 0.03).min(600.0);
                let t0 = Instant::now();

                let send_res = self
                    .client
                    .post(&url)
                    .bearer_auth(&key)
                    .timeout(Duration::from_secs_f64(timeout_s))
                    .json(&body)
                    .send()
                    .await;

                let elapsed = t0.elapsed().as_secs_f64();

                match send_res {
                    Ok(resp) => {
                        let status = resp.status();
                        let retry_after = resp
                            .headers()
                            .get("Retry-After")
                            .and_then(|v| v.to_str().ok())
                            .and_then(|s| s.parse::<f64>().ok())
                            .map(|v| (v as u64).clamp(5, 600));

                        if !status.is_success() {
                            let code = status.as_u16();
                            last_err = format!("HTTP {code}");
                            if code == 400 && think_switch == Some(false) {
                                think_switch = None;
                                continue;
                            }
                            if matches!(code, 429 | 500 | 502 | 503 | 504) {
                                self.record_health(
                                    &model,
                                    false,
                                    elapsed,
                                    &last_err,
                                    if code == 429 { Some(retry_after.unwrap_or(60)) } else { None },
                                )
                                .await;
                            }
                            break;
                        }

                        let val: serde_json::Value = match resp.json().await {
                            Ok(v) => v,
                            Err(e) => {
                                last_err = format!("JSON parse error: {e}");
                                self.record_health(&model, false, elapsed, &last_err, None).await;
                                break;
                            }
                        };

                        let choice = &val["choices"][0];
                        let raw_content = choice["message"]["content"].as_str().unwrap_or_default();
                        let txt = strip_think(raw_content);

                        if !txt.is_empty() {
                            self.record_health(&model, true, elapsed, "", None).await;
                            return Ok(CompletionResponse {
                                content: txt,
                                model: model.clone(),
                                tokens_used: val["usage"]["total_tokens"].as_u64(),
                            });
                        }

                        let reasoned = choice["message"]["reasoning_content"].is_string()
                            || choice["message"]["reasoning"].is_string();
                        let finish_length = choice["finish_reason"].as_str() == Some("length");
                        if think_switch != Some(false) && (reasoned || finish_length) {
                            last_err = "reasoning used up the reply".into();
                            think_switch = Some(false);
                            continue;
                        }

                        last_err = "empty reply".into();
                        break;
                    }
                    Err(e) => {
                        last_err = format!("Request error after {elapsed:.0}s: {e}");
                        if e.is_timeout() || attempt > 0 {
                            self.record_health(&model, false, elapsed, &last_err, None).await;
                            break;
                        }
                        tokio::time::sleep(Duration::from_secs(3)).await;
                    }
                }
            }
            info!("NIM {} failed ({}); trying next model in cascade", model, last_err);
        }

        Err(OpenAccountsError::LMGateway(format!(
            "All NVIDIA NIM cascade models failed (last: {last_err})"
        )))
    }
}

pub struct OllamaAdapter {
    endpoint: String,
    model: String,
    client: Client,
}

impl OllamaAdapter {
    pub fn new(endpoint: String, model: String) -> Self {
        Self {
            endpoint,
            model,
            client: Client::new(),
        }
    }
}

#[async_trait]
impl ModelAdapter for OllamaAdapter {
    async fn complete(&self, req: CompletionRequest, _api_key: Option<String>) -> Result<CompletionResponse> {
        let url = format!("{}/api/chat", self.endpoint.trim_end_matches('/'));
        let body = serde_json::json!({
            "model": self.model,
            "messages": req.messages,
            "stream": false,
            "options": {
                "temperature": req.temperature.unwrap_or(0.7)
            }
        });

        let res = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| OpenAccountsError::LMGateway(format!("Ollama connection error: {e}")))?;

        let val: serde_json::Value = res
            .json()
            .await
            .map_err(|e| OpenAccountsError::LMGateway(format!("Ollama response parse error: {e}")))?;

        let content = val["message"]["content"]
            .as_str()
            .unwrap_or_default()
            .to_string();

        Ok(CompletionResponse {
            content,
            model: self.model.clone(),
            tokens_used: None,
        })
    }
}

pub struct OpenAICompatibleAdapter {
    endpoint: String,
    model: String,
    client: Client,
}

impl OpenAICompatibleAdapter {
    pub fn new(endpoint: String, model: String) -> Self {
        Self {
            endpoint,
            model,
            client: Client::new(),
        }
    }
}

#[async_trait]
impl ModelAdapter for OpenAICompatibleAdapter {
    async fn complete(&self, req: CompletionRequest, api_key: Option<String>) -> Result<CompletionResponse> {
        let url = format!("{}/chat/completions", self.endpoint.trim_end_matches('/'));
        let mut builder = self.client.post(&url);

        if let Some(key) = api_key {
            builder = builder.bearer_auth(key);
        }

        let body = serde_json::json!({
            "model": self.model,
            "messages": req.messages,
            "temperature": req.temperature.unwrap_or(0.7),
            "max_tokens": req.max_tokens.unwrap_or(1024)
        });

        let res = builder
            .json(&body)
            .send()
            .await
            .map_err(|e| OpenAccountsError::LMGateway(format!("OpenAI adapter error: {e}")))?;

        let val: serde_json::Value = res
            .json()
            .await
            .map_err(|e| OpenAccountsError::LMGateway(format!("OpenAI response parse error: {e}")))?;

        let content = val["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or_default()
            .to_string();

        Ok(CompletionResponse {
            content,
            model: self.model.clone(),
            tokens_used: val["usage"]["total_tokens"].as_u64(),
        })
    }
}

pub struct LMGateway {
    profiles: Arc<RwLock<HashMap<String, ModelProfile>>>,
    adapters: Arc<RwLock<HashMap<String, Arc<dyn ModelAdapter>>>>,
    nim_cascade: Arc<NvidiaNimCascadeAdapter>,
}

impl LMGateway {
    pub fn new() -> Self {
        let nim_cascade = Arc::new(NvidiaNimCascadeAdapter::new(None, "text", None));
        let mut initial_profiles = HashMap::new();
        let mut initial_adapters: HashMap<String, Arc<dyn ModelAdapter>> = HashMap::new();

        // Pre-load the StickExplainers NVIDIA NIM Cascade as primary free LM engine
        let cascade_models = load_stick_nim_cascade(None, "text");
        let top_model = cascade_models
            .first()
            .cloned()
            .unwrap_or_else(|| "nvidia/nemotron-3.5-super-vl-120b-a12b".to_string());

        let default_nim_profile = ModelProfile {
            id: "nim-cascade".into(),
            name: format!("NVIDIA NIM Cascade ({} + {} fallbacks)", top_model, cascade_models.len().saturating_sub(1)),
            provider_type: ModelProviderType::NvidiaNim,
            endpoint: Some(NIM_URL.into()),
            model_id: top_model,
            api_key_secret_id: Some("NVIDIA_API_KEY".into()),
            max_tokens: Some(4096),
            temperature: Some(0.6),
        };

        initial_profiles.insert("nim-cascade".into(), default_nim_profile);
        initial_adapters.insert("nim-cascade".into(), nim_cascade.clone());

        let vision_models = load_stick_nim_cascade(None, "vision");
        let top_vision = vision_models
            .first()
            .cloned()
            .unwrap_or_else(|| "nvidia/nemotron-3.5-super-vl-120b-a12b".to_string());

        let vision_nim_profile = ModelProfile {
            id: "nim-vision-cascade".into(),
            name: format!("NVIDIA NIM Vision Cascade ({})", top_vision),
            provider_type: ModelProviderType::NvidiaNim,
            endpoint: Some(NIM_URL.into()),
            model_id: top_vision,
            api_key_secret_id: Some("NVIDIA_API_KEY".into()),
            max_tokens: Some(4096),
            temperature: Some(0.4),
        };

        initial_profiles.insert("nim-vision-cascade".into(), vision_nim_profile);
        initial_adapters.insert(
            "nim-vision-cascade".into(),
            Arc::new(NvidiaNimCascadeAdapter::new(None, "vision", None)),
        );

        Self {
            profiles: Arc::new(RwLock::new(initial_profiles)),
            adapters: Arc::new(RwLock::new(initial_adapters)),
            nim_cascade,
        }
    }

    pub fn get_nim_cascade_models(&self, kind: &str) -> Vec<String> {
        load_stick_nim_cascade(None, kind)
    }

    pub fn nim_cascade_adapter(&self) -> Arc<NvidiaNimCascadeAdapter> {
        self.nim_cascade.clone()
    }

    pub async fn register_profile(&self, profile: ModelProfile) {
        let id = profile.id.clone();
        let adapter: Arc<dyn ModelAdapter> = match profile.provider_type {
            ModelProviderType::NvidiaNim => {
                let kind = if profile.id.contains("vision") || profile.model_id.contains("vl") {
                    "vision"
                } else {
                    "text"
                };
                Arc::new(NvidiaNimCascadeAdapter::new(profile.endpoint.clone(), kind, None))
            }
            ModelProviderType::Ollama => {
                let endpoint = profile.endpoint.clone().unwrap_or_else(|| "http://localhost:11434".into());
                Arc::new(OllamaAdapter::new(endpoint, profile.model_id.clone()))
            }
            ModelProviderType::OpenAICompatible
            | ModelProviderType::OpenRouter
            | ModelProviderType::Gemini
            | ModelProviderType::Anthropic
            | ModelProviderType::CustomHttp => {
                let endpoint = profile.endpoint.clone().unwrap_or_else(|| "https://api.openai.com/v1".into());
                Arc::new(OpenAICompatibleAdapter::new(endpoint, profile.model_id.clone()))
            }
            ModelProviderType::OpenCodeCli | ModelProviderType::ClaudeCodeCli => {
                let endpoint = "http://localhost:8000".into();
                Arc::new(OpenAICompatibleAdapter::new(endpoint, profile.model_id.clone()))
            }
        };

        self.profiles.write().await.insert(id.clone(), profile);
        self.adapters.write().await.insert(id, adapter);
        info!("Registered LM Profile");
    }

    pub async fn generate(
        &self,
        profile_id: &str,
        request: CompletionRequest,
        api_key: Option<String>,
    ) -> Result<CompletionResponse> {
        let adapters = self.adapters.read().await;
        let adapter = adapters
            .get(profile_id)
            .cloned()
            .unwrap_or_else(|| self.nim_cascade.clone());

        adapter.complete(request, api_key).await
    }

    pub async fn list_profiles(&self) -> Vec<ModelProfile> {
        self.profiles.read().await.values().cloned().collect()
    }
}

impl Default for LMGateway {
    fn default() -> Self {
        Self::new()
    }
}
