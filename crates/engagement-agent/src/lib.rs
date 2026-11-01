use chrono::{DateTime, Duration as ChronoDuration, Utc};
use openaccounts_core::{AccountId, OpenAccountsError, Result};
use openaccounts_lm_gateway::{LMGateway, DEFAULT_PROFILE_ID};
use openaccounts_platform_connectors::{
    normalize_platform, ConnectorRegistry, InteractionKind, PlatformConnector, RemoteActionReceipt, RemoteInteraction,
};
use openaccounts_policy_engine::PolicyEngine;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::Arc,
    time::Duration,
};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InteractionType {
    Comment,
    Mention,
    DirectMessage,
    Reply,
    PostLike,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ClassificationCategory {
    GeneralInquiry,
    SalesLead,
    CustomerSupport,
    Compliment,
    Complaint,
    Spam,
    SecurityOrLegalEscalation,
}

impl ClassificationCategory {
    pub fn requires_human(&self) -> bool {
        matches!(
            self,
            ClassificationCategory::Complaint
                | ClassificationCategory::CustomerSupport
                | ClassificationCategory::SecurityOrLegalEscalation
        )
    }

    pub fn is_auto_reply_candidate(&self) -> bool {
        matches!(
            self,
            ClassificationCategory::GeneralInquiry
                | ClassificationCategory::SalesLead
                | ClassificationCategory::Compliment
        )
    }
}

pub const STATUS_NEW: &str = "New";
pub const STATUS_PENDING_APPROVAL: &str = "PendingApproval";
pub const STATUS_SENDING: &str = "Sending";
pub const STATUS_REPLIED: &str = "Replied";
pub const STATUS_ARCHIVED: &str = "Archived";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedInboxMessage {
    pub message_id: String,
    pub account_id: AccountId,
    pub platform: String,
    pub author_handle: String,
    pub interaction_type: InteractionType,
    pub content_text: String,
    pub category: ClassificationCategory,
    pub sentiment_score: f32,
    pub drafted_reply: Option<String>,
    pub is_auto_reply_eligible: bool,
    pub requires_human_approval: bool,
    pub status: String,
    pub received_at: DateTime<Utc>,
    #[serde(default)]
    pub remote_id: Option<String>,
    #[serde(default)]
    pub permalink: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum EngagementActionType {
    Like,
    Follow,
    Subscribe,
    Reshare,
    Comment,
    Reply,
}

impl From<InteractionKind> for InteractionType {
    fn from(kind: InteractionKind) -> Self {
        match kind {
            InteractionKind::Comment => InteractionType::Comment,
            InteractionKind::Mention => InteractionType::Mention,
            InteractionKind::DirectMessage => InteractionType::DirectMessage,
            InteractionKind::Reply => InteractionType::Reply,
        }
    }
}

impl EngagementActionType {
    fn policy_action(&self) -> &'static str {
        match self {
            EngagementActionType::Like => "LikePost",
            EngagementActionType::Follow => "FollowUser",
            EngagementActionType::Subscribe => "SubscribeChannel",
            EngagementActionType::Reshare => "ReshareContent",
            EngagementActionType::Comment => "PostComment",
            EngagementActionType::Reply => "DispatchReply",
        }
    }

    fn is_idempotent(&self) -> bool {
        matches!(
            self,
            EngagementActionType::Like
                | EngagementActionType::Follow
                | EngagementActionType::Subscribe
                | EngagementActionType::Reshare
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngagementActionResult {
    pub action_id: String,
    pub account_id: AccountId,
    pub platform: String,
    pub action_type: EngagementActionType,
    pub target_id: String,
    pub success: bool,
    pub error_message: Option<String>,
    pub executed_at: DateTime<Utc>,
    #[serde(default)]
    pub remote_id: Option<String>,
    #[serde(default)]
    pub permalink: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EngagementCaps {
    pub daily: HashMap<EngagementActionType, u32>,
}

impl Default for EngagementCaps {
    fn default() -> Self {
        let daily = HashMap::from([
            (EngagementActionType::Like, 12),
            (EngagementActionType::Follow, 3),
            (EngagementActionType::Subscribe, 1),
            (EngagementActionType::Reshare, 5),
            (EngagementActionType::Comment, 2),
            (EngagementActionType::Reply, 15),
        ]);
        Self { daily }
    }
}

impl EngagementCaps {
    pub fn from_profile_metadata(metadata: &serde_json::Value) -> Self {
        let mut caps = Self::default();
        let Some(source) = metadata.get("daily_caps") else {
            return caps;
        };
        let keys: [(EngagementActionType, &[&str]); 6] = [
            (EngagementActionType::Like, &["likes", "like"]),
            (EngagementActionType::Follow, &["follows", "follow"]),
            (EngagementActionType::Subscribe, &["subs", "subscribes", "subscriptions"]),
            (EngagementActionType::Reshare, &["reshares", "reposts", "shares"]),
            (EngagementActionType::Comment, &["comments", "comment"]),
            (EngagementActionType::Reply, &["replies", "comment_replies", "reply"]),
        ];
        for (action, names) in keys {
            if let Some(v) = names.iter().find_map(|n| source.get(*n).and_then(|v| v.as_u64())) {
                caps.daily.insert(action, v.min(u32::MAX as u64) as u32);
            }
        }
        caps
    }

    pub fn limit(&self, action: EngagementActionType) -> u32 {
        self.daily.get(&action).copied().unwrap_or(0)
    }
}

#[derive(Debug, Clone)]
pub struct EngagementConfig {
    pub lm_profile: String,
    pub lm_timeout: Duration,
    pub use_lm: bool,
    pub history_limit: usize,
}

impl Default for EngagementConfig {
    fn default() -> Self {
        Self {
            lm_profile: DEFAULT_PROFILE_ID.to_string(),
            lm_timeout: Duration::from_secs(20),
            use_lm: true,
            history_limit: 10_000,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Triage {
    pub category: ClassificationCategory,
    pub sentiment: f32,
}

const SECURITY_TERMS: &[&str] = &[
    "lawyer", "legal", "lawsuit", "attorney", "subpoena", "gdpr", "dmca", "copyright", "hacked",
    "breach", "password", "vulnerability", "phishing", "police", "court", "contract", "sue", "trademark",
];
const SPAM_TERMS: &[&str] = &[
    "crypto giveaway", "free followers", "click the link", "promo code", "check my profile", "earn $",
    "whatsapp me", "telegram me", "bit.ly", "dm me to earn", "investment opportunity", "100% guaranteed",
];
const COMPLAINT_TERMS: &[&str] = &[
    "terrible", "worst", "refund", "scam", "angry", "disappointed", "awful", "hate", "useless",
    "doesn't work", "does not work", "not working", "ripoff", "rip-off", "unacceptable",
];
const SUPPORT_TERMS: &[&str] = &[
    "help", "issue", "problem", "error", "bug", "can't", "cannot", "unable", "how do i", "stuck",
    "crash", "crashes", "broken", "login", "fix",
];
const SALES_TERMS: &[&str] = &[
    "price", "pricing", "buy", "purchase", "license", "quote", "enterprise", "demo", "subscription",
    "plan", "plans", "cost", "partner", "partnership", "collab", "collaboration", "sponsor", "sponsorship",
    "merch", "order",
];
const COMPLIMENT_TERMS: &[&str] = &[
    "love", "loved", "amazing", "awesome", "great", "thanks", "thank", "fantastic", "incredible",
    "beautiful", "best", "brilliant", "helpful", "🔥", "❤", "❤️", "🙌",
];
const NEGATIVE_TERMS: &[&str] = &[
    "terrible", "worst", "scam", "angry", "disappointed", "awful", "hate", "useless", "broken", "bad",
    "annoying", "ripoff", "unacceptable", "slow",
];

fn tokenize(lower: &str) -> HashSet<String> {
    lower
        .split(|c: char| !(c.is_alphanumeric() || c == '\''))
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect()
}

fn count_terms(lower: &str, tokens: &HashSet<String>, terms: &[&str]) -> usize {
    terms
        .iter()
        .filter(|term| {
            let is_simple_word = term.chars().all(|c| c.is_alphanumeric() || c == '\'');
            if is_simple_word {
                tokens.contains(**term)
            } else {
                lower.contains(**term)
            }
        })
        .count()
}

pub fn classify_heuristic(text: &str) -> Triage {
    let lower = text.to_lowercase();
    let tokens = tokenize(&lower);
    let hits = |terms: &[&str]| count_terms(&lower, &tokens, terms);

    let link_count = lower.matches("http://").count() + lower.matches("https://").count();
    let positive = hits(COMPLIMENT_TERMS) as f32;
    let negative = hits(NEGATIVE_TERMS) as f32;
    let sentiment = if positive + negative == 0.0 {
        0.0
    } else {
        ((positive - negative) / (positive + negative)).clamp(-1.0, 1.0)
    };

    let category = if hits(SECURITY_TERMS) > 0 {
        ClassificationCategory::SecurityOrLegalEscalation
    } else if hits(SPAM_TERMS) > 0 || link_count >= 2 {
        ClassificationCategory::Spam
    } else if hits(COMPLAINT_TERMS) > 0 {
        ClassificationCategory::Complaint
    } else if hits(SUPPORT_TERMS) > 0 {
        ClassificationCategory::CustomerSupport
    } else if hits(SALES_TERMS) > 0 {
        ClassificationCategory::SalesLead
    } else if positive > 0.0 && negative == 0.0 {
        ClassificationCategory::Compliment
    } else {
        ClassificationCategory::GeneralInquiry
    };

    Triage { category, sentiment }
}

pub fn platform_char_limit(platform: &str) -> usize {
    match platform.to_lowercase().as_str() {
        "x" | "twitter" => 280,
        "bluesky" => 300,
        "tiktok" => 150,
        "threads" => 500,
        "instagram" => 2200,
        "youtube" | "reddit" => 10_000,
        _ => 2000,
    }
}

pub fn sanitize_reply(text: &str, platform: &str) -> String {
    let trimmed = text
        .trim()
        .trim_matches(|c| c == '"' || c == '“' || c == '”')
        .trim();
    let collapsed = trimmed
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n");
    let limit = platform_char_limit(platform);
    if collapsed.chars().count() <= limit {
        return collapsed;
    }
    let mut cut: String = collapsed.chars().take(limit.saturating_sub(1)).collect();
    if let Some(idx) = cut.rfind(char::is_whitespace) {
        if idx > limit / 2 {
            cut.truncate(idx);
        }
    }
    format!("{}…", cut.trim_end())
}

pub fn template_reply(category: ClassificationCategory, author: &str) -> Option<String> {
    let who = if author.trim().is_empty() { "there" } else { author.trim() };
    let text = match category {
        ClassificationCategory::GeneralInquiry => {
            format!("Thanks for reaching out, {who}! Great question, we'll follow up with the details shortly.")
        }
        ClassificationCategory::SalesLead => {
            format!("Thanks {who}! We'd love to help. Send us a DM and our team will share everything you need.")
        }
        ClassificationCategory::CustomerSupport => {
            format!("Sorry for the trouble, {who}. Please DM us the details and our support team will sort it out.")
        }
        ClassificationCategory::Compliment => {
            format!("Thank you so much, {who}! That means a lot to the whole team.")
        }
        ClassificationCategory::Complaint => {
            format!("We're sorry to hear this, {who}. We take it seriously, please DM us so we can make it right.")
        }
        ClassificationCategory::Spam | ClassificationCategory::SecurityOrLegalEscalation => return None,
    };
    Some(text)
}

#[derive(Deserialize)]
struct LmTriage {
    category: ClassificationCategory,
    #[serde(default)]
    sentiment: f32,
}

fn parse_lm_triage(raw: &str) -> Option<Triage> {
    let start = raw.find('{')?;
    let end = raw.rfind('}')?;
    if end <= start {
        return None;
    }
    let parsed: LmTriage = serde_json::from_str(&raw[start..=end]).ok()?;
    let sentiment = if parsed.sentiment.is_finite() { parsed.sentiment.clamp(-1.0, 1.0) } else { 0.0 };
    Some(Triage { category: parsed.category, sentiment })
}

const TRIAGE_SYSTEM_PROMPT: &str = "You triage social media messages sent to a brand account. \
Respond with JSON only, no prose: {\"category\": \"<one of general_inquiry, sales_lead, customer_support, \
compliment, complaint, spam, security_or_legal_escalation>\", \"sentiment\": <number from -1 to 1>}.";

struct Incoming {
    account_id: AccountId,
    platform: String,
    author_handle: String,
    interaction_type: InteractionType,
    content_text: String,
    received_at: DateTime<Utc>,
    remote_id: Option<String>,
    permalink: Option<String>,
}

#[derive(Default)]
struct ActionLedger {
    history: VecDeque<EngagementActionResult>,
    in_flight: Vec<(AccountId, String, EngagementActionType, String)>,
}

enum ConnectorCall {
    Like(String),
    Follow(String),
    Subscribe(String),
    Reshare(String, Option<String>),
    Comment(String, String),
    Reply(String, String),
}

impl ConnectorCall {
    async fn run(&self, connector: &dyn PlatformConnector) -> Result<RemoteActionReceipt> {
        match self {
            ConnectorCall::Like(t) => connector.like(t).await,
            ConnectorCall::Follow(t) => connector.follow(t).await,
            ConnectorCall::Subscribe(t) => connector.subscribe(t).await,
            ConnectorCall::Reshare(t, q) => connector.reshare(t, q.as_deref()).await,
            ConnectorCall::Comment(t, text) => connector.comment(t, text).await,
            ConnectorCall::Reply(t, text) => connector.reply(t, text).await,
        }
    }
}

pub struct EngagementAgent {
    policy_engine: Arc<PolicyEngine>,
    lm_gateway: Arc<LMGateway>,
    connectors: Arc<ConnectorRegistry>,
    config: EngagementConfig,
    inbox: Arc<RwLock<HashMap<String, UnifiedInboxMessage>>>,
    ledger: Arc<RwLock<ActionLedger>>,
    account_caps: Arc<RwLock<HashMap<AccountId, EngagementCaps>>>,
    default_caps: EngagementCaps,
}

impl EngagementAgent {
    pub fn new(policy_engine: Arc<PolicyEngine>, lm_gateway: Arc<LMGateway>) -> Self {
        Self::with_config(policy_engine, lm_gateway, EngagementConfig::default())
    }

    pub fn with_config(policy_engine: Arc<PolicyEngine>, lm_gateway: Arc<LMGateway>, config: EngagementConfig) -> Self {
        Self::with_connectors(policy_engine, lm_gateway, config, Arc::new(ConnectorRegistry::from_env()))
    }

    pub fn with_connectors(
        policy_engine: Arc<PolicyEngine>,
        lm_gateway: Arc<LMGateway>,
        config: EngagementConfig,
        connectors: Arc<ConnectorRegistry>,
    ) -> Self {
        Self {
            policy_engine,
            lm_gateway,
            connectors,
            config,
            inbox: Arc::new(RwLock::new(HashMap::new())),
            ledger: Arc::new(RwLock::new(ActionLedger::default())),
            account_caps: Arc::new(RwLock::new(HashMap::new())),
            default_caps: EngagementCaps::default(),
        }
    }

    pub fn config(&self) -> &EngagementConfig {
        &self.config
    }

    pub fn connectors(&self) -> Arc<ConnectorRegistry> {
        self.connectors.clone()
    }

    pub async fn set_account_caps(&self, account_id: AccountId, caps: EngagementCaps) {
        self.account_caps.write().await.insert(account_id, caps);
    }

    pub async fn caps_for(&self, account_id: &AccountId) -> EngagementCaps {
        self.account_caps
            .read()
            .await
            .get(account_id)
            .cloned()
            .unwrap_or_else(|| self.default_caps.clone())
    }

    pub async fn remaining_quota(&self, account_id: &AccountId, action: EngagementActionType) -> u32 {
        let cap = self.caps_for(account_id).await.limit(action);
        let window_start = Utc::now() - ChronoDuration::hours(24);
        let used = self
            .ledger
            .read()
            .await
            .history
            .iter()
            .filter(|r| r.success && &r.account_id == account_id && r.action_type == action && r.executed_at > window_start)
            .count() as u32;
        cap.saturating_sub(used)
    }

    pub async fn list_inbox(&self) -> Vec<UnifiedInboxMessage> {
        let mut messages: Vec<UnifiedInboxMessage> = self.inbox.read().await.values().cloned().collect();
        messages.sort_by(|a, b| b.received_at.cmp(&a.received_at).then_with(|| a.message_id.cmp(&b.message_id)));
        messages
    }

    pub async fn pending_inbox_count(&self) -> usize {
        self.inbox
            .read()
            .await
            .values()
            .filter(|m| m.status == STATUS_NEW || m.status == STATUS_PENDING_APPROVAL)
            .count()
    }

    pub async fn get_message(&self, message_id: &str) -> Option<UnifiedInboxMessage> {
        self.inbox.read().await.get(message_id).cloned()
    }

    pub async fn triage(&self, text: &str) -> Triage {
        let heuristic = classify_heuristic(text);
        if matches!(
            heuristic.category,
            ClassificationCategory::SecurityOrLegalEscalation | ClassificationCategory::Spam
        ) || !self.config.use_lm
        {
            return heuristic;
        }

        let user = format!("Message:\n{}", text.chars().take(2000).collect::<String>());
        match self
            .lm_gateway
            .quick_complete(&self.config.lm_profile, TRIAGE_SYSTEM_PROMPT, &user, 120, self.config.lm_timeout)
            .await
        {
            Ok(raw) => match parse_lm_triage(&raw) {
                Some(lm) => lm,
                None => {
                    debug!("LM triage returned unparseable output; using heuristic");
                    heuristic
                }
            },
            Err(e) => {
                debug!("LM triage unavailable ({}); using heuristic", e);
                heuristic
            }
        }
    }

    async fn compose_reply(&self, msg: &UnifiedInboxMessage) -> Option<String> {
        let fallback = template_reply(msg.category, &msg.author_handle);
        fallback.as_ref()?;
        if !self.config.use_lm {
            return fallback;
        }

        let limit = platform_char_limit(&msg.platform);
        let system = format!(
            "You are the community manager for a brand on {platform}. Write one reply to the message below. \
Keep it under {limit} characters, warm, specific and human. Plain text only, no hashtags, no links. \
Never promise refunds, pricing, legal outcomes or timelines, and never ask for passwords or codes. \
Output only the reply text.",
            platform = msg.platform,
        );
        let user = format!(
            "Category: {:?}\nAuthor: {}\nMessage: {}",
            msg.category,
            msg.author_handle,
            msg.content_text.chars().take(2000).collect::<String>()
        );
        match self
            .lm_gateway
            .quick_complete(&self.config.lm_profile, &system, &user, 300, self.config.lm_timeout)
            .await
        {
            Ok(text) if !text.trim().is_empty() => Some(sanitize_reply(&text, &msg.platform)),
            Ok(_) => fallback,
            Err(e) => {
                debug!("LM reply drafting unavailable ({}); using template", e);
                fallback
            }
        }
    }

    async fn build_message(&self, incoming: Incoming) -> UnifiedInboxMessage {
        let Incoming {
            account_id,
            platform,
            author_handle,
            interaction_type,
            content_text,
            received_at,
            remote_id,
            permalink,
        } = incoming;
        let triage = self.triage(&content_text).await;
        let requires_human_approval = triage.category.requires_human() || triage.sentiment < -0.3;
        let is_spam = triage.category == ClassificationCategory::Spam;

        let mut msg = UnifiedInboxMessage {
            message_id: format!("inb-{}", Uuid::new_v4().simple()),
            account_id,
            platform: normalize_platform(&platform),
            author_handle,
            interaction_type,
            content_text,
            category: triage.category,
            sentiment_score: triage.sentiment,
            drafted_reply: None,
            is_auto_reply_eligible: triage.category.is_auto_reply_candidate() && !requires_human_approval,
            requires_human_approval,
            status: if is_spam {
                STATUS_ARCHIVED.into()
            } else if requires_human_approval {
                STATUS_PENDING_APPROVAL.into()
            } else {
                STATUS_NEW.into()
            },
            received_at,
            remote_id,
            permalink,
        };
        if !is_spam {
            msg.drafted_reply = self.compose_reply(&msg).await;
        }
        msg
    }

    pub async fn ingest_message(
        &self,
        account_id: AccountId,
        platform: &str,
        author_handle: &str,
        interaction_type: InteractionType,
        content_text: &str,
    ) -> Result<UnifiedInboxMessage> {
        if content_text.trim().is_empty() {
            return Err(OpenAccountsError::Automation("Cannot ingest an empty message".into()));
        }
        let msg = self
            .build_message(Incoming {
                account_id,
                platform: platform.to_string(),
                author_handle: author_handle.to_string(),
                interaction_type,
                content_text: content_text.to_string(),
                received_at: Utc::now(),
                remote_id: None,
                permalink: None,
            })
            .await;
        info!(
            "Inbox: {:?} from {} on {} triaged as {:?}",
            msg.interaction_type, msg.author_handle, msg.platform, msg.category
        );
        self.inbox.write().await.insert(msg.message_id.clone(), msg.clone());
        Ok(msg)
    }

    pub async fn ingest_remote_interaction(
        &self,
        account_id: AccountId,
        platform: &str,
        interaction: RemoteInteraction,
    ) -> Result<Option<UnifiedInboxMessage>> {
        let platform = normalize_platform(platform);
        if interaction.text.trim().is_empty() || self.has_remote(&platform, &interaction.remote_id).await {
            return Ok(None);
        }
        let msg = self
            .build_message(Incoming {
                account_id,
                platform: platform.clone(),
                author_handle: interaction.author_handle.clone(),
                interaction_type: interaction.kind.into(),
                content_text: interaction.text.clone(),
                received_at: interaction.created_at,
                remote_id: Some(interaction.remote_id.clone()),
                permalink: interaction.permalink.clone(),
            })
            .await;
        let mut inbox = self.inbox.write().await;
        let duplicate = inbox
            .values()
            .any(|m| m.platform == platform && m.remote_id.as_deref() == Some(interaction.remote_id.as_str()));
        if duplicate {
            return Ok(None);
        }
        inbox.insert(msg.message_id.clone(), msg.clone());
        Ok(Some(msg))
    }

    async fn has_remote(&self, platform: &str, remote_id: &str) -> bool {
        self.inbox
            .read()
            .await
            .values()
            .any(|m| m.platform == platform && m.remote_id.as_deref() == Some(remote_id))
    }

    pub async fn sync_inbox(&self, account_id: AccountId, platform: &str) -> Result<Vec<UnifiedInboxMessage>> {
        let platform = normalize_platform(platform);
        let connector = self.connectors.connector_for(&platform).await?;
        let since = self
            .inbox
            .read()
            .await
            .values()
            .filter(|m| m.platform == platform && m.account_id == account_id && m.remote_id.is_some())
            .map(|m| m.received_at)
            .max();
        let interactions = connector.fetch_inbox(since, 50).await?;

        let mut added = Vec::new();
        for interaction in interactions {
            if let Some(msg) = self
                .ingest_remote_interaction(account_id.clone(), &platform, interaction)
                .await?
            {
                added.push(msg);
            }
        }
        info!("Synced {} new {} interactions into the inbox", added.len(), platform);
        Ok(added)
    }

    pub async fn redraft_reply(&self, message_id: &str) -> Result<Option<String>> {
        let msg = self
            .get_message(message_id)
            .await
            .ok_or_else(|| OpenAccountsError::Internal("Message not found".into()))?;
        let draft = self.compose_reply(&msg).await;
        if let Some(m) = self.inbox.write().await.get_mut(message_id) {
            m.drafted_reply = draft.clone();
        }
        Ok(draft)
    }

    pub async fn archive_message(&self, message_id: &str) -> Result<()> {
        let mut inbox = self.inbox.write().await;
        let msg = inbox
            .get_mut(message_id)
            .ok_or_else(|| OpenAccountsError::Internal("Message not found".into()))?;
        msg.status = STATUS_ARCHIVED.into();
        Ok(())
    }

    pub async fn approve_and_send_reply(&self, message_id: &str, final_reply: &str) -> Result<()> {
        if final_reply.trim().is_empty() {
            return Err(OpenAccountsError::Automation("Reply text cannot be empty".into()));
        }

        let (account_id, platform, remote_id, previous_status) = {
            let mut inbox = self.inbox.write().await;
            let msg = inbox
                .get_mut(message_id)
                .ok_or_else(|| OpenAccountsError::Internal("Message not found".into()))?;
            if msg.status == STATUS_REPLIED || msg.status == STATUS_SENDING {
                return Err(OpenAccountsError::Automation(format!(
                    "Message {message_id} has already been answered"
                )));
            }
            let remote_id = msg.remote_id.clone().ok_or_else(|| {
                OpenAccountsError::Automation(format!(
                    "Message {message_id} was not synced from {} and has no remote thread to reply to",
                    msg.platform
                ))
            })?;
            let previous = std::mem::replace(&mut msg.status, STATUS_SENDING.into());
            (msg.account_id.clone(), msg.platform.clone(), remote_id, previous)
        };

        let reply = sanitize_reply(final_reply, &platform);
        let dispatched = self.dispatch_reply(account_id, &platform, &remote_id, &reply).await;

        let mut inbox = self.inbox.write().await;
        let msg = inbox
            .get_mut(message_id)
            .ok_or_else(|| OpenAccountsError::Internal("Message not found".into()))?;
        match dispatched {
            Ok(_) => {
                msg.drafted_reply = Some(reply);
                msg.status = STATUS_REPLIED.into();
                info!("Approved and dispatched reply to {} on {}", msg.author_handle, msg.platform);
                Ok(())
            }
            Err(e) => {
                msg.status = previous_status;
                Err(e)
            }
        }
    }

    pub async fn auto_reply_pending(&self) -> Vec<Result<String>> {
        let candidates: Vec<(String, String)> = self
            .inbox
            .read()
            .await
            .values()
            .filter(|m| m.status == STATUS_NEW && m.is_auto_reply_eligible && !m.requires_human_approval)
            .filter(|m| m.remote_id.is_some())
            .filter_map(|m| m.drafted_reply.clone().map(|r| (m.message_id.clone(), r)))
            .collect();

        let mut outcomes = Vec::with_capacity(candidates.len());
        for (id, reply) in candidates {
            outcomes.push(self.approve_and_send_reply(&id, &reply).await.map(|_| id));
        }
        outcomes
    }

    async fn perform(
        &self,
        account_id: AccountId,
        platform: &str,
        action_type: EngagementActionType,
        target_id: String,
        call: ConnectorCall,
    ) -> Result<EngagementActionResult> {
        let platform = normalize_platform(platform);
        self.validate_policy(&platform, action_type.policy_action()).await?;
        let connector = self.connectors.connector_for(&platform).await?;
        let cap = self.caps_for(&account_id).await.limit(action_type);
        let key = (account_id.clone(), platform.clone(), action_type, target_id.clone());

        {
            let now = Utc::now();
            let window_start = now - ChronoDuration::hours(24);
            let mut ledger = self.ledger.write().await;

            let previous = ledger.history.iter().rev().find(|r| {
                r.success
                    && r.account_id == account_id
                    && r.platform == platform
                    && r.action_type == action_type
                    && r.target_id == target_id
            });
            if let Some(prev) = previous {
                if action_type.is_idempotent() {
                    debug!("Skipping repeated {:?} on {} ({})", action_type, platform, prev.target_id);
                    return Ok(prev.clone());
                }
                return Err(OpenAccountsError::Automation(format!(
                    "Duplicate {:?} on {} suppressed to avoid spam",
                    action_type, platform
                )));
            }
            if ledger.in_flight.contains(&key) {
                return Err(OpenAccountsError::Automation(format!(
                    "An identical {:?} on {} is already in progress",
                    action_type, platform
                )));
            }

            let used_times: Vec<DateTime<Utc>> = ledger
                .history
                .iter()
                .filter(|r| {
                    r.success && r.account_id == account_id && r.action_type == action_type && r.executed_at > window_start
                })
                .map(|r| r.executed_at)
                .collect();
            let in_flight = ledger
                .in_flight
                .iter()
                .filter(|(a, _, t, _)| *a == account_id && *t == action_type)
                .count();
            if (used_times.len() + in_flight) as u32 >= cap {
                let retry_after = used_times
                    .iter()
                    .min()
                    .map(|oldest| (*oldest + ChronoDuration::hours(24) - now).num_seconds().max(1) as u64)
                    .unwrap_or(86_400);
                warn!(
                    "Daily {:?} cap ({}) reached for account {} on {}",
                    action_type, cap, account_id, platform
                );
                return Err(OpenAccountsError::RateLimitExceeded(account_id.to_string(), retry_after));
            }
            ledger.in_flight.push(key.clone());
        }

        let outcome = call.run(connector.as_ref()).await;

        let mut ledger = self.ledger.write().await;
        ledger.in_flight.retain(|k| k != &key);
        let mut result = EngagementActionResult {
            action_id: Uuid::new_v4().to_string(),
            account_id,
            platform: platform.clone(),
            action_type,
            target_id,
            success: outcome.is_ok(),
            error_message: None,
            executed_at: Utc::now(),
            remote_id: None,
            permalink: None,
        };
        let returned = match outcome {
            Ok(receipt) => {
                result.remote_id = Some(receipt.remote_id);
                result.permalink = receipt.permalink;
                info!("Engagement {:?} on {} -> {}", action_type, platform, result.target_id);
                Ok(result.clone())
            }
            Err(e) => {
                warn!("Engagement {:?} on {} failed: {}", action_type, platform, e);
                result.error_message = Some(e.to_string());
                Err(e)
            }
        };
        ledger.history.push_back(result);
        while ledger.history.len() > self.config.history_limit.max(1) {
            ledger.history.pop_front();
        }
        returned
    }

    pub async fn like_post(&self, account_id: AccountId, platform: &str, post_id: &str) -> Result<EngagementActionResult> {
        self.perform(
            account_id,
            platform,
            EngagementActionType::Like,
            post_id.to_string(),
            ConnectorCall::Like(post_id.to_string()),
        )
        .await
    }

    pub async fn follow_user(
        &self,
        account_id: AccountId,
        platform: &str,
        target_handle: &str,
    ) -> Result<EngagementActionResult> {
        self.perform(
            account_id,
            platform,
            EngagementActionType::Follow,
            target_handle.to_string(),
            ConnectorCall::Follow(target_handle.to_string()),
        )
        .await
    }

    pub async fn subscribe_channel(
        &self,
        account_id: AccountId,
        platform: &str,
        channel_id: &str,
    ) -> Result<EngagementActionResult> {
        self.perform(
            account_id,
            platform,
            EngagementActionType::Subscribe,
            channel_id.to_string(),
            ConnectorCall::Subscribe(channel_id.to_string()),
        )
        .await
    }

    pub async fn reshare_content(
        &self,
        account_id: AccountId,
        platform: &str,
        post_id: &str,
        quote_text: Option<&str>,
    ) -> Result<EngagementActionResult> {
        let quote = quote_text
            .map(|q| sanitize_reply(q, platform))
            .filter(|q| !q.is_empty());
        let target = match &quote {
            Some(q) => format!("{}:{}", post_id, q),
            None => post_id.to_string(),
        };
        self.perform(
            account_id,
            platform,
            EngagementActionType::Reshare,
            target,
            ConnectorCall::Reshare(post_id.to_string(), quote),
        )
        .await
    }

    pub async fn post_comment(
        &self,
        account_id: AccountId,
        platform: &str,
        post_id: &str,
        comment_text: &str,
    ) -> Result<EngagementActionResult> {
        let text = sanitize_reply(comment_text, platform);
        if text.is_empty() {
            return Err(OpenAccountsError::Automation("Comment text cannot be empty".into()));
        }
        self.perform(
            account_id,
            platform,
            EngagementActionType::Comment,
            format!("{}:{}", post_id, text),
            ConnectorCall::Comment(post_id.to_string(), text),
        )
        .await
    }

    pub async fn dispatch_reply(
        &self,
        account_id: AccountId,
        platform: &str,
        interaction_id: &str,
        reply_text: &str,
    ) -> Result<EngagementActionResult> {
        let text = sanitize_reply(reply_text, platform);
        if text.is_empty() {
            return Err(OpenAccountsError::Automation("Reply text cannot be empty".into()));
        }
        self.perform(
            account_id,
            platform,
            EngagementActionType::Reply,
            format!("{}:{}", interaction_id, text),
            ConnectorCall::Reply(interaction_id.to_string(), text),
        )
        .await
    }

    pub async fn get_engagement_history(&self) -> Vec<EngagementActionResult> {
        self.ledger.read().await.history.iter().cloned().collect()
    }

    async fn validate_policy(&self, platform: &str, action: &str) -> Result<()> {
        let check = self
            .policy_engine
            .check_automation_compliance(platform, action, true, false)
            .await?;
        if !check.is_allowed {
            return Err(OpenAccountsError::PermissionDenied(format!(
                "Policy violation: Action {} on platform {} not permitted: {}",
                action, platform, check.reason
            )));
        }
        Ok(())
    }
}
