use chrono::{DateTime, Utc};
use openaccounts_account_registry::AccountRegistry;
use openaccounts_core::{python_interpreter, studio_root, AccountId, OpenAccountsError, Result};
use openaccounts_lm_gateway::LMGateway;
use openaccounts_policy_engine::PolicyEngine;
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, process::Stdio, sync::Arc, time::Duration};
use tokio::{io::AsyncWriteExt, process::Command, sync::RwLock};
use tracing::info;

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
    /// The registry account that owns the platform, when one is imported.
    pub account_id: Option<AccountId>,
    pub platform: String,
    pub author_handle: String,
    pub interaction_type: InteractionType,
    pub content_text: String,
    pub category: ClassificationCategory,
    /// -1.0 to 1.0; `None` because nothing scores sentiment yet.
    pub sentiment_score: Option<f32>,
    pub drafted_reply: Option<String>,
    pub is_auto_reply_eligible: bool,
    pub requires_human_approval: bool,
    pub status: String, // "New", "PendingApproval", "Replied", "Archived"
    /// The studio's comment queue does not record when a comment arrived.
    pub received_at: Option<DateTime<Utc>>,
    /// Where the message actually came from (the studio's comment state file).
    pub source_url: Option<String>,
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
    account_registry: Arc<AccountRegistry>,
    studio_root: PathBuf,
    history: Arc<RwLock<Vec<EngagementActionResult>>>,
}

impl EngagementAgent {
    pub fn new(
        policy_engine: Arc<PolicyEngine>,
        lm_gateway: Arc<LMGateway>,
        account_registry: Arc<AccountRegistry>,
    ) -> Self {
        Self {
            policy_engine,
            lm_gateway,
            account_registry,
            studio_root: studio_root(),
            history: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Points the agent at a different studio workspace.
    pub fn with_studio_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.studio_root = root.into();
        self
    }

    /// The real inbox: the pending comment drafts the studio's `comments.py`
    /// has produced and not yet sent. Read from `insights/comments.json` on
    /// every call, so the app shows the actual queue and nothing else.
    pub async fn list_inbox(&self) -> Result<Vec<UnifiedInboxMessage>> {
        let path = self.studio_root.join("insights").join("comments.json");
        let raw = match tokio::fs::read_to_string(&path).await {
            Ok(raw) => raw,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => {
                return Err(OpenAccountsError::Internal(format!(
                    "Cannot read comment inbox at {}: {e}",
                    path.display()
                )))
            }
        };

        let parsed: serde_json::Value = serde_json::from_str(&raw).map_err(|e| {
            OpenAccountsError::Internal(format!(
                "Comment inbox at {} is not valid JSON: {e}",
                path.display()
            ))
        })?;

        let pending = parsed
            .get("pending")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();

        let accounts = self.account_registry.list_accounts().await.unwrap_or_default();

        let mut messages = Vec::with_capacity(pending.len());
        for (id, entry) in pending {
            let platform = entry
                .get("platform")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            let author = entry
                .get("author")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            let text = entry
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let draft = entry
                .get("draft")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let url = entry
                .get("url")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let category_raw = entry
                .get("category")
                .and_then(|v| v.as_str())
                .unwrap_or("other")
                .to_lowercase();

            let category = match category_raw.as_str() {
                "question" | "suggestion" => ClassificationCategory::GeneralInquiry,
                "praise" | "compliment" => ClassificationCategory::Compliment,
                "criticism" => ClassificationCategory::Complaint,
                "spam" => ClassificationCategory::Spam,
                "toxic" | "personal_info" => ClassificationCategory::SecurityOrLegalEscalation,
                "outbound" => ClassificationCategory::SalesLead,
                _ => ClassificationCategory::CustomerSupport,
            };

            let account_id = accounts
                .iter()
                .find(|a| a.platform.to_string().to_lowercase() == platform.to_lowercase())
                .map(|a| a.id.clone());

            messages.push(UnifiedInboxMessage {
                message_id: id,
                account_id,
                platform,
                author_handle: author,
                interaction_type: InteractionType::Comment,
                content_text: text,
                category,
                sentiment_score: None,
                drafted_reply: draft.clone(),
                // Every reply goes through the studio's own approval path,
                // which re-checks safety rules and daily limits.
                is_auto_reply_eligible: false,
                requires_human_approval: true,
                status: if draft.is_some() {
                    "PendingApproval".into()
                } else {
                    "New".into()
                },
                received_at: None,
                source_url: url,
            });
        }

        messages.sort_by(|a, b| a.message_id.cmp(&b.message_id));
        Ok(messages)
    }

    /// Sends an approved reply through the studio's real approval path
    /// (`comments.post_reply`), which applies the safety filter, the daily
    /// per-platform reply caps and the real platform client. The reply text is
    /// passed as JSON on stdin so no shell quoting can corrupt it.
    pub async fn approve_and_send_reply(&self, message_id: &str, final_reply: &str) -> Result<()> {
        if !self.studio_root.is_dir() {
            return Err(OpenAccountsError::NotConfigured(format!(
                "Studio workspace not found at {}",
                self.studio_root.display()
            )));
        }

        let payload = serde_json::json!({ "id": message_id, "text": final_reply });
        let script = "import json,sys,comments; a=json.load(sys.stdin); print(comments.post_reply(a['id'], a['text']))";

        let mut child = Command::new(python_interpreter())
            .arg("-c")
            .arg(script)
            .current_dir(&self.studio_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .creation_flags(openaccounts_core::no_window_flag())
            .spawn()
            .map_err(|e| {
                OpenAccountsError::Automation(format!("Could not start the studio reply job: {e}"))
            })?;

        child
            .stdin
            .as_mut()
            .expect("stdin was piped")
            .write_all(serde_json::to_string(&payload)?.as_bytes())
            .await
            .map_err(|e| OpenAccountsError::Automation(format!("Could not pass reply to job: {e}")))?;
        drop(child.stdin.take());

        let output = tokio::time::timeout(Duration::from_secs(180), child.wait_with_output())
            .await
            .map_err(|_| OpenAccountsError::Automation("Reply job timed out after 180s".into()))?
            .map_err(|e| OpenAccountsError::Automation(format!("Reply job failed: {e}")))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let summary = stdout.trim().lines().last().unwrap_or_default().to_string();

        if !output.status.success() {
            let fallback = if stderr.trim().is_empty() {
                stdout.trim().to_string()
            } else {
                stderr.trim().to_string()
            };
            return Err(OpenAccountsError::Automation(format!(
                "Reply job exited with {:?}: {}",
                output.status.code(),
                if summary.is_empty() { fallback } else { summary }
            )));
        }

        // post_reply reports refusals (safety rules, daily cap) as text
        // without failing; surface them instead of pretending success.
        let refused = summary.starts_with("Not sent") || summary.starts_with("That draft");
        if refused {
            return Err(OpenAccountsError::HumanInterventionRequired(summary));
        }

        info!("Reply dispatched for {message_id}: {summary}");
        Ok(())
    }

    /// Individual like/follow/subscribe actions are not exposed as one-off
    /// commands by the studio's engagement job (`engage.py run` performs them
    /// as part of its platform flows). Refuse rather than record a fake win.
    fn not_available(action: &str, platform: &str, target: &str) -> OpenAccountsError {
        OpenAccountsError::NotConfigured(format!(
            "Single '{action}' on {platform} targeting {target} is not available as a direct command; \
             real engagement runs through the supervised 'engage.py run' job"
        ))
    }

    pub async fn like_post(
        &self,
        account_id: AccountId,
        platform: &str,
        post_id: &str,
    ) -> Result<EngagementActionResult> {
        let _ = account_id;
        self.validate_policy(platform, "LikePost").await?;
        Err(Self::not_available("LikePost", platform, post_id))
    }

    pub async fn follow_user(
        &self,
        account_id: AccountId,
        platform: &str,
        target_handle: &str,
    ) -> Result<EngagementActionResult> {
        let _ = account_id;
        self.validate_policy(platform, "FollowUser").await?;
        Err(Self::not_available("FollowUser", platform, target_handle))
    }

    pub async fn subscribe_channel(
        &self,
        account_id: AccountId,
        platform: &str,
        channel_id: &str,
    ) -> Result<EngagementActionResult> {
        let _ = account_id;
        self.validate_policy(platform, "SubscribeChannel").await?;
        Err(Self::not_available("SubscribeChannel", platform, channel_id))
    }

    pub async fn reshare_content(
        &self,
        account_id: AccountId,
        platform: &str,
        post_id: &str,
        quote_text: Option<&str>,
    ) -> Result<EngagementActionResult> {
        let _ = account_id;
        self.validate_policy(platform, "ReshareContent").await?;
        let target = match quote_text {
            Some(q) => format!("{post_id}:{q}"),
            None => post_id.to_string(),
        };
        Err(Self::not_available("ReshareContent", platform, &target))
    }

    pub async fn post_comment(
        &self,
        account_id: AccountId,
        platform: &str,
        post_id: &str,
        comment_text: &str,
    ) -> Result<EngagementActionResult> {
        let _ = account_id;
        self.validate_policy(platform, "PostComment").await?;
        Err(Self::not_available(
            "PostComment",
            platform,
            &format!("{post_id}:{comment_text}"),
        ))
    }

    pub async fn dispatch_reply(
        &self,
        account_id: AccountId,
        platform: &str,
        interaction_id: &str,
        reply_text: &str,
    ) -> Result<EngagementActionResult> {
        let _ = account_id;
        self.validate_policy(platform, "DispatchReply").await?;
        Err(Self::not_available(
            "DispatchReply",
            platform,
            &format!("{interaction_id}:{reply_text}"),
        ))
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
