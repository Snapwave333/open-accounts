use async_trait::async_trait;
use openaccounts_core::{OpenAccountsError, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[async_trait]
pub trait ModelAdapter: Send + Sync {
    async fn complete(&self, req: CompletionRequest, api_key: Option<String>) -> Result<CompletionResponse>;
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
}

impl LMGateway {
    pub fn new() -> Self {
        Self {
            profiles: Arc::new(RwLock::new(HashMap::new())),
            adapters: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn register_profile(&self, profile: ModelProfile) {
        let id = profile.id.clone();
        let adapter: Arc<dyn ModelAdapter> = match profile.provider_type {
            ModelProviderType::Ollama => {
                let endpoint = profile.endpoint.clone().unwrap_or_else(|| "http://localhost:11434".into());
                Arc::new(OllamaAdapter::new(endpoint, profile.model_id.clone()))
            }
            ModelProviderType::OpenAICompatible
            | ModelProviderType::OpenRouter
            | ModelProviderType::NvidiaNim
            | ModelProviderType::Gemini
            | ModelProviderType::Anthropic
            | ModelProviderType::CustomHttp => {
                let endpoint = profile.endpoint.clone().unwrap_or_else(|| "https://api.openai.com/v1".into());
                Arc::new(OpenAICompatibleAdapter::new(endpoint, profile.model_id.clone()))
            }
            ModelProviderType::OpenCodeCli | ModelProviderType::ClaudeCodeCli => {
                // CLI adapters route through standard input/output execution
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
            .ok_or_else(|| OpenAccountsError::LMGateway(format!("Profile {profile_id} not found")))?;

        adapter.complete(request, api_key).await
    }

    pub async fn list_profiles(&self) -> Vec<ModelProfile> {
        self.profiles.read().await.values().cloned().collect()
    }
}
