use crate::state::AppState;
use openaccounts_lm_gateway::{ChatMessage, CompletionRequest, DEFAULT_PROFILE_ID};
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub const CHAT_PROFILE_SETTING: &str = "agent_chat_profile";
const MAX_ATTACHMENT_CHARS: usize = 12_000;
const MAX_HISTORY: usize = 12;
const CHAT_TIMEOUT: Duration = Duration::from_secs(90);

#[derive(Debug, Clone, Deserialize)]
pub struct ChatTurn {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChatAttachmentInput {
    pub name: String,
    pub mime_type: String,
    pub size_label: String,
    pub text_content: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentChatReply {
    pub reply: String,
    pub model: String,
    pub profile_id: String,
    pub tokens_used: Option<u64>,
    pub proposed_actions: Vec<AgentAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentAction {
    pub kind: String,
    pub label: String,
    #[serde(default)]
    pub args: serde_json::Value,
}

pub const ACTION_KINDS: &[(&str, &str)] = &[
    ("sync_inbox", "{\"account\": \"<handle>\"} pull new comments/mentions for one account"),
    ("refresh_metrics", "{\"account\": \"<handle>\"} fetch follower/view metrics for one account"),
    ("run_policy_check", "{} re-check every platform's official policy pages"),
    ("run_job", "{\"name\": \"<scheduled job name>\"} run a scheduled job now"),
    ("pause_autopilot", "{} pause the autonomous marketing loop"),
    ("resume_autopilot", "{} resume the autonomous marketing loop"),
    ("research_trends", "{\"platform\": \"<platform>\"} fetch current trending topics"),
];

fn action_label(kind: &str, args: &serde_json::Value) -> String {
    let arg = |k: &str| args.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    match kind {
        "sync_inbox" => format!("Sync inbox for {}", arg("account")),
        "refresh_metrics" => format!("Refresh metrics for {}", arg("account")),
        "run_policy_check" => "Run policy check".into(),
        "run_job" => format!("Run job: {}", arg("name")),
        "pause_autopilot" => "Pause autopilot".into(),
        "resume_autopilot" => "Resume autopilot".into(),
        "research_trends" => format!("Research {} trends", arg("platform")),
        other => other.to_string(),
    }
}

pub fn extract_actions(text: &str) -> (String, Vec<AgentAction>) {
    let mut clean = String::with_capacity(text.len());
    let mut actions = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("```action") {
        clean.push_str(&rest[..start]);
        let after = &rest[start + "```action".len()..];
        let Some(end) = after.find("```") else {
            rest = "";
            break;
        };
        let body = after[..end].trim();
        rest = &after[end + 3..];
        let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else { continue };
        let entries = match value {
            serde_json::Value::Array(items) => items,
            other => vec![other],
        };
        for entry in entries {
            let kind = entry.get("action").or_else(|| entry.get("kind")).and_then(|v| v.as_str()).unwrap_or("");
            if !ACTION_KINDS.iter().any(|(k, _)| *k == kind) {
                continue;
            }
            let args = entry.get("args").cloned().unwrap_or_else(|| serde_json::json!({}));
            let action = AgentAction { kind: kind.to_string(), label: action_label(kind, &args), args };
            if !actions.contains(&action) && actions.len() < 4 {
                actions.push(action);
            }
        }
    }
    clean.push_str(rest);
    (clean.trim().to_string(), actions)
}

async fn resolve_account(state: &AppState, needle: &str) -> Result<openaccounts_account_registry::AccountRecord, String> {
    let needle = needle.trim().trim_start_matches('@').to_lowercase();
    if needle.is_empty() {
        return Err("No account specified".into());
    }
    let accounts = state.account_registry.list_accounts().await.map_err(|e| e.to_string())?;
    accounts
        .iter()
        .find(|a| a.id.0.to_string() == needle || a.username.trim_start_matches('@').to_lowercase() == needle)
        .cloned()
        .ok_or_else(|| format!("No account matches '{needle}'"))
}

pub async fn execute_action(state: &AppState, action: &AgentAction) -> Result<String, String> {
    let arg = |k: &str| action.args.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    let mutating = matches!(action.kind.as_str(), "sync_inbox" | "run_job" | "resume_autopilot");
    if mutating && state.automation_engine.is_kill_switch_active() {
        return Err("Kill switch is active; reset it first".into());
    }
    match action.kind.as_str() {
        "sync_inbox" => {
            let acc = resolve_account(state, &arg("account")).await?;
            let msgs = state
                .engagement_agent
                .sync_inbox(acc.id.clone(), &acc.platform.to_string())
                .await
                .map_err(|e| e.to_string())?;
            Ok(format!("Synced {} new interaction(s) for {}.", msgs.len(), acc.username))
        }
        "refresh_metrics" => {
            let acc = resolve_account(state, &arg("account")).await?;
            let m = state
                .social_analytics
                .refresh_metrics(&acc.id.0.to_string(), &acc.platform.to_string())
                .await
                .map_err(|e| e.to_string())?;
            Ok(format!(
                "{}: followers {}, views {}, posts {}.",
                acc.username,
                m.followers.map(|v| v.to_string()).unwrap_or_else(|| "n/a".into()),
                m.views,
                m.posts.map(|v| v.to_string()).unwrap_or_else(|| "n/a".into())
            ))
        }
        "run_policy_check" => {
            let r = state.policy_monitor.run_policy_check_cycle_report().await;
            Ok(format!(
                "Checked {} documents across {} platforms: {} changed, {} unreachable, {} new baselines.",
                r.documents_checked, r.platforms_checked, r.changes_detected, r.unreachable, r.baselines_recorded
            ))
        }
        "run_job" => {
            let name = arg("name").to_lowercase();
            let job = state
                .scheduler
                .list_jobs()
                .await
                .into_iter()
                .find(|j| !name.is_empty() && j.name.to_lowercase().contains(&name))
                .ok_or_else(|| format!("No scheduled job matches '{name}'"))?;
            let outcome = state.scheduler.run_now(&job.id).await.map_err(|e| e.to_string())?;
            Ok(format!("{}: {}", job.name, outcome))
        }
        "pause_autopilot" => {
            state.social_media_manager.pause_autonomous_loop();
            state.db.set_setting(crate::state::AUTOPILOT_SETTING, "false").await.map_err(|e| e.to_string())?;
            Ok("Autopilot paused.".into())
        }
        "resume_autopilot" => {
            state.social_media_manager.start_autonomous_loop();
            state.db.set_setting(crate::state::AUTOPILOT_SETTING, "true").await.map_err(|e| e.to_string())?;
            Ok("Autopilot resumed.".into())
        }
        "research_trends" => {
            let platform = arg("platform");
            let topics = state
                .content_strategy
                .research_trends(if platform.is_empty() { "all" } else { &platform })
                .await
                .map_err(|e| e.to_string())?;
            let top: Vec<String> = topics.iter().take(6).map(|t| format!("{} ({})", t.topic, t.volume_score)).collect();
            Ok(format!("Top trends: {}.", top.join(", ")))
        }
        other => Err(format!("'{other}' is not an allowed agent action")),
    }
}

async fn system_context(state: &AppState, scope: &str) -> String {
    let accounts = state.account_registry.list_accounts().await.unwrap_or_default();
    let tasks = state.human_controller.list_pending_tasks().await;
    let inbox = state.engagement_agent.list_inbox().await;
    let awaiting_inbox = inbox.iter().filter(|m| m.requires_human_approval && m.status != "Replied").count();
    let new_inbox = inbox.iter().filter(|m| m.status == "New").count();
    let calendar = state.content_strategy.list_calendar().await;
    let unapproved = calendar.iter().filter(|s| !s.is_approved).count();
    let jobs = state.scheduler.list_jobs().await;
    let vault = state.vault.status().await;

    let mut ctx = String::new();
    ctx.push_str(
        "You are the OpenAccounts operations agent embedded in a local desktop app that manages the operator's own social and email accounts. \
Answer using ONLY the live state below; if something is not in the state, say you don't know rather than guessing. \
You cannot execute actions directly. When one of the allowed actions below would help, propose it by ending your reply with a fenced block \
```action\n{\"action\": \"<kind>\", \"args\": {...}}\n``` (one JSON object or an array, at most 4). The operator must click to confirm, so never claim an action already happened. \
For anything else, tell them which screen or control to use. \
Never ask for or reveal passwords, tokens or vault contents. Be concise.\n\n",
    );
    ctx.push_str("Allowed actions:\n");
    for (kind, desc) in ACTION_KINDS {
        ctx.push_str(&format!("- {kind}: {desc}\n"));
    }
    ctx.push_str(&format!("Current time (UTC): {}\n", chrono::Utc::now().format("%Y-%m-%d %H:%M")));
    ctx.push_str(&format!(
        "Autopilot loop: {}. Kill switch: {}. Vault: {} ({} entries).\n",
        if state.social_media_manager.is_loop_running() { "running" } else { "paused" },
        if state.automation_engine.is_kill_switch_active() { "ACTIVE (all automation halted)" } else { "off" },
        if vault.unlocked { "unlocked" } else { "locked" },
        vault.secret_count
    ));
    ctx.push_str(&format!("Operator's chat focus: {scope}\n\nAccounts ({}):\n", accounts.len()));
    for a in accounts.iter().take(40) {
        ctx.push_str(&format!(
            "- {} on {} · status {} · last active {}\n",
            a.username,
            a.platform,
            a.status,
            a.last_active.map(|t| t.format("%Y-%m-%d").to_string()).unwrap_or_else(|| "never".into())
        ));
    }
    ctx.push_str(&format!("\nHuman tasks waiting ({}):\n", tasks.len()));
    for t in tasks.iter().take(15) {
        ctx.push_str(&format!("- [{:?}] {}\n", t.challenge_type, t.title));
    }
    ctx.push_str(&format!(
        "\nInbox: {} messages total, {} new, {} awaiting human approval.\nContent calendar: {} slots, {} awaiting approval.\n",
        inbox.len(),
        new_inbox,
        awaiting_inbox,
        calendar.len(),
        unapproved
    ));
    ctx.push_str(&format!("\nScheduled jobs ({}):\n", jobs.len()));
    for j in jobs.iter().take(10) {
        ctx.push_str(&format!(
            "- {} ({}) next {} · last outcome: {}\n",
            j.name,
            j.cron_expression,
            j.next_run.format("%Y-%m-%d %H:%M"),
            j.last_outcome.clone().unwrap_or_else(|| "not run yet".into())
        ));
    }
    if let Ok(recent) = state.audit_engine.recent(10).await {
        ctx.push_str("\nRecent audit entries:\n");
        for r in recent {
            ctx.push_str(&format!("- {} {} → {}\n", r.created_at.format("%m-%d %H:%M"), r.action_type, r.status));
        }
    }
    ctx
}

pub async fn chat(
    state: &AppState,
    message: &str,
    scope: &str,
    history: Vec<ChatTurn>,
    attachments: Vec<ChatAttachmentInput>,
) -> Result<AgentChatReply, String> {
    if message.trim().is_empty() && attachments.is_empty() {
        return Err("Message is empty".into());
    }

    let mut messages = vec![ChatMessage::system(system_context(state, scope).await)];
    for turn in history.into_iter().rev().take(MAX_HISTORY).rev() {
        let role = if turn.role == "assistant" { "assistant" } else { "user" };
        if !turn.content.trim().is_empty() {
            messages.push(ChatMessage { role: role.into(), content: turn.content });
        }
    }

    let mut user = message.trim().to_string();
    let mut budget = MAX_ATTACHMENT_CHARS;
    for att in &attachments {
        user.push_str(&format!("\n\n[Attachment: {} · {} · {}]", att.name, att.mime_type, att.size_label));
        if let Some(text) = att.text_content.as_deref().filter(|t| !t.trim().is_empty()) {
            let excerpt: String = text.chars().take(budget).collect();
            budget = budget.saturating_sub(excerpt.chars().count());
            user.push_str(&format!("\n```\n{excerpt}\n```"));
        } else {
            user.push_str(" (binary content not included)");
        }
    }
    messages.push(ChatMessage::user(user));

    let preferred = state
        .db
        .get_setting(CHAT_PROFILE_SETTING)
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| DEFAULT_PROFILE_ID.to_string());
    let mut chain = vec![preferred.clone()];
    for p in state.lm_gateway.list_profiles().await {
        if !chain.contains(&p.id) && !p.id.contains("vision") {
            chain.push(p.id);
        }
    }

    let request = CompletionRequest::chat(messages).with_max_tokens(900).with_temperature(0.4);
    let mut attempts: Vec<String> = Vec::new();
    for profile_id in chain {
        match tokio::time::timeout(CHAT_TIMEOUT, state.lm_gateway.generate(&profile_id, request.clone(), None)).await {
            Ok(Ok(resp)) => {
                let (reply, proposed_actions) = extract_actions(&resp.content);
                return Ok(AgentChatReply {
                    reply,
                    proposed_actions,
                    model: resp.model,
                    profile_id,
                    tokens_used: resp.tokens_used,
                })
            }
            Ok(Err(e)) => attempts.push(format!("{profile_id}: {e}")),
            Err(_) => attempts.push(format!("{profile_id}: timed out after {}s", CHAT_TIMEOUT.as_secs())),
        }
    }
    Err(format!(
        "No language model answered. Tried {}. Configure NVIDIA_API_KEY or register another LM profile.",
        if attempts.is_empty() { "nothing".to_string() } else { attempts.join(" · ") }
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_whitelisted_actions_and_strips_blocks() {
        let text = "Your inbox is stale.\n```action\n[{\"action\": \"sync_inbox\", \"args\": {\"account\": \"@snapwave\"}}, {\"action\": \"delete_everything\"}]\n```\nAlso:\n```action\n{\"action\": \"run_policy_check\"}\n```";
        let (clean, actions) = extract_actions(text);
        assert_eq!(clean, "Your inbox is stale.\n\nAlso:");
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0].kind, "sync_inbox");
        assert_eq!(actions[0].label, "Sync inbox for @snapwave");
        assert_eq!(actions[1].kind, "run_policy_check");
    }

    #[test]
    fn malformed_blocks_are_ignored() {
        let (clean, actions) = extract_actions("ok ```action\nnot json\n``` done");
        assert_eq!(clean, "ok  done");
        assert!(actions.is_empty());
        let (clean, actions) = extract_actions("no actions here");
        assert_eq!(clean, "no actions here");
        assert!(actions.is_empty());
    }
}
