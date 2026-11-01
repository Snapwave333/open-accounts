use chrono::{DateTime, Duration as ChronoDuration, Utc};
use openaccounts_core::{OpenAccountsError, Result};
use openaccounts_email_provider_gateway::{EmailProviderAdapter, IncomingEmailMessage};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    sync::{Arc, OnceLock},
    time::Duration,
};
use tracing::{debug, info, warn};

pub const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(5);
pub const CODE_VALIDITY_MINUTES: i64 = 15;
const CLOCK_SKEW_SECONDS: i64 = 5;
const VERIFICATION_KEYWORDS: &[&str] = &[
    "verification",
    "verify",
    "confirm",
    "confirmation",
    "one-time",
    "one time",
    "otp",
    "security code",
    "passcode",
    "login code",
    "sign-in code",
    "activate",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationCodeExtraction {
    pub code: Option<String>,
    pub verification_link: Option<String>,
    pub sender: String,
    pub extracted_at: DateTime<Utc>,
    pub is_expired: bool,
}

fn labelled_after_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)\b(?:code|otp|pin|passcode|password|verification|security code|one[- ]time code)\b[^0-9\n]{0,30}?\b(\d{3}[- ]\d{3}|\d{4,8})\b",
        )
        .expect("valid labelled-after regex")
    })
}

fn labelled_before_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)\b(\d{3}[- ]\d{3}|\d{4,8})\b\s+(?:is\s+)?(?:your\s+)?(?:[a-z]+\s+){0,2}(?:code|otp|pin|passcode)\b")
            .expect("valid labelled-before regex")
    })
}

fn bare_number_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b(\d{4,8})\b").expect("valid bare-number regex"))
}

fn link_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?i)https?://[^\s<>"']*(?:verify|verification|confirm|activate|token|code|auth)[^\s<>"']*"#)
            .expect("valid link regex")
    })
}

fn normalize_code(raw: &str) -> String {
    raw.chars().filter(|c| c.is_ascii_digit()).collect()
}

fn looks_like_year(code: &str) -> bool {
    code.len() == 4 && code.parse::<u32>().map(|y| (1900..=2099).contains(&y)).unwrap_or(false)
}

pub fn extract_otp(text: &str) -> Option<String> {
    for re in [labelled_after_re(), labelled_before_re()] {
        if let Some(code) = re
            .captures_iter(text)
            .map(|cap| normalize_code(&cap[1]))
            .find(|c| (4..=8).contains(&c.len()))
        {
            return Some(code);
        }
    }

    let candidates: Vec<String> = bare_number_re()
        .captures_iter(text)
        .map(|cap| cap[1].to_string())
        .filter(|c| !looks_like_year(c))
        .collect();
    candidates
        .iter()
        .find(|c| c.len() == 6)
        .or_else(|| candidates.first())
        .cloned()
}

pub fn extract_verification_link(text: &str) -> Option<String> {
    link_re()
        .find(text)
        .map(|m| m.as_str().trim_end_matches(['.', ',', ')', ']', ';']).replace("&amp;", "&"))
}

pub fn is_verification_candidate(message: &IncomingEmailMessage, expected_sender_domain: &str) -> bool {
    let domain = expected_sender_domain.trim().to_lowercase();
    if !domain.is_empty() && message.sender.to_lowercase().contains(&domain) {
        return true;
    }
    let haystack = format!("{} {}", message.subject, message.body_plain).to_lowercase();
    VERIFICATION_KEYWORDS.iter().any(|k| haystack.contains(k))
}

#[derive(Debug, Clone)]
pub struct EmailVerificationEngine {
    poll_interval: Duration,
}

impl EmailVerificationEngine {
    pub fn new() -> Self {
        Self { poll_interval: DEFAULT_POLL_INTERVAL }
    }

    pub fn with_poll_interval(poll_interval: Duration) -> Self {
        Self { poll_interval: poll_interval.max(Duration::from_millis(10)) }
    }

    pub fn poll_interval(&self) -> Duration {
        self.poll_interval
    }

    pub fn extract_code_or_link(&self, message: &IncomingEmailMessage) -> VerificationCodeExtraction {
        let code = extract_otp(&message.subject).or_else(|| extract_otp(&message.body_plain));
        let verification_link = extract_verification_link(&message.body_plain).or_else(|| {
            message
                .body_html
                .as_deref()
                .and_then(extract_verification_link)
        });

        VerificationCodeExtraction {
            code,
            verification_link,
            sender: message.sender.clone(),
            extracted_at: Utc::now(),
            is_expired: Utc::now() - message.received_at > ChronoDuration::minutes(CODE_VALIDITY_MINUTES),
        }
    }

    pub async fn wait_for_verification_email(
        &self,
        provider: Arc<dyn EmailProviderAdapter>,
        expected_sender_domain: &str,
        timeout_seconds: u64,
    ) -> Result<VerificationCodeExtraction> {
        self.wait_for_verification_email_to(provider, expected_sender_domain, None, timeout_seconds)
            .await
    }

    pub async fn wait_for_verification_email_to(
        &self,
        provider: Arc<dyn EmailProviderAdapter>,
        expected_sender_domain: &str,
        recipient: Option<&str>,
        timeout_seconds: u64,
    ) -> Result<VerificationCodeExtraction> {
        let started = Utc::now();
        let cutoff = started - ChronoDuration::seconds(CLOCK_SKEW_SECONDS);
        let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_seconds);
        let recipient = recipient.map(|r| r.trim().to_lowercase()).filter(|r| !r.is_empty());
        let mut seen: HashSet<String> = HashSet::new();
        let mut last_error: Option<String>;

        info!(
            "Waiting up to {}s for verification email from {}",
            timeout_seconds, expected_sender_domain
        );

        loop {
            match provider.fetch_recent_messages(20, Some(cutoff)).await {
                Ok(messages) => {
                    last_error = None;
                    for msg in messages {
                        if msg.received_at < cutoff || !seen.insert(msg.message_id.clone()) {
                            continue;
                        }
                        if let Some(r) = recipient.as_deref() {
                            if !msg.recipient.to_lowercase().contains(r) {
                                continue;
                            }
                        }
                        if !is_verification_candidate(&msg, expected_sender_domain) {
                            continue;
                        }
                        let extraction = self.extract_code_or_link(&msg);
                        if extraction.code.is_some() || extraction.verification_link.is_some() {
                            info!("Extracted verification token from message {}", msg.message_id);
                            return Ok(extraction);
                        }
                        debug!("Message {} matched but contained no code or link", msg.message_id);
                    }
                }
                Err(e) => {
                    warn!("Mailbox poll failed: {}", e);
                    last_error = Some(e.to_string());
                }
            }

            let now = tokio::time::Instant::now();
            if now >= deadline {
                break;
            }
            tokio::time::sleep(self.poll_interval.min(deadline - now)).await;
        }

        Err(OpenAccountsError::Internal(match last_error {
            Some(e) => format!(
                "No verification email from {expected_sender_domain} within {timeout_seconds}s (last mailbox error: {e})"
            ),
            None => format!("No verification email from {expected_sender_domain} within {timeout_seconds}s"),
        }))
    }
}

impl Default for EmailVerificationEngine {
    fn default() -> Self {
        Self::new()
    }
}
