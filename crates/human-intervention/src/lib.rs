use chrono::{DateTime, Utc};
use openaccounts_browser_abstraction::ChallengeType;
use openaccounts_core::{OpenAccountsError, Result, TaskId};
use openaccounts_database::Database;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{broadcast, RwLock};
use tracing::{info, warn};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HumanTaskStatus {
    Pending,
    InProgress,
    Resolved,
    Dismissed,
}

impl HumanTaskStatus {
    fn as_str(&self) -> &'static str {
        match self {
            HumanTaskStatus::Pending => "Pending",
            HumanTaskStatus::InProgress => "InProgress",
            HumanTaskStatus::Resolved => "Resolved",
            HumanTaskStatus::Dismissed => "Dismissed",
        }
    }

    fn parse(s: &str) -> Self {
        match s {
            "InProgress" => HumanTaskStatus::InProgress,
            "Resolved" => HumanTaskStatus::Resolved,
            "Dismissed" => HumanTaskStatus::Dismissed,
            _ => HumanTaskStatus::Pending,
        }
    }

    pub fn is_open(&self) -> bool {
        matches!(self, HumanTaskStatus::Pending | HumanTaskStatus::InProgress)
    }
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

fn parse_time(s: Option<String>) -> Option<DateTime<Utc>> {
    s.and_then(|v| DateTime::parse_from_rfc3339(&v).ok()).map(|d| d.with_timezone(&Utc))
}

pub struct HumanInterventionController {
    tasks: Arc<RwLock<HashMap<TaskId, HumanTask>>>,
    notifier: broadcast::Sender<HumanTask>,
    db: Option<Arc<Database>>,
}

impl Default for HumanInterventionController {
    fn default() -> Self {
        Self::new()
    }
}

impl HumanInterventionController {
    pub fn new() -> Self {
        let (notifier, _) = broadcast::channel(100);
        Self {
            tasks: Arc::new(RwLock::new(HashMap::new())),
            notifier,
            db: None,
        }
    }

    pub async fn with_database(db: Arc<Database>) -> Result<Self> {
        let rows = sqlx::query(
            "SELECT id, account_id, automation_id, task_type, title, reason, page_url, status, created_at, resolved_at FROM human_tasks",
        )
        .fetch_all(db.pool())
        .await
        .map_err(|e| OpenAccountsError::Database(e.to_string()))?;

        let mut tasks = HashMap::new();
        for row in rows {
            let id_str: String = row.get("id");
            let Ok(uuid) = uuid::Uuid::parse_str(&id_str) else { continue };
            let Ok(challenge_type) = serde_json::from_str::<ChallengeType>(&row.get::<String, _>("task_type")) else {
                warn!("Skipping human task {} with unknown challenge type", id_str);
                continue;
            };
            let reason: String = row.get("reason");
            let task = HumanTask {
                id: TaskId(uuid),
                account_id: row.get("account_id"),
                automation_id: row.get("automation_id"),
                challenge_type,
                title: row.get::<Option<String>, _>("title").unwrap_or_else(|| reason.clone()),
                instructions: reason,
                page_url: row.get("page_url"),
                status: HumanTaskStatus::parse(&row.get::<String, _>("status")),
                created_at: parse_time(Some(row.get("created_at"))).unwrap_or_else(Utc::now),
                resolved_at: parse_time(row.get("resolved_at")),
            };
            tasks.insert(task.id.clone(), task);
        }
        info!("Loaded {} human intervention tasks", tasks.len());

        let mut controller = Self::new();
        controller.tasks = Arc::new(RwLock::new(tasks));
        controller.db = Some(db);
        Ok(controller)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<HumanTask> {
        self.notifier.subscribe()
    }

    async fn persist(&self, task: &HumanTask) -> Result<()> {
        let Some(db) = &self.db else { return Ok(()) };
        sqlx::query(
            "INSERT INTO human_tasks (id, account_id, automation_id, task_type, title, reason, page_url, status, created_at, resolved_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(id) DO UPDATE SET status = excluded.status, resolved_at = excluded.resolved_at",
        )
        .bind(task.id.0.to_string())
        .bind(&task.account_id)
        .bind(&task.automation_id)
        .bind(serde_json::to_string(&task.challenge_type)?)
        .bind(&task.title)
        .bind(&task.instructions)
        .bind(&task.page_url)
        .bind(task.status.as_str())
        .bind(task.created_at.to_rfc3339())
        .bind(task.resolved_at.map(|t| t.to_rfc3339()))
        .execute(db.pool())
        .await
        .map_err(|e| OpenAccountsError::Database(e.to_string()))?;
        Ok(())
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

        self.persist(&task).await?;
        self.tasks.write().await.insert(id.clone(), task.clone());
        let _ = self.notifier.send(task);
        info!("Human intervention requested: id={}", id);
        Ok(id)
    }

    async fn transition(&self, id: &TaskId, status: HumanTaskStatus) -> Result<HumanTask> {
        let updated = {
            let mut tasks = self.tasks.write().await;
            let task = tasks
                .get_mut(id)
                .ok_or_else(|| OpenAccountsError::Internal("Task not found".into()))?;
            if !task.status.is_open() && status != HumanTaskStatus::InProgress {
                return Err(OpenAccountsError::Internal(format!(
                    "Task {} is already {}",
                    id,
                    task.status.as_str()
                )));
            }
            task.status = status;
            task.resolved_at = if status.is_open() { None } else { Some(Utc::now()) };
            task.clone()
        };
        self.persist(&updated).await?;
        let _ = self.notifier.send(updated.clone());
        Ok(updated)
    }

    pub async fn start_task(&self, id: &TaskId) -> Result<()> {
        self.transition(id, HumanTaskStatus::InProgress).await.map(|_| ())
    }

    pub async fn resolve_task(&self, id: &TaskId) -> Result<()> {
        self.transition(id, HumanTaskStatus::Resolved).await?;
        info!("Human intervention resolved: id={}", id);
        Ok(())
    }

    pub async fn dismiss_task(&self, id: &TaskId) -> Result<()> {
        self.transition(id, HumanTaskStatus::Dismissed).await?;
        info!("Human intervention dismissed: id={}", id);
        Ok(())
    }

    pub async fn get_task(&self, id: &TaskId) -> Option<HumanTask> {
        self.tasks.read().await.get(id).cloned()
    }

    pub async fn list_pending_tasks(&self) -> Vec<HumanTask> {
        let mut out: Vec<HumanTask> = self
            .tasks
            .read()
            .await
            .values()
            .filter(|t| t.status.is_open())
            .cloned()
            .collect();
        out.sort_by_key(|t| t.created_at);
        out
    }

    pub async fn list_recent_tasks(&self, limit: usize) -> Vec<HumanTask> {
        let mut out: Vec<HumanTask> = self.tasks.read().await.values().cloned().collect();
        out.sort_by_key(|t| std::cmp::Reverse(t.created_at));
        out.truncate(limit);
        out
    }
}
