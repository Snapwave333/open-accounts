use crate::state::AppState;
use chrono::{DateTime, Utc};
use openaccounts_account_registry::AccountRecord;
use serde::Serialize;
use std::collections::HashMap;

const COLUMN_LIMIT: usize = 15;

#[derive(Debug, Clone, Serialize)]
pub struct FlowCardItem {
    pub id: String,
    pub kind: String,
    pub ref_id: Option<String>,
    pub title: String,
    pub detail: String,
    pub tag: String,
    pub time_label: String,
    pub eta: Option<String>,
    pub progress: Option<u8>,
    pub platform: String,
    pub account_handle: String,
    pub can_run_now: bool,
    #[serde(skip)]
    pub sort_key: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ActivityFlow {
    pub just_did: Vec<FlowCardItem>,
    pub actively_doing: Vec<FlowCardItem>,
    pub up_next: Vec<FlowCardItem>,
}

pub fn relative(t: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let secs = (now - t).num_seconds();
    let (future, secs) = if secs < 0 { (true, -secs) } else { (false, secs) };
    let span = if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86_400 {
        format!("{}h", secs / 3600)
    } else {
        format!("{}d", secs / 86_400)
    };
    if future { format!("in {span}") } else { format!("{span} ago") }
}

fn humanize(action: &str) -> String {
    let cleaned = action.trim_matches('"').replace('_', " ");
    let mut out = String::with_capacity(cleaned.len() + 4);
    for (i, ch) in cleaned.chars().enumerate() {
        if i > 0 && ch.is_uppercase() && !cleaned.chars().nth(i - 1).is_some_and(|p| p == ' ' || p.is_uppercase()) {
            out.push(' ');
        }
        out.push(ch);
    }
    let mut chars = out.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => out,
    }
}

struct AccountIndex(HashMap<String, (String, String)>);

impl AccountIndex {
    fn new(accounts: &[AccountRecord]) -> Self {
        Self(
            accounts
                .iter()
                .map(|a| (a.id.0.to_string(), (a.username.clone(), a.platform.to_string())))
                .collect(),
        )
    }

    fn lookup(&self, id: Option<&str>, fallback_platform: &str) -> (String, String) {
        id.and_then(|i| self.0.get(i).cloned())
            .unwrap_or_else(|| ("OpenAccounts".into(), fallback_platform.to_string()))
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(n).collect::<String>())
    }
}

pub async fn build(state: &AppState) -> ActivityFlow {
    let now = Utc::now();
    let accounts = state.account_registry.list_accounts().await.unwrap_or_default();
    let index = AccountIndex::new(&accounts);
    let mut flow = ActivityFlow::default();

    if let Ok(entries) = state.audit_engine.recent(COLUMN_LIMIT as u32).await {
        for e in entries {
            let platform = e.details.get("platform").and_then(|v| v.as_str()).unwrap_or("system").to_string();
            let (handle, platform) = index.lookup(e.account_id.as_deref(), &platform);
            flow.just_did.push(FlowCardItem {
                id: format!("audit-{}", e.id),
                kind: "audit".into(),
                ref_id: Some(e.id.clone()),
                title: humanize(&e.action_type),
                detail: format!("Outcome: {} · risk {}", e.status, e.risk_level),
                tag: e.status.to_uppercase(),
                time_label: relative(e.created_at, now),
                eta: None,
                progress: None,
                platform,
                account_handle: handle,
                can_run_now: false,
                sort_key: e.created_at,
            });
        }
    }

    for r in state.engagement_agent.get_engagement_history().await.into_iter().rev().take(COLUMN_LIMIT) {
        let (handle, _) = index.lookup(Some(&r.account_id.0.to_string()), &r.platform);
        flow.just_did.push(FlowCardItem {
            id: format!("engagement-{}", r.action_id),
            kind: "engagement".into(),
            ref_id: r.permalink.clone(),
            title: format!("{:?} on {}", r.action_type, r.platform),
            detail: truncate(&r.target_id, 120),
            tag: if r.success { "DONE".into() } else { "FAILED".into() },
            time_label: relative(r.executed_at, now),
            eta: None,
            progress: None,
            platform: r.platform.clone(),
            account_handle: handle,
            can_run_now: false,
            sort_key: r.executed_at,
        });
    }

    for job in state.creative_pipeline.list_jobs().await {
        let updated = job.updated_at.unwrap_or(job.created_at);
        let item = FlowCardItem {
            id: format!("render-{}", job.job_id),
            kind: "render".into(),
            ref_id: Some(job.job_id.clone()),
            title: format!("Render {}", job.asset_id),
            detail: match (&job.error_message, &job.output_path) {
                (Some(err), _) => truncate(err, 140),
                (None, Some(path)) => path.clone(),
                (None, None) => job.current_stage.clone(),
            },
            tag: job.status.to_uppercase(),
            time_label: relative(updated, now),
            eta: None,
            progress: Some(job.progress_percent),
            platform: "youtube".into(),
            account_handle: "Creative pipeline".into(),
            can_run_now: job.status == "Queued",
            sort_key: updated,
        };
        match job.status.as_str() {
            "Processing" => flow.actively_doing.push(item),
            "Queued" => flow.up_next.push(item),
            _ => flow.just_did.push(item),
        }
    }

    for task in state.human_controller.list_pending_tasks().await {
        if matches!(task.status, openaccounts_human_intervention::HumanTaskStatus::InProgress) {
            let (handle, platform) = index.lookup(task.account_id.as_deref(), "system");
            flow.actively_doing.push(FlowCardItem {
                id: format!("task-{}", task.id),
                kind: "human_task".into(),
                ref_id: Some(task.id.to_string()),
                title: task.title.clone(),
                detail: truncate(&task.instructions, 140),
                tag: "WITH YOU".into(),
                time_label: relative(task.created_at, now),
                eta: None,
                progress: None,
                platform,
                account_handle: handle,
                can_run_now: false,
                sort_key: task.created_at,
            });
        }
    }

    let jobs = state.scheduler.list_jobs().await;
    let loop_running = state.social_media_manager.is_loop_running() && !state.automation_engine.is_kill_switch_active();
    for job in jobs {
        flow.up_next.push(FlowCardItem {
            id: format!("job-{}", job.id),
            kind: "job".into(),
            ref_id: Some(job.id.to_string()),
            title: job.name.clone(),
            detail: format!(
                "{} · {} runs · last: {}",
                job.cron_expression,
                job.run_count,
                job.last_outcome.clone().unwrap_or_else(|| "not run yet".into())
            ),
            tag: if job.is_active { "SCHEDULED".into() } else { "PAUSED".into() },
            time_label: job.last_run.map(|t| relative(t, now)).unwrap_or_else(|| "never ran".into()),
            eta: Some(relative(job.next_run, now)),
            progress: None,
            platform: "system".into(),
            account_handle: "Scheduler".into(),
            can_run_now: true,
            sort_key: job.next_run,
        });
    }

    for slot in state.content_strategy.list_calendar().await {
        if slot.status == "Published" {
            continue;
        }
        let (handle, platform) = index.lookup(slot.account_id.as_deref(), &slot.target_platform);
        flow.up_next.push(FlowCardItem {
            id: format!("slot-{}", slot.slot_id),
            kind: "slot".into(),
            ref_id: Some(slot.slot_id.clone()),
            title: format!("{} post", humanize(&slot.content_type)),
            detail: truncate(&slot.caption, 140),
            tag: if slot.is_approved { "APPROVED".into() } else { "NEEDS APPROVAL".into() },
            time_label: slot.status.clone(),
            eta: Some(relative(slot.scheduled_time, now)),
            progress: None,
            platform,
            account_handle: handle,
            can_run_now: slot.is_approved && loop_running,
            sort_key: slot.scheduled_time,
        });
    }

    for msg in state.engagement_agent.list_inbox().await {
        if msg.status != "New" && msg.status != "PendingApproval" {
            continue;
        }
        let (handle, _) = index.lookup(Some(&msg.account_id.0.to_string()), &msg.platform);
        flow.up_next.push(FlowCardItem {
            id: format!("inbox-{}", msg.message_id),
            kind: "inbox".into(),
            ref_id: Some(msg.message_id.clone()),
            title: format!("Reply to {}", msg.author_handle),
            detail: truncate(&msg.content_text, 140),
            tag: if msg.requires_human_approval { "NEEDS REVIEW".into() } else { format!("{:?}", msg.category).to_uppercase() },
            time_label: relative(msg.received_at, now),
            eta: None,
            progress: None,
            platform: msg.platform.clone(),
            account_handle: handle,
            can_run_now: false,
            sort_key: msg.received_at,
        });
    }

    flow.just_did.sort_by_key(|i| std::cmp::Reverse(i.sort_key));
    flow.just_did.truncate(COLUMN_LIMIT);
    flow.actively_doing.sort_by_key(|i| std::cmp::Reverse(i.sort_key));
    flow.up_next.sort_by_key(|i| i.sort_key);
    flow.up_next.truncate(COLUMN_LIMIT * 2);
    flow
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_labels_and_humanized_actions() {
        let now = Utc::now();
        assert_eq!(relative(now - chrono::Duration::minutes(5), now), "5m ago");
        assert_eq!(relative(now + chrono::Duration::hours(2), now), "in 2h");
        assert_eq!(humanize("\"like_post\""), "Like post");
        assert_eq!(humanize("PolicyVerification"), "Policy Verification");
    }
}
