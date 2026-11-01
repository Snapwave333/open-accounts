use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use openaccounts_email_provider_gateway::{
    build_raw_message, decode_base64_any, encode_raw_message_for_gmail, extract_gmail_bodies, graph_draft_body,
    html_to_text, parse_gmail_message, parse_graph_message, plus_alias, sanitize_alias_tag, EmailProviderAdapter,
    GmailApiAdapter, MicrosoftGraphAdapter, GMAIL_TOKEN_ENV,
};
use serde_json::json;

fn b64(s: &str) -> String {
    URL_SAFE_NO_PAD.encode(s.as_bytes())
}

#[test]
fn plus_alias_builds_deliverable_tagged_address() {
    let alias = plus_alias("Eli.Kenny@Gmail.com", "Brand Actor #1!", "a1B2c3").unwrap();
    assert_eq!(alias, "Eli.Kenny+brand-actor-1-a1b2c3@gmail.com");

    let retagged = plus_alias("ops+old@outlook.com", "instagram", "zz9").unwrap();
    assert_eq!(retagged, "ops+instagram-zz9@outlook.com");

    assert!(plus_alias("not-an-email", "x", "1").is_err());
    assert!(plus_alias("user@localhost", "x", "1").is_err());
}

#[test]
fn alias_tag_sanitising_handles_empty_and_long_prefixes() {
    assert_eq!(sanitize_alias_tag("@@@"), "alias");
    assert_eq!(sanitize_alias_tag("  ..Hello__World--  "), "hello__world");
    assert!(sanitize_alias_tag(&"x".repeat(100)).len() <= 32);
}

#[test]
fn raw_message_is_rfc2822_and_rejects_header_injection() {
    let raw = build_raw_message("me@gmail.com", "you@example.com", "Héllo", "Body line").unwrap();
    assert!(raw.contains("From: me@gmail.com\r\n"));
    assert!(raw.contains("To: you@example.com\r\n"));
    assert!(raw.contains("Subject: =?UTF-8?B?"));
    assert!(raw.contains("MIME-Version: 1.0\r\n"));
    let body = raw.split("\r\n\r\n").nth(1).unwrap();
    let decoded = decode_base64_any(&body.replace("\r\n", "")).unwrap();
    assert_eq!(String::from_utf8(decoded).unwrap(), "Body line");

    assert!(build_raw_message("me@gmail.com", "you@example.com\r\nBcc: evil@x.com", "s", "b").is_err());
    assert!(build_raw_message("me@gmail.com", "you@example.com", "s\nBcc: x", "b").is_err());

    let encoded = encode_raw_message_for_gmail(&raw);
    assert!(!encoded.contains('+') && !encoded.contains('/') && !encoded.contains('='));
}

#[test]
fn gmail_multipart_payload_is_decoded_recursively() {
    let fixture = json!({
        "id": "18c0ffee",
        "internalDate": "1761955200000",
        "snippet": "ignored",
        "payload": {
            "mimeType": "multipart/mixed",
            "headers": [
                { "name": "From", "value": "Instagram <security@mail.instagram.com>" },
                { "name": "To", "value": "eli+ig-abc123@gmail.com" },
                { "name": "Subject", "value": "Confirm your account" }
            ],
            "parts": [
                {
                    "mimeType": "multipart/alternative",
                    "parts": [
                        { "mimeType": "text/plain", "body": { "data": b64("Your code is 381904") } },
                        { "mimeType": "text/html", "body": { "data": b64("<p>Your code is <b>381904</b></p>") } }
                    ]
                },
                { "mimeType": "application/pdf", "filename": "terms.pdf", "body": { "attachmentId": "att" } }
            ]
        }
    });
    let msg = parse_gmail_message(&fixture).unwrap();
    assert_eq!(msg.message_id, "18c0ffee");
    assert_eq!(msg.sender, "Instagram <security@mail.instagram.com>");
    assert_eq!(msg.recipient, "eli+ig-abc123@gmail.com");
    assert_eq!(msg.subject, "Confirm your account");
    assert_eq!(msg.body_plain, "Your code is 381904");
    assert!(msg.body_html.unwrap().contains("<b>381904</b>"));
    assert_eq!(msg.received_at.timestamp(), 1_761_955_200);
}

#[test]
fn gmail_html_only_payload_falls_back_to_text_conversion() {
    let payload = json!({
        "mimeType": "text/html",
        "body": { "data": b64("<html><style>p{}</style><p>Tap <a href=\"https://x.com/verify?t=1\">here</a>&nbsp;now</p></html>") }
    });
    let (plain, html) = extract_gmail_bodies(&payload);
    assert!(html.is_some());
    assert_eq!(plain, "Tap https://x.com/verify?t=1 here now");
}

#[test]
fn graph_message_fixture_is_parsed() {
    let fixture = json!({
        "id": "AAMk-1",
        "subject": "Your Reddit verification code",
        "receivedDateTime": "2026-11-01T06:00:00Z",
        "from": { "emailAddress": { "address": "noreply@reddit.com", "name": "Reddit" } },
        "toRecipients": [ { "emailAddress": { "address": "focogaming+reddit@outlook.com" } } ],
        "body": { "contentType": "html", "content": "<div>Code: 552310</div>" }
    });
    let msg = parse_graph_message(&fixture).unwrap();
    assert_eq!(msg.sender, "noreply@reddit.com");
    assert_eq!(msg.recipient, "focogaming+reddit@outlook.com");
    assert_eq!(msg.body_plain, "Code: 552310");
    assert_eq!(msg.received_at.to_rfc3339(), "2026-11-01T06:00:00+00:00");
}

#[test]
fn graph_draft_body_shape() {
    let body = graph_draft_body("a@b.com", "Hi", "Text").unwrap();
    assert_eq!(body["toRecipients"][0]["emailAddress"]["address"], "a@b.com");
    assert_eq!(body["body"]["contentType"], "Text");
    assert!(graph_draft_body("nope", "Hi", "Text").is_err());
}

#[test]
fn html_to_text_strips_scripts() {
    assert_eq!(html_to_text("<script>var a=1;</script><b>Hi</b> &amp; bye"), "Hi & bye");
}

#[tokio::test]
async fn adapters_surface_http_errors_instead_of_fake_success() {
    let gmail = GmailApiAdapter::with_base_url("bad-token", "http://127.0.0.1:9");
    assert!(gmail.mailbox_address().await.is_err());
    assert!(gmail.create_alias_or_mailbox("x").await.is_err());

    let graph = MicrosoftGraphAdapter::with_base_url("bad-token", "http://127.0.0.1:9");
    assert!(graph.fetch_recent_messages(5, None).await.is_err());
    assert!(graph.send_email("a@b.com", "s", "b").await.is_err());

    if std::env::var(GMAIL_TOKEN_ENV).is_err() {
        assert!(GmailApiAdapter::from_env().is_none());
    }
}
