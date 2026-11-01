mod instagram;
mod reddit;
mod x;
mod youtube;

pub use instagram::{InstagramConnector, INSTAGRAM_API_BASE};
pub use reddit::{reddit_fullname, split_subreddit_title, RedditConnector, REDDIT_API_BASE, REDDIT_DEFAULT_USER_AGENT};
pub use x::{XConnector, X_API_BASE};
pub use youtube::{YouTubeConnector, YOUTUBE_API_BASE, YOUTUBE_UPLOAD_BASE};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use openaccounts_core::{OpenAccountsError, Result};
use reqwest::{Client, Response};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{Arc, OnceLock, RwLock as StdRwLock},
    time::Duration,
};
use tokio::sync::RwLock;

pub const SUPPORTED_PLATFORMS: &[&str] = &["x", "youtube", "reddit", "instagram"];
pub(crate) const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const ERROR_PREVIEW_CHARS: usize = 300;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct PlatformCredentials {
    pub access_token: String,
    pub user_id: Option<String>,
    #[serde(default)]
    pub extra: HashMap<String, String>,
}

impl PlatformCredentials {
    pub fn new(access_token: impl Into<String>) -> Self {
        Self { access_token: access_token.into(), ..Default::default() }
    }

    pub fn with_user_id(mut self, user_id: impl Into<String>) -> Self {
        self.user_id = Some(user_id.into());
        self
    }

    pub fn with_extra(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.extra.insert(key.into(), value.into());
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlatformAuthInfo {
    pub platform: String,
    pub account_handle: Option<String>,
    pub account_remote_id: Option<String>,
    pub scopes: Vec<String>,
    pub is_authenticated: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteActionReceipt {
    pub remote_id: String,
    pub permalink: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct PublishRequest {
    pub caption: String,
    #[serde(default)]
    pub hashtags: Vec<String>,
    #[serde(default)]
    pub media: Vec<String>,
    pub title: Option<String>,
}

impl PublishRequest {
    pub fn full_text(&self) -> String {
        let tags: Vec<String> = self
            .hashtags
            .iter()
            .map(|h| h.trim())
            .filter(|h| !h.is_empty())
            .map(|h| if h.starts_with('#') { h.to_string() } else { format!("#{h}") })
            .collect();
        if tags.is_empty() {
            self.caption.trim().to_string()
        } else if self.caption.trim().is_empty() {
            tags.join(" ")
        } else {
            format!("{}\n\n{}", self.caption.trim(), tags.join(" "))
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InteractionKind {
    Comment,
    Mention,
    DirectMessage,
    Reply,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RemoteInteraction {
    pub remote_id: String,
    pub author_handle: String,
    pub kind: InteractionKind,
    pub text: String,
    pub permalink: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct RemoteMetrics {
    pub followers: Option<u64>,
    pub following: Option<u64>,
    pub posts: Option<u64>,
    pub views: Option<u64>,
    pub likes: Option<u64>,
    pub comments: Option<u64>,
    pub shares: Option<u64>,
    pub impressions: Option<u64>,
    pub reach: Option<u64>,
    #[serde(default)]
    pub raw: serde_json::Value,
}

#[async_trait]
pub trait PlatformConnector: Send + Sync {
    fn platform(&self) -> &str;
    async fn verify_auth(&self) -> Result<PlatformAuthInfo>;
    async fn like(&self, target: &str) -> Result<RemoteActionReceipt>;
    async fn follow(&self, target: &str) -> Result<RemoteActionReceipt>;
    async fn subscribe(&self, target: &str) -> Result<RemoteActionReceipt>;
    async fn reshare(&self, target: &str, quote: Option<&str>) -> Result<RemoteActionReceipt>;
    async fn comment(&self, target: &str, text: &str) -> Result<RemoteActionReceipt>;
    async fn reply(&self, target: &str, text: &str) -> Result<RemoteActionReceipt>;
    async fn publish(&self, request: PublishRequest) -> Result<RemoteActionReceipt>;
    async fn fetch_inbox(&self, since: Option<DateTime<Utc>>, limit: u32) -> Result<Vec<RemoteInteraction>>;
    async fn fetch_metrics(&self) -> Result<RemoteMetrics>;
}

#[async_trait]
pub trait TokenSource: Send + Sync {
    async fn credentials(&self, platform: &str) -> Option<PlatformCredentials>;
}

pub fn normalize_platform(platform: &str) -> String {
    match platform.trim().to_lowercase().as_str() {
        "twitter" | "x.com" | "twitter.com" => "x".into(),
        "yt" | "youtube.com" => "youtube".into(),
        "ig" | "instagram.com" => "instagram".into(),
        "reddit.com" => "reddit".into(),
        other => other.to_string(),
    }
}

pub fn token_env_var(platform: &str) -> Option<&'static str> {
    match normalize_platform(platform).as_str() {
        "x" => Some("X_ACCESS_TOKEN"),
        "youtube" => Some("YOUTUBE_ACCESS_TOKEN"),
        "reddit" => Some("REDDIT_ACCESS_TOKEN"),
        "instagram" => Some("INSTAGRAM_ACCESS_TOKEN"),
        _ => None,
    }
}

fn env_value(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

#[derive(Debug, Clone, Default)]
pub struct EnvTokenSource;

impl EnvTokenSource {
    pub fn read(platform: &str) -> Option<PlatformCredentials> {
        let platform = normalize_platform(platform);
        let token = env_value(token_env_var(&platform)?)?;
        let mut creds = PlatformCredentials::new(token);
        let (user_var, extras): (Option<&str>, &[(&str, &str)]) = match platform.as_str() {
            "x" => (Some("X_USER_ID"), &[]),
            "youtube" => (
                Some("YOUTUBE_CHANNEL_ID"),
                &[("api_key", "YOUTUBE_API_KEY"), ("privacy_status", "YOUTUBE_PRIVACY_STATUS")],
            ),
            "reddit" => (
                None,
                &[("user_agent", "REDDIT_USER_AGENT"), ("default_subreddit", "REDDIT_DEFAULT_SUBREDDIT")],
            ),
            "instagram" => (Some("INSTAGRAM_USER_ID"), &[]),
            _ => (None, &[]),
        };
        creds.user_id = user_var.and_then(env_value);
        for (key, var) in extras {
            if let Some(v) = env_value(var) {
                creds.extra.insert((*key).to_string(), v);
            }
        }
        Some(creds)
    }
}

#[async_trait]
impl TokenSource for EnvTokenSource {
    async fn credentials(&self, platform: &str) -> Option<PlatformCredentials> {
        Self::read(platform)
    }
}

#[derive(Default)]
pub struct StaticTokenSource {
    entries: RwLock<HashMap<String, PlatformCredentials>>,
}

impl StaticTokenSource {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_map(map: HashMap<String, PlatformCredentials>) -> Self {
        let entries = map.into_iter().map(|(k, v)| (normalize_platform(&k), v)).collect();
        Self { entries: RwLock::new(entries) }
    }

    pub async fn insert(&self, platform: &str, credentials: PlatformCredentials) {
        self.entries.write().await.insert(normalize_platform(platform), credentials);
    }

    pub async fn remove(&self, platform: &str) {
        self.entries.write().await.remove(&normalize_platform(platform));
    }
}

#[async_trait]
impl TokenSource for StaticTokenSource {
    async fn credentials(&self, platform: &str) -> Option<PlatformCredentials> {
        self.entries.read().await.get(&normalize_platform(platform)).cloned()
    }
}

pub struct ChainedTokenSource {
    sources: Vec<Arc<dyn TokenSource>>,
}

impl ChainedTokenSource {
    pub fn new(sources: Vec<Arc<dyn TokenSource>>) -> Self {
        Self { sources }
    }
}

#[async_trait]
impl TokenSource for ChainedTokenSource {
    async fn credentials(&self, platform: &str) -> Option<PlatformCredentials> {
        for source in &self.sources {
            if let Some(c) = source.credentials(platform).await {
                return Some(c);
            }
        }
        None
    }
}

pub struct ConnectorRegistry {
    tokens: Arc<dyn TokenSource>,
    overrides: StdRwLock<HashMap<String, Arc<dyn PlatformConnector>>>,
    base_urls: StdRwLock<HashMap<String, String>>,
}

impl ConnectorRegistry {
    pub fn new(tokens: Arc<dyn TokenSource>) -> Self {
        Self {
            tokens,
            overrides: StdRwLock::new(HashMap::new()),
            base_urls: StdRwLock::new(HashMap::new()),
        }
    }

    pub fn from_env() -> Self {
        Self::new(Arc::new(EnvTokenSource))
    }

    pub fn with_connector(self, platform: &str, connector: Arc<dyn PlatformConnector>) -> Self {
        self.set_connector(platform, connector);
        self
    }

    pub fn set_connector(&self, platform: &str, connector: Arc<dyn PlatformConnector>) {
        if let Ok(mut map) = self.overrides.write() {
            map.insert(normalize_platform(platform), connector);
        }
    }

    pub fn with_base_url(self, platform: &str, base_url: &str) -> Self {
        if let Ok(mut map) = self.base_urls.write() {
            map.insert(normalize_platform(platform), base_url.to_string());
        }
        self
    }

    fn base_url(&self, platform: &str) -> Option<String> {
        self.base_urls.read().ok().and_then(|m| m.get(platform).cloned())
    }

    pub async fn is_connected(&self, platform: &str) -> bool {
        self.connector_for(platform).await.is_ok()
    }

    pub async fn connector_for(&self, platform: &str) -> Result<Arc<dyn PlatformConnector>> {
        let platform = normalize_platform(platform);
        if let Some(c) = self.overrides.read().ok().and_then(|m| m.get(&platform).cloned()) {
            return Ok(c);
        }
        let Some(env) = token_env_var(&platform) else {
            return Err(OpenAccountsError::Automation(format!(
                "{platform} has no official API connector in OpenAccounts"
            )));
        };
        let creds = self.tokens.credentials(&platform).await.ok_or_else(|| {
            OpenAccountsError::Automation(format!("{platform} is not connected: set {env}"))
        })?;
        let base = self.base_url(&platform);
        let connector: Arc<dyn PlatformConnector> = match platform.as_str() {
            "x" => Arc::new(XConnector::with_base_url(creds, base.as_deref().unwrap_or(X_API_BASE))),
            "youtube" => {
                let api = base.clone().unwrap_or_else(|| YOUTUBE_API_BASE.to_string());
                let upload = base.unwrap_or_else(|| YOUTUBE_UPLOAD_BASE.to_string());
                Arc::new(YouTubeConnector::with_base_urls(creds, &api, &upload))
            }
            "reddit" => Arc::new(RedditConnector::with_base_url(creds, base.as_deref().unwrap_or(REDDIT_API_BASE))),
            "instagram" => Arc::new(InstagramConnector::with_base_url(
                creds,
                base.as_deref().unwrap_or(INSTAGRAM_API_BASE),
            )),
            other => {
                return Err(OpenAccountsError::Automation(format!(
                    "{other} has no official API connector in OpenAccounts"
                )))
            }
        };
        Ok(connector)
    }
}

pub(crate) fn http_client() -> Client {
    static CLIENT: OnceLock<Client> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .user_agent(concat!("OpenAccounts/", env!("CARGO_PKG_VERSION")))
                .build()
                .unwrap_or_else(|_| Client::new())
        })
        .clone()
}

pub(crate) fn transport_error(platform: &str, op: &str, err: reqwest::Error) -> OpenAccountsError {
    OpenAccountsError::Automation(format!("{platform} {op} request failed: {err}"))
}

pub(crate) fn unsupported(op: &str, platform: &str) -> OpenAccountsError {
    OpenAccountsError::Automation(format!("{op} is not supported by the official {platform} API"))
}

pub(crate) async fn read_json(resp: Response, platform: &str, op: &str) -> Result<serde_json::Value> {
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| transport_error(platform, op, e))?;
    if !status.is_success() {
        let preview: String = text.chars().take(ERROR_PREVIEW_CHARS).collect();
        return Err(OpenAccountsError::Automation(format!(
            "{platform} {op} failed with HTTP {}: {}",
            status.as_u16(),
            preview.trim()
        )));
    }
    if text.trim().is_empty() {
        return Ok(serde_json::Value::Null);
    }
    serde_json::from_str(&text)
        .map_err(|e| OpenAccountsError::Automation(format!("{platform} {op} returned invalid JSON: {e}")))
}

pub(crate) fn json_u64(v: &serde_json::Value) -> Option<u64> {
    v.as_u64().or_else(|| v.as_str().and_then(|s| s.trim().parse().ok())).or_else(|| {
        v.as_i64().map(|n| n.max(0) as u64)
    })
}

pub(crate) fn json_str(v: &serde_json::Value) -> Option<String> {
    v.as_str().map(str::to_string).filter(|s| !s.is_empty())
}

pub(crate) fn parse_rfc3339(v: &serde_json::Value) -> Option<DateTime<Utc>> {
    v.as_str()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.with_timezone(&Utc))
}

pub(crate) fn truncate_chars(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

pub(crate) fn join_url(base: &str, path: &str) -> String {
    format!("{}/{}", base.trim_end_matches('/'), path.trim_start_matches('/'))
}

pub(crate) fn is_remote_url(path: &str) -> bool {
    let lower = path.trim().to_lowercase();
    lower.starts_with("https://") || lower.starts_with("http://")
}
