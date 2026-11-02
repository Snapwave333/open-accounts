use async_trait::async_trait;
use chrono::{DateTime, Utc};
use openaccounts_core::{OpenAccountsError, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmailProviderType {
    GmailApi,
    MicrosoftGraph,
    ImapSmtp,
    CustomDomainApi,
    MailboxProvisioningService,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncomingEmailMessage {
    pub message_id: String,
    pub sender: String,
    pub recipient: String,
    pub subject: String,
    pub body_plain: String,
    pub body_html: Option<String>,
    pub received_at: DateTime<Utc>,
}

#[async_trait]
pub trait EmailProviderAdapter: Send + Sync {
    fn provider_type(&self) -> EmailProviderType;

    async fn send_email(
        &self,
        to: &str,
        subject: &str,
        body_text: &str,
    ) -> Result<String>;

    async fn fetch_recent_messages(
        &self,
        limit: u32,
        since: Option<DateTime<Utc>>,
    ) -> Result<Vec<IncomingEmailMessage>>;

    async fn create_alias_or_mailbox(&self, prefix: &str) -> Result<String>;
}

/// A provider that is declared but has no live connection. Allocation and
/// sends fail loudly here instead of returning a fabricated mailbox or a fake
/// message id.
pub struct UnconfiguredEmailProviderAdapter {
    pub provider_id: String,
    pub provider_type: EmailProviderType,
    pub reason: String,
}

impl UnconfiguredEmailProviderAdapter {
    pub fn new(provider_id: &str, provider_type: EmailProviderType, reason: &str) -> Self {
        Self {
            provider_id: provider_id.to_string(),
            provider_type,
            reason: reason.to_string(),
        }
    }
}

#[async_trait]
impl EmailProviderAdapter for UnconfiguredEmailProviderAdapter {
    fn provider_type(&self) -> EmailProviderType {
        self.provider_type.clone()
    }

    async fn send_email(&self, _to: &str, _subject: &str, _body_text: &str) -> Result<String> {
        Err(OpenAccountsError::NotConfigured(format!(
            "Cannot send email: provider '{}' is not connected ({})",
            self.provider_id, self.reason
        )))
    }

    async fn fetch_recent_messages(
        &self,
        _limit: u32,
        _since: Option<DateTime<Utc>>,
    ) -> Result<Vec<IncomingEmailMessage>> {
        Err(OpenAccountsError::NotConfigured(format!(
            "Cannot read inbox: provider '{}' is not connected ({})",
            self.provider_id, self.reason
        )))
    }

    async fn create_alias_or_mailbox(&self, _prefix: &str) -> Result<String> {
        Err(OpenAccountsError::NotConfigured(format!(
            "Cannot allocate a mailbox: provider '{}' is not connected ({})",
            self.provider_id, self.reason
        )))
    }
}
