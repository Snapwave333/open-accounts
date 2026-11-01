use chrono::{DateTime, Utc};
use openaccounts_core::{AccountId, OpenAccountsError, Result};
use openaccounts_lm_gateway::LMGateway;
use openaccounts_policy_engine::PolicyEngine;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionType {
    Comment,
    Mention,
    DirectMessage,
    Reply,
    PostLike,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClassificationCategory {
    GeneralInquiry,
    SalesLead,
    CustomerSupport,
    Compliment,
    Complaint,
    Spam,
    SecurityOrLegalEscalation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedInboxMessage {
    pub message_id: String,
    pub account_id: AccountId,
    pub platform: String,
    pub author_handle: String,
    pub interaction_type: InteractionType,
    pub content_text: String,
    pub category: ClassificationCategory,
    pub sentiment_score: f32, // -1.0 to 1.0
    pub drafted_reply: Option<String>,
    pub is_auto_reply_eligible: bool,
    pub requires_human_approval: bool,
    pub status: String, // "New", "PendingApproval", "Replied", "Archived"
    pub received_at: DateTime<Utc>,
}

pub struct EngagementAgent {
    #[allow(dead_code)]
    policy_engine: Arc<PolicyEngine>,
    #[allow(dead_code)]
    lm_gateway: Arc<LMGateway>,
    inbox: Arc<RwLock<HashMap<String, UnifiedInboxMessage>>>,
}

impl EngagementAgent {
    pub fn new(policy_engine: Arc<PolicyEngine>, lm_gateway: Arc<LMGateway>) -> Self {
        let mut initial_inbox = HashMap::new();
        let m1 = UnifiedInboxMessage {
            message_id: "inb-101".into(),
            account_id: AccountId::new(),
            platform: "instagram".into(),
            author_handle: "@creator_studio".into(),
            interaction_type: InteractionType::Comment,
            content_text: "Does OpenAccounts support scheduling 9:16 Instagram Reels automatically?".into(),
            category: ClassificationCategory::SalesLead,
            sentiment_score: 0.8,
            drafted_reply: Some("Yes! OpenAccounts includes full 9:16 Reel rendering, audio normalization, and automated publishing via official Graph API.".into()),
            is_auto_reply_eligible: true,
            requires_human_approval: false,
            status: "New".into(),
            received_at: Utc::now(),
        };
        let m2 = UnifiedInboxMessage {
            message_id: "inb-102".into(),
            account_id: AccountId::new(),
            platform: "x".into(),
            author_handle: "@enterprise_buyer".into(),
            interaction_type: InteractionType::DirectMessage,
            content_text: "We want to purchase an enterprise license and require custom legal terms for our multi-account setup.".into(),
            category: ClassificationCategory::SecurityOrLegalEscalation,
            sentiment_score: 0.5,
            drafted_reply: None,
            is_auto_reply_eligible: false,
            requires_human_approval: true,
            status: "PendingApproval".into(),
            received_at: Utc::now(),
        };
        initial_inbox.insert(m1.message_id.clone(), m1);
        initial_inbox.insert(m2.message_id.clone(), m2);

        Self {
            policy_engine,
            lm_gateway,
            inbox: Arc::new(RwLock::new(initial_inbox)),
        }
    }

    pub async fn list_inbox(&self) -> Vec<UnifiedInboxMessage> {
        self.inbox.read().await.values().cloned().collect()
    }

    pub async fn approve_and_send_reply(&self, message_id: &str, final_reply: &str) -> Result<()> {
        let mut inbox = self.inbox.write().await;
        let msg = inbox
            .get_mut(message_id)
            .ok_or_else(|| OpenAccountsError::Internal("Message not found".into()))?;

        msg.drafted_reply = Some(final_reply.to_string());
        msg.status = "Replied".into();
        info!("Approved and dispatched reply to {} on {}", msg.author_handle, msg.platform);
        Ok(())
    }
}
