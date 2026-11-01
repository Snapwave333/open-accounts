use chrono::{DateTime, Utc};
use openaccounts_core::{ActionType, Result, RiskLevel};
use openaccounts_database::{AuditInsert, ChainVerification, Database};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditRecord {
    pub id: String,
    pub execution_id: String,
    pub account_id: Option<String>,
    pub automation_id: Option<String>,
    pub action_type: String,
    pub risk_level: String,
    pub details: serde_json::Value,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

pub struct AuditEngine {
    db: Arc<Database>,
}

impl AuditEngine {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn log_action(
        &self,
        execution_id: &str,
        account_id: Option<&str>,
        automation_id: Option<&str>,
        action_type: &ActionType,
        risk_level: RiskLevel,
        details: serde_json::Value,
        status: &str,
    ) -> Result<String> {
        let action_str = serde_json::to_string(action_type)?;
        let risk_str = format!("{:?}", risk_level);
        let details_str = serde_json::to_string(&details)?;
        self.db
            .append_audit(AuditInsert {
                execution_id,
                account_id,
                automation_id,
                action_type: &action_str,
                risk_level: &risk_str,
                details: &details_str,
                status,
            })
            .await
    }

    pub async fn recent(&self, limit: u32) -> Result<Vec<AuditRecord>> {
        let rows = self.db.recent_audit(limit).await?;
        Ok(rows
            .into_iter()
            .map(|r| AuditRecord {
                id: r.id,
                execution_id: r.execution_id,
                account_id: r.account_id,
                automation_id: r.automation_id,
                action_type: r.action_type.trim_matches('"').to_string(),
                risk_level: r.risk_level,
                details: serde_json::from_str(&r.details).unwrap_or(serde_json::Value::String(r.details)),
                status: r.status,
                created_at: DateTime::parse_from_rfc3339(&r.created_at)
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
            })
            .collect())
    }

    pub async fn verify_integrity(&self) -> Result<ChainVerification> {
        self.db.verify_audit_chain().await
    }
}
