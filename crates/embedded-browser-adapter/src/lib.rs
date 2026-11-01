use async_trait::async_trait;
use openaccounts_browser_abstraction::{BrowserProvider, PageState};
use openaccounts_chrome_relay_adapter::{CdpDriver, ChromeProcess};
use openaccounts_core::Result;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tokio::sync::Mutex;
use tracing::info;

pub use openaccounts_chrome_relay_adapter::{find_browser_binary, CHROME_PATH_ENV};

struct RunningBrowser {
    process: ChromeProcess,
    driver: Arc<CdpDriver>,
}

pub struct EmbeddedBrowserAdapter {
    binary: Option<PathBuf>,
    origin_fence: Vec<String>,
    running: Mutex<Option<RunningBrowser>>,
    paused: AtomicBool,
}

impl EmbeddedBrowserAdapter {
    pub fn new() -> Self {
        Self {
            binary: None,
            origin_fence: Vec::new(),
            running: Mutex::new(None),
            paused: AtomicBool::new(false),
        }
    }

    pub fn with_binary(mut self, binary: impl Into<PathBuf>) -> Self {
        self.binary = Some(binary.into());
        self
    }

    pub fn with_origin_fence(mut self, origin_fence: Vec<String>) -> Self {
        self.origin_fence = origin_fence;
        self
    }

    pub async fn is_running(&self) -> bool {
        self.running.lock().await.is_some()
    }

    pub async fn devtools_endpoint(&self) -> Option<String> {
        self.running
            .lock()
            .await
            .as_ref()
            .map(|r| r.process.http_base().to_string())
    }

    async fn driver(&self) -> Result<Arc<CdpDriver>> {
        let mut guard = self.running.lock().await;
        if let Some(running) = guard.as_ref() {
            return Ok(running.driver.clone());
        }
        let process = ChromeProcess::launch(self.binary.clone()).await?;
        let driver = Arc::new(CdpDriver::new(process.http_base(), self.origin_fence.clone()));
        driver.set_paused(self.paused.load(Ordering::SeqCst));
        info!("Embedded browser session started at {}", process.http_base());
        *guard = Some(RunningBrowser {
            process,
            driver: driver.clone(),
        });
        Ok(driver)
    }
}

impl Default for EmbeddedBrowserAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl BrowserProvider for EmbeddedBrowserAdapter {
    async fn navigate(&self, url: &str) -> Result<()> {
        self.driver().await?.navigate(url).await
    }

    async fn click(&self, selector: &str) -> Result<()> {
        self.driver().await?.click(selector).await
    }

    async fn type_text(&self, selector: &str, text: &str, is_secret: bool) -> Result<()> {
        self.driver().await?.type_text(selector, text, is_secret).await
    }

    async fn get_page_state(&self) -> Result<PageState> {
        self.driver().await?.get_page_state().await
    }

    async fn take_screenshot(&self) -> Result<Vec<u8>> {
        self.driver().await?.take_screenshot().await
    }

    async fn evaluate_script(&self, script: &str) -> Result<serde_json::Value> {
        self.driver().await?.evaluate_script(script).await
    }

    async fn pause(&self) -> Result<()> {
        self.paused.store(true, Ordering::SeqCst);
        if let Some(running) = self.running.lock().await.as_ref() {
            running.driver.set_paused(true);
        }
        info!("Embedded browser paused for human takeover");
        Ok(())
    }

    async fn resume(&self) -> Result<()> {
        self.paused.store(false, Ordering::SeqCst);
        if let Some(running) = self.running.lock().await.as_ref() {
            running.driver.set_paused(false);
        }
        info!("Embedded browser resumed");
        Ok(())
    }

    async fn close(&self) -> Result<()> {
        let running = self.running.lock().await.take();
        if let Some(mut running) = running {
            let _ = running.driver.close().await;
            running.process.shutdown().await;
            info!("Embedded browser session closed");
        }
        Ok(())
    }
}
