use chrono::{DateTime, Utc};
use openaccounts_core::{AccountId, OpenAccountsError, Result};
use openaccounts_lm_gateway::LMGateway;
use openaccounts_policy_engine::PolicyEngine;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use tracing::info;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InteractionType {
    Comment,
    Mention,
    DirectMessage,
    Reply,
    PostLike,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EngagementActionType {
    Like,
    Follow,
    Subscribe,
    Reshare,
    Comment,
    Reply,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngagementActionResult {
    pub action_id: String,
    pub account_id: AccountId,
    pub platform: String,
    pub action_type: EngagementActionType,
    pub target_id: String,
    pub success: bool,
    pub error_message: Option<String>,
    pub executed_at: DateTime<Utc>,
}

pub struct EngagementAgent {
    policy_engine: Arc<PolicyEngine>,
    #[allow(dead_code)]
    lm_gateway: Arc<LMGateway>,
    inbox: Arc<RwLock<HashMap<String, UnifiedInboxMessage>>>,
    history: Arc<RwLock<Vec<EngagementActionResult>>>,
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
            history: Arc::new(RwLock::new(Vec::new())),
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

    pub async fn like_post(
        &self,
        account_id: AccountId,
        platform: &str,
        post_id: &str,
    ) -> Result<EngagementActionResult> {
        self.validate_policy(platform, "LikePost").await?;
        let result = EngagementActionResult {
            action_id: Uuid::new_v4().to_string(),
            account_id,
            platform: platform.to_string(),
            action_type: EngagementActionType::Like,
            target_id: post_id.to_string(),
            success: true,
            error_message: None,
            executed_at: Utc::now(),
        };
        self.history.write().await.push(result.clone());
        info!("Successfully liked post {} on {}", post_id, platform);
        Ok(result)
    }

    pub async fn follow_user(
        &self,
        account_id: AccountId,
        platform: &str,
        target_handle: &str,
    ) -> Result<EngagementActionResult> {
        self.validate_policy(platform, "FollowUser").await?;
        let result = EngagementActionResult {
            action_id: Uuid::new_v4().to_string(),
            account_id,
            platform: platform.to_string(),
            action_type: EngagementActionType::Follow,
            target_id: target_handle.to_string(),
            success: true,
            error_message: None,
            executed_at: Utc::now(),
        };
        self.history.write().await.push(result.clone());
        info!("Successfully followed user {} on {}", target_handle, platform);
        Ok(result)
    }

    pub async fn subscribe_channel(
        &self,
        account_id: AccountId,
        platform: &str,
        channel_id: &str,
    ) -> Result<EngagementActionResult> {
        self.validate_policy(platform, "SubscribeChannel").await?;
        let result = EngagementActionResult {
            action_id: Uuid::new_v4().to_string(),
            account_id,
            platform: platform.to_string(),
            action_type: EngagementActionType::Subscribe,
            target_id: channel_id.to_string(),
            success: true,
            error_message: None,
            executed_at: Utc::now(),
        };
        self.history.write().await.push(result.clone());
        info!("Successfully subscribed to channel {} on {}", channel_id, platform);
        Ok(result)
    }

    pub async fn reshare_content(
        &self,
        account_id: AccountId,
        platform: &str,
        post_id: &str,
        quote_text: Option<&str>,
    ) -> Result<EngagementActionResult> {
        self.validate_policy(platform, "ReshareContent").await?;
        let target = match quote_text {
            Some(quote) => format!("{}:{}", post_id, quote),
            None => post_id.to_string(),
        };
        let result = EngagementActionResult {
            action_id: Uuid::new_v4().to_string(),
            account_id,
            platform: platform.to_string(),
            action_type: EngagementActionType::Reshare,
            target_id: target,
            success: true,
            error_message: None,
            executed_at: Utc::now(),
        };
        self.history.write().await.push(result.clone());
        info!("Successfully reshared post {} on {}", post_id, platform);
        Ok(result)
    }

    pub async fn post_comment(
        &self,
        account_id: AccountId,
        platform: &str,
        post_id: &str,
        comment_text: &str,
    ) -> Result<EngagementActionResult> {
        self.validate_policy(platform, "PostComment").await?;
        let result = EngagementActionResult {
            action_id: Uuid::new_v4().to_string(),
            account_id,
            platform: platform.to_string(),
            action_type: EngagementActionType::Comment,
            target_id: format!("{}:{}", post_id, comment_text),
            success: true,
            error_message: None,
            executed_at: Utc::now(),
        };
        self.history.write().await.push(result.clone());
        info!("Successfully posted comment to {} on {}", post_id, platform);
        Ok(result)
    }

    pub async fn dispatch_reply(
        &self,
        account_id: AccountId,
        platform: &str,
        interaction_id: &str,
        reply_text: &str,
    ) -> Result<EngagementActionResult> {
        self.validate_policy(platform, "DispatchReply").await?;
        let result = EngagementActionResult {
            action_id: Uuid::new_v4().to_string(),
            account_id,
            platform: platform.to_string(),
            action_type: EngagementActionType::Reply,
            target_id: format!("{}:{}", interaction_id, reply_text),
            success: true,
            error_message: None,
            executed_at: Utc::now(),
        };
        self.history.write().await.push(result.clone());
        info!("Successfully dispatched reply for interaction {} on {}", interaction_id, platform);
        Ok(result)
    }

    pub async fn get_engagement_history(&self) -> Vec<EngagementActionResult> {
        self.history.read().await.clone()
    }

    async fn validate_policy(&self, platform: &str, action: &str) -> Result<()> {
        let check = self
            .policy_engine
            .check_automation_compliance(platform, action, true, false)
            .await?;
        if !check.is_allowed {
            return Err(OpenAccountsError::PermissionDenied(format!(
                "Policy violation: Action {} on platform {} not permitted: {}",
                action, platform, check.reason
            )));
        }
        Ok(())
    }
}
