use openaccounts_brand_manager::BrandManager;
use openaccounts_content_strategy::{
    hashtags_from_title, parse_google_trends_rss, parse_reddit_hot, parse_traffic, parse_youtube_most_popular,
    score_trends, ContentStrategyEngine, TrendSources,
};
use openaccounts_lm_gateway::LMGateway;
use std::sync::Arc;

const RSS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss xmlns:ht="https://trends.google.com/trending/rss" version="2.0"><channel>
<title>Daily Search Trends</title>
<item><title>Northern Lights</title><ht:approx_traffic>500K+</ht:approx_traffic>
<ht:news_item><ht:news_item_url>https://news.example.com/aurora?a=1&amp;b=2</ht:news_item_url></ht:news_item></item>
<item><title><![CDATA[Rust 2.0 &amp; Cargo]]></title><ht:approx_traffic>20,000+</ht:approx_traffic></item>
</channel></rss>"#;

#[test]
fn google_trends_rss_is_parsed_with_traffic() {
    let items = parse_google_trends_rss(RSS);
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].title, "Northern Lights");
    assert_eq!(items[0].value, 500_000);
    assert_eq!(items[0].url.as_deref(), Some("https://news.example.com/aurora?a=1&b=2"));
    assert_eq!(items[1].title, "Rust 2.0 & Cargo");
    assert_eq!(items[1].value, 20_000);
    assert_eq!(parse_traffic("2M+"), 2_000_000);
}

#[test]
fn reddit_hot_skips_nsfw_and_stickied_posts() {
    let json = serde_json::json!({ "data": { "children": [
        { "data": { "title": "Scientists map deep ocean vents", "ups": 40000, "upvote_ratio": 0.95, "permalink": "/r/science/a" } },
        { "data": { "title": "nsfw thing", "ups": 90000, "over_18": true } },
        { "data": { "title": "Announcement", "ups": 10, "stickied": true } },
        { "data": { "title": "Hot take on tabs vs spaces", "ups": 20000, "upvote_ratio": 0.6 } }
    ]}});
    let items = parse_reddit_hot(&json);
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].sentiment, "Positive");
    assert_eq!(items[1].sentiment, "Divisive");
    assert_eq!(items[0].url.as_deref(), Some("https://www.reddit.com/r/science/a"));
}

#[test]
fn youtube_most_popular_uses_view_counts() {
    let json = serde_json::json!({ "items": [
        { "id": "abc", "snippet": { "title": "Launch day recap" }, "statistics": { "viewCount": "1234567" } }
    ]});
    let items = parse_youtube_most_popular(&json);
    assert_eq!(items[0].value, 1_234_567);
    assert_eq!(items[0].url.as_deref(), Some("https://www.youtube.com/watch?v=abc"));
}

#[test]
fn scores_are_relative_within_each_source() {
    let mut raw = parse_google_trends_rss(RSS);
    raw.extend(parse_reddit_hot(&serde_json::json!({ "data": { "children": [
        { "data": { "title": "Ocean vents", "ups": 400 } }
    ]}})));
    let topics = score_trends(raw);
    let lights = topics.iter().find(|t| t.topic == "Northern Lights").unwrap();
    let rust = topics.iter().find(|t| t.topic.starts_with("Rust")).unwrap();
    let vents = topics.iter().find(|t| t.topic == "Ocean vents").unwrap();
    assert_eq!(lights.volume_score, 100);
    assert_eq!(rust.volume_score, 4);
    assert_eq!(vents.volume_score, 100);
    assert_eq!(lights.raw_volume, 500_000);
    assert!(lights.recommended_hashtags.contains(&"#NorthernLights".to_string()));
}

#[test]
fn hashtags_ignore_stopwords_and_numbers() {
    let tags = hashtags_from_title("What the new Webb telescope found about 2024 galaxies");
    assert!(tags.contains(&"#Telescope".to_string()));
    assert!(tags.contains(&"#Galaxies".to_string()));
    assert!(!tags.iter().any(|t| t == "#2024" || t == "#What" || t == "#About"));
}

#[tokio::test]
async fn research_fails_honestly_when_no_source_is_reachable() {
    let engine = ContentStrategyEngine::new(Arc::new(BrandManager::new()), Arc::new(LMGateway::new()))
        .with_trend_sources(TrendSources {
            reddit_url: Some("http://127.0.0.1:9/hot.json".into()),
            google_trends_url: Some("http://127.0.0.1:9/rss".into()),
            youtube_url: None,
            youtube_api_key: None,
        });
    let err = engine.research_trends("reddit").await.unwrap_err();
    assert!(err.to_string().contains("All trend sources failed"), "{err}");
}
