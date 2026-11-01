use chrono::{DateTime, Utc};
use openaccounts_platform_intelligence::PlatformKnowledgeProfile;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyDiff {
    pub change_id: String,
    pub platform_id: String,
    pub section_modified: String,
    pub old_summary: String,
    pub new_summary: String,
    pub requires_workflow_pause: bool,
    pub detected_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyVersionRecord {
    pub version_id: String,
    pub platform_id: String,
    pub version_tag: String,
    pub snapshot: PlatformKnowledgeProfile,
    pub created_at: DateTime<Utc>,
    pub diffs_from_previous: Vec<PolicyDiff>,
}

pub struct PolicyVersioningRegistry {
    history: Arc<RwLock<HashMap<String, Vec<PolicyVersionRecord>>>>,
}

impl PolicyVersioningRegistry {
    pub fn new() -> Self {
        Self {
            history: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn record_version(
        &self,
        platform_id: &str,
        snapshot: PlatformKnowledgeProfile,
        diffs: Vec<PolicyDiff>,
    ) -> String {
        let version_id = Uuid::new_v4().to_string();
        let record = PolicyVersionRecord {
            version_id: version_id.clone(),
            platform_id: platform_id.to_string(),
            version_tag: snapshot.version.clone(),
            snapshot,
            created_at: Utc::now(),
            diffs_from_previous: diffs,
        };

        let mut lock = self.history.write().await;
        lock.entry(platform_id.to_string())
            .or_default()
            .push(record);

        version_id
    }

    pub async fn get_history(&self, platform_id: &str) -> Vec<PolicyVersionRecord> {
        self.history
            .read()
            .await
            .get(platform_id)
            .cloned()
            .unwrap_or_default()
    }
}

impl Default for PolicyVersioningRegistry {
    fn default() -> Self {
        Self::new()
    }
}
