use async_trait::async_trait;
use openaccounts_credential_vault::VaultManager;
use openaccounts_platform_connectors::{PlatformCredentials, TokenSource};
use std::sync::Arc;

pub struct VaultTokenSource {
    vault: Arc<VaultManager>,
}

impl VaultTokenSource {
    pub fn new(vault: Arc<VaultManager>) -> Self {
        Self { vault }
    }

    async fn read(&self, label: &str) -> Option<String> {
        let id = self.vault.find_secret(None, label).await?;
        self.vault
            .get_secret(&id)
            .await
            .ok()
            .map(|p| p.value.trim().to_string())
            .filter(|v| !v.is_empty())
    }
}

pub fn platform_key(platform: &str) -> String {
    match platform.trim().to_lowercase().as_str() {
        "twitter" => "x".into(),
        other => other.to_string(),
    }
}

#[async_trait]
impl TokenSource for VaultTokenSource {
    async fn credentials(&self, platform: &str) -> Option<PlatformCredentials> {
        if !self.vault.is_unlocked().await {
            return None;
        }
        let key = platform_key(platform);
        let token = self.read(&format!("{key}:access_token")).await?;
        let mut creds = PlatformCredentials::new(token);
        creds.user_id = self.read(&format!("{key}:user_id")).await;
        for extra in ["api_key", "user_agent", "default_subreddit", "privacy_status"] {
            if let Some(v) = self.read(&format!("{key}:{extra}")).await {
                creds.extra.insert(extra.to_string(), v);
            }
        }
        Some(creds)
    }
}
