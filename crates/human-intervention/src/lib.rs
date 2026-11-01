use chrono::{DateTime, Utc};
use openaccounts_browser_abstraction::ChallengeType;
use openaccounts_core::{OpenAccountsError, Result, TaskId};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{broadcast, RwLock};
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HumanTaskStatus {
    Pending,
    InProgress,
    Resolved,
    Dismissed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HumanTask {
    pub id: TaskId,
    pub account_id: Option<String>,
    pub automation_id: Option<String>,
    pub challenge_type: ChallengeType,
    pub title: String,
    pub instructions: String,
    pub page_url: Option<String>,
    pub status: HumanTaskStatus,
    pub created_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
}

pub struct HumanInterventionController {
    tasks: Arc<RwLock<HashMap<TaskId, HumanTask>>>,
    notifier: broadcast::Sender<HumanTask>,
}

impl HumanInterventionController {
    pub fn new() -> Self {
        let (notifier, _) = broadcast::channel(100);
        Self {
            tasks: Arc::new(RwLock::new(HashMap::new())),
            notifier,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<HumanTask> {
        self.notifier.subscribe()
    }

    pub async fn create_task(
        &self,
        account_id: Option<String>,
        automation_id: Option<String>,
        challenge_type: ChallengeType,
        title: String,
        instructions: String,
        page_url: Option<String>,
    ) -> Result<TaskId> {
        let id = TaskId::new();
        let task = HumanTask {
            id: id.clone(),
            account_id,
            automation_id,
            challenge_type,
            title,
            instructions,
            page_url,
            status: HumanTaskStatus::Pending,
            created_at: Utc::now(),
            resolved_at: None,
        };

        self.tasks.write().await.insert(id.clone(), task.clone());
        let _ = self.notifier.send(task);
        info!("Human intervention requested: id={}", id);
        Ok(id)
    }

    pub async fn resolve_task(&self, id: &TaskId) -> Result<()> {
        let mut tasks = self.tasks.write().await;
        let task = tasks
            .get_mut(id)
            .ok_or_else(|| OpenAccountsError::Internal("Task not found".into()))?;

        task.status = HumanTaskStatus::Resolved;
        task.resolved_at = Some(Utc::now());
        let _ = self.notifier.send(task.clone());
        info!("Human intervention resolved: id={}", id);
        Ok(())
    }

    pub async fn list_pending_tasks(&self) -> Vec<HumanTask> {
        let tasks = self.tasks.read().await;
        tasks
            .values()
            .filter(|t| matches!(t.status, HumanTaskStatus::Pending | HumanTaskStatus::InProgress))
            .cloned()
            .collect()
    }
}
