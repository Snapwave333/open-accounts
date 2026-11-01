use async_trait::async_trait;
use openaccounts_core::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChallengeType {
    Captcha,
    LegalConsent,
    TwoFactorCode,
    AgeVerification,
    ManualReview,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeDetection {
    pub challenge_type: ChallengeType,
    pub description: String,
    pub element_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageState {
    pub url: String,
    pub title: String,
    pub text_content: String,
    pub is_ready: bool,
    pub pending_challenge: Option<ChallengeDetection>,
}

#[async_trait]
pub trait BrowserProvider: Send + Sync {
    async fn navigate(&self, url: &str) -> Result<()>;
    async fn click(&self, selector: &str) -> Result<()>;
    async fn type_text(&self, selector: &str, text: &str, is_secret: bool) -> Result<()>;
    async fn get_page_state(&self) -> Result<PageState>;
    async fn take_screenshot(&self) -> Result<Vec<u8>>;
    async fn evaluate_script(&self, script: &str) -> Result<serde_json::Value>;
    async fn pause(&self) -> Result<()>;
    async fn resume(&self) -> Result<()>;
    async fn close(&self) -> Result<()>;
}
