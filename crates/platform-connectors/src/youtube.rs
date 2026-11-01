use crate::{
    http_client, is_remote_url, join_url, json_str, json_u64, parse_rfc3339, read_json, transport_error,
    truncate_chars, unsupported, InteractionKind, PlatformAuthInfo, PlatformConnector, PlatformCredentials,
    PublishRequest, RemoteActionReceipt, RemoteInteraction, RemoteMetrics, DEFAULT_TIMEOUT,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use openaccounts_core::{OpenAccountsError, Result};
use reqwest::Client;
use serde_json::{json, Value};
use std::time::Duration;
use tokio::sync::OnceCell;

pub const YOUTUBE_API_BASE: &str = "https://www.googleapis.com/youtube/v3";
pub const YOUTUBE_UPLOAD_BASE: &str = "https://www.googleapis.com/upload/youtube/v3";
const PLATFORM: &str = "YouTube";
const UPLOAD_TIMEOUT: Duration = Duration::from_secs(60 * 30);

pub struct YouTubeConnector {
    api_base: String,
    upload_base: String,
    creds: PlatformCredentials,
    client: Client,
    channel: OnceCell<String>,
}

fn sanitize_title(raw: &str) -> String {
    let cleaned: String = raw.chars().filter(|c| *c != '<' && *c != '>').collect();
    let first_line = cleaned.lines().next().unwrap_or_default().trim().to_string();
    truncate_chars(&first_line, 100)
}

impl YouTubeConnector {
    pub fn new(creds: PlatformCredentials) -> Self {
        Self::with_base_urls(creds, YOUTUBE_API_BASE, YOUTUBE_UPLOAD_BASE)
    }

    pub fn with_base_urls(creds: PlatformCredentials, api_base: &str, upload_base: &str) -> Self {
        Self {
            api_base: api_base.to_string(),
            upload_base: upload_base.to_string(),
            creds,
            client: http_client(),
            channel: OnceCell::new(),
        }
    }

    async fn get(&self, path: &str, query: &[(&str, String)], op: &str) -> Result<Value> {
        let resp = self
            .client
            .get(join_url(&self.api_base, path))
            .bearer_auth(&self.creds.access_token)
            .query(query)
            .timeout(DEFAULT_TIMEOUT)
            .send()
            .await
            .map_err(|e| transport_error(PLATFORM, op, e))?;
        read_json(resp, PLATFORM, op).await
    }

    async fn post(&self, path: &str, query: &[(&str, String)], body: Option<Value>, op: &str) -> Result<Value> {
        let mut req = self
            .client
            .post(join_url(&self.api_base, path))
            .bearer_auth(&self.creds.access_token)
            .query(query)
            .timeout(DEFAULT_TIMEOUT);
        req = match body {
            Some(b) => req.json(&b),
            None => req.header("Content-Length", "0"),
        };
        let resp = req.send().await.map_err(|e| transport_error(PLATFORM, op, e))?;
        read_json(resp, PLATFORM, op).await
    }

    async fn channel_id(&self) -> Result<String> {
        self.channel
            .get_or_try_init(|| async {
                if let Some(id) = self.creds.user_id.clone() {
                    return Ok(id);
                }
                let res = self
                    .get("channels", &[("part", "id".into()), ("mine", "true".into())], "channel lookup")
                    .await?;
                json_str(&res["items"][0]["id"]).ok_or_else(|| {
                    OpenAccountsError::Automation("YouTube token is not linked to any channel".into())
                })
            })
            .await
            .cloned()
    }

    async fn subscribe_to(&self, channel_id: &str) -> Result<RemoteActionReceipt> {
        let channel_id = channel_id.trim().trim_start_matches('@');
        let body = json!({
            "snippet": { "resourceId": { "kind": "youtube#channel", "channelId": channel_id } }
        });
        let res = self
            .post("subscriptions", &[("part", "snippet".into())], Some(body), "subscribe")
            .await?;
        Ok(RemoteActionReceipt {
            remote_id: json_str(&res["id"]).unwrap_or_else(|| channel_id.to_string()),
            permalink: Some(format!("https://www.youtube.com/channel/{channel_id}")),
        })
    }
}

#[async_trait]
impl PlatformConnector for YouTubeConnector {
    fn platform(&self) -> &str {
        "youtube"
    }

    async fn verify_auth(&self) -> Result<PlatformAuthInfo> {
        let res = self
            .get("channels", &[("part", "snippet".into()), ("mine", "true".into())], "channels")
            .await;
        Ok(match res {
            Ok(v) if v["items"][0]["id"].is_string() => {
                let item = &v["items"][0];
                PlatformAuthInfo {
                    platform: "youtube".into(),
                    account_handle: json_str(&item["snippet"]["customUrl"]).or_else(|| json_str(&item["snippet"]["title"])),
                    account_remote_id: json_str(&item["id"]),
                    scopes: vec![],
                    is_authenticated: true,
                    error: None,
                }
            }
            Ok(_) => PlatformAuthInfo {
                platform: "youtube".into(),
                account_handle: None,
                account_remote_id: None,
                scopes: vec![],
                is_authenticated: false,
                error: Some("Token is valid but not linked to a YouTube channel".into()),
            },
            Err(e) => PlatformAuthInfo {
                platform: "youtube".into(),
                account_handle: None,
                account_remote_id: None,
                scopes: vec![],
                is_authenticated: false,
                error: Some(e.to_string()),
            },
        })
    }

    async fn like(&self, target: &str) -> Result<RemoteActionReceipt> {
        self.post(
            "videos/rate",
            &[("id", target.to_string()), ("rating", "like".into())],
            None,
            "like",
        )
        .await?;
        Ok(RemoteActionReceipt {
            remote_id: target.to_string(),
            permalink: Some(format!("https://www.youtube.com/watch?v={target}")),
        })
    }

    async fn follow(&self, target: &str) -> Result<RemoteActionReceipt> {
        self.subscribe_to(target).await
    }

    async fn subscribe(&self, target: &str) -> Result<RemoteActionReceipt> {
        self.subscribe_to(target).await
    }

    async fn reshare(&self, _target: &str, _quote: Option<&str>) -> Result<RemoteActionReceipt> {
        Err(unsupported("Reshare", PLATFORM))
    }

    async fn comment(&self, target: &str, text: &str) -> Result<RemoteActionReceipt> {
        let body = json!({
            "snippet": { "videoId": target, "topLevelComment": { "snippet": { "textOriginal": text } } }
        });
        let res = self
            .post("commentThreads", &[("part", "snippet".into())], Some(body), "comment")
            .await?;
        let id = json_str(&res["id"])
            .ok_or_else(|| OpenAccountsError::Automation("YouTube comment returned no id".into()))?;
        Ok(RemoteActionReceipt {
            permalink: Some(format!("https://www.youtube.com/watch?v={target}&lc={id}")),
            remote_id: id,
        })
    }

    async fn reply(&self, target: &str, text: &str) -> Result<RemoteActionReceipt> {
        let body = json!({ "snippet": { "parentId": target, "textOriginal": text } });
        let res = self
            .post("comments", &[("part", "snippet".into())], Some(body), "reply")
            .await?;
        let id = json_str(&res["id"])
            .ok_or_else(|| OpenAccountsError::Automation("YouTube reply returned no id".into()))?;
        Ok(RemoteActionReceipt { remote_id: id, permalink: None })
    }

    async fn publish(&self, request: PublishRequest) -> Result<RemoteActionReceipt> {
        let path = request
            .media
            .first()
            .ok_or_else(|| OpenAccountsError::Automation("YouTube publishing requires a video file".into()))?;
        if is_remote_url(path) {
            return Err(OpenAccountsError::Automation(
                "YouTube uploads require a local video file path, not a URL".into(),
            ));
        }
        let bytes = tokio::fs::read(path)
            .await
            .map_err(|e| OpenAccountsError::Automation(format!("Cannot read video file {path}: {e}")))?;

        let title = sanitize_title(request.title.as_deref().unwrap_or(&request.caption));
        if title.is_empty() {
            return Err(OpenAccountsError::Automation("YouTube uploads require a title".into()));
        }
        let tags: Vec<String> = request
            .hashtags
            .iter()
            .map(|h| h.trim().trim_start_matches('#').to_string())
            .filter(|h| !h.is_empty())
            .collect();
        let privacy = self
            .creds
            .extra
            .get("privacy_status")
            .cloned()
            .unwrap_or_else(|| "public".into());
        let metadata = json!({
            "snippet": { "title": title, "description": request.full_text(), "tags": tags },
            "status": { "privacyStatus": privacy, "selfDeclaredMadeForKids": false }
        });

        let init = self
            .client
            .post(join_url(&self.upload_base, "videos"))
            .bearer_auth(&self.creds.access_token)
            .query(&[("uploadType", "resumable"), ("part", "snippet,status")])
            .header("X-Upload-Content-Type", "video/*")
            .header("X-Upload-Content-Length", bytes.len().to_string())
            .json(&metadata)
            .timeout(DEFAULT_TIMEOUT)
            .send()
            .await
            .map_err(|e| transport_error(PLATFORM, "upload init", e))?;
        let status = init.status();
        let location = init
            .headers()
            .get("Location")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        if !status.is_success() {
            read_json(init, PLATFORM, "upload init").await?;
        }
        let location = location.ok_or_else(|| {
            OpenAccountsError::Automation("YouTube did not return a resumable upload URL".into())
        })?;

        let uploaded = self
            .client
            .put(location)
            .bearer_auth(&self.creds.access_token)
            .header("Content-Type", "video/*")
            .body(bytes)
            .timeout(UPLOAD_TIMEOUT)
            .send()
            .await
            .map_err(|e| transport_error(PLATFORM, "upload", e))?;
        let video = read_json(uploaded, PLATFORM, "upload").await?;
        let id = json_str(&video["id"])
            .ok_or_else(|| OpenAccountsError::Automation("YouTube upload returned no video id".into()))?;
        Ok(RemoteActionReceipt { permalink: Some(format!("https://youtu.be/{id}")), remote_id: id })
    }

    async fn fetch_inbox(&self, since: Option<DateTime<Utc>>, limit: u32) -> Result<Vec<RemoteInteraction>> {
        let channel = self.channel_id().await?;
        let res = self
            .get(
                "commentThreads",
                &[
                    ("part", "snippet".into()),
                    ("allThreadsRelatedToChannelId", channel),
                    ("maxResults", limit.clamp(1, 100).to_string()),
                    ("order", "time".into()),
                    ("textFormat", "plainText".into()),
                ],
                "comment threads",
            )
            .await?;
        let items = res["items"].as_array().cloned().unwrap_or_default();
        Ok(items
            .iter()
            .filter_map(|thread| {
                let top = &thread["snippet"]["topLevelComment"];
                let snippet = &top["snippet"];
                let id = json_str(&top["id"]).or_else(|| json_str(&thread["id"]))?;
                let created_at = parse_rfc3339(&snippet["publishedAt"]).unwrap_or_else(Utc::now);
                if since.map(|s| created_at <= s).unwrap_or(false) {
                    return None;
                }
                let video = json_str(&thread["snippet"]["videoId"]);
                Some(RemoteInteraction {
                    author_handle: json_str(&snippet["authorDisplayName"]).unwrap_or_else(|| "unknown".into()),
                    kind: InteractionKind::Comment,
                    text: json_str(&snippet["textOriginal"])
                        .or_else(|| json_str(&snippet["textDisplay"]))
                        .unwrap_or_default(),
                    permalink: video.map(|v| format!("https://www.youtube.com/watch?v={v}&lc={id}")),
                    created_at,
                    remote_id: id,
                })
            })
            .collect())
    }

    async fn fetch_metrics(&self) -> Result<RemoteMetrics> {
        let res = self
            .get("channels", &[("part", "statistics".into()), ("mine", "true".into())], "statistics")
            .await?;
        let stats = &res["items"][0]["statistics"];
        if stats.is_null() {
            return Err(OpenAccountsError::Automation(
                "YouTube token is not linked to any channel".into(),
            ));
        }
        Ok(RemoteMetrics {
            followers: json_u64(&stats["subscriberCount"]),
            posts: json_u64(&stats["videoCount"]),
            views: json_u64(&stats["viewCount"]),
            raw: res,
            ..Default::default()
        })
    }
}
