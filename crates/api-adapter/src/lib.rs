use async_trait::async_trait;
use openaccounts_core::{OpenAccountsError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformApiConfig {
    pub base_url: String,
    pub api_key_secret_id: Option<String>,
    pub headers: HashMap<String, String>,
}

#[async_trait]
pub trait PlatformApiAdapter: Send + Sync {
    async fn execute_action(
        &self,
        endpoint: &str,
        payload: serde_json::Value,
        token: Option<String>,
    ) -> Result<serde_json::Value>;
}

pub struct GenericApiAdapter {
    config: PlatformApiConfig,
    client: reqwest::Client,
}

impl GenericApiAdapter {
    pub fn new(config: PlatformApiConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl PlatformApiAdapter for GenericApiAdapter {
    async fn execute_action(
        &self,
        endpoint: &str,
        payload: serde_json::Value,
        token: Option<String>,
    ) -> Result<serde_json::Value> {
        let url = format!("{}/{}", self.config.base_url.trim_end_matches('/'), endpoint.trim_start_matches('/'));
        let mut req = self.client.post(&url);

        if let Some(tok) = token {
            req = req.bearer_auth(tok);
        }

        for (k, v) in &self.config.headers {
            req = req.header(k, v);
        }

        let res = req
            .json(&payload)
            .send()
            .await
            .map_err(|e| OpenAccountsError::Internal(format!("API call failed: {e}")))?;

        let data: serde_json::Value = res
            .json()
            .await
            .map_err(|e| OpenAccountsError::Internal(format!("API parse failed: {e}")))?;

        Ok(data)
    }
}
