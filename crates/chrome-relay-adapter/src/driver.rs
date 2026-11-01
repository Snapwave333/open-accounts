use crate::cdp::{close_tab, devtools_http_client, find_tab, open_tab, CdpSession};
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD, Engine};
use openaccounts_browser_abstraction::{BrowserProvider, ChallengeDetection, ChallengeType, PageState};
use openaccounts_core::{OpenAccountsError, Result};
use serde_json::{json, Value};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::sync::{broadcast::error::RecvError, Mutex};
use tracing::{info, warn};

pub const NAVIGATION_TIMEOUT: Duration = Duration::from_secs(30);
const READY_POLL_INTERVAL: Duration = Duration::from_millis(100);
const PAGE_TEXT_LIMIT: usize = 20_000;

fn normalize_fence_entry(entry: &str) -> String {
    let e = entry.trim().to_lowercase();
    let e = e.split("://").last().unwrap_or("").to_string();
    let e = e.split(['/', '?', '#']).next().unwrap_or("").to_string();
    let e = e.trim_start_matches("*.").trim_start_matches('.').to_string();
    match e.rsplit_once(':') {
        Some((host, port)) if port.chars().all(|c| c.is_ascii_digit()) => host.to_string(),
        _ => e,
    }
}

pub fn is_url_allowed(url: &str, origin_fence: &[String]) -> bool {
    let lower = url.trim().to_lowercase();
    if lower == "about:blank" || lower.starts_with("data:") {
        return true;
    }
    if origin_fence.is_empty() {
        return true;
    }
    let Ok(parsed) = reqwest::Url::parse(url.trim()) else {
        return false;
    };
    if !matches!(parsed.scheme(), "http" | "https") {
        return false;
    }
    let Some(host) = parsed.host_str().map(|h| h.trim_end_matches('.').to_lowercase()) else {
        return false;
    };
    origin_fence.iter().any(|entry| {
        let fence = normalize_fence_entry(entry);
        fence == "*" || (!fence.is_empty() && (host == fence || host.ends_with(&format!(".{fence}"))))
    })
}

const PAGE_STATE_SCRIPT: &str = r#"(() => {
  const describe = (el) => {
    if (!el) return null;
    let d = el.tagName.toLowerCase();
    if (el.id) d += '#' + el.id;
    if (el.getAttribute && el.getAttribute('name')) d += '[name="' + el.getAttribute('name') + '"]';
    return d;
  };
  const q = (s) => document.querySelector(s);
  const body = document.body ? (document.body.innerText || '') : '';
  const lower = body.toLowerCase();
  let challenge = null;
  const captcha = q('iframe[src*="recaptcha"], iframe[src*="hcaptcha"], iframe[src*="challenges.cloudflare.com"], script[src*="recaptcha"], script[src*="hcaptcha"], script[src*="turnstile"], .g-recaptcha, .h-captcha, .cf-turnstile, [data-sitekey]');
  if (captcha) {
    challenge = { kind: 'Captcha', description: 'CAPTCHA widget present on page', hint: describe(captcha) };
  } else if (/verify (that )?you are (a )?human|are you a robot|i'm not a robot|checking your browser|security check/.test(lower)) {
    challenge = { kind: 'Captcha', description: 'Human verification prompt detected', hint: null };
  }
  if (!challenge) {
    const otp = q('input[autocomplete="one-time-code"], input[name*="otp" i], input[id*="otp" i], input[name*="2fa" i], input[name*="verification_code" i], input[name*="verificationcode" i], input[name*="security_code" i]');
    if (otp || /enter the (\d+[- ]digit )?(verification |security )?code|two-factor|2-step verification|authentication code|one-time code/.test(lower)) {
      challenge = { kind: 'TwoFactorCode', description: 'One-time verification code requested', hint: describe(otp) };
    }
  }
  if (!challenge) {
    const age = q('input[name*="birth" i], select[name*="birth" i], input[name*="dob" i], select[name*="dob" i], input[name="age" i], input[id*="birthday" i]');
    if (age || /date of birth|confirm your age|are you (over )?18|verify your age|enter your birthday/.test(lower)) {
      challenge = { kind: 'AgeVerification', description: 'Age verification gate detected', hint: describe(age) };
    }
  }
  if (!challenge) {
    const consent = Array.from(document.querySelectorAll('input[type="checkbox"]')).find((cb) => {
      let label = '';
      const wrap = cb.closest('label');
      if (wrap) label = wrap.innerText || '';
      if (!label && cb.id) {
        const forLabel = document.querySelector('label[for="' + CSS.escape(cb.id) + '"]');
        if (forLabel) label = forLabel.innerText || '';
      }
      label = (label + ' ' + (cb.name || '') + ' ' + (cb.getAttribute('aria-label') || '')).toLowerCase();
      return /terms|privacy|consent|agree|policy/.test(label);
    });
    if (consent || /i agree to the terms|accept the terms|terms of service and privacy policy/.test(lower)) {
      challenge = { kind: 'LegalConsent', description: 'Terms or consent acceptance required', hint: describe(consent) };
    }
  }
  return {
    url: location.href,
    title: document.title || '',
    text: body.slice(0, __LIMIT__),
    ready: document.readyState === 'complete',
    challenge,
  };
})()"#;

fn challenge_from_kind(kind: &str) -> ChallengeType {
    match kind {
        "Captcha" => ChallengeType::Captcha,
        "TwoFactorCode" => ChallengeType::TwoFactorCode,
        "AgeVerification" => ChallengeType::AgeVerification,
        "LegalConsent" => ChallengeType::LegalConsent,
        _ => ChallengeType::ManualReview,
    }
}

pub fn page_state_from_value(val: &Value) -> PageState {
    let pending_challenge = val.get("challenge").filter(|c| c.is_object()).map(|c| ChallengeDetection {
        challenge_type: challenge_from_kind(c.get("kind").and_then(Value::as_str).unwrap_or_default()),
        description: c.get("description").and_then(Value::as_str).unwrap_or_default().to_string(),
        element_hint: c.get("hint").and_then(Value::as_str).map(str::to_string),
    });
    PageState {
        url: val.get("url").and_then(Value::as_str).unwrap_or_default().to_string(),
        title: val.get("title").and_then(Value::as_str).unwrap_or_default().to_string(),
        text_content: val.get("text").and_then(Value::as_str).unwrap_or_default().to_string(),
        is_ready: val.get("ready").and_then(Value::as_bool).unwrap_or(false),
        pending_challenge,
    }
}

fn js_string(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into())
}

struct PageHandle {
    target_id: String,
    session: CdpSession,
}

pub struct CdpDriver {
    http_base: String,
    origin_fence: Vec<String>,
    attach_target: Option<String>,
    http: reqwest::Client,
    page: Mutex<Option<Arc<PageHandle>>>,
    paused: AtomicBool,
}

impl CdpDriver {
    pub fn new(http_base: impl Into<String>, origin_fence: Vec<String>) -> Self {
        Self {
            http_base: http_base.into(),
            origin_fence,
            attach_target: None,
            http: devtools_http_client(),
            page: Mutex::new(None),
            paused: AtomicBool::new(false),
        }
    }

    pub fn with_attach_target(mut self, target_id: Option<String>) -> Self {
        self.attach_target = target_id.filter(|t| !t.trim().is_empty());
        self
    }

    pub fn http_base(&self) -> &str {
        &self.http_base
    }

    pub fn origin_fence(&self) -> &[String] {
        &self.origin_fence
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::SeqCst);
    }

    pub async fn target_id(&self) -> Option<String> {
        self.page.lock().await.as_ref().map(|p| p.target_id.clone())
    }

    fn ensure_active(&self) -> Result<()> {
        if self.is_paused() {
            Err(OpenAccountsError::HumanInterventionRequired(
                "Browser session is paused for human takeover".into(),
            ))
        } else {
            Ok(())
        }
    }

    async fn page(&self) -> Result<Arc<PageHandle>> {
        let mut guard = self.page.lock().await;
        if let Some(existing) = guard.as_ref() {
            if !existing.session.is_closed() {
                return Ok(existing.clone());
            }
        }

        let attached = match &self.attach_target {
            Some(id) => find_tab(&self.http, &self.http_base, id).await?,
            None => None,
        };
        let target = match attached {
            Some(t) => t,
            None => open_tab(&self.http, &self.http_base, "about:blank").await?,
        };
        let session = CdpSession::connect(&target.ws_url).await?;
        session.call("Page.enable", json!({})).await?;
        info!("CDP session attached to target {}", target.id);

        let handle = Arc::new(PageHandle { target_id: target.id, session });
        *guard = Some(handle.clone());
        Ok(handle)
    }

    async fn evaluate(&self, page: &PageHandle, expression: &str) -> Result<Value> {
        let res = page
            .session
            .call(
                "Runtime.evaluate",
                json!({ "expression": expression, "returnByValue": true, "awaitPromise": true }),
            )
            .await?;
        if let Some(ex) = res.get("exceptionDetails") {
            let message = ex
                .pointer("/exception/description")
                .and_then(Value::as_str)
                .or_else(|| ex.get("text").and_then(Value::as_str))
                .unwrap_or("script threw an exception");
            return Err(OpenAccountsError::Browser(format!("Script error: {message}")));
        }
        Ok(res.pointer("/result/value").cloned().unwrap_or(Value::Null))
    }

    async fn wait_until_ready(&self, page: &PageHandle, deadline: Instant) -> bool {
        while Instant::now() < deadline {
            if let Ok(Value::String(state)) = self.evaluate(page, "document.readyState").await {
                if state == "complete" {
                    return true;
                }
            }
            tokio::time::sleep(READY_POLL_INTERVAL).await;
        }
        false
    }

    async fn current_url(&self, page: &PageHandle) -> Option<String> {
        self.evaluate(page, "location.href").await.ok()?.as_str().map(str::to_string)
    }
}

#[async_trait]
impl BrowserProvider for CdpDriver {
    async fn navigate(&self, url: &str) -> Result<()> {
        self.ensure_active()?;
        if !is_url_allowed(url, &self.origin_fence) {
            return Err(OpenAccountsError::PermissionDenied(format!(
                "Navigation to {url} is outside the session origin fence"
            )));
        }
        let page = self.page().await?;
        let mut events = page.session.subscribe();
        let deadline = Instant::now() + NAVIGATION_TIMEOUT;

        let res = page.session.call("Page.navigate", json!({ "url": url })).await?;
        if let Some(err) = res.get("errorText").and_then(Value::as_str).filter(|e| !e.is_empty()) {
            return Err(OpenAccountsError::Browser(format!("Navigation to {url} failed: {err}")));
        }

        if res.get("loaderId").and_then(Value::as_str).is_some() {
            loop {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    break;
                }
                match tokio::time::timeout(remaining, events.recv()).await {
                    Ok(Ok(evt)) if evt.method == "Page.loadEventFired" => break,
                    Ok(Ok(_)) | Ok(Err(RecvError::Lagged(_))) => continue,
                    Ok(Err(RecvError::Closed)) | Err(_) => break,
                }
            }
        }

        if !self.wait_until_ready(&page, deadline).await {
            warn!("Page {} did not reach readyState=complete before timeout", url);
        }

        if let Some(final_url) = self.current_url(&page).await {
            if !is_url_allowed(&final_url, &self.origin_fence) {
                let _ = page.session.call("Page.navigate", json!({ "url": "about:blank" })).await;
                return Err(OpenAccountsError::PermissionDenied(format!(
                    "Navigation redirected to {final_url}, outside the session origin fence"
                )));
            }
        }
        Ok(())
    }

    async fn click(&self, selector: &str) -> Result<()> {
        self.ensure_active()?;
        let page = self.page().await?;
        let script = format!(
            "(() => {{ const el = document.querySelector({sel}); if (!el) return false; el.scrollIntoView({{block: 'center'}}); el.click(); return true; }})()",
            sel = js_string(selector)
        );
        match self.evaluate(&page, &script).await? {
            Value::Bool(true) => Ok(()),
            _ => Err(OpenAccountsError::Browser(format!("Element not found: {selector}"))),
        }
    }

    async fn type_text(&self, selector: &str, text: &str, is_secret: bool) -> Result<()> {
        self.ensure_active()?;
        let page = self.page().await?;
        let focus_script = format!(
            "(() => {{ const el = document.querySelector({sel}); if (!el) return false; el.focus(); \
if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement) {{ \
const proto = el instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype; \
const setter = Object.getOwnPropertyDescriptor(proto, 'value').set; setter.call(el, ''); \
el.dispatchEvent(new Event('input', {{bubbles: true}})); }} \
else if (el.isContentEditable) {{ el.textContent = ''; }} return true; }})()",
            sel = js_string(selector)
        );
        if self.evaluate(&page, &focus_script).await? != Value::Bool(true) {
            return Err(OpenAccountsError::Browser(format!("Element not found: {selector}")));
        }
        page.session.call("Input.insertText", json!({ "text": text })).await?;
        let change_script = format!(
            "(() => {{ const el = document.querySelector({sel}); if (el) el.dispatchEvent(new Event('change', {{bubbles: true}})); return true; }})()",
            sel = js_string(selector)
        );
        self.evaluate(&page, &change_script).await?;
        if is_secret {
            info!("Typed secret value into {}", selector);
        } else {
            info!("Typed {} characters into {}", text.chars().count(), selector);
        }
        Ok(())
    }

    async fn get_page_state(&self) -> Result<PageState> {
        let page = self.page().await?;
        let script = PAGE_STATE_SCRIPT.replace("__LIMIT__", &PAGE_TEXT_LIMIT.to_string());
        let val = self.evaluate(&page, &script).await?;
        Ok(page_state_from_value(&val))
    }

    async fn take_screenshot(&self) -> Result<Vec<u8>> {
        let page = self.page().await?;
        let res = page
            .session
            .call("Page.captureScreenshot", json!({ "format": "png" }))
            .await?;
        let data = res
            .get("data")
            .and_then(Value::as_str)
            .ok_or_else(|| OpenAccountsError::Browser("Screenshot response missing data".into()))?;
        STANDARD
            .decode(data)
            .map_err(|e| OpenAccountsError::Browser(format!("Screenshot decode failed: {e}")))
    }

    async fn evaluate_script(&self, script: &str) -> Result<Value> {
        self.ensure_active()?;
        let page = self.page().await?;
        self.evaluate(&page, script).await
    }

    async fn pause(&self) -> Result<()> {
        self.set_paused(true);
        info!("Browser automation paused for human takeover");
        Ok(())
    }

    async fn resume(&self) -> Result<()> {
        self.set_paused(false);
        info!("Browser automation resumed");
        Ok(())
    }

    async fn close(&self) -> Result<()> {
        let handle = self.page.lock().await.take();
        if let Some(page) = handle {
            page.session.close().await;
            if let Err(e) = close_tab(&self.http, &self.http_base, &page.target_id).await {
                warn!("{}", e);
            }
            info!("CDP session for target {} closed", page.target_id);
        }
        Ok(())
    }
}
