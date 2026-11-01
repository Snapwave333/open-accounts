use chrono::{DateTime, Utc};
use openaccounts_brand_manager::BrandManager;
use openaccounts_core::{OpenAccountsError, Result};
use openaccounts_lm_gateway::LMGateway;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};
use tokio::sync::RwLock;
use tracing::{info, warn};

pub const REDDIT_POPULAR_URL: &str = "https://www.reddit.com/r/popular/hot.json?limit=25&raw_json=1";
pub const GOOGLE_TRENDS_RSS_URL: &str = "https://trends.google.com/trending/rss?geo=US";
pub const YOUTUBE_POPULAR_URL: &str = "https://www.googleapis.com/youtube/v3/videos";
const TREND_USER_AGENT: &str = concat!("desktop:openaccounts-trends:", env!("CARGO_PKG_VERSION"));
const MAX_TRENDS: usize = 20;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendTopic {
    pub topic: String,
    pub platform: String,
    pub volume_score: u32,
    pub sentiment: String,
    pub recommended_hashtags: Vec<String>,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub raw_volume: u64,
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CampaignBrief {
    pub campaign_id: String,
    pub brand_id: String,
    pub title: String,
    pub objective: String,
    pub target_platforms: Vec<String>,
    pub target_audience: String,
    pub content_formats: Vec<String>,
    pub hashtags: Vec<String>,
    pub start_date: DateTime<Utc>,
    pub end_date: DateTime<Utc>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarPostSlot {
    pub slot_id: String,
    pub campaign_id: Option<String>,
    pub brand_id: String,
    pub target_platform: String,
    pub scheduled_time: DateTime<Utc>,
    pub content_type: String,
    pub caption: String,
    pub hashtags: Vec<String>,
    pub asset_ids: Vec<String>,
    pub is_approved: bool,
    pub status: String,
    #[serde(default)]
    pub account_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RawTrend {
    pub title: String,
    pub value: u64,
    pub source: String,
    pub sentiment: String,
    pub url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TrendSources {
    pub reddit_url: Option<String>,
    pub google_trends_url: Option<String>,
    pub youtube_url: Option<String>,
    pub youtube_api_key: Option<String>,
}

impl Default for TrendSources {
    fn default() -> Self {
        Self {
            reddit_url: Some(REDDIT_POPULAR_URL.into()),
            google_trends_url: Some(GOOGLE_TRENDS_RSS_URL.into()),
            youtube_url: Some(YOUTUBE_POPULAR_URL.into()),
            youtube_api_key: std::env::var("YOUTUBE_API_KEY")
                .ok()
                .map(|k| k.trim().to_string())
                .filter(|k| !k.is_empty()),
        }
    }
}

const STOPWORDS: &[&str] = &[
    "the", "and", "for", "with", "that", "this", "from", "your", "you", "are", "was", "were", "has", "have",
    "had", "not", "but", "what", "when", "who", "why", "how", "all", "can", "will", "just", "into", "about",
    "after", "over", "out", "its", "his", "her", "they", "them", "their", "our", "new", "now", "get", "got",
    "one", "more", "than", "being", "been", "says", "said", "like", "til", "does", "did", "any", "off",
];

pub fn hashtags_from_title(title: &str) -> Vec<String> {
    let words: Vec<String> = title
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect();
    let mut tags: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut push = |tag: String| {
        if tag.len() > 1 && seen.insert(tag.to_lowercase()) {
            tags.push(tag);
        }
    };

    if (1..=3).contains(&words.len()) {
        push(format!("#{}", words.iter().map(|w| capitalize(w)).collect::<String>()));
    }
    let mut keywords: Vec<&String> = words
        .iter()
        .filter(|w| w.chars().count() >= 4 && !STOPWORDS.contains(&w.to_lowercase().as_str()))
        .filter(|w| !w.chars().all(|c| c.is_ascii_digit()))
        .collect();
    keywords.sort_by_key(|w| std::cmp::Reverse(w.chars().count()));
    for w in keywords.into_iter().take(3) {
        push(format!("#{}", capitalize(w)));
    }
    tags
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn decode_entities(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
}

fn tag_value<'a>(block: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = block.find(&open)? + open.len();
    let end = block[start..].find(&close)? + start;
    Some(block[start..end].trim())
}

fn strip_cdata(text: &str) -> &str {
    text.strip_prefix("<![CDATA[")
        .and_then(|t| t.strip_suffix("]]>"))
        .unwrap_or(text)
        .trim()
}

pub fn parse_traffic(text: &str) -> u64 {
    let digits: String = text.chars().filter(|c| c.is_ascii_digit()).collect();
    let base: u64 = digits.parse().unwrap_or(0);
    let upper = text.to_uppercase();
    if upper.contains('M') {
        base.saturating_mul(1_000_000)
    } else if upper.contains('K') {
        base.saturating_mul(1_000)
    } else {
        base
    }
}

pub fn parse_google_trends_rss(xml: &str) -> Vec<RawTrend> {
    xml.split("<item>")
        .skip(1)
        .filter_map(|item| {
            let item = item.split("</item>").next().unwrap_or(item);
            let title = decode_entities(strip_cdata(tag_value(item, "title")?));
            if title.is_empty() {
                return None;
            }
            let value = tag_value(item, "ht:approx_traffic").map(parse_traffic).unwrap_or(0);
            let url = tag_value(item, "ht:news_item_url").map(|u| decode_entities(strip_cdata(u)));
            Some(RawTrend { title, value, source: "google_trends".into(), sentiment: "Unrated".into(), url })
        })
        .collect()
}

pub fn parse_reddit_hot(json: &Value) -> Vec<RawTrend> {
    json["data"]["children"]
        .as_array()
        .map(|children| {
            children
                .iter()
                .filter_map(|c| {
                    let d = &c["data"];
                    if d["over_18"].as_bool().unwrap_or(false) || d["stickied"].as_bool().unwrap_or(false) {
                        return None;
                    }
                    let title = decode_entities(d["title"].as_str()?.trim());
                    let ratio = d["upvote_ratio"].as_f64();
                    let sentiment = match ratio {
                        Some(r) if r >= 0.9 => "Positive",
                        Some(r) if r >= 0.7 => "Mixed",
                        Some(_) => "Divisive",
                        None => "Unrated",
                    };
                    Some(RawTrend {
                        title,
                        value: d["ups"].as_u64().or_else(|| d["score"].as_u64()).unwrap_or(0),
                        source: "reddit".into(),
                        sentiment: sentiment.into(),
                        url: d["permalink"].as_str().map(|p| format!("https://www.reddit.com{p}")),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn parse_youtube_most_popular(json: &Value) -> Vec<RawTrend> {
    json["items"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|v| {
                    let title = v["snippet"]["title"].as_str()?.trim().to_string();
                    let value = v["statistics"]["viewCount"]
                        .as_str()
                        .and_then(|s| s.parse().ok())
                        .or_else(|| v["statistics"]["viewCount"].as_u64())
                        .unwrap_or(0);
                    Some(RawTrend {
                        title,
                        value,
                        source: "youtube".into(),
                        sentiment: "Unrated".into(),
                        url: v["id"].as_str().map(|id| format!("https://www.youtube.com/watch?v={id}")),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn score_trends(raw: Vec<RawTrend>) -> Vec<TrendTopic> {
    let mut max_by_source: HashMap<String, u64> = HashMap::new();
    for r in &raw {
        let entry = max_by_source.entry(r.source.clone()).or_insert(0);
        *entry = (*entry).max(r.value);
    }
    let mut topics: Vec<TrendTopic> = raw
        .into_iter()
        .map(|r| {
            let max = max_by_source.get(&r.source).copied().unwrap_or(0);
            let volume_score = if max == 0 {
                1
            } else {
                (((r.value as f64 / max as f64) * 100.0).round() as u32).clamp(1, 100)
            };
            TrendTopic {
                recommended_hashtags: hashtags_from_title(&r.title),
                topic: r.title,
                platform: r.source.clone(),
                volume_score,
                sentiment: r.sentiment,
                source: r.source,
                raw_volume: r.value,
                url: r.url,
            }
        })
        .collect();
    topics.sort_by(|a, b| b.volume_score.cmp(&a.volume_score).then_with(|| b.raw_volume.cmp(&a.raw_volume)));
    topics
}

pub struct ContentStrategyEngine {
    #[allow(dead_code)]
    brand_manager: Arc<BrandManager>,
    #[allow(dead_code)]
    lm_gateway: Arc<LMGateway>,
    campaigns: Arc<RwLock<HashMap<String, CampaignBrief>>>,
    calendar: Arc<RwLock<Vec<CalendarPostSlot>>>,
    trend_sources: TrendSources,
    client: reqwest::Client,
}

impl ContentStrategyEngine {
    pub fn new(brand_manager: Arc<BrandManager>, lm_gateway: Arc<LMGateway>) -> Self {
        Self {
            brand_manager,
            lm_gateway,
            campaigns: Arc::new(RwLock::new(HashMap::new())),
            calendar: Arc::new(RwLock::new(Vec::new())),
            trend_sources: TrendSources::default(),
            client: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .timeout(Duration::from_secs(20))
                .user_agent(TREND_USER_AGENT)
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        }
    }

    pub fn with_trend_sources(mut self, sources: TrendSources) -> Self {
        self.trend_sources = sources;
        self
    }

    pub async fn create_campaign_brief(&self, brief: CampaignBrief) -> String {
        let id = brief.campaign_id.clone();
        self.campaigns.write().await.insert(id.clone(), brief);
        info!("Created new campaign brief: {}", id);
        id
    }

    pub async fn schedule_post_slot(&self, slot: CalendarPostSlot) -> String {
        let id = slot.slot_id.clone();
        let mut calendar = self.calendar.write().await;
        calendar.retain(|s| s.slot_id != id);
        calendar.push(slot);
        info!("Scheduled post slot: {}", id);
        id
    }

    pub async fn update_slot_status(&self, slot_id: &str, status: &str) -> bool {
        let mut calendar = self.calendar.write().await;
        match calendar.iter_mut().find(|s| s.slot_id == slot_id) {
            Some(slot) => {
                slot.status = status.to_string();
                true
            }
            None => false,
        }
    }

    pub async fn approve_slot(&self, slot_id: &str) -> bool {
        let mut calendar = self.calendar.write().await;
        match calendar.iter_mut().find(|s| s.slot_id == slot_id) {
            Some(slot) => {
                slot.is_approved = true;
                true
            }
            None => false,
        }
    }

    pub async fn list_calendar(&self) -> Vec<CalendarPostSlot> {
        let mut slots = self.calendar.read().await.clone();
        slots.sort_by_key(|s| s.scheduled_time);
        slots
    }

    pub async fn list_campaigns(&self) -> Vec<CampaignBrief> {
        self.campaigns.read().await.values().cloned().collect()
    }

    async fn fetch_text(&self, url: &str, query: &[(&str, &str)]) -> std::result::Result<String, String> {
        let resp = self
            .client
            .get(url)
            .query(query)
            .send()
            .await
            .map_err(|e| format!("{url}: {e}"))?;
        let status = resp.status();
        let body = resp.text().await.map_err(|e| format!("{url}: {e}"))?;
        if !status.is_success() {
            return Err(format!("{url}: HTTP {}", status.as_u16()));
        }
        Ok(body)
    }

    async fn fetch_reddit(&self) -> std::result::Result<Vec<RawTrend>, String> {
        let Some(url) = self.trend_sources.reddit_url.as_deref() else { return Ok(vec![]) };
        let body = self.fetch_text(url, &[]).await?;
        let json: Value = serde_json::from_str(&body).map_err(|e| format!("reddit: {e}"))?;
        Ok(parse_reddit_hot(&json))
    }

    async fn fetch_google_trends(&self) -> std::result::Result<Vec<RawTrend>, String> {
        let Some(url) = self.trend_sources.google_trends_url.as_deref() else { return Ok(vec![]) };
        Ok(parse_google_trends_rss(&self.fetch_text(url, &[]).await?))
    }

    async fn fetch_youtube(&self) -> std::result::Result<Vec<RawTrend>, String> {
        let (Some(url), Some(key)) = (
            self.trend_sources.youtube_url.as_deref(),
            self.trend_sources.youtube_api_key.as_deref(),
        ) else {
            return Ok(vec![]);
        };
        let body = self
            .fetch_text(
                url,
                &[
                    ("part", "snippet,statistics"),
                    ("chart", "mostPopular"),
                    ("regionCode", "US"),
                    ("maxResults", "15"),
                    ("key", key),
                ],
            )
            .await?;
        let json: Value = serde_json::from_str(&body).map_err(|e| format!("youtube: {e}"))?;
        Ok(parse_youtube_most_popular(&json))
    }

    pub async fn research_trends(&self, platform: &str) -> Result<Vec<TrendTopic>> {
        let (reddit, google, youtube) =
            tokio::join!(self.fetch_reddit(), self.fetch_google_trends(), self.fetch_youtube());

        let mut raw = Vec::new();
        let mut errors = Vec::new();
        for outcome in [reddit, google, youtube] {
            match outcome {
                Ok(items) => raw.extend(items),
                Err(e) => {
                    warn!("Trend source unavailable: {}", e);
                    errors.push(e);
                }
            }
        }
        if raw.is_empty() {
            return Err(OpenAccountsError::Automation(if errors.is_empty() {
                "No trend sources returned data".into()
            } else {
                format!("All trend sources failed: {}", errors.join("; "))
            }));
        }

        let wanted = platform.trim().to_lowercase();
        let mut topics = score_trends(raw);
        if topics.iter().any(|t| t.source == wanted) {
            topics.sort_by_key(|t| (t.source != wanted, std::cmp::Reverse(t.volume_score)));
        }
        topics.truncate(MAX_TRENDS);
        Ok(topics)
    }
}
