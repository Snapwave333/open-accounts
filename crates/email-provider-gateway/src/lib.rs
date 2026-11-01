use async_trait::async_trait;
use chrono::{DateTime, Utc};
use openaccounts_core::Result;
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

pub struct MockEmailProviderAdapter {
    pub provider_type: EmailProviderType,
    pub domain: String,
}

impl MockEmailProviderAdapter {
    pub fn new(provider_type: EmailProviderType, domain: String) -> Self {
        Self {
            provider_type,
            domain,
        }
    }
}

#[async_trait]
impl EmailProviderAdapter for MockEmailProviderAdapter {
    async fn send_email(&self, _to: &str, _subject: &str, _body_text: &str) -> Result<String> {
        Ok("msg_mock_sent_9918".into())
    }

    async fn fetch_recent_messages(
        &self,
        _limit: u32,
        _since: Option<DateTime<Utc>>,
    ) -> Result<Vec<IncomingEmailMessage>> {
        Ok(vec![IncomingEmailMessage {
            message_id: "msg_verify_101".into(),
            sender: "no-reply@instagram.mail.com".into(),
            recipient: format!("auth@{}", self.domain),
            subject: "Your Instagram Verification Code: 492810".into(),
            body_plain: "Please use verification code 492810 to confirm your account registration or click https://instagram.com/verify?code=492810".into(),
            body_html: None,
            received_at: Utc::now(),
        }])
    }

    async fn create_alias_or_mailbox(&self, prefix: &str) -> Result<String> {
        Ok(format!("{}_{}@{}", prefix, uuid::Uuid::new_v4().to_string()[..6].to_lowercase(), self.domain))
    }
}
