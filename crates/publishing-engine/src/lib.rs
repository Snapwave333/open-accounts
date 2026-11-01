use chrono::{DateTime, Utc};
use openaccounts_core::{AccountId, OpenAccountsError, Result};
use openaccounts_platform_connectors::{normalize_platform, ConnectorRegistry, PublishRequest};
use openaccounts_policy_engine::PolicyEngine;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use tracing::{info, warn};

pub const STATUS_DRAFT: &str = "Draft";
pub const STATUS_AWAITING_APPROVAL: &str = "AwaitingApproval";
pub const STATUS_PUBLISHING: &str = "Publishing";
pub const STATUS_PUBLISHED: &str = "Published";
pub const STATUS_FAILED: &str = "Failed";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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
    pub status: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub remote_id: Option<String>,
    #[serde(default)]
    pub permalink: Option<String>,
    #[serde(default)]
    pub error_message: Option<String>,
    #[serde(default)]
    pub published_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformAuthStatus {
    pub platform: String,
    pub account_id: AccountId,
    pub is_authenticated: bool,
    pub scopes: Vec<String>,
    pub token_expires_at: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
    #[serde(default)]
    pub account_handle: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformPublishResult {
    pub post_id: String,
    pub platform: String,
    pub remote_media_id: String,
    pub remote_permalink: Option<String>,
    pub published_at: DateTime<Utc>,
}

pub struct PublishingEngine {
    policy_engine: Arc<PolicyEngine>,
    connectors: Arc<ConnectorRegistry>,
    posts: Arc<RwLock<HashMap<String, SocialPostPayload>>>,
}

fn blocks_new_post(status: &str) -> bool {
    matches!(status, STATUS_PUBLISHED | STATUS_PUBLISHING | STATUS_AWAITING_APPROVAL)
}

impl PublishingEngine {
    pub fn new(policy_engine: Arc<PolicyEngine>) -> Self {
        Self::with_connectors(policy_engine, Arc::new(ConnectorRegistry::from_env()))
    }

    pub fn with_connectors(policy_engine: Arc<PolicyEngine>, connectors: Arc<ConnectorRegistry>) -> Self {
        Self {
            policy_engine,
            connectors,
            posts: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn connectors(&self) -> Arc<ConnectorRegistry> {
        self.connectors.clone()
    }

    pub async fn verify_platform_auth(&self, platform: &str, account_id: &AccountId) -> Result<PlatformAuthStatus> {
        let platform = normalize_platform(platform);
        let base = PlatformAuthStatus {
            platform: platform.clone(),
            account_id: account_id.clone(),
            is_authenticated: false,
            scopes: vec![],
            token_expires_at: None,
            error_message: None,
            account_handle: None,
        };
        let connector = match self.connectors.connector_for(&platform).await {
            Ok(c) => c,
            Err(e) => return Ok(PlatformAuthStatus { error_message: Some(e.to_string()), ..base }),
        };
        let info = connector.verify_auth().await?;
        Ok(PlatformAuthStatus {
            is_authenticated: info.is_authenticated,
            scopes: info.scopes,
            error_message: info.error,
            account_handle: info.account_handle,
            ..base
        })
    }

    async fn check_policy(&self, platform: &str) -> Result<()> {
        let check = self
            .policy_engine
            .check_automation_compliance(platform, "PublishPost", true, false)
            .await?;
        if !check.is_allowed {
            return Err(OpenAccountsError::PermissionDenied(check.reason));
        }
        Ok(())
    }

    pub async fn queue_post(&self, mut post: SocialPostPayload) -> Result<String> {
        post.platform = normalize_platform(&post.platform);
        if post.idempotency_key.trim().is_empty() {
            return Err(OpenAccountsError::Automation("Posts require an idempotency key".into()));
        }
        self.check_policy(&post.platform).await?;

        let post_id = post.post_id.clone();
        let needs_approval = post.requires_human_approval && !post.is_approved;
        {
            let mut posts = self.posts.write().await;
            let conflict = posts
                .values()
                .any(|p| p.idempotency_key == post.idempotency_key && blocks_new_post(&p.status));
            if conflict {
                return Err(OpenAccountsError::Automation(
                    "Duplicate post prevented by idempotency key".into(),
                ));
            }
            if let Some(existing) = posts.get(&post_id) {
                post.retry_count = existing.retry_count;
            }
            post.remote_id = None;
            post.permalink = None;
            post.error_message = None;
            post.published_at = None;
            post.status = if needs_approval { STATUS_AWAITING_APPROVAL } else { STATUS_PUBLISHING }.into();
            posts.insert(post_id.clone(), post);
        }

        if needs_approval {
            info!("Post {} is awaiting human approval", post_id);
            return Ok(post_id);
        }
        self.publish_now(&post_id).await.map(|_| post_id)
    }

    async fn publish_now(&self, post_id: &str) -> Result<PlatformPublishResult> {
        let post = self
            .posts
            .read()
            .await
            .get(post_id)
            .cloned()
            .ok_or_else(|| OpenAccountsError::Internal("Post not found".into()))?;

        let request = PublishRequest {
            caption: post.caption.clone(),
            hashtags: post.hashtags.clone(),
            media: post.media_paths.clone(),
            title: post.title.clone(),
        };
        let outcome = match self.connectors.connector_for(&post.platform).await {
            Ok(connector) => connector.publish(request).await,
            Err(e) => Err(e),
        };

        let mut posts = self.posts.write().await;
        let stored = posts
            .get_mut(post_id)
            .ok_or_else(|| OpenAccountsError::Internal("Post not found".into()))?;
        match outcome {
            Ok(receipt) => {
                let now = Utc::now();
                stored.status = STATUS_PUBLISHED.into();
                stored.remote_id = Some(receipt.remote_id.clone());
                stored.permalink = receipt.permalink.clone();
                stored.error_message = None;
                stored.published_at = Some(now);
                info!("Published post {} to {} as {}", post_id, stored.platform, receipt.remote_id);
                Ok(PlatformPublishResult {
                    post_id: post_id.to_string(),
                    platform: stored.platform.clone(),
                    remote_media_id: receipt.remote_id,
                    remote_permalink: receipt.permalink,
                    published_at: now,
                })
            }
            Err(e) => {
                stored.status = STATUS_FAILED.into();
                stored.retry_count += 1;
                stored.error_message = Some(e.to_string());
                warn!("Publishing post {} to {} failed: {}", post_id, stored.platform, e);
                Err(e)
            }
        }
    }

    pub async fn approve_and_publish(&self, post_id: &str) -> Result<PlatformPublishResult> {
        let platform = {
            let mut posts = self.posts.write().await;
            let post = posts
                .get_mut(post_id)
                .ok_or_else(|| OpenAccountsError::Internal("Post not found".into()))?;
            match post.status.as_str() {
                STATUS_PUBLISHED => {
                    return Err(OpenAccountsError::Automation(format!("Post {post_id} is already published")))
                }
                STATUS_PUBLISHING => {
                    return Err(OpenAccountsError::Automation(format!("Post {post_id} is already being published")))
                }
                _ => {}
            }
            post.is_approved = true;
            post.status = STATUS_PUBLISHING.into();
            post.platform.clone()
        };

        if let Err(e) = self.check_policy(&platform).await {
            if let Some(post) = self.posts.write().await.get_mut(post_id) {
                post.status = STATUS_FAILED.into();
                post.error_message = Some(e.to_string());
            }
            return Err(e);
        }
        self.publish_now(post_id).await
    }

    pub async fn get_post(&self, post_id: &str) -> Option<SocialPostPayload> {
        self.posts.read().await.get(post_id).cloned()
    }

    pub async fn list_posts(&self) -> Vec<SocialPostPayload> {
        let mut posts: Vec<SocialPostPayload> = self.posts.read().await.values().cloned().collect();
        posts.sort_by(|a, b| b.scheduled_at.cmp(&a.scheduled_at).then_with(|| a.post_id.cmp(&b.post_id)));
        posts
    }
}
