use chrono::{DateTime, Utc};
use openaccounts_core::{OpenAccountsError, Result};
use openaccounts_email_provider_gateway::{EmailProviderAdapter, IncomingEmailMessage};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::Duration};
use tracing::info;

const POLL_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationCodeExtraction {
    pub code: Option<String>,
    pub verification_link: Option<String>,
    pub sender: String,
    pub extracted_at: DateTime<Utc>,
    pub is_expired: bool,
}

pub struct EmailVerificationEngine;

impl EmailVerificationEngine {
    pub fn new() -> Self {
        Self
    }

    pub fn extract_code_or_link(&self, message: &IncomingEmailMessage) -> VerificationCodeExtraction {
        // Regex pattern for 4-8 digit numeric or alphanumeric OTP codes
        let code_regex = Regex::new(r"\b([0-9]{4,8})\b").unwrap();
        let link_regex = Regex::new(r"https?://[^\s<>]*(?:verify|confirm|token|code)[^\s<>]*").unwrap();

        let code = code_regex
            .captures(&message.subject)
            .or_else(|| code_regex.captures(&message.body_plain))
            .map(|cap| cap[1].to_string());

        let verification_link = link_regex
            .find(&message.body_plain)
            .map(|mat| mat.as_str().to_string());

        VerificationCodeExtraction {
            code,
            verification_link,
            sender: message.sender.clone(),
            extracted_at: Utc::now(),
            is_expired: false,
        }
    }

    /// Polls the provider until a matching verification email arrives or
    /// `timeout_seconds` elapse.
    pub async fn wait_for_verification_email(
        &self,
        provider: Arc<dyn EmailProviderAdapter>,
        expected_sender_domain: &str,
        timeout_seconds: u64,
    ) -> Result<VerificationCodeExtraction> {
        info!("Listening for verification message from {}", expected_sender_domain);
        let start = Utc::now();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_seconds);

        loop {
            let messages = provider.fetch_recent_messages(5, Some(start)).await?;
            for msg in messages {
                if msg.sender.contains(expected_sender_domain)
                    || msg.subject.to_lowercase().contains("verification")
                {
                    info!("Extracted verification token from email");
                    return Ok(self.extract_code_or_link(&msg));
                }
            }
            if tokio::time::Instant::now() >= deadline {
                break;
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }

        Err(OpenAccountsError::Internal(format!(
            "No verification email from {expected_sender_domain} within {timeout_seconds}s"
        )))
    }
}

impl Default for EmailVerificationEngine {
    fn default() -> Self {
        Self::new()
    }
}
