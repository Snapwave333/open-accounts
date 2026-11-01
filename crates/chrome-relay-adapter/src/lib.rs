pub mod cdp;
pub mod driver;
pub mod launch;

pub use cdp::{devtools_http_base, CdpEvent, CdpSession, CdpTarget};
pub use driver::{is_url_allowed, page_state_from_value, CdpDriver};
pub use launch::{find_browser_binary, ChromeProcess, CHROME_PATH_ENV};

use async_trait::async_trait;
use openaccounts_browser_abstraction::{BrowserProvider, PageState};
use openaccounts_core::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChromeRelayConfig {
    pub endpoint: String,
    pub tab_lease_id: Option<String>,
    pub origin_fence: Vec<String>,
}

pub struct ChromeRelayAdapter {
    pub config: ChromeRelayConfig,
    driver: CdpDriver,
}

impl ChromeRelayAdapter {
    pub fn new(config: ChromeRelayConfig) -> Self {
        let driver = CdpDriver::new(devtools_http_base(&config.endpoint), config.origin_fence.clone())
            .with_attach_target(config.tab_lease_id.clone());
        Self { config, driver }
    }

    pub fn driver(&self) -> &CdpDriver {
        &self.driver
    }

    pub fn is_paused(&self) -> bool {
        self.driver.is_paused()
    }
}

#[async_trait]
impl BrowserProvider for ChromeRelayAdapter {
    async fn navigate(&self, url: &str) -> Result<()> {
        self.driver.navigate(url).await
    }

    async fn click(&self, selector: &str) -> Result<()> {
        self.driver.click(selector).await
    }

    async fn type_text(&self, selector: &str, text: &str, is_secret: bool) -> Result<()> {
        self.driver.type_text(selector, text, is_secret).await
    }

    async fn get_page_state(&self) -> Result<PageState> {
        self.driver.get_page_state().await
    }

    async fn take_screenshot(&self) -> Result<Vec<u8>> {
        self.driver.take_screenshot().await
    }

    async fn evaluate_script(&self, script: &str) -> Result<serde_json::Value> {
        self.driver.evaluate_script(script).await
    }

    async fn pause(&self) -> Result<()> {
        self.driver.pause().await
    }

    async fn resume(&self) -> Result<()> {
        self.driver.resume().await
    }

    async fn close(&self) -> Result<()> {
        self.driver.close().await
    }
}
