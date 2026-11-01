use async_trait::async_trait;
use openaccounts_browser_abstraction::{BrowserProvider, PageState};
use openaccounts_core::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChromeRelayConfig {
    pub endpoint: String,
    pub tab_lease_id: Option<String>,
    pub origin_fence: Vec<String>,
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
                title: "New Tab".into(),
                text_content: "".into(),
                is_ready: true,
                pending_challenge: None,
            })),
        }
    }
}

#[async_trait]
impl BrowserProvider for ChromeRelayAdapter {
    async fn navigate(&self, url: &str) -> Result<()> {
        info!("Chrome Relay navigating to {}", url);
        let mut state = self.current_state.write().await;
        state.url = url.to_string();
        state.is_ready = true;
        Ok(())
    }

    async fn click(&self, selector: &str) -> Result<()> {
        info!("Chrome Relay clicking selector: {}", selector);
        Ok(())
    }

    async fn type_text(&self, selector: &str, text: &str, is_secret: bool) -> Result<()> {
        if is_secret {
            info!("Chrome Relay typing secret into selector: {}", selector);
        } else {
            info!("Chrome Relay typing '{}' into selector: {}", text, selector);
        }
        Ok(())
    }

    async fn get_page_state(&self) -> Result<PageState> {
        let state = self.current_state.read().await;
        Ok(state.clone())
    }

    async fn take_screenshot(&self) -> Result<Vec<u8>> {
        Ok(vec![])
    }

    async fn evaluate_script(&self, _script: &str) -> Result<serde_json::Value> {
        Ok(serde_json::json!({ "result": "ok" }))
    }

    async fn pause(&self) -> Result<()> {
        info!("Chrome Relay automation paused for human takeover");
        Ok(())
    }

    async fn resume(&self) -> Result<()> {
        info!("Chrome Relay automation resumed by user");
        Ok(())
    }

    async fn close(&self) -> Result<()> {
        info!("Chrome Relay session closed");
        Ok(())
    }
}
