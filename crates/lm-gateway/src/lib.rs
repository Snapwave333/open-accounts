use async_trait::async_trait;
use openaccounts_core::{OpenAccountsError, Result};
use reqwest::{Client, Response};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

pub const NIM_URL: &str = "https://integrate.api.nvidia.com/v1/chat/completions";
pub const NIM_MODELS_URL: &str = "https://integrate.api.nvidia.com/v1/models";
pub const STICK_CASCADE_PATH: &str = r"C:\Users\chrom\Desktop\StickExplainers\insights\models\cascade.json";
pub const CASCADE_PATH_ENV: &str = "OPENACCOUNTS_NIM_CASCADE";

pub const OPENAI_DEFAULT_URL: &str = "https://api.openai.com/v1";
pub const OPENROUTER_DEFAULT_URL: &str = "https://openrouter.ai/api/v1";
pub const GEMINI_DEFAULT_URL: &str = "https://generativelanguage.googleapis.com/v1beta/openai";
pub const ANTHROPIC_DEFAULT_URL: &str = "https://api.anthropic.com";
pub const ANTHROPIC_VERSION: &str = "2023-06-01";
pub const OLLAMA_DEFAULT_URL: &str = "http://localhost:11434";
pub const LOCAL_CLI_BRIDGE_URL: &str = "http://localhost:8000";

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
const CASCADE_CACHE_TTL: Duration = Duration::from_secs(30);
const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(120);
const ERROR_BODY_PREVIEW: usize = 240;

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

impl ModelProviderType {
    pub fn default_endpoint(&self) -> &'static str {
        match self {
            ModelProviderType::Ollama => OLLAMA_DEFAULT_URL,
            ModelProviderType::OpenAICompatible | ModelProviderType::CustomHttp => OPENAI_DEFAULT_URL,
            ModelProviderType::Anthropic => ANTHROPIC_DEFAULT_URL,
            ModelProviderType::Gemini => GEMINI_DEFAULT_URL,
            ModelProviderType::NvidiaNim => NIM_URL,
            ModelProviderType::OpenRouter => OPENROUTER_DEFAULT_URL,
            ModelProviderType::OpenCodeCli | ModelProviderType::ClaudeCodeCli => LOCAL_CLI_BRIDGE_URL,
        }
    }

    pub fn default_api_key_env(&self) -> Option<&'static str> {
        match self {
            ModelProviderType::OpenAICompatible => Some("OPENAI_API_KEY"),
            ModelProviderType::Anthropic => Some("ANTHROPIC_API_KEY"),
            ModelProviderType::Gemini => Some("GEMINI_API_KEY"),
            ModelProviderType::NvidiaNim => Some("NVIDIA_API_KEY"),
            ModelProviderType::OpenRouter => Some("OPENROUTER_API_KEY"),
            _ => None,
        }
    }
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self { role: "system".into(), content: content.into() }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self { role: "user".into(), content: content.into() }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self { role: "assistant".into(), content: content.into() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CompletionRequest {
    pub prompt: Option<String>,
    pub messages: Vec<ChatMessage>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    #[serde(default)]
    pub stop: Vec<String>,
}

impl CompletionRequest {
    pub fn chat(messages: Vec<ChatMessage>) -> Self {
        Self { messages, ..Default::default() }
    }

    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }

    pub fn effective_messages(&self) -> Vec<ChatMessage> {
        let mut messages = self.messages.clone();
        if let Some(prompt) = self.prompt.as_ref().filter(|p| !p.trim().is_empty()) {
            messages.push(ChatMessage::user(prompt.clone()));
        }
        messages
    }
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

pub fn strip_think(txt: &str) -> String {
    let mut cleaned = String::with_capacity(txt.len());
    let mut rest = txt;
    while let Some(start) = rest.find("<think>") {
        cleaned.push_str(&rest[..start]);
        match rest[start + 7..].find("</think>") {
            Some(end) => rest = &rest[start + 7 + end + 8..],
            None => {
                rest = "";
                break;
            }
        }
    }
    cleaned.push_str(rest);
    match cleaned.rsplit_once("</think>") {
        Some((_, after)) => after.trim().to_string(),
        None => cleaned.trim().to_string(),
    }
}

fn now_epoch_s() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

pub fn shared_http_client() -> Client {
    static CLIENT: OnceLock<Client> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .pool_idle_timeout(Duration::from_secs(90))
                .user_agent(concat!("OpenAccounts/", env!("CARGO_PKG_VERSION")))
                .build()
                .unwrap_or_else(|_| Client::new())
        })
        .clone()
}

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

pub fn resolve_env_secret(name: &str) -> Option<String> {
    if let Some(val) = non_empty_env(name) {
        return Some(val);
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(output) = std::process::Command::new("reg")
            .args(["query", r"HKCU\Environment", "/v", name])
            .output()
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    if line.contains(name) {
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

pub fn resolve_nvidia_api_key() -> Option<String> {
    resolve_env_secret("NVIDIA_API_KEY")
}

pub fn default_cascade_path() -> PathBuf {
    non_empty_env(CASCADE_PATH_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(STICK_CASCADE_PATH))
}

fn builtin_cascade(kind: &str) -> Vec<String> {
    let src = if kind == "vision" { NIM_VISION_BUILTIN } else { NIM_TEXT_BUILTIN };
    src.iter().map(|s| (*s).to_string()).collect()
}

pub fn load_stick_nim_cascade(cascade_path: Option<&Path>, kind: &str) -> Vec<String> {
    let builtin = builtin_cascade(kind);
    let path = cascade_path.map(PathBuf::from).unwrap_or_else(default_cascade_path);

    let Ok(raw) = std::fs::read_to_string(&path) else {
        return builtin;
    };
    let Ok(parsed) = serde_json::from_str::<StickCascadeFile>(&raw) else {
        warn!("Ignoring malformed NIM cascade file at {}", path.display());
        return builtin;
    };

    let ranked = if kind == "vision" { parsed.vision } else { parsed.text };
    let min_len = if kind == "vision" { 1 } else { 2 };
    if ranked.len() < min_len || (parsed.at > 0.0 && (now_epoch_s() - parsed.at) > CASCADE_MAX_AGE_S) {
        return builtin;
    }

    let dead: HashSet<String> = parsed.dead.into_iter().collect();
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::with_capacity(ranked.len() + builtin.len());
    for m in ranked.into_iter().chain(builtin) {
        if !dead.contains(&m) && seen.insert(m.clone()) {
            out.push(m);
        }
    }
    out
}

type CascadeCacheKey = (PathBuf, String);
type CascadeCacheEntry = (Instant, Arc<Vec<String>>);

fn cascade_cache() -> &'static Mutex<HashMap<CascadeCacheKey, CascadeCacheEntry>> {
    static CACHE: OnceLock<Mutex<HashMap<CascadeCacheKey, CascadeCacheEntry>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn cached_nim_cascade(cascade_path: Option<&Path>, kind: &str) -> Arc<Vec<String>> {
    let path = cascade_path.map(PathBuf::from).unwrap_or_else(default_cascade_path);
    let key = (path.clone(), kind.to_string());
    if let Ok(cache) = cascade_cache().lock() {
        if let Some((at, models)) = cache.get(&key) {
            if at.elapsed() < CASCADE_CACHE_TTL {
                return models.clone();
            }
        }
    }
    let models = Arc::new(load_stick_nim_cascade(Some(&path), kind));
    if let Ok(mut cache) = cascade_cache().lock() {
        cache.insert(key, (Instant::now(), models.clone()));
    }
    models
}

async fn ensure_success(resp: Response, provider: &str) -> Result<Response> {
    let status = resp.status();
    if status.is_success() {
        return Ok(resp);
    }
    let body = resp.text().await.unwrap_or_default();
    let preview: String = body.chars().take(ERROR_BODY_PREVIEW).collect();
    Err(OpenAccountsError::LMGateway(format!(
        "{provider} returned HTTP {}: {}",
        status.as_u16(),
        preview.trim()
    )))
}

fn non_empty_content(content: String, provider: &str) -> Result<String> {
    if content.trim().is_empty() {
        Err(OpenAccountsError::LMGateway(format!("{provider} returned an empty reply")))
    } else {
        Ok(content)
    }
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
            client: shared_http_client(),
            health: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn current_cascade(&self) -> Vec<String> {
        cached_nim_cascade(self.cascade_path.as_deref(), &self.kind).as_ref().clone()
    }

    pub async fn health_snapshot(&self) -> HashMap<String, NimModelHealth> {
        self.health.read().await.clone()
    }

    pub async fn ordered_models(&self) -> Vec<String> {
        let models = cached_nim_cascade(self.cascade_path.as_deref(), &self.kind);
        let health = self.health.read().await;
        let now = now_epoch_s();

        let mut up: Vec<(bool, usize, &String)> = models
            .iter()
            .enumerate()
            .filter(|(_, m)| health.get(*m).map(|h| h.until_epoch_s <= now).unwrap_or(true))
            .map(|(idx, m)| {
                let slow = health.get(m).map(|h| h.slow_until_epoch_s > now).unwrap_or(false);
                (slow, idx, m)
            })
            .collect();

        if up.is_empty() {
            let mut all: Vec<String> = models.as_ref().clone();
            all.sort_by(|a, b| {
                let ua = health.get(a).map(|h| h.until_epoch_s).unwrap_or(0.0);
                let ub = health.get(b).map(|h| h.until_epoch_s).unwrap_or(0.0);
                ua.total_cmp(&ub)
            });
            return all;
        }

        up.sort_by_key(|(slow, idx, _)| (*slow, *idx));
        up.into_iter().map(|(_, _, m)| m.clone()).collect()
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
            let wait = retry_after_s.unwrap_or_else(|| COOLDOWN_S[entry.fails.min(COOLDOWN_S.len() - 1)]);
            entry.fails += 1;
            entry.until_epoch_s = now + wait as f64;
            entry.last_fail_epoch_s = now;
            entry.why = why.chars().take(80).collect();
            warn!("NIM {} skipped for {}s ({})", model, wait, entry.why);
        }
    }

    fn chat_url(&self) -> String {
        if self.endpoint.ends_with("/chat/completions") {
            self.endpoint.clone()
        } else {
            format!("{}/chat/completions", self.endpoint.trim_end_matches('/'))
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
        let messages = req.effective_messages();
        let ordered = self.ordered_models().await;
        let url = self.chat_url();
        let mut last_err = "no model tried".to_string();

        for model in ordered {
            let mut think_switch: Option<bool> = Some(!short);

            for attempt in 0..3 {
                let effective_max_tokens = tokens + if short { REASONING_ROOM } else { 0 };
                let mut body = serde_json::json!({
                    "model": model,
                    "messages": messages,
                    "max_tokens": effective_max_tokens,
                    "temperature": req.temperature.unwrap_or(0.6),
                });
                if !req.stop.is_empty() {
                    body["stop"] = serde_json::json!(req.stop);
                }
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
                            if matches!(code, 401 | 403) {
                                return Err(OpenAccountsError::LMGateway(format!(
                                    "NVIDIA NIM rejected the API key (HTTP {code})"
                                )));
                            }
                            if matches!(code, 404 | 429 | 500 | 502 | 503 | 504) {
                                let retry = if code == 429 { Some(retry_after.unwrap_or(60)) } else { None };
                                self.record_health(&model, false, elapsed, &last_err, retry).await;
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
                        let txt = strip_think(choice["message"]["content"].as_str().unwrap_or_default());

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
        Self { endpoint, model, client: shared_http_client() }
    }
}

#[async_trait]
impl ModelAdapter for OllamaAdapter {
    async fn complete(&self, req: CompletionRequest, _api_key: Option<String>) -> Result<CompletionResponse> {
        let url = format!("{}/api/chat", self.endpoint.trim_end_matches('/'));
        let mut options = serde_json::json!({ "temperature": req.temperature.unwrap_or(0.7) });
        if let Some(max) = req.max_tokens {
            options["num_predict"] = serde_json::json!(max);
        }
        if !req.stop.is_empty() {
            options["stop"] = serde_json::json!(req.stop);
        }
        let body = serde_json::json!({
            "model": self.model,
            "messages": req.effective_messages(),
            "stream": false,
            "options": options,
        });

        let res = self
            .client
            .post(&url)
            .timeout(DEFAULT_REQUEST_TIMEOUT)
            .json(&body)
            .send()
            .await
            .map_err(|e| OpenAccountsError::LMGateway(format!("Ollama connection error: {e}")))?;
        let val: serde_json::Value = ensure_success(res, "Ollama")
            .await?
            .json()
            .await
            .map_err(|e| OpenAccountsError::LMGateway(format!("Ollama response parse error: {e}")))?;

        let content = strip_think(val["message"]["content"].as_str().unwrap_or_default());
        let tokens_used = match (val["prompt_eval_count"].as_u64(), val["eval_count"].as_u64()) {
            (None, None) => None,
            (a, b) => Some(a.unwrap_or(0) + b.unwrap_or(0)),
        };

        Ok(CompletionResponse {
            content: non_empty_content(content, "Ollama")?,
            model: self.model.clone(),
            tokens_used,
        })
    }
}

pub struct OpenAICompatibleAdapter {
    endpoint: String,
    model: String,
    client: Client,
    provider_label: String,
}

impl OpenAICompatibleAdapter {
    pub fn new(endpoint: String, model: String) -> Self {
        Self::with_label(endpoint, model, "OpenAI-compatible")
    }

    pub fn with_label(endpoint: String, model: String, label: &str) -> Self {
        Self {
            endpoint,
            model,
            client: shared_http_client(),
            provider_label: label.to_string(),
        }
    }
}

#[async_trait]
impl ModelAdapter for OpenAICompatibleAdapter {
    async fn complete(&self, req: CompletionRequest, api_key: Option<String>) -> Result<CompletionResponse> {
        let url = format!("{}/chat/completions", self.endpoint.trim_end_matches('/'));
        let mut builder = self.client.post(&url).timeout(DEFAULT_REQUEST_TIMEOUT);
        if let Some(key) = api_key {
            builder = builder.bearer_auth(key);
        }

        let mut body = serde_json::json!({
            "model": self.model,
            "messages": req.effective_messages(),
            "temperature": req.temperature.unwrap_or(0.7),
            "max_tokens": req.max_tokens.unwrap_or(1024),
        });
        if !req.stop.is_empty() {
            body["stop"] = serde_json::json!(req.stop);
        }

        let label = self.provider_label.as_str();
        let res = builder
            .json(&body)
            .send()
            .await
            .map_err(|e| OpenAccountsError::LMGateway(format!("{label} connection error: {e}")))?;
        let val: serde_json::Value = ensure_success(res, label)
            .await?
            .json()
            .await
            .map_err(|e| OpenAccountsError::LMGateway(format!("{label} response parse error: {e}")))?;

        let content = strip_think(val["choices"][0]["message"]["content"].as_str().unwrap_or_default());
        Ok(CompletionResponse {
            content: non_empty_content(content, label)?,
            model: val["model"].as_str().unwrap_or(&self.model).to_string(),
            tokens_used: val["usage"]["total_tokens"].as_u64(),
        })
    }
}

pub struct AnthropicAdapter {
    endpoint: String,
    model: String,
    client: Client,
}

impl AnthropicAdapter {
    pub fn new(endpoint: String, model: String) -> Self {
        Self { endpoint, model, client: shared_http_client() }
    }

    fn messages_url(&self) -> String {
        let base = self.endpoint.trim_end_matches('/');
        if base.ends_with("/v1/messages") {
            base.to_string()
        } else if base.ends_with("/v1") {
            format!("{base}/messages")
        } else {
            format!("{base}/v1/messages")
        }
    }

    pub fn build_body(&self, req: &CompletionRequest) -> serde_json::Value {
        let mut system_parts: Vec<String> = Vec::new();
        let mut turns: Vec<serde_json::Value> = Vec::new();
        for msg in req.effective_messages() {
            match msg.role.as_str() {
                "system" => system_parts.push(msg.content),
                role => {
                    let role = if role == "assistant" { "assistant" } else { "user" };
                    if let Some(last) = turns.last_mut() {
                        if last["role"] == role {
                            let merged = format!("{}\n\n{}", last["content"].as_str().unwrap_or_default(), msg.content);
                            last["content"] = serde_json::json!(merged);
                            continue;
                        }
                    }
                    turns.push(serde_json::json!({ "role": role, "content": msg.content }));
                }
            }
        }

        let mut body = serde_json::json!({
            "model": self.model,
            "max_tokens": req.max_tokens.unwrap_or(1024),
            "messages": turns,
        });
        if !system_parts.is_empty() {
            body["system"] = serde_json::json!(system_parts.join("\n\n"));
        }
        if let Some(t) = req.temperature {
            body["temperature"] = serde_json::json!(t.clamp(0.0, 1.0));
        }
        if !req.stop.is_empty() {
            body["stop_sequences"] = serde_json::json!(req.stop);
        }
        body
    }
}

#[async_trait]
impl ModelAdapter for AnthropicAdapter {
    async fn complete(&self, req: CompletionRequest, api_key: Option<String>) -> Result<CompletionResponse> {
        let key = api_key
            .or_else(|| resolve_env_secret("ANTHROPIC_API_KEY"))
            .ok_or_else(|| OpenAccountsError::LMGateway("ANTHROPIC_API_KEY not set".into()))?;

        let res = self
            .client
            .post(self.messages_url())
            .timeout(DEFAULT_REQUEST_TIMEOUT)
            .header("x-api-key", key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .json(&self.build_body(&req))
            .send()
            .await
            .map_err(|e| OpenAccountsError::LMGateway(format!("Anthropic connection error: {e}")))?;
        let val: serde_json::Value = ensure_success(res, "Anthropic")
            .await?
            .json()
            .await
            .map_err(|e| OpenAccountsError::LMGateway(format!("Anthropic response parse error: {e}")))?;

        let content: String = val["content"]
            .as_array()
            .map(|blocks| {
                blocks
                    .iter()
                    .filter(|b| b["type"] == "text")
                    .filter_map(|b| b["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("")
            })
            .unwrap_or_default();
        let usage = &val["usage"];
        let tokens_used = match (usage["input_tokens"].as_u64(), usage["output_tokens"].as_u64()) {
            (None, None) => None,
            (a, b) => Some(a.unwrap_or(0) + b.unwrap_or(0)),
        };

        Ok(CompletionResponse {
            content: non_empty_content(content.trim().to_string(), "Anthropic")?,
            model: val["model"].as_str().unwrap_or(&self.model).to_string(),
            tokens_used,
        })
    }
}

pub fn build_adapter(profile: &ModelProfile) -> Arc<dyn ModelAdapter> {
    let endpoint = profile
        .endpoint
        .clone()
        .filter(|e| !e.trim().is_empty())
        .unwrap_or_else(|| profile.provider_type.default_endpoint().to_string());
    let model = profile.model_id.clone();
    match profile.provider_type {
        ModelProviderType::NvidiaNim => {
            let kind = if profile.id.contains("vision") || profile.model_id.contains("vl") {
                "vision"
            } else {
                "text"
            };
            Arc::new(NvidiaNimCascadeAdapter::new(Some(endpoint), kind, None))
        }
        ModelProviderType::Ollama => Arc::new(OllamaAdapter::new(endpoint, model)),
        ModelProviderType::Anthropic => Arc::new(AnthropicAdapter::new(endpoint, model)),
        ModelProviderType::Gemini => Arc::new(OpenAICompatibleAdapter::with_label(endpoint, model, "Gemini")),
        ModelProviderType::OpenRouter => Arc::new(OpenAICompatibleAdapter::with_label(endpoint, model, "OpenRouter")),
        ModelProviderType::OpenAICompatible
        | ModelProviderType::CustomHttp
        | ModelProviderType::OpenCodeCli
        | ModelProviderType::ClaudeCodeCli => Arc::new(OpenAICompatibleAdapter::new(endpoint, model)),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProfileUsage {
    pub requests: u64,
    pub failures: u64,
    pub tokens_used: u64,
    pub total_latency_ms: u64,
    pub last_error: Option<String>,
}

impl ProfileUsage {
    pub fn average_latency_ms(&self) -> u64 {
        let ok = self.requests.saturating_sub(self.failures);
        self.total_latency_ms.checked_div(ok).unwrap_or(0)
    }
}

pub const DEFAULT_PROFILE_ID: &str = "nim-cascade";

pub struct LMGateway {
    profiles: Arc<RwLock<HashMap<String, ModelProfile>>>,
    adapters: Arc<RwLock<HashMap<String, Arc<dyn ModelAdapter>>>>,
    usage: Arc<RwLock<HashMap<String, ProfileUsage>>>,
    nim_cascade: Arc<NvidiaNimCascadeAdapter>,
}

impl LMGateway {
    pub fn new() -> Self {
        let nim_cascade = Arc::new(NvidiaNimCascadeAdapter::new(None, "text", None));
        let mut initial_profiles = HashMap::new();
        let mut initial_adapters: HashMap<String, Arc<dyn ModelAdapter>> = HashMap::new();

        let cascade_models = cached_nim_cascade(None, "text");
        let top_model = cascade_models
            .first()
            .cloned()
            .unwrap_or_else(|| NIM_TEXT_BUILTIN[0].to_string());

        initial_profiles.insert(
            DEFAULT_PROFILE_ID.to_string(),
            ModelProfile {
                id: DEFAULT_PROFILE_ID.into(),
                name: format!(
                    "NVIDIA NIM Cascade ({} + {} fallbacks)",
                    top_model,
                    cascade_models.len().saturating_sub(1)
                ),
                provider_type: ModelProviderType::NvidiaNim,
                endpoint: Some(NIM_URL.into()),
                model_id: top_model,
                api_key_secret_id: Some("NVIDIA_API_KEY".into()),
                max_tokens: Some(4096),
                temperature: Some(0.6),
            },
        );
        initial_adapters.insert(DEFAULT_PROFILE_ID.into(), nim_cascade.clone());

        let top_vision = cached_nim_cascade(None, "vision")
            .first()
            .cloned()
            .unwrap_or_else(|| NIM_VISION_BUILTIN[0].to_string());
        initial_profiles.insert(
            "nim-vision-cascade".into(),
            ModelProfile {
                id: "nim-vision-cascade".into(),
                name: format!("NVIDIA NIM Vision Cascade ({})", top_vision),
                provider_type: ModelProviderType::NvidiaNim,
                endpoint: Some(NIM_URL.into()),
                model_id: top_vision,
                api_key_secret_id: Some("NVIDIA_API_KEY".into()),
                max_tokens: Some(4096),
                temperature: Some(0.4),
            },
        );
        initial_adapters.insert(
            "nim-vision-cascade".into(),
            Arc::new(NvidiaNimCascadeAdapter::new(None, "vision", None)),
        );

        Self {
            profiles: Arc::new(RwLock::new(initial_profiles)),
            adapters: Arc::new(RwLock::new(initial_adapters)),
            usage: Arc::new(RwLock::new(HashMap::new())),
            nim_cascade,
        }
    }

    pub fn get_nim_cascade_models(&self, kind: &str) -> Vec<String> {
        cached_nim_cascade(None, kind).as_ref().clone()
    }

    pub fn nim_cascade_adapter(&self) -> Arc<NvidiaNimCascadeAdapter> {
        self.nim_cascade.clone()
    }

    pub async fn register_profile(&self, profile: ModelProfile) {
        let adapter = build_adapter(&profile);
        self.register_profile_with_adapter(profile, adapter).await;
    }

    pub async fn register_profile_with_adapter(&self, profile: ModelProfile, adapter: Arc<dyn ModelAdapter>) {
        let id = profile.id.clone();
        info!("Registered LM profile '{}' ({:?} · {})", id, profile.provider_type, profile.model_id);
        self.profiles.write().await.insert(id.clone(), profile);
        self.adapters.write().await.insert(id, adapter);
    }

    pub async fn remove_profile(&self, profile_id: &str) -> bool {
        if profile_id == DEFAULT_PROFILE_ID {
            return false;
        }
        let removed = self.profiles.write().await.remove(profile_id).is_some();
        self.adapters.write().await.remove(profile_id);
        removed
    }

    pub async fn has_profile(&self, profile_id: &str) -> bool {
        self.adapters.read().await.contains_key(profile_id)
    }

    async fn resolve(&self, profile_id: &str) -> (String, Arc<dyn ModelAdapter>, Option<ModelProfile>) {
        let adapter = self.adapters.read().await.get(profile_id).cloned();
        let profile = self.profiles.read().await.get(profile_id).cloned();
        match adapter {
            Some(a) => (profile_id.to_string(), a, profile),
            None => {
                warn!("LM profile '{}' not registered; routing to '{}'", profile_id, DEFAULT_PROFILE_ID);
                let fallback = self.profiles.read().await.get(DEFAULT_PROFILE_ID).cloned();
                (DEFAULT_PROFILE_ID.to_string(), self.nim_cascade.clone(), fallback)
            }
        }
    }

    pub async fn generate(
        &self,
        profile_id: &str,
        mut request: CompletionRequest,
        api_key: Option<String>,
    ) -> Result<CompletionResponse> {
        let (resolved_id, adapter, profile) = self.resolve(profile_id).await;

        let api_key = api_key.or_else(|| {
            let profile = profile.as_ref()?;
            profile
                .api_key_secret_id
                .as_deref()
                .and_then(resolve_env_secret)
                .or_else(|| profile.provider_type.default_api_key_env().and_then(resolve_env_secret))
        });
        if let Some(profile) = profile.as_ref() {
            if request.max_tokens.is_none() {
                request.max_tokens = profile.max_tokens;
            }
            if request.temperature.is_none() {
                request.temperature = profile.temperature;
            }
        }

        let started = Instant::now();
        let result = adapter.complete(request, api_key).await;
        let latency_ms = started.elapsed().as_millis() as u64;

        let mut usage = self.usage.write().await;
        let entry = usage.entry(resolved_id.clone()).or_default();
        entry.requests += 1;
        match &result {
            Ok(resp) => {
                entry.total_latency_ms += latency_ms;
                entry.tokens_used += resp.tokens_used.unwrap_or(0);
                debug!("LM profile '{}' answered via {} in {}ms", resolved_id, resp.model, latency_ms);
            }
            Err(e) => {
                entry.failures += 1;
                entry.last_error = Some(e.to_string());
            }
        }
        result
    }

    pub async fn generate_with_fallback(
        &self,
        profile_ids: &[&str],
        request: CompletionRequest,
    ) -> Result<CompletionResponse> {
        let mut last_err = OpenAccountsError::LMGateway("no LM profiles supplied".into());
        for id in profile_ids {
            match self.generate(id, request.clone(), None).await {
                Ok(resp) => return Ok(resp),
                Err(e) => {
                    warn!("LM profile '{}' failed: {}", id, e);
                    last_err = e;
                }
            }
        }
        Err(last_err)
    }

    pub async fn quick_complete(
        &self,
        profile_id: &str,
        system: &str,
        user: &str,
        max_tokens: u32,
        timeout: Duration,
    ) -> Result<String> {
        let request = CompletionRequest::chat(vec![ChatMessage::system(system), ChatMessage::user(user)])
            .with_max_tokens(max_tokens);
        match tokio::time::timeout(timeout, self.generate(profile_id, request, None)).await {
            Ok(res) => res.map(|r| r.content),
            Err(_) => Err(OpenAccountsError::LMGateway(format!(
                "LM profile '{profile_id}' timed out after {}s",
                timeout.as_secs()
            ))),
        }
    }

    pub async fn list_profiles(&self) -> Vec<ModelProfile> {
        let mut profiles: Vec<ModelProfile> = self.profiles.read().await.values().cloned().collect();
        profiles.sort_by(|a, b| a.id.cmp(&b.id));
        profiles
    }

    pub async fn usage_report(&self) -> HashMap<String, ProfileUsage> {
        self.usage.read().await.clone()
    }
}

impl Default for LMGateway {
    fn default() -> Self {
        Self::new()
    }
}
