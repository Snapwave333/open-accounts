use crate::{
    http_client, is_remote_url, join_url, json_str, json_u64, read_json, transport_error, truncate_chars,
    InteractionKind, PlatformAuthInfo, PlatformConnector, PlatformCredentials, PublishRequest, RemoteActionReceipt,
    RemoteInteraction, RemoteMetrics, DEFAULT_TIMEOUT,
};
use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use openaccounts_core::{OpenAccountsError, Result};
use reqwest::{Client, RequestBuilder};
use serde_json::{json, Value};

pub const REDDIT_API_BASE: &str = "https://oauth.reddit.com";
pub const REDDIT_DEFAULT_USER_AGENT: &str = concat!("desktop:openaccounts:", env!("CARGO_PKG_VERSION"));
const PLATFORM: &str = "Reddit";

pub struct RedditConnector {
    base: String,
    creds: PlatformCredentials,
    client: Client,
}

pub fn reddit_fullname(target: &str, default_prefix: &str) -> String {
    let t = target.trim();
    let has_prefix = t.len() > 3 && t.starts_with('t') && t.as_bytes()[2] == b'_' && t.as_bytes()[1].is_ascii_digit();
    if has_prefix {
        t.to_string()
    } else {
        format!("{default_prefix}{t}")
    }
}

pub fn split_subreddit_title(title: &str) -> Option<(String, String)> {
    let t = title.trim();
    let rest = t.strip_prefix("r/").or_else(|| t.strip_prefix("/r/"))?;
    let (sub, remainder) = match rest.find(|c: char| c == ':' || c.is_whitespace()) {
        Some(idx) => (&rest[..idx], rest[idx..].trim_start_matches(':').trim()),
        None => (rest, ""),
    };
    if sub.is_empty() {
        return None;
    }
    Some((sub.to_string(), remainder.to_string()))
}

fn api_errors(res: &Value) -> Option<String> {
    let errors = res["json"]["errors"].as_array()?;
    if errors.is_empty() {
        return None;
    }
    Some(
        errors
            .iter()
            .map(|e| {
                e.as_array()
                    .map(|parts| parts.iter().filter_map(|p| p.as_str()).collect::<Vec<_>>().join(": "))
                    .unwrap_or_else(|| e.to_string())
            })
            .collect::<Vec<_>>()
            .join("; "),
    )
}

impl RedditConnector {
    pub fn new(creds: PlatformCredentials) -> Self {
        Self::with_base_url(creds, REDDIT_API_BASE)
    }

    pub fn with_base_url(creds: PlatformCredentials, base: &str) -> Self {
        Self { base: base.to_string(), creds, client: http_client() }
    }

    fn user_agent(&self) -> String {
        self.creds
            .extra
            .get("user_agent")
            .cloned()
            .unwrap_or_else(|| REDDIT_DEFAULT_USER_AGENT.to_string())
    }

    fn authed(&self, req: RequestBuilder) -> RequestBuilder {
        req.bearer_auth(&self.creds.access_token)
            .header("User-Agent", self.user_agent())
            .timeout(DEFAULT_TIMEOUT)
    }

    async fn get(&self, path: &str, query: &[(&str, String)], op: &str) -> Result<Value> {
        let resp = self
            .authed(self.client.get(join_url(&self.base, path)))
            .query(query)
            .send()
            .await
            .map_err(|e| transport_error(PLATFORM, op, e))?;
        read_json(resp, PLATFORM, op).await
    }

    async fn post_form(&self, path: &str, form: &[(&str, String)], op: &str) -> Result<Value> {
        let resp = self
            .authed(self.client.post(join_url(&self.base, path)))
            .form(form)
            .send()
            .await
            .map_err(|e| transport_error(PLATFORM, op, e))?;
        let res = read_json(resp, PLATFORM, op).await?;
        if let Some(err) = api_errors(&res) {
            return Err(OpenAccountsError::Automation(format!("Reddit {op} rejected: {err}")));
        }
        Ok(res)
    }

    async fn post_thing(&self, thing_id: String, text: &str, op: &str) -> Result<RemoteActionReceipt> {
        let res = self
            .post_form(
                "api/comment",
                &[("api_type", "json".into()), ("thing_id", thing_id), ("text", text.to_string())],
                op,
            )
            .await?;
        let thing = &res["json"]["data"]["things"][0]["data"];
        let id = json_str(&thing["name"])
            .or_else(|| json_str(&thing["id"]))
            .ok_or_else(|| OpenAccountsError::Automation(format!("Reddit {op} returned no comment id")))?;
        Ok(RemoteActionReceipt {
            remote_id: id,
            permalink: json_str(&thing["permalink"]).map(|p| format!("https://www.reddit.com{p}")),
        })
    }

    async fn submit(&self, form: Vec<(&str, String)>, op: &str) -> Result<RemoteActionReceipt> {
        let res = self.post_form("api/submit", &form, op).await?;
        let data = &res["json"]["data"];
        let id = json_str(&data["name"])
            .or_else(|| json_str(&data["id"]))
            .ok_or_else(|| OpenAccountsError::Automation(format!("Reddit {op} returned no post id")))?;
        Ok(RemoteActionReceipt { remote_id: id, permalink: json_str(&data["url"]) })
    }

    fn default_subreddit(&self) -> Option<String> {
        self.creds
            .extra
            .get("default_subreddit")
            .map(|s| s.trim().trim_start_matches("/r/").trim_start_matches("r/").to_string())
            .filter(|s| !s.is_empty())
    }
}

#[async_trait]
impl PlatformConnector for RedditConnector {
    fn platform(&self) -> &str {
        "reddit"
    }

    async fn verify_auth(&self) -> Result<PlatformAuthInfo> {
        Ok(match self.get("api/v1/me", &[], "me").await {
            Ok(me) if me["name"].is_string() => PlatformAuthInfo {
                platform: "reddit".into(),
                account_handle: json_str(&me["name"]).map(|n| format!("u/{n}")),
                account_remote_id: json_str(&me["id"]),
                scopes: vec![],
                is_authenticated: true,
                error: None,
            },
            Ok(_) => PlatformAuthInfo {
                platform: "reddit".into(),
                account_handle: None,
                account_remote_id: None,
                scopes: vec![],
                is_authenticated: false,
                error: Some("Reddit token lacks the identity scope".into()),
            },
            Err(e) => PlatformAuthInfo {
                platform: "reddit".into(),
                account_handle: None,
                account_remote_id: None,
                scopes: vec![],
                is_authenticated: false,
                error: Some(e.to_string()),
            },
        })
    }

    async fn like(&self, target: &str) -> Result<RemoteActionReceipt> {
        let id = reddit_fullname(target, "t3_");
        self.post_form("api/vote", &[("id", id.clone()), ("dir", "1".into())], "upvote")
            .await?;
        Ok(RemoteActionReceipt { remote_id: id, permalink: None })
    }

    async fn follow(&self, target: &str) -> Result<RemoteActionReceipt> {
        let name = target.trim().trim_start_matches("/u/").trim_start_matches("u/").trim_start_matches('@');
        let resp = self
            .authed(self.client.put(join_url(&self.base, &format!("api/v1/me/friends/{name}"))))
            .json(&json!({ "name": name }))
            .send()
            .await
            .map_err(|e| transport_error(PLATFORM, "follow", e))?;
        let res = read_json(resp, PLATFORM, "follow").await?;
        Ok(RemoteActionReceipt {
            remote_id: json_str(&res["id"]).unwrap_or_else(|| name.to_string()),
            permalink: Some(format!("https://www.reddit.com/user/{name}")),
        })
    }

    async fn subscribe(&self, target: &str) -> Result<RemoteActionReceipt> {
        let sub = target.trim().trim_start_matches("/r/").trim_start_matches("r/");
        self.post_form(
            "api/subscribe",
            &[("action", "sub".into()), ("sr_name", sub.to_string())],
            "subscribe",
        )
        .await?;
        Ok(RemoteActionReceipt {
            remote_id: sub.to_string(),
            permalink: Some(format!("https://www.reddit.com/r/{sub}")),
        })
    }

    async fn reshare(&self, target: &str, quote: Option<&str>) -> Result<RemoteActionReceipt> {
        let sr = self.default_subreddit().ok_or_else(|| {
            OpenAccountsError::Automation("Reddit crossposting requires REDDIT_DEFAULT_SUBREDDIT".into())
        })?;
        let title = quote
            .map(str::trim)
            .filter(|q| !q.is_empty())
            .map(|q| truncate_chars(q, 300))
            .unwrap_or_else(|| "Crosspost".into());
        self.submit(
            vec![
                ("api_type", "json".into()),
                ("kind", "crosspost".into()),
                ("sr", sr),
                ("title", title),
                ("crosspost_fullname", reddit_fullname(target, "t3_")),
            ],
            "crosspost",
        )
        .await
    }

    async fn comment(&self, target: &str, text: &str) -> Result<RemoteActionReceipt> {
        self.post_thing(reddit_fullname(target, "t3_"), text, "comment").await
    }

    async fn reply(&self, target: &str, text: &str) -> Result<RemoteActionReceipt> {
        self.post_thing(reddit_fullname(target, "t1_"), text, "reply").await
    }

    async fn publish(&self, request: PublishRequest) -> Result<RemoteActionReceipt> {
        let raw_title = request
            .title
            .clone()
            .unwrap_or_else(|| request.caption.lines().next().unwrap_or_default().to_string());
        let (sr, title) = match split_subreddit_title(&raw_title) {
            Some((sr, rest)) => (sr, rest),
            None => (
                self.default_subreddit().ok_or_else(|| {
                    OpenAccountsError::Automation(
                        "Reddit posts need a subreddit: prefix the title with r/<name> or set REDDIT_DEFAULT_SUBREDDIT"
                            .into(),
                    )
                })?,
                raw_title,
            ),
        };
        let title = truncate_chars(title.trim(), 300);
        if title.is_empty() {
            return Err(OpenAccountsError::Automation("Reddit posts require a title".into()));
        }
        let mut form = vec![("api_type", "json".to_string()), ("sr", sr), ("title", title)];
        match request.media.first() {
            Some(url) if is_remote_url(url) => {
                form.push(("kind", "link".into()));
                form.push(("url", url.clone()));
            }
            Some(local) => {
                return Err(OpenAccountsError::Automation(format!(
                    "Reddit link posts need a public URL; local file {local} cannot be attached"
                )))
            }
            None => {
                form.push(("kind", "self".into()));
                form.push(("text", request.full_text()));
            }
        }
        self.submit(form, "submit").await
    }

    async fn fetch_inbox(&self, since: Option<DateTime<Utc>>, limit: u32) -> Result<Vec<RemoteInteraction>> {
        let res = self
            .get("message/inbox", &[("limit", limit.clamp(1, 100).to_string())], "inbox")
            .await?;
        let children = res["data"]["children"].as_array().cloned().unwrap_or_default();
        Ok(children
            .iter()
            .filter_map(|child| {
                let d = &child["data"];
                let id = json_str(&d["name"])?;
                let created_at = d["created_utc"]
                    .as_f64()
                    .and_then(|ts| Utc.timestamp_opt(ts as i64, 0).single())
                    .unwrap_or_else(Utc::now);
                if since.map(|s| created_at <= s).unwrap_or(false) {
                    return None;
                }
                let kind = if d["was_comment"].as_bool().unwrap_or(false) {
                    if json_str(&d["type"]).as_deref() == Some("username_mention") {
                        InteractionKind::Mention
                    } else {
                        InteractionKind::Reply
                    }
                } else {
                    InteractionKind::DirectMessage
                };
                Some(RemoteInteraction {
                    author_handle: json_str(&d["author"]).map(|a| format!("u/{a}")).unwrap_or_else(|| "unknown".into()),
                    kind,
                    text: json_str(&d["body"]).unwrap_or_default(),
                    permalink: json_str(&d["context"]).map(|c| format!("https://www.reddit.com{c}")),
                    created_at,
                    remote_id: id,
                })
            })
            .collect())
    }

    async fn fetch_metrics(&self) -> Result<RemoteMetrics> {
        let me = self.get("api/v1/me", &[], "metrics").await?;
        Ok(RemoteMetrics {
            followers: json_u64(&me["subreddit"]["subscribers"]),
            likes: json_u64(&me["total_karma"]).or_else(|| {
                match (json_u64(&me["link_karma"]), json_u64(&me["comment_karma"])) {
                    (None, None) => None,
                    (a, b) => Some(a.unwrap_or(0) + b.unwrap_or(0)),
                }
            }),
            raw: me,
            ..Default::default()
        })
    }
}
