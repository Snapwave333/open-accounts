use async_trait::async_trait;
use base64::{
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
    Engine,
};
use chrono::{DateTime, TimeZone, Utc};
use openaccounts_core::{OpenAccountsError, Result};
use reqwest::{Client, RequestBuilder};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;
use tracing::{debug, info};

pub const GMAIL_API_BASE: &str = "https://gmail.googleapis.com/gmail/v1";
pub const GRAPH_API_BASE: &str = "https://graph.microsoft.com/v1.0";
pub const GMAIL_TOKEN_ENV: &str = "GMAIL_ACCESS_TOKEN";
pub const OUTLOOK_TOKEN_ENV: &str = "OUTLOOK_ACCESS_TOKEN";

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const ERROR_PREVIEW: usize = 300;
const MAX_FETCH: u32 = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmailProviderType {
    GmailApi,
    MicrosoftGraph,
    ImapSmtp,
    CustomDomainApi,
    MailboxProvisioningService,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IncomingEmailMessage {
    pub message_id: String,
    pub sender: String,
    pub recipient: String,
    pub subject: String,
    pub body_plain: String,
    pub body_html: Option<String>,
    pub received_at: DateTime<Utc>,
}

#[async_trait]
pub trait EmailProviderAdapter: Send + Sync {
    fn provider_type(&self) -> EmailProviderType;

    async fn mailbox_address(&self) -> Result<String>;

    async fn send_email(&self, to: &str, subject: &str, body_text: &str) -> Result<String>;

    async fn fetch_recent_messages(
        &self,
        limit: u32,
        since: Option<DateTime<Utc>>,
    ) -> Result<Vec<IncomingEmailMessage>>;

    async fn create_alias_or_mailbox(&self, prefix: &str) -> Result<String>;
}

fn email_error(msg: impl Into<String>) -> OpenAccountsError {
    OpenAccountsError::Internal(msg.into())
}

fn http_client() -> Client {
    Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .connect_timeout(Duration::from_secs(10))
        .build()
        .unwrap_or_else(|_| Client::new())
}

fn token_from_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

async fn send_checked(builder: RequestBuilder, provider: &str) -> Result<reqwest::Response> {
    let resp = builder
        .send()
        .await
        .map_err(|e| email_error(format!("{provider} request failed: {e}")))?;
    let status = resp.status();
    if status.is_success() {
        return Ok(resp);
    }
    let body = resp.text().await.unwrap_or_default();
    let preview: String = body.chars().take(ERROR_PREVIEW).collect();
    Err(email_error(format!(
        "{provider} API returned HTTP {}: {}",
        status.as_u16(),
        preview.trim()
    )))
}

async fn json_checked(builder: RequestBuilder, provider: &str) -> Result<Value> {
    send_checked(builder, provider)
        .await?
        .json()
        .await
        .map_err(|e| email_error(format!("{provider} response parse error: {e}")))
}

fn contains_line_break(s: &str) -> bool {
    s.contains('\r') || s.contains('\n')
}

pub fn encode_header_value(value: &str) -> String {
    if value.is_ascii() {
        value.to_string()
    } else {
        format!("=?UTF-8?B?{}?=", STANDARD.encode(value.as_bytes()))
    }
}

pub fn build_raw_message(from: &str, to: &str, subject: &str, body_text: &str) -> Result<String> {
    for (name, value) in [("From", from), ("To", to), ("Subject", subject)] {
        if contains_line_break(value) {
            return Err(email_error(format!("{name} header must not contain line breaks")));
        }
    }
    if !to.contains('@') {
        return Err(email_error(format!("Invalid recipient address: {to}")));
    }

    let encoded_body = STANDARD.encode(body_text.as_bytes());
    let wrapped_body = encoded_body
        .as_bytes()
        .chunks(76)
        .map(|c| String::from_utf8_lossy(c).into_owned())
        .collect::<Vec<_>>()
        .join("\r\n");

    let mut raw = String::new();
    if !from.trim().is_empty() {
        raw.push_str(&format!("From: {from}\r\n"));
    }
    raw.push_str(&format!("To: {to}\r\n"));
    raw.push_str(&format!("Subject: {}\r\n", encode_header_value(subject)));
    raw.push_str(&format!("Date: {}\r\n", Utc::now().to_rfc2822()));
    raw.push_str("MIME-Version: 1.0\r\n");
    raw.push_str("Content-Type: text/plain; charset=\"UTF-8\"\r\n");
    raw.push_str("Content-Transfer-Encoding: base64\r\n\r\n");
    raw.push_str(&wrapped_body);
    raw.push_str("\r\n");
    Ok(raw)
}

pub fn encode_raw_message_for_gmail(raw: &str) -> String {
    URL_SAFE_NO_PAD.encode(raw.as_bytes())
}

pub fn sanitize_alias_tag(prefix: &str) -> String {
    let cleaned: String = prefix
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-' { c } else { '-' })
        .collect();
    let collapsed = cleaned
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let trimmed: String = collapsed
        .trim_matches(|c| c == '.' || c == '-' || c == '_')
        .chars()
        .take(32)
        .collect();
    let trimmed = trimmed.trim_matches(|c| c == '.' || c == '-' || c == '_').to_string();
    if trimmed.is_empty() {
        "alias".into()
    } else {
        trimmed
    }
}

pub fn plus_alias(mailbox: &str, prefix: &str, unique_suffix: &str) -> Result<String> {
    let (local, domain) = mailbox
        .trim()
        .rsplit_once('@')
        .ok_or_else(|| email_error(format!("Mailbox address is not an email: {mailbox}")))?;
    let base_local = local.split('+').next().unwrap_or(local);
    if base_local.is_empty() || domain.is_empty() || !domain.contains('.') {
        return Err(email_error(format!("Mailbox address is not an email: {mailbox}")));
    }
    let tag = sanitize_alias_tag(prefix);
    let suffix: String = unique_suffix
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(12)
        .collect();
    let tag = if suffix.is_empty() { tag } else { format!("{tag}-{suffix}") };
    Ok(format!("{base_local}+{tag}@{}", domain.to_lowercase()))
}

fn random_suffix() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..6].to_string()
}

pub fn decode_base64_any(data: &str) -> Option<Vec<u8>> {
    let normalized: String = data
        .trim()
        .trim_end_matches('=')
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| match c {
            '+' => '-',
            '/' => '_',
            other => other,
        })
        .collect();
    URL_SAFE_NO_PAD.decode(normalized.as_bytes()).ok()
}

pub fn html_to_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut tag = String::new();
    let mut skip_depth = 0usize;
    for c in html.chars() {
        match c {
            '<' => {
                in_tag = true;
                tag.clear();
            }
            '>' if in_tag => {
                in_tag = false;
                let name = tag
                    .trim_start_matches('/')
                    .split(|ch: char| ch.is_whitespace() || ch == '/')
                    .next()
                    .unwrap_or("")
                    .to_lowercase();
                let closing = tag.starts_with('/');
                if name == "script" || name == "style" {
                    if closing {
                        skip_depth = skip_depth.saturating_sub(1);
                    } else {
                        skip_depth += 1;
                    }
                }
                if matches!(name.as_str(), "br" | "p" | "div" | "tr" | "li" | "h1" | "h2" | "h3" | "td") {
                    out.push(' ');
                }
                if name == "a" && !closing {
                    if let Some(href) = extract_href(&tag) {
                        out.push(' ');
                        out.push_str(&href);
                        out.push(' ');
                    }
                }
            }
            _ if in_tag => tag.push(c),
            _ if skip_depth > 0 => {}
            _ => out.push(c),
        }
    }
    let decoded = out
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'");
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn extract_href(tag: &str) -> Option<String> {
    let lower = tag.to_lowercase();
    let idx = lower.find("href=")?;
    let rest = &tag[idx + 5..];
    let quote = rest.chars().next()?;
    if quote == '"' || quote == '\'' {
        let inner = &rest[1..];
        inner.find(quote).map(|end| inner[..end].to_string())
    } else {
        rest.split_whitespace().next().map(str::to_string)
    }
}

fn header_value(headers: &Value, name: &str) -> Option<String> {
    headers.as_array()?.iter().find_map(|h| {
        let key = h["name"].as_str()?;
        if key.eq_ignore_ascii_case(name) {
            h["value"].as_str().map(str::to_string)
        } else {
            None
        }
    })
}

fn collect_gmail_parts(part: &Value, plain: &mut Vec<String>, html: &mut Vec<String>) {
    let mime = part["mimeType"].as_str().unwrap_or_default().to_lowercase();
    let is_attachment = part["filename"].as_str().map(|f| !f.is_empty()).unwrap_or(false);
    if !is_attachment {
        if let Some(data) = part["body"]["data"].as_str() {
            if let Some(bytes) = decode_base64_any(data) {
                let text = String::from_utf8_lossy(&bytes).into_owned();
                if mime.starts_with("text/html") {
                    html.push(text);
                } else if mime.starts_with("text/plain") || mime.is_empty() {
                    plain.push(text);
                }
            }
        }
    }
    if let Some(parts) = part["parts"].as_array() {
        for child in parts {
            collect_gmail_parts(child, plain, html);
        }
    }
}

pub fn extract_gmail_bodies(payload: &Value) -> (String, Option<String>) {
    let mut plain = Vec::new();
    let mut html = Vec::new();
    collect_gmail_parts(payload, &mut plain, &mut html);
    let html_joined = if html.is_empty() { None } else { Some(html.join("\n")) };
    let plain_joined = if plain.is_empty() {
        html_joined.as_deref().map(html_to_text).unwrap_or_default()
    } else {
        plain.join("\n")
    };
    (plain_joined, html_joined)
}

pub fn parse_gmail_message(message: &Value) -> Result<IncomingEmailMessage> {
    let id = message["id"]
        .as_str()
        .ok_or_else(|| email_error("Gmail message is missing an id"))?;
    let payload = &message["payload"];
    let headers = &payload["headers"];
    let (body_plain, body_html) = extract_gmail_bodies(payload);
    let received_at = message["internalDate"]
        .as_str()
        .and_then(|s| s.parse::<i64>().ok())
        .and_then(|ms| Utc.timestamp_millis_opt(ms).single())
        .or_else(|| {
            header_value(headers, "Date")
                .and_then(|d| DateTime::parse_from_rfc2822(d.trim()).ok())
                .map(|d| d.with_timezone(&Utc))
        })
        .unwrap_or_else(Utc::now);
    let body_plain = if body_plain.trim().is_empty() {
        message["snippet"].as_str().unwrap_or_default().to_string()
    } else {
        body_plain
    };

    Ok(IncomingEmailMessage {
        message_id: id.to_string(),
        sender: header_value(headers, "From").unwrap_or_default(),
        recipient: header_value(headers, "To")
            .or_else(|| header_value(headers, "Delivered-To"))
            .unwrap_or_default(),
        subject: header_value(headers, "Subject").unwrap_or_default(),
        body_plain,
        body_html,
        received_at,
    })
}

pub fn parse_graph_message(message: &Value) -> Result<IncomingEmailMessage> {
    let id = message["id"]
        .as_str()
        .ok_or_else(|| email_error("Graph message is missing an id"))?;
    let content = message["body"]["content"].as_str().unwrap_or_default().to_string();
    let is_html = message["body"]["contentType"]
        .as_str()
        .map(|t| t.eq_ignore_ascii_case("html"))
        .unwrap_or(false);
    let (body_plain, body_html) = if is_html {
        (html_to_text(&content), Some(content))
    } else {
        (content, None)
    };
    let received_at = message["receivedDateTime"]
        .as_str()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or_else(Utc::now);
    let sender = message["from"]["emailAddress"]["address"]
        .as_str()
        .or_else(|| message["sender"]["emailAddress"]["address"].as_str())
        .unwrap_or_default()
        .to_string();
    let recipient = message["toRecipients"]
        .as_array()
        .and_then(|r| r.first())
        .and_then(|r| r["emailAddress"]["address"].as_str())
        .unwrap_or_default()
        .to_string();

    Ok(IncomingEmailMessage {
        message_id: id.to_string(),
        sender,
        recipient,
        subject: message["subject"].as_str().unwrap_or_default().to_string(),
        body_plain,
        body_html,
        received_at,
    })
}

pub struct GmailApiAdapter {
    access_token: String,
    base_url: String,
    client: Client,
}

impl GmailApiAdapter {
    pub fn new(access_token: impl Into<String>) -> Self {
        Self::with_base_url(access_token, GMAIL_API_BASE)
    }

    pub fn with_base_url(access_token: impl Into<String>, base_url: &str) -> Self {
        Self {
            access_token: access_token.into(),
            base_url: base_url.trim_end_matches('/').to_string(),
            client: http_client(),
        }
    }

    pub fn from_env() -> Option<Self> {
        token_from_env(GMAIL_TOKEN_ENV).map(Self::new)
    }

    fn url(&self, path: &str) -> String {
        format!("{}/users/me/{}", self.base_url, path.trim_start_matches('/'))
    }
}

#[async_trait]
impl EmailProviderAdapter for GmailApiAdapter {
    fn provider_type(&self) -> EmailProviderType {
        EmailProviderType::GmailApi
    }

    async fn mailbox_address(&self) -> Result<String> {
        let val = json_checked(self.client.get(self.url("profile")).bearer_auth(&self.access_token), "Gmail").await?;
        val["emailAddress"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| email_error("Gmail profile response missing emailAddress"))
    }

    async fn send_email(&self, to: &str, subject: &str, body_text: &str) -> Result<String> {
        let from = self.mailbox_address().await?;
        let raw = build_raw_message(&from, to, subject, body_text)?;
        let body = serde_json::json!({ "raw": encode_raw_message_for_gmail(&raw) });
        let val = json_checked(
            self.client
                .post(self.url("messages/send"))
                .bearer_auth(&self.access_token)
                .json(&body),
            "Gmail",
        )
        .await?;
        let id = val["id"]
            .as_str()
            .ok_or_else(|| email_error("Gmail send response missing message id"))?;
        info!("Gmail message {} sent to {}", id, to);
        Ok(id.to_string())
    }

    async fn fetch_recent_messages(
        &self,
        limit: u32,
        since: Option<DateTime<Utc>>,
    ) -> Result<Vec<IncomingEmailMessage>> {
        let limit = limit.clamp(1, MAX_FETCH);
        let mut query: Vec<(&str, String)> = vec![("maxResults", limit.to_string())];
        if let Some(since) = since {
            query.push(("q", format!("after:{}", since.timestamp())));
        }
        let listing = json_checked(
            self.client
                .get(self.url("messages"))
                .bearer_auth(&self.access_token)
                .query(&query),
            "Gmail",
        )
        .await?;

        let ids: Vec<String> = listing["messages"]
            .as_array()
            .map(|items| items.iter().filter_map(|m| m["id"].as_str().map(str::to_string)).collect())
            .unwrap_or_default();

        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            let full = json_checked(
                self.client
                    .get(self.url(&format!("messages/{id}")))
                    .bearer_auth(&self.access_token)
                    .query(&[("format", "full")]),
                "Gmail",
            )
            .await?;
            let msg = parse_gmail_message(&full)?;
            if since.map(|s| msg.received_at >= s).unwrap_or(true) {
                out.push(msg);
            }
        }
        out.sort_by_key(|m| std::cmp::Reverse(m.received_at));
        debug!("Fetched {} Gmail messages", out.len());
        Ok(out)
    }

    async fn create_alias_or_mailbox(&self, prefix: &str) -> Result<String> {
        let mailbox = self.mailbox_address().await?;
        plus_alias(&mailbox, prefix, &random_suffix())
    }
}

pub struct MicrosoftGraphAdapter {
    access_token: String,
    base_url: String,
    client: Client,
}

impl MicrosoftGraphAdapter {
    pub fn new(access_token: impl Into<String>) -> Self {
        Self::with_base_url(access_token, GRAPH_API_BASE)
    }

    pub fn with_base_url(access_token: impl Into<String>, base_url: &str) -> Self {
        Self {
            access_token: access_token.into(),
            base_url: base_url.trim_end_matches('/').to_string(),
            client: http_client(),
        }
    }

    pub fn from_env() -> Option<Self> {
        token_from_env(OUTLOOK_TOKEN_ENV).map(Self::new)
    }

    fn url(&self, path: &str) -> String {
        format!("{}/{}", self.base_url, path.trim_start_matches('/'))
    }
}

pub fn graph_draft_body(to: &str, subject: &str, body_text: &str) -> Result<Value> {
    if contains_line_break(to) || !to.contains('@') {
        return Err(email_error(format!("Invalid recipient address: {to}")));
    }
    Ok(serde_json::json!({
        "subject": subject,
        "body": { "contentType": "Text", "content": body_text },
        "toRecipients": [ { "emailAddress": { "address": to } } ],
    }))
}

#[async_trait]
impl EmailProviderAdapter for MicrosoftGraphAdapter {
    fn provider_type(&self) -> EmailProviderType {
        EmailProviderType::MicrosoftGraph
    }

    async fn mailbox_address(&self) -> Result<String> {
        let val = json_checked(
            self.client
                .get(self.url("me"))
                .bearer_auth(&self.access_token)
                .query(&[("$select", "mail,userPrincipalName")]),
            "Microsoft Graph",
        )
        .await?;
        val["mail"]
            .as_str()
            .filter(|s| !s.is_empty())
            .or_else(|| val["userPrincipalName"].as_str())
            .map(str::to_string)
            .ok_or_else(|| email_error("Microsoft Graph /me response missing mail address"))
    }

    async fn send_email(&self, to: &str, subject: &str, body_text: &str) -> Result<String> {
        let draft = json_checked(
            self.client
                .post(self.url("me/messages"))
                .bearer_auth(&self.access_token)
                .json(&graph_draft_body(to, subject, body_text)?),
            "Microsoft Graph",
        )
        .await?;
        let id = draft["id"]
            .as_str()
            .ok_or_else(|| email_error("Microsoft Graph draft response missing id"))?
            .to_string();
        send_checked(
            self.client
                .post(self.url(&format!("me/messages/{id}/send")))
                .bearer_auth(&self.access_token)
                .header("Content-Length", "0"),
            "Microsoft Graph",
        )
        .await?;
        info!("Microsoft Graph message {} sent to {}", id, to);
        Ok(id)
    }

    async fn fetch_recent_messages(
        &self,
        limit: u32,
        since: Option<DateTime<Utc>>,
    ) -> Result<Vec<IncomingEmailMessage>> {
        let limit = limit.clamp(1, MAX_FETCH);
        let mut query: Vec<(&str, String)> = vec![
            ("$top", limit.to_string()),
            ("$orderby", "receivedDateTime desc".into()),
            ("$select", "id,subject,from,sender,toRecipients,body,receivedDateTime".into()),
        ];
        if let Some(since) = since {
            query.push((
                "$filter",
                format!("receivedDateTime ge {}", since.format("%Y-%m-%dT%H:%M:%SZ")),
            ));
        }
        let listing = json_checked(
            self.client
                .get(self.url("me/messages"))
                .bearer_auth(&self.access_token)
                .query(&query),
            "Microsoft Graph",
        )
        .await?;

        let mut out = Vec::new();
        if let Some(items) = listing["value"].as_array() {
            for item in items {
                out.push(parse_graph_message(item)?);
            }
        }
        out.sort_by_key(|m| std::cmp::Reverse(m.received_at));
        debug!("Fetched {} Microsoft Graph messages", out.len());
        Ok(out)
    }

    async fn create_alias_or_mailbox(&self, prefix: &str) -> Result<String> {
        let mailbox = self.mailbox_address().await?;
        plus_alias(&mailbox, prefix, &random_suffix())
    }
}
