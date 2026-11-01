use async_trait::async_trait;
use openaccounts_browser_abstraction::{BrowserProvider, PageState};
use openaccounts_core::Result;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct EmbeddedBrowserAdapter {
    state: Arc<RwLock<PageState>>,
}

impl EmbeddedBrowserAdapter {
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(PageState {
                url: "about:blank".into(),
                title: "Embedded Session".into(),
                text_content: "".into(),
                is_ready: true,
                pending_challenge: None,
            })),
        }
    }
}

#[async_trait]
impl BrowserProvider for EmbeddedBrowserAdapter {
    async fn navigate(&self, url: &str) -> Result<()> {
        let mut s = self.state.write().await;
        s.url = url.to_string();
        Ok(())
    }

    async fn click(&self, _selector: &str) -> Result<()> {
        Ok(())
    }

    async fn type_text(&self, _selector: &str, _text: &str, _is_secret: bool) -> Result<()> {
        Ok(())
    }

    async fn get_page_state(&self) -> Result<PageState> {
        Ok(self.state.read().await.clone())
    }

    async fn take_screenshot(&self) -> Result<Vec<u8>> {
        Ok(vec![])
    }

    async fn evaluate_script(&self, _script: &str) -> Result<serde_json::Value> {
        Ok(serde_json::json!({}))
    }

    async fn pause(&self) -> Result<()> {
        Ok(())
    }

    async fn resume(&self) -> Result<()> {
        Ok(())
    }

    async fn close(&self) -> Result<()> {
        Ok(())
    }
}
