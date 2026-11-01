use chrono::{DateTime, Utc};
use openaccounts_core::{ActionType, Result, RiskLevel};
use openaccounts_database::Database;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

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
        let id = Uuid::new_v4().to_string();
        let action_str = serde_json::to_string(action_type).unwrap_or_default();
        let risk_str = format!("{:?}", risk_level);
        let details_str = serde_json::to_string(&details).unwrap_or_default();
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            r#"
            INSERT INTO audit_logs (id, execution_id, account_id, automation_id, action_type, risk_level, details, status, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(execution_id)
        .bind(account_id)
        .bind(automation_id)
        .bind(&action_str)
        .bind(&risk_str)
        .bind(&details_str)
        .bind(status)
        .bind(&now)
        .execute(self.db.pool())
        .await
        .map_err(|e| openaccounts_core::OpenAccountsError::Database(e.to_string()))?;

        Ok(id)
    }
}
