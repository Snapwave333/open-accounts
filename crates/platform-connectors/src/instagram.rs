use crate::{
    http_client, join_url, json_str, json_u64, read_json, transport_error, unsupported, InteractionKind,
    PlatformAuthInfo, PlatformConnector, PlatformCredentials, PublishRequest, RemoteActionReceipt, RemoteInteraction,
    RemoteMetrics, DEFAULT_TIMEOUT,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use openaccounts_core::{OpenAccountsError, Result};
use reqwest::Client;
use serde_json::Value;
use std::time::Duration;

pub const INSTAGRAM_API_BASE: &str = "https://graph.facebook.com/v21.0";
const PLATFORM: &str = "Instagram";
const VIDEO_EXTENSIONS: &[&str] = &[".mp4", ".mov", ".m4v"];

pub struct InstagramConnector {
    base: String,
    creds: PlatformCredentials,
    client: Client,
    poll_interval: Duration,
    poll_attempts: u32,
}

fn parse_ig_time(v: &Value) -> Option<DateTime<Utc>> {
    let s = v.as_str()?;
    DateTime::parse_from_rfc3339(s)
        .or_else(|_| DateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%z"))
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

impl InstagramConnector {
    pub fn new(creds: PlatformCredentials) -> Self {
        Self::with_base_url(creds, INSTAGRAM_API_BASE)
    }

    pub fn with_base_url(creds: PlatformCredentials, base: &str) -> Self {
        Self {
            base: base.to_string(),
            creds,
            client: http_client(),
            poll_interval: Duration::from_secs(3),
            poll_attempts: 60,
        }
    }

    pub fn with_polling(mut self, interval: Duration, attempts: u32) -> Self {
        self.poll_interval = interval;
        self.poll_attempts = attempts.max(1);
        self
    }

    fn user_id(&self) -> Result<String> {
        self.creds
            .user_id
            .clone()
            .ok_or_else(|| OpenAccountsError::Automation("INSTAGRAM_USER_ID is not set".into()))
    }

    async fn get(&self, path: &str, query: &[(&str, String)], op: &str) -> Result<Value> {
        let resp = self
            .client
            .get(join_url(&self.base, path))
            .query(&[("access_token", self.creds.access_token.as_str())])
            .query(query)
            .timeout(DEFAULT_TIMEOUT)
            .send()
            .await
            .map_err(|e| transport_error(PLATFORM, op, e))?;
        read_json(resp, PLATFORM, op).await
    }

    async fn post(&self, path: &str, form: &[(&str, String)], op: &str) -> Result<Value> {
        let resp = self
            .client
            .post(join_url(&self.base, path))
            .query(&[("access_token", self.creds.access_token.as_str())])
            .form(form)
            .timeout(DEFAULT_TIMEOUT)
            .send()
            .await
            .map_err(|e| transport_error(PLATFORM, op, e))?;
        read_json(resp, PLATFORM, op).await
    }

    async fn wait_for_container(&self, container: &str) -> Result<()> {
        for _ in 0..self.poll_attempts {
            let status = self
                .get(container, &[("fields", "status_code".into())], "container status")
                .await?;
            match status["status_code"].as_str().unwrap_or_default() {
                "FINISHED" | "PUBLISHED" => return Ok(()),
                "ERROR" | "EXPIRED" => {
                    return Err(OpenAccountsError::Automation(format!(
                        "Instagram rejected media container {container}: {}",
                        status["status_code"]
                    )))
                }
                _ => tokio::time::sleep(self.poll_interval).await,
            }
        }
        Err(OpenAccountsError::Automation(format!(
            "Instagram media container {container} did not finish processing in time"
        )))
    }
}

#[async_trait]
impl PlatformConnector for InstagramConnector {
    fn platform(&self) -> &str {
        "instagram"
    }

    async fn verify_auth(&self) -> Result<PlatformAuthInfo> {
        let failure = |error: String| PlatformAuthInfo {
            platform: "instagram".into(),
            account_handle: None,
            account_remote_id: None,
            scopes: vec![],
            is_authenticated: false,
            error: Some(error),
        };
        let uid = match self.user_id() {
            Ok(u) => u,
            Err(e) => return Ok(failure(e.to_string())),
        };
        Ok(match self.get(&uid, &[("fields", "id,username".into())], "profile").await {
            Ok(v) => PlatformAuthInfo {
                platform: "instagram".into(),
                account_handle: json_str(&v["username"]).map(|u| format!("@{u}")),
                account_remote_id: json_str(&v["id"]),
                scopes: vec![],
                is_authenticated: true,
                error: None,
            },
            Err(e) => failure(e.to_string()),
        })
    }

    async fn like(&self, _target: &str) -> Result<RemoteActionReceipt> {
        Err(unsupported("Like", PLATFORM))
    }

    async fn follow(&self, _target: &str) -> Result<RemoteActionReceipt> {
        Err(unsupported("Follow", PLATFORM))
    }

    async fn subscribe(&self, _target: &str) -> Result<RemoteActionReceipt> {
        Err(unsupported("Subscribe", PLATFORM))
    }

    async fn reshare(&self, _target: &str, _quote: Option<&str>) -> Result<RemoteActionReceipt> {
        Err(unsupported("Reshare", PLATFORM))
    }

    async fn comment(&self, target: &str, text: &str) -> Result<RemoteActionReceipt> {
        let res = self
            .post(&format!("{target}/comments"), &[("message", text.to_string())], "comment")
            .await?;
        let id = json_str(&res["id"])
            .ok_or_else(|| OpenAccountsError::Automation("Instagram comment returned no id".into()))?;
        Ok(RemoteActionReceipt { remote_id: id, permalink: None })
    }

    async fn reply(&self, target: &str, text: &str) -> Result<RemoteActionReceipt> {
        let res = self
            .post(&format!("{target}/replies"), &[("message", text.to_string())], "reply")
            .await?;
        let id = json_str(&res["id"])
            .ok_or_else(|| OpenAccountsError::Automation("Instagram reply returned no id".into()))?;
        Ok(RemoteActionReceipt { remote_id: id, permalink: None })
    }

    async fn publish(&self, request: PublishRequest) -> Result<RemoteActionReceipt> {
        let uid = self.user_id()?;
        let media = match request.media.as_slice() {
            [] => {
                return Err(OpenAccountsError::Automation(
                    "Instagram posts require an image or video".into(),
                ))
            }
            [one] => one.trim().to_string(),
            _ => {
                return Err(OpenAccountsError::Automation(
                    "Instagram carousel publishing is not supported yet; attach a single media file".into(),
                ))
            }
        };
        if !media.to_lowercase().starts_with("https://") {
            return Err(OpenAccountsError::Automation(format!(
                "Instagram requires media hosted at a public https URL; {media} is not one"
            )));
        }
        let lower = media.to_lowercase();
        let path_part = lower.split(['?', '#']).next().unwrap_or_default();
        let is_video = VIDEO_EXTENSIONS.iter().any(|ext| path_part.ends_with(ext));

        let mut form = vec![("caption", request.full_text())];
        if is_video {
            form.push(("media_type", "REELS".into()));
            form.push(("video_url", media));
        } else {
            form.push(("image_url", media));
        }
        let container = self.post(&format!("{uid}/media"), &form, "media container").await?;
        let container_id = json_str(&container["id"])
            .ok_or_else(|| OpenAccountsError::Automation("Instagram returned no media container id".into()))?;

        self.wait_for_container(&container_id).await?;

        let published = self
            .post(&format!("{uid}/media_publish"), &[("creation_id", container_id)], "media publish")
            .await?;
        let id = json_str(&published["id"])
            .ok_or_else(|| OpenAccountsError::Automation("Instagram publish returned no media id".into()))?;
        let permalink = self
            .get(&id, &[("fields", "permalink".into())], "permalink")
            .await
            .ok()
            .and_then(|v| json_str(&v["permalink"]));
        Ok(RemoteActionReceipt { remote_id: id, permalink })
    }

    async fn fetch_inbox(&self, since: Option<DateTime<Utc>>, limit: u32) -> Result<Vec<RemoteInteraction>> {
        let uid = self.user_id()?;
        let media = self
            .get(
                &format!("{uid}/media"),
                &[("fields", "id,permalink,comments_count".into()), ("limit", "10".into())],
                "recent media",
            )
            .await?;
        let mut out = Vec::new();
        for item in media["data"].as_array().cloned().unwrap_or_default() {
            if out.len() >= limit as usize {
                break;
            }
            if json_u64(&item["comments_count"]) == Some(0) {
                continue;
            }
            let Some(media_id) = json_str(&item["id"]) else { continue };
            let permalink = json_str(&item["permalink"]);
            let comments = self
                .get(
                    &format!("{media_id}/comments"),
                    &[("fields", "id,text,username,timestamp".into()), ("limit", "50".into())],
                    "comments",
                )
                .await?;
            for c in comments["data"].as_array().cloned().unwrap_or_default() {
                let Some(id) = json_str(&c["id"]) else { continue };
                let created_at = parse_ig_time(&c["timestamp"]).unwrap_or_else(Utc::now);
                if since.map(|s| created_at <= s).unwrap_or(false) {
                    continue;
                }
                out.push(RemoteInteraction {
                    author_handle: json_str(&c["username"]).map(|u| format!("@{u}")).unwrap_or_else(|| "unknown".into()),
                    kind: InteractionKind::Comment,
                    text: json_str(&c["text"]).unwrap_or_default(),
                    permalink: permalink.clone(),
                    created_at,
                    remote_id: id,
                });
                if out.len() >= limit as usize {
                    break;
                }
            }
        }
        Ok(out)
    }

    async fn fetch_metrics(&self) -> Result<RemoteMetrics> {
        let uid = self.user_id()?;
        let res = self
            .get(&uid, &[("fields", "followers_count,follows_count,media_count".into())], "metrics")
            .await?;
        Ok(RemoteMetrics {
            followers: json_u64(&res["followers_count"]),
            following: json_u64(&res["follows_count"]),
            posts: json_u64(&res["media_count"]),
            raw: res,
            ..Default::default()
        })
    }
}
