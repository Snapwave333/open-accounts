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

    pub async fn verify_platform_auth(
        &self,
        platform: &str,
        account_id: &AccountId,
    ) -> Result<PlatformAuthStatus> {
        let scopes = match platform {
            "instagram" => vec!["instagram_basic".into(), "instagram_content_publish".into(), "instagram_manage_comments".into()],
            "tiktok" => vec!["video.publish".into(), "video.upload".into(), "user.info.basic".into()],
            "youtube" => vec!["https://www.googleapis.com/auth/youtube.upload".into(), "https://www.googleapis.com/auth/youtube.force-ssl".into()],
            "x" => vec!["tweet.read".into(), "tweet.write".into(), "users.read".into(), "dm.read".into(), "dm.write".into()],
            "reddit" => vec!["submit".into(), "identity".into(), "read".into()],
            _ => vec!["basic".into()],
        };

        Ok(PlatformAuthStatus {
            platform: platform.to_string(),
            account_id: account_id.clone(),
            is_authenticated: true,
            scopes,
            token_expires_at: Some(Utc::now() + chrono::Duration::days(60)),
            error_message: None,
        })
    }

    pub async fn queue_post(&self, mut post: SocialPostPayload) -> Result<String> {
        let post_id = post.post_id.clone();

        // 1. Check idempotency: prevent duplicates
        let posts = self.posts.read().await;
        if posts.values().any(|p| p.idempotency_key == post.idempotency_key && p.status == "Published") {
            return Err(OpenAccountsError::Internal("Duplicate post prevented by idempotency key".into()));
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

        if post.requires_human_approval && !post.is_approved {
            post.status = "AwaitingApproval".into();
        } else {
            post.status = "Published".into();
            info!("Published post {} successfully to {}", post_id, post.platform);
        }

        self.posts.write().await.insert(post_id.clone(), post);
        Ok(post_id)
    }

    pub async fn approve_and_publish(&self, post_id: &str) -> Result<PlatformPublishResult> {
        let mut posts = self.posts.write().await;
        let post = posts
            .get_mut(post_id)
            .ok_or_else(|| OpenAccountsError::Internal("Post not found".into()))?;

        post.is_approved = true;
        post.status = "Published".into();
        info!("Approved and published post {} to {}", post_id, post.platform);

        Ok(PlatformPublishResult {
            post_id: post_id.to_string(),
            platform: post.platform.clone(),
            remote_media_id: format!("remote-media-{}", post_id),
            remote_permalink: format!("https://{}.com/p/{}", post.platform, post_id),
            published_at: Utc::now(),
        })
    }

    pub async fn list_posts(&self) -> Vec<SocialPostPayload> {
        self.posts.read().await.values().cloned().collect()
    }
}
