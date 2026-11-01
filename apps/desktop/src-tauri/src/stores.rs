use async_trait::async_trait;
use chrono::{DateTime, Utc};
use openaccounts_content_strategy::{CalendarPostSlot, CalendarStore};
use openaccounts_core::{OpenAccountsError, Result};
use openaccounts_database::Database;
use openaccounts_engagement_agent::{EngagementActionResult, EngagementStore, UnifiedInboxMessage};
use sqlx::Row;
use std::sync::Arc;
use tracing::warn;

pub struct SqliteEngagementStore {
    db: Arc<Database>,
}

impl SqliteEngagementStore {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }
}

fn db_err(e: sqlx::Error) -> OpenAccountsError {
    OpenAccountsError::Database(e.to_string())
}

#[async_trait]
impl EngagementStore for SqliteEngagementStore {
    async fn load_actions_since(&self, since: DateTime<Utc>) -> Result<Vec<EngagementActionResult>> {
        let rows = sqlx::query("SELECT payload FROM engagement_actions WHERE executed_at >= ? ORDER BY executed_at")
            .bind(since.to_rfc3339())
            .fetch_all(self.db.pool())
            .await
            .map_err(db_err)?;
        Ok(rows
            .into_iter()
            .filter_map(|r| {
                serde_json::from_str(&r.get::<String, _>("payload"))
                    .map_err(|e| warn!("Skipping unreadable engagement action: {}", e))
                    .ok()
            })
            .collect())
    }

    async fn record_action(&self, action: &EngagementActionResult) -> Result<()> {
        sqlx::query("INSERT OR REPLACE INTO engagement_actions (id, account_id, executed_at, payload) VALUES (?, ?, ?, ?)")
            .bind(&action.action_id)
            .bind(action.account_id.0.to_string())
            .bind(action.executed_at.to_rfc3339())
            .bind(serde_json::to_string(action)?)
            .execute(self.db.pool())
            .await
            .map_err(db_err)?;
        let action_type = format!("Engagement{:?}", action.action_type);
        let details = serde_json::json!({
            "platform": action.platform,
            "target": action.target_id.chars().take(200).collect::<String>(),
            "remote_id": action.remote_id,
            "error": action.error_message,
        })
        .to_string();
        let account = action.account_id.0.to_string();
        self.db
            .append_audit(openaccounts_database::AuditInsert {
                execution_id: "ENGAGEMENT",
                account_id: Some(&account),
                automation_id: None,
                action_type: &action_type,
                risk_level: "Medium",
                details: &details,
                status: if action.success { "Completed" } else { "Failed" },
            })
            .await?;
        Ok(())
    }

    async fn load_inbox(&self) -> Result<Vec<UnifiedInboxMessage>> {
        let rows = sqlx::query("SELECT payload FROM inbox_messages ORDER BY updated_at DESC LIMIT 2000")
            .fetch_all(self.db.pool())
            .await
            .map_err(db_err)?;
        Ok(rows
            .into_iter()
            .filter_map(|r| serde_json::from_str(&r.get::<String, _>("payload")).ok())
            .collect())
    }

    async fn save_message(&self, message: &UnifiedInboxMessage) -> Result<()> {
        sqlx::query(
            "INSERT INTO inbox_messages (id, account_id, updated_at, payload) VALUES (?, ?, ?, ?) \
             ON CONFLICT(id) DO UPDATE SET updated_at = excluded.updated_at, payload = excluded.payload",
        )
        .bind(&message.message_id)
        .bind(message.account_id.0.to_string())
        .bind(Utc::now().to_rfc3339())
        .bind(serde_json::to_string(message)?)
        .execute(self.db.pool())
        .await
        .map_err(db_err)?;
        Ok(())
    }
}

pub struct SqliteCalendarStore {
    db: Arc<Database>,
}

impl SqliteCalendarStore {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl CalendarStore for SqliteCalendarStore {
    async fn load_slots(&self) -> Result<Vec<CalendarPostSlot>> {
        let rows = sqlx::query("SELECT payload FROM calendar_slots ORDER BY scheduled_time")
            .fetch_all(self.db.pool())
            .await
            .map_err(db_err)?;
        Ok(rows
            .into_iter()
            .filter_map(|r| serde_json::from_str(&r.get::<String, _>("payload")).ok())
            .collect())
    }

    async fn save_slot(&self, slot: &CalendarPostSlot) -> Result<()> {
        sqlx::query(
            "INSERT INTO calendar_slots (id, scheduled_time, payload) VALUES (?, ?, ?) \
             ON CONFLICT(id) DO UPDATE SET scheduled_time = excluded.scheduled_time, payload = excluded.payload",
        )
        .bind(&slot.slot_id)
        .bind(slot.scheduled_time.to_rfc3339())
        .bind(serde_json::to_string(slot)?)
        .execute(self.db.pool())
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn delete_slot(&self, slot_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM calendar_slots WHERE id = ?")
            .bind(slot_id)
            .execute(self.db.pool())
            .await
            .map_err(db_err)?;
        Ok(())
    }
}
