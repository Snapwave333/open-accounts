use chrono::{DateTime, Utc};
use openaccounts_core::TaskId;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleJob {
    pub id: TaskId,
    pub name: String,
    pub cron_expression: String,
    pub next_run: DateTime<Utc>,
    pub is_active: bool,
}

pub struct TaskScheduler {
    jobs: Arc<RwLock<HashMap<TaskId, ScheduleJob>>>,
}

impl TaskScheduler {
    pub fn new() -> Self {
        Self {
            jobs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn register_job(&self, name: String, cron_expression: String) -> TaskId {
        let id = TaskId::new();
        let job = ScheduleJob {
            id: id.clone(),
            name,
            cron_expression,
            next_run: Utc::now() + chrono::Duration::hours(1),
            is_active: true,
        };
        self.jobs.write().await.insert(id.clone(), job);
        info!("Scheduled job registered: id={}", id);
        id
    }

    pub async fn list_jobs(&self) -> Vec<ScheduleJob> {
        self.jobs.read().await.values().cloned().collect()
    }
}
