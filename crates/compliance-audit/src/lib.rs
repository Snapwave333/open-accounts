use chrono::{DateTime, Utc};
use openaccounts_core::Result;
use openaccounts_database::Database;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

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
        let entry_id = Uuid::new_v4().to_string();
        let event_type_str = format!("{:?}", event_type);
        let now = Utc::now().to_rfc3339();

        if let Some(obj) = details.as_object_mut() {
            obj.insert("platform".into(), serde_json::json!(platform));
        }
        let details_str = serde_json::to_string(&details).unwrap_or_default();

        let _ = sqlx::query(
            r#"
            INSERT INTO audit_logs (id, execution_id, account_id, automation_id, action_type, risk_level, details, status, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&entry_id)
        .bind("COMPLIANCE_RUN")
        .bind(account_id)
        .bind(official_policy_url)
        .bind(&event_type_str)
        .bind("Low")
        .bind(&details_str)
        .bind(outcome)
        .bind(&now)
        .execute(self.db.pool())
        .await;

        Ok(entry_id)
    }
}
