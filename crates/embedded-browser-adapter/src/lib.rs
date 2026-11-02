use async_trait::async_trait;
use openaccounts_browser_abstraction::{BrowserProvider, PageState};
use openaccounts_core::{OpenAccountsError, Result};

/// In-app webview browser adapter. No embedded engine is wired up,
/// so every operation fails instead of pretending a page was driven. Use the
/// Chrome relay adapter for real browser work.
pub struct EmbeddedBrowserAdapter;

impl EmbeddedBrowserAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for EmbeddedBrowserAdapter {
    fn default() -> Self {
        Self::new()
    }
}

fn unavailable(op: &str) -> OpenAccountsError {
    OpenAccountsError::NotConfigured(format!(
        "Embedded browser is not implemented ({op}); use the Chrome relay adapter"
    ))
}

#[async_trait]
impl BrowserProvider for EmbeddedBrowserAdapter {
    async fn navigate(&self, _url: &str) -> Result<()> {
        Err(unavailable("navigate"))
    }

    async fn click(&self, _selector: &str) -> Result<()> {
        Err(unavailable("click"))
    }

    async fn type_text(&self, _selector: &str, _text: &str, _is_secret: bool) -> Result<()> {
        Err(unavailable("type_text"))
    }

    async fn get_page_state(&self) -> Result<PageState> {
        Err(unavailable("get_page_state"))
    }

    async fn take_screenshot(&self) -> Result<Vec<u8>> {
        Err(unavailable("take_screenshot"))
    }

    async fn evaluate_script(&self, _script: &str) -> Result<serde_json::Value> {
        Err(unavailable("evaluate_script"))
    }

    async fn pause(&self) -> Result<()> {
        Err(unavailable("pause"))
    }

    async fn resume(&self) -> Result<()> {
        Err(unavailable("resume"))
    }

    async fn close(&self) -> Result<()> {
        Err(unavailable("close"))
    }
}
