use base64::{engine::general_purpose::STANDARD, Engine};
use openaccounts_browser_abstraction::{BrowserProvider, ChallengeType};
use openaccounts_chrome_relay_adapter::{
    devtools_http_base, find_browser_binary, is_url_allowed, ChromeProcess, ChromeRelayAdapter, ChromeRelayConfig,
};
use openaccounts_core::OpenAccountsError;

fn data_url(html: &str) -> String {
    format!("data:text/html;base64,{}", STANDARD.encode(html))
}

const FORM_PAGE: &str = r#"<!doctype html><html><head><title>Relay Test</title></head><body>
<h1 id="status">idle</h1>
<button id="go" onclick="document.getElementById('status').textContent='clicked'">Go</button>
<input id="name" oninput="document.getElementById('echo').textContent=this.value">
<p id="echo"></p>
</body></html>"#;

const OTP_PAGE: &str = r#"<!doctype html><html><head><title>Verify</title></head><body>
<form><label>Code <input name="code" autocomplete="one-time-code"></label></form>
</body></html>"#;

async fn launch() -> Option<ChromeProcess> {
    if find_browser_binary().is_none() {
        eprintln!("skipping: no Chrome/Chromium binary available");
        return None;
    }
    Some(ChromeProcess::launch(None).await.expect("browser should launch"))
}

#[test]
fn fence_matching_rules() {
    let fence = vec!["*.instagram.com".to_string(), "https://reddit.com/r/x".to_string()];
    assert!(is_url_allowed("https://instagram.com/p/1", &fence));
    assert!(is_url_allowed("https://www.instagram.com", &fence));
    assert!(is_url_allowed("https://old.reddit.com", &fence));
    assert!(!is_url_allowed("https://evilinstagram.com", &fence));
    assert!(!is_url_allowed("https://instagram.com.evil.io", &fence));
    assert!(!is_url_allowed("file:///etc/passwd", &fence));
    assert!(is_url_allowed("about:blank", &fence));
    assert!(is_url_allowed("data:text/html,hi", &fence));
    assert!(is_url_allowed("https://anything.example", &[]));
}

#[test]
fn endpoint_normalisation() {
    assert_eq!(devtools_http_base("ws://localhost:9222"), "http://localhost:9222");
    assert_eq!(
        devtools_http_base("ws://127.0.0.1:9333/devtools/browser/abc"),
        "http://127.0.0.1:9333"
    );
    assert_eq!(devtools_http_base("localhost:9222"), "http://localhost:9222");
    assert_eq!(devtools_http_base("https://relay.local:443/"), "https://relay.local");
}

#[tokio::test]
async fn drives_a_real_browser_over_cdp() {
    let Some(mut chrome) = launch().await else { return };
    let ws_endpoint = chrome.http_base().replace("http://", "ws://");
    let adapter = ChromeRelayAdapter::new(ChromeRelayConfig {
        endpoint: ws_endpoint,
        tab_lease_id: Some("not-an-existing-target".into()),
        origin_fence: vec![],
    });

    adapter.navigate(&data_url(FORM_PAGE)).await.unwrap();
    let state = adapter.get_page_state().await.unwrap();
    assert_eq!(state.title, "Relay Test");
    assert!(state.is_ready);
    assert!(state.text_content.contains("idle"));
    assert!(state.pending_challenge.is_none());

    adapter.click("#go").await.unwrap();
    let status = adapter
        .evaluate_script("document.getElementById('status').textContent")
        .await
        .unwrap();
    assert_eq!(status, "clicked");

    adapter.type_text("#name", "hello \"world\"", false).await.unwrap();
    let echo = adapter
        .evaluate_script("document.getElementById('echo').textContent")
        .await
        .unwrap();
    assert_eq!(echo, "hello \"world\"");

    let missing = adapter.click("#does-not-exist").await;
    assert!(matches!(missing, Err(OpenAccountsError::Browser(_))));

    let thrown = adapter.evaluate_script("(() => { throw new Error('boom') })()").await;
    assert!(matches!(thrown, Err(OpenAccountsError::Browser(ref m)) if m.contains("boom")));

    let awaited = adapter
        .evaluate_script("new Promise(r => setTimeout(() => r(41 + 1), 20))")
        .await
        .unwrap();
    assert_eq!(awaited, 42);

    let png = adapter.take_screenshot().await.unwrap();
    assert!(png.len() > 100);
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");

    adapter.navigate(&data_url(OTP_PAGE)).await.unwrap();
    let challenge = adapter.get_page_state().await.unwrap().pending_challenge.unwrap();
    assert_eq!(challenge.challenge_type, ChallengeType::TwoFactorCode);
    assert!(challenge.element_hint.unwrap().contains("code"));

    adapter.pause().await.unwrap();
    let blocked = adapter.click("input").await;
    assert!(matches!(blocked, Err(OpenAccountsError::HumanInterventionRequired(_))));
    assert!(adapter.get_page_state().await.is_ok());
    adapter.resume().await.unwrap();
    assert!(adapter.evaluate_script("1 + 1").await.is_ok());

    adapter.close().await.unwrap();
    chrome.shutdown().await;
}

#[tokio::test]
async fn origin_fence_blocks_off_fence_navigation() {
    let Some(mut chrome) = launch().await else { return };
    let adapter = ChromeRelayAdapter::new(ChromeRelayConfig {
        endpoint: chrome.http_base().to_string(),
        tab_lease_id: None,
        origin_fence: vec!["example.com".into()],
    });

    let denied = adapter.navigate("https://instagram.com/").await;
    assert!(matches!(denied, Err(OpenAccountsError::PermissionDenied(_))));
    adapter.navigate("about:blank").await.unwrap();

    adapter.close().await.unwrap();
    chrome.shutdown().await;
}

#[tokio::test]
async fn attaches_to_existing_tab_by_lease_id() {
    let Some(mut chrome) = launch().await else { return };
    let first = ChromeRelayAdapter::new(ChromeRelayConfig {
        endpoint: chrome.http_base().to_string(),
        tab_lease_id: None,
        origin_fence: vec![],
    });
    first.navigate(&data_url(FORM_PAGE)).await.unwrap();
    let target = first.driver().target_id().await.unwrap();

    let second = ChromeRelayAdapter::new(ChromeRelayConfig {
        endpoint: chrome.http_base().to_string(),
        tab_lease_id: Some(target.clone()),
        origin_fence: vec![],
    });
    let state = second.get_page_state().await.unwrap();
    assert_eq!(state.title, "Relay Test");
    assert_eq!(second.driver().target_id().await.unwrap(), target);

    first.close().await.unwrap();
    chrome.shutdown().await;
}

#[tokio::test]
async fn unreachable_endpoint_is_reported() {
    let adapter = ChromeRelayAdapter::new(ChromeRelayConfig {
        endpoint: "ws://127.0.0.1:1".into(),
        tab_lease_id: None,
        origin_fence: vec![],
    });
    let err = adapter.navigate("about:blank").await.unwrap_err();
    assert!(matches!(err, OpenAccountsError::Browser(ref m) if m.contains("unreachable")));
}
