use async_trait::async_trait;
use chrono::{DateTime, Utc};
use openaccounts_core::{AccountId, OpenAccountsError, Result};
use openaccounts_engagement_agent::{
    classify_heuristic, sanitize_reply, ClassificationCategory, EngagementActionType, EngagementAgent,
    EngagementCaps, EngagementConfig, InteractionType, STATUS_PENDING_APPROVAL, STATUS_REPLIED,
};
use openaccounts_lm_gateway::LMGateway;
use openaccounts_platform_connectors::{
    ConnectorRegistry, InteractionKind, PlatformAuthInfo, PlatformConnector, PublishRequest, RemoteActionReceipt,
    RemoteInteraction, RemoteMetrics, StaticTokenSource,
};
use openaccounts_platform_intelligence::PlatformIntelligence;
use openaccounts_policy_engine::PolicyEngine;
use std::sync::{Arc, Mutex};

struct RecordingConnector {
    platform: String,
    calls: Mutex<Vec<String>>,
    inbox: Vec<RemoteInteraction>,
    fail_ops: Vec<&'static str>,
}

impl RecordingConnector {
    fn new(platform: &str) -> Arc<Self> {
        Arc::new(Self { platform: platform.into(), calls: Mutex::new(vec![]), inbox: vec![], fail_ops: vec![] })
    }

    fn with_inbox(platform: &str, inbox: Vec<RemoteInteraction>) -> Arc<Self> {
        Arc::new(Self { platform: platform.into(), calls: Mutex::new(vec![]), inbox, fail_ops: vec![] })
    }

    fn failing(platform: &str, ops: Vec<&'static str>) -> Arc<Self> {
        Arc::new(Self { platform: platform.into(), calls: Mutex::new(vec![]), inbox: vec![], fail_ops: ops })
    }

    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }

    fn record(&self, op: &'static str, detail: String) -> Result<RemoteActionReceipt> {
        self.calls.lock().unwrap().push(format!("{op}:{detail}"));
        if self.fail_ops.contains(&op) {
            return Err(OpenAccountsError::Automation(format!("{op} rejected by remote")));
        }
        let n = self.calls.lock().unwrap().len();
        Ok(RemoteActionReceipt { remote_id: format!("remote-{n}"), permalink: Some(format!("https://example/{n}")) })
    }
}

#[async_trait]
impl PlatformConnector for RecordingConnector {
    fn platform(&self) -> &str {
        &self.platform
    }
    async fn verify_auth(&self) -> Result<PlatformAuthInfo> {
        Ok(PlatformAuthInfo {
            platform: self.platform.clone(),
            account_handle: Some("@me".into()),
            account_remote_id: Some("1".into()),
            scopes: vec![],
            is_authenticated: true,
            error: None,
        })
    }
    async fn like(&self, target: &str) -> Result<RemoteActionReceipt> {
        self.record("like", target.into())
    }
    async fn follow(&self, target: &str) -> Result<RemoteActionReceipt> {
        self.record("follow", target.into())
    }
    async fn subscribe(&self, target: &str) -> Result<RemoteActionReceipt> {
        self.record("subscribe", target.into())
    }
    async fn reshare(&self, target: &str, quote: Option<&str>) -> Result<RemoteActionReceipt> {
        self.record("reshare", format!("{target}|{}", quote.unwrap_or_default()))
    }
    async fn comment(&self, target: &str, text: &str) -> Result<RemoteActionReceipt> {
        self.record("comment", format!("{target}|{text}"))
    }
    async fn reply(&self, target: &str, text: &str) -> Result<RemoteActionReceipt> {
        self.record("reply", format!("{target}|{text}"))
    }
    async fn publish(&self, request: PublishRequest) -> Result<RemoteActionReceipt> {
        self.record("publish", request.caption)
    }
    async fn fetch_inbox(&self, since: Option<DateTime<Utc>>, _limit: u32) -> Result<Vec<RemoteInteraction>> {
        Ok(self
            .inbox
            .iter()
            .filter(|i| since.map(|s| i.created_at > s).unwrap_or(true))
            .cloned()
            .collect())
    }
    async fn fetch_metrics(&self) -> Result<RemoteMetrics> {
        Ok(RemoteMetrics::default())
    }
}

fn offline_config() -> EngagementConfig {
    EngagementConfig { use_lm: false, ..Default::default() }
}

fn agent_with(connectors: Vec<(&str, Arc<RecordingConnector>)>) -> EngagementAgent {
    let intel = Arc::new(PlatformIntelligence::new());
    let policy = Arc::new(PolicyEngine::new(intel));
    let lm = Arc::new(LMGateway::new());
    let mut registry = ConnectorRegistry::new(Arc::new(StaticTokenSource::new()));
    for (platform, c) in connectors {
        registry = registry.with_connector(platform, c);
    }
    EngagementAgent::with_connectors(policy, lm, offline_config(), Arc::new(registry))
}

fn interaction(id: &str, author: &str, text: &str, kind: InteractionKind) -> RemoteInteraction {
    RemoteInteraction {
        remote_id: id.into(),
        author_handle: author.into(),
        kind,
        text: text.into(),
        permalink: Some(format!("https://youtube.com/watch?v=v&lc={id}")),
        created_at: Utc::now(),
    }
}

#[tokio::test]
async fn engagement_actions_call_the_platform_and_keep_receipts() {
    let x = RecordingConnector::new("x");
    let yt = RecordingConnector::new("youtube");
    let reddit = RecordingConnector::new("reddit");
    let agent = agent_with(vec![("x", x.clone()), ("youtube", yt.clone()), ("reddit", reddit.clone())]);
    let account = AccountId::new();

    let like = agent.like_post(account.clone(), "x", "tweet-1").await.unwrap();
    assert_eq!(like.action_type, EngagementActionType::Like);
    assert!(like.remote_id.is_some());
    assert!(like.permalink.is_some());

    agent.follow_user(account.clone(), "twitter", "@builder").await.unwrap();
    agent.subscribe_channel(account.clone(), "youtube", "UC123").await.unwrap();
    agent.reshare_content(account.clone(), "x", "tweet-2", Some("Worth reading")).await.unwrap();
    agent.post_comment(account.clone(), "youtube", "vid-1", "Great breakdown!").await.unwrap();
    agent.dispatch_reply(account.clone(), "reddit", "t1_abc", "Thanks for the question").await.unwrap();

    assert_eq!(x.calls(), vec!["like:tweet-1", "follow:@builder", "reshare:tweet-2|Worth reading"]);
    assert_eq!(yt.calls(), vec!["subscribe:UC123", "comment:vid-1|Great breakdown!"]);
    assert_eq!(reddit.calls(), vec!["reply:t1_abc|Thanks for the question"]);
    assert_eq!(agent.get_engagement_history().await.len(), 6);
}

#[tokio::test]
async fn repeated_actions_are_deduplicated_before_reaching_the_platform() {
    let x = RecordingConnector::new("x");
    let agent = agent_with(vec![("x", x.clone())]);
    let account = AccountId::new();

    let first = agent.like_post(account.clone(), "x", "tweet-9").await.unwrap();
    let second = agent.like_post(account.clone(), "x", "tweet-9").await.unwrap();
    assert_eq!(first.action_id, second.action_id);

    agent.post_comment(account.clone(), "x", "tweet-9", "nice").await.unwrap();
    let dup = agent.post_comment(account.clone(), "x", "tweet-9", "nice").await;
    assert!(dup.unwrap_err().to_string().contains("Duplicate"));
    assert_eq!(x.calls().len(), 2);
}

#[tokio::test]
async fn daily_caps_block_further_actions() {
    let x = RecordingConnector::new("x");
    let agent = agent_with(vec![("x", x.clone())]);
    let account = AccountId::new();
    let mut caps = EngagementCaps::default();
    caps.daily.insert(EngagementActionType::Like, 1);
    agent.set_account_caps(account.clone(), caps).await;

    agent.like_post(account.clone(), "x", "a").await.unwrap();
    let err = agent.like_post(account.clone(), "x", "b").await.unwrap_err();
    assert!(matches!(err, OpenAccountsError::RateLimitExceeded(_, secs) if secs > 0));
    assert_eq!(agent.remaining_quota(&account, EngagementActionType::Like).await, 0);
    assert_eq!(x.calls().len(), 1);
}

#[tokio::test]
async fn platform_failures_are_errors_and_do_not_consume_quota() {
    let x = RecordingConnector::failing("x", vec!["like"]);
    let agent = agent_with(vec![("x", x.clone())]);
    let account = AccountId::new();
    let before = agent.remaining_quota(&account, EngagementActionType::Like).await;

    let err = agent.like_post(account.clone(), "x", "t").await.unwrap_err();
    assert!(err.to_string().contains("rejected by remote"));
    let history = agent.get_engagement_history().await;
    assert_eq!(history.len(), 1);
    assert!(!history[0].success);
    assert!(history[0].error_message.is_some());
    assert_eq!(agent.remaining_quota(&account, EngagementActionType::Like).await, before);
}

#[tokio::test]
async fn unconnected_platforms_fail_instead_of_pretending() {
    let agent = agent_with(vec![]);
    let err = agent.like_post(AccountId::new(), "youtube", "vid").await.unwrap_err();
    assert!(err.to_string().contains("youtube is not connected"), "{err}");
    assert!(agent.get_engagement_history().await.is_empty());
}

#[tokio::test]
async fn inbox_starts_empty_and_syncs_real_interactions_once() {
    let yt = RecordingConnector::with_inbox(
        "youtube",
        vec![
            interaction("c-legal", "@lawfirm", "Our lawyer will contact you about copyright", InteractionKind::Comment),
            interaction("c-love", "@fan", "Love this episode, amazing work!", InteractionKind::Comment),
        ],
    );
    let agent = agent_with(vec![("youtube", yt.clone())]);
    let account = AccountId::new();
    assert!(agent.list_inbox().await.is_empty());

    let added = agent.sync_inbox(account.clone(), "youtube").await.unwrap();
    assert_eq!(added.len(), 2);
    assert!(agent.sync_inbox(account.clone(), "youtube").await.unwrap().is_empty());
    assert_eq!(agent.pending_inbox_count().await, 2);

    let inbox = agent.list_inbox().await;
    let legal = inbox.iter().find(|m| m.remote_id.as_deref() == Some("c-legal")).unwrap();
    assert_eq!(legal.category, ClassificationCategory::SecurityOrLegalEscalation);
    assert_eq!(legal.status, STATUS_PENDING_APPROVAL);
    assert!(legal.drafted_reply.is_none());

    let fan = inbox.iter().find(|m| m.remote_id.as_deref() == Some("c-love")).unwrap();
    assert_eq!(fan.category, ClassificationCategory::Compliment);
    assert!(fan.is_auto_reply_eligible);
    let draft = fan.drafted_reply.clone().unwrap();

    agent.approve_and_send_reply(&fan.message_id, &draft).await.unwrap();
    assert_eq!(yt.calls(), vec![format!("reply:c-love|{draft}")]);
    assert_eq!(agent.get_message(&fan.message_id).await.unwrap().status, STATUS_REPLIED);
    assert!(agent.approve_and_send_reply(&fan.message_id, "again").await.is_err());
}

#[tokio::test]
async fn manually_ingested_messages_cannot_be_replied_to_without_a_remote_thread() {
    let agent = agent_with(vec![("x", RecordingConnector::new("x"))]);
    let msg = agent
        .ingest_message(AccountId::new(), "x", "@someone", InteractionType::Mention, "How much is the pro plan?")
        .await
        .unwrap();
    assert_eq!(msg.category, ClassificationCategory::SalesLead);
    let err = agent.approve_and_send_reply(&msg.message_id, "DM us!").await.unwrap_err();
    assert!(err.to_string().contains("no remote thread"));
    assert_ne!(agent.get_message(&msg.message_id).await.unwrap().status, STATUS_REPLIED);
}

#[tokio::test]
async fn failed_reply_restores_message_status() {
    let yt = Arc::new(RecordingConnector {
        platform: "youtube".into(),
        calls: Mutex::new(vec![]),
        inbox: vec![interaction("c1", "@fan", "Thanks, great video!", InteractionKind::Comment)],
        fail_ops: vec!["reply"],
    });
    let agent = agent_with(vec![("youtube", yt)]);
    let added = agent.sync_inbox(AccountId::new(), "youtube").await.unwrap();
    let id = &added[0].message_id;
    let before = added[0].status.clone();
    assert!(agent.approve_and_send_reply(id, "Thank you!").await.is_err());
    assert_eq!(agent.get_message(id).await.unwrap().status, before);
}

#[test]
fn heuristics_and_sanitising() {
    assert_eq!(classify_heuristic("FREE FOLLOWERS click the link").category, ClassificationCategory::Spam);
    assert_eq!(classify_heuristic("The app crashes on login").category, ClassificationCategory::CustomerSupport);
    assert_eq!(classify_heuristic("Worst update ever, I want a refund").category, ClassificationCategory::Complaint);
    let long = "word ".repeat(100);
    assert!(sanitize_reply(&long, "x").chars().count() <= 280);
    assert_eq!(sanitize_reply("  \"hi there\"  ", "x"), "hi there");
    let caps = EngagementCaps::from_profile_metadata(&serde_json::json!({ "daily_caps": { "likes": 7, "replies": 3 } }));
    assert_eq!(caps.limit(EngagementActionType::Like), 7);
    assert_eq!(caps.limit(EngagementActionType::Reply), 3);
}

#[derive(Default)]
struct MemoryStore {
    actions: Mutex<Vec<openaccounts_engagement_agent::EngagementActionResult>>,
    messages: Mutex<std::collections::HashMap<String, openaccounts_engagement_agent::UnifiedInboxMessage>>,
}

#[async_trait]
impl openaccounts_engagement_agent::EngagementStore for MemoryStore {
    async fn load_actions_since(&self, since: DateTime<Utc>) -> Result<Vec<openaccounts_engagement_agent::EngagementActionResult>> {
        Ok(self.actions.lock().unwrap().iter().filter(|a| a.executed_at >= since).cloned().collect())
    }
    async fn record_action(&self, action: &openaccounts_engagement_agent::EngagementActionResult) -> Result<()> {
        self.actions.lock().unwrap().push(action.clone());
        Ok(())
    }
    async fn load_inbox(&self) -> Result<Vec<openaccounts_engagement_agent::UnifiedInboxMessage>> {
        Ok(self.messages.lock().unwrap().values().cloned().collect())
    }
    async fn save_message(&self, message: &openaccounts_engagement_agent::UnifiedInboxMessage) -> Result<()> {
        self.messages.lock().unwrap().insert(message.message_id.clone(), message.clone());
        Ok(())
    }
}

#[tokio::test]
async fn caps_and_inbox_survive_a_restart() {
    let store = Arc::new(MemoryStore::default());
    let account = AccountId::new();
    let mut caps = EngagementCaps::default();
    caps.daily.insert(EngagementActionType::Like, 1);

    let yt = RecordingConnector::with_inbox(
        "youtube",
        vec![interaction("c-1", "@fan", "Great video, thanks!", InteractionKind::Comment)],
    );
    let first = agent_with(vec![("x", RecordingConnector::new("x")), ("youtube", yt)]).with_store(store.clone());
    first.set_account_caps(account.clone(), caps.clone()).await;
    first.like_post(account.clone(), "x", "post-1").await.unwrap();
    first.sync_inbox(account.clone(), "youtube").await.unwrap();
    let msg_id = first.list_inbox().await[0].message_id.clone();
    first.archive_message(&msg_id).await.unwrap();

    let x = RecordingConnector::new("x");
    let yt_again = RecordingConnector::with_inbox(
        "youtube",
        vec![interaction("c-1", "@fan", "Great video, thanks!", InteractionKind::Comment)],
    );
    let second = agent_with(vec![("x", x.clone()), ("youtube", yt_again)]).with_store(store.clone());
    assert_eq!(second.restore().await.unwrap(), (1, 1));
    second.set_account_caps(account.clone(), caps).await;

    assert_eq!(second.remaining_quota(&account, EngagementActionType::Like).await, 0);
    assert!(second.like_post(account.clone(), "x", "post-2").await.is_err());
    assert!(x.calls().is_empty(), "cap must hold across restarts without calling the platform");
    assert_eq!(second.get_message(&msg_id).await.unwrap().status, "Archived");
    assert!(second.sync_inbox(account, "youtube").await.unwrap().is_empty(), "synced items must not be re-ingested");
}
