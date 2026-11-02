//! The Nova Vexley influencer project: its human gates surface in the app's
//! Needs You tab. The pipeline writes `data/needs_you.json`; approving a task
//! here writes the matching gate into `data/approvals.json`, the same file
//! `py pipeline.py approve <gate>` writes.

use chrono::{DateTime, Utc};
use openaccounts_browser_abstraction::ChallengeType;
use openaccounts_core::TaskId;
use openaccounts_human_intervention::{HumanTask, HumanTaskStatus};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Namespace for turning a gate name into a stable task id.
const GATE_NAMESPACE: Uuid = Uuid::from_bytes([
    0x6e, 0x6f, 0x76, 0x61, 0x2d, 0x76, 0x65, 0x78, 0x6c, 0x65, 0x79, 0x2d, 0x67, 0x61, 0x74, 0x65,
]);

#[derive(Debug, Deserialize)]
struct NeedsYouFile {
    #[serde(default)]
    tasks: Vec<NeedsYouTask>,
}

#[derive(Debug, Deserialize)]
struct NeedsYouTask {
    gate: String,
    title: String,
    instructions: String,
    page_url: Option<String>,
    created_at: Option<String>,
}

pub fn root() -> PathBuf {
    openaccounts_core::influencer_root()
}

fn task_id_for(gate: &str) -> TaskId {
    TaskId(Uuid::new_v5(&GATE_NAMESPACE, gate.as_bytes()))
}

/// The pipeline's open gates as human tasks. Missing file means no gates.
pub fn pending_tasks() -> Vec<HumanTask> {
    let path = root().join("data").join("needs_you.json");
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let file: NeedsYouFile = match serde_json::from_str(&raw) {
        Ok(f) => f,
        Err(e) => {
            tracing::warn!("{} is not readable: {e}", path.display());
            return Vec::new();
        }
    };
    file.tasks
        .into_iter()
        .map(|t| HumanTask {
            id: task_id_for(&t.gate),
            account_id: None,
            automation_id: Some(format!("influencer:{}", t.gate)),
            challenge_type: ChallengeType::ManualReview,
            title: format!("Nova: {}", t.title),
            instructions: t.instructions,
            page_url: t.page_url,
            status: HumanTaskStatus::Pending,
            created_at: t
                .created_at
                .and_then(|c| DateTime::parse_from_rfc3339(&c).ok())
                .map(|c| c.with_timezone(&Utc))
                .unwrap_or_else(Utc::now),
            resolved_at: None,
        })
        .collect()
}

/// Grants the gate behind `task_id` if it is one of the pipeline's. Returns
/// false when the id belongs to no influencer gate.
pub fn approve(task_id: &TaskId) -> Result<bool, String> {
    let Some(task) = pending_tasks().into_iter().find(|t| &t.id == task_id) else {
        return Ok(false);
    };
    let gate = task
        .automation_id
        .as_deref()
        .and_then(|a| a.strip_prefix("influencer:"))
        .ok_or("task has no gate")?
        .to_string();

    let path = root().join("data").join("approvals.json");
    let mut doc: serde_json::Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_else(|| serde_json::json!({ "gates": {} }));
    doc["gates"][&gate] = serde_json::json!({
        "granted_at": Utc::now().to_rfc3339(),
        "by": "Eli (OpenAccounts app)",
        "note": ""
    });
    write_atomic(&path, serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())?.as_bytes())
        .map_err(|e| format!("cannot write {}: {e}", path.display()))?;

    // Drop the task from the list right away; the next `plan` run rewrites it anyway.
    let needs = root().join("data").join("needs_you.json");
    if let Ok(raw) = std::fs::read_to_string(&needs) {
        if let Ok(mut v) = serde_json::from_str::<serde_json::Value>(&raw) {
            if let Some(tasks) = v["tasks"].as_array_mut() {
                tasks.retain(|t| t["gate"].as_str() != Some(gate.as_str()));
                let _ = serde_json::to_string_pretty(&v).map(|s| write_atomic(&needs, s.as_bytes()));
            }
        }
    }
    tracing::info!("influencer gate {gate} granted from the app");
    Ok(true)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    std::fs::write(&tmp, bytes)?;
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}
