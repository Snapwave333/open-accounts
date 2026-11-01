use chrono::{DateTime, Utc};
use openaccounts_core::{AccountId, OpenAccountsError, Platform, Result};
use openaccounts_database::Database;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::sync::Arc;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountRecord {
    pub id: AccountId,
    pub platform: Platform,
    pub username: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub status: String,
    pub profile_metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_active: Option<DateTime<Utc>>,
}

pub struct AccountRegistry {
    db: Arc<Database>,
}

impl AccountRegistry {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    pub async fn create_account(&self, account: AccountRecord) -> Result<AccountId> {
        let id_str = account.id.0.to_string();
        let platform_str = account.platform.to_string();
        let metadata_str = serde_json::to_string(&account.profile_metadata).unwrap_or_default();
        let created_at_str = account.created_at.to_rfc3339();
        let updated_at_str = account.updated_at.to_rfc3339();

        sqlx::query(
            r#"
            INSERT INTO accounts (id, platform, username, email, phone, status, profile_metadata, created_at, updated_at, last_active)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id_str)
        .bind(&platform_str)
        .bind(&account.username)
        .bind(&account.email)
        .bind(&account.phone)
        .bind(&account.status)
        .bind(&metadata_str)
        .bind(&created_at_str)
        .bind(&updated_at_str)
        .bind(account.last_active.map(|t| t.to_rfc3339()))
        .execute(self.db.pool())
        .await
        .map_err(|e| OpenAccountsError::Database(e.to_string()))?;

        info!("Created account {} for platform {}", account.username, platform_str);
        Ok(account.id)
    }

    pub async fn list_accounts(&self) -> Result<Vec<AccountRecord>> {
        let rows = sqlx::query("SELECT id, platform, username, email, phone, status, profile_metadata, created_at, updated_at, last_active FROM accounts ORDER BY created_at DESC")
            .fetch_all(self.db.pool())
            .await
            .map_err(|e| OpenAccountsError::Database(e.to_string()))?;

        let mut list = Vec::new();
        for r in rows {
            let id_str: String = r.get("id");
            let id = AccountId(uuid::Uuid::parse_str(&id_str).unwrap_or_default());
            let platform_str: String = r.get("platform");
            let username: String = r.get("username");
            let email: Option<String> = r.get("email");
            let phone: Option<String> = r.get("phone");
            let status: String = r.get("status");
            let meta_str: String = r.get("profile_metadata");
            let created_str: String = r.get("created_at");
            let updated_str: String = r.get("updated_at");
            let last_active_str: Option<String> = r.get("last_active");

            let platform = match platform_str.as_str() {
                "instagram" => Platform::Instagram,
                "reddit" => Platform::Reddit,
                "youtube" => Platform::YouTube,
                "discord" => Platform::Discord,
                "x" => Platform::X,
                "email" => Platform::Email,
                other => Platform::Custom(other.into()),
            };

            list.push(AccountRecord {
                id,
                platform,
                username,
                email,
                phone,
                status,
                profile_metadata: serde_json::from_str(&meta_str).unwrap_or_default(),
                created_at: DateTime::parse_from_rfc3339(&created_str).map(|d| d.with_timezone(&Utc)).unwrap_or_else(|_| Utc::now()),
                updated_at: DateTime::parse_from_rfc3339(&updated_str).map(|d| d.with_timezone(&Utc)).unwrap_or_else(|_| Utc::now()),
                last_active: last_active_str
                    .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                    .map(|d| d.with_timezone(&Utc)),
            });
        }

        Ok(list)
    }

    pub async fn get_account(&self, id: &AccountId) -> Result<Option<AccountRecord>> {
        Ok(self.list_accounts().await?.into_iter().find(|a| &a.id == id))
    }

    pub async fn count_for_platform(&self, platform: &str) -> Result<u32> {
        let row = sqlx::query("SELECT COUNT(*) AS n FROM accounts WHERE platform = ?")
            .bind(platform.to_lowercase())
            .fetch_one(self.db.pool())
            .await
            .map_err(|e| OpenAccountsError::Database(e.to_string()))?;
        Ok(row.get::<i64, _>("n").max(0) as u32)
    }

    pub async fn update_status(&self, id: &AccountId, status: &str) -> Result<()> {
        let res = sqlx::query("UPDATE accounts SET status = ?, updated_at = ? WHERE id = ?")
            .bind(status)
            .bind(Utc::now().to_rfc3339())
            .bind(id.0.to_string())
            .execute(self.db.pool())
            .await
            .map_err(|e| OpenAccountsError::Database(e.to_string()))?;
        if res.rows_affected() == 0 {
            return Err(OpenAccountsError::AccountNotFound(id.to_string()));
        }
        Ok(())
    }

    pub async fn update_metadata(&self, id: &AccountId, metadata: &serde_json::Value) -> Result<()> {
        let res = sqlx::query("UPDATE accounts SET profile_metadata = ?, updated_at = ? WHERE id = ?")
            .bind(serde_json::to_string(metadata)?)
            .bind(Utc::now().to_rfc3339())
            .bind(id.0.to_string())
            .execute(self.db.pool())
            .await
            .map_err(|e| OpenAccountsError::Database(e.to_string()))?;
        if res.rows_affected() == 0 {
            return Err(OpenAccountsError::AccountNotFound(id.to_string()));
        }
        Ok(())
    }

    pub async fn touch(&self, id: &AccountId) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query("UPDATE accounts SET last_active = ?, updated_at = ? WHERE id = ?")
            .bind(&now)
            .bind(&now)
            .bind(id.0.to_string())
            .execute(self.db.pool())
            .await
            .map_err(|e| OpenAccountsError::Database(e.to_string()))?;
        Ok(())
    }

    pub async fn delete_account(&self, id: &AccountId) -> Result<bool> {
        let res = sqlx::query("DELETE FROM accounts WHERE id = ?")
            .bind(id.0.to_string())
            .execute(self.db.pool())
            .await
            .map_err(|e| OpenAccountsError::Database(e.to_string()))?;
        Ok(res.rows_affected() > 0)
    }
}
