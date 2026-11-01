use async_trait::async_trait;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use openaccounts_core::Result;
use openaccounts_email_provider_gateway::{EmailProviderAdapter, EmailProviderType, IncomingEmailMessage};
use openaccounts_email_verification::{extract_otp, extract_verification_link, EmailVerificationEngine};
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

fn message(id: &str, sender: &str, recipient: &str, subject: &str, body: &str, at: DateTime<Utc>) -> IncomingEmailMessage {
    IncomingEmailMessage {
        message_id: id.into(),
        sender: sender.into(),
        recipient: recipient.into(),
        subject: subject.into(),
        body_plain: body.into(),
        body_html: None,
        received_at: at,
    }
}

struct ScriptedInbox {
    polls: AtomicUsize,
    deliver_on_poll: usize,
    pending: Mutex<Vec<IncomingEmailMessage>>,
    old: Vec<IncomingEmailMessage>,
}

#[async_trait]
impl EmailProviderAdapter for ScriptedInbox {
    fn provider_type(&self) -> EmailProviderType {
        EmailProviderType::GmailApi
    }

    async fn mailbox_address(&self) -> Result<String> {
        Ok("owner@gmail.com".into())
    }

    async fn send_email(&self, _to: &str, _subject: &str, _body: &str) -> Result<String> {
        Ok("id".into())
    }

    async fn fetch_recent_messages(&self, _limit: u32, _since: Option<DateTime<Utc>>) -> Result<Vec<IncomingEmailMessage>> {
        let n = self.polls.fetch_add(1, Ordering::SeqCst) + 1;
        let mut out = self.old.clone();
        if n >= self.deliver_on_poll {
            let pending = self.pending.lock().unwrap();
            out.extend(pending.iter().map(|m| IncomingEmailMessage { received_at: Utc::now(), ..m.clone() }));
        }
        Ok(out)
    }

    async fn create_alias_or_mailbox(&self, prefix: &str) -> Result<String> {
        Ok(format!("owner+{prefix}@gmail.com"))
    }
}

#[test]
fn otp_extraction_prefers_labelled_codes_and_skips_years() {
    assert_eq!(extract_otp("© 2026 Instagram. Your code is 481920."), Some("481920".into()));
    assert_eq!(extract_otp("Use verification code: 5521 to continue"), Some("5521".into()));
    assert_eq!(extract_otp("492810 is your Instagram code"), Some("492810".into()));
    assert_eq!(extract_otp("Security code 123-456"), Some("123456".into()));
    assert_eq!(extract_otp("Copyright 2025 Reddit, Inc. Order 99812345"), Some("99812345".into()));
    assert_eq!(extract_otp("Welcome! Since 2019 we have grown."), None);
    assert_eq!(extract_otp("Your PIN: 2024"), Some("2024".into()));
}

#[test]
fn verification_links_are_trimmed() {
    assert_eq!(
        extract_verification_link("Click https://www.reddit.com/verification/abc?x=1&amp;y=2. Thanks"),
        Some("https://www.reddit.com/verification/abc?x=1&y=2".into())
    );
    assert_eq!(extract_verification_link("Visit https://example.com/home"), None);
}

#[tokio::test]
async fn wait_polls_until_a_fresh_matching_email_arrives() {
    let stale = message(
        "old",
        "security@mail.instagram.com",
        "owner+ig@gmail.com",
        "Your code is 111111",
        "",
        Utc::now() - ChronoDuration::hours(2),
    );
    let fresh = message(
        "new",
        "Instagram <security@mail.instagram.com>",
        "owner+ig@gmail.com",
        "Confirm your account",
        "Your Instagram code is 492810",
        Utc::now(),
    );
    let inbox = Arc::new(ScriptedInbox {
        polls: AtomicUsize::new(0),
        deliver_on_poll: 3,
        pending: Mutex::new(vec![fresh]),
        old: vec![stale],
    });

    let engine = EmailVerificationEngine::with_poll_interval(Duration::from_millis(20));
    let found = engine
        .wait_for_verification_email_to(inbox.clone(), "instagram.com", Some("owner+ig@gmail.com"), 5)
        .await
        .unwrap();
    assert_eq!(found.code.as_deref(), Some("492810"));
    assert!(!found.is_expired);
    assert!(inbox.polls.load(Ordering::SeqCst) >= 3);
}

#[tokio::test]
async fn wait_times_out_when_nothing_relevant_arrives() {
    let unrelated = message("n1", "news@shop.com", "owner@gmail.com", "Weekly deals", "Save 20%", Utc::now());
    let inbox = Arc::new(ScriptedInbox {
        polls: AtomicUsize::new(0),
        deliver_on_poll: 1,
        pending: Mutex::new(vec![unrelated]),
        old: vec![],
    });
    let engine = EmailVerificationEngine::with_poll_interval(Duration::from_millis(20));
    let started = std::time::Instant::now();
    let err = engine
        .wait_for_verification_email(inbox.clone(), "reddit.com", 1)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("reddit.com"));
    assert!(started.elapsed() >= Duration::from_millis(900));
    assert!(inbox.polls.load(Ordering::SeqCst) > 5);
}
