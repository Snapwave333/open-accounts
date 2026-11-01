use crate::{
    http_client, join_url, json_str, json_u64, parse_rfc3339, read_json, transport_error, unsupported,
    InteractionKind, PlatformAuthInfo, PlatformConnector, PlatformCredentials, PublishRequest, RemoteActionReceipt,
    RemoteInteraction, RemoteMetrics, DEFAULT_TIMEOUT,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use openaccounts_core::{OpenAccountsError, Result};
use reqwest::Client;
use serde_json::{json, Value};
use std::collections::HashMap;
use tokio::sync::OnceCell;

pub const X_API_BASE: &str = "https://api.x.com/2";
const PLATFORM: &str = "X";

pub struct XConnector {
    base: String,
    creds: PlatformCredentials,
    client: Client,
    me: OnceCell<String>,
}

fn tweet_permalink(id: &str) -> String {
    format!("https://x.com/i/web/status/{id}")
}

impl XConnector {
    pub fn new(creds: PlatformCredentials) -> Self {
        Self::with_base_url(creds, X_API_BASE)
    }

    pub fn with_base_url(creds: PlatformCredentials, base: &str) -> Self {
        Self { base: base.to_string(), creds, client: http_client(), me: OnceCell::new() }
    }

    async fn get(&self, path: &str, query: &[(&str, String)], op: &str) -> Result<Value> {
        let resp = self
            .client
            .get(join_url(&self.base, path))
            .bearer_auth(&self.creds.access_token)
            .query(query)
            .timeout(DEFAULT_TIMEOUT)
            .send()
            .await
            .map_err(|e| transport_error(PLATFORM, op, e))?;
        read_json(resp, PLATFORM, op).await
    }

    async fn post(&self, path: &str, body: Value, op: &str) -> Result<Value> {
        let resp = self
            .client
            .post(join_url(&self.base, path))
            .bearer_auth(&self.creds.access_token)
            .json(&body)
            .timeout(DEFAULT_TIMEOUT)
            .send()
            .await
            .map_err(|e| transport_error(PLATFORM, op, e))?;
        read_json(resp, PLATFORM, op).await
    }

    async fn my_id(&self) -> Result<String> {
        self.me
            .get_or_try_init(|| async {
                if let Some(id) = self.creds.user_id.clone() {
                    return Ok(id);
                }
                let me = self.get("users/me", &[], "users/me").await?;
                json_str(&me["data"]["id"])
                    .ok_or_else(|| OpenAccountsError::Automation("X users/me returned no user id".into()))
            })
            .await
            .cloned()
    }

    async fn resolve_user_id(&self, target: &str) -> Result<String> {
        let handle = target.trim().trim_start_matches('@');
        if !handle.is_empty() && handle.chars().all(|c| c.is_ascii_digit()) {
            return Ok(handle.to_string());
        }
        let user = self
            .get(&format!("users/by/username/{handle}"), &[], "user lookup")
            .await?;
        json_str(&user["data"]["id"])
            .ok_or_else(|| OpenAccountsError::Automation(format!("X user @{handle} was not found")))
    }

    async fn create_tweet(&self, body: Value, op: &str) -> Result<RemoteActionReceipt> {
        let created = self.post("tweets", body, op).await?;
        let id = json_str(&created["data"]["id"])
            .ok_or_else(|| OpenAccountsError::Automation(format!("X {op} returned no tweet id")))?;
        Ok(RemoteActionReceipt { permalink: Some(tweet_permalink(&id)), remote_id: id })
    }
}

#[async_trait]
impl PlatformConnector for XConnector {
    fn platform(&self) -> &str {
        "x"
    }

    async fn verify_auth(&self) -> Result<PlatformAuthInfo> {
        match self.get("users/me", &[], "users/me").await {
            Ok(me) => Ok(PlatformAuthInfo {
                platform: "x".into(),
                account_handle: json_str(&me["data"]["username"]).map(|u| format!("@{u}")),
                account_remote_id: json_str(&me["data"]["id"]),
                scopes: vec![],
                is_authenticated: true,
                error: None,
            }),
            Err(e) => Ok(PlatformAuthInfo {
                platform: "x".into(),
                account_handle: None,
                account_remote_id: None,
                scopes: vec![],
                is_authenticated: false,
                error: Some(e.to_string()),
            }),
        }
    }

    async fn like(&self, target: &str) -> Result<RemoteActionReceipt> {
        let me = self.my_id().await?;
        let res = self
            .post(&format!("users/{me}/likes"), json!({ "tweet_id": target }), "like")
            .await?;
        if res["data"]["liked"] == false {
            return Err(OpenAccountsError::Automation(format!("X refused to like tweet {target}")));
        }
        Ok(RemoteActionReceipt { remote_id: target.to_string(), permalink: Some(tweet_permalink(target)) })
    }

    async fn follow(&self, target: &str) -> Result<RemoteActionReceipt> {
        let me = self.my_id().await?;
        let target_id = self.resolve_user_id(target).await?;
        self.post(
            &format!("users/{me}/following"),
            json!({ "target_user_id": target_id }),
            "follow",
        )
        .await?;
        let handle = target.trim().trim_start_matches('@');
        Ok(RemoteActionReceipt { remote_id: target_id, permalink: Some(format!("https://x.com/{handle}")) })
    }

    async fn subscribe(&self, _target: &str) -> Result<RemoteActionReceipt> {
        Err(unsupported("Subscribe", PLATFORM))
    }

    async fn reshare(&self, target: &str, quote: Option<&str>) -> Result<RemoteActionReceipt> {
        match quote.map(str::trim).filter(|q| !q.is_empty()) {
            Some(q) => self.create_tweet(json!({ "text": q, "quote_tweet_id": target }), "quote").await,
            None => {
                let me = self.my_id().await?;
                self.post(&format!("users/{me}/retweets"), json!({ "tweet_id": target }), "retweet")
                    .await?;
                Ok(RemoteActionReceipt { remote_id: target.to_string(), permalink: Some(tweet_permalink(target)) })
            }
        }
    }

    async fn comment(&self, target: &str, text: &str) -> Result<RemoteActionReceipt> {
        self.reply(target, text).await
    }

    async fn reply(&self, target: &str, text: &str) -> Result<RemoteActionReceipt> {
        self.create_tweet(
            json!({ "text": text, "reply": { "in_reply_to_tweet_id": target } }),
            "reply",
        )
        .await
    }

    async fn publish(&self, request: PublishRequest) -> Result<RemoteActionReceipt> {
        if !request.media.is_empty() {
            return Err(OpenAccountsError::Automation(
                "X media posts require the media upload endpoint, which OpenAccounts does not support yet; publish text only"
                    .into(),
            ));
        }
        let text = request.full_text();
        if text.is_empty() {
            return Err(OpenAccountsError::Automation("Cannot publish an empty post to X".into()));
        }
        self.create_tweet(json!({ "text": text }), "publish").await
    }

    async fn fetch_inbox(&self, since: Option<DateTime<Utc>>, limit: u32) -> Result<Vec<RemoteInteraction>> {
        let me = self.my_id().await?;
        let mut query = vec![
            ("max_results", limit.clamp(5, 100).to_string()),
            ("tweet.fields", "created_at,author_id,in_reply_to_user_id".to_string()),
            ("expansions", "author_id".to_string()),
            ("user.fields", "username".to_string()),
        ];
        if let Some(s) = since {
            query.push(("start_time", s.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)));
        }
        let res = self.get(&format!("users/{me}/mentions"), &query, "mentions").await?;

        let users: HashMap<String, String> = res["includes"]["users"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|u| Some((json_str(&u["id"])?, json_str(&u["username"])?)))
                    .collect()
            })
            .unwrap_or_default();

        let items = res["data"].as_array().cloned().unwrap_or_default();
        Ok(items
            .iter()
            .filter_map(|t| {
                let id = json_str(&t["id"])?;
                let author_id = json_str(&t["author_id"]).unwrap_or_default();
                let kind = if json_str(&t["in_reply_to_user_id"]).as_deref() == Some(me.as_str()) {
                    InteractionKind::Reply
                } else {
                    InteractionKind::Mention
                };
                Some(RemoteInteraction {
                    author_handle: users
                        .get(&author_id)
                        .map(|u| format!("@{u}"))
                        .unwrap_or(author_id),
                    kind,
                    text: json_str(&t["text"]).unwrap_or_default(),
                    permalink: Some(tweet_permalink(&id)),
                    created_at: parse_rfc3339(&t["created_at"]).unwrap_or_else(Utc::now),
                    remote_id: id,
                })
            })
            .take(limit as usize)
            .collect())
    }

    async fn fetch_metrics(&self) -> Result<RemoteMetrics> {
        let res = self
            .get("users/me", &[("user.fields", "public_metrics".to_string())], "metrics")
            .await?;
        let m = &res["data"]["public_metrics"];
        Ok(RemoteMetrics {
            followers: json_u64(&m["followers_count"]),
            following: json_u64(&m["following_count"]),
            posts: json_u64(&m["tweet_count"]),
            likes: json_u64(&m["like_count"]),
            raw: res,
            ..Default::default()
        })
    }
}
