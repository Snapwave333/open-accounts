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
You cannot execute actions yourself: when the operator wants something done, tell them exactly which screen or control to use. \
Never ask for or reveal passwords, tokens or vault contents. Be concise.\n\n",
    );
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
    let mut last_err = String::from("no LM profile available");
    for profile_id in chain {
        match tokio::time::timeout(CHAT_TIMEOUT, state.lm_gateway.generate(&profile_id, request.clone(), None)).await {
            Ok(Ok(resp)) => {
                return Ok(AgentChatReply {
                    reply: resp.content,
                    model: resp.model,
                    profile_id,
                    tokens_used: resp.tokens_used,
                })
            }
            Ok(Err(e)) => last_err = format!("{profile_id}: {e}"),
            Err(_) => last_err = format!("{profile_id}: timed out after {}s", CHAT_TIMEOUT.as_secs()),
        }
    }
    Err(format!(
        "No language model answered ({last_err}). Configure NVIDIA_API_KEY or register another LM profile."
    ))
}
