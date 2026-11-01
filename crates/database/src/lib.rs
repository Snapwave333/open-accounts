use chrono::Utc;
use openaccounts_core::{OpenAccountsError, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    Pool, Row, Sqlite,
};
use std::{path::Path, str::FromStr, time::Duration};
use tokio::sync::Mutex;
use tracing::info;
use uuid::Uuid;

pub type DbPool = Pool<Sqlite>;

pub const GENESIS_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS accounts (
    id TEXT PRIMARY KEY,
    platform TEXT NOT NULL,
    username TEXT NOT NULL,
    email TEXT,
    phone TEXT,
    status TEXT NOT NULL,
    profile_metadata TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    last_active TEXT
);

CREATE TABLE IF NOT EXISTS automations (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL,
    name TEXT NOT NULL,
    platform TEXT NOT NULL,
    module TEXT NOT NULL,
    configuration TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (account_id) REFERENCES accounts(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS audit_logs (
    id TEXT PRIMARY KEY,
    execution_id TEXT NOT NULL,
    account_id TEXT,
    automation_id TEXT,
    action_type TEXT NOT NULL,
    risk_level TEXT NOT NULL,
    details TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS human_tasks (
    id TEXT PRIMARY KEY,
    account_id TEXT,
    automation_id TEXT,
    task_type TEXT NOT NULL,
    reason TEXT NOT NULL,
    page_url TEXT,
    screenshot_path TEXT,
    status TEXT NOT NULL,
    created_at TEXT NOT NULL,
    resolved_at TEXT
);

CREATE TABLE IF NOT EXISTS lm_providers (
    id TEXT PRIMARY KEY,
    provider_type TEXT NOT NULL,
    name TEXT NOT NULL,
    endpoint TEXT,
    model_name TEXT NOT NULL,
    api_key_ref TEXT,
    is_active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS vault_meta (
    key TEXT PRIMARY KEY,
    value BLOB NOT NULL
);

CREATE TABLE IF NOT EXISTS vault_secrets (
    id TEXT PRIMARY KEY,
    account_id TEXT,
    secret_type TEXT NOT NULL,
    label TEXT NOT NULL,
    nonce BLOB NOT NULL,
    ciphertext BLOB NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS policy_snapshots (
    platform_id TEXT NOT NULL,
    url TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    http_status INTEGER NOT NULL,
    checked_at TEXT NOT NULL,
    PRIMARY KEY (platform_id, url)
);

CREATE TABLE IF NOT EXISTS app_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_accounts_platform ON accounts(platform);
CREATE INDEX IF NOT EXISTS idx_audit_logs_created ON audit_logs(created_at);
CREATE INDEX IF NOT EXISTS idx_audit_logs_account ON audit_logs(account_id);
CREATE INDEX IF NOT EXISTS idx_human_tasks_status ON human_tasks(status);
CREATE INDEX IF NOT EXISTS idx_vault_secrets_account ON vault_secrets(account_id);
"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditInsert<'a> {
    pub execution_id: &'a str,
    pub account_id: Option<&'a str>,
    pub automation_id: Option<&'a str>,
    pub action_type: &'a str,
    pub risk_level: &'a str,
    pub details: &'a str,
    pub status: &'a str,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditRow {
    pub seq: i64,
    pub id: String,
    pub execution_id: String,
    pub account_id: Option<String>,
    pub automation_id: Option<String>,
    pub action_type: String,
    pub risk_level: String,
    pub details: String,
    pub status: String,
    pub created_at: String,
    pub prev_hash: String,
    pub entry_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChainVerification {
    pub entries_checked: u64,
    pub is_intact: bool,
    pub first_broken_id: Option<String>,
}

#[allow(clippy::too_many_arguments)]
pub fn audit_entry_hash(
    prev_hash: &str,
    id: &str,
    execution_id: &str,
    account_id: Option<&str>,
    automation_id: Option<&str>,
    action_type: &str,
    risk_level: &str,
    details: &str,
    status: &str,
    created_at: &str,
) -> String {
    let mut hasher = Sha256::new();
    for part in [
        prev_hash,
        id,
        execution_id,
        account_id.unwrap_or(""),
        automation_id.unwrap_or(""),
        action_type,
        risk_level,
        details,
        status,
        created_at,
    ] {
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part.as_bytes());
    }
    hex(&hasher.finalize())
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn db_err(context: &str) -> impl Fn(sqlx::Error) -> OpenAccountsError + '_ {
    move |e| OpenAccountsError::Database(format!("{context}: {e}"))
}

pub struct Database {
    pool: DbPool,
    audit_lock: Mutex<()>,
}

impl Database {
    pub async fn init(database_path: &Path) -> Result<Self> {
        if let Some(parent) = database_path.parent().filter(|p| !p.as_os_str().is_empty()) {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| OpenAccountsError::Database(format!("Failed to create db dir: {e}")))?;
        }

        let options = SqliteConnectOptions::new()
            .filename(database_path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(5));

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await
            .map_err(db_err("Failed to connect to SQLite"))?;

        let db = Self { pool, audit_lock: Mutex::new(()) };
        db.run_migrations().await?;
        info!("Database initialized at {:?}", database_path);
        Ok(db)
    }

    pub async fn in_memory() -> Result<Self> {
        let options = SqliteConnectOptions::from_str("sqlite::memory:")
            .map_err(db_err("Invalid in-memory options"))?
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .idle_timeout(None)
            .max_lifetime(None)
            .connect_with(options)
            .await
            .map_err(db_err("Failed to open in-memory SQLite"))?;
        let db = Self { pool, audit_lock: Mutex::new(()) };
        db.run_migrations().await?;
        Ok(db)
    }

    pub fn pool(&self) -> &DbPool {
        &self.pool
    }

    async fn column_exists(&self, table: &str, column: &str) -> Result<bool> {
        let rows = sqlx::query(&format!("PRAGMA table_info({table})"))
            .fetch_all(&self.pool)
            .await
            .map_err(db_err("Schema inspection failed"))?;
        Ok(rows.iter().any(|r| r.try_get::<String, _>("name").map(|n| n == column).unwrap_or(false)))
    }

    pub async fn run_migrations(&self) -> Result<()> {
        sqlx::raw_sql(SCHEMA)
            .execute(&self.pool)
            .await
            .map_err(db_err("Migration failed"))?;

        for (table, column, ddl) in [
            ("audit_logs", "seq", "ALTER TABLE audit_logs ADD COLUMN seq INTEGER"),
            ("audit_logs", "prev_hash", "ALTER TABLE audit_logs ADD COLUMN prev_hash TEXT"),
            ("audit_logs", "entry_hash", "ALTER TABLE audit_logs ADD COLUMN entry_hash TEXT"),
            ("human_tasks", "title", "ALTER TABLE human_tasks ADD COLUMN title TEXT"),
        ] {
            if !self.column_exists(table, column).await? {
                sqlx::query(ddl)
                    .execute(&self.pool)
                    .await
                    .map_err(db_err("Audit chain migration failed"))?;
            }
        }
        sqlx::query("CREATE UNIQUE INDEX IF NOT EXISTS idx_audit_logs_seq ON audit_logs(seq)")
            .execute(&self.pool)
            .await
            .map_err(db_err("Audit index migration failed"))?;

        self.backfill_audit_chain().await
    }

    async fn backfill_audit_chain(&self) -> Result<()> {
        let _guard = self.audit_lock.lock().await;
        let unchained = sqlx::query(
            "SELECT id, execution_id, account_id, automation_id, action_type, risk_level, details, status, created_at \
             FROM audit_logs WHERE entry_hash IS NULL ORDER BY created_at, rowid",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db_err("Audit backfill read failed"))?;
        if unchained.is_empty() {
            return Ok(());
        }

        let mut tx = self.pool.begin().await.map_err(db_err("Audit backfill tx failed"))?;
        let tip = sqlx::query("SELECT seq, entry_hash FROM audit_logs WHERE entry_hash IS NOT NULL ORDER BY seq DESC LIMIT 1")
            .fetch_optional(&mut *tx)
            .await
            .map_err(db_err("Audit tip read failed"))?;
        let (mut seq, mut prev) = match tip {
            Some(r) => (r.try_get::<i64, _>("seq").unwrap_or(0), r.try_get::<String, _>("entry_hash").unwrap_or_default()),
            None => (0, GENESIS_HASH.to_string()),
        };

        for row in unchained {
            let id: String = row.get("id");
            let account: Option<String> = row.get("account_id");
            let automation: Option<String> = row.get("automation_id");
            let hash = audit_entry_hash(
                &prev,
                &id,
                &row.get::<String, _>("execution_id"),
                account.as_deref(),
                automation.as_deref(),
                &row.get::<String, _>("action_type"),
                &row.get::<String, _>("risk_level"),
                &row.get::<String, _>("details"),
                &row.get::<String, _>("status"),
                &row.get::<String, _>("created_at"),
            );
            seq += 1;
            sqlx::query("UPDATE audit_logs SET seq = ?, prev_hash = ?, entry_hash = ? WHERE id = ?")
                .bind(seq)
                .bind(&prev)
                .bind(&hash)
                .bind(&id)
                .execute(&mut *tx)
                .await
                .map_err(db_err("Audit backfill write failed"))?;
            prev = hash;
        }
        tx.commit().await.map_err(db_err("Audit backfill commit failed"))?;
        Ok(())
    }

    pub async fn append_audit(&self, entry: AuditInsert<'_>) -> Result<String> {
        let _guard = self.audit_lock.lock().await;
        let id = Uuid::new_v4().to_string();
        let created_at = Utc::now().to_rfc3339();

        let mut tx = self.pool.begin().await.map_err(db_err("Audit tx failed"))?;
        let tip = sqlx::query("SELECT seq, entry_hash FROM audit_logs WHERE entry_hash IS NOT NULL ORDER BY seq DESC LIMIT 1")
            .fetch_optional(&mut *tx)
            .await
            .map_err(db_err("Audit tip read failed"))?;
        let (seq, prev) = match tip {
            Some(r) => (r.try_get::<i64, _>("seq").unwrap_or(0) + 1, r.try_get::<String, _>("entry_hash").unwrap_or_default()),
            None => (1, GENESIS_HASH.to_string()),
        };
        let hash = audit_entry_hash(
            &prev,
            &id,
            entry.execution_id,
            entry.account_id,
            entry.automation_id,
            entry.action_type,
            entry.risk_level,
            entry.details,
            entry.status,
            &created_at,
        );

        sqlx::query(
            "INSERT INTO audit_logs (id, execution_id, account_id, automation_id, action_type, risk_level, details, status, created_at, seq, prev_hash, entry_hash) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(entry.execution_id)
        .bind(entry.account_id)
        .bind(entry.automation_id)
        .bind(entry.action_type)
        .bind(entry.risk_level)
        .bind(entry.details)
        .bind(entry.status)
        .bind(&created_at)
        .bind(seq)
        .bind(&prev)
        .bind(&hash)
        .execute(&mut *tx)
        .await
        .map_err(db_err("Audit insert failed"))?;
        tx.commit().await.map_err(db_err("Audit commit failed"))?;
        Ok(id)
    }

    pub async fn recent_audit(&self, limit: u32) -> Result<Vec<AuditRow>> {
        let rows = sqlx::query(
            "SELECT seq, id, execution_id, account_id, automation_id, action_type, risk_level, details, status, created_at, prev_hash, entry_hash \
             FROM audit_logs WHERE entry_hash IS NOT NULL ORDER BY seq DESC LIMIT ?",
        )
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(db_err("Audit read failed"))?;
        Ok(rows.into_iter().map(row_to_audit).collect())
    }

    pub async fn verify_audit_chain(&self) -> Result<ChainVerification> {
        let rows = sqlx::query(
            "SELECT seq, id, execution_id, account_id, automation_id, action_type, risk_level, details, status, created_at, prev_hash, entry_hash \
             FROM audit_logs ORDER BY seq",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db_err("Audit read failed"))?;

        let mut prev = GENESIS_HASH.to_string();
        let mut checked = 0u64;
        for row in rows {
            let r = row_to_audit(row);
            let expected = audit_entry_hash(
                &prev,
                &r.id,
                &r.execution_id,
                r.account_id.as_deref(),
                r.automation_id.as_deref(),
                &r.action_type,
                &r.risk_level,
                &r.details,
                &r.status,
                &r.created_at,
            );
            checked += 1;
            if r.prev_hash != prev || r.entry_hash != expected {
                return Ok(ChainVerification { entries_checked: checked, is_intact: false, first_broken_id: Some(r.id) });
            }
            prev = r.entry_hash;
        }
        Ok(ChainVerification { entries_checked: checked, is_intact: true, first_broken_id: None })
    }

    pub async fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let row = sqlx::query("SELECT value FROM app_settings WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .map_err(db_err("Settings read failed"))?;
        Ok(row.map(|r| r.get::<String, _>("value")))
    }

    pub async fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO app_settings (key, value, updated_at) VALUES (?, ?, ?) \
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        )
        .bind(key)
        .bind(value)
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(db_err("Settings write failed"))?;
        Ok(())
    }
}

fn row_to_audit(row: sqlx::sqlite::SqliteRow) -> AuditRow {
    AuditRow {
        seq: row.try_get("seq").unwrap_or(0),
        id: row.get("id"),
        execution_id: row.get("execution_id"),
        account_id: row.get("account_id"),
        automation_id: row.get("automation_id"),
        action_type: row.get("action_type"),
        risk_level: row.get("risk_level"),
        details: row.get("details"),
        status: row.get("status"),
        created_at: row.get("created_at"),
        prev_hash: row.try_get::<Option<String>, _>("prev_hash").ok().flatten().unwrap_or_default(),
        entry_hash: row.try_get::<Option<String>, _>("entry_hash").ok().flatten().unwrap_or_default(),
    }
}
