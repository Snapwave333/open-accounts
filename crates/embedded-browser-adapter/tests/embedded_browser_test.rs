use base64::{engine::general_purpose::STANDARD, Engine};
use openaccounts_browser_abstraction::{BrowserProvider, ChallengeType};
use openaccounts_core::OpenAccountsError;
use openaccounts_embedded_browser_adapter::{find_browser_binary, EmbeddedBrowserAdapter};

fn data_url(html: &str) -> String {
    format!("data:text/html;base64,{}", STANDARD.encode(html))
}

#[tokio::test]
async fn launches_headless_browser_and_drives_it() {
    if find_browser_binary().is_none() {
        eprintln!("skipping: no Chrome/Chromium binary available");
        return;
    }
    let adapter = EmbeddedBrowserAdapter::new();
    assert!(!adapter.is_running().await);

    let page = r#"<!doctype html><html><head><title>Embedded</title></head><body>
<span id="out">waiting</span>
<button id="btn" onclick="document.getElementById('out').textContent='pressed'">Press</button>
<input id="field">
<label><input type="checkbox" id="tos"> I agree to the Terms of Service</label>
</body></html>"#;
    adapter.navigate(&data_url(page)).await.unwrap();
    assert!(adapter.is_running().await);
    assert!(adapter.devtools_endpoint().await.unwrap().starts_with("http://127.0.0.1:"));

    adapter.click("#btn").await.unwrap();
    assert_eq!(
        adapter.evaluate_script("document.getElementById('out').textContent").await.unwrap(),
        "pressed"
    );

    adapter.type_text("#field", "s3cret-value", true).await.unwrap();
    assert_eq!(
        adapter.evaluate_script("document.getElementById('field').value").await.unwrap(),
        "s3cret-value"
    );

    let state = adapter.get_page_state().await.unwrap();
    assert_eq!(state.title, "Embedded");
    assert_eq!(
        state.pending_challenge.map(|c| c.challenge_type),
        Some(ChallengeType::LegalConsent)
    );

    let png = adapter.take_screenshot().await.unwrap();
    assert_eq!(&png[..4], b"\x89PNG");

    adapter.close().await.unwrap();
    assert!(!adapter.is_running().await);
}

#[tokio::test]
async fn pause_before_launch_is_honoured_and_fence_applies() {
    if find_browser_binary().is_none() {
        eprintln!("skipping: no Chrome/Chromium binary available");
        return;
    }
    let adapter = EmbeddedBrowserAdapter::new().with_origin_fence(vec!["*.youtube.com".into()]);
    adapter.pause().await.unwrap();
    let blocked = adapter.navigate("about:blank").await;
    assert!(matches!(blocked, Err(OpenAccountsError::HumanInterventionRequired(_))));
    adapter.resume().await.unwrap();

    let denied = adapter.navigate("https://tiktok.com/").await;
    assert!(matches!(denied, Err(OpenAccountsError::PermissionDenied(_))));

    let captcha = r#"<!doctype html><html><body><div class="g-recaptcha" data-sitekey="abc"></div></body></html>"#;
    adapter.navigate(&data_url(captcha)).await.unwrap();
    let challenge = adapter.get_page_state().await.unwrap().pending_challenge.unwrap();
    assert_eq!(challenge.challenge_type, ChallengeType::Captcha);

    adapter.close().await.unwrap();
}

#[tokio::test]
async fn missing_binary_reports_clear_error() {
    let adapter = EmbeddedBrowserAdapter::new().with_binary("/nonexistent/chrome-binary");
    let err = adapter.navigate("about:blank").await.unwrap_err();
    assert!(matches!(err, OpenAccountsError::Browser(ref m) if m.contains("Failed to launch")));
}
