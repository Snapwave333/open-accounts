use chrono::{DateTime, Utc};
use openaccounts_core::Result;
use openaccounts_database::{AuditInsert, Database};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ComplianceEventType {
    PolicyVerification,
    MultiAccountCheck,
    EmailIdentityAllocated,
    OtpExtracted,
    HumanApprovalGranted,
    PreflightStarted,
    PreflightCompleted,
    PolicyChangeDetected,
    WorkflowPaused,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceAuditEntry {
    pub entry_id: String,
    pub event_type: ComplianceEventType,
    pub platform: String,
    pub account_id: Option<String>,
    pub official_policy_url: Option<String>,
    pub outcome: String,
    pub details: serde_json::Value,
    pub timestamp: DateTime<Utc>,
}

pub struct ComplianceAuditEngine {
    db: Arc<Database>,
}

impl ComplianceAuditEngine {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    pub async fn log_event(
        &self,
        event_type: ComplianceEventType,
        platform: &str,
        account_id: Option<&str>,
        official_policy_url: Option<&str>,
        outcome: &str,
        mut details: serde_json::Value,
    ) -> Result<String> {
        let event_type_str = format!("{:?}", event_type);
        if let Some(obj) = details.as_object_mut() {
            obj.insert("platform".into(), serde_json::json!(platform));
            if let Some(url) = official_policy_url {
                obj.insert("official_policy_url".into(), serde_json::json!(url));
            }
        }
        let details_str = serde_json::to_string(&details)?;
        self.db
            .append_audit(AuditInsert {
                execution_id: "COMPLIANCE_RUN",
                account_id,
                automation_id: None,
                action_type: &event_type_str,
                risk_level: "Low",
                details: &details_str,
                status: outcome,
            })
            .await
    }
}
