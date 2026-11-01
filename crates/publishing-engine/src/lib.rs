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

    pub async fn approve_and_publish(&self, post_id: &str) -> Result<()> {
        let mut posts = self.posts.write().await;
        let post = posts
            .get_mut(post_id)
            .ok_or_else(|| OpenAccountsError::Internal("Post not found".into()))?;

        post.is_approved = true;
        post.status = "Published".into();
        info!("Approved and published post {} to {}", post_id, post.platform);
        Ok(())
    }

    pub async fn list_posts(&self) -> Vec<SocialPostPayload> {
        self.posts.read().await.values().cloned().collect()
    }
}
