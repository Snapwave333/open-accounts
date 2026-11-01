use openaccounts_platform_connectors::{
    reddit_fullname, split_subreddit_title, ConnectorRegistry, InstagramConnector, PlatformConnector,
    PlatformCredentials, PublishRequest, RedditConnector, StaticTokenSource, XConnector, YouTubeConnector,
};
use std::sync::{Arc, Mutex};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

#[derive(Debug, Clone)]
struct Recorded {
    method: String,
    target: String,
    headers: Vec<(String, String)>,
    body: String,
}

impl Recorded {
    fn header(&self, name: &str) -> Option<String> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.clone())
    }

    fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.body).unwrap_or(serde_json::Value::Null)
    }
}

type Responder = fn(&Recorded) -> (u16, String);

async fn spawn_server(responder: Responder) -> (String, Arc<Mutex<Vec<Recorded>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let log: Arc<Mutex<Vec<Recorded>>> = Arc::new(Mutex::new(Vec::new()));
    let log_clone = log.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else { return };
            let log = log_clone.clone();
            tokio::spawn(async move {
                let mut buf = Vec::new();
                let mut chunk = [0u8; 4096];
                let header_end = loop {
                    let n = socket.read(&mut chunk).await.unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                    if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        break pos + 4;
                    }
                };
                let head = String::from_utf8_lossy(&buf[..header_end]).to_string();
                let mut lines = head.split("\r\n");
                let request_line = lines.next().unwrap_or_default().to_string();
                let mut parts = request_line.split_whitespace();
                let method = parts.next().unwrap_or_default().to_string();
                let target = parts.next().unwrap_or_default().to_string();
                let headers: Vec<(String, String)> = lines
                    .filter_map(|l| l.split_once(':'))
                    .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
                    .collect();
                let len: usize = headers
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                    .and_then(|(_, v)| v.parse().ok())
                    .unwrap_or(0);
                while buf.len() < header_end + len {
                    let n = socket.read(&mut chunk).await.unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                }
                let body = String::from_utf8_lossy(&buf[header_end..]).to_string();
                let rec = Recorded { method, target, headers, body };
                let (status, payload) = responder(&rec);
                log.lock().unwrap().push(rec);
                let response = format!(
                    "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
                    payload.len()
                );
                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.shutdown().await;
            });
        }
    });
    (format!("http://{addr}"), log)
}

fn x_responder(req: &Recorded) -> (u16, String) {
    if req.target.contains("/likes") {
        (200, r#"{"data":{"liked":true}}"#.into())
    } else if req.target.ends_with("/tweets") {
        (201, r#"{"data":{"id":"777","text":"ok"}}"#.into())
    } else if req.target.contains("/users/by/username/") {
        (200, r#"{"data":{"id":"555","username":"someone"}}"#.into())
    } else {
        (403, r#"{"title":"Forbidden","detail":"missing scope"}"#.into())
    }
}

#[tokio::test]
async fn x_like_posts_tweet_id_to_user_likes_endpoint() {
    let (base, log) = spawn_server(x_responder).await;
    let x = XConnector::with_base_url(PlatformCredentials::new("tok").with_user_id("42"), &format!("{base}/2"));
    let receipt = x.like("999").await.unwrap();
    assert_eq!(receipt.remote_id, "999");

    let reqs = log.lock().unwrap().clone();
    assert_eq!(reqs.len(), 1);
    assert_eq!(reqs[0].method, "POST");
    assert_eq!(reqs[0].target, "/2/users/42/likes");
    assert_eq!(reqs[0].header("authorization").as_deref(), Some("Bearer tok"));
    assert_eq!(reqs[0].json()["tweet_id"], "999");
}

#[tokio::test]
async fn x_reply_sets_in_reply_to_and_returns_new_tweet() {
    let (base, log) = spawn_server(x_responder).await;
    let x = XConnector::with_base_url(PlatformCredentials::new("tok").with_user_id("42"), &format!("{base}/2"));
    let receipt = x.reply("123", "thanks!").await.unwrap();
    assert_eq!(receipt.remote_id, "777");
    assert_eq!(receipt.permalink.as_deref(), Some("https://x.com/i/web/status/777"));

    let body = log.lock().unwrap()[0].json();
    assert_eq!(body["text"], "thanks!");
    assert_eq!(body["reply"]["in_reply_to_tweet_id"], "123");
}

#[tokio::test]
async fn x_follow_resolves_handles_before_following() {
    let (base, log) = spawn_server(x_responder).await;
    let x = XConnector::with_base_url(PlatformCredentials::new("tok").with_user_id("42"), &format!("{base}/2"));
    let err = x.follow("@someone").await.unwrap_err().to_string();
    assert!(err.contains("HTTP 403"), "{err}");
    assert!(err.contains("missing scope"), "{err}");
    let reqs = log.lock().unwrap().clone();
    assert_eq!(reqs[0].target, "/2/users/by/username/someone");
    assert_eq!(reqs[1].target, "/2/users/42/following");
    assert_eq!(reqs[1].json()["target_user_id"], "555");
}

#[tokio::test]
async fn x_rejects_media_posts_instead_of_dropping_media() {
    let x = XConnector::with_base_url(PlatformCredentials::new("tok").with_user_id("42"), "http://127.0.0.1:9");
    let err = x
        .publish(PublishRequest { caption: "hi".into(), media: vec!["a.png".into()], ..Default::default() })
        .await
        .unwrap_err();
    assert!(err.to_string().contains("media"));
}

fn youtube_responder(req: &Recorded) -> (u16, String) {
    if req.target.starts_with("/youtube/v3/comments") {
        (200, r#"{"id":"Ugx-reply-1","snippet":{"textOriginal":"hello"}}"#.into())
    } else {
        (404, r#"{"error":{"message":"not found"}}"#.into())
    }
}

#[tokio::test]
async fn youtube_reply_uses_comments_insert_with_parent_id() {
    let (base, log) = spawn_server(youtube_responder).await;
    let yt = YouTubeConnector::with_base_urls(
        PlatformCredentials::new("yt-token"),
        &format!("{base}/youtube/v3"),
        &format!("{base}/upload/youtube/v3"),
    );
    let receipt = yt.reply("Ugx-parent", "hello").await.unwrap();
    assert_eq!(receipt.remote_id, "Ugx-reply-1");

    let req = log.lock().unwrap()[0].clone();
    assert_eq!(req.method, "POST");
    assert_eq!(req.target, "/youtube/v3/comments?part=snippet");
    assert_eq!(req.header("authorization").as_deref(), Some("Bearer yt-token"));
    let body = req.json();
    assert_eq!(body["snippet"]["parentId"], "Ugx-parent");
    assert_eq!(body["snippet"]["textOriginal"], "hello");
}

#[tokio::test]
async fn youtube_publish_requires_local_file() {
    let yt = YouTubeConnector::new(PlatformCredentials::new("t"));
    let err = yt
        .publish(PublishRequest {
            caption: "c".into(),
            media: vec!["https://example.com/v.mp4".into()],
            ..Default::default()
        })
        .await
        .unwrap_err();
    assert!(err.to_string().contains("local video file"));
}

fn reddit_responder(_req: &Recorded) -> (u16, String) {
    (
        200,
        r#"{"json":{"errors":[],"data":{"things":[{"kind":"t1","data":{"name":"t1_new","permalink":"/r/rust/comments/x/y/new/"}}]}}}"#
            .into(),
    )
}

#[tokio::test]
async fn reddit_reply_posts_form_with_user_agent() {
    let (base, log) = spawn_server(reddit_responder).await;
    let reddit = RedditConnector::with_base_url(
        PlatformCredentials::new("rd").with_extra("user_agent", "test:oa:1.0 (by /u/tester)"),
        &base,
    );
    let receipt = reddit.reply("abc123", "nice post").await.unwrap();
    assert_eq!(receipt.remote_id, "t1_new");
    assert_eq!(
        receipt.permalink.as_deref(),
        Some("https://www.reddit.com/r/rust/comments/x/y/new/")
    );

    let req = log.lock().unwrap()[0].clone();
    assert_eq!(req.target, "/api/comment");
    assert_eq!(req.header("user-agent").as_deref(), Some("test:oa:1.0 (by /u/tester)"));
    assert!(req.body.contains("thing_id=t1_abc123"), "{}", req.body);
    assert!(req.body.contains("text=nice+post"), "{}", req.body);
}

#[test]
fn reddit_helpers_parse_targets() {
    assert_eq!(reddit_fullname("abc", "t3_"), "t3_abc");
    assert_eq!(reddit_fullname("t1_xyz", "t3_"), "t1_xyz");
    assert_eq!(
        split_subreddit_title("r/rust: Shipping 1.0"),
        Some(("rust".into(), "Shipping 1.0".into()))
    );
    assert_eq!(split_subreddit_title("Plain title"), None);
}

#[tokio::test]
async fn instagram_reports_unsupported_operations_honestly() {
    let ig = InstagramConnector::new(PlatformCredentials::new("t").with_user_id("1"));
    let err = ig.like("123").await.unwrap_err().to_string();
    assert!(err.contains("not supported by the official Instagram API"), "{err}");
    let err = ig
        .publish(PublishRequest { caption: "c".into(), media: vec!["/tmp/local.jpg".into()], ..Default::default() })
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("public https URL"), "{err}");
}

#[tokio::test]
async fn registry_reports_missing_credentials_and_normalises_platforms() {
    let registry = ConnectorRegistry::new(Arc::new(StaticTokenSource::new()));
    let err = registry.connector_for("Twitter").await.err().unwrap().to_string();
    assert_eq!(err, "Automation error: x is not connected: set X_ACCESS_TOKEN");
    let err = registry.connector_for("tiktok").await.err().unwrap().to_string();
    assert!(err.contains("no official API connector"), "{err}");
    assert!(!registry.is_connected("youtube").await);
}

#[tokio::test]
async fn registry_builds_connectors_from_token_source_and_overrides() {
    let tokens = Arc::new(StaticTokenSource::new());
    tokens.insert("youtube", PlatformCredentials::new("yt")).await;
    let registry = ConnectorRegistry::new(tokens.clone())
        .with_connector("ig", Arc::new(InstagramConnector::new(PlatformCredentials::new("x"))));
    assert_eq!(registry.connector_for("YouTube").await.unwrap().platform(), "youtube");
    assert_eq!(registry.connector_for("instagram").await.unwrap().platform(), "instagram");
    tokens.remove("youtube").await;
    assert!(!registry.is_connected("youtube").await);
}
