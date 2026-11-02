use chrono::{DateTime, Utc};
use openaccounts_core::ExecutionStatus;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStep {
    pub step_id: String,
    pub name: String,
    pub action_type: String,
    pub parameters: serde_json::Value,
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowCheckpoint {
    pub workflow_id: String,
    pub current_step_index: usize,
    pub state_data: HashMap<String, serde_json::Value>,
    pub status: ExecutionStatus,
    pub saved_at: DateTime<Utc>,
}

pub struct WorkflowRuntime;

impl WorkflowRuntime {
    pub fn new() -> Self {
        Self
    }

    /// Builds a checkpoint value. It is not persisted anywhere; the caller
    /// owns storing it.
    pub fn build_checkpoint(
        &self,
        workflow_id: &str,
        step_index: usize,
        state: HashMap<String, serde_json::Value>,
        status: ExecutionStatus,
    ) -> WorkflowCheckpoint {
        WorkflowCheckpoint {
            workflow_id: workflow_id.to_string(),
            current_step_index: step_index,
            state_data: state,
            status,
            saved_at: Utc::now(),
        }
    }
}

impl Default for WorkflowRuntime {
    fn default() -> Self {
        Self::new()
    }
}
