use chrono::{DateTime, Utc};
use openaccounts_core::{OpenAccountsError, Result, TaskId};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;

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

    /// There is no cron evaluator or job runner behind this scheduler, so
    /// registering a job would only record work that never runs. Recurring
    /// work runs through the desktop app's autonomy runtime instead.
    pub async fn register_job(&self, name: String, cron_expression: String) -> Result<TaskId> {
        Err(OpenAccountsError::NotConfigured(format!(
            "Cannot schedule '{name}' ({cron_expression}): no scheduler runner is connected"
        )))
    }

    pub async fn list_jobs(&self) -> Vec<ScheduleJob> {
        self.jobs.read().await.values().cloned().collect()
    }
}

impl Default for TaskScheduler {
    fn default() -> Self {
        Self::new()
    }
}
