use chrono::{DateTime, Utc};
use openaccounts_core::{AccountId, OpenAccountsError, Result};
use openaccounts_policy_engine::PolicyEngine;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocialPostPayload {
    pub post_id: String,
    pub account_id: AccountId,
    pub platform: String,
    pub caption: String,
    pub hashtags: Vec<String>,
    pub media_paths: Vec<String>,
    pub idempotency_key: String,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub requires_human_approval: bool,
    pub is_approved: bool,
    pub retry_count: u32,
    pub status: String, // "Draft", "AwaitingApproval", "Published", "Failed"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformAuthStatus {
    pub platform: String,
    pub account_id: AccountId,
    pub is_authenticated: bool,
    pub scopes: Vec<String>,
    pub token_expires_at: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformPublishResult {
    pub post_id: String,
    pub platform: String,
    pub remote_media_id: String,
    pub remote_permalink: String,
    pub published_at: DateTime<Utc>,
}

pub struct PublishingEngine {
    policy_engine: Arc<PolicyEngine>,
    posts: Arc<RwLock<HashMap<String, SocialPostPayload>>>,
}

impl PublishingEngine {
    pub fn new(policy_engine: Arc<PolicyEngine>) -> Self {
        Self {
            policy_engine,
            posts: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// OpenAccounts holds no platform OAuth tokens, so it cannot vouch for any
    /// account's publishing scopes. Publishing runs through the studio's own
    /// supervised jobs, which use their own credentials.
    pub async fn verify_platform_auth(
        &self,
        platform: &str,
        account_id: &AccountId,
    ) -> Result<PlatformAuthStatus> {
        Err(OpenAccountsError::NotConfigured(format!(
            "No {platform} API credentials are connected for account {account_id}; \
             publishing authorization cannot be verified"
        )))
    }

    /// Queues a post. Approved posts are marked "Approved", never "Published":
    /// nothing in this engine sends posts to a platform.
    pub async fn queue_post(&self, mut post: SocialPostPayload) -> Result<String> {
        let post_id = post.post_id.clone();

        // 1. Idempotency: one post per key, whatever its state.
        let posts = self.posts.read().await;
        if posts.values().any(|p| p.idempotency_key == post.idempotency_key) {
            return Err(OpenAccountsError::Internal(
                "Duplicate post prevented by idempotency key".into(),
            ));
        }
        drop(posts);

        // 2. Policy compliance check
        let check = self
            .policy_engine
            .check_automation_compliance(&post.platform, "PublishPost", true, false)
            .await?;

        if !check.is_allowed {
            return Err(OpenAccountsError::PermissionDenied(check.reason));
        }

        post.status = if post.requires_human_approval && !post.is_approved {
            "AwaitingApproval".into()
        } else {
            "Approved".into()
        };
        info!("Queued post {} for {} ({})", post_id, post.platform, post.status);

        self.posts.write().await.insert(post_id.clone(), post);
        Ok(post_id)
    }

    /// No direct platform publisher is connected, so this refuses rather than
    /// marking the post published with an invented permalink.
    pub async fn approve_and_publish(&self, post_id: &str) -> Result<PlatformPublishResult> {
        let posts = self.posts.read().await;
        let post = posts
            .get(post_id)
            .ok_or_else(|| OpenAccountsError::Internal(format!("Post {post_id} not found")))?;

        Err(OpenAccountsError::NotConfigured(format!(
            "No direct {} publisher is connected; post {post_id} was not published. \
             Studio posts are published by the supervised 'Social publish' job.",
            post.platform
        )))
    }

    pub async fn list_posts(&self) -> Vec<SocialPostPayload> {
        self.posts.read().await.values().cloned().collect()
    }
}
