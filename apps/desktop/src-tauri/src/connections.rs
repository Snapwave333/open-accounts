use crate::state::AppState;
use futures::future::join_all;
use openaccounts_lm_gateway::{resolve_env_secret, ModelProviderType};
use openaccounts_platform_connectors::SUPPORTED_PLATFORMS;
use serde::Serialize;
use std::time::{Duration, Instant};

const CACHE_TTL: Duration = Duration::from_secs(60);
const CHECK_TIMEOUT: Duration = Duration::from_secs(12);

pub type ConnectionCache = Option<(Instant, Vec<ConnectionStatus>)>;

#[derive(Debug, Clone, Serialize)]
pub struct ConnectionStatus {
    pub id: String,
    pub service: String,
    pub kind: String,
    pub connected: bool,
    pub detail: String,
}

fn clean_error(e: impl std::fmt::Display) -> String {
    let text = e.to_string();
    for prefix in ["Automation error: ", "LM Gateway error: ", "Internal error: ", "Database error: "] {
        if let Some(rest) = text.strip_prefix(prefix) {
            return rest.to_string();
        }
    }
    text
}

pub fn service_label(platform: &str) -> String {
    match platform {
        "x" => "X".into(),
        "youtube" => "YouTube".into(),
        "reddit" => "Reddit".into(),
        "instagram" => "Instagram".into(),
        "gmail" => "Gmail".into(),
        "outlook" => "Outlook".into(),
        other => other.to_string(),
    }
}

async fn social(state: &AppState, platform: &str) -> ConnectionStatus {
    let status = |connected: bool, detail: String| ConnectionStatus {
        id: platform.to_string(),
        service: service_label(platform),
        kind: "social".into(),
        connected,
        detail,
    };
    let connector = match state.connectors.connector_for(platform).await {
        Ok(c) => c,
        Err(e) => return status(false, format!("{}, or store {platform}:access_token in the Vault", clean_error(e))),
    };
    match tokio::time::timeout(CHECK_TIMEOUT, connector.verify_auth()).await {
        Ok(Ok(info)) if info.is_authenticated => status(
            true,
            info.account_handle
                .or(info.account_remote_id)
                .map(|h| format!("Signed in as {h}"))
                .unwrap_or_else(|| "Authenticated".into()),
        ),
        Ok(Ok(info)) => status(false, info.error.unwrap_or_else(|| "Token rejected".into())),
        Ok(Err(e)) => status(false, clean_error(e)),
        Err(_) => status(false, format!("No response within {}s", CHECK_TIMEOUT.as_secs())),
    }
}

async fn email(state: &AppState, provider_id: String) -> ConnectionStatus {
    let (connected, detail) = match state.email_manager.provider(&provider_id).await {
        Some(provider) => match tokio::time::timeout(CHECK_TIMEOUT, provider.mailbox_address()).await {
            Ok(Ok(address)) => (true, format!("Mailbox {address}")),
            Ok(Err(e)) => (false, clean_error(e)),
            Err(_) => (false, format!("No response within {}s", CHECK_TIMEOUT.as_secs())),
        },
        None => (false, "Not registered".into()),
    };
    ConnectionStatus { id: provider_id.clone(), service: service_label(&provider_id), kind: "email".into(), connected, detail }
}

async fn language_models(state: &AppState) -> Vec<ConnectionStatus> {
    let usage = state.lm_gateway.usage_report().await;
    let mut out = Vec::new();
    for profile in state.lm_gateway.list_profiles().await {
        if profile.id.contains("vision") {
            continue;
        }
        let needs_key = !matches!(
            profile.provider_type,
            ModelProviderType::Ollama | ModelProviderType::OpenCodeCli | ModelProviderType::ClaudeCodeCli | ModelProviderType::CustomHttp
        );
        let key_name = profile
            .api_key_secret_id
            .clone()
            .or_else(|| profile.provider_type.default_api_key_env().map(str::to_string));
        let has_key = !needs_key || key_name.as_deref().and_then(resolve_env_secret).is_some();
        let last_error = usage.get(&profile.id).and_then(|u| u.last_error.clone());
        let detail = match (has_key, &last_error) {
            (false, _) => format!("Set {} to enable this model", key_name.unwrap_or_else(|| "an API key".into())),
            (true, Some(e)) => format!("{} · last error: {}", profile.model_id, clean_error(e)),
            (true, None) => profile.model_id.clone(),
        };
        out.push(ConnectionStatus {
            id: profile.id.clone(),
            service: if profile.id == openaccounts_lm_gateway::DEFAULT_PROFILE_ID {
                "NVIDIA NIM cascade".into()
            } else {
                profile.name.clone()
            },
            kind: "lm".into(),
            connected: has_key && last_error.is_none(),
            detail,
        });
    }
    out
}

pub async fn statuses(state: &AppState, force: bool) -> Vec<ConnectionStatus> {
    if !force {
        if let Some((at, cached)) = state.connection_cache.read().await.as_ref() {
            if at.elapsed() < CACHE_TTL {
                return cached.clone();
            }
        }
    }
    let socials = join_all(SUPPORTED_PLATFORMS.iter().map(|p| social(state, p)));
    let providers = state.email_manager.list_providers().await;
    let mut out = socials.await;
    if providers.is_empty() {
        out.push(ConnectionStatus {
            id: "email".into(),
            service: "Email".into(),
            kind: "email".into(),
            connected: false,
            detail: "Set GMAIL_ACCESS_TOKEN or OUTLOOK_ACCESS_TOKEN to connect a mailbox".into(),
        });
    } else {
        out.extend(join_all(providers.into_iter().map(|p| email(state, p))).await);
    }
    out.extend(language_models(state).await);
    *state.connection_cache.write().await = Some((Instant::now(), out.clone()));
    out
}
